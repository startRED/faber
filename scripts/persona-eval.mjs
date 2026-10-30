// A small opt-in evaluation of the words Faber gives both CLIs (task 130; the idea comes from T3 Code's `evals/`). Never
// part of `cargo t` or CI: it runs the real CLIs with the person's own sign-in and spends a little real quota.
//
//   node scripts/persona-eval.mjs [--samples 3] [--cli claude,codex] [--only <case,...>] [--old <file>] [--new <file>]
//                                 [--claude-model sonnet] [--codex-model gpt-6-luna] [--codex-effort medium] [--plan]
//
// Each case is a synthetic request written the way the owner writes to Faber. The weak spots come from the 2026-10-29
// diagnosis (English helper titles on a Portuguese screen, technical words let in by a pasted log) and from the owner's rule that replies follow the person's language, or the language chosen for the
// screen when a message has no words of its own; they should fail with the old words and pass with the new. The others
// are normal use that both must pass. A case with `language` runs as a screen set to that language. Every case runs
// `--samples` times per CLI with the old words (default: the frozen `scripts/persona-eval/persona-v8.json`) and the new
// ones (default: what `engine/examples/persona_text.rs` prints now), against the same model, each in a fresh empty
// folder. The grader is the plain functions below, tested in `scripts/persona-eval.test.mjs`. It prints passes per case
// and side, and what it cost: Claude Code's own cost figure and every CLI's tokens. Every run is kept in
// `target/persona-eval/<time>.jsonl`.
//
// Children start with no shell: npm's `.cmd` shim is resolved to the real program first.
import { spawn, spawnSync } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readsIn } from '../ui/language.ts';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The technical words a reply to the pasted log must leave out unless the person asks for them. */
const LOG_WORDS = ['CI', 'lint', 'clippy', 'branch', 'commit', 'merge', 'pull request', 'PR', '3f9c2a1', 'needless_return', 'fix/login-timeout', 'pipeline'];

/** How many times `text` holds `word` as a whole word (or phrase), in any case. A slash is a boundary: "CI/lint" holds both. */
const count = (text, word) => [...text.matchAll(new RegExp(`(?<![\\p{L}\\p{N}_])${word.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')}(?![\\p{L}\\p{N}_])`, 'giu'))].length;
const has = (text, word) => count(text, word) > 0;

/** Small words only one of the screen's languages uses (none two of them share, such as "de" or "que"). */
const FUNCTION_WORDS = {
  en: 'the is are and of to it you your this that with for what which not can',
  'pt-BR': 'você não é são isso esse essa está uma com em sem os ao na nessa nesse pelo seu sua também mas quando pra ficou ainda então já muito',
  es: 'el es son usted y del los su también pero cuando qué',
  fr: 'le les est sont et vous pas avec pour ce qui mais quand',
  de: 'der die das ist sind und nicht sie mit für ein eine auch aber wenn',
};

/** The language a reply is written in, by its small words, leaving code blocks and inline code out; '' with none. */
export function languageOf(text) {
  const words = text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`[^`]*`/g, ' ')
    .toLowerCase()
    .split(/[^\p{L}]+/u);
  let best = '';
  let most = 0;
  for (const [lang, list] of Object.entries(FUNCTION_WORDS)) {
    const own = new Set(list.split(' '));
    const hits = words.filter((w) => own.has(w)).length;
    if (hits > most) [best, most] = [lang, hits];
  }
  return best;
}

/** Whether a reply is in `lang`. A short English reply ("Both finished: 391 and 5555") may hold none of the small words,
 * so English is also a reply with no mark of another language, as `readsIn` reads a title. */
export function repliesIn(text, lang) {
  const found = languageOf(text);
  return found === lang || (lang === 'en' && !found && readsIn(text.replace(/```[\s\S]*?```|`[^`]*`/g, ' '), 'en'));
}

/** A case graded by the language of the reply. */
const inLanguage = (lang) => (run) => {
  const found = languageOf(run.text);
  return { pass: repliesIn(run.text, lang), why: `replied in ${found || 'no language it could tell'}` };
};

