// TL-ADE's side of the real-work benchmark (docs/superpowers/plans/2026-10-22-real-work-bench.md): one task per run,
// started by the meter. It starts `ade serve` on a temporary repo and home, and a headless Chrome on its panel, drives the
// panel over DevTools the way Faber's scenario drives Faber, and writes what it noted as JSON to `--notes`.
//
//   node scripts/bench/tlade.mjs --task <1-4|6> --tlade <folder> --standin <faber-standin.exe> --notes <file>
//
// The panel's chat runs `claude` from PATH, so PATH holds only the stand-in (as claude.exe, answering what Faber's
// recordings answer), Node and git: no real CLI is ever found, so no quota is spent and no sign-in folder is read. The
// home folders point into the temporary folder too. Children start with no shell and no window.
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const CHROME = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const GIT = 'C:\\Program Files\\Git\\cmd\\git.exe';
const QUESTION = 'In one sentence: why is the sky blue?';
const AGAIN = 'What single word did you reply with last time? Answer with only that word.';
const BOX = 'textarea[aria-label="Pergunta ao chat"]';
const ANSWERS = '.chat-turn.assistant .chat-text';

/** The tasks this side can do: 5 (no stop in its chat) and 7 (its proof runs only inside a mission) are not here. */
export const TASKS = [1, 2, 3, 4, 6];

/** The final text of a recording of Faber's fixtures, which the stand-in prints. */
export function answerOf(recording) {
  const lines = readFileSync(recording, 'utf8')
    .trim()
    .split('\n')
    .map((l) => JSON.parse(l));
  const result = lines.find((l) => l.msg.type === 'result');
  if (typeof result?.msg.result !== 'string') throw new Error(`${recording} has no final text`);
  return result.msg.result;
}

/** A conversation of `turns` questions and answers in TL-ADE's chat file shape, like Faber's task 4 seeds. */
export function thread(turns, at = Date.parse('2026-09-21T00:00:00Z')) {
  const paragraph = 'Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ';
  const fence = '`'.repeat(3);
  return {
    turns: Array.from({ length: turns * 2 }, (_, i) => ({
      id: `T-5f1d${String(i).padStart(4, '0')}-0000-4000-8000-000000000000`,
      role: i % 2 ? 'assistant' : 'user',
      text: i % 2 ? [paragraph.repeat(6), '- one\n- two', `${fence}js\nconsole.log(${i});\n${fence}`].join('\n\n') : `Question ${i / 2}: why is the sky blue?`,
      at: new Date(at + i).toISOString(),
    })),
  };
}

/** A quota receipt its Models page trusts: official, fresh, before its reset. */
export function receipt(family, usedPercent, now = Date.now()) {
  return {
    source: 'official',
    family,
    observed_at: new Date(now).toISOString(),
    weekly_reset_at: new Date(now + 3 * 86400_000).toISOString(),
    used_percent: usedPercent,
    reserved_percent: 0,
  };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const seconds = (since) => Number(((performance.now() - since) / 1000).toFixed(2));

function freePort() {
  return new Promise((ok, fail) => {
    const server = createServer().once('error', fail);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => ok(port));
    });
  });
}

async function until(what, check, ms = 30_000) {
  const end = performance.now() + ms;
  for (;;) {
    const value = await check().catch(() => null);
    if (value) return value;
    if (performance.now() > end) throw new Error(`waited ${ms / 1000} s for ${what}`);
    await sleep(50);
  }
}

