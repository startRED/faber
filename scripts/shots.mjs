// The screenshots a visual critique looks at (R169, `.claude/skills/visual-critique/SKILL.md`). Runs the composition
// scenario of the UX sentinel, which photographs the main screens at 1920x1080 and 1280x800 in Dark Room, plus any
// scenario named with `--filter`, and copies the photos of the screens the diff touches into one folder with an index:
//
//   node scripts/shots.mjs before            (on the base commit, or before the first edit)
//   node scripts/shots.mjs after             (when the change is done)
//   node scripts/shots.mjs after --filter homes:
//
// The folder is `<target>/faber-test/critique/<label>/`. It runs hidden like every screen test (C006): the window lives on
// a desktop no monitor shows. Nothing here calls an AI: the critique is a separate session that reads the folder.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The screens the composition scenario photographs, by the name its photos start with. */
export const SCREENS = ['chat-home', 'project-home', 'conversation', 'settings'];

const ALL = 'all';
/** Which main screens a changed file can change. A file of the screen not named here may change any of them. */
const TOUCHES = [
  [/^ui\/(Welcome|Hole|Ambient|Ghost|Mark)\.tsx$/, ['chat-home', 'project-home']],
  [/^ui\/(Projects|ProjectSettings)\.tsx$/, ['project-home', 'settings']],
  [/^ui\/Composer\.tsx$/, ['chat-home', 'project-home', 'conversation']],
  [/^ui\/(Thread|Markdown|Steps)\.tsx$|^ui\/worklog\.ts$/, ['conversation']],
  [/^ui\/Settings[A-Za-z]*\.tsx$/, ['settings']],
  [/^ui\/|^src-tauri\/src\/themes\.json$|^index\.html$/, [ALL]],
];

/** The main screens a list of changed files can change, in the scenario's order. */
export function affected(files) {
  const hit = new Set();
  for (const file of files.map((f) => f.replaceAll('\\', '/'))) {
    const rule = TOUCHES.find(([pattern]) => pattern.test(file));
    if (rule) for (const screen of rule[1]) hit.add(screen);
  }
  return hit.has(ALL) ? [...SCREENS] : SCREENS.filter((s) => hit.has(s));
}

/** The screens to keep: those the diff touches, or all four when it touches none (a `before` on a clean checkout). */
export function keep(files) {
  const touched = affected(files);
  return touched.length ? touched : [...SCREENS];
}

/** Width and height of a PNG, from its header. */
export function pngSize(bytes) {
  const signature = [0x89, 0x50, 0x4e, 0x47];
  if (bytes.length < 24 || signature.some((b, i) => bytes[i] !== b)) return null;
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

/** The photos to keep: those taken since `since` (ms), of an affected main screen or of a scenario asked for by name. */
export function pick(photos, since, screens, extra) {
  return photos
    .filter((p) => p.modified >= since && p.name.endsWith('.png'))
    .filter((p) => {
      const main = p.name.match(/^critique-(.+)-\d+x\d+\.png$/);
      return main ? screens.includes(main[1]) : extra;
    })
    .map((p) => p.name)
    .sort();
}

/** The index a critique starts from: what was photographed, at what size, and what the diff touched. */
export function index(label, screens, files) {
  const lines = [`# Screenshots: ${label}`, '', `Screens the diff touches: ${screens.length ? screens.join(', ') : 'none'}.`, ''];
  lines.push('| Photo | Size |', '| :--- | :--- |');
  for (const [name, size] of files) lines.push(`| ${name} | ${size ? `${size.width}x${size.height}` : '?'} |`);
  lines.push('', 'Critique them with `.claude/skills/visual-critique/SKILL.md`, in a fresh session that did not write the change.');
  return `${lines.join('\n')}\n`;
}

/** The paths of `git status --porcelain`: two status letters, a space, the path (the new one of a rename). */
export function statusPaths(text) {
  return text
    .split('\n')
    .filter((line) => line.length > 3)
    .map((line) => line.slice(3).split(' -> ').pop());
}

/** A label names one folder inside the critique folder, nothing else. */
export const LABEL = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;

function git(root, args) {
  const out = spawnSync('git', ['-c', 'core.quotepath=false', ...args], { cwd: root, encoding: 'utf8', windowsHide: true });
  return out.status === 0 ? out.stdout : '';
}

function changed(root) {
  const base = git(root, ['merge-base', 'HEAD', 'origin/phase-1-base']).trim();
  const committed = base ? git(root, ['diff', '--name-only', base, 'HEAD']).split('\n').filter(Boolean) : [];
  return [...new Set([...committed, ...statusPaths(git(root, ['status', '--porcelain', '-uall']))])];
}

function main(argv) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const label = argv[0];
  if (!label || !LABEL.test(label)) {
    console.error('usage: node scripts/shots.mjs <label> [--filter <scenario text>] [--every-screen]');
    return 2;
  }
  const extra = argv.includes('--filter') ? argv[argv.indexOf('--filter') + 1] : undefined;
  const screens = argv.includes('--every-screen') ? [...SCREENS] : keep(changed(root));
  const configured = process.env.CARGO_TARGET_DIR;
  const target = join(configured ? (isAbsolute(configured) ? configured : join(root, configured)) : join(root, 'target'), 'faber-test');
  const since = Date.now();
  let status = 0;
  for (const filter of ['sentinel: the main screens', extra].filter(Boolean)) {
    const run = spawnSync('cargo', ['t', '--suite', 'screen', '--filter', filter], { cwd: root, stdio: 'inherit', windowsHide: true });
    // A failed scenario still leaves its photos, and a failed one is what most needs a look.
    if (run.status !== 0) status = run.status ?? 1;
  }
  const shots = join(target, 'shots');
  const photos = existsSync(shots) ? readdirSync(shots).map((name) => ({ name, modified: statSync(join(shots, name)).mtimeMs })) : [];
  const kept = pick(photos, since, screens, Boolean(extra));
  const folder = join(target, 'critique', label);
  rmSync(folder, { recursive: true, force: true });
  mkdirSync(folder, { recursive: true });
  const sizes = kept.map((name) => {
    copyFileSync(join(shots, name), join(folder, name));
    return [name, pngSize(readFileSync(join(folder, name)))];
  });
  writeFileSync(join(folder, 'index.md'), index(label, screens, sizes));
  console.log(`${kept.length} photos in ${folder}`);
  return status;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
