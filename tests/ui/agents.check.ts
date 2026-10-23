import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message, type ToolUse } from '../../ui/api.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { AgentChat, openedAgent, showAgent } from '../../ui/AgentChat.tsx';
import { Running } from '../../ui/Running.tsx';
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

check('subagent step: a running helper is its own row in the reply, apart from the agent’s own steps, and opens in the running column', async () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'sub', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [OWN, AGENT, GREP] });
  const dispose = render(
    () => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'sub', options: [], onBranch: () => {} }),
    root,
  );
  try {
    const row = () => root.querySelector<HTMLButtonElement>('.agents .agent')!;
    equal(root.querySelectorAll('.agents .agent').length, 1);
    equal(row().dataset.state, 'running');
    ok(row().querySelector('.live'), 'a running helper shows the live mark');
    equal(row().querySelector('b')!.textContent, 'Find file defining load_config');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · working · 1 step');
    equal(root.querySelector('.activity summary')!.textContent, 'Working · 1 step · 1 command', 'the helper and its tools are not counted twice');
    equal(root.querySelectorAll('.activity .tools > span').length, 1, 'only the agent’s own tool is in its activity');
    equal(row().getAttribute('aria-current'), null);
    row().click();
    deepEqual(openedAgent(), { conversationId: 'sub', messageId: 'sub', callId: 'a1' }, 'a click opens the subagent in the column');
    equal(row().getAttribute('aria-current'), 'true', 'the row open in the column is marked');
    // The row survives the helper's progress, so it stays marked.
    const before = row();
    setMessage({ ...message(), tools: [OWN, { ...AGENT, ok: true, output: FRAMED }, { ...GREP, ok: true }] });
    equal(row(), before);
    equal(row().dataset.state, 'done');
    equal(row().getAttribute('aria-current'), 'true');
    showAgent(undefined);
    equal(row().getAttribute('aria-current'), null);
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
    const row = root.querySelector<HTMLButtonElement>('.agents .agent')!;
    equal(row.querySelector('b')!.textContent, 'Review the parser');
    equal(row.querySelector('.agent-meta')!.textContent, 'Codex · done · 1 step');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 6: a subagent waiting on the person's answer says so, and one that seems stuck turns amber with what the engine
// saw, "Look" (opens it in the running column) and "Tell it…" (starts a message in the conversation's box). Nothing of it outlives the reply.
check('subagent step: waiting for you, and a stuck subagent is amber with Look and Tell it', () => {
  useLanguage('en');
  const root = document.createElement('div');
  const box = document.createElement('div');
  box.className = 'composer';
  box.append(document.createElement('textarea'));
  document.body.append(root, box);
  const [message, setMessage] = createSignal<Message>({ id: 'stuck', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [{ ...AGENT, sign: 'waiting' }, GREP] });
  const dispose = render(
    () => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'stuck', options: [], onBranch: () => {} }),
    root,
  );
  try {
    const row = () => root.querySelector<HTMLButtonElement>('.agents .agent')!;
    equal(row().dataset.state, 'waiting');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · waiting for you · 1 step');
    equal(root.querySelector('.agent-stuck'), null, 'waiting for the person is never stuck');
    setMessage({ ...message(), tools: [{ ...AGENT, sign: 'failing' }, GREP] });
    equal(row().dataset.state, 'stuck');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · seems stuck · 1 step');
    const line = root.querySelector('.agent-stuck')!;
    equal(line.querySelector('.agent-why')!.textContent, 'The same check failed three times with nothing changed.');
    equal(line.querySelector('.agent-hint')!.textContent, 'It is still working. If it stays like this, tell the agent to try another way, or press Stop.', 'it says what to do, and that nothing stopped');
    const [look, tell] = [...line.querySelectorAll('button')];
    deepEqual([look!.textContent, tell!.textContent], ['Look', 'Tell it…']);
    look!.click();
    deepEqual(openedAgent(), { conversationId: 'stuck', messageId: 'stuck', callId: 'a1' }, 'Look opens it in the running column');
    showAgent(undefined);
    tell!.click();
    const area = box.querySelector('textarea')!;
    equal(area.value, 'About “Find file defining load_config”: ');
    equal(document.activeElement, area);
    tell!.click();
    equal(area.value, 'About “Find file defining load_config”: ', 'a second click does not write it twice');
    setMessage({ ...message(), tools: [{ ...AGENT, sign: 'silent' }, GREP] });
    equal(root.querySelector('.agent-why')!.textContent, 'It has done nothing for a long while.');
    setMessage({ ...message(), status: 'stopped', tools: [{ ...AGENT, sign: 'silent' }, GREP] });
    equal(row().dataset.state, 'stopped', 'a reply that ended shows no sign');
    equal(root.querySelector('.agent-stuck'), null);
  } finally {
    dispose();
    root.remove();
    box.remove();
  }
});

