import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { lstat, mkdir, readdir, readFile, realpath, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const languages = { en: 'en', 'pt-BR': 'ptBR', es: 'es', fr: 'fr', de: 'de' };
const files = ['index.tsx', 'frame.tsx', 'scene.mjs', 'scene.d.mts', 'faber.css', 'viewer.ts', 'viewer.css'];
const notices = { 'stageshow-MIT.txt': 'LICENSE', 'stageshow-NOTICE.md': 'NOTICE.md', 'react-MIT.txt': 'node_modules/react/LICENSE', 'react-dom-MIT.txt': 'node_modules/react-dom/LICENSE', 'lucide.txt': 'node_modules/lucide-react/LICENSE', 'vite.txt': 'node_modules/vite/LICENSE.md' };
const texts = ['title', 'description', 'synthetic', 'project', 'request', 'reading', 'building', 'checking', 'done', 'result', 'resultDetail', 'projects', 'preview', 'conversation', 'replay', 'pause', 'resume', 'error', 'slow', 'step'];

export function options(args) {
  const result = { language: 'en', port: 4187, mode: 'dev' };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--lang' && Object.hasOwn(languages, args[i + 1])) result.language = args[++i];
    else if (arg === '--port' && /^\d{4,5}$/.test(args[i + 1] ?? '') && Number(args[i + 1]) >= 1024 && Number(args[i + 1]) <= 65535) result.port = Number(args[++i]);
    else if ((arg === '--prepare' || arg === '--build') && result.mode === 'dev') result.mode = arg.slice(2);
    else throw new Error(`Unknown or invalid demo argument: ${arg}`);
  }
  return result;
}

export function sourcePin(catalog) {
  const pin = catalog.skills?.find((skill) => skill.id === 'stageshow');
  if (pin?.repo !== 'benjitaylor/skills' || pin.path !== 'stageshow' || !/^[a-f0-9]{40}$/.test(pin.commit) || !/^[a-f0-9]{64}$/.test(pin.digest)) throw new Error('The Stageshow catalog pin is missing or invalid');
  return pin;
}

export function digest(entries) {
  const hash = createHash('sha256');
  for (const [name, bytes] of [...entries].sort(([a], [b]) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) hash.update(name).update('\0').update(String(bytes.length)).update('\0').update(bytes);
  return hash.digest('hex');
}

export function localized(messages, language) {
  const content = { language, brand: messages['act.faber'] };
  for (const key of texts) {
    const text = messages[`demo.faber.${key}`];
    if (typeof text !== 'string' || !text.trim()) throw new Error(`Missing demo text: demo.faber.${key}`);
    content[key] = text;
  }
  return content;
}

async function plain(directory) {
  const resolved = path.resolve(directory);
  const parent = path.dirname(resolved);
  if (parent !== resolved) await plain(parent);
  const info = await lstat(resolved);
  if (!info.isDirectory() || info.isSymbolicLink()) throw new Error(`Demo cache cannot contain linked directories: ${resolved}`);
  return resolved;
}

export async function ensureDirectory(folder) {
  await plain(path.dirname(folder));
  await mkdir(folder, { recursive: true });
  await plain(folder);
}

export async function writeGenerated(filename, bytes) {
  await plain(path.dirname(filename));
  try {
    const info = await lstat(filename);
    if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1) throw new Error(`Linked or unsafe demo destination: ${filename}`);
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  const temporary = path.join(path.dirname(filename), `.${path.basename(filename)}.${randomUUID()}.tmp`);
  try {
    await writeFile(temporary, bytes, { flag: 'wx' });
    await rename(temporary, filename);
  } finally { await rm(temporary, { force: true }); }
}

export async function validateCache(checkout, tracked) {
  const allowed = new Set([...tracked, ...files.map((file) => `stageshow/examples/faber/${file}`), ...['content.json', 'assets.json', 'tokens.css', 'mark.svg', 'archivo.woff2'].map((file) => `stageshow/examples/faber/${file}`), ...[...Object.keys(notices), 'archivo-OFL.txt', 'faber.txt'].map((file) => `stageshow/demo-public/notices/${file}`), 'stageshow/playground/faber.html', 'stageshow/playground/faber-entry.ts', 'stageshow/playground/faber.config.mjs']);
  const generatedDirectories = new Set(['.git', 'stageshow/node_modules']);
  const outputs = new Set(['stageshow/audit', 'stageshow/dist-faber']);
  let remaining = 20000;
  async function walk(relative = '', output = false) {
    for (const entry of await readdir(path.join(checkout, relative), { withFileTypes: true })) {
      if (--remaining < 0) throw new Error('Demo source cache has too many paths');
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isSymbolicLink()) throw new Error(`Linked demo source entry: ${name}`);
      if (entry.isDirectory() && generatedDirectories.has(name)) continue;
      if (entry.isFile() && (await lstat(path.join(checkout, name))).nlink !== 1) throw new Error(`Hard-linked demo source entry: ${name}`);
      if (entry.isDirectory() && (output || outputs.has(name))) { await walk(name, true); continue; }
      if (output && entry.isFile()) continue;
      if (entry.isFile() && allowed.has(name)) continue;
      if (entry.isDirectory() && [...allowed].some((file) => file.startsWith(`${name}/`))) await walk(name);
      else throw new Error(`Unexpected demo source entry: ${name}`);
    }
  }
  await walk();
}