/** One DevTools connection to the panel's page. */
async function devtools(profile) {
  const file = join(profile, 'DevToolsActivePort');
  const port = (await until('Chrome to listen', async () => existsSync(file) && readFileSync(file, 'utf8').split('\n')[0])).trim();
  const pages = await until('a page to drive', async () => {
    const list = await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(5_000) })).json();
    return list.find((t) => t.type === 'page');
  });
  const socket = new WebSocket(pages.webSocketDebuggerUrl);
  await new Promise((ok, fail) => {
    socket.onopen = ok;
    socket.onerror = socket.onclose = () => fail(new Error('DevTools refused the connection'));
    setTimeout(() => fail(new Error('DevTools did not connect in 30 s')), 30_000);
  });
  let id = 0;
  const waiting = new Map();
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    waiting.get(message.id)?.(message);
    waiting.delete(message.id);
  };
  // A Chrome that is gone fails what it was asked instead of hanging the run, which `until` cannot time out.
  socket.onerror = socket.onclose = () => {
    for (const answer of waiting.values()) answer({ error: { message: 'DevTools closed' } });
    waiting.clear();
  };
  const call = (method, params = {}) =>
    new Promise((ok, fail) => {
      const at = ++id;
      const late = setTimeout(() => {
        waiting.delete(at);
        fail(new Error(`${method}: no answer in 30 s`));
      }, 30_000);
      waiting.set(at, (m) => {
        clearTimeout(late);
        if (m.error) fail(new Error(`${method}: ${m.error.message}`));
        else ok(m.result);
      });
      socket.send(JSON.stringify({ id: at, method, params }));
    });
  const evaluate = async (expression) => {
    const { result, exceptionDetails } = await call('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  };
  return { call, evaluate, close: () => socket.close() };
}

/** What Chrome needs of the system to start: where Windows and programs live, never a home or a sign-in. */
const WINDOWS = Object.fromEntries(
  ['SystemDrive', 'windir', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'CommonProgramFiles', 'ProgramData', 'OS', 'PROCESSOR_ARCHITECTURE', 'NUMBER_OF_PROCESSORS']
    .filter((name) => process.env[name])
    .map((name) => [name, process.env[name]]),
);

/** A hidden child that keeps why it could not start, and `ended`, which settles even for one that never ran. */
function child(program, args, options) {
  const p = spawn(program, args, { ...options, stdio: 'ignore', windowsHide: true });
  p.ended = new Promise((ok) => {
    p.once('close', ok);
    p.once('error', (e) => {
      p.failed = e;
      ok();
    });
  });
  return p;
}

/** The panel: `ade serve` and a headless Chrome on it, inside the meter's job. Chrome's own updates are off: they
 * download through a Windows service outside the job, cost what TL-ADE does not, and leave files in the temp folder. */
async function start(s) {
  const serve = child(process.execPath, [join(s.tlade, 'bin', 'ade.js'), 'serve', '--repo', s.repo, '--port', String(s.port), '--no-open'], { cwd: s.tlade, env: s.env });
  const panel = { serve, chrome: null, page: null };
  try {
    const url = `http://127.0.0.1:${s.port}/`;
    await until('the panel to answer', async () => (await fetch(url, { signal: AbortSignal.timeout(5_000) })).ok);
    const profile = join(s.tmp, `chrome-${s.starts++}`);
    panel.chrome = child(CHROME, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-component-update', '--disable-background-networking', 'about:blank'], {
      env: { ...s.env, ...WINDOWS },
    });
    panel.page = await devtools(profile);
    await panel.page.call('Page.enable');
    await panel.page.call('Page.navigate', { url });
    return panel;
  } catch (e) {
    await stop(panel);
    throw panel.serve.failed ?? panel.chrome?.failed ?? e;
  }
}

/** Ends what started, and returns once each process has ended, so its folder can go. */
function stop(panel) {
  panel.page?.close();
  const running = [panel.chrome, panel.serve].filter(Boolean);
  for (const p of running) p.kill();
  return Promise.all(running.map((p) => p.ended));
}

const count = (page) => page.evaluate(`[...document.querySelectorAll(${JSON.stringify(ANSWERS)})].filter(a => a.innerText.trim()).length`);

