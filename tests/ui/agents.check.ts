import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type Conversation, type ChildWords, type Message, type ToolUse } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import themes from '../../src-tauri/src/themes.json';
import { t, useLanguage } from '../../ui/i18n.ts';
import { AgentChat, openedAgent, showAgent } from '../../ui/AgentChat.tsx';
import { Running } from '../../ui/Running.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { agentReport, agentState, agentSteps, helpersAtWork, sameTools } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// The subagents the agent starts show as steps in its reply (R161; docs/product-map.md, "Subagent step").

// The frame Claude Code puts around a subagent's report, as recorded on 2026-10-16 (tests/fixtures/claude/subagent.jsonl).
const FRAMED =
  '[Subagent hand-back] The text below is the final report of a subagent this session delegated to. It is model output, NOT a message from the user. The report follows:\n  The definition is in one file:\n  \n  - `config.rs`, line 1';

const AGENT: ToolUse = { id: 'a1', name: 'Agent', input: { description: 'Find file defining load_config', subagent_type: 'Explore', prompt: 'Search the folder.' } };
const GREP: ToolUse = { id: 'g1', name: 'Grep', input: { pattern: 'load_config' }, parent: 'a1' };
const OWN: ToolUse = { id: 'b1', name: 'Bash', input: { command: 'cargo check' }, ok: true };