/** The first sentence of a reply, after any heading or emphasis marks. */
export function firstSentence(text) {
  const plain = text.replace(/^[\s#>*_-]+/, '').replace(/\*\*/g, '');
  return plain.split(/(?<=[.!?])\s|\n/)[0] ?? '';
}

/** What a helper title must be: a few words. */
const short = (title) => title.trim().split(/\s+/).length <= 8 && title.length <= 60;

const WAIT_TALK = /\b(?:aguard\w*|esperando|em segundo plano|ainda\s+(?:est[aá]|estou)\s+(?:rodando|trabalhando|executando))\b/i;

// The answer to "can I buy it?" is no: a refusal of the purchase ("Não, ...", "não dá", "será recusada"), not any "não"
// ("Não se preocupe, você consegue"), and no "yes".
const NO = /^n[aã]o\b\s*(?:[.,!;:—–-]|$)|\bn[aã]o\s+(?:\S+\s+)?(?:consegue|d[aá]|passa|cabe|pode|vai\s+(?:passar|dar)|[eé]\s+poss[ií]vel)(?!\p{L})|\brecusad\w*|\bnegad\w*/iu;
const YES = /\bsim\b/i;

/** The cases. `aim` says which ones the old words should fail; `clis` which CLIs can run one in a single turn (Codex's
 * subagents and background shells are not reachable from one `codex exec`). `tools` is Claude Code's tool list. */
export const CASES = [
  {
    id: 'titles',
    aim: 'weak spot',
    clis: ['claude'],
    tools: 'Agent',
    prompt:
      'Preciso de duas contas feitas ao mesmo tempo, cada uma por um subagente separado (use a ferramenta de subagente, um para cada conta): quanto é 17 × 23 e quanto é 1234 + 4321. Depois me diga os dois resultados.',
    grade: (run) => {
      const titles = run.helpers;
      if (!titles.length) return { pass: false, why: 'no helper started' };
      const bad = titles.filter((title) => !readsIn(title, 'pt-BR') || !short(title));
      // The answer around them stays in the person's language too (a turn after helpers drifted into others).
      const reply = languageOf(run.text);
      if (!bad.length && !repliesIn(run.text, 'pt-BR')) return { pass: false, why: `titles fine, reply in ${reply || 'no language it could tell'}` };
      return { pass: !bad.length, why: bad.length ? `titles not short Portuguese: ${bad.join(' | ')}` : `titles: ${titles.join(' | ')}` };
    },
  },
  {
    id: 'pasted-log',
    aim: 'weak spot',
    clis: ['claude', 'codex'],
    tools: '',
    prompt: [
      'Recebi isto por e-mail e não entendi nada:',
      '',
      '```',
      'Run failed: CI / lint (pull_request)',
      'branch: fix/login-timeout   commit 3f9c2a1',
      'error: clippy::needless_return at src/auth.rs:42',
      "merge blocked: required check 'lint' failed",
      '```',
      '',
      'Isso é grave? O que eu faço?',
    ].join('\n'),
    grade: (run) => {
      // One technical word may stay once; any more, or the same one again, fails.
      const used = LOG_WORDS.flatMap((word) => Array(count(run.text, word)).fill(word));
      return { pass: used.length <= 1, why: used.length ? `technical words: ${used.join(', ')}` : 'no technical words' };
    },
  },
  {
    id: 'screen-pt-pasted-only',
    aim: 'weak spot',
    clis: ['claude', 'codex'],
    tools: '',
    language: 'pt-BR',
    prompt: ['```', "Error: ENOSPC: no space left on device, write '/home/app/uploads/report.pdf'", '    at Object.writeSync (node:fs:927:3)', '    at saveUpload (/home/app/server.js:88:6)', '```'].join('\n'),
    grade: inLanguage('pt-BR'),
  },
  {
    id: 'conclusion-first',
    aim: 'shape',
    clis: ['claude', 'codex'],
    tools: '',
    prompt: 'Tenho R$ 3.000 de limite no cartão e quero comprar uma TV de R$ 3.200 à vista no cartão, numa compra só. Consigo? Me explica.',
    grade: (run) => {
      const first = firstSentence(run.text);
      return { pass: NO.test(first) && !YES.test(first), why: `first sentence: ${first.slice(0, 120)}` };
    },
  },
  {
    id: 'asked-for-terms',
    aim: 'control',
    clis: ['claude', 'codex'],
    tools: '',
    prompt: 'No git, qual comando mostra o hash do último commit e qual mostra o nome do branch atual? Quero os nomes técnicos, sem simplificar.',
    grade: (run) => {
      // The command itself is a technical term too: `git rev-parse HEAD` answers for "commit".
      const terms = { git: ['git'], commit: ['commit', 'rev-parse', 'git log'], branch: ['branch', 'show-current', 'symbolic-ref', 'abbrev-ref'] };
      const missing = Object.keys(terms).filter((term) => !terms[term].some((word) => has(run.text, word)));
      return { pass: !missing.length, why: missing.length ? `left out: ${missing.join(', ')}` : 'kept the asked-for terms' };
    },
  },
  {
    id: 'english-titles',
    aim: 'control',
    clis: ['claude'],
    tools: 'Agent',
    prompt: 'I need two sums done at the same time, each by its own subagent (use the subagent tool, one per sum): 17 × 23 and 1234 + 4321. Then tell me both results.',
    grade: (run) => {
      const titles = run.helpers;
      if (!titles.length) return { pass: false, why: 'no helper started' };
      const bad = titles.filter((title) => !readsIn(title, 'en') || !short(title));
      const reply = languageOf(run.text);
      if (!bad.length && !repliesIn(run.text, 'en')) return { pass: false, why: `titles fine, reply in ${reply || 'no language it could tell'}` };
      return { pass: !bad.length, why: bad.length ? `titles not short English: ${bad.join(' | ')}` : `titles: ${titles.join(' | ')}` };
    },
  },
  {
    id: 'screen-en-person-pt',
    aim: 'control',
    clis: ['claude', 'codex'],
    tools: '',
    language: 'en',
    prompt: 'Qual a diferença entre memória RAM e armazenamento? Em poucas linhas, por favor.',
    grade: inLanguage('pt-BR'),
  },
  {
    id: 'screen-pt-person-en',
    aim: 'control',
    clis: ['claude', 'codex'],
    tools: '',
    language: 'pt-BR',
    prompt: 'What is the difference between RAM and storage? Just a few lines, please.',
    grade: inLanguage('en'),
  },
  {
    id: 'quiet',
    aim: 'control',
    clis: ['claude', 'codex'],
    tools: '',
    prompt: 'Qual é a capital da Austrália?',
    grade: (run) => {
      const right = /\bcan?berr?a\b|camberra/i.test(run.text);
      const talk = WAIT_TALK.test(run.text);
      return { pass: right && !talk && run.text.length <= 600, why: `${right ? 'right answer' : 'wrong answer'}${talk ? '; talks about waiting' : ''}; ${run.text.length} characters` };
    },
  },
];

/** What one Claude Code run said, from its `stream-json` lines: the final text, the titles of the helpers it started, and
 * what it cost. */
export function parseClaude(out) {
  const run = { text: '', helpers: [], cost: 0, tokens: 0, error: '' };
  for (const line of out.split('\n')) {
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    if (event.type === 'assistant' && !event.parent_tool_use_id) {
      for (const block of event.message?.content ?? []) {
        if (block.type === 'tool_use' && (block.name === 'Agent' || block.name === 'Task') && typeof block.input?.description === 'string') run.helpers.push(block.input.description);
      }
    }
    if (event.type === 'result') {
      run.text = typeof event.result === 'string' ? event.result : '';
      run.cost = Number(event.total_cost_usd) || 0;
      const u = event.usage ?? {};
      run.tokens = (u.input_tokens ?? 0) + (u.output_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + (u.cache_read_input_tokens ?? 0);
      if (event.is_error) run.error = run.text || event.subtype || 'error';
    }
  }
  return run;
}

/** What one `codex exec --json` run said: its last message and its tokens (a subscription gives no price). */
export function parseCodex(out) {
  const run = { text: '', helpers: [], cost: 0, tokens: 0, error: '' };
  for (const line of out.split('\n')) {
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    if (event.type === 'item.completed' && event.item?.type === 'agent_message') run.text = event.item.text ?? '';
    if (event.type === 'turn.completed') {
      const u = event.usage ?? {};
      run.tokens += (u.input_tokens ?? 0) + (u.output_tokens ?? 0);
    }
    if (event.type === 'turn.failed' || event.type === 'error') run.error = event.error?.message ?? event.message ?? 'error';
  }
  return run;
}

/** The words a case gets: the ones for its screen language when the words have them (the old ones do not). */
export function wordsFor(persona, c) {
  return (c.language && persona.languages?.[c.language]) || persona;
}

/** Why a finished run failed, or '' when it answered: a time-out, an error it reported, a non-zero exit or no answer. */
export function failure({ code, timedOut, err }, parsed) {
  if (timedOut) return 'timed out';
  if (parsed.error) return parsed.error;
  if (code !== 0) return `exit ${code}: ${err.trim().slice(-300)}`;
  return parsed.text ? '' : err.trim().slice(-300) || 'no answer';
}

/** The quoted paths after `%dp0%` or `%~dp0` in an npm `.cmd` shim, relative to its folder. */
export function shimTargets(shim) {
  return [...shim.matchAll(/"%~?dp0%?\\?([^"%]+)"/gi)].map((m) => m[1]);
}

/** Every run the options ask for, in the order they go: each sample of a case runs the old words, then the new. */
export function plan({ samples, clis, only }) {
  const runs = [];
  for (const c of CASES) {
    if (only && !only.includes(c.id)) continue;
    for (const cli of clis) {
      if (!c.clis.includes(cli)) continue;
      for (let sample = 1; sample <= samples; sample++) for (const side of ['old', 'new']) runs.push({ case: c.id, cli, side, sample });
    }
  }
  return runs;
}

/** Passes per case, CLI and side, with what the runs cost, as the lines to print. */
export function summary(results) {
  const rows = new Map();
  let cost = 0;
  const tokens = { claude: 0, codex: 0 };
  for (const r of results) {
    const key = `${r.case} ${r.cli}`;
    const row = rows.get(key) ?? { case: r.case, cli: r.cli, old: [0, 0], new: [0, 0] };
    row[r.side][1]++;
    if (r.pass) row[r.side][0]++;
    rows.set(key, row);
    cost += r.cost ?? 0;
    tokens[r.cli] = (tokens[r.cli] ?? 0) + (r.tokens ?? 0);
  }
  const aim = (id) => CASES.find((c) => c.id === id)?.aim ?? '';
  const lines = ['case                    cli     aim         old   new'];
  for (const row of rows.values()) {
    lines.push(`${row.case.padEnd(23)} ${row.cli.padEnd(7)} ${aim(row.case).padEnd(11)} ${`${row.old[0]}/${row.old[1]}`.padEnd(5)} ${row.new[0]}/${row.new[1]}`);
  }
  lines.push(`cost: Claude Code says US$ ${cost.toFixed(2)} (${tokens.claude} tokens); Codex ${tokens.codex} tokens`);
  return lines;
}

// Everything below runs the CLIs; the functions above are pure and tested.

function option(args, name, otherwise) {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : otherwise;
}

/** A program on the PATH as `[executable, ...arguments]`: npm's `.cmd` shims need a shell, and nothing here starts one, so
 * a shim gives way to what it points to, a native `.exe` or `node script.js` (as `engine/src/cli.rs` does). */
function program(name) {
  const windows = process.platform === 'win32';
  for (const dir of (process.env.PATH || '').split(delimiter).filter(Boolean)) {
    const exe = join(dir, name + (windows ? '.exe' : ''));
    if (existsSync(exe)) return [exe];
    const shim = join(dir, `${name}.cmd`);
    if (!windows || !existsSync(shim)) continue;
    const targets = shimTargets(readFileSync(shim, 'utf8')).map((t) => join(dir, t));
    const native = targets.findLast((t) => /\.exe$/i.test(t) && !/node\.exe$/i.test(t) && existsSync(t));
    if (native) return [native];
    const script = targets.findLast((t) => /\.[cm]?js$/i.test(t) && existsSync(t));
    if (script) return [process.execPath, script];
  }
  throw new Error(`${name} is not on the PATH`);
}

/** Runs a program with no window and no shell, feeding it `input`; resolves with its output, its exit code and whether it
 * ran out of time. */
function run([exe, ...pre], args, { cwd, input, timeout = 600_000 }) {
  return new Promise((done) => {
    const child = spawn(exe, [...pre, ...args], { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    let err = '';
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      // A launcher (npm's `codex.js`) runs the real program as its own child, which a plain kill leaves running.
      if (process.platform === 'win32') spawn(join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'taskkill.exe'), ['/pid', String(child.pid), '/t', '/f'], { windowsHide: true, stdio: 'ignore' });
      else child.kill();
      // Its output may stay open a little longer; the run is over either way.
      setTimeout(() => done({ out, err, code: null, timedOut }), 5_000).unref();
    }, timeout);
    // As text, so a letter split between two chunks ("ã" is two bytes) stays whole.
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', (e) => {
      clearTimeout(timer);
      done({ out, err: String(e), code: -1, timedOut });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      done({ out, err, code, timedOut });
    });
    child.stdin.end(input);
  });
}

