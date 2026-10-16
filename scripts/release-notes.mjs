// Builds the body of a GitHub release from the English "what's new" entries (ui/locales/en.ts, `notes.<id>.<kind>`).
//
//   node scripts/release-notes.mjs [previous-tag]
//
// The entries newer than the highest one in the previous tag are listed by kind. With no previous tag, or one from before
// the entries existed, all of them are. Nothing new prints a plain sentence, never an empty body. The release workflow
// (.github/workflows/release.yml) passes the output to `gh release create --notes-file`.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const KINDS = [
  ['new', 'New'],
  ['better', 'Better'],
  ['fixed', 'Fixed'],
];
// One entry per line, in either quote style (en.ts mixes them): `'notes.1.new': 'text',` or `"notes.1.new": "text",`.
const ENTRY = /^\s*(['"])notes\.(\d+)\.(new|better|fixed)\1:\s*(['"])((?:(?!\4)[^\\\n]|\\.)*)\4,?\s*$/gm;

/** `[{ id, kind, text }]` from the source of en.ts. */
export function entries(source) {
  return [...source.matchAll(ENTRY)].map(([, , id, kind, , text]) => ({ id: Number(id), kind, text: text.replace(/\\(['"\\])/g, '$1') }));
}

/** The markdown body for the entries of `source` newer than the newest one in `previous` (source of an older en.ts). */
export function body(source, previous = '') {
  const seen = entries(previous).reduce((top, e) => Math.max(top, e.id), 0);
  const fresh = entries(source).filter((e) => e.id > seen);
  const parts = KINDS.flatMap(([kind, title]) => {
    const lines = fresh.filter((e) => e.kind === kind).map((e) => `- ${e.text}`);
    return lines.length ? [`### ${title}\n\n${lines.join('\n')}`] : [];
  });
  return parts.length ? `${parts.join('\n\n')}\n` : 'Small fixes and improvements.\n';
}

if (process.argv[1]?.endsWith('release-notes.mjs')) {
  const [tag] = process.argv.slice(2);
  let previous = '';
  if (tag) {
    try {
      previous = execFileSync('git', ['show', `${tag}:ui/locales/en.ts`], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
    } catch {
      // A tag from before the file existed has no entries: every entry is new.
    }
  }
  process.stdout.write(body(readFileSync(new URL('../ui/locales/en.ts', import.meta.url), 'utf8'), previous));
}