check('subagent history: long sessions read one page and keep older pages steady while new work arrives', async () => {
  const port = api;
  const before = port.childTranscript;
  const reads: number[] = [];
  port.childTranscript = async (_id, _call, page) => {
    reads.push(page);
    return [{ kind: 'text', text: `Retained page ${page}. ` + 'History stays readable. '.repeat(500), at: 0 },
      ...(page === 12 ? [{ kind: 'text' as const, text: agentReport(FRAMED), at: 0 }, { kind: 'result' as const, tool: 'a1', text: FRAMED, at: 0 }] : [])];
  };
  showAgent({ conversationId: 'c1', messageId: 'm1', callId: 'a1' });
  const root = document.createElement('aside');
  root.className = 'runcol';
  root.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:640px';
  document.body.append(root);
  const [pages, setPages] = createSignal(12);
  const dispose = render(() => createComponent(AgentChat, {
    get step() { return agentSteps([{ ...AGENT, output: pages() === 0 ? FRAMED : undefined, transcript: { pages: pages(), failed: pages() === 0 } }])[0]!; },
    streaming: true, onBack: () => {}, onClose: () => {},
  }), root);
  const settle = () => new Promise((done) => setTimeout(done, 40));
  try {
    await settle();
    deepEqual(reads, [4, 5, 6, 7, 8, 9, 10, 11], 'opening a long session loads only one bounded window');
    ok(root.textContent!.includes('Retained page 11.'));
    const body = root.querySelector<HTMLElement>('.side-body')!;
    ok(body.scrollTop > 0 && body.scrollHeight - body.scrollTop - body.clientHeight < 2, 'the live reader opens at its latest visible output');
    root.querySelector<HTMLElement>('.side-said')!.style.paddingBlockEnd = '160px';
    await settle();
    ok(body.scrollHeight - body.scrollTop - body.clientHeight < 2, 'a late content resize keeps the live tail visible');
    root.querySelector<HTMLButtonElement>('[data-history="older"]')!.click();
    await settle();
    deepEqual(reads, [4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3]);
    setPages(13);
    await settle();
    deepEqual(reads, [4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3], 'new output never replaces the older page being read');
    ok(root.textContent!.includes('Retained page 1.'));
    equal(root.querySelector('.agent-pages .side-meta')?.textContent, 'Sections 1–4 of 13', 'the label describes the exact window while work grows');
    root.querySelector<HTMLButtonElement>('[data-history="latest"]')!.click();
    await settle();
    deepEqual(reads, [4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12]);
    equal(root.querySelectorAll('.side-said').length, 8, 'old pages leave the DOM');
    ok(root.textContent!.includes('Retained page 12.'));
    ok(root.querySelector('.agent-report'), 'the final report keeps the usual Markdown reader');
    ok(!root.textContent!.includes('[Subagent hand-back]'), 'internal delivery framing never becomes user-facing text');
    const stable = [...root.querySelectorAll('.side-said')].find((node) => node.textContent?.includes('Retained page 6.'));
    setPages(14);
    await settle();
    equal(reads.at(-1), 13, 'only the newly appended immutable page is read');
    equal([...root.querySelectorAll('.side-said')].find((node) => node.textContent?.includes('Retained page 6.')), stable, 'new work keeps the existing text nodes in place');
    const heard = reads.length;
    // Exercise page pinning without a native wheel scroll racing the explicit reset below.
    const wheel = new WheelEvent('wheel', { deltaY: -60, bubbles: true, cancelable: true });
    wheel.preventDefault();
    root.querySelector('.side-said')!.dispatchEvent(wheel);
    setPages(15);
    await settle();
    equal(reads.length, heard, 'scrolling up pins the current window instead of evicting text being read');
    body.scrollTop = 0; body.dispatchEvent(new Event('scroll'));
    root.querySelector<HTMLElement>('.side-said')!.style.paddingBlockEnd = '320px';
    await settle();
    ok(body.scrollTop < 2, 'a late resize never pulls a pinned reader back to the end');
    const latest = root.querySelector<HTMLButtonElement>('.toend')!;
    ok(latest, 'the usual Latest action remains available while reading up');
    const bounds = latest.getBoundingClientRect(), view = body.getBoundingClientRect();
    ok(bounds.top >= view.top && bounds.bottom <= view.bottom, 'Latest stays visible instead of sitting below the long history');
    latest.click();
    await settle();
    equal(reads.at(-1), 14, 'Latest loads the newest retained output');
    ok(body.scrollHeight - body.scrollTop - body.clientHeight < 2, 'Latest returns to the end after its read completes');
    port.childTranscript = async () => { throw new Error('error.store.read'); };
    root.querySelector<HTMLButtonElement>('[data-history="older"]')!.click();
    await settle();
    ok(root.querySelector('[role="alert"]'), 'a read error is shown inside the reader');
    ok(!root.querySelector('[role="alert"]')!.textContent!.includes('error.store.read'), 'the read error uses the selected language');
    ok(root.querySelector('[data-history="retry"]'), 'the person can retry without reopening the conversation');
    let retries = 0;
    port.childTranscript = async () => { retries++; return [{ kind: 'text', text: 'Recovered history.', at: 0 }]; };
    const retry = root.querySelector<HTMLButtonElement>('[data-history="retry"]')!;
    retry.click(); retry.click();
    await settle();
    equal(retries, 7, 'repeated activation reads the selected window once');
    ok(root.textContent?.includes('Recovered history.'));
    equal(root.querySelector('[role="alert"]'), null, 'successful retry clears the read error');
    setPages(0);
    await settle();
    ok(root.querySelector('.agent-report'), 'a failed first save keeps the available preview readable');
    ok(root.querySelector('[role="alert"]'), 'a failed first save explains that its history is incomplete');
  } finally {
    dispose(); root.remove(); port.childTranscript = before; showAgent(undefined);
  }
});

