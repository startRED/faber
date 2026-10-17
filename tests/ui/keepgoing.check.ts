import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
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
