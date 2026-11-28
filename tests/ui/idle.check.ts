import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Ambient } from '../../ui/Ambient.tsx';
import { appliedLook, applyLook, watchSystem } from '../../ui/appearance.ts';
import { Composer } from '../../ui/Composer.tsx';
import { keepRoot, sala } from './appearance.check.ts';
import { check, equal, ok } from './check.ts';

// The quiet life of the two homes (docs/design/looks.md, "Motion"): a typing bar and hints in the idle composer, a light behind
// each home, and none of it with reduced motion, with no motion, or while the window is hidden.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const HINTS = ['One', 'Two', 'Three'];

/** Runs `body` with the page wearing a look of this motion level, and puts the page back after. */
async function withMotion(motion: 'full' | 'reduced' | 'none', body: () => Promise<void>) {
  const restore = keepRoot();
  const saved = appliedLook();
  try {
    await applyLook(sala({ motion }));
    await body();
  } finally {
    if (saved) await applyLook(saved);
    restore();
  }
}

/** The colour a field draws its placeholder in. WebKit (macOS) answers a style query on `::placeholder` with the field's own
 * style, so when the answer is the field's own colour the cascade is read from the stylesheet: the last rule whose selector
 * reaches this field's placeholder (the rules that decide it share a specificity order with their place in the sheet). */
function placeholderColor(field: HTMLTextAreaElement): string {
  const asked = getComputedStyle(field, '::placeholder').color;
  if (asked !== getComputedStyle(field).color) return asked;
  let found = '';
  const walk = (rules: CSSRuleList) => {
    for (const rule of rules) {
      if (rule instanceof CSSStyleRule) {
        if (rule.selectorText.includes('::placeholder') && rule.style.color) {
          try {
            if (field.matches(rule.selectorText.replace(/::placeholders*$/, ''))) found = rule.style.color;
          } catch { /* a selector this engine cannot match is not this field's */ }
        }
      } else if ('cssRules' in rule) walk((rule as CSSGroupingRule).cssRules);
    }
  };
  for (const sheet of document.styleSheets) walk(sheet.cssRules);
  if (!found) return asked;
  const probe = document.createElement('span');
  probe.style.color = found;
  field.parentElement!.append(probe);
  const color = getComputedStyle(probe).color;
  probe.remove();
  return color;
}

async function withComposer(hints: string[] | undefined, body: (root: HTMLElement, field: HTMLTextAreaElement) => Promise<void>) {
  const root = document.createElement('div');
  root.className = 'welcome-grid';
  root.style.cssText = 'position:fixed;left:0;top:0;width:640px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'The plain placeholder', hints,
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
  }), root);
  try {
    await settle();
    await body(root, root.querySelector('textarea')!);
  } finally {
    dispose();
    root.remove();
  }
}

check('idle: the empty composer of a home draws a typing bar and hints, and never two carets at once', () => withMotion('full', () => withComposer(HINTS, async (root, field) => {
  ok(root.querySelector('.idle-box'), 'the idle composer has its hints');
  equal(root.querySelectorAll('.g-hint').length, 3);
  equal(root.querySelectorAll('.g-hint.on').length, 1, 'one hint shows at a time');
  equal(placeholderColor(field), 'rgba(0, 0, 0, 0)', 'the hints stand in for the placeholder, so it is not drawn twice');
  const caret = () => root.querySelectorAll('.g-caret').length;
  // The window has the keyboard and the field has focus: the real caret blinks, and the screen draws none.
  window.dispatchEvent(new Event('focus'));
  field.dispatchEvent(new Event('focus'));
  await settle();
  equal(caret(), 0, 'a focused field in the front window shows its own caret only');
  // The window loses the keyboard: the real caret goes, so the calm one comes back.
  window.dispatchEvent(new Event('blur'));
  await settle();
  equal(caret(), 1, 'an inactive window still shows a blinking bar');
  ok(getComputedStyle(root.querySelector('.g-caret')!).animationName.includes('blink'), 'and it blinks');
  window.dispatchEvent(new Event('focus'));
  field.dispatchEvent(new Event('blur'));
  await settle();
  equal(caret(), 1, 'a field without focus shows the bar');
  // Typing removes all of it, and the placeholder comes back for the field.
  field.value = 'x';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  equal(root.querySelector('.idle-box'), null, 'typed text leaves no hint and no bar');
  field.value = '';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  ok(root.querySelector('.idle-box'), 'an emptied field idles again');
})));

