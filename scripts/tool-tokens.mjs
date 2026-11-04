// The score for the words Faber adds to every CLI turn (task: cut the tokens, keep what works; the idea comes from T3 Code
// cutting its tool descriptions by ~78%). Every tool a session offers (description + input schema) and the persona are
// sent again with every message, so each token here costs the person's quota on every turn.
//
//   node scripts/tool-tokens.mjs [--baseline] [--json]
//
// It runs `engine/examples/tool_text.rs` and `persona_text.rs`, counts tokens with a small offline estimate (no tokenizer
// to install; a word costs about one token per 7 letters, a digit run per 3 digits, a mark one), and prints a table next to
// `scripts/tool-tokens/baseline.json`. `--baseline` rewrites that file from the current words. The estimate is a score for
// climbing (a cut that lowers it lowers any real tokenizer's count too), not a price.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BASELINE = join(ROOT, 'scripts', 'tool-tokens', 'baseline.json');

/** The estimated tokens of a text. */
export function tokens(text) {
  let total = 0;
  for (const [piece] of text.matchAll(/\p{L}+|\p{N}+|[^\s\p{L}\p{N}]/gu)) {
    if (/^\p{L}/u.test(piece)) total += Math.ceil(piece.length / 7);
    else if (/^\p{N}/u.test(piece)) total += Math.ceil(piece.length / 3);
    else total += 1;
  }
  return total;
}

/** Tokens per tool (`description` and `schema`) and per persona, from what the two examples print. */
export function measure(toolText, personaText) {
  const tools = {};
  for (const t of toolText.mcp) tools[t.name] = { description: tokens(t.description), schema: tokens(JSON.stringify(t.inputSchema)) };
  return { tools, persona: { claude: tokens(personaText.claude), codex: tokens(personaText.codex) } };
}

const sum = (m) => Object.values(m.tools).reduce((a, t) => a + t.description + t.schema, 0);

/** The table of tokens before and after, one row per tool and persona, with the total. */
export function table(before, after) {
  const rows = [['what', 'before', 'after', 'saved']];
  const row = (name, b, a) => rows.push([name, String(b), String(a), b ? `${Math.round(((b - a) / b) * 100)}%` : '-']);
  for (const name of Object.keys(after.tools)) {
    const b = before.tools[name] ?? { description: 0, schema: 0 };
    const a = after.tools[name];
    row(`${name} description`, b.description, a.description);
    row(`${name} schema`, b.schema, a.schema);
  }
  row('persona (Claude)', before.persona.claude, after.persona.claude);
  row('persona (Codex)', before.persona.codex, after.persona.codex);
  row('all tools', sum(before), sum(after));
  const width = rows[0].map((_, i) => Math.max(...rows.map((r) => r[i].length)));
  return rows.map((r) => r.map((c, i) => (i ? c.padStart(width[i]) : c.padEnd(width[i]))).join('  ')).join('\n');
}

function example(name) {
  const run = spawnSync(process.execPath, ['scripts/cargo.mjs', 'run', '-q', '-p', 'faber-engine', '--example', name], {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 1 << 26,
  });
  const line = run.stdout.split('\n').find((l) => l.startsWith('{'));
  if (run.status !== 0 || !line) throw new Error(`could not print the words (engine/examples/${name}.rs)`);
  return JSON.parse(line);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const now = measure(example('tool_text'), example('persona_text'));
  if (process.argv.includes('--baseline')) {
    writeFileSync(BASELINE, `${JSON.stringify(now, null, 2)}\n`);
    console.log(`baseline written: ${sum(now)} tokens in tools`);
  } else {
    const before = JSON.parse(readFileSync(BASELINE, 'utf8'));
    console.log(process.argv.includes('--json') ? JSON.stringify(now) : table(before, now));
  }
}
