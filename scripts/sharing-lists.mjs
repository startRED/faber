// Regenerates the fixed, public field values used by the sharing report.
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
writeFileSync('engine/src/sharing-errors.txt', [...errors].sort().join('\n') + '\n');
writeFileSync('engine/src/sharing-sources.txt', [...sources].sort().join('\n') + '\n');
