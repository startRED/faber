import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { ImageViewerHost } from '../../ui/ImageViewer.tsx';
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

check('composer: a dropped image shows as a thumbnail with its own remove button, not as a name', async () => {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent = 0;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.images.length; },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 60));
  // Reading a dropped file takes as long as the system's file reader does: a slow runner needs more than one pause.
  const drawn = async (n: number) => {
    for (let i = 0; i < 50 && root.querySelectorAll('.attach-thumb img').length !== n; i++) await settle();
  };
  try {
    const form = root.querySelector('form')!;
    const drop = new Event('drop', { bubbles: true, cancelable: true });
    const files = new DataTransfer();
    for (const name of ['one.png', 'two.png']) files.items.add(new File([new Uint8Array([137, 80, 78, 71])], name, { type: 'image/png' }));
    Object.defineProperty(drop, 'dataTransfer', { value: files });
    form.dispatchEvent(drop);
    await drawn(2);
    equal(root.querySelectorAll('.attach-thumb img').length, 2, 'each image is drawn');
    ok(!root.textContent!.includes('one.png'), 'an image is not named in a chip');
    root.querySelector<HTMLButtonElement>('.attach-remove')!.click();
    await settle();
    equal(root.querySelectorAll('.attach-thumb img').length, 1, 'its button takes out only that image');
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, 1, 'only the image left goes out');
    equal(root.querySelectorAll('.attach-thumb').length, 0, 'sending clears the thumbnails');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: a click on a thumbnail opens the image large, and Escape closes it', async () => {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => [
    createComponent(Composer, {
      vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    }),
    createComponent(ImageViewerHost, {}),
  ], root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 60));
  try {
    const drop = new Event('drop', { bubbles: true, cancelable: true });
    const files = new DataTransfer();
    files.items.add(new File([new Uint8Array([137, 80, 78, 71])], 'one.png', { type: 'image/png' }));
    Object.defineProperty(drop, 'dataTransfer', { value: files });
    root.querySelector('form')!.dispatchEvent(drop);
    for (let i = 0; i < 50 && !root.querySelector('.pic'); i++) await settle();
    equal(root.querySelector('.viewer'), null, 'nothing is large before a click');
    root.querySelector<HTMLButtonElement>('.pic')!.click();
    await settle();
    ok(root.querySelector('.viewer[role="dialog"] img'), 'the click opens the image large');
    root.querySelector('.viewer-scrim')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    equal(root.querySelector('.viewer'), null, 'Escape closes it');
    ok(root.querySelector('.attach-thumb'), 'the thumbnail stays in the box');
  } finally {
    dispose();
    root.remove();
  }
});
