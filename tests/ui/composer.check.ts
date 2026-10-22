import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer, type Draft } from '../../ui/Composer.tsx';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

check('composer: wrapped text grows to 40vh, scrolls inside and shrinks after sending', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  let fill!: (text: string) => void;
  let sent = '';
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'A placeholder that wraps over multiple lines. '.repeat(8),
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    const initial = field.clientHeight;
    equal(initial, 60, 'a wrapped placeholder must not grow an empty field');
    // No newlines: wrapping must grow the field too.
    fill('A long prompt with several words. '.repeat(12));
    await settle();
    ok(field.clientHeight > initial, 'wrapped text grows the field');
    const long = 'A very long prompt. '.repeat(500);
    fill(long);
    await settle();
    ok(field.clientHeight <= innerHeight * 0.4 + 1, 'height stays within 40vh');
    ok(field.scrollHeight > field.clientHeight, 'overflow scrolls inside the field');
    field.scrollTop = field.scrollHeight;
    ok(field.scrollTop > 0, 'the overflow can actually be scrolled');
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, long);
    equal(field.value, '');
    equal(field.clientHeight, initial, 'sending restores the empty height');
    equal(field.scrollTop, 0, 'sending resets the internal scroll');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: typed text survives a reload and is forgotten once sent', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  let sent = '';
  const open = () => {
    const root = document.createElement('div');
    root.dataset.motion = 'none';
    document.body.append(root);
    let fill!: (text: string) => void;
    const dispose = render(() => createComponent(Composer, {
      draftKey: 'check-reload', vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
    }), root);
    return { root, fill, close: () => (dispose(), root.remove()) };
  };
  try {
    const first = open();
    await settle();
    first.fill('an unsent thought');
    await settle();
    first.close(); // the window goes away, as it does for an update
    const second = open();
    await settle();
    equal(second.root.querySelector('textarea')!.value, 'an unsent thought', 'the text is back after the reload');
    second.root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, 'an unsent thought');
    second.close();
    const third = open();
    await settle();
    equal(third.root.querySelector('textarea')!.value, '', 'a sent message is not restored');
    third.close();
  } finally {
    localStorage.removeItem('faber.unsent.check-reload');
  }
});

const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

check('composer: images show thumbnails and files small cards, each with its own x that removes only it', async () => {
  const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent: Draft | undefined;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
  }), root);
  try {
    await settle();
    equal(root.querySelector('.attach-row'), null, 'nothing attached, no row');
    const form = root.querySelector('form')!;
    const image = (name: string) => new File([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], name, { type: 'image/png' });
    const transfer = new DataTransfer();
    for (const file of [image('one.png'), image('two.png'), new File(['%PDF-1.4'], 'doc.pdf', { type: 'application/pdf' }), new File(['hello notes'], 'notes.txt', { type: 'text/plain' })]) transfer.items.add(file);
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    const thumbs = () => [...root.querySelectorAll<HTMLElement>('.attach-row .thumb')];
    // Reading the dropped files is asynchronous, and slower in WebKit: wait for them instead of a fixed time.
    for (let i = 0; i < 100 && thumbs().length < 4; i++) await settle(20);
    const nameOf = (el: HTMLElement) => el.querySelector('img')?.alt ?? el.querySelector('b')!.textContent;
    equal(thumbs().map(nameOf).join(), 'one.png,two.png,doc.pdf,notes.txt');
    equal(thumbs().filter((el) => el.querySelector('img')).length, 2, 'the pictures are thumbnails');
    equal(thumbs().filter((el) => el.classList.contains('file')).length, 2, 'the others are cards');
    for (const el of thumbs().filter((e) => e.querySelector('img'))) ok(el.getBoundingClientRect().width <= 64, 'a thumbnail is small');
    const field = root.querySelector('textarea')!;
    ok(field.value.includes('hello notes'), 'a small text file goes inside the message');
    const x = (name: string) => root.querySelector<HTMLButtonElement>(`.thumb-x[aria-label="${t('composer.removeOne', { name })}"]`)!;
    equal(root.querySelectorAll('.thumb-x').length, 4, 'each one has its own x');
    equal(getComputedStyle(x('two.png')).opacity, '0', 'shown only for the pointer or the keyboard');
    equal(getComputedStyle(x('two.png')).visibility, 'visible', 'so the keyboard still reaches it');
    // A hidden test window never has the focus or a pointer, so the rule that shows it is read.
    const rules = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]).filter((r): r is CSSStyleRule => r instanceof CSSStyleRule);
    ok(rules.some((r) => r.selectorText.includes('.thumb-x') && r.selectorText.includes(':focus-within') && r.style.opacity === '1'), 'and by hover or focus it shows');
    ok(root.querySelector('.attach-all'), 'with several, one quiet "remove all" remains');
    x('two.png').click();
    x('notes.txt').click();
    await settle();
    equal(thumbs().map(nameOf).join(), 'one.png,doc.pdf', 'only the ones pressed went');
    ok(!field.value.includes('hello notes'), 'a text file takes its words out of the message with it');
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle(400);
    equal(sent?.images.length, 1, 'one picture is sent');
    equal(sent?.files.map((f) => f.name).join(), 'doc.pdf');
    equal(root.querySelector('.attach-row'), null, 'and the row closes after sending');
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    await settle(150);
    root.querySelector<HTMLButtonElement>('.attach-all')!.click();
    await settle();
    equal(thumbs().length, 0, 'remove all clears the row');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: a rejected message comes back with its pictures, files and text files, each still with its own x', async () => {
  const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px';
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent: Draft | undefined;
  let restore!: (draft: Draft) => boolean;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
    onRestore: (put) => { restore = put; },
  }), root);
  try {
    await settle();
    const form = root.querySelector('form')!;
    const transfer = new DataTransfer();
    transfer.items.add(new File([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], 'shot.png', { type: 'image/png' }));
    transfer.items.add(new File(['hello notes'], 'notes.txt', { type: 'text/plain' }));
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    // Each state is waited for, not timed: a slow runner reads the dropped files later than a fixed pause.
    const thumbs = async (n: number) => {
      for (const start = Date.now(); root.querySelectorAll('.thumb').length !== n && Date.now() - start < 3000; ) await settle();
    };
    await thumbs(2);
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await thumbs(0);
    equal(root.querySelectorAll('.thumb').length, 0, 'sent, the box is empty');
    ok(restore(sent!), 'the screen puts the rejected message back');
    await settle();
    const names = [...root.querySelectorAll('.attach-row .thumb')].map((el) => el.querySelector('img')?.alt ?? el.querySelector('b')!.textContent);
    equal(names.join(), 'shot.png,notes.txt', 'with the names it had, the text file too');
    equal(root.querySelectorAll('.thumb-x').length, 2);
  } finally {
    dispose();
    root.remove();
  }
});
