import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer, mentionAt, mentionText } from '../../ui/Composer.tsx';
import type { Mention } from '../../ui/api.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('mention: an @ at a word start, before the caret, is a mention', () => {
  deepEqual(mentionAt('look at @src/pa', 15), { at: 8, query: 'src/pa' });
  deepEqual(mentionAt('@', 1), { at: 0, query: '' });
  equal(mentionAt('mail me at a@b.c', 16), undefined, 'an address is not a mention');
  equal(mentionAt('@src/parser.rs and', 18), undefined, 'a finished path followed by words is not');
  deepEqual(mentionAt('see @"My Folder/no', 18), { at: 4, query: 'My Folder/no' }, 'a quote keeps a path with spaces open');
  equal(mentionText('My Folder/', true), '@"My Folder/');
  equal(mentionText('My Folder/a b.txt', false), '@"My Folder/a b.txt" ');
  equal(mentionText('src/', true), '@src/');
  equal(mentionText('a@b/', true), '@"a@b/');
  deepEqual(mentionAt('@"a@b/', 6), { at: 0, query: 'a@b/' }, 'what is written is read back');
});

check('mention: @ lists the folder, filters as you type, inserts the path and closes with Esc', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;bottom:0;width:600px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  const folder: Mention[] = [
    { path: 'src/', dir: true },
    { path: 'README.md', dir: false },
    { path: 'src/parser.rs', dir: false },
    { path: 'src/lexer.rs', dir: false },
  ];
  const asked: string[] = [];
  let sent = '';
  let escaped = 0;
  const onWindowEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') escaped++; };
  window.addEventListener('keydown', onWindowEscape);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Write',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; },
    onMention: async (query) => {
      asked.push(query);
      if (slow) await new Promise((resolve) => setTimeout(resolve, 600));
      if (failing) throw new Error('no folder');
      return folder.filter((m) => m.path.toLowerCase().includes(query.toLowerCase()));
    },
  }), root);
  let slow = false;
  let failing = false;
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = () => root.querySelector('textarea')!;
  const type = (text: string) => {
    const f = field();
    f.value = text;
    f.setSelectionRange(text.length, text.length);
    f.dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const press = (key: string) => field().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
  const shown = () => [...root.querySelectorAll('.mentions [role="option"]')].map((li) => li.textContent);
  try {
    await settle();
    type('fix @');
    await settle();
    deepEqual(shown(), ['src/', 'README.md', 'src/parser.rs', 'src/lexer.rs'], 'an @ lists the folder');
    equal(field().getAttribute('aria-activedescendant'), 'mention-0');

    type('fix @pars');
    await settle();
    deepEqual(shown(), ['src/parser.rs'], 'it filters as you type');
    press('Enter');
    await settle();
    equal(field().value, 'fix @src/parser.rs ', 'Enter puts the path in the message');
    equal(sent, '', 'and sends nothing');
    equal(root.querySelector('.mentions'), null, 'the list closes once a file is in');

    type('fix @src/parser.rs and @');
    await settle();
    press('ArrowDown');
    await settle();
    root.querySelectorAll<HTMLElement>('.mentions [role="option"]')[3]!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    await settle();
    equal(field().value, 'fix @src/parser.rs and @src/lexer.rs ', 'a click puts that path in');

    type('fix @R');
    await settle();
    ok(root.querySelector('.mentions'), 'the list is open again');
    press('Escape');
    await settle();
    equal(root.querySelector('.mentions'), null, 'Esc closes it');
    equal(escaped, 0, 'and the window\'s own Esc never hears it');
    equal(field().value, 'fix @R', 'the words stay as typed');
    press('Enter');
    await settle();
    equal(sent, 'fix @R', 'with the list closed, Enter sends again');
    ok(asked.includes('pars'), 'the engine is asked for what follows the @');

    sent = '';
    type('fix @R');
    await settle();
    root.querySelector<HTMLElement>('.mentions [role="option"]')!.dispatchEvent(new MouseEvent('mousedown', { button: 2, bubbles: true, cancelable: true }));
    await settle();
    equal(field().value, 'fix @R', 'a right click puts nothing in');
    const back = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true });
    field().dispatchEvent(back);
    equal(field().value, 'fix @R', 'Shift+Tab puts nothing in (it moves focus back)');
    field().setSelectionRange(0, 0);
    field().dispatchEvent(new KeyboardEvent('keyup', { key: 'Home', bubbles: true }));
    await settle();
    equal(root.querySelector('.mentions'), null, 'moving the caret away from the @ closes the list');
    press('Enter');
    equal(sent, 'fix @R', 'and Enter sends the words as they are');

    sent = '';
    slow = true;
    type('go @');
    await settle();
    press('Enter');
    await settle();
    equal(sent, '', 'while the answer is on its way, Enter sends nothing');
    equal(field().value, 'go @');
    await settle();
    await settle();
    ok(root.querySelector('.mentions.waiting[role="status"]'), 'a slow answer shows a waiting line');
    await new Promise((resolve) => setTimeout(resolve, 500));
    equal(shown().length, 4, 'and the list shows when it comes');
    equal(root.querySelector('.mentions.waiting'), null);

    slow = false;
    failing = true;
    type('go @x');
    await settle();
    press('Enter');
    equal(sent, 'go @x', 'a failed answer leaves Enter free to send');
  } finally {
    window.removeEventListener('keydown', onWindowEscape);
    dispose();
    root.remove();
  }
});

check('mention: with no working folder, @ is only a character', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Write',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
  }), root);
  try {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const field = root.querySelector('textarea')!;
    field.value = '@';
    field.setSelectionRange(1, 1);
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(root.querySelector('.mentions'), null);
  } finally {
    dispose();
    root.remove();
  }
});