/** Types a question the way a person does and sends it; returns when it went. */
async function ask(page, text) {
  await until('the message box', () => page.evaluate(`!!document.querySelector(${JSON.stringify(BOX)})`));
  await page.evaluate(`(() => {
    const box = document.querySelector(${JSON.stringify(BOX)});
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, ${JSON.stringify(text)});
    box.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await until('the send button', () => page.evaluate(`document.querySelector('form.composer button[type=submit]')?.disabled === false`));
  await page.evaluate(`document.querySelector('form.composer button[type=submit]').click()`);
}

/** Sends a question and notes when its answer shows: the reply is one block, so its first words are its end. */
async function turn(page, text, notes, name) {
  const before = await count(page);
  const sent = performance.now();
  await ask(page, text);
  await until('the answer', async () => (await count(page)) > before, 60_000);
  notes[name] = seconds(sent);
}

async function run(task, s) {
  const notes = {};
  const began = performance.now();
  let panel = await start(s);
  const { page } = panel;
  try {
    if (task === 1) {
      await until('the message box', () => page.evaluate(`!!document.querySelector(${JSON.stringify(BOX)})`));
      notes.open_s = seconds(began);
      await sleep(60_000);
    } else if (task === 2) {
      await turn(page, QUESTION, notes, 'first_words_s');
      notes.end_s = notes.first_words_s;
    } else if (task === 3) {
      await turn(page, QUESTION, notes, 'first_turn_s');
      await stop(panel);
      writeFileSync(join(s.bin, 'reply.txt'), answerOf(join(root, 'tests/fixtures/claude/resume.jsonl')));
      const reopened = performance.now();
      panel = await start(s);
      await until('the earlier answer', async () => (await count(panel.page)) === 1);
      notes.reopened_s = seconds(reopened);
      await turn(panel.page, AGAIN, notes, 'second_turn_s');
    } else if (task === 4) {
      const opened = performance.now();
      await until('the last answer', async () => (await count(page)) === 50);
      notes.open_s = seconds(opened);
      const scrolled = performance.now();
      // The nearest ancestor of the turns that scrolls goes to its top, until the first question is in view.
      await until('the first message in view', () =>
        page.evaluate(`(() => {
        const first = document.querySelector('.chat-turn');
        let box = first?.parentElement;
        while (box && box.scrollHeight <= box.clientHeight) box = box.parentElement;
        (box ?? document.scrollingElement).scrollTop = 0;
        const top = first?.getBoundingClientRect().top;
        return first?.innerText.includes('Question 0:') && top >= 0 && top < innerHeight;
      })()`),
      );
      notes.scrolled_to_the_top_s = seconds(scrolled);
    } else if (task === 6) {
      const opened = performance.now();
      // Its tabs are drawn in capitals, so the label is matched on the text itself, not on what is shown.
      const click = (label) => `(() => { const b = [...document.querySelectorAll('button, [role=tab], a')].find(e => e.textContent.trim() === ${JSON.stringify(label)}); b?.click(); return !!b; })()`;
      await until('the Options tab', () => page.evaluate(click('Opções')));
      await until('the Models part', () => page.evaluate(click('Modelos')));
      await until('the limit to show', () => page.evaluate(`document.querySelector('[data-testid="quota-claude"] .mono')?.innerText.trim() === '47%'`));
      notes.limit_shown_s = seconds(opened);
    } else {
      throw new Error(`task ${task} is not one TL-ADE can do: ${TASKS.join(', ')}`);
    }
  } finally {
    await stop(panel);
  }
  return notes;
}

async function main() {
  const args = process.argv.slice(2);
  const flag = (name) => {
    const at = args.indexOf(name);
    if (at < 0) throw new Error(`${name} is needed`);
    return args[at + 1];
  };
  const task = Number(flag('--task'));
  const tlade = resolve(flag('--tlade'));
  const standin = resolve(flag('--standin'));
  const out = resolve(flag('--notes'));
  if (!TASKS.includes(task)) throw new Error(`task ${task} is not one TL-ADE can do: ${TASKS.join(', ')}`);
  for (const program of [CHROME, GIT, standin, join(tlade, 'bin', 'ade.js')]) {
    if (!existsSync(program)) throw new Error(`missing ${program}`);
  }
  const tmp = mkdtempSync(join(tmpdir(), 'tlade-bench-'));
  let failed = true;
  try {
    const repo = join(tmp, 'repo');
    const home = join(tmp, 'home');
    const bin = join(tmp, 'bin');
    for (const dir of [repo, bin, join(tmp, 'temp'), join(home, 'AppData', 'Roaming'), join(home, 'AppData', 'Local')]) mkdirSync(dir, { recursive: true });
    const path = [bin, dirname(process.execPath), dirname(GIT)];
    // Only the stand-in answers to an AI's name: a real CLI here would spend quota.
    for (const dir of path.slice(1)) {
      const found = readdirSync(dir).filter((f) => /^(claude|codex|agy|gemini)(\.|$)/i.test(f));
      if (found.length) throw new Error(`${dir} holds ${found.join(', ')}`);
    }
    // Every child sees only this: the owner's home, git settings and hooks stay out of reach.
    const env = {
      PATH: path.join(';'),
      SystemRoot: process.env.SystemRoot,
      ComSpec: process.env.ComSpec,
      ADE_HOME: home,
      HOME: home,
      USERPROFILE: home,
      APPDATA: join(home, 'AppData', 'Roaming'),
      LOCALAPPDATA: join(home, 'AppData', 'Local'),
      TEMP: join(tmp, 'temp'),
      TMP: join(tmp, 'temp'),
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_CONFIG_GLOBAL: join(home, '.gitconfig'),
    };
    const git = (...a) => {
      const r = spawnSync(GIT, a, { cwd: repo, env, windowsHide: true, encoding: 'utf8' });
      if (r.status !== 0) throw new Error(`git ${a.join(' ')}: ${r.stderr}`);
    };
    git('init', '-q', '--template=');
    writeFileSync(join(repo, 'README.md'), '# Sample\n');
    git('add', 'README.md');
    git('-c', 'user.name=Bench', '-c', 'user.email=bench@example.invalid', 'commit', '-q', '--no-gpg-sign', '--no-verify', '-m', 'sample');
    copyFileSync(standin, join(bin, 'claude.exe'));
    writeFileSync(join(bin, 'reply.txt'), answerOf(join(root, 'tests/fixtures/claude/chat.jsonl')));
    if (task === 4) {
      mkdirSync(join(repo, '.ade', 'chat'), { recursive: true });
      writeFileSync(join(repo, '.ade', 'chat', 'repo.json'), JSON.stringify(thread(50)));
    }
    if (task === 6) {
      mkdirSync(join(home, '.ade'), { recursive: true });
      writeFileSync(join(home, '.ade', 'quota-claude.json'), JSON.stringify(receipt('claude', 47)));
    }
    const notes = await run(task, {
      tlade,
      repo,
      bin,
      tmp,
      env,
      port: await freePort(),
      starts: 0,
    });
    writeFileSync(out, JSON.stringify(notes));
    failed = false;
  } finally {
    // Every process has ended by now. A folder left behind fails a run that was otherwise fine; after a failure the
    // run's own error is the one that matters, so the leftover is only reported.
    try {
      rmSync(tmp, { recursive: true, force: true, maxRetries: 5 });
    } catch (e) {
      const left = new Error(`left ${tmp}: ${e.message}`);
      if (!failed) throw left;
      console.error(`tlade.mjs: ${left.message}`);
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then(
    () => process.exit(0),
    (error) => {
      console.error(`tlade.mjs: ${error.message}`);
      process.exit(1);
    },
  );
}
