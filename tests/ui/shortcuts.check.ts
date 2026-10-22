import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { t } from '../../ui/i18n.ts';
import { cap, keysOf, pressed, type Shortcut, SHORTCUTS } from '../../ui/keys.ts';
import { neighbour } from '../../ui/sidebar.ts';
import { Shortcuts, shortcutsOpen } from '../../ui/Shortcuts.tsx';
import { mac } from '../../ui/WindowControls.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// Parity K1: one list of the shortcuts, opened with "?", and no key that works without being on it.
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const key = (k: string, init: KeyboardEventInit = {}) => new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init });
const press = (target: EventTarget, k: string, init: KeyboardEventInit = {}) => target.dispatchEvent(key(k, init));

check('shortcuts: "?" opens the list with every key of ui/keys.ts, and Esc closes it and gives focus back', async () => {
  const root = document.createElement('div');
  const before = document.createElement('button');
  document.body.append(root, before);
  const dispose = render(() => createComponent(Shortcuts, { enabled: true }), root);
  try {
    before.focus();
    press(before, '?');
    await settle();
    ok(shortcutsOpen(), 'opened by "?"');
    const sheet = document.querySelector('.keys-sheet')!;
    equal(sheet.getAttribute('role'), 'dialog');
    equal(sheet.querySelector('h2')!.textContent, t('keys.title'));
    const rows = [...sheet.querySelectorAll('.keys-row')].map((row) => [row.querySelector('dt')!.textContent, [...row.querySelectorAll('kbd')].map((k) => k.textContent).join(' ')]);
    deepEqual(rows, (Object.values(SHORTCUTS) as Shortcut[]).map((s) => [t(s.label), [...s.caps, ...(s.also ?? [])].map(cap).join(' ')]));
    // A Ctrl key of the screen underneath waits while the list is on top, and Tab stays in the list.
    ok(!press(document.activeElement!, 'n', { ctrlKey: true }), 'Ctrl N is held back');
    let below = 0;
    const count = () => below++;
    window.addEventListener('keydown', count);
    press(document.activeElement!, 'Tab');
    window.removeEventListener('keydown', count);
    equal(below, 0, 'Tab never reaches the shell underneath');
    ok(sheet.contains(document.activeElement), 'focus stays in the list');
    // An Esc a right-click menu over the list already answered leaves the list open.
    const answered = key('Escape');
    answered.preventDefault();
    document.activeElement!.dispatchEvent(answered);
    ok(shortcutsOpen(), 'the menu takes its own Esc');
    press(document.activeElement!, 'Escape');
    await settle();
    ok(!shortcutsOpen(), 'closed by Esc');
    equal(document.activeElement, before, 'focus goes back');
    // A "?" typed with AltGr opens it too (a Mac has no AltGr, and its WebKit sets no AltGraph state); in a text box "?"
    // is a letter like any other.
    if (!mac) {
      press(before, '?', { ctrlKey: true, altKey: true, modifierAltGraph: true } as KeyboardEventInit);
      await settle();
      ok(shortcutsOpen(), 'AltGr ?');
      press(document.activeElement!, 'Escape');
      await settle();
    }
    const box = document.createElement('textarea');
    root.append(box);
    box.focus();
    press(box, '?');
    await settle();
    ok(!shortcutsOpen(), '"?" in a text box types');
  } finally {
    dispose();
    root.remove();
    before.remove();
  }
});

check('shortcuts: a key keeps the modifiers it always took', () => {
  ok(pressed(key('N', { ctrlKey: true, shiftKey: true }), 'new'), 'Ctrl+Shift+N still opens a conversation');
  ok(pressed(key(',', { ctrlKey: true, shiftKey: true }), 'settings'), 'a comma typed with Shift still opens Settings');
  ok(!pressed(key('k', { ctrlKey: true, shiftKey: true }), 'palette'), 'Ctrl+Shift+K is not Ctrl+K');
  ok(pressed(key('k', { metaKey: true }), 'palette'), '⌘K on a Mac');
  // A key that takes Shift is another key without it.
  ok(pressed(key('D', { ctrlKey: true, shiftKey: true }), 'changes'), 'Ctrl+Shift+D is the changes');
  ok(!pressed(key('d', { ctrlKey: true }), 'changes'), 'Ctrl+D is not');
  ok(pressed(key('B', { ctrlKey: true, shiftKey: true }), 'browser'), 'Ctrl+Shift+B is the browser');
  ok(!pressed(key('B', { ctrlKey: true, shiftKey: true }), 'focus'), 'and never focus mode, which is Ctrl+B alone');
});

/** The statements that handle a Ctrl key by themselves instead of asking `pressed` (ui/keys.ts): such a key would work
 * without being on the list. A Ctrl shortcut is one character (Ctrl N, Ctrl ,); Tab, Esc and the arrows are not. A
 * condition that runs over several lines is read as one. */
