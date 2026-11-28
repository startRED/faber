// Faber Dev: the owner's own copy of Faber for trying what landed, next to the installed Faber and never touching it.
//
//   node scripts/dev-edition.mjs install          first time: builds Faber Dev, the hub page and the desktop shortcuts
//   node scripts/dev-edition.mjs start [--keep]   opens Faber Dev as a new person (--keep: reopens the last session)
//   node scripts/dev-edition.mjs update [--force] builds the newest green `phase-1-base` and refreshes the hub page
//   node scripts/dev-edition.mjs hub              refreshes and opens the hub page (changes by day and every screen)
//
// Everything lives in one folder (FABER_DEV_HOME, default E:\Apps\FaberDev): its own clone of the repository (never the
// working checkout other sessions change), its own build folder, its own data and the hub page. It builds only commits
// that landed on `phase-1-base`, which CI lets in only when every system is green, and a failed build keeps the last
// working program. Faber Dev runs with FABER_DATA_DIR, so the app keeps its own data and, as in tests, never updates
// itself, never registers to start with Windows and never holds the single-instance lock. The AIs are the real CLIs with
// their own sign-in; nothing here reads or copies it.
//
// No shell anywhere: every program starts with an argument list. Builds run at idle priority so a game keeps the processor.
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { constants, setPriority } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const HOME = resolve(process.env.FABER_DEV_HOME ?? 'E:\\Apps\\FaberDev');
const SRC = join(HOME, 'src');
const TARGET = join(HOME, 'target');
const APP = join(HOME, 'app');
const EXE = join(APP, 'faber-dev.exe');
const NEXT = join(APP, 'faber-dev.next.exe');
const DATA = join(HOME, 'data');
const HUB = join(HOME, 'hub');
const STATE = join(HOME, 'state.json');
const LOCK = join(HOME, 'update.lock');
const LOG = join(HOME, 'update.log');
const SELF = join(HOME, 'dev-edition.mjs');
const REPO = 'startRED/faber';
const BRANCH = 'phase-1-base';
const DAYS = 21;
const HERE = dirname(fileURLToPath(import.meta.url));
// The owner's PC keeps Rust outside the default folders (AGENTS.md); elsewhere the usual ones are found on PATH.
const RUST = { RUSTUP_HOME: 'E:\\Apps\\rust\\rustup', CARGO_HOME: 'E:\\Apps\\rust\\cargo', extra: ['E:\\Apps\\zig', 'E:\\Apps\\rust\\cargo\\bin'] };
// Only what tells this build apart from the installed Faber: its own name and its own WebView2 profile folder.
const CONFIG = JSON.stringify({ identifier: 'io.github.startred.faber.dev', productName: 'Faber Dev', bundle: { active: false } });

const say = (line) => console.log(`[faber-dev] ${line}`);
const readState = () => (existsSync(STATE) ? JSON.parse(readFileSync(STATE, 'utf8')) : {});
const writeState = (patch) => writeFileSync(STATE, JSON.stringify({ ...readState(), ...patch }, null, 2));

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: 'utf8', windowsHide: true, maxBuffer: 256 * 1024 * 1024, ...options });
  if (result.error) throw result.error;
  if (result.status !== 0 && !options.allowFail) {
    throw new Error(`${basename(program)} ${args.slice(0, 3).join(' ')} failed (${result.status}): ${(result.stderr ?? '').slice(-2000)}`);
  }
  return result;
}
const git = (...args) => run('git', ['-C', SRC, ...args]).stdout.trim();

function buildEnv() {
  const env = { ...process.env, CARGO_TARGET_DIR: TARGET };
  if (existsSync(RUST.CARGO_HOME)) {
    Object.assign(env, { RUSTUP_HOME: RUST.RUSTUP_HOME, CARGO_HOME: RUST.CARGO_HOME });
    env.PATH = [...RUST.extra, env.PATH ?? env.Path].join(';');
    delete env.Path;
  }
  return env;
}