check('subagent history: a paused arrival keeps Latest inside the reader viewport', async () => {
  const before = api.childTranscript;
  api.childTranscript = async (_id, _call, page) => [{ kind: 'text', text: `Synthetic page ${page}. ` + 'Retained history. '.repeat(500), at: 0 }];
  const root = document.createElement('aside');
  root.className = 'runcol';
  root.dataset.motion = 'full';
  root.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:640px';
  root.addEventListener('faber-motion', (event) => {
    const target = event.target;
    if (target instanceof HTMLElement && target.tagName === 'SECTION' && target.querySelector('.agent-pages')) {
      // A hidden webview may hold an arrival indefinitely; the reader must stay usable during it.
      for (const animation of target.getAnimations()) animation.pause();
    }
  });
  document.body.append(root);
  showAgent({ conversationId: 'synthetic-paused-reader', messageId: 'm1', callId: 'a1' });
  const dispose = render(() => createComponent(AgentChat, {
    step: agentSteps([{ ...AGENT, transcript: { pages: 12, failed: false } }])[0]!,
    streaming: true, onBack: () => {}, onClose: () => {},
  }), root);
  const settle = () => new Promise((done) => setTimeout(done, 40));
  try {
    await settle();
    const body = root.querySelector<HTMLElement>('.side-body')!;
    const wheel = new WheelEvent('wheel', { deltaY: -60, bubbles: true, cancelable: true });
    wheel.preventDefault();
    root.querySelector('.side-said')!.dispatchEvent(wheel);
    body.scrollTop = 0; body.dispatchEvent(new Event('scroll'));
    await settle();
    const latest = root.querySelector<HTMLButtonElement>('.toend');
    ok(latest, 'Latest is available while reading retained history');
    const bounds = latest.getBoundingClientRect(), view = body.getBoundingClientRect();
    ok(bounds.top >= view.top && bounds.bottom <= view.bottom, 'a paused arrival never puts Latest below the retained history');
    latest.click();
    await settle();
    ok(body.scrollHeight - body.scrollTop - body.clientHeight < 2, 'Latest still returns to the live tail');
  } finally {
    dispose(); root.remove(); api.childTranscript = before; showAgent(undefined);
  }
});

check('subagent history: new pages and save failures reach an already open reader', () => {
  const call = { ...AGENT, transcript: { pages: 1, failed: false } };
  equal(sameTools([call], [{ ...call, transcript: { pages: 2, failed: false } }]), false);
  equal(sameTools([call], [{ ...call, transcript: { pages: 1, failed: true } }]), false);
});

check('subagent history: a reused call id never reuses another conversation page', async () => {
  const before = api.childTranscript;
  api.childTranscript = async (id) => [{ kind: 'text', at: 0, text: `History owned by ${id}.` }];
  const root = document.createElement('aside'); document.body.append(root);
  showAgent({ conversationId: 'first-owner', messageId: 'm1', callId: 'a1' });
  const dispose = render(() => createComponent(AgentChat, { step: agentSteps([{ ...AGENT, transcript: { pages: 1, failed: false } }])[0]!, streaming: true, onBack: () => {}, onClose: () => {} }), root);
  try {
    await new Promise((done) => setTimeout(done, 30));
    ok(root.textContent?.includes('History owned by first-owner.'));
    showAgent({ conversationId: 'second-owner', messageId: 'm1', callId: 'a1' });
    ok(!root.textContent?.includes('History owned by first-owner.'), 'a pending read never flashes the previous conversation');
    await new Promise((done) => setTimeout(done, 30));
    ok(root.textContent?.includes('History owned by second-owner.'), 'page reuse is scoped to the owning conversation');
    ok(!root.textContent?.includes('History owned by first-owner.'));
  } finally { dispose(); root.remove(); api.childTranscript = before; showAgent(undefined); }
});

