import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import type { Message, VendorUsage } from '../../ui/api.ts';
import { keepGoingOutlook } from '../../ui/keep-going.ts';
import { KeepGoingNote } from '../../ui/KeepGoingNote.tsx';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('keep going: the project composer offers it as a chip that says whether it is on', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [on, setOn] = createSignal<boolean | undefined>(false);
  const asked: boolean[] = [];
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'claude' }, busy: true, placeholder: 'Write here',
    get keepGoing() { return on(); },
    onKeepGoing: (value) => { asked.push(value); setOn(value); },
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
  }), root);
  try {
    await settle();
    const chip = () => root.querySelector<HTMLButtonElement>('button.keep');
    ok(chip(), 'the chip is there in a conversation under way');
    equal(chip()!.getAttribute('aria-pressed'), 'false');
    equal(chip()!.textContent?.trim(), t('composer.keepGoing'));
    chip()!.click();
    await settle();
    deepEqual(asked, [true], 'a click turns it on');
    equal(chip()!.getAttribute('aria-pressed'), 'true', 'on, it says so');
    equal(chip()!.title, t('composer.keepGoing.on'), 'and how it ends');
    ok(root.querySelector('button.stop'), 'the same Stop ends a turn that runs on its own');
    setOn(undefined);
    await settle();
    equal(chip(), null, 'not offered where keep going does not apply (Chat, a new conversation)');
  } finally {
    dispose();
    root.remove();
  }
});

check('keep going: Faber\'s own message is a quiet line, and a stop says why on the reply', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const mine: Message = { id: 'u1', role: 'user', text: 'Fix the login page', at: 1 };
  const first: Message = { id: 'a1', role: 'assistant', text: 'Fixed.', at: 2, status: 'done', vendor: 'codex' };
  const woke: Message = { id: 'u2', role: 'user', text: 'The person is away and asked you to keep going…', at: 3, wake: { why: 'wake.keepGoing' } };
  const last: Message = { id: 'a2', role: 'assistant', text: 'All done.', at: 4, status: 'done', vendor: 'codex', halt: 'reason.keepGoing.pace' };
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'conv-keep', messages: [mine, first, woke, last], from: 0, options: [], onBranch: () => {},
  }), root);
  try {
    await settle();
    equal(root.querySelectorAll('.you').length, 1, 'only the person\'s own words are a bubble');
    const lines = [...root.querySelectorAll('.woke')].map((el) => el.textContent?.trim());
    deepEqual(lines, [t('wake.keepGoing'), t('reason.keepGoing.pace', { ai: 'Codex' })]);
  } finally {
    dispose();
    root.remove();
  }
});

const usage = (vendor: 'claude' | 'codex', state: VendorUsage['state'], more: Partial<VendorUsage> = {}): VendorUsage => ({
  vendor, state, learning: false, relearning: false, foreignUse: false, overageOn: false, billedPerUse: false, billingUnknown: false,
  askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0, counting: false, windows: [], ...more,
});

check('keep going: poor quota is read across every AI, and one AI with room means no warning', () => {
  equal(keepGoingOutlook([]), undefined, 'nothing known, nothing said');
  equal(keepGoingOutlook([usage('claude', 'out'), usage('codex', 'ok')]), undefined, 'it moves to the AI with room');
  deepEqual(keepGoingOutlook([usage('claude', 'low'), usage('codex', 'low')]), { kind: 'low' });
  deepEqual(keepGoingOutlook([usage('claude', 'low'), usage('codex', 'fast')]), { kind: 'fast' });
  deepEqual(keepGoingOutlook([usage('claude', 'fast'), usage('codex', 'out')]), { kind: 'fast' }, 'an AI that is out does not count');
  const full = (resetsAt: number) => [{ name: 'seven_day', used: 1, elapsed: 0, expected: 0, resetsAt, readAt: 0 }];
  deepEqual(keepGoingOutlook([usage('claude', 'out', { windows: full(500) }), usage('codex', 'out', { windows: full(300) })]), { kind: 'out', when: 300 }, 'the first AI back');
  deepEqual(keepGoingOutlook([usage('claude', 'out')]), { kind: 'out', when: undefined });
  deepEqual(keepGoingOutlook([usage('claude', 'out', { billedPerUse: true }), usage('codex', 'unknown', { billingUnknown: true })]), { kind: 'billed' });
  deepEqual(keepGoingOutlook([usage('claude', 'out'), usage('codex', 'ok', { billedPerUse: true })]), { kind: 'out', when: undefined }, 'an AI billed per use is never the way out');
});

check('keep going: with poor quota the line asks before it turns on, and while it is on it stays in view', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [on, setOn] = createSignal(false);
  const [asking, setAsking] = createSignal(false);
  const [warning, setWarning] = createSignal<string | undefined>('Quota is low');
  const dispose = render(() => createComponent(KeepGoingNote, {
    get warning() { return warning(); },
    get on() { return on(); },
    get asking() { return asking(); },
    onYes: () => (setAsking(false), setOn(true)),
    onNo: () => setAsking(false),
  }), root);
  try {
    await settle();
    const note = () => root.querySelector<HTMLElement>('.kg-note');
    equal(note(), null, 'nothing is said before the person reaches for it');
    setAsking(true);
    await settle();
    ok(note()?.textContent?.includes('Quota is low'), 'the chip asks, and says why');
    equal(root.querySelector('.kg-yes')?.textContent, t('keepGoing.warn.anyway'));
    root.querySelector<HTMLButtonElement>('.kg-no')!.click();
    await settle();
    equal(note(), null, 'not now leaves it off');
    equal(on(), false);
    setAsking(true);
    await settle();
    root.querySelector<HTMLButtonElement>('.kg-yes')!.click();
    await settle();
    equal(on(), true, 'going on anyway turns it on');
    ok(note()?.textContent?.includes('Quota is low'), 'on, the warning stays where the person looks');
    equal(root.querySelector('.kg-yes'), null, 'without the question');
    setWarning('Every AI is out of quota');
    await settle();
    ok(note()?.textContent?.includes('Every AI is out'), 'quota that turns poor after it was on shows up too');
    setWarning(undefined);
    await settle();
    equal(note(), null, 'and the line goes once the quota is fine');
  } finally {
    dispose();
    root.remove();
  }
});