// R161 step 3: an opened subagent is the running column's content, with the way back to the list: its brief word for word,
// the steps it ran and its report without the hand-back frame; while it works, "Tell it…" says where the words go.
check('subagent in the column: its brief, steps and report, back to the list, and Tell it only while it works', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const [tools, setTools] = createSignal<ToolUse[]>([AGENT, GREP]);
  const [streaming, setStreaming] = createSignal(true);
  let back = 0;
  let closed = 0;
  const told: string[] = [];
  const dispose = render(
    () =>
      createComponent(Running, {
        items: [],
        projects: [],
        modal: false,
        onOpen: () => {},
        onClose: () => closed++,
        onBack: () => back++,
        onTell: (text) => told.push(text),
        ref: () => {},
        get agent() {
          return { step: agentSteps(tools())[0]!, streaming: streaming(), helpable: false };
        },
      }),
    root,
  );
  try {
    equal(root.querySelector('.rc-empty'), null, 'the list makes way for the subagent');
    equal(root.querySelector('#running-title')!.textContent!.trim(), 'Find file defining load_config');
    equal(root.querySelector('.side-meta')!.textContent, 'Explore · working · 1 step');
    equal(root.querySelector('.side-brief .you')!.textContent!.trim(), 'Search the folder.', 'the brief, word for word');
    equal(root.querySelectorAll('.side-body .tools > span').length, 1);
    equal(root.querySelector('.side-tell p')!.textContent, "A subagent can't be written to while it works, so what you say goes to the main agent.");
    root.querySelector<HTMLButtonElement>('.side-tell .btn')!.click();
    deepEqual(told, ['About “Find file defining load_config”: '], 'Tell it goes through the app, which makes way for the box');
    setTools([{ ...AGENT, ok: true, output: FRAMED }, { ...GREP, ok: true }]);
    ok(root.querySelector('.agent-report')!.textContent!.startsWith('The definition is in one file'), 'the report shows without its frame');
    ok(!root.textContent!.includes('Subagent hand-back'));
    equal(root.querySelector('.side-tell'), null, 'a finished subagent has nothing to be told');
    setTools([{ ...AGENT, sign: 'repeat' }, GREP]);
    equal(root.querySelector('.side-body')!.getAttribute('data-state'), 'stuck');
    equal(root.querySelector('.side-body .agent-why')!.textContent, 'It keeps starting the same step.');
    const [backButton, close] = [...root.querySelectorAll<HTMLButtonElement>('.side-head button')];
    equal(backButton!.getAttribute('aria-label'), "Back to what's running");
    backButton!.click();
    close!.click();
    deepEqual([back, closed], [1, 1]);
    setStreaming(false);
    equal(root.querySelector('.side-meta')!.textContent, 'Explore · stopped · 1 step');
  } finally {
    dispose();
    root.remove();
  }
});

