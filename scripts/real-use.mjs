// What went wrong in real use, read from the conversations Faber saved on this PC. It spends no token, starts no CLI and
// writes nothing: the files are only read. On 2026-11-11 the owner saw three team starts refused, two parts running on
// the everyday model and a helper stopped two minutes in; the saved conversations showed the same patterns for six days
// (ten of eleven Auto starts never judged, 80 failed steps of 2,345) and no check had read them.
//
//   node scripts/real-use.mjs                 the last 7 days of the installed Faber's own data folder
//   node scripts/real-use.mjs --days 2        another span
//   node scripts/real-use.mjs --data <folder> another data folder (the one that holds `conversations/`)
//   node scripts/real-use.mjs --check         exits 1 when a pattern below is over its limit
//
// Only Faber's own sentences are printed (why it refused a call, why a helper stopped), with ids masked; a step of any
// other tool is counted by the tool's name. Nothing a person or a model wrote is shown.

import { readdirSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

/** The first assistant reasons that mean nobody judged the request. */
const UNJUDGED = new Set(['reason.autoImmediate', 'reason.autoDefault', 'reason.autoOnly']);
/** How Faber's own refusals and stops begin; the rest of a failed step's output may hold a person's text. */
const OWN = /^(No conversation was started|Nothing was done|Nothing to read|The helper stopped|No helper was started|error\.[a-z.]+)\b/i;
/** Sentences that were bugs, fixed on the date beside each: seeing one again after that date is a regression. */
export const FIXED = [
  { text: 'model and effort cannot be used with auto', since: Date.UTC(2026, 10, 12) },
];
/** Above this share of Auto starts in projects with no judgement, with at least `MIN_STARTS` of them, `--check` fails. */
export const UNJUDGED_LIMIT = 0.5;
export const MIN_STARTS = 4;

const ms = (at) => (typeof at === 'number' ? (at > 1e12 ? at : at * 1000) : Date.parse(at) || 0);
const sentence = (output) => String(output ?? '').split('\n')[0].replace(/\b[0-9a-f]{8,}\b|\b[A-Za-z0-9_-]{20,}\b/g, '<id>').slice(0, 140);

/** Counts over `conversations` (parsed files) changed at or after `since` (ms). */
export function summarize(conversations, since = 0) {
  const sum = { conversations: 0, replies: 0, steps: 0, failed: 0, starts: { judged: 0, unjudged: 0, project: 0, projectUnjudged: 0, byLead: 0, byLeadUnjudged: 0 }, own: new Map(), tools: new Map(), regressions: [] };
  for (const c of conversations) {
    if (ms(c.updatedAt) < since) continue;
    sum.conversations += 1;
    const first = (c.messages ?? []).find((m) => m.role === 'assistant');
    if (c.choice?.vendor === 'auto' && first?.reason && first.reason !== 'reason.routing') {
      const unjudged = UNJUDGED.has(first.reason);
      sum.starts[unjudged ? 'unjudged' : 'judged'] += 1;
      if (c.area === 'project') {
        sum.starts.project += 1;
        if (unjudged) sum.starts.projectUnjudged += 1;
      }
      if (c.startedBy) {
        sum.starts.byLead += 1;
        if (unjudged) sum.starts.byLeadUnjudged += 1;
      }
    }
    for (const m of c.messages ?? []) {
      if (m.role !== 'assistant') continue;
      sum.replies += 1;
      for (const tool of m.tools ?? []) {
        sum.steps += 1;
        if (tool.ok !== false) continue;
        sum.failed += 1;
        const text = sentence(tool.output);
        if (OWN.test(text)) {
          sum.own.set(text, (sum.own.get(text) ?? 0) + 1);
          const at = ms(tool.ended ?? tool.started ?? m.at);
          for (const fixed of FIXED) if (text.includes(fixed.text) && at >= fixed.since) sum.regressions.push(text);
        } else sum.tools.set(tool.name, (sum.tools.get(tool.name) ?? 0) + 1);
      }
    }
  }
  return sum;
}

/** What `--check` refuses, one line each; empty when real use is clean. */
export function problems(sum) {
  const found = [];
  const { project, projectUnjudged } = sum.starts;
  if (project >= MIN_STARTS && projectUnjudged / project > UNJUDGED_LIMIT) {
    found.push(`${projectUnjudged} of ${project} Auto starts in projects were never judged (limit ${UNJUDGED_LIMIT * 100}%)`);
  }
  for (const text of new Set(sum.regressions)) found.push(`a refusal that was fixed came back: ${text}`);
  return found;
}

/** The report as lines of text. */
export function report(sum, days) {
  const top = (map, n) => [...map].sort((a, b) => b[1] - a[1]).slice(0, n).map(([text, count]) => `  ${String(count).padStart(4)}  ${text}`);
  const s = sum.starts;
  return [
    `Real use, last ${days} days: ${sum.conversations} conversations, ${sum.replies} replies, ${sum.steps} steps, ${sum.failed} failed`,
    `Auto starts: ${s.judged} judged, ${s.unjudged} not judged (projects ${s.projectUnjudged} of ${s.project}; started by a lead ${s.byLeadUnjudged} of ${s.byLead})`,
    'Faber refused or stopped:',
    ...(sum.own.size ? top(sum.own, 20) : ['  nothing']),
    'Failed steps of other tools:',
    ...(sum.tools.size ? top(sum.tools, 12) : ['  nothing']),
  ];
}

/** The parsed conversations of a data folder; a file that does not parse is skipped. */
export function load(data) {
  const folder = join(data, 'conversations');
  const found = [];
  for (const name of readdirSync(folder)) {
    if (!name.endsWith('.json')) continue;
    try {
      found.push(JSON.parse(readFileSync(join(folder, name), 'utf8')));
    } catch {
      // A file being written or damaged says nothing about how a turn went.
    }
  }
  return found;
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].replaceAll('\\', '/').split('/').at(-1))) {
  const args = process.argv.slice(2);
  const value = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);
  const days = Number(value('--days') ?? 7);
  const data = value('--data') ?? process.env.FABER_DATA_DIR ?? join(process.env.APPDATA ?? join(homedir(), '.config'), 'Faber');
  let conversations;
  try {
    conversations = load(data);
  } catch {
    console.log(`No conversations in ${data}: nothing to read.`);
    process.exit(0);
  }
  const sum = summarize(conversations, Date.now() - days * 86_400_000);
  console.log(report(sum, days).join('\n'));
  const found = problems(sum);
  if (args.includes('--check') && found.length) {
    console.error(found.map((line) => `FAIL  ${line}`).join('\n'));
    process.exit(1);
  }
}
