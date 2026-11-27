// Regenerates the fixed, public field values used by the sharing report; `--check` only compares them.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const roots = ['builds', 'channel', 'cli', 'engine', 'engine-core', 'engine-drivers', 'engine-projects', 'engine-store', 'host', 'sidecar', 'src-tauri', 'update', 'winproc'];
const errors = new Set();
const sources = new Set();
for (const line of readFileSync('ui/locales/en.ts', 'utf8').split(/\r?\n/)) {
  const match = line.match(/^\s*(['"])((?:error|reason)\.[^'"]+)\1\s*:/);
  if (match && match[2].length <= 80) errors.add(match[2]);
}
function visit(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const file = join(dir, entry.name);
    if (entry.isDirectory()) visit(file);
    else if (entry.name.endsWith('.rs')) {
      if (entry.name.length <= 64) sources.add(entry.name);
      for (const match of readFileSync(file, 'utf8').matchAll(/"((?:error|reason)\.[A-Za-z0-9_.]+)"/g)) {
        if (match[1].length <= 80) errors.add(match[1]);
      }
    }
  }
}
for (const root of roots) visit(join(root, 'src'));
const lists = { 'engine/src/sharing-errors.txt': errors, 'engine/src/sharing-sources.txt': sources };
// `--check` writes nothing and fails on a stale list, so shipping catches what the engine's list tests fail on in CI.
if (process.argv.includes('--check')) {
  let stale = false;
  for (const [file, names] of Object.entries(lists)) {
    const kept = new Set(readFileSync(file, 'utf8').split(/\r?\n/).filter(Boolean));
    const missing = [...names].filter((name) => !kept.has(name)).sort();
    const extra = [...kept].filter((name) => !names.has(name)).sort();
    if (missing.length || extra.length) {
      stale = true;
      console.error(`${file} is stale (missing: ${missing.join(', ') || 'none'}; extra: ${extra.join(', ') || 'none'}); run \`node scripts/sharing-lists.mjs\`.`);
    }
  }
  process.exit(stale ? 1 : 0);
}
for (const [file, names] of Object.entries(lists)) writeFileSync(file, [...names].sort().join('\n') + '\n');
