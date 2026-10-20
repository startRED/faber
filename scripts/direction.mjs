// The direction check (docs/dev/workflow.md#direction-check): does the work still head where docs/product says?
//
//   node scripts/direction.mjs            asks a fresh read-only Codex session, writes the report and the proposals
//   node scripts/direction.mjs --prompt   prints the question it would ask and runs nothing
//   node scripts/direction.mjs --from <answer.json>   writes the report and proposals from a saved answer
//
// It hands Codex the product pages (docs/product/), the product map with its known incoherences, the decisions and the
// task queue, and asks three things: which goals have no feature yet, which features drift from their goal, and which
// queued tasks no longer serve a goal. The gaps come back as proposed tasks in the docs/tasks format, written to
// docs/tasks/proposed/ (ignored by git), each marked as a proposal. Nothing is queued: the owner or the lead session
// picks a proposal by giving it an order and moving it to docs/tasks/.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parseName, parseTask, TASKS } from './tasks.mjs';

export const PROPOSED = `${TASKS}/proposed`;
export const MODEL = 'gpt-6.1-sol';
/** The first line under a proposal's title; also how a later run knows which files are its own to replace. */
export const MARK = 'Proposal from the direction check';
const ROOT = resolve(import.meta.dirname, '..');

/** The pages the check reads, relative to the repository: the product pages first, then the map and the decisions. */
export function pages(root = ROOT) {
  const product = readdirSync(join(root, 'docs/product'))
    .filter((f) => f.endsWith('.md'))
    .sort((a, b) => (a === 'README.md' ? -1 : b === 'README.md' ? 1 : a.localeCompare(b)))
    .map((f) => `docs/product/${f}`);
  return [...product, 'docs/product-map.md', 'docs/decisions.md'];
}

/** The queued tasks in order, `{ file, title }`, read from the working tree. */
export function tasks(root = ROOT) {
  return readdirSync(join(root, TASKS))
    .map((file) => ({ file, task: parseName(file) }))
    .filter((t) => t.task)
    .sort((a, b) => a.task.order - b.task.order)
    .map(({ file }) => ({ file: `${TASKS}/${file}`, title: parseTask(readFileSync(join(root, TASKS, file), 'utf8')).title }));
}

/** The question for Codex. It reads the pages itself (read-only), so the prompt names them instead of pasting them. */
export function buildPrompt(pageList, taskList, today) {
  const queueLines = taskList.length ? taskList.map((t) => `- ${t.file}: ${t.title}`).join('\n') : '- (the queue is empty)';
  return `You are checking whether the work on Faber still heads where its product pages say. Today is ${today}.
Do not edit any file. Read AGENTS.md first, then these pages in full:

${pageList.map((p) => `- ${p}`).join('\n')}

The task queue (each file is a task, lowest number first; read each one):

${queueLines}

docs/product/ says what Faber should be (scope and goals). docs/product-map.md says what exists today, and its "Known
incoherences" section lists known gaps. docs/decisions.md is the owner's decision ledger: a later row overrides an
earlier one. Dated specs, plans and research (docs/superpowers/, docs/research/) are history, never a source for how
Faber works today. You may open code to confirm whether a feature exists, but keep to what the pages need.

Answer three questions:
1. Which goals in docs/product have no feature yet (nothing in the product map or the code does it)?
2. Which features drift from their goal (they exist, but work against or beside what docs/product or a decision says)?
3. Which tasks in the queue no longer serve a goal (done already, overridden by a later decision, or aimed nowhere)?

Rules:
- Every finding names the page and heading (or decision number) it rests on. No source, no finding.
- Skip what the queue or the "Known incoherences" already cover, unless it is wrong there.
- Plain words, short: one or two sentences per finding, at most 5 findings per question, the ones that matter most for
  the goals first. A later-phase goal the pages already call later is not a finding. No praise, no padding.
- Turn each gap worth doing into a proposed task: a short lowercase name with dashes, a title, and the brief a fresh
  session needs (the task, the files or area, what done looks like), in the style of the files in docs/tasks/. At most 8,
  most valuable first. Propose dropping a task only as a finding under question 3, not as a proposal.
- List under could_not_judge what you could not decide from the pages and why (a goal too vague to check, a feature
  you could not find either way, a page you could not read). Say so rather than guess.`;
}

/** The answer's shape, given to Codex as --output-schema (strict: every field required, nothing extra). */
export const SCHEMA = (() => {
  const obj = (props) => ({ type: 'object', additionalProperties: false, required: Object.keys(props), properties: props });
  const str = { type: 'string' };
  const list = (props) => ({ type: 'array', items: obj(props) });
  return obj({
    goals_without_feature: list({ goal: str, source: str, why: str }),
    drifting_features: list({ feature: str, source: str, why: str }),
    tasks_off_goal: list({ task: str, why: str }),
    proposals: list({ name: str, title: str, brief: str }),
    could_not_judge: { type: 'array', items: str },
  });
})();