/** Brings the private clone to the newest `phase-1-base`; returns its commit. */
function syncSource() {
  mkdirSync(HOME, { recursive: true });
  if (!existsSync(join(SRC, '.git'))) {
    say('first copy of the code (one time)...');
    const local = resolve(HERE, '..');
    const reference = existsSync(join(local, '.git')) ? ['--reference-if-able', local, '--dissociate'] : [];
    run('git', ['clone', '--branch', BRANCH, ...reference, `https://github.com/${REPO}.git`, SRC], { stdio: 'inherit' });
  }
  git('fetch', '--quiet', 'origin', BRANCH);
  const sha = git('rev-parse', 'FETCH_HEAD');
  git('checkout', '--quiet', '--force', '--detach', sha);
  git('clean', '-fdq', '-e', 'node_modules');
  return sha;
}

const hashOf = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');

function build(sha) {
  const env = buildEnv();
  const lock = hashOf(join(SRC, 'package-lock.json'));
  if (readState().lock !== lock || !existsSync(join(SRC, 'node_modules'))) {
    say('installing the screen packages...');
    run(process.execPath, [npmCli(), 'ci', '--no-audit', '--no-fund'], { cwd: SRC, env, stdio: 'inherit' });
    writeState({ lock });
  }
  say(`building Faber Dev at ${sha.slice(0, 8)} (the first build takes a few minutes, later ones less)...`);
  const tauri = join(SRC, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
  const args = [tauri, 'build', '--debug', '--no-bundle', '--config', CONFIG];
  try {
    run(process.execPath, args, { cwd: SRC, env, stdio: 'inherit' });
  } catch {
    // The shared compile cache's server sometimes drops its connection mid-build; a build without it is slower but sure.
    say('build failed; trying once more without the shared compile cache...');
    run(process.execPath, args, { cwd: SRC, env: { ...env, RUSTC_WRAPPER: '' }, stdio: 'inherit' });
  }
  mkdirSync(APP, { recursive: true });
  copyFileSync(join(TARGET, 'debug', 'faber.exe'), NEXT);
  writeState({ pending: sha, builtAt: new Date().toISOString(), lastFailure: null });
}

function npmCli() {
  const candidates = [join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js'), 'E:\\Apps\\nodejs\\node_modules\\npm\\bin\\npm-cli.js'];
  const found = candidates.find(existsSync);
  if (!found) throw new Error('npm not found next to node');
  return found;
}

/** A waiting build replaces the program only while Faber Dev is closed: Windows refuses to rename a running one. */
function promote() {
  if (!existsSync(NEXT)) return;
  try {
    if (existsSync(EXE)) renameSync(EXE, join(APP, 'faber-dev.prev.exe'));
    renameSync(NEXT, EXE);
    const state = readState();
    writeState({ current: state.pending, pending: null });
    say(`now on ${String(state.pending).slice(0, 8)}`);
  } catch {
    say('Faber Dev is open: the new build waits for the next start');
  }
}

function update({ force = false, quiet = false } = {}) {
  mkdirSync(HOME, { recursive: true });
  if (existsSync(LOCK) && Date.now() - statSync(LOCK).mtimeMs < 60 * 60 * 1000) {
    say('another update is running');
    return false;
  }
  writeFileSync(LOCK, String(process.pid));
  try {
    try {
      setPriority(constants.priority.PRIORITY_LOW);
    } catch {}
    const sha = syncSource();
    const state = readState();
    const have = state.pending ?? state.current;
    if (force || have !== sha || !(existsSync(EXE) || existsSync(NEXT))) {
      try {
        build(sha);
      } catch (error) {
        writeState({ lastFailure: { sha, at: new Date().toISOString(), message: String(error.message).slice(-1500) } });
        say(`build failed, the last working Faber Dev stays: ${error.message.split('\n')[0]}`);
      }
    } else if (!quiet) say('already on the newest green version');
    const script = join(SRC, 'scripts', 'dev-edition.mjs');
    if (existsSync(script) && resolve(script) !== resolve(SELF)) copyFileSync(script, SELF);
    hub({ open: false });
    return true;
  } finally {
    rmSync(LOCK, { force: true });
  }
}

/**
 * Deletes a session folder unless it is still open. A project's working copy is a git worktree of the person's own
 * repository, so each repository a conversation named is told to forget the copies that are gone (`git worktree prune`).
 */
function removeSession(folder) {
  const roots = new Set();
  for (const conversations of [join(folder, 'faber', 'conversations'), join(folder, 'conversations')]) {
    if (!existsSync(conversations)) continue;
    for (const name of readdirSync(conversations)) {
      try {
        const root = JSON.parse(readFileSync(join(conversations, name), 'utf8')).copy?.root;
        if (root) roots.add(root);
      } catch {}
    }
  }
  try {
    rmSync(folder, { recursive: true });
  } catch {
    return;
  }
  for (const root of roots) run('git', ['-C', root, 'worktree', 'prune'], { allowFail: true });
}

/**
 * Every start is a new person by default: a new, empty session folder holding both Faber's data and the page's own
 * browser profile (WebView2 lets WEBVIEW2_USER_DATA_FOLDER override the folder the app asks for), so nothing carries
 * over: not conversations, not projects, not what the screen keeps in local storage. `keep` reopens the newest session.
 * Older sessions are deleted, except one still open (Windows refuses to delete files in use).
 */
function start({ keep = false } = {}) {
  if (!existsSync(EXE) && !existsSync(NEXT)) update();
  promote();
  if (!existsSync(EXE)) throw new Error(`no Faber Dev program yet, see ${LOG}`);
  mkdirSync(DATA, { recursive: true });
  const newest = readdirSync(DATA).filter((n) => /^\d{4}-\d\d-\d\dT[\d-]+Z$/.test(n)).sort().at(-1);
  const session = keep && newest ? newest : new Date().toISOString().replace(/[:.]/g, '-');
  for (const old of readdirSync(DATA)) if (old !== session) removeSession(join(DATA, old));
  // Faber puts a few folders next to its data folder, so the data sits one level down and the session holds them all.
  const folder = join(DATA, session);
  mkdirSync(join(folder, 'webview'), { recursive: true });
  say(keep && newest ? `continuing session ${session}` : 'starting as a new person, from the first screen');
  const env = { ...process.env, FABER_DATA_DIR: join(folder, 'faber'), WEBVIEW2_USER_DATA_FOLDER: join(folder, 'webview') };
  delete env.FABER_CHANNEL;
  spawn(EXE, [], { cwd: APP, env, detached: true, stdio: 'ignore' }).unref();
  // Look for a newer green build in the background; it is used from the next start.
  writeFileSync(LOG, `--- ${new Date().toISOString()}\n`);
  const out = openSync(LOG, 'a');
  const script = existsSync(SELF) ? SELF : fileURLToPath(import.meta.url);
  spawn(process.execPath, [script, 'update', '--quiet'], { detached: true, windowsHide: true, stdio: ['ignore', out, out] }).unref();
}

// ---------------------------------------------------------------------------------------------------------------- hub

const KIND = { new: 'Novo', better: 'Melhor', fixed: 'Corrigido' };
const TYPE = { feat: 'Novo', fix: 'Correção', perf: 'Mais leve' };

/** Commits of the last DAYS days on the clone's branch, newest first. */
function commits() {
  const out = git('log', `--since=${DAYS}.days`, '--date=format-local:%Y-%m-%d %H:%M', '--format=%x1e%H%x1f%ad%x1f%s', 'HEAD');
  return out
    .split('\x1e')
    .filter(Boolean)
    .map((chunk) => {
      const [hash, when, subject] = chunk.trim().split('\x1f');
      const type = /^(\w+)/.exec(subject)?.[1] ?? 'other';
      return { hash, day: when.slice(0, 10), time: when.slice(11), subject, type, notes: [] };
    });
}

/** The pt-BR "what's new" lines each commit added, numbered (locale file) or waiting for a number (docs/next). */
function attachNotes(list) {
  const byHash = new Map(list.map((c) => [c.hash, c]));
  const numbered = git('log', `--since=${DAYS}.days`, '--format=%x1e%H', '-p', '-U0', 'HEAD', '--', 'ui/locales/pt-BR.ts');
  for (const chunk of numbered.split('\x1e').filter(Boolean)) {
    const [hash, ...lines] = chunk.trim().split('\n');
    for (const line of lines) {
      const m = /^\+\s*'notes\.(\d+)\.(new|better|fixed)':\s*(['"`])(.*)\3,?\s*$/.exec(line);
      if (m && byHash.has(hash)) byHash.get(hash).notes.push({ kind: m[2], text: m[4].replace(/\\'/g, "'") });
    }
  }
  const waiting = git('log', `--since=${DAYS}.days`, '--diff-filter=A', '--format=%x1e%H', '--name-only', 'HEAD', '--', 'docs/next/');
  for (const chunk of waiting.split('\x1e').filter(Boolean)) {
    const [hash, ...files] = chunk.trim().split('\n').filter(Boolean);
    for (const file of files) {
      try {
        const notes = JSON.parse(git('show', `${hash}:${file}`)).notes ?? {};
        for (const [kind, langs] of Object.entries(notes)) if (langs['pt-BR'] && byHash.has(hash)) byHash.get(hash).notes.push({ kind, text: langs['pt-BR'] });
      } catch {}
    }
  }
}

/** The screenshots of the newest green CI run of the branch, downloaded once per run. */
function screens() {
  const shots = join(HUB, 'shots');
  try {
    const list = JSON.parse(
      run('gh', ['run', 'list', '-R', REPO, '--branch', BRANCH, '--workflow', 'ci.yml', '--status', 'success', '-L', '1', '--json', 'databaseId,headSha,createdAt']).stdout,
    );
    if (!list.length) return null;
    const { databaseId: id, headSha, createdAt } = list[0];
    const folder = join(shots, String(id));
    if (!existsSync(folder)) {
      say('downloading the screens of the newest green run...');
      const tmp = join(HUB, 'download');
      rmSync(tmp, { recursive: true, force: true });
      run('gh', ['run', 'download', String(id), '-R', REPO, '-p', 'log-screen-*', '-D', tmp], { allowFail: true });
      mkdirSync(folder, { recursive: true });
      for (const file of walk(tmp)) if (file.endsWith('.png')) copyFileSync(file, join(folder, basename(file)));
      rmSync(tmp, { recursive: true, force: true });
      for (const old of readdirSync(shots)) if (old !== String(id)) rmSync(join(shots, old), { recursive: true, force: true });
    }
    return { id, headSha, createdAt, files: readdirSync(folder).filter((f) => f.endsWith('.png')).sort() };
  } catch (error) {
    say(`screens not refreshed: ${String(error.message).split('\n')[0]}`);
    return null;
  }
}

function* walk(dir) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else yield full;
  }
}

// From the first screen a newcomer sees to the deepest ones; a photo goes to the first group whose words its name has.
const GROUPS = [
  ['Primeira vez e configuração', /setup|first|welcome|tutorial|tour|onboard|sign|account|install|whats-new|notes|update/],
  ['Chat', /chat|conversation|thread|composer|answer|ask|message|recents|home/],
  ['Projetos', /project|agent|subagent|helper|running|step|mission|browser|preview|context|worklog/],
  ['Ajustes e aparência', /setting|appearance|theme|look|quota|limit|preference|workbench|manage/],
  ['Outras', /./],
];

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const DAY_NAME = new Intl.DateTimeFormat('pt-BR', { weekday: 'long', day: 'numeric', month: 'long' });

function hub({ open = true } = {}) {
  if (!existsSync(join(SRC, '.git'))) syncSource();
  mkdirSync(HUB, { recursive: true });
  const list = commits();
  attachNotes(list);
  const shots = screens();
  const state = readState();
  const days = [...new Set(list.map((c) => c.day))];

  const dayHtml = days
    .map((day) => {
      const items = list.filter((c) => c.day === day);
      const notes = items.flatMap((c) => c.notes);
      const visible = items.filter((c) => TYPE[c.type]);
      const behind = items.filter((c) => !TYPE[c.type]);
      const date = DAY_NAME.format(new Date(`${day}T12:00:00`));
      return `<section class="day"><h2>${esc(date)} <small>${items.length} mudanças</small></h2>
${notes.length ? `<h3>O que você vê de diferente</h3><ul class="notes">${notes.map((n) => `<li><b class="k-${n.kind}">${KIND[n.kind] ?? n.kind}</b> ${esc(n.text)}</li>`).join('')}</ul>` : ''}
${visible.length ? `<h3>Mudanças no app</h3><ul class="commits">${visible.map((c) => `<li><b class="t-${c.type}">${TYPE[c.type]}</b> ${esc(c.subject.replace(/^\w+(\([^)]*\))?!?:\s*/, '').replace(/\s*\(R-?\w+(, R-?\w+)*\)$/, ''))} <a href="https://github.com/${REPO}/commit/${c.hash}">${c.time}</a></li>`).join('')}</ul>` : ''}
${behind.length ? `<details><summary>Bastidores (${behind.length})</summary><ul class="commits">${behind.map((c) => `<li>${esc(c.subject)} <a href="https://github.com/${REPO}/commit/${c.hash}">${c.time}</a></li>`).join('')}</ul></details>` : ''}
</section>`;
    })
    .join('\n');

  let screenHtml = '<p>Sem fotos agora (o GitHub CLI não respondeu). Tente "Atualizar" de novo.</p>';
  if (shots) {
    const placed = new Map(GROUPS.map(([name]) => [name, []]));
    for (const file of shots.files) placed.get(GROUPS.find(([, re]) => re.test(file))[0]).push(file);
    screenHtml =
      `<p class="meta">${shots.files.length} fotos da última versão aprovada em todos os testes (${esc(shots.headSha.slice(0, 8))}, ${esc(new Date(shots.createdAt).toLocaleString('pt-BR'))}). Clique para ampliar.</p>` +
      [...placed]
        .filter(([, files]) => files.length)
        .map(
          ([name, files]) =>
            `<h2>${esc(name)} <small>${files.length}</small></h2><div class="grid">${files.map((f) => `<figure><img loading="lazy" src="shots/${shots.id}/${esc(f)}" alt="${esc(f)}" onclick="zoom(this)"><figcaption>${esc(f.replace(/\.png$/, '').replaceAll('-', ' '))}</figcaption></figure>`).join('')}</div>`,
        )
        .join('');
  }

  const failure = state.lastFailure && state.lastFailure.sha !== state.current && state.lastFailure.sha !== state.pending;
  const html = `<!doctype html><html lang="pt-BR"><meta charset="utf-8"><title>Faber Dev</title>
<style>
:root{color-scheme:dark;--bg:#111214;--fg:#e8e6e1;--mute:#8d8b86;--line:#26272b;--acc:#e0a96d}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 system-ui,Segoe UI,sans-serif}
header{position:sticky;top:0;background:var(--bg);border-bottom:1px solid var(--line);padding:18px 32px;display:flex;gap:24px;align-items:baseline;z-index:2}
header h1{margin:0;font-size:20px}header nav a{color:var(--mute);margin-right:16px;text-decoration:none;cursor:pointer}header nav a.on{color:var(--fg)}
main{max-width:1240px;margin:0 auto;padding:24px 32px 80px}.meta,small{color:var(--mute);font-weight:400}
.day{border-bottom:1px solid var(--line);padding:8px 0 20px}h2{font-size:18px;text-transform:capitalize}h3{font-size:13px;color:var(--mute);text-transform:uppercase;letter-spacing:.06em;margin:16px 0 6px}
ul{margin:0;padding-left:0;list-style:none}li{padding:4px 0}li b{display:inline-block;min-width:84px;font-size:12px;font-weight:600}
.k-new,.t-feat{color:#8fd19e}.k-better,.t-perf{color:#8ab4f8}.k-fixed,.t-fix{color:var(--acc)}a{color:var(--mute)}
details summary{cursor:pointer;color:var(--mute);margin-top:10px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:16px}figure{margin:0}
img{width:100%;border-radius:8px;border:1px solid var(--line);cursor:zoom-in;display:block}figcaption{color:var(--mute);font-size:13px;padding-top:4px}
#big{position:fixed;inset:0;background:#000d;display:none;align-items:center;justify-content:center;z-index:9;cursor:zoom-out}#big img{max-width:96vw;max-height:94vh;width:auto;cursor:zoom-out}
.card{border:1px solid var(--line);border-radius:10px;padding:16px 20px;margin:12px 0}.warn{border-color:var(--acc)}code{background:#1c1d20;padding:1px 6px;border-radius:4px}
[hidden]{display:none!important}
</style>
<header><h1>Faber Dev</h1><nav><a data-tab="days" class="on">Mudanças por dia</a><a data-tab="screens">Todas as telas</a><a data-tab="how">Como testar</a></nav>
<span class="meta">Versão aberta: ${esc(String(state.current ?? '—').slice(0, 8))}${state.pending ? ` · nova esperando: ${esc(state.pending.slice(0, 8))} (feche e abra o Faber Dev)` : ''} · página de ${esc(new Date().toLocaleString('pt-BR'))}</span></header>
<main>
<div id="days">${failure ? `<div class="card warn">A versão ${esc(state.lastFailure.sha.slice(0, 8))} não montou no seu PC; o Faber Dev continua na anterior. Detalhes em <code>${esc(LOG)}</code>.</div>` : ''}${dayHtml || '<p>Nenhuma mudança nos últimos dias.</p>'}</div>
<div id="screens" hidden>${screenHtml}</div>
<div id="how" hidden>
<div class="card"><b>Faber Dev</b> (atalho na área de trabalho): abre o app como um usuário novo, sempre pela primeira tela de configuração, sem nenhuma conversa, projeto ou preferência de antes. Ao abrir, ele procura sozinho a versão mais nova aprovada nos testes e a monta em segundo plano; ela entra na próxima vez que você abrir.</div>
<div class="card"><b>Faber Dev continuar</b>: reabre a última sessão de teste, para seguir de onde parou.</div>
<div class="card"><b>Faber Dev novidades</b>: atualiza e abre esta página.</div>
<div class="card">O Faber Dev é separado do seu Faber normal: dados próprios em <code>${esc(DATA)}</code> (apagados na próxima vez que abrir), não se atualiza sozinho, não abre com o Windows e pode ficar aberto junto com o Faber normal. As IAs são as de verdade, com o seu login de sempre.</div>
<div class="card">Só entram versões que passaram em todos os testes. Se uma não montar no seu PC, o Faber Dev continua na anterior.</div>
</div></main>
<div id="big" onclick="this.style.display='none'"><img></div>
<script>
function zoom(img){const b=document.getElementById('big');b.querySelector('img').src=img.src;b.style.display='flex'}
document.querySelectorAll('nav a').forEach(a=>a.onclick=()=>{document.querySelectorAll('nav a').forEach(x=>x.classList.toggle('on',x===a));['days','screens','how'].forEach(id=>document.getElementById(id).hidden=id!==a.dataset.tab)});
document.addEventListener('keydown',e=>{if(e.key==='Escape')document.getElementById('big').style.display='none'});
</script></html>`;
  const page = join(HUB, 'index.html');
  writeFileSync(page, html);
  if (open) spawn('explorer.exe', [page], { detached: true, stdio: 'ignore' }).unref();
  return page;
}

// ------------------------------------------------------------------------------------------------------------ install

function shortcuts() {
  const icon = join(HOME, 'faber-dev.ico');
  const source = join(SRC, 'src-tauri', 'icons', 'icon.ico');
  if (existsSync(source)) copyFileSync(source, icon);
  const items = [
    ['Faber Dev', 'start'],
    ['Faber Dev continuar', 'start --keep'],
    ['Faber Dev novidades', 'hub'],
  ];
  const lines = items.map(
    ([name, args]) =>
      `$s=$w.CreateShortcut((Join-Path $d '${name}.lnk'));$s.TargetPath='${process.execPath}';$s.Arguments='"${SELF}" ${args}';$s.WorkingDirectory='${HOME}';$s.IconLocation='${icon}';$s.WindowStyle=7;$s.Save()`,
  );
  const script = `$w=New-Object -ComObject WScript.Shell;$d=[Environment]::GetFolderPath('Desktop');${lines.join(';')}`;
  run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script]);
  say('shortcuts on the desktop: Faber Dev, Faber Dev continuar, Faber Dev novidades');
}

function install() {
  mkdirSync(HOME, { recursive: true });
  copyFileSync(fileURLToPath(import.meta.url), SELF);
  update();
  promote();
  shortcuts();
  hub();
}

const [command = 'start', ...flags] = process.argv.slice(2);
try {
  if (command === 'install') install();
  else if (command === 'start') start({ keep: flags.includes('--keep') });
  else if (command === 'update') update({ force: flags.includes('--force'), quiet: flags.includes('--quiet') });
  else if (command === 'hub') hub();
  else if (command === 'shortcuts') shortcuts();
  else throw new Error(`unknown command ${command}`);
} catch (error) {
  say(error.message);
  process.exitCode = 1;
}