/** The words each CLI gets, `{version, claude, codex, languages?}`: from a file, or what the engine builds now. */
function words(file) {
  if (file) return JSON.parse(readFileSync(file, 'utf8'));
  const built = spawnSync(process.execPath, ['scripts/cargo.mjs', 'run', '-q', '-p', 'faber-engine', '--example', 'persona_text'], {
    cwd: ROOT,
    encoding: 'utf8',
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'inherit'],
    maxBuffer: 16 * 1024 * 1024,
  });
  const line = built.stdout.split('\n').findLast((l) => l.startsWith('{'));
  if (built.status !== 0 || !line) throw new Error('could not print the current words (engine/examples/persona_text.rs)');
  return JSON.parse(line);
}

async function runCase(c, cli, words, models) {
  const persona = wordsFor(words, c);
  const home = mkdtempSync(join(tmpdir(), 'faber-persona-eval-'));
  try {
    if (cli === 'claude') {
      const args = ['-p', '--model', models.claude, '--tools', c.tools, '--strict-mcp-config', '--safe-mode', '--setting-sources', '',
        '--settings', JSON.stringify({ disableAllHooks: true }), '--permission-mode', 'bypassPermissions', '--no-session-persistence',
        '--output-format', 'stream-json', '--verbose', '--append-system-prompt', persona.claude];
      const done = await run(program('claude'), args, { cwd: home, input: c.prompt });
      const parsed = parseClaude(done.out);
      return { ...parsed, error: failure(done, parsed) };
    }
    const off = ['shell_tool', 'unified_exec', 'apps', 'browser_use', 'browser_use_external', 'computer_use', 'multi_agent'].flatMap((f) => ['--disable', f]);
    const args = ['exec', '--ignore-user-config', '-m', models.codex, '-c', `model_reasoning_effort="${models.effort}"`,
      '-c', `developer_instructions=${JSON.stringify(persona.codex)}`, ...off, '-s', 'read-only', '-C', home, '--skip-git-repo-check',
      '--ephemeral', '--color', 'never', '--json', '-'];
    const done = await run(program('codex'), args, { cwd: home, input: c.prompt });
    const parsed = parseCodex(done.out);
    return { ...parsed, error: failure(done, parsed) };
  } finally {
    // A program the agent left running may still hold the folder; the system's temporary folder clears it later.
    try {
      rmSync(home, { recursive: true, force: true, maxRetries: 3 });
    } catch {}
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'persona-eval.mjs'))) {
  const args = process.argv.slice(2);
  const samples = Number(option(args, '--samples', '3'));
  const clis = option(args, '--cli', 'claude,codex').split(',');
  const only = args.includes('--only') ? option(args, '--only', '').split(',') : undefined;
  const runs = plan({ samples, clis, only });
  if (args.includes('--plan')) {
    for (const r of runs) console.log(`${r.case} ${r.cli} ${r.side} #${r.sample}`);
    console.log(`${runs.length} runs`);
  } else {
    const models = { claude: option(args, '--claude-model', 'sonnet'), codex: option(args, '--codex-model', 'gpt-6-luna'), effort: option(args, '--codex-effort', 'medium') };
    const personas = { old: words(option(args, '--old', join(ROOT, 'scripts/persona-eval/persona-v8.json'))), new: words(option(args, '--new', '')) };
    const dir = join(ROOT, 'target', 'persona-eval');
    mkdirSync(dir, { recursive: true });
    const log = join(dir, `${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
    console.log(`${runs.length} runs, old words v${personas.old.version}, new words v${personas.new.version}; Claude ${models.claude}, Codex ${models.codex} ${models.effort}`);
    const results = [];
    // Three at a time: a run is mostly waiting on the model.
    const queue = [...runs];
    await Promise.all(
      Array.from({ length: 3 }, async () => {
        for (let r = queue.shift(); r; r = queue.shift()) {
          const c = CASES.find((x) => x.id === r.case);
          const said = await runCase(c, r.cli, personas[r.side], models);
          const graded = said.error ? { pass: false, why: `error: ${said.error}` } : c.grade(said);
          const result = { ...r, ...graded, cost: said.cost, tokens: said.tokens, text: said.text, helpers: said.helpers };
          results.push(result);
          appendFileSync(log, JSON.stringify(result) + '\n');
          console.log(`${r.case} ${r.cli} ${r.side} #${r.sample}: ${graded.pass ? 'pass' : 'FAIL'} (${graded.why})`);
        }
      }),
    );
    console.log(['', ...summary(results), `runs kept in ${log}`].join('\n'));
  }
}