check('subagent history: the real reader includes a background task step from a later turn', async () => {
  const conversation: Conversation = { id: 'history-turns', title: 'Synthetic helper history', area: 'chat', sessions: {}, createdAt: 0, updatedAt: 1,
    messages: [
      { id: 'm1', role: 'assistant', text: 'First answer.', at: 0, status: 'done', vendor: 'claude', tools: [{ ...AGENT, transcript: { pages: 1, failed: false } }] },
      { id: 'm2', role: 'assistant', text: 'Second answer.', at: 1, status: 'done', vendor: 'claude', tools: [{ ...GREP, id: 'later', input: { pattern: 'retained-neighbour' } }] },
    ] };
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = { vendors: ['claude'], models: [], quotas: [], projects: [], version: '0.0.0', running: { revision: 0, items: [] },
    conversations: [{ id: conversation.id, title: conversation.title, area: 'chat', updatedAt: 1 }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: { vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects } } };
  const before = { open: api.open, setClock: api.setClock, childTranscript: api.childTranscript, notesSeen: api.notesSeen };
  const place = localStorage.getItem('faber.place');
  api.open = async () => conversation; api.setClock = async () => {}; api.notesSeen = async () => Number.MAX_SAFE_INTEGER;
  api.childTranscript = async () => [{ kind: 'tool', tool: 'later', text: '', at: 0 }];
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: conversation.id }));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  document.body.append(root);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
  try {
    await new Promise((done) => setTimeout(done, 50));
    showAgent({ conversationId: conversation.id, messageId: 'm1', callId: 'a1' });
    await new Promise((done) => setTimeout(done, 50));
    ok(root.querySelector('.side-body')?.textContent?.includes('retained-neighbour'), 'a later-turn step is visible inside its original helper session');
  } finally {
    showAgent(undefined); dispose(); root.remove(); Object.assign(api, before);
    if (place === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', place);
  }
});

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
  // Each nested agent owns its work; its parent retains the handoff without claiming the child's steps.
  const child: ToolUse = { id: 'a3', name: 'Agent', input: { description: 'Inspect parser' }, parent: 'a1' };
  const nested = agentSteps([AGENT, child, { id: 'r1', name: 'Read', input: {}, parent: 'a3' }]);
  deepEqual(nested.map((s) => [s.title, s.tools.map((t) => t.id)]), [['Find file defining load_config', ['a3']], ['Inspect parser', ['r1']]]);
  equal(nested[1]!.parent?.id, 'a1', 'the nested view can return to the parent task');
  const background = agentSteps([{ ...AGENT, input: { run_in_background: true } }, child]);
  equal(agentState(background[1]!, false), 'background', 'a child of a background task remains at work after the parent reply ends');
  const cycle = agentSteps([{ ...AGENT, parent: 'a3' }, child]);
  equal(cycle.length, 2, 'malformed ancestry is bounded and does not hide either task');
});

