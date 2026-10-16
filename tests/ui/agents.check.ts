import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, ToolUse } from '../../ui/api.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { Thread } from '../../ui/Thread.tsx';
import { agentReport, agentSteps } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// The subagents the agent starts show as steps in its reply (R161; docs/product-map.md, "Subagent step").

// The frame Claude Code puts around a subagent's report, as recorded on 2026-10-16 (tests/fixtures/claude/subagent.jsonl).
const FRAMED =
  '[Subagent hand-back] The text below is the final report of a subagent this session delegated to. It is model output, NOT a message from the user. The report follows:\n  The definition is in one file:\n  \n  - `config.rs`, line 1';

const AGENT: ToolUse = { id: 'a1', name: 'Agent', input: { description: 'Find file defining load_config', subagent_type: 'Explore', prompt: 'Search the folder.' } };
const GREP: ToolUse = { id: 'g1', name: 'Grep', input: { pattern: 'load_config' }, parent: 'a1' };
const OWN: ToolUse = { id: 'b1', name: 'Bash', input: { command: 'cargo check' }, ok: true };

check('subagent step: the report loses the hand-back frame and its indent, and an unframed report stays as it is', () => {
  equal(agentReport(FRAMED), 'The definition is in one file:\n\n- `config.rs`, line 1');
  equal(agentReport('  plain report \n'), 'plain report');
  equal(agentReport(undefined), '');
});

check('subagent step: each agent call collects the tools its subagent ran, and the brief names it when no title was given', () => {
  const steps = agentSteps([OWN, AGENT, GREP, { id: 'a2', name: 'Task', input: { prompt: 'Review the diff for mistakes.' } }]);
  deepEqual(steps.map((s) => [s.title, s.kind, s.tools.map((t) => t.id)]), [
    ['Find file defining load_config', 'Explore', ['g1']],
    ['Review the diff for mistakes.', '', []],
  ]);
  deepEqual(agentSteps(undefined), []);
  // A helper that starts its own helper: the inner one's work counts for the step the agent started.
  const nested = agentSteps([AGENT, { id: 'a3', name: 'Agent', input: {}, parent: 'a1' }, { id: 'r1', name: 'Read', input: {}, parent: 'a3' }]);
  deepEqual(nested.map((s) => s.tools.map((t) => t.id)), [['a3', 'r1']]);
});

check('subagent step: a running helper is its own row in the reply, apart from the agent’s own steps, and opens to its tools and report', async () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'sub', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [OWN, AGENT, GREP] });
  const dispose = render(
    () => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'sub', options: [], onBranch: () => {} }),
    root,
  );
  try {
    const row = () => root.querySelector<HTMLDetailsElement>('.agents .agent')!;
    equal(root.querySelectorAll('.agents .agent').length, 1);
    equal(row().dataset.state, 'running');
    ok(row().querySelector('.live'), 'a running helper shows the live mark');
    equal(row().querySelector('summary b')!.textContent, 'Find file defining load_config');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · working · 1 step');
    equal(root.querySelector('.activity summary')!.textContent, 'Working · 1 step · 1 command', 'the helper and its tools are not counted twice');
    equal(root.querySelectorAll('.activity .tools > span').length, 1, 'only the agent’s own tool is in its activity');
    row().open = true;
    // The row survives the helper's progress, so an opened row stays open.
    setMessage({ ...message(), tools: [OWN, { ...AGENT, ok: true, output: FRAMED }, { ...GREP, ok: true }] });
    equal(row().open, true);
    equal(row().dataset.state, 'done');
    equal(row().querySelectorAll('.tools > span').length, 1);
    ok(row().querySelector('.agent-report')!.textContent!.startsWith('The definition is in one file'), 'the report shows without its frame');
    ok(!row().textContent!.includes('Subagent hand-back'));
    setMessage({ ...message(), status: 'stopped', tools: [OWN, AGENT, GREP] });
    equal(row().dataset.state, 'stopped', 'a helper that never finished is not shown as working once the reply ended');
    equal(row().querySelector('.live'), null);
    // A refusal is not a failure, and a helper sent to the background keeps working after the reply ends.
    setMessage({ ...message(), tools: [{ ...AGENT, ok: false, declined: true, output: 'The user declined.' }] });
    equal(row().dataset.state, 'declined');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · declined · 0 steps');
    setMessage({ ...message(), tools: [{ ...AGENT, input: { ...(AGENT.input as object), run_in_background: true } }] });
    equal(row().dataset.state, 'background');
    ok(row().querySelector('.live'), 'a background helper still shows the live mark');
  } finally {
    dispose();
    root.remove();
  }
});

// R161: a helper from the other AI (Faber's own `Helper` tool) is the same kind of step, named by its title and by the AI that ran it.
const HELPER: ToolUse = { id: 'h1', name: 'Helper', input: { title: 'Review the parser', brief: 'Read parser.rs and list mistakes.' } };
const HELPER_RUN: ToolUse = { id: 'c1', name: 'commandExecution', input: 'rg -n parse', parent: 'h1', ok: true };

check('helper step: a helper call collects the other AI’s commands, is named by its title, and its kind is the AI that ran it', () => {
  const steps = agentSteps([OWN, HELPER, HELPER_RUN], 'Codex');
  deepEqual(steps.map((s) => [s.title, s.kind, s.helper, s.tools.map((t) => t.id)]), [['Review the parser', 'Codex', true, ['c1']]]);
  deepEqual(agentSteps([{ ...HELPER, input: { brief: 'Read parser.rs and list mistakes.' } }], 'Codex')[0].title, 'Read parser.rs and list mistakes.');
  equal(agentSteps([AGENT], 'Codex')[0].helper, false);
});

check('helper step: in a reply on Claude Code the helper row names Codex, and its report is the helper’s own words', async () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const tools = [OWN, { ...HELPER, ok: true, output: 'Two mistakes in parser.rs.' }, HELPER_RUN];
  const message: Message = { id: 'help', role: 'assistant', at: 0, status: 'done', vendor: 'claude', text: 'Done.', tools };
  const dispose = render(() => createComponent(Thread, { messages: [message], from: 0, conversationId: 'help', options: [], onBranch: () => {} }), root);
  try {
    const row = root.querySelector<HTMLDetailsElement>('.agents .agent')!;
    equal(row.querySelector('summary b')!.textContent, 'Review the parser');
    equal(row.querySelector('.agent-meta')!.textContent, 'Codex · done · 1 step');
    equal(row.querySelector('.agent-report')!.textContent!.trim(), 'Two mistakes in parser.rs.');
  } finally {
    dispose();
    root.remove();
  }
});