check('idle: the hint changes slowly, once per tick, and its timer ends with the composer', () => withMotion('full', async () => {
  const set = window.setInterval;
  const clear = window.clearInterval;
  const timers = new Map<number, { callback: () => void; ms: number }>();
  let next = 9000;
  window.setInterval = ((callback: () => void, ms: number) => (timers.set(++next, { callback, ms }), next)) as unknown as typeof setInterval;
  window.clearInterval = ((id: number) => void timers.delete(id)) as typeof clearInterval;
  try {
    await withComposer(HINTS, async (root) => {
      const hint = [...timers.values()].find((t) => t.ms >= 6000);
      ok(hint, 'one timer rotates the hints');
      ok(hint.ms >= 6000, 'slowly: not more than once every few seconds');
      const shown = () => [...root.querySelectorAll('.g-hint')].findIndex((h) => h.classList.contains('on'));
      equal(shown(), 0);
      hint.callback();
      await settle();
      equal(shown(), 1, 'the next hint takes over');
    });
    equal([...timers.values()].filter((t) => t.ms >= 6000).length, 0, 'closing the composer ends the timer');
  } finally {
    window.setInterval = set;
    window.clearInterval = clear;
  }
}));

check('idle: reduced motion and no motion render no hints and no bar, only the plain placeholder', async () => {
  for (const level of ['reduced', 'none'] as const) {
    await withMotion(level, () => withComposer(HINTS, async (root, field) => {
      equal(root.querySelector('.idle-box'), null, `${level}: nothing idles`);
      equal(root.querySelector('.g-caret'), null, `${level}: no bar`);
      equal(field.placeholder, 'The plain placeholder');
      ok(placeholderColor(field) !== 'rgba(0, 0, 0, 0)', `${level}: the placeholder is drawn`);
    }));
  }
});

check('idle: a hidden window renders no hints, has no timer left and pauses every CSS animation', () => withMotion('full', async () => {
  const root = document.documentElement;
  const stop = watchSystem();
  const hide = (value: boolean) => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  };
  const probe = document.createElement('span');
  probe.className = 'cursor';
  document.body.append(probe);
  try {
    await withComposer(HINTS, async (host) => {
      ok(host.querySelector('.idle-box'), 'shown: the composer idles');
      equal(getComputedStyle(probe).animationPlayState, 'running');
      hide(true);
      await settle();
      equal(host.querySelector('.idle-box'), null, 'hidden: no hint and no bar');
      equal(getComputedStyle(probe).animationPlayState, 'paused', 'hidden: even an animation the effects know nothing about stops');
      hide(false);
      await settle();
      ok(host.querySelector('.idle-box'), 'shown again: the composer idles again');
      equal(getComputedStyle(probe).animationPlayState, 'running');
    });
  } finally {
    probe.remove();
    Reflect.deleteProperty(document, 'hidden');
    root.classList.remove('hidden-win');
    stop();
  }
}));

check('idle: a look with no effects of its own gets a drifting light, off with the ambient switch and with reduced motion', async () => {
  const bare = (motion: 'full' | 'reduced', ambient: boolean) => {
    const look = sala({ motion });
    look.vars = { ...look.vars, 'amb-beam': 'none', 'amb-loom': 'none', 'amb-flow': 'none' };
    look.effects = { ambient, texture: true, glow: true };
    return look;
  };
  const restore = keepRoot();
  const saved = appliedLook();
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:500px';
  document.body.append(host);
  const dispose = render(() => createComponent(Ambient, { vendors: ['claude'], avoid: () => [], variant: 'chat' }), host);
  try {
    await applyLook(bare('full', true));
    await settle();
    const light = host.querySelector<HTMLElement>('.amb-drift')!;
    equal(getComputedStyle(light).display, 'block', 'a plain look has a light');
    equal(getComputedStyle(light).animationName, 'drift');
    await applyLook(bare('full', false));
    await settle();
    equal(getComputedStyle(light).display, 'none', 'the ambient switch turns it off');
    await applyLook(bare('reduced', true));
    await settle();
    equal(getComputedStyle(light).animationName, 'none', 'reduced motion stops it');
    await applyLook(sala());
    await settle();
    equal(getComputedStyle(light).display, 'none', 'a look with its own effect does not add a second light');
  } finally {
    dispose();
    host.remove();
    if (saved) await applyLook(saved);
    restore();
  }
});

check('idle: a project home has no beam of light, the chat home keeps its own', async () => {
  const restore = keepRoot();
  const saved = appliedLook();
  const beam = async (variant: 'chat' | 'project') => {
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
    document.body.append(host);
    const dispose = render(() => createComponent(Ambient, { vendors: [], avoid: () => [], variant }), host);
    try {
      await settle();
      return [...host.querySelectorAll<HTMLElement>('.amb-beam, .amb-glow')].filter((el) => el.getClientRects().length > 0).length;
    } finally {
      dispose();
      host.remove();
    }
  };
  try {
    await applyLook(sala());
    ok((await beam('chat')) > 0, 'the chat home has its beam');
    equal(await beam('project'), 0, 'a project screen is a place to work: no beam and no glow');
  } finally {
    if (saved) await applyLook(saved);
    restore();
  }
});