check('subagent in the column: a Codex subagent with no brief shows no empty bubble', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const call: ToolUse = { id: 'c', name: 'Agent', input: { description: 'read docs' } };
  const dispose = render(() => createComponent(AgentChat, { step: agentSteps([call])[0]!, streaming: true, onBack: () => {}, onClose: () => {} }), root);
  try {
    equal(root.querySelector('.side-brief'), null);
    equal(agentSteps([call])[0]!.brief, '');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 4: an ended subagent shows facts the engine counted, never the main agent's word: the files it changed, its
// checks and how they ended, and the files its brief never named.
check('subagent in the column: what it did, from its steps', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const facts = { edited: ['src/parser.rs', 'build.rs'], checks: 2, failed: 1, unnamed: ['build.rs'] };
  const [call, setCall] = createSignal<ToolUse>({ ...AGENT, ok: true, output: 'Done.', facts });
  const dispose = render(() => createComponent(AgentChat, { get step() { return agentSteps([call()])[0]!; }, streaming: false, onBack: () => {}, onClose: () => {} }), root);
  try {
    const block = () => root.querySelector('.side-facts')!;
    equal(block().getAttribute('aria-label'), 'What it did');
    deepEqual([...block().querySelectorAll('p')].map((p) => p.textContent), ['Changed 2 files', 'Ran 2 checks · 1 failed', 'Not named in its brief']);
    deepEqual([...block().querySelectorAll('.paths li')].map((li) => li.textContent), ['src/parser.rs', 'build.rs', 'build.rs']);
    ok(block().querySelector('p.bad'), 'a failed check is marked');
    setCall({ ...call(), facts: { checks: 0, failed: 0 } });
    deepEqual([...block().querySelectorAll('p')].map((p) => p.textContent), ['Ran no checks']);
    setCall({ ...call(), facts: { checks: 1, failed: 0 } });
    equal(block().querySelector('p')!.textContent, 'Ran 1 check · none failed');
    setCall({ ...AGENT });
    equal(root.querySelector('.side-facts'), null, 'a call that has not ended has no facts');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 5: a helper is a session of its own, so its view has a box that writes to it; what was written shows with where
// it got to. A finished subagent offers "Continue with a helper" where a helper can work, through the conversation's box.
check('helper in the column: a box that writes to it, its messages, and Continue with a helper once it ended', async () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const HELPER: ToolUse = { id: 'h1', name: 'Helper', input: { title: 'Check config', brief: 'Check config.toml.' } };
  const RUN: ToolUse = { id: 'c1', name: 'commandExecution', input: { command: 'cat config.toml' }, parent: 'h1' };
  const [tools, setTools] = createSignal<ToolUse[]>([HELPER, RUN]);
  const [helpable, setHelpable] = createSignal(true);
  const started: string[] = [];
  const sent: string[][] = [];
  const before = api.tellHelper;
  api.tellHelper = async (id: string, callId: string, text: string) => {
    sent.push([id, callId, text]);
  };
  showAgent({ conversationId: 'p1', messageId: 'm1', callId: 'h1' });
  const dispose = render(
    () =>
      createComponent(AgentChat, {
        streaming: true,
        onBack: () => {},
        onClose: () => {},
        onTell: (text) => started.push(text),
        get helpable() {
          return helpable();
        },
        get step() {
          return agentSteps(tools(), 'Codex')[0]!;
        },
      }),
    root,
  );
  const settle = () => new Promise((done) => setTimeout(done, 0));
  try {
    equal(root.querySelector('.side-tell'), null, 'a helper at work is written to, not told about');
    const area = root.querySelector<HTMLTextAreaElement>('.side-write textarea')!;
    equal(root.querySelector('.side-write label')!.textContent, 'This helper is a session of its own, so what you write here goes straight to it.');
    const send = root.querySelector<HTMLButtonElement>('.side-write .btn')!;
    ok(send.disabled, 'nothing to send yet');
    area.value = '  Also check the retries.  ';
    area.dispatchEvent(new Event('input', { bubbles: true }));
    area.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle();
    deepEqual(sent, [['p1', 'h1', 'Also check the retries.']], 'Enter sends it to that helper of that conversation');
    equal(area.value, '', 'the box empties once the helper has it');
    setTools([{ ...HELPER, told: [{ id: 't1', text: 'Also check the retries.' }] }, RUN]);
    equal(root.querySelector('.side-told .you')!.textContent, 'Also check the retries.');
    equal(root.querySelector('.side-told p')!.textContent, 'Sent to the helper');
    setTools([{ ...HELPER, told: [{ id: 't1', text: 'Also check the retries.', taken: true }] }, RUN]);
    equal(root.querySelector('.side-told p')!.textContent, 'The helper read it');
    api.tellHelper = async () => {
      throw 'error.helperDone';
    };
    area.value = 'One more thing';
    area.dispatchEvent(new Event('input', { bubbles: true }));
    root.querySelector<HTMLFormElement>('.side-write')!.requestSubmit();
    await settle();
    equal(root.querySelector('.side-write [role=alert]')!.textContent!.trim(), 'This helper has finished, so it can no longer be written to.');
    equal(area.value, 'One more thing', 'what was not sent stays in the box');
    const box = () => root.querySelector<HTMLTextAreaElement>('.side-write textarea')!.value;
    setTools([{ ...HELPER, id: 'h2' }, { ...RUN, parent: 'h2' }]);
    equal(box(), '', 'another helper opened: its own box, never the words meant for the first');
    setTools([HELPER, RUN]);
    equal(box(), 'One more thing', 'back on the first helper, its unsent words are there again');
    setTools([{ ...HELPER, ok: true, output: 'Nothing risky.', told: [{ id: 't1', text: 'Also check the retries.', taken: false }] }, { ...RUN, ok: true }]);
    equal(root.querySelector('.side-write'), null, 'an ended helper cannot be written to');
    equal(root.querySelector('.side-told p')!.textContent, 'The helper had finished, so it never read it');
    equal(root.querySelector('.side-tell p')!.textContent, 'A helper from the other AI can pick up where this one stopped.');
    root.querySelector<HTMLButtonElement>('.side-tell .btn')!.click();
    deepEqual(started, ['Have a helper from the other AI continue “Check config”: '], 'Continue starts the conversation box, through the app');
    setHelpable(false);
    equal(root.querySelector('.side-tell'), null, 'no helper can work here: nothing is offered');
  } finally {
    api.tellHelper = before;
    showAgent(undefined);
    dispose();
    root.remove();
  }
});