/** The answer, checked: a missing list is empty, and anything that is not the expected shape is an error. */
export function parseAnswer(text) {
  const raw = text.trim().replace(/^```(?:json)?\s*|\s*```$/g, '');
  const answer = JSON.parse(raw);
  for (const key of Object.keys(SCHEMA.properties)) {
    answer[key] ??= [];
    if (!Array.isArray(answer[key])) throw new Error(`the answer's ${key} is not a list`);
  }
  return answer;
}

/** A proposal's file name: lowercase letters, digits and dashes only, so a name can never leave the folder. */
export function slug(name) {
  const s = String(name).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60).replace(/-+$/, '');
  return s || 'proposal';
}

/** A proposal in the docs/tasks format, marked so nobody mistakes it for a queued task. */
export function renderProposal(p, today) {
  return `# ${p.title.trim()}\n\n${MARK} of ${today}: not in the queue. To take it, give it an order and move it to\n${TASKS}/ (docs/dev/workflow.md#direction-check).\n\n${p.brief.trim()}\n`;
}

/** The short report: the three answers, the proposals and what could not be judged. */
export function renderReport(a, today, files) {
  const section = (title, items, line) => `## ${title}\n\n${items.length ? items.map(line).join('\n') : 'Nothing found.'}\n`;
  return [
    `# Direction check, ${today}\n\nAsked a fresh read-only Codex session (${MODEL}). These are proposals: nothing was queued.\n`,
    section('Goals with no feature yet', a.goals_without_feature, (g) => `- ${g.goal}: ${g.why} (${g.source})`),
    section('Features that drift from their goal', a.drifting_features, (f) => `- ${f.feature}: ${f.why} (${f.source})`),
    section('Queued tasks that no longer serve a goal', a.tasks_off_goal, (t) => `- ${t.task}: ${t.why}`),
    section('Proposed tasks', files, (f) => `- ${f.title}: \`${f.path}\``),
    section('What it could not judge', a.could_not_judge, (c) => `- ${c}`),
  ].join('\n');
}

/** Writes the report and the proposals; the previous run's proposals go first, the person's own drafts stay. */
export function write(answer, today, root = ROOT) {
  const dir = join(root, PROPOSED);
  mkdirSync(dir, { recursive: true });
  for (const f of readdirSync(dir)) {
    if (f.endsWith('.md') && readFileSync(join(dir, f), 'utf8').includes(MARK)) unlinkSync(join(dir, f));
  }
  const files = [];
  for (const p of answer.proposals) {
    let name = slug(p.name || p.title);
    for (let n = 2; files.some((f) => f.name === name) || existsSync(join(dir, `${name}.md`)); n++) name = `${slug(p.name || p.title)}-${n}`;
    writeFileSync(join(dir, `${name}.md`), renderProposal(p, today));
    files.push({ name, title: p.title.trim(), path: `${PROPOSED}/${name}.md` });
  }
  const report = renderReport(answer, today, files);
  writeFileSync(join(dir, 'REPORT.md'), report);
  return report;
}

/** One fresh, read-only Codex session that keeps nothing; its last message is the answer. */
function ask(prompt) {
  const tmp = mkdtempSync(join(tmpdir(), 'faber-direction-'));
  try {
    const schema = join(tmp, 'schema.json');
    const out = join(tmp, 'answer.json');
    writeFileSync(schema, JSON.stringify(SCHEMA));
    // No shell: codex is a real executable; the prompt goes through stdin.
    execFileSync(
      'codex',
      ['exec', '-s', 'read-only', '-m', MODEL, '-c', 'model_reasoning_effort="high"', '--ephemeral', '--cd', ROOT, '--output-schema', schema, '-o', out, '-'],
      { input: prompt, stdio: ['pipe', 'ignore', 'inherit'], windowsHide: true, maxBuffer: 64 * 1024 * 1024 },
    );
    return readFileSync(out, 'utf8');
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'direction.mjs'))) {
  const args = process.argv.slice(2);
  const today = new Date().toISOString().slice(0, 10);
  const prompt = buildPrompt(pages(), tasks(), today);
  if (args[0] === '--prompt') {
    console.log(prompt);
  } else if (args[0] === '--from' && args[1]) {
    console.log(write(parseAnswer(readFileSync(args[1], 'utf8')), today));
  } else if (args.length) {
    console.error('usage: node scripts/direction.mjs [--prompt | --from <answer.json>]');
    process.exitCode = 2;
  } else {
    let text;
    try {
      text = ask(prompt);
    } catch (e) {
      console.error(`The direction check did not run: Codex failed or is not installed or signed in (${e.message.split('\n')[0]}).`);
      process.exitCode = 1;
    }
    if (text !== undefined) console.log(write(parseAnswer(text), today));
  }
}
