import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type Message, type Project, type Summary } from '../../ui/api.ts';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { t } from '../../ui/i18n.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

/** A right-click on `el`; false means the page stopped the browser's own menu. */
function rightClick(el: Element, x = 40, y = 40): boolean {
  return el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, composed: true, button: 2, clientX: x, clientY: y }));
}

const key = (el: Element, init: KeyboardEventInit) => el.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));
const menus = () => [...document.querySelectorAll<HTMLElement>('.cmenu:not(.leaving)')];
const entries = () => [...(menus()[0]?.querySelectorAll<HTMLButtonElement>('[role=menuitem]') ?? [])];
const labels = () => entries().map((b) => b.textContent);
const entry = (label: string) => entries().find((b) => b.textContent === label)!;
const clickOutside = () => document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));

/** A screen with room for a menu, in the look's own colours and with no motion. */
function stage() {
  installContextMenu();
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:520px';
  root.dataset.motion = 'none';
  for (const [name, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${name}`, value);
  document.body.append(root);
  return root;
}

function clipboard(read = '') {
  const written: string[] = [];
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { readText: async () => read, writeText: async (text: string) => void written.push(text) } });
  return written;
}
const restoreClipboard = () => delete (navigator as { clipboard?: unknown }).clipboard;

const chats: Summary[] = [{ id: 'c1', title: 'Plan the trip', area: 'chat', updatedAt: Date.now() }];
const projects: Project[] = [{ id: 'p1', name: 'Garden', path: 'X:/garden' }];

function sidebar(root: HTMLElement, calls: string[]) {
  return render(
    () => [
      createComponent(Sidebar, {
        conversations: chats, projects, vendors: [], version: '0', place: { area: 'chat' }, running: [], quotas: {}, now: Date.now(),
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: (id: string) => calls.push(`new ${id}`), onAddProject: () => {}, onOpen: () => {},
        onRemove: (id: string) => calls.push(`remove ${id}`), onSettings: () => {},
      }),
      createComponent(ContextMenuHost, {}),
    ],
    root,
  );
}

check('right-click: an empty area, a button and the chrome show no menu and the browser menu is stopped', async () => {
  const root = stage();
  root.innerHTML = '<div id="empty" style="width:100px;height:100px"></div><button>A button</button><div class="side-foot">Footer</div>';
  const dispose = render(() => createComponent(ContextMenuHost, {}), root);
  try {
    for (const el of [root.querySelector('#empty')!, root.querySelector('button')!, root.querySelector('.side-foot')!, document.body]) {
      equal(rightClick(el), false, 'the default (the browser menu) must be prevented');
      await settle();
      equal(menus().length, 0, 'no Faber menu opens either');
    }
  } finally {
    dispose();
    root.remove();
  }
});

check('right-click: the browser reload, print, save and view-source shortcuts do nothing, the others stay', () => {
  installContextMenu();
  for (const init of [{ key: 'F5' }, { key: 'r', ctrlKey: true }, { key: 'R', ctrlKey: true, shiftKey: true }, { key: 'p', ctrlKey: true }, { key: 's', ctrlKey: true }, { key: 'u', ctrlKey: true }]) {
    equal(key(document.body, init), false, `${init.key} must not reach the browser`);
  }
  equal(key(document.body, { key: 'a', ctrlKey: true }), true, 'Ctrl A (select all) is left alone');
  equal(key(document.body, { key: 'F2' }), true, 'other keys are left alone');
});

check('right-click: a chat offers Rename and Delete, Delete asks twice, Rename edits in place', async () => {
  const root = stage();
  const calls: string[] = [];
  const renamed: Array<[string, string]> = [];
  const original = api.rename;
  api.rename = async (id, title) => void renamed.push([id, title]);
  const dispose = sidebar(root, calls);
  try {
    const row = [...root.querySelectorAll('.crow')].find((r) => r.textContent?.includes('Plan the trip'))!;
    equal(rightClick(row), false);
    await settle();
    deepEqual(labels(), [t('menu.rename'), t('menu.delete')]);
    ok(entry(t('menu.delete')).classList.contains('danger'), 'Delete is last and in the danger colour');
    entry(t('menu.delete')).click();
    await settle();
    equal(menus().length, 1, 'the first click only asks');
    equal(entry(t('side.confirmDelete')).textContent, t('side.confirmDelete'));
    deepEqual(calls, []);
    entry(t('side.confirmDelete')).click();
    await settle();
    deepEqual(calls, ['remove c1']);
    equal(menus().length, 0, 'the menu closes once it did it');

    rightClick(row);
    await settle();
    entry(t('menu.rename')).click();
    await settle();
    const box = root.querySelector<HTMLInputElement>('input.rename')!;
    ok(box, 'the row turns into a text box');
    equal(box.value, 'Plan the trip');
    equal(document.activeElement, box, 'and it has focus');
    box.value = 'Plan the summer trip';
    key(box, { key: 'Enter' });
    await settle();
    deepEqual(renamed, [['c1', 'Plan the summer trip']]);
    equal(root.querySelector('input.rename'), null);

    // Esc keeps the old name.
    rightClick(row);
    await settle();
    entry(t('menu.rename')).click();
    await settle();
    const again = root.querySelector<HTMLInputElement>('input.rename')!;
    again.value = 'Nope';
    key(again, { key: 'Escape' });
    await settle();
    equal(renamed.length, 1);
    equal(root.querySelector('input.rename'), null);
  } finally {
    api.rename = original;
    dispose();
    root.remove();
  }
});

check('right-click: a project offers a new conversation, its folder and removal, which asks twice', async () => {
  const root = stage();
  const calls: string[] = [];
  const original = { remove: api.removeProject, show: api.showProject };
  api.removeProject = async (id) => void calls.push(`removeProject ${id}`);
  api.showProject = async (id) => void calls.push(`folder ${id}`);
  const dispose = sidebar(root, calls);
  try {
    const row = root.querySelector('.prow')!;
    equal(rightClick(row), false);
    await settle();
    deepEqual(labels(), [t('menu.newHere'), t('menu.openFolder'), t('menu.removeProject')]);
    entry(t('menu.newHere')).click();
    await settle();
    rightClick(row);
    await settle();
    entry(t('menu.openFolder')).click();
    await settle();
    rightClick(row);
    await settle();
    entry(t('menu.removeProject')).click();
    await settle();
    equal(entry(t('menu.removeConfirm')).textContent, t('menu.removeConfirm'));
    deepEqual(calls, ['new p1', 'folder p1']);
    entry(t('menu.removeConfirm')).click();
    await settle();
    deepEqual(calls, ['new p1', 'folder p1', 'removeProject p1']);
  } finally {
    api.removeProject = original.remove;
    api.showProject = original.show;
    dispose();
    root.remove();
  }
});

check('right-click: the keyboard opens the menu on a focused row, arrows move, Esc closes and gives focus back', async () => {
  const root = stage();
  const dispose = sidebar(root, []);
  try {
    const item = root.querySelector<HTMLButtonElement>('.crow .item')!;
    item.focus();
    key(item, { key: 'F10', shiftKey: true });
    await settle();
    deepEqual(labels(), [t('menu.rename'), t('menu.delete')]);
    equal(document.activeElement, entry(t('menu.rename')), 'the first item takes focus');
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.delete')));
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.rename')), 'arrows wrap around');
    key(document.activeElement!, { key: 'Escape' });
    await settle(260);
    equal(menus().length, 0);
    equal(document.activeElement, item, 'focus is back on the row');
    item.focus();
    key(item, { key: 'ContextMenu' });
    await settle();
    equal(menus().length, 1, 'the Menu key opens it too');
    clickOutside();
    await settle(260);
    equal(menus().length, 0, 'a click outside closes it');
  } finally {
    dispose();
    root.remove();
  }
});

check('right-click: a menu near a window edge stays inside the window', async () => {
  const root = stage();
  const dispose = sidebar(root, []);
  try {
    const row = root.querySelector('.crow')!;
    for (const [x, y] of [[innerWidth - 1, innerHeight - 1], [0, 0], [innerWidth - 1, 0], [0, innerHeight - 1]] as const) {
      rightClick(row, x, y);
      await settle();
      // Layout position, not the drawn one: the entrance is still moving the menu a few pixels.
      const m = menus()[0]!;
      const [left, top, right, bottom] = [m.offsetLeft, m.offsetTop, m.offsetLeft + m.offsetWidth, m.offsetTop + m.offsetHeight];
      ok(left >= 0 && top >= 0 && right <= innerWidth && bottom <= innerHeight, `inside the window at ${x},${y}: ${[left, top, right, bottom]}`);
      clickOutside();
      await settle(260);
    }
  } finally {
    dispose();
    root.remove();
  }
});

check('right-click: a text field offers Cut, Copy, Paste and Select all, and Paste inserts at the cursor', async () => {
  const root = stage();
  const written = clipboard('XY');
  root.innerHTML = '<textarea id="t" style="width:200px;height:60px"></textarea><input id="empty"><input id="ro" readonly value="locked">';
  const dispose = render(() => createComponent(ContextMenuHost, {}), root);
  try {
    const area = root.querySelector<HTMLTextAreaElement>('#t')!;
    area.value = 'abcdef';
    area.setSelectionRange(2, 4);
    let typed = 0;
    area.addEventListener('input', () => typed++);
    equal(rightClick(area), false);
    await settle();
    deepEqual(labels(), [t('menu.cut'), t('menu.copy'), t('menu.paste'), t('menu.selectAll')]);
    ok(entries().every((b) => !b.disabled), 'all four apply to a selection');
    entry(t('menu.copy')).click();
    await settle();
    deepEqual(written, ['cd']);

    area.setSelectionRange(2, 4);
    rightClick(area);
    await settle();
    entry(t('menu.paste')).click();
    await settle();
    equal(area.value, 'abXYef', 'the selection is replaced by what was on the clipboard');
    ok(typed > 0, 'the page hears an input event');

    rightClick(area);
    await settle();
    entry(t('menu.selectAll')).click();
    await settle();
    equal(area.selectionEnd - area.selectionStart, area.value.length);

    rightClick(root.querySelector('#empty')!);
    await settle();
    deepEqual(entries().map((b) => b.disabled), [true, true, false, true], 'an empty field can only paste');
    clickOutside();
    await settle(260);

    const locked = root.querySelector<HTMLInputElement>('#ro')!;
    locked.setSelectionRange(0, 3);
    rightClick(locked);
    await settle();
    deepEqual(entries().map((b) => b.disabled), [true, false, true, false], 'a read-only field can copy but not cut or paste');
  } finally {
    restoreClipboard();
    dispose();
    root.remove();
  }
});

check('right-click: selected text offers Copy, and only where the click lands on it', async () => {
  const root = stage();
  const written = clipboard();
  root.innerHTML = '<p id="p" style="display:inline-block;margin:20px">Some words to copy</p><div id="far" style="width:100px;height:60px"></div>';
  const dispose = render(() => createComponent(ContextMenuHost, {}), root);
  try {
    const p = root.querySelector('#p')!;
    const range = document.createRange();
    range.selectNodeContents(p);
    getSelection()!.removeAllRanges();
    getSelection()!.addRange(range);
    const box = range.getBoundingClientRect();
    equal(rightClick(p, box.left + 4, box.top + 4), false);
    await settle();
    deepEqual(labels(), [t('menu.copy')]);
    entry(t('menu.copy')).click();
    await settle();
    deepEqual(written, ['Some words to copy']);

    equal(rightClick(root.querySelector('#far')!, box.right + 120, box.bottom + 80), false);
    await settle();
    equal(menus().length, 0, 'a click away from the selection opens nothing');
  } finally {
    getSelection()!.removeAllRanges();
    restoreClipboard();
    dispose();
    root.remove();
  }
});

check('right-click: your message offers Copy and Edit, an answer offers Copy and Redo, a running answer only Copy', async () => {
  const root = stage();
  const written = clipboard();
  const branches: Array<[string, string, unknown]> = [];
  const messages: Message[] = [
    { id: 'u1', role: 'user', text: 'Make a plan', at: 0 },
    { id: 'a1', role: 'assistant', text: 'Here is the plan', at: 1, status: 'done', vendor: 'claude', model: 'sonnet' },
    { id: 'u2', role: 'user', text: 'And now?', at: 2 },
    { id: 'a2', role: 'assistant', text: 'Working on it', at: 3, status: 'streaming', vendor: 'claude', model: 'sonnet' },
  ];
  const dispose = render(
    () => [
      createComponent(Thread, { messages, from: 0, conversationId: 'c', options: [], onBranch: (id: string, text: string, choice?: unknown) => void branches.push([id, text, choice]) }),
      createComponent(ContextMenuHost, {}),
    ],
    root,
  );
  try {
    const mine = root.querySelector('.mine')!;
    equal(rightClick(mine), false);
    await settle();
    deepEqual(labels(), [t('menu.copy'), t('mine.edit')]);
    entry(t('menu.copy')).click();
    await settle();
    deepEqual(written, ['Make a plan']);
    rightClick(mine);
    await settle();
    entry(t('mine.edit')).click();
    await settle();
    ok(root.querySelector('form.editing textarea'), 'Edit opens the message for editing');

    const [done, running] = [...root.querySelectorAll('.ai')];
    rightClick(done!);
    await settle();
    deepEqual(labels(), [t('menu.copy'), t('menu.redo')]);
    entry(t('menu.redo')).click();
    await settle();
    deepEqual(branches, [['u1', 'Make a plan', undefined]]);

    rightClick(running!);
    await settle();
    deepEqual(labels(), [t('menu.copy')]);
  } finally {
    restoreClipboard();
    dispose();
    root.remove();
  }
});