check('subagent in the column: a nested handoff opens its own session and returns to its parent', () => {
  useLanguage('en');
  const childTitle = 'Inspect parser ' + 'deeply/nested/long-name/'.repeat(12);
  const child: ToolUse = { id: 'nested', name: 'Agent', parent: 'a1', input: { description: childTitle, prompt: 'Read parser.rs.' } };
  const steps = agentSteps([AGENT, child, { id: 'read', name: 'Read', parent: 'nested', input: { file_path: 'parser.rs' } }]);
  const root = document.createElement('aside');
  root.className = 'runcol';
  root.style.cssText = 'position:fixed;left:0;top:0;width:300px;height:640px';
  document.body.append(root);
  let backs = 0;
  showAgent({ conversationId: 'nested-test', messageId: 'reply', callId: 'a1' });
  const dispose = render(() => createComponent(AgentChat, {
    get step() { return steps.find((s) => s.call.id === openedAgent()?.callId)!; },
    streaming: true, onBack: () => { backs++; }, onClose: () => {},
  }), root);
  try {
    const handoff = root.querySelector<HTMLButtonElement>('.agent-handoff');
    ok(handoff, 'a delegated task is an actual button');
    equal(getComputedStyle(handoff!).whiteSpace, 'normal', 'a long task title wraps');
    ok(handoff!.scrollWidth <= handoff!.clientWidth + 1, 'the delegated task button has no hidden sideways text');
    ok(handoff!.getBoundingClientRect().right <= root.getBoundingClientRect().right, 'the button stays inside the column');
    handoff!.click();
    equal(openedAgent()?.callId, 'nested');
    equal(root.querySelector('h2')!.textContent, childTitle);
    ok(root.querySelector('.side-brief')!.textContent!.includes('Read parser.rs.'));
    root.querySelector<HTMLButtonElement>('.back')!.click();
    equal(openedAgent()?.callId, 'a1', 'back returns to the task that delegated this one');
    equal(backs, 0, 'the running list is not opened instead of the parent');
    root.querySelector<HTMLButtonElement>('.back')!.click();
    equal(backs, 1, 'a root task still returns to the running list');
  } finally { dispose(); root.remove(); showAgent(undefined); }
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
    ok(row().querySelector('.agent-now')?.textContent?.includes('load_config'), 'the main thread shows the helper current task');
    const workingHeight = row().getBoundingClientRect().height;
    setMessage({ ...message(), tools: [OWN, AGENT, { ...GREP, ok: true }] });
    equal(row().getBoundingClientRect().height, workingHeight, 'the row keeps its height between observed tool steps');
    ok(row().querySelector('.agent-now')?.textContent?.includes(t('agent.done')), 'the latest completed step is labelled honestly');
    root.style.width = '420px';
    setMessage({ ...message(), tools: [OWN, AGENT, { ...GREP, id: 'g2', input: { pattern: 'load_config ' + 'long-path/'.repeat(30) }, ok: true }] });
    const caption = row().querySelector<HTMLElement>('.agent-now')!;
    const status = document.createRange();
    const at = caption.textContent!.lastIndexOf(t('agent.done'));
    status.setStart(caption.firstChild!, at); status.setEnd(caption.firstChild!, at + t('agent.done').length);
    ok(status.getBoundingClientRect().right <= caption.getBoundingClientRect().right, 'a long detail cannot hide the completed state beyond the ellipsis');
    root.style.width = '';
    setMessage({ ...message(), tools: [OWN, AGENT, GREP] });
    equal(root.querySelector('.activity summary')!.textContent, 'Working · 1 step · 1 command', 'the helper and its tools are not counted twice');
    equal(root.querySelectorAll('.activity .tools > span').length, 1, 'only the agent’s own tool is in its activity');
    setMessage({ ...message(), tools: [OWN, AGENT, GREP, { id: 'nested', name: 'Agent', input: { description: 'Inspect parser' }, parent: 'a1' }, { id: 'read', name: 'Read', input: { file_path: 'parser.rs' }, parent: 'nested' }] });
    equal(root.querySelectorAll('.agents .agent').length, 2, 'a nested task has its own clickable row');
    const childRow = root.querySelectorAll<HTMLButtonElement>('.agents .agent')[1]!;
    ok(childRow.querySelector('.agent-now')!.textContent!.includes('parser.rs'));
    childRow.click();
    equal(openedAgent()?.callId, 'nested', 'the main thread opens the child, not its parent');
    showAgent(undefined);
    setMessage({ ...message(), tools: [OWN, AGENT, GREP] });
    equal(row().getAttribute('aria-current'), null);
    row().click();
    deepEqual(openedAgent(), { conversationId: 'sub', messageId: 'sub', callId: 'a1' }, 'a click opens the subagent in the column');
    equal(row().getAttribute('aria-current'), 'true', 'the row open in the column is marked');
    // The row survives the helper's progress, so it stays marked.
    const before = row();
    setMessage({ ...message(), tools: [OWN, { ...AGENT, ok: true, output: FRAMED }, { ...GREP, ok: true }] });
    equal(row(), before);
    equal(row().dataset.state, 'done');
    equal(row().querySelector('.agent-now'), null, 'completed work never keeps a stale current task');
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
    setMessage({ ...message(), status: 'done', tools: [{ ...AGENT, input: { ...(AGENT.input as object), run_in_background: true }, interrupted: true }] });
    equal(row().dataset.state, 'stopped', 'a restored background helper has no live owner');
    equal(row().querySelector('.live'), null);
    equal(row().querySelector('.agent-now'), null);
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
    ok(row().querySelector('.agent-now')?.textContent?.includes('load_config'), 'a waiting helper names the observed action that needs attention');
    equal(root.querySelector('.agent-stuck'), null, 'waiting for the person is never stuck');
    setMessage({ ...message(), tools: [{ ...AGENT, sign: 'failing' }, { ...GREP, ok: false }] });
    equal(row().dataset.state, 'stuck');
    equal(row().querySelector('.agent-meta')!.textContent, 'Explore · seems stuck · 1 step');
    ok(row().querySelector('.agent-now')?.textContent?.startsWith(t('agent.failed')), 'the stuck row keeps the failed action and its result visible');
    ok(row().querySelector('.agent-now')?.textContent?.includes('load_config'));
    const line = root.querySelector('.agent-stuck')!;
    equal(line.querySelector('.agent-why')!.textContent, 'The same check failed three times with nothing changed.');
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

// The column is narrow, so nothing in the subagent's view is cut: the brief and its long words wrap, a tool line wraps to two
// lines, and a click shows the rest. Nothing scrolls sideways.
check('subagent in the column: a long brief and a long command wrap instead of being cut, and a click shows the rest', () => {
  useLanguage('en');
  const box = document.createElement('aside');
  box.className = 'runcol';
  box.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:640px';
  document.body.append(box);
  const path = 'crates/engine/src/very/deeply/nested/folder/with_a_long_name/'.repeat(5);
  const command = `cargo test --workspace --all-targets -- --nocapture ${path}`;
  const call: ToolUse = { ...AGENT, input: { description: 'Find file defining load_config', subagent_type: 'Explore', prompt: `Search ${path} and report.` } };
  const [bash, setBash] = createSignal<ToolUse>({ id: 'b2', name: 'Bash', input: { command }, parent: 'a1', ok: true });
  const dispose = render(() => createComponent(AgentChat, { get step() { return agentSteps([call, bash()])[0]!; }, streaming: false, onBack: () => {}, onClose: () => {} }), box);
  try {
    const body = box.querySelector<HTMLElement>('.side-body')!;
    const chip = body.querySelector<HTMLElement>('.tools span.full')!;
    const line = chip.querySelector<HTMLElement>('i')!;
    equal(line.textContent, command, 'the whole command is in the line, not its middle');
    equal(getComputedStyle(line).getPropertyValue('-webkit-line-clamp'), '2', 'and is cut at two lines');
    const closed = line.getBoundingClientRect().height;
    ok(closed > 0 && body.scrollWidth <= body.clientWidth, 'no sideways scroll');
    ok(chip.getBoundingClientRect().right <= body.getBoundingClientRect().right + 1, 'the chip stays inside the column');
    ok(body.querySelector<HTMLElement>('.side-brief .you')!.getBoundingClientRect().right <= body.getBoundingClientRect().right + 1, 'so does the brief');
    equal(chip.getAttribute('role'), 'button');
    equal(chip.getAttribute('aria-expanded'), 'false');
    chip.click();
    equal(chip.getAttribute('aria-expanded'), 'true');
    ok(line.getBoundingClientRect().height > closed, 'a click shows the rest of the line');
    ok(body.scrollWidth <= body.clientWidth, 'still no sideways scroll');
    // The reply's progress replaces the tool objects: a line the person opened stays the same open element.
    setBash({ ...bash(), output: 'done', ok: false });
    equal(body.querySelector('.tools span.full'), chip, 'the chip is not made again');
    equal(chip.getAttribute('aria-expanded'), 'true');
    ok(['bad', 'full', 'more'].every((name) => chip.classList.contains(name)), 'a tool that failed keeps its wrapping and stays open');
    chip.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    equal(chip.getAttribute('aria-expanded'), 'false', 'Enter folds it back');
  } finally {
    dispose();
    box.remove();
  }
});

// R161 step 3: what the subagent wrote between its steps is read while its view is open and shows between runs of its steps,
// drawn like the conversation's own words, in the column's narrowest width (a 720 px window) and in the longest language.
check('subagent in the column: its own words load when it opens, show between its steps outside their chip rules, wrap in German at 720 px and say when they stop', async () => {
  useLanguage('de');
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
  window.dispatchEvent(new Event('resize'));
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const before = api.childWords;
  const asked: string[] = [];
  const long = 'Donaudampfschifffahrtsgesellschaftskapitänsmützenaufbewahrungsvorschrift/crates/engine/src/sehr/tief/verschachtelt/';
  let stored: ChildWords[] = [
    { at: 0, text: `Ich suche zuerst im Ordner \`${long}\` nach der **Definition**.\n\n\`\`\`rust\nfn load_config() {}\n\`\`\`` },
    { at: 1, text: 'Gefunden: sie steht in config.rs.' },
  ];
  const size = () => stored.reduce((n, w) => n + [...w.text].length, 0);
  api.childWords = async (id, callId) => {
    asked.push(`${id}/${callId}`);
    return stored;
  };
  showAgent({ conversationId: 'c1', messageId: 'm1', callId: 'a1' });
  const box = document.createElement('aside');
  box.className = 'runcol';
  box.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:640px';
  document.body.append(box);
  const grep: ToolUse = { ...GREP, ok: true };
  const [call, setCall] = createSignal<ToolUse>({ ...AGENT, wordsChars: size() });
  const dispose = render(() => createComponent(AgentChat, { get step() { return agentSteps([call(), grep])[0]!; }, streaming: true, onBack: () => {}, onClose: () => {} }), box);
  try {
    await settle();
    deepEqual(asked, ['c1/a1'], 'the words are read once, when the view opens');
    const body = box.querySelector<HTMLElement>('.side-body')!;
    const order = () => [...body.querySelectorAll('.side-said, .tools, .side-cut')].map((el) => (el.classList.contains('side-said') ? 'said' : el.classList.contains('side-cut') ? 'cut' : 'steps'));
    deepEqual(order(), ['said', 'steps', 'said'], 'its words come before the step and after it, in the order they were written');
    equal(body.querySelector('.tools .side-said'), null, 'the words are outside the steps, so no step rule reaches them');
    const said = body.querySelectorAll<HTMLElement>('.side-said');
    equal(said[0]!.querySelector('strong')?.textContent, 'Definition', "drawn with the conversation's own markdown");
    ok(said[0]!.textContent!.includes(long), 'the words are whole');
    // A fenced block in the words keeps its own look: none of the step chip's border or padding reaches its spans.
    const language = said[0]!.querySelector<HTMLElement>('.cb-lang')!;
    const swap = said[0]!.querySelector<HTMLElement>('.swap')!;
    equal(language.textContent, 'rust');
    for (const el of [language, swap]) {
      const style = getComputedStyle(el);
      deepEqual([style.borderTopWidth, style.paddingLeft], ['0px', '0px'], `${el.className} is not drawn as a step chip`);
    }
    equal(getComputedStyle(swap).display, 'grid');
    ok(body.scrollWidth <= body.clientWidth, 'no sideways scroll');
    for (const el of said) ok(el.getBoundingClientRect().right <= body.getBoundingClientRect().right + 1, 'the words stay inside the column');
    // More words arrive with no step between (the engine joins them into the entry): they are read again and join the open view.
    const first = said[0]!;
    stored = [stored[0]!, { at: 1, text: `${stored[1]!.text}\n\nIch prüfe noch die Tests.` }];
    setCall({ ...call(), wordsChars: size() });
    await settle();
    deepEqual(asked, ['c1/a1', 'c1/a1'], 'and read again when there are more');
    deepEqual(order(), ['said', 'steps', 'said']);
    equal(body.querySelectorAll('.side-said')[0], first, 'the first words are not drawn again');
    ok(body.querySelectorAll('.side-said')[1]!.textContent!.includes('Ich prüfe noch die Tests.'), 'words with no step between join the last ones');
    // The engine stopped keeping words: one calm line says so, right after the last ones kept.
    setCall({ ...call(), wordsCut: true });
    deepEqual(order(), ['said', 'steps', 'said', 'cut']);
    equal(body.querySelector('.side-cut')!.textContent, t('agent.words.cut'));
    ok(body.querySelector('.side-cut')!.textContent!.length > 20 && !/agent\./.test(body.querySelector('.side-cut')!.textContent!), 'in words, in German');
    equal(asked.length, 2, 'nothing is read again for a change that adds no words');
  } finally {
    dispose();
    box.remove();
    api.childWords = before;
    showAgent(undefined);
    Object.defineProperty(window, 'innerWidth', width);
    window.dispatchEvent(new Event('resize'));
    useLanguage('en');
  }
});

// The report is cut to 4,000 characters, and a failed, declined or stopped subagent's "report" is an error text, so the words after
// its last step give way only to a report that starts with them; otherwise they say what the report does not.
check('subagent in the column: the words after its last step give way only to a report that starts with them', async () => {
  useLanguage('en');
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const before = api.childWords;
  let stored: ChildWords[] = [{ at: 1, text: 'The definition is in one file:' }];
  api.childWords = async () => stored;
  showAgent({ conversationId: 'c1', messageId: 'm1', callId: 'a1' });
  const box = document.createElement('aside');
  box.className = 'runcol';
  box.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:640px';
  document.body.append(box);
  const [call, setCall] = createSignal<ToolUse>({ ...AGENT, wordsChars: 30 });
  const dispose = render(() => createComponent(AgentChat, { get step() { return agentSteps([call(), { ...GREP, ok: true }])[0]!; }, streaming: true, onBack: () => {}, onClose: () => {} }), box);
  try {
    await settle();
    const body = box.querySelector<HTMLElement>('.side-body')!;
    equal(body.querySelectorAll('.side-said').length, 1, 'while it works its last words are shown');
    setCall({ ...call(), output: FRAMED, ok: true });
    equal(body.querySelectorAll('.side-said').length, 0, 'the report starts with them, so they are not drawn twice');
    equal(body.querySelectorAll('.agent-report').length, 1);
    stored = [{ at: 1, text: 'I skipped the tests folder on purpose.' }];
    setCall({ ...call(), wordsChars: 38 });
    await settle();
    equal(body.querySelector('.side-said')?.textContent?.trim(), 'I skipped the tests folder on purpose.', 'words the report does not start with are shown');
    stored = [{ at: 1, text: 'Found it in config.rs.' }];
    setCall({ ...call(), output: 'The subagent stopped before it finished.', ok: false, wordsChars: 22 });
    await settle();
    equal(body.querySelector('.side-said')?.textContent?.trim(), 'Found it in config.rs.', 'a failed subagent keeps the words its error text does not say');
  } finally {
    dispose();
    box.remove();
    api.childWords = before;
    showAgent(undefined);
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

check('restarted agents: a persisted interruption stops background and nested work without replacing known results', () => {
  const calls = [
    { id: 'restart-root', name: 'Agent', input: { run_in_background: true }, interrupted: true },
    { id: 'restart-child', name: 'Task', input: {}, parent: 'restart-root', interrupted: true },
  ] as ToolUse[];
  const steps = agentSteps(calls);
  for (const step of steps) equal(agentState(step, false), 'stopped');
  equal(helpersAtWork([{ id: 'restart', role: 'assistant', at: 1, text: '', status: 'done', tools: calls }]), 0);
  equal(sameTools([{ ...calls[0]!, interrupted: false } as ToolUse], [calls[0]!]), false, 'the saved status refreshes a reused row');
  const done = agentSteps([{ ...calls[0]!, ok: true }])[0]!;
  equal(agentState(done, false), 'done');
  const live = calls.map(({ ...call }) => ({ ...call, interrupted: false }));
  equal(helpersAtWork([{ id: 'live', role: 'assistant', at: 1, text: '', status: 'done', tools: live }]), 2, 'a live session still owns background work');
});