function unlisted(source: string): string[] {
  const statements: string[] = [];
  for (const line of source.split('\n')) {
    const last = statements.at(-1);
    if (last !== undefined && (/(?:&&|\|\||\()\s*$/.test(last) || /^\s*(?:&&|\|\||\))/.test(line))) statements[statements.length - 1] = `${last} ${line.trim()}`;
    else statements.push(line);
  }
  const ctrl = /(?<![!\w.])(?:ctrl|e\.ctrlKey|e\.metaKey)\b(?!\s*=[^=])/;
  const oneKey = /\.key(?:\.toLowerCase\(\))?\s*===\s*'.'|'.'\s*===\s*\w+\.key|\]\.includes\(\s*(?:\w+\.key|key\b)/;
  return statements.filter((line) => ctrl.test(line) && oneKey.test(line));
}

check('shortcuts: no Ctrl key works without being on the list', async () => {
  // The planted twins: handlers that define Ctrl J on their own are found, and one that asks the list is not.
  for (const twin of [
    "      if (ctrl && e.key.toLowerCase() === 'j') jump();",
    "      if (e.ctrlKey && !e.altKey && e.key === 'j') jump();",
    "      if (e.key === 'j' && ctrl) jump();",
    "      if (ctrl && ['j'].includes(e.key.toLowerCase())) jump();",
    "      if (\n        ctrl &&\n        e.key === 'j'\n      ) jump();",
  ]) {
    equal(unlisted(twin).length, 1, twin);
  }
  equal(unlisted("      if (pressed(e, 'new')) {").length, 0);
  equal(unlisted("      } else if (e.key === 'Tab' && !ctrl && !e.altKey) {").length, 0);
  const files = import.meta.glob('../../ui/**/*.{ts,tsx}', { query: '?raw', import: 'default' }) as Record<string, () => Promise<string>>;
  const found: string[] = [];
  for (const [path, load] of Object.entries(files)) {
    if (path.endsWith('/keys.ts')) continue;
    for (const line of unlisted(await load())) found.push(`${path.replace('../../', '')}: ${line.trim()}`);
  }
  ok(Object.keys(files).length > 20, `the scan found only ${Object.keys(files).length} files`);
  ok(found.length === 0, found.join('\n'));
  // The sidebar's caps come from the same list.
  ok(/^(Ctrl|Strg) N$|^⌘N$/.test(keysOf('new')), keysOf('new'));
});

check('shortcuts: Ctrl 1, 2, 3 and Alt Up, Down are on the list, and Up alone and Ctrl Up are not them', () => {
  ok(pressed(key('1', { ctrlKey: true }), 'chat') && pressed(key('2', { metaKey: true }), 'projects') && pressed(key('3', { ctrlKey: true }), 'catalog'));
  ok(pressed(key('&', { ctrlKey: true, code: 'Digit1' }), 'chat'), 'a French keyboard types & on the 1');
  ok(!pressed(key('1', { ctrlKey: true, altKey: true }), 'chat') && !pressed(key('1'), 'chat'), 'Ctrl Alt 1 and a bare 1 are not Chat');
  ok(pressed(key('ArrowUp', { altKey: true }), 'previous') && pressed(key('ArrowDown', { altKey: true }), 'next'));
  ok(!pressed(key('ArrowUp'), 'previous') && !pressed(key('ArrowUp', { ctrlKey: true }), 'previous') && !pressed(key('ArrowUp', { altKey: true, shiftKey: true }), 'previous'), 'the message box keeps its Up');
  ok(/^(Ctrl|Strg) 2$|^⌘2$/.test(keysOf('projects')) && /^Alt ↑$|^⌥↑$/.test(keysOf('previous')), `${keysOf('projects')} ${keysOf('previous')}`);
});

check('shortcuts: Alt Up and Down walk the sidebar order of the open list, skipping archived ones and stopping at the ends', () => {
  const now = Date.parse('2026-10-22T12:00:00');
  const c = (id: string, area: string, ago: number, extra = {}) => ({ id, area, updatedAt: now - ago * 3_600_000, ...extra });
  const all = [c('old', 'chat', 48), c('new', 'chat', 1), c('pin', 'chat', 30, { pinnedAt: 1 }), c('gone', 'chat', 2, { archived: true }), c('p1', 'project', 1, { projectId: 'a' }), c('p2', 'project', 2, { projectId: 'b' })];
  const at = (id: string, step: 1 | -1, area = 'chat', projectId?: string) => neighbour(all, { id, area, projectId }, step, now)?.id;
  equal(at('pin', 1), 'new', 'pinned first');
  equal(at('new', 1), 'old');
  equal(at('old', 1), undefined, 'the last one');
  equal(at('pin', -1), undefined, 'the first one');
  equal(at('gone', 1), undefined, 'an archived one is not in the list');
  equal(at('p1', 1, 'project', 'a'), undefined, 'a project walks its own conversations');
});

check('shortcuts: Alt Up and Down skip chats that sit in a group, as the sidebar list does', () => {
  const now = Date.parse('2026-10-22T12:00:00');
  const all = ['a', 'b', 'c'].map((id, i) => ({ id, area: 'chat', updatedAt: now - i * 3_600_000 }));
  equal(neighbour(all, { id: 'a', area: 'chat' }, 1, now, new Set(['b']))?.id, 'c');
});
