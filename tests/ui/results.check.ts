import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message, type Results, type Summary } from '../../ui/api.ts';
import { ProjectResults } from '../../ui/Results.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, ok, equal } from './check.ts';

const empty = (): Results => ({
  delivered: { files: 0, edits: 0, checksRun: 0, checksPassed: 0, checksFailed: 0, commits: 0, prs: 0, artifacts: 0, answers: 0, concluded: false },
  cost: { turns: 12, workMs: 10800000, spanMs: 10800000, usd: 4.2, inputTokens: 0, outputTokens: 0, unpriced: 0, byVendor: { claude: { turns: 12, workMs: 10800000, usd: 4.2, outputTokens: 0 } } },
  dry: { turns: 0, byItself: 0, workMs: 0, usd: 0 },
});

const until = async (find: () => unknown) => {
  for (let n = 0; n < 100 && !find(); n++) await new Promise((resolve) => setTimeout(resolve, 20));
};

check('results: the last project reply shows what was delivered and what it cost, and a dry run says so', async () => {
  const delivered: Results = { ...empty(), delivered: { ...empty().delivered, files: 3, checksPassed: 2, commits: 1 } };
  const dry: Results = { ...empty(), dry: { turns: 12, byItself: 12, workMs: 10800000, usd: 4.2 } };
  let current = delivered;
  const oldResults = api.results;
  const oldOn = api.on;
  api.results = async () => current;
  api.on = async () => () => {};
  const [messages, setMessages] = createSignal<Message[]>([]);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { get messages() { return messages(); }, from: 0, conversationId: 'results-check', area: 'project', options: [], onBranch: () => {} }), root);
  try {
    equal(root.querySelector('[data-results-note]'), null, 'no reply, no note');
    setMessages([{ id: 'reply', role: 'assistant', at: 1, status: 'done', text: 'Done' }]);
    await until(() => root.querySelector('[data-results-note]'));
    const note = root.querySelector('[data-results-note]');
    ok(note, 'the last reply has a note');
    const text = note.textContent ?? '';
    ok(text.includes('3 files changed') && text.includes('2 checks passed') && text.includes('1 commit'), `delivered words: ${text}`);
    ok(text.includes('$4.20'), 'API price');
    ok(!text.includes('Claude $4.20'), 'one vendor is not listed again after the total');
    ok(!text.includes('delivered nothing'), 'no dry line when something was delivered');
    current = dry;
    setMessages([{ id: 'reply2', role: 'assistant', at: 2, status: 'done', text: 'Waiting' }]);
    await until(() => root.querySelector('[data-results-note]')?.textContent?.includes('delivered nothing'));
    const quiet = root.querySelector('[data-results-note]')?.textContent ?? '';
    ok(quiet.includes('Nothing delivered yet'), `nothing delivered, once: ${quiet}`);
    ok(quiet.includes('The last 12 turns delivered nothing'), 'dry run');
    ok(!quiet.includes('12 turns and'), 'the count is not said twice');
    setMessages([{ id: 'streaming', role: 'assistant', at: 3, status: 'streaming', text: '' }]);
    await new Promise((resolve) => setTimeout(resolve, 60));
    equal(root.querySelector('[data-results-note]')?.textContent?.includes('delivered nothing') ?? false, false, 'a reply that is still running shows no note of its own');
  } finally {
    dispose(); root.remove(); api.results = oldResults; api.on = oldOn;
  }
});

check('results: a chat conversation shows no note, and a project lists only conversations with turns', async () => {
  const row = (id: string, title: string, usd: number, turns: number) => ({ id, title, results: { ...empty(), cost: { ...empty().cost, usd, turns } } });
  const oldProject = api.projectResults;
  api.projectResults = async () => ({ total: empty(), conversations: [row('a', 'Cheap one', 1, 3), row('b', 'Costly one', 9, 5), row('c', 'Empty one', 0, 0)] });
  const root = document.createElement('div');
  document.body.append(root);
  const summaries = [{ id: 'a', projectId: 'p' }] as Summary[];
  const dispose = render(() => createComponent(ProjectResults, { projectId: 'p', summaries }), root);
  try {
    await until(() => root.querySelector('section'));
    const rows = [...root.querySelectorAll('p.note')].map((p) => p.textContent ?? '');
    ok(rows[0]?.startsWith('Total'), 'the total is first');
    equal(rows.length, 3, 'total and two conversations');
    ok(rows[1]?.startsWith('Costly one'), 'most spent first');
    ok(!rows.some((r) => r.includes('Empty one')), 'a conversation with no turn is left out');
    equal(root.querySelector('section')?.hasAttribute('aria-label'), false, 'the heading names the section once');
  } finally {
    dispose(); root.remove(); api.projectResults = oldProject;
  }
  const note = document.createElement('div');
  document.body.append(note);
  const noNote = render(() => createComponent(Thread, { messages: [{ id: 'r', role: 'assistant', at: 1, status: 'done', text: 'Hi' }], from: 0, conversationId: 'chat-check', area: 'chat', options: [], onBranch: () => {} }), note);
  try {
    await new Promise((resolve) => setTimeout(resolve, 60));
    equal(note.querySelector('[data-results-note]'), null, 'Chat has no results note');
  } finally {
    noNote(); note.remove();
  }
});
