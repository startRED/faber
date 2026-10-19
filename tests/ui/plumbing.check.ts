import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { toolLine } from '../../ui/worklog.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// The 2026-10-19 design (docs/superpowers/specs/2026-10-19-legible-agent-runs-design.md): the conversation shows what the agent
// did, where the work stands and what needs the person; Faber's own plumbing never takes a row, and a reply that is still at
// work says what it is doing instead of blinking.
const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const mine: Message = { id: 'u1', role: 'user', text: 'Fix the login page', at: 1 };
const reply = (id: string, over: Partial<Message> = {}): Message => ({ id, role: 'assistant', text: 'Fixed.', at: 2, status: 'done', vendor: 'claude', ...over });
const wake = (id: string): Message => ({ id, role: 'user', text: 'The person is away and asked you to keep going…', at: 3, wake: { why: 'wake.keepGoing' } });

function mount(messages: () => Message[]) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { get messages() { return messages(); }, from: 0, conversationId: 'plumbing', options: [], onBranch: () => {} }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('plumbing: a turn Faber started and cut with nothing in it is not drawn, and only the person\'s own Stop is said', async () => {
  const view = mount(() => [mine, reply('a1'), wake('w1'), reply('a2', { text: '', status: 'stopped' }), wake('w2'), reply('a3', { text: 'More.', status: 'stopped' })]);
  try {
    await settle();
    equal(view.root.querySelectorAll('.ai').length, 2, 'the empty reply to Faber\'s message is gone');
    equal(view.root.querySelectorAll('.woke').length, 0, 'no row says Faber asked it to keep going');
    equal(view.root.querySelector('.ai .note'), null, 'a turn Faber started says nothing about being cut');
  } finally {
    view.close();
  }
  const own = mount(() => [mine, reply('a1', { status: 'stopped' })]);
  try {
    await settle();
    equal(own.root.querySelector('.ai .note')!.textContent, t('answer.stopped'), 'the person\'s Stop is still said');
  } finally {
    own.close();
  }
});

check('plumbing: a meter gap is a "≈" before the cost, and a history sent again is a mark in the header, never a line', async () => {
  const view = mount(() => [mine, reply('a1', { costUsd: 0.25, incomplete: true, note: 'note.historyResent' })]);
  try {
    await settle();
    const cost = view.root.querySelector<HTMLElement>('.ai .cost')!;
    ok(cost.textContent!.startsWith('≈'), 'the number says it is a lower bound');
    ok(cost.title.includes(t('answer.meterIncomplete')), 'why is in the tooltip');
    ok(!view.root.textContent!.includes(t('answer.meterIncomplete')), 'and nowhere on screen');
    equal(view.root.querySelector('.ai .model .by-faber')!.getAttribute('aria-label'), t('note.historyResent'));
    equal(view.root.querySelector('.ai .pace'), null, 'no line for it');
  } finally {
    view.close();
  }
  const bare = mount(() => [mine, reply('a1', { incomplete: true })]);
  try {
    await settle();
    const cost = bare.root.querySelector<HTMLElement>('.ai .cost')!;
    equal(cost.textContent, '≈', 'with no number the mark still shows');
    ok(cost.title.includes(t('answer.meterIncomplete')), 'and still says why');
  } finally {
    bare.close();
  }
});

check('plumbing: a failed check that brought back no lines says so instead of an empty box', async () => {
  const checks = { state: 'failed' as const, passed: 0, failed: [{ command: 'cargo t --claims', lines: [] }] };
  const view = mount(() => [mine, reply('a1', { checks })]);
  try {
    await settle();
    equal(view.root.querySelector('.checks pre'), null);
    equal(view.root.querySelector('.checks-failed p')!.textContent, t('checks.noDetails'));
  } finally {
    view.close();
  }
});

check('plumbing: once its words stop, a reply still at work shows its step under them instead of a blinking caret', async () => {
  const start = Date.now();
  const command = { id: 't1', name: 'Bash', input: { command: 'sleep 540' }, started: start };
  const [m, setM] = createSignal<Message>(reply('a1', { status: 'streaming', text: 'I put it in the plan.', at: start }));
  const view = mount(() => [mine, m()]);
  const caret = () => view.root.querySelector('.ai .md .cursor');
  const after = () => view.root.querySelector<HTMLElement>('.ai .activity-latest.after .step')?.textContent;
  try {
    await settle();
    ok(caret(), 'words just arrived: the caret blinks');
    equal(after(), undefined);
    setM({ ...m(), tools: [command] });
    await settle();
    equal(caret(), null, 'a step runs: no caret');
    await settle(1_600);
    equal(after(), toolLine(command), 'the step sits under the words');
    const read = { id: 't2', name: 'Read', input: { file_path: 'src/login.ts' }, started: start, ok: true };
    setM({ ...m(), tools: [command, read] });
    await settle();
    equal(after(), toolLine(command), 'a later step that ended does not hide the one still running');
    equal(caret(), null);
    setM({ ...m(), tools: [{ ...command, ok: true }, read] });
    await settle();
    equal(after(), t('activity.thinkingNow'), 'between steps it says it is thinking');
    equal(caret(), null, 'still no caret: no words are arriving');
    setM({ ...m(), text: `${m().text}\n\nDone.` });
    await settle();
    ok(caret(), 'new words: the caret is back');
    equal(after(), undefined, 'and the line steps aside');
    setM({ ...m(), finishedAt: Date.now() });
    await settle();
    deepEqual([caret(), after()], [null, undefined], 'finished: nothing says it is still at work');
  } finally {
    view.close();
  }
});

check('plumbing: Keep going\'s stop line never shows, not even while it arrives', async () => {
  const [m, setM] = createSignal<Message>(reply('a1', { status: 'streaming', text: 'All done.\n\n[keep go', at: Date.now() }));
  const view = mount(() => [mine, m()]);
  const shown = () => view.root.querySelector('.ai .md')!.textContent!;
  try {
    await settle();
    ok(!shown().includes('[keep'), `a line still arriving: ${shown()}`);
    setM({ ...m(), text: 'Checked it.\n** [keep going: stop] **\nThe hook said ok.' });
    await settle();
    ok(!shown().includes('keep going') && shown().includes('The hook said ok.'), `a line of its own mid-reply: ${shown()}`);
    setM({ ...m(), text: 'The line [keep going: stop] is for Faber, then more' });
    await settle();
    ok(shown().includes('[keep going: stop]'), 'inside a sentence it is words');
    setM({ ...m(), status: 'done', text: 'The link is:\n[keep going:' });
    await settle();
    ok(shown().includes('[keep going:'), 'once the reply is done, a last line that only began like it is words');
  } finally {
    view.close();
  }
});
