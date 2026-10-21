import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const question = (id = 'q'): Message => ({ id, role: 'user', text: 'Fix the date parsing', at: 0 });
const answer = (extra: Partial<Message>, id = 'a'): Message => ({ id, role: 'assistant', text: 'Fixed.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', effort: 'high', ...extra });
const look = { reason: 'look.fixed' };

// The conversation's box lives outside the thread; a bare one stands in for it.
function mount(messages: Message[], onReply?: (text: string) => void) {
  const root = document.createElement('div');
  const composer = document.createElement('div');
  composer.className = 'composer';
  const box = document.createElement('textarea');
  composer.append(box);
  document.body.append(root, composer);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'look-check', options: [], onBranch: () => {}, onReply }), root);
  return { root, box, close: () => { dispose(); root.remove(); composer.remove(); } };
}

const offers = (root: HTMLElement) => [...root.querySelectorAll<HTMLButtonElement>('.actions .nudge')].filter((b) => b.textContent === t('answer.look'));

check('look around: a reply that fixed something offers to look for the same mistake elsewhere, and only starts the box', async () => {
  const sent: string[] = [];
  const offered = mount([question(), answer({ look })], (text) => sent.push(text));
  try {
    const buttons = offers(offered.root);
    equal(buttons.length, 1);
    equal(buttons[0]!.title, t('look.fixed'), 'the tooltip says why and what it does');
    buttons[0]!.click();
    ok(offered.box.value.startsWith(t('look.ask')), `the box holds the request: ${offered.box.value}`);
    buttons[0]!.click();
    equal(offered.box.value, t('look.ask'), 'a second click does not write it twice');
    equal(sent.length, 0, 'nothing is sent until the person sends it');
  } finally {
    offered.close();
  }

  const drafted = mount([question(), answer({ look })], () => {});
  try {
    const draft = '```ts\nconst a = 1;\n```';
    drafted.box.value = draft;
    offers(drafted.root)[0]!.click();
    equal(drafted.box.value, `${t('look.ask')}\n\n${draft}`, 'a draft already there stays a paragraph of its own');
    offers(drafted.root)[0]!.click();
    equal(drafted.box.value, `${t('look.ask')}\n\n${draft}`, 'and the request is not written twice');
  } finally {
    drafted.close();
  }

  const older = mount([question(), answer({ look }), question('q2'), answer({}, 'b')], () => {});
  try {
    equal(offers(older.root).length, 0, 'only the newest answer offers it');
  } finally {
    older.close();
  }

  const nudge = { reason: 'nudge.noCheck', vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' } as const;
  const both = mount([question(), answer({ look, nudge })], () => {});
  try {
    equal(offers(both.root).length, 0, 'never beside a step up');
    equal(both.root.querySelectorAll('.actions .nudge').length, 1);
  } finally {
    both.close();
  }

  const streaming = mount([question(), answer({ look, status: 'streaming' })], () => {});
  try {
    equal(offers(streaming.root).length, 0, 'nothing is offered while the answer is still being written');
  } finally {
    streaming.close();
  }

  const readOnly = mount([question(), answer({ look })]);
  try {
    equal(offers(readOnly.root).length, 0, 'a conversation that takes no message offers nothing');
  } finally {
    readOnly.close();
  }
});

check('look around: the places Faber found go into the tooltip and the request, for a fix or for text still elsewhere', async () => {
  const places = ['src/b.rs:2', 'src/deep/c.rs:3'];
  const fixed = mount([question(), answer({ look: { reason: 'look.fixed', places } })], () => {});
  try {
    const button = offers(fixed.root)[0]!;
    equal(button.title, `${t('look.fixed')} ${t('look.found', { n: 2 })}`, 'the fix says how many more places hold the line');
    button.click();
    equal(fixed.box.value, `${t('look.ask')} ${t('look.askPlaces', { places: 'src/b.rs:2, src/deep/c.rs:3' })}`);
  } finally {
    fixed.close();
  }

  const same = mount([question(), answer({ look: { reason: 'look.sameText', places: ['notes/later.md:3'] } })], () => {});
  try {
    const button = offers(same.root)[0]!;
    equal(button.title, t('look.sameText', { n: 1 }), 'text still elsewhere has its own words, in the singular for one place');
    ok(button.title !== t('look.sameText', { n: 2 }));
  } finally {
    same.close();
  }
});
