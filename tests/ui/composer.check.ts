import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
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
