// The first-user review: a fresh AI that has never seen Faber uses it like a newcomer and says, in plain pt-BR, what
// confused it. It sees only the screen: a screenshot, the visible text and the list of controls a screen reader would
// name. It never sees the code: the AI runs with no tools at all (Claude) or with its shell and apps switched off
// (Codex), in an empty folder outside the repository.
//
//   node scripts/first-user.mjs                    (Haiku, 720x480, pt-BR, first run with no AI connected)
//   node scripts/first-user.mjs --agent codex      (Codex gpt-6-luna instead)
//   node scripts/first-user.mjs --size 1280x800 --steps 10 --no-build
//
// Faber runs as the hidden, sandboxed test build that `testkit/examples/ux_audit.rs` drives (C006: the window lives on a
// desktop no monitor shows), with no CLI connected, so no subscription is spent inside it; the reviewer's own calls are
// its only cost. Builds run at idle priority. Findings go to `docs/research/first-user-<date>-<suffix>.md`, screenshots
// to `<target>/faber-test/first-user/<date>-<suffix>/`. Each run keeps its own evidence. Nothing is fixed by the review.
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir, setPriority, constants } from 'node:os';
import { basename, delimiter, dirname, join, relative, resolve, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const EXE = process.platform === 'win32' ? '.exe' : '';
const TARGET = process.env.CARGO_TARGET_DIR ? resolve(ROOT, process.env.CARGO_TARGET_DIR) : join(ROOT, 'target');

/** Every review keeps its own evidence, even when the same configuration runs twice in one day. */
export function runPaths(root, target, date, output) {
  const parent = join(target, 'faber-test', 'first-user');
  mkdirSync(parent, { recursive: true });
  const shots = mkdtempSync(join(parent, `${date}-`));
  return { shots, out: resolve(root, output ?? `docs/research/first-user-${basename(shots)}.md`) };
}

/** What a newcomer is asked to do, in order, in the same app: each task starts where the last one ended. */
export const TASKS = [
  {
    id: 'primeira-abertura',
    goal:
      'Você acabou de instalar o Faber e o abriu pela primeira vez. Você ainda não conectou nenhuma IA. ' +
      'Passe pela primeira abertura até chegar à tela principal do app.',
  },
  { id: 'conversa', goal: 'Comece uma conversa nova e mande uma pergunta simples, por exemplo "o que você sabe fazer?".' },
  { id: 'projetos', goal: 'Abra a área de Projetos e descubra para que ela serve.' },
  { id: 'configuracoes', goal: 'Encontre as Configurações do app e descubra onde se muda o idioma.' },
];

/** Runs in the page: the visible text, and every control a person can see and reach, numbered, with its centre. */
export const OBSERVE = `(() => {
  const vw = innerWidth, vh = innerHeight;
  const sel = 'button, a[href], input, textarea, select, summary, [role=button], [role=link], [role=tab], ' +
    '[role=menuitem], [role=option], [role=checkbox], [role=switch], [role=radio], [contenteditable=true]';
  const named = (el) => {
    const by = el.getAttribute('aria-labelledby');
    const label = by && by.split(' ').map((id) => document.getElementById(id)?.innerText || '').join(' ').trim();
    return (el.getAttribute('aria-label') || label || el.innerText || el.title || el.placeholder || el.value || '')
      .replace(/\\s+/g, ' ').trim().slice(0, 80);
  };
  const controls = [];
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2 || r.right < 0 || r.bottom < 0 || r.left > vw || r.top > vh) continue;
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || Number(style.opacity) === 0) continue;
    const x = Math.round(Math.min(Math.max(r.left + r.width / 2, 1), vw - 1));
    const y = Math.round(Math.min(Math.max(r.top + r.height / 2, 1), vh - 1));
    const top = document.elementFromPoint(x, y);
    if (top && !el.contains(top) && !top.contains(el)) continue;
    const role = el.getAttribute('role') || ({ A: 'link', INPUT: 'input', TEXTAREA: 'input', SELECT: 'list' }[el.tagName]) ||
      (el.isContentEditable ? 'input' : 'button');
    controls.push({ role, name: named(el), x, y, disabled: !!el.disabled || el.getAttribute('aria-disabled') === 'true' });
    if (controls.length >= 80) break;
  }
  return { text: (document.body.innerText || '').replace(/\\n{3,}/g, '\\n\\n').slice(0, 4000), controls };
})()`;

/** Is `path` the repository or inside it? The reviewer's folder must not be. */
export function insideRepo(path, root = ROOT) {
  const rel = relative(resolve(root), resolve(path));
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

/** The prompt for one step: who the reviewer is, the task, what it did so far, and the screen as words. */
export function stepPrompt(task, history, seen, size) {
  const controls = seen.controls
    .map((c, i) => `[${i + 1}] ${c.role}: ${c.name || '(sem nome)'}${c.disabled ? ' (desativado)' : ''}`)
    .join('\n');
  const done = history.length
    ? history.map((h, i) => `${i + 1}. ${h.thought} -> ${describe(h.action)}`).join('\n')
    : '(nada ainda)';
  return [
    'Você é uma pessoa que nunca usou o Faber, um app de computador para conversar com IAs. Você não é programador.',
    'Você só vê a tela: a imagem anexada, o texto visível e a lista de controles abaixo. Não tente abrir arquivos nem código.',
    `A janela tem ${size.width}x${size.height} pixels e o app está em português.`,
    '',
    `Tarefa: ${task.goal}`,
    '',
    'O que você já fez nesta tarefa:',
    done,
    '',
    'Texto visível na tela:',
    seen.text || '(nenhum)',
    '',
    'Controles que dá para usar (número, tipo, nome):',
    controls || '(nenhum)',
    '',
    'Escolha UMA próxima ação. Responda só com um JSON, sem nada antes ou depois:',
    '{"thought": "o que você pensa ao ver a tela, em uma frase", "confusion": "o que te confundiu aqui, ou null",',
    ' "action": {"type": "click", "n": 3} | {"type": "type", "text": "..."} | {"type": "key", "key": "Enter"} |',
    '           {"type": "scroll", "dy": 400} | {"type": "done", "summary": "como conseguiu"} |',
    '           {"type": "stuck", "where": "onde e por que travou"}}',
    'Para escrever num campo, clique nele primeiro. Diga "done" só quando a tarefa estiver feita de verdade.',
  ].join('\n');
}

/** The prompt that turns the whole walk into the short list of findings. */
export function summaryPrompt(results) {
  const walk = results
    .map((r) => {
      const steps = r.steps
        .map((s, i) => `  ${i + 1}. ${s.thought} -> ${describe(s.action)}${s.confusion ? ` | confuso: ${s.confusion}` : ''} (${s.shot})`)
        .join('\n');
      return `Tarefa "${r.task.goal}" — resultado: ${r.outcome}\n${steps}`;
    })
    .join('\n\n');
  return [
    'Você acabou de usar o Faber pela primeira vez, como um novato. Este é o registro do que você fez:',
    '',
    walk,
    '',
    'Escreva a lista curta do que confundiu ou travou um novato, em português simples, sem termos técnicos.',
    'Responda só com um JSON: {"findings": [{"task": "id da tarefa", "tried": "o que tentou", "stuck": "onde travou ou',
    ' o que confundiu", "shot": "caminho da imagem do passo"}]}. No máximo 8 itens, os mais graves primeiro.',
    `Ids das tarefas: ${results.map((r) => r.task.id).join(', ')}.`,
  ].join('\n');
}

/** The first JSON object in an answer, or null. Models wrap it in prose or a code fence now and then. */
export function firstJson(text) {
  const start = text.indexOf('{');
  if (start < 0) return null;
  for (let end = text.lastIndexOf('}'); end > start; end = text.lastIndexOf('}', end - 1)) {
    try {
      return JSON.parse(text.slice(start, end + 1));
    } catch {
      // A shorter slice may parse.
    }
  }
  return null;
}

const TYPES = new Set(['click', 'type', 'key', 'scroll', 'done', 'stuck']);

/** The step an answer asks for, checked against the controls on screen; an answer that is not one ends the task. */
export function parseStep(text, controls) {
  const json = firstJson(text);
  const action = json?.action;
  if (!action || !TYPES.has(action.type)) return { thought: 'resposta sem ação válida', confusion: null, action: { type: 'stuck', where: `resposta inválida: ${text.slice(0, 200)}` } };
  if (action.type === 'click' && !(Number.isInteger(action.n) && action.n >= 1 && action.n <= controls.length)) {
    return { thought: json.thought || '', confusion: json.confusion || null, action: { type: 'stuck', where: `clicou num controle que não existe (${action.n})` } };
  }
  return { thought: String(json.thought || ''), confusion: json.confusion ? String(json.confusion) : null, action };
}

/** One line for an action, in the words of the report. */
export function describe(action) {
  switch (action.type) {
    case 'click':
      return `clicou em [${action.n}]${action.name ? ` "${action.name}"` : ''}`;
    case 'type':
      return `escreveu "${action.text}"`;
    case 'key':
      return `apertou ${action.key}`;
    case 'scroll':
      return `rolou ${action.dy}`;
    case 'done':
      return `conseguiu: ${action.summary || ''}`;
    default:
      return `travou: ${action.where || ''}`;
  }
}

/** The findings straight from the walk, when the summary answer is not usable: every confusion and every stuck task. */
export function fallback(results) {
  return results.flatMap((r) =>
    r.steps
      .filter((s) => s.confusion || s.action.type === 'stuck')
      .map((s) => ({ task: r.task.id, tried: describe(s.action), stuck: s.confusion || s.action.where || '', shot: s.shot })),
  );
}

/** The report, in plain pt-BR. */
export function report({ date, agent, size, lang, findings, results, shots }) {
  const outcome = { done: 'conseguiu', stuck: 'travou', steps: 'não terminou no limite de passos' };
  const lines = [
    `# Primeiro usuário, ${date}`,
    '',
    `Uma IA que nunca viu o Faber (${agent}) usou o app como um novato, vendo só a tela: a imagem, o texto visível e os`,
    `nomes dos controles, nunca o código. Janela de ${size.width}x${size.height}, idioma ${lang}, primeira abertura sem nenhuma IA`,
    'conectada. Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.',
    '',
    `As imagens ficam em \`${shots}\` (só neste computador).`,
    '',
    '## O que confundiu',
    '',
  ];
  if (!findings.length) lines.push('Nada: o novato não relatou confusão.');
  const end = (t) => String(t || '').trim().replace(/[.\s]+$/, '');
  for (const f of findings) lines.push(`- **${f.task}**: ${end(f.tried)}. ${end(f.stuck)}. (\`${f.shot}\`)`);
  lines.push('', '## Tarefa por tarefa', '');
  for (const r of results) {
    lines.push(`### ${r.task.id}: ${outcome[r.outcome]}`, '', r.task.goal, '');
    for (const [i, s] of r.steps.entries()) {
      lines.push(`${i + 1}. ${s.thought} → ${describe(s.action)}${s.confusion ? ` (confuso: ${s.confusion})` : ''} \`${s.shot}\``);
    }
    lines.push('');
  }
  return lines.join('\n');
}

// Everything below runs the review; the functions above are pure and tested (`scripts/first-user.test.mjs`).

function option(args, name, otherwise) {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : otherwise;
}

/** A program on the PATH, as its real executable: npm's `.cmd` shims need a shell, and nothing here starts one. */
function program(name) {
  for (const dir of (process.env.PATH || '').split(delimiter)) {
    const exe = join(dir, name + EXE);
    if (existsSync(exe)) return exe;
    const shimmed = join(dir, 'node_modules/@anthropic-ai/claude-code/bin', name + EXE);
    if (existsSync(shimmed)) return shimmed;
  }
  throw new Error(`${name} is not on the PATH`);
}

/** Runs a program with no window and no shell, feeding it `input`; resolves with its output. */
function run(exe, args, { cwd, input, timeout = 300_000 }) {
  return new Promise((done, fail) => {
    const child = spawn(exe, args, { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    let err = '';
    const timer = setTimeout(() => child.kill(), timeout);
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', fail);
    child.on('close', (code) => {
      clearTimeout(timer);
      code === 0 ? done(out) : fail(new Error(`${exe} exited ${code}: ${err.slice(-500)}`));
    });
    child.stdin.end(input);
  });
}

/** The reviewer: a fresh, tool-less model call per step, in an empty folder outside the repository. */
function reviewer(kind, home) {
  if (kind === 'claude') {
    const exe = program('claude');
    const args = ['-p', '--model', 'haiku', '--tools', '', '--strict-mcp-config', '--disable-slash-commands',
      '--no-session-persistence', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose'];
    return async (prompt, image) => {
      const content = [{ type: 'text', text: prompt }];
      if (image) content.unshift({ type: 'image', source: { type: 'base64', media_type: 'image/png', data: readFileSync(image).toString('base64') } });
      const out = await run(exe, args, { cwd: home, input: JSON.stringify({ type: 'user', message: { role: 'user', content } }) + '\n' });
      const result = out.split('\n').map((l) => firstJson(l)).find((m) => m?.type === 'result');
      if (!result || result.is_error) throw new Error(`no answer from the reviewer: ${out.slice(-300)}`);
      return result.result;
    };
  }
  const exe = program('codex');
  const off = ['shell_tool', 'unified_exec', 'apps', 'browser_use', 'browser_use_external', 'computer_use'].flatMap((f) => ['--disable', f]);
  return async (prompt, image) => {
    const last = join(home, 'last.txt');
    rmSync(last, { force: true });
    // --ignore-user-config leaves out the person's MCP servers too, which could otherwise read files.
    const args = ['exec', '--ignore-user-config', '-m', 'gpt-6-luna', '-c', 'model_reasoning_effort="low"', ...off, '-s', 'read-only', '-C', home,
      '--skip-git-repo-check', '--ephemeral', '--color', 'never', '-o', last, ...(image ? ['-i', image] : []), '-'];
    await run(exe, args, { cwd: home, input: prompt });
    return readFileSync(last, 'utf8');
  };
}

/** The hidden test build, one JSON operation per line. */
function driver(exe, flags) {
  const child = spawn(exe, flags, { cwd: ROOT, windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'] });
  const lines = createInterface({ input: child.stdout });
  const waiting = [];
  const queued = [];
  let ended = null;
  const closed = new Promise((done) => child.on('close', (code) => done(code)));
  lines.on('line', (line) => (waiting.length ? waiting.shift().done(line) : queued.push(line)));
  closed.then((code) => {
    ended = new Error(`the test build ended (exit ${code})`);
    for (const w of waiting.splice(0)) w.fail(ended);
  });
  // No limit while it builds and opens; an operation that takes over two minutes means the app is stuck.
  const next = (limit) =>
    new Promise((done, fail) => {
      if (queued.length) return done(queued.shift());
      if (ended) return fail(ended);
      const timer = limit && setTimeout(() => fail(new Error('the test build did not answer in time')), limit);
      waiting.push({ done: (line) => (clearTimeout(timer), done(line)), fail });
    });
  return {
    async ready() {
      while ((await next()) !== 'READY');
    },
    async op(op, value, extra = {}) {
      child.stdin.write(JSON.stringify({ op, value, ...extra }) + '\n');
      const answer = JSON.parse(await next(120_000));
      if ('error' in answer) throw new Error(`${op}: ${answer.error}`);
      return answer.ok;
    },
    /** Ends the app; fails unless the sandbox audit and the clean-up passed (the driver prints CLEAN last). */
    async quit() {
      if (ended) throw ended;
      child.stdin.write('{"op":"quit"}\n');
      const code = await closed;
      if (code !== 0 || !queued.includes('CLEAN')) throw new Error(`the test build did not end clean (exit ${code})`);
    },
  };
}

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

async function act(app, action, seen, size) {
  if (action.type === 'click') {
    const { x, y } = seen.controls[action.n - 1];
    for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
      await app.op('call', 'Input.dispatchMouseEvent', { params: { type, x, y, button: 'left', clickCount: 1 } });
    }
  } else if (action.type === 'type') {
    await app.op('call', 'Input.insertText', { params: { text: String(action.text) } });
  } else if (action.type === 'key') {
    await app.op('press', String(action.key));
  } else if (action.type === 'scroll') {
    const params = { type: 'mouseWheel', x: size.width / 2, y: size.height / 2, deltaX: 0, deltaY: Number(action.dy) || 400 };
    await app.op('call', 'Input.dispatchMouseEvent', { params });
  }
  await pause(1200);
}

async function main(args) {
  const agent = option(args, '--agent', 'claude');
  const [width, height] = option(args, '--size', '720x480').split('x').map(Number);
  const size = { width, height };
  const lang = option(args, '--lang', 'pt-BR');
  const maxSteps = Number(option(args, '--steps', '8'));
  const date = new Date().toISOString().slice(0, 10);
  const { shots, out } = runPaths(ROOT, TARGET, date, option(args, '--out'));

  const home = mkdtempSync(join(tmpdir(), 'faber-first-user-'));
  if (insideRepo(home)) throw new Error(`the reviewer's folder ${home} is inside the repository`);
  const ask = reviewer(agent, home);

  // The build of the driver inherits this priority; the test app's own build in `ux_audit --build` sets idle itself.
  try {
    setPriority(constants.priority.PRIORITY_LOW);
  } catch {
    // Not allowed here: build at the normal priority.
  }
  const built = spawnSync('cargo', ['build', '--quiet', '-p', 'faber-testkit', '--example', 'ux_audit'], { cwd: ROOT, stdio: 'inherit', windowsHide: true });
  if (built.status !== 0) throw new Error('the build of testkit/examples/ux_audit.rs failed');
  const flags = ['--first-run', '--no-ai', '--lang', lang, ...(args.includes('--no-build') ? [] : ['--build'])];
  const app = driver(join(TARGET, `debug/examples/ux_audit${EXE}`), flags);
  await app.ready();
  const results = [];
  try {
    await app.op('call', 'Emulation.setDeviceMetricsOverride', { params: { width, height, deviceScaleFactor: 1, mobile: false } });
    await pause(1500);
    for (const task of TASKS) {
      const steps = [];
      let outcome = 'steps';
      for (let n = 1; n <= maxSteps; n++) {
        const shot = join(shots, `${task.id}-${n}.png`);
        await app.op('shot', shot);
        const seen = await app.op('eval', OBSERVE);
        const step = parseStep(await ask(stepPrompt(task, steps, seen, size), shot), seen.controls);
        if (step.action.type === 'click') step.action.name = seen.controls[step.action.n - 1].name;
        steps.push({ ...step, shot: relative(ROOT, shot).replaceAll('\\', '/') });
        console.log(`${task.id} ${n}: ${describe(step.action)}`);
        if (step.action.type === 'done' || step.action.type === 'stuck') {
          outcome = step.action.type;
          break;
        }
        await act(app, step.action, seen, size);
      }
      results.push({ task, steps, outcome });
    }
  } finally {
    await app.quit();
  }
  const summary = firstJson(await ask(summaryPrompt(results)))?.findings;
  const findings = Array.isArray(summary) ? summary : fallback(results);
  const name = agent === 'claude' ? 'Claude Haiku, sem nenhuma ferramenta' : 'Codex gpt-6-luna, sem terminal nem apps';
  writeFileSync(out, report({ date, agent: name, size, lang, findings, results, shots: relative(ROOT, shots).replaceAll('\\', '/') }));
  rmSync(home, { recursive: true, force: true });
  console.log(`findings: ${relative(ROOT, out)}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((e) => {
    console.error(e.message);
    process.exit(1);
  });
}