export function command(command, args, cwd, capture = false) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, shell: false, windowsHide: true, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit' });
    let output = '';
    if (capture) child.stdout.setEncoding('utf8').on('data', (chunk) => { output += chunk; });
    const stop = () => child.kill();
    process.once('SIGINT', stop);
    process.once('SIGTERM', stop);
    const cleanup = () => { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); };
    child.once('error', (error) => { cleanup(); reject(error); });
    child.once('close', (code, signal) => {
      cleanup();
      if (code !== 0) reject(new Error(`${path.basename(command)} failed (${code ?? signal})`));
      else resolve(output.trim());
    });
  });
}

async function npmCli() {
  const candidates = [process.env.npm_execpath, path.join(path.dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js'), path.resolve(path.dirname(process.execPath), '../lib/node_modules/npm/bin/npm-cli.js')];
  for (const candidate of candidates) {
    if (!candidate || !candidate.endsWith('npm-cli.js')) continue;
    try { if ((await lstat(candidate)).isFile()) return await realpath(candidate); } catch { /* Try the next installed npm location. */ }
  }
  throw new Error('npm-cli.js was not found. Run this command through npm run demo with Node.js 24 or newer.');
}

export async function verifiedSource(pin, run = command) {
  const cache = path.join(root, 'target', 'stageshow');
  await ensureDirectory(path.join(root, 'target'));
  await ensureDirectory(cache);
  const checkout = path.join(cache, pin.commit);
  let exists = false;
  try { await lstat(checkout); exists = true; } catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (!exists) {
    await ensureDirectory(checkout);
    await run('git', ['init', '--quiet', checkout], root);
    await run('git', ['-C', checkout, 'config', 'core.autocrlf', 'false'], root);
    await run('git', ['-C', checkout, 'remote', 'add', 'origin', `https://github.com/${pin.repo}.git`], root);
    await run('git', ['-C', checkout, 'fetch', '--depth=1', 'origin', pin.commit], root);
    await run('git', ['-C', checkout, '-c', 'core.hooksPath=/dev/null', 'checkout', '--quiet', '--detach', pin.commit], root);
  }
  await plain(checkout);
  let revision;
  try { revision = await run('git', ['-C', checkout, 'rev-parse', 'HEAD'], root, true); }
  catch (cause) { throw new Error('Demo cache was not prepared completely; move target/stageshow aside and run again', { cause }); }
  if (revision !== pin.commit) throw new Error('Demo cache has the wrong source revision; move target/stageshow aside and run again');
  const directory = path.join(checkout, pin.path);
  await plain(directory);
  const tracked = (await run('git', ['-C', checkout, 'ls-tree', '-rz', pin.commit], root, true)).split('\0').filter(Boolean).map((line) => {
    const match = line.match(/^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/s);
    if (!match || match[3].split('/').some((piece) => piece === '..' || piece === '.')) throw new Error('Unexpected pinned source path');
    return { full: match[3], hash: match[2] };
  });
  await validateCache(checkout, tracked.map((entry) => entry.full));
  const entries = [];
  for (const { full, hash } of tracked) {
    const filename = path.join(checkout, full);
    await plain(path.dirname(filename));
    const info = await lstat(filename);
    if (!info.isFile() || info.isSymbolicLink()) throw new Error(`Unsafe pinned source file: ${full}`);
    const bytes = await readFile(filename);
    if (createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') !== hash) throw new Error(`Pinned source was modified: ${full}`);
    if (full.startsWith(`${pin.path}/`)) entries.push([full.slice(pin.path.length + 1), bytes]);
  }
  if (digest(entries) !== pin.digest) throw new Error('Stageshow source differs from the verified catalog pin; move target/stageshow aside and run again');
  return directory;
}

export async function prepareDemo(settings, run = command) {
  const pin = sourcePin(JSON.parse(await readFile(path.join(root, 'engine-projects/src/tools.json'), 'utf8')));
  const upstream = await verifiedSource(pin, run);
  const npm = await npmCli();
  // Recheck the lockfile on every run, then let npm enforce its integrity pins.
  await run(process.execPath, [npm, 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], upstream);
  const demo = path.join(upstream, 'examples/faber');
  await ensureDirectory(demo);
  // Refuse unexpected local entries before replacing this generated demo's known files.
  const generated = new Set([...files, 'content.json', 'assets.json', 'tokens.css', 'mark.svg', 'archivo.woff2']);
  for (const item of await readdir(demo, { withFileTypes: true })) if (!generated.has(item.name) || !item.isFile() || item.isSymbolicLink()) throw new Error(`Unexpected demo cache entry: ${item.name}`);
  for (const filename of files) await writeGenerated(path.join(demo, filename), await readFile(path.join(root, 'demos/faber', filename)));
  const module = await import(pathToFileURL(path.join(root, 'ui/locales', `${settings.language}.ts`)).href);
  await writeGenerated(path.join(demo, 'content.json'), `${JSON.stringify(localized(module[languages[settings.language]], settings.language), null, 2)}\n`);
  const theme = JSON.parse(await readFile(path.join(root, 'src-tauri/src/themes.json'), 'utf8')).css.base;
  await writeGenerated(path.join(demo, 'tokens.css'), `:root, .faber-demo {${Object.entries(theme).map(([key, value]) => `--${key}:${value};`).join('')}}\n`);
  const glyphs = await readFile(path.join(root, 'ui/Glyphs.tsx'), 'utf8');
  const icons = {};
  for (const name of ['folder', 'panel', 'up', 'check']) {
    const line = glyphs.match(new RegExp(`\\b${name}: \\[([^\\]]+)\\]`));
    if (!line) throw new Error(`Faber icon is missing: ${name}`);
    icons[name] = [...line[1].matchAll(/'([^']+)'/g)].map((match) => match[1]);
  }
  await writeGenerated(path.join(demo, 'assets.json'), `${JSON.stringify(icons)}\n`);
  await writeGenerated(path.join(demo, 'mark.svg'), (await readFile(path.join(root, 'docs/brand/mark.svg'), 'utf8')).replace('currentColor', theme.ink));
  const fonts = path.join(root, 'node_modules/@fontsource-variable/archivo/files');
  const font = (await readdir(fonts)).find((name) => name === 'archivo-latin-wdth-normal.woff2');
  if (!font) throw new Error('Faber fonts are missing; run npm ci in Faber before preparing the demo');
  await writeGenerated(path.join(demo, 'archivo.woff2'), await readFile(path.join(fonts, font)));
  const publicFiles = path.join(upstream, 'demo-public');
  await ensureDirectory(publicFiles);
  const noticeDirectory = path.join(publicFiles, 'notices');
  await ensureDirectory(noticeDirectory);
  for (const [name, from] of Object.entries(notices)) await writeGenerated(path.join(noticeDirectory, name), await readFile(path.join(upstream, from)));
  await writeGenerated(path.join(noticeDirectory, 'archivo-OFL.txt'), await readFile(path.join(fonts, '../LICENSE')));
  await writeGenerated(path.join(noticeDirectory, 'faber.txt'), await readFile(path.join(root, 'LICENSE')));
  for (const [from, to] of [['faber.html', 'faber.html'], ['entry.ts', 'faber-entry.ts'], ['vite.config.mjs', 'faber.config.mjs']]) {
    const destination = path.join(upstream, 'playground', to);
    await writeGenerated(destination, await readFile(path.join(root, 'demos/faber', from)));
  }
  return upstream;
}

async function main() {
  const settings = options(process.argv.slice(2));
  const upstream = await prepareDemo(settings);
  if (settings.mode === 'prepare') { console.log(`Demo prepared in ${upstream}`); return; }
  const vite = path.join(upstream, 'node_modules/vite/bin/vite.js');
  if (settings.mode === 'build') {
    await command(process.execPath, [vite, 'build', 'playground', '--config', 'playground/faber.config.mjs'], upstream);
    await writeGenerated(path.join(upstream, 'dist-faber/README.md'), await readFile(path.join(root, 'demos/faber/EXPORT.md')));
  }
  else {
    console.log(`Faber demo: http://127.0.0.1:${settings.port}/faber.html`);
    await command(process.execPath, [vite, 'playground', '--config', 'playground/faber.config.mjs', '--port', String(settings.port)], upstream);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch((error) => { console.error(`Faber demo: ${error.message}`); process.exitCode = 1; });
