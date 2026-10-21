import { type Accessor, createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type Area, type Message, type Project, type RunningItem, type Summary } from '../../ui/api.ts';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { pickUp } from '../../ui/running.ts';
import { nest } from '../../ui/sidebar.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { t } from '../../ui/i18n.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
/** Longer than the time in which a second click still belongs to the gesture that armed an item. */
const deliberate = () => settle(560);

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
  setArea('chat');
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

const working = (projectId: string): RunningItem => ({
  conversationId: 'w1', turnId: 't1', title: 'Busy', area: 'project', projectId, state: 'working', since: 0, permissions: [],
});

// Each area lists only its own: chats in Chat, projects with their conversations in Projects.
const [area, setArea] = createSignal<Area>('chat');
function sidebar(
  root: HTMLElement,
  calls: string[],
  running: Accessor<RunningItem[]> = () => [],
  onError: (err: unknown) => void = () => {},
  onRemove: (id: string, discard?: boolean) => Promise<'unsaved' | undefined> | void = (id) => void calls.push(`remove ${id}`),
) {
  return render(
    () => [
      createComponent(Sidebar, {
        conversations: chats, projects, vendors: [], version: '0', get place() { return { area: area() }; }, get running() { return running(); }, quotas: {}, now: Date.now(), onError,
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: (id: string) => calls.push(`new ${id}`), onAddProject: () => {}, onOpen: () => {},
        onRemove, onSettings: () => {},
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

check('right-click: a chat offers Rename, Export, Archive and Delete, Delete asks twice, Rename edits in place', async () => {
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
    deepEqual(labels(), [t('menu.rename'), t('menu.pin'), t('menu.export'), t('menu.moveToGroup'), t('menu.moveToProject'), t('menu.archive'), t('menu.delete')]);
    ok(entry(t('menu.delete')).classList.contains('danger'), 'Delete is last and in the danger colour');
    entry(t('menu.delete')).click();
    await settle();
    equal(menus().length, 1, 'the first click only asks');
    equal(entry(t('side.confirmDelete')).textContent, t('side.confirmDelete'));
    deepEqual(calls, []);
    await deliberate();
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

check('conversation export: both areas save their own row, cancel silently and show a refused save', async () => {
  const calls: Array<[string, unknown]> = [];
  const errors: unknown[] = [];
  const original = api.exportConversation;
  let outcome: boolean | string = true;
  api.exportConversation = async (id, words) => {
    calls.push([id, words]);
    if (typeof outcome === 'string') throw { key: outcome };
    return outcome;
  };
  try {
    for (const place of ['chat', 'project'] as const) {
      const root = stage();
      const dispose = render(() => [
        createComponent(Sidebar, {
          conversations: [...chats, { id: 'pc', title: 'Project chat', area: 'project', projectId: 'p1', updatedAt: Date.now() }],
          projects, vendors: [], version: '0', place: { area: place, projectId: place === 'project' ? 'p1' : undefined }, running: [], quotas: {}, now: Date.now(),
          onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
          onError: (err: unknown) => errors.push(err),
        }),
        createComponent(ContextMenuHost, {}),
      ], root);
      try {
        await settle();
        for (const result of [true, false, 'error.export.write']) {
          outcome = result;
          const title = place === 'chat' ? 'Plan the trip' : 'Project chat';
          const row = [...root.querySelectorAll('.crow')].find((el) => el.querySelector('.item > span')?.textContent === title);
          ok(row, `${title} is listed in ${place}`);
          rightClick(row);
          await settle();
          const button = entry(t('menu.export'));
          ok(button, 'Export is in the conversation menu');
          const before = calls.length;
          button.click();
          await settle();
          equal(calls.length, before + 1, 'each activation exports exactly once');
          deepEqual(calls.at(-1), [place === 'chat' ? 'c1' : 'pc', { you: t('export.you'), auto: t('model.auto'), attachments: t('export.attachments'), files: t('export.files') }]);
          equal(menus().length, 0, 'the menu closes after Export');
        }
      } finally {
        dispose();
        root.remove();
      }
    }
    deepEqual(errors, [{ key: 'error.export.write' }, { key: 'error.export.write' }], 'only failed saves say anything');
  } finally {
    api.exportConversation = original;
  }
});

check('move chat to project: the keyboard picker names projects, refuses busy chats and moves the pinned row at once', async () => {
  const root = stage();
  root.style.width = '720px';
  const [rows, setRows] = createSignal<Summary[]>([{ ...chats[0]!, pinnedAt: 10 }]);
  const [place, setPlace] = createSignal<{ area: Area; id?: string; projectId?: string }>({ area: 'chat', id: 'c1' });
  const [available, setAvailable] = createSignal(projects);
  const [grouped, setGrouped] = createSignal(false);
  const errors: unknown[] = [];
  const calls: [string, string][] = [];
  const original = api.moveToProject;
  let busy = true;
  api.moveToProject = async (id, projectId) => {
    calls.push([id, projectId]);
    if (busy) throw { key: 'error.moveChat.busy' };
    return { id, title: 'Plan the trip', area: 'project', projectId, pinnedAt: 10, sessions: {}, createdAt: 1, updatedAt: 2, messages: [] };
  };
  const dispose = render(() => [
    createComponent(Sidebar, {
      get conversations() { return rows(); }, get projects() { return available(); }, get place() { return place(); },
      get groups() { return [{ id: 'g1', name: 'Trips', instructions: '', files: [], conversationIds: grouped() ? ['c1'] : [] }]; },
      vendors: [], version: '0', running: [], quotas: {}, now: Date.now(),
      onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
      onOpen: () => {}, onRemove: () => {}, onSettings: () => {}, onError: (err) => errors.push(err),
      onMoved: (c) => { setRows([{ ...rows()[0]!, area: c.area, projectId: c.projectId }]); setPlace({ area: c.area, id: c.id, projectId: c.projectId }); },
    }),
    createComponent(ContextMenuHost, {}),
  ], root);
  const open = async () => {
    const row = root.querySelector<HTMLButtonElement>('.crow .item')!;
    row.focus({ preventScroll: true });
    key(row, { key: 'F10', shiftKey: true });
    await settle();
  };
  try {
    await open();
    ok(entry(t('menu.moveToProject')), 'the Chat row offers Move to project');
    ok(!entry(t('menu.removeFromGroup')), 'an outside chat has no Remove from group');
    clickOutside(); await settle();
    setGrouped(true);
    root.querySelector<HTMLButtonElement>('[data-chat-group-id="g1"] .prow .item')!.click();
    await settle();
    await open();
    deepEqual(labels(), [t('menu.rename'), t('menu.unpin'), t('menu.export'), t('menu.moveToGroup'), t('menu.removeFromGroup'), t('menu.moveToProject'), t('menu.archive'), t('menu.delete')], 'both moves share the Chat menu in the agreed order');
    clickOutside(); await settle();
    setGrouped(false);
    setAvailable([]);
    await open();
    ok(!entry(t('menu.moveToProject')), 'with no projects there is no dead-end action');
    clickOutside(); await settle();
    setAvailable([...projects, { id: 'p2', name: 'A second project', path: 'X:/second' }]);
    setAvailable(Array.from({ length: 40 }, (_, i) => ({ id: `many-${i}`, name: i ? `Project ${i}` : 'A very long project name '.repeat(12), path: 'X:/many' })));
    await open();
    entry(t('menu.moveToProject')).click(); await settle();
    const picker = menus()[0]!;
    ok(picker.getBoundingClientRect().bottom <= innerHeight, 'a long project list stays inside the window');
    ok(entries()[0]!.getBoundingClientRect().right <= picker.getBoundingClientRect().right, 'long project names wrap inside the picker');
    key(document.activeElement!, { key: 'End' }); await settle();
    equal(document.activeElement, entries().at(-1), 'the keyboard reaches the last project');
    picker.dispatchEvent(new Event('scroll'));
    await settle(); equal(menus().length, 1, 'scrolling the projects never dismisses the picker');
    key(document.activeElement!, { key: 'Escape' }); await settle();
    equal(menus().length, 0, 'Escape cancels without moving');
    setAvailable([...projects, { id: 'p2', name: 'A second project', path: 'X:/second' }]);
    for (const refused of [true, false]) {
      busy = refused;
      await open();
      entry(t('menu.moveToProject')).click();
      await settle();
      deepEqual(labels(), ['Garden', 'A second project'], 'the picker contains project names, in the existing menu');
      equal(document.activeElement, entry('Garden'), 'the keyboard reaches the first project');
      key(document.activeElement!, { key: 'ArrowDown' });
      equal(document.activeElement, entry('A second project'));
      entry('Garden').click();
      await settle();
      equal(menus().length, 0);
      if (refused) {
        deepEqual(errors, [{ key: 'error.moveChat.busy' }]);
        equal(place().area, 'chat');
        equal(root.querySelector('.crow .item > span')?.textContent, 'Plan the trip');
      }
    }
    deepEqual(calls, [['c1', 'p1'], ['c1', 'p1']]);
    deepEqual(place(), { area: 'project', id: 'c1', projectId: 'p1' }, 'the open chat follows its project');
    equal(root.querySelectorAll('.crow').length, 1, 'the moved row appears once');
    equal(root.querySelector('.proj-chats .crow .item > span')?.textContent, 'Plan the trip');
    equal(root.querySelector('.crow .item')?.getAttribute('aria-current'), 'true');
    ok([...root.querySelectorAll('.group-h')].some((el) => el.textContent === t('side.pinned')), 'the moved row keeps its pin');
    await open();
    ok(!entry(t('menu.moveToProject')), 'a project conversation cannot move back');
    clickOutside(); await settle();
    setRows([{ ...chats[0]!, archived: true }]); setPlace({ area: 'chat' });
    const search = root.querySelector<HTMLInputElement>('input[type=search]')!;
    search.value = 'trip'; search.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle(); await open();
    ok(!entry(t('menu.moveToProject')), 'an archived Chat row cannot move');
    clickOutside();
  } finally {
    api.moveToProject = original;
    dispose(); root.remove();
  }
});

check('archive: an archived chat leaves the sidebar, a search finds it marked, and the menu archives and brings back', async () => {
  const root = stage();
  const archived: Array<[string, boolean]> = [];
  const original = api.archive;
  api.archive = async (id, on) => void archived.push([id, on]);
  const conversations: Summary[] = [...chats, { id: 'c2', title: 'Old trip notes', area: 'chat', updatedAt: Date.now(), archived: true }];
  const dispose = render(
    () => [
      createComponent(Sidebar, {
        conversations, projects, vendors: [], version: '0', place: { area: 'chat' }, running: [], quotas: {}, now: Date.now(),
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
      }),
      createComponent(ContextMenuHost, {}),
    ],
    root,
  );
  const titles = () => [...root.querySelectorAll('.crow .item')].map((el) => el.textContent);
  try {
    await settle();
    deepEqual(titles(), ['Plan the trip'], 'the archived chat is not listed');
    rightClick(root.querySelector('.crow')!);
    await settle();
    entry(t('menu.archive')).click();
    await settle();
    const search = root.querySelector<HTMLInputElement>('input[type=search]')!;
    search.value = 'trip';
    search.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    deepEqual(titles(), ['Plan the trip', `Old trip notes${t('archive.tag')}`], 'a search finds it, marked');
    rightClick([...root.querySelectorAll('.crow')].find((r) => r.textContent?.includes('Old trip notes'))!);
    await settle();
    deepEqual(labels(), [t('menu.rename'), t('menu.export'), t('menu.moveToGroup'), t('menu.unarchive'), t('menu.delete')]);
    entry(t('menu.unarchive')).click();
    await settle();
    deepEqual(archived, [['c1', true], ['c2', false]]);
  } finally {
    api.archive = original;
    dispose();
    root.remove();
  }
});

check('nested: a conversation an agent started sits under the one that started it, in the order given', () => {
  const rows = [{ id: 'c' }, { id: 'a' }, { id: 'b', startedBy: 'a' }, { id: 'd', startedBy: 'gone' }, { id: 'e', startedBy: 'b' }, { id: 'f', startedBy: 'c' }];
  deepEqual(nest(rows).map((r) => r.id), ['c', 'f', 'a', 'b', 'e', 'd']);
  ok(nest(rows).every((r) => rows.includes(r)), 'the same rows, so the sidebar keeps their nodes');
});

check('nested: the sidebar indents a conversation an agent started under its starter', async () => {
  const root = stage();
  setArea('project');
  const now = Date.now();
  const conversations: Summary[] = [
    { id: 'k1', title: 'Docs page', area: 'project', projectId: 'p1', updatedAt: now, startedBy: 'm1' },
    { id: 'm1', title: 'Main work', area: 'project', projectId: 'p1', updatedAt: now - 1 },
  ];
  const dispose = render(() => createComponent(Sidebar, {
    conversations, projects, vendors: [], version: '0', place: { area: 'project', projectId: 'p1' }, running: [], quotas: {}, now,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), root);
  try {
    await settle();
    const rows = [...root.querySelectorAll('.proj-chats .crow')];
    deepEqual(rows.map((r) => r.textContent?.trim()), ['Main work', 'Docs page']);
    deepEqual(rows.map((r) => r.classList.contains('child')), [false, true]);
  } finally {
    setArea('chat');
    dispose();
    root.remove();
  }
});

check('archive: the pick-up lists leave out an archived conversation', () => {
  const recent = [{ id: 'a', archived: true }, { id: 'b' }, { id: 'c' }];
  deepEqual(pickUp(recent, [], 3).map((c) => c.id), ['b', 'c']);
});

check('right-click: a project offers a new conversation, its folder and removal, which asks twice', async () => {
  const root = stage();
  setArea('project');
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
    localStorage.setItem('faber.project.goal.p1', 'Water the plants');
    localStorage.setItem('faber.project.mission.p1', 'm1');
    localStorage.setItem('faber.unsent.proof.p1', 'npm test');
    entry(t('menu.removeProject')).click();
    await settle();
    equal(entry(t('menu.removeConfirm')).textContent, t('menu.removeConfirm'));
    deepEqual(calls, ['new p1', 'folder p1']);
    await deliberate();
    entry(t('menu.removeConfirm')).click();
    await settle();
    deepEqual(calls, ['new p1', 'folder p1', 'removeProject p1']);
    equal(localStorage.getItem('faber.project.goal.p1'), null, 'the goal typed for a removed project goes with it');
    equal(localStorage.getItem('faber.project.mission.p1'), null, 'and so does the mission shown for it');
    equal(localStorage.getItem('faber.unsent.proof.p1'), null, 'and the checks typed for it and not saved');
  } finally {
    api.removeProject = original.remove;
    api.showProject = original.show;
    dispose();
    root.remove();
  }
});

check('sidebar: a project that needs the person says so in words beside the amber mark, never with a bare number', async () => {
  const root = stage();
  setArea('project');
  const dispose = sidebar(root, [], () => [{ ...working('p1'), state: 'needsYou' }]);
  try {
    await settle();
    equal(root.querySelector('.prow .amber-tag')?.textContent, t('run.needs', { n: 1 }));
  } finally {
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
    deepEqual(labels(), [t('menu.rename'), t('menu.pin'), t('menu.export'), t('menu.moveToGroup'), t('menu.moveToProject'), t('menu.archive'), t('menu.delete')]);
    equal(document.activeElement, entry(t('menu.rename')), 'the first item takes focus');
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.pin')));
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.export')));
    ok(entry(t('menu.moveToGroup')).disabled, 'without groups the unavailable move stays visible and arrows skip it');
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.moveToProject')));
    key(document.activeElement!, { key: 'ArrowDown' });
    equal(document.activeElement, entry(t('menu.archive')));
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

check('right-click: your message offers Copy, Edit and Go back, an answer offers Copy and Retry, a running answer only Copy', async () => {
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
    deepEqual(labels(), [t('menu.copy'), t('mine.edit'), t('rewind.title')]);
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
    // The menu says which model the retry uses: the composer's choice, here Auto.
    deepEqual(labels(), [t('menu.copy'), t('answer.retry', { model: t('model.auto') })]);
    entry(t('answer.retry', { model: t('model.auto') })).click();
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

check('a conversation whose copy holds unsaved work is deleted only after its row asks once more (R201)', async () => {
  const root = stage();
  const calls: string[] = [];
  // The engine refuses the first delete: its copy has changes found nowhere else.
  const dispose = sidebar(root, calls, () => [], () => {}, async (id, discard) => {
    calls.push(discard ? `discard ${id}` : `remove ${id}`);
    return discard ? undefined : 'unsaved';
  });
  try {
    const x = root.querySelector<HTMLButtonElement>('.crow .x')!;
    x.click();
    await settle();
    equal(x.textContent, t('side.confirmDelete'), 'the first click asks as always');
    x.click();
    await settle();
    deepEqual(calls, ['remove c1']);
    equal(x.textContent, t('side.confirmDiscard'), 'refused: the row now says what would be lost');
    equal(x.getAttribute('aria-label'), t('side.unsavedCopy'));
    x.click();
    await settle();
    deepEqual(calls, ['remove c1', 'discard c1'], 'only the click on that question discards');

    calls.length = 0;
    rightClick(root.querySelector('.crow')!);
    await settle();
    entry(t('menu.delete')).click();
    await settle();
    await deliberate();
    entry(t('side.confirmDelete')).click();
    await settle();
    deepEqual(calls, ['remove c1'], 'the menu asks the engine the same way');
    equal(x.textContent, t('side.confirmDiscard'), 'and the row asks the question');
  } finally {
    dispose();
    root.remove();
  }
});

check('right-click: a double click on Delete or Remove arms the item but never confirms it', async () => {
  const root = stage();
  const calls: string[] = [];
  const original = api.removeProject;
  api.removeProject = async (id) => void calls.push(`removeProject ${id}`);
  const dispose = sidebar(root, calls);
  try {
    const open = async (el: Element, first: string) => {
      rightClick(el);
      await settle();
      const button = entry(first);
      button.click();
      button.click();
      await settle();
    };
    await open([...root.querySelectorAll('.crow')][0]!, t('menu.delete'));
    deepEqual(calls, [], 'two quick clicks do not delete');
    equal(menus().length, 1);
    equal(entry(t('side.confirmDelete')).textContent, t('side.confirmDelete'), 'it waits armed for a deliberate click');
    await deliberate();
    entry(t('side.confirmDelete')).click();
    await settle();
    deepEqual(calls, ['remove c1'], 'a later click does');

    setArea('project');
    await settle();
    await open(root.querySelector('.prow')!, t('menu.removeProject'));
    deepEqual(calls, ['remove c1'], 'two quick clicks do not remove the project');
    equal(menus().length, 1);
  } finally {
    api.removeProject = original;
    dispose();
    root.remove();
  }
});

check('right-click: a project with work running cannot be removed, also when the work starts while the menu is open', async () => {
  const root = stage();
  setArea('project');
  const calls: string[] = [];
  const original = api.removeProject;
  api.removeProject = async (id) => void calls.push(`removeProject ${id}`);
  const [running, setRunning] = createSignal<RunningItem[]>([working('p1')]);
  const dispose = sidebar(root, calls, running);
  try {
    const row = root.querySelector('.prow')!;
    rightClick(row);
    await settle();
    ok(entry(t('menu.removeProject')).disabled, 'Remove is off while the project runs something');
    entry(t('menu.removeProject')).click();
    await deliberate();
    entry(t('menu.removeProject')).click();
    await settle();
    deepEqual(calls, []);
    clickOutside();
    await settle(260);

    // Idle when the menu opens, busy by the time it is confirmed.
    setRunning([]);
    rightClick(row);
    await settle();
    ok(!entry(t('menu.removeProject')).disabled);
    entry(t('menu.removeProject')).click();
    await settle();
    setRunning([working('p1')]);
    await deliberate();
    ok(entry(t('menu.removeConfirm')).disabled, 'the item went off while the menu was open');
    entry(t('menu.removeConfirm')).click();
    await settle();
    deepEqual(calls, [], 'a confirmation that arrives after the work started does nothing');
  } finally {
    api.removeProject = original;
    dispose();
    root.remove();
  }
});

check('right-click: what the engine refuses (a project still working, a folder that is gone) reaches the screen', async () => {
  const root = stage();
  setArea('project');
  const refused: unknown[] = [];
  const original = { remove: api.removeProject, show: api.showProject };
  api.removeProject = async () => Promise.reject('error.project.busy');
  api.showProject = async () => Promise.reject('error.folder.missing');
  const dispose = sidebar(root, [], () => [], (err) => refused.push(err));
  try {
    const row = root.querySelector('.prow')!;
    rightClick(row);
    await settle();
    entry(t('menu.openFolder')).click();
    await settle();
    rightClick(row);
    await settle();
    entry(t('menu.removeProject')).click();
    await deliberate();
    entry(t('menu.removeConfirm')).click();
    await settle();
    deepEqual(refused, ['error.folder.missing', 'error.project.busy']);
  } finally {
    api.removeProject = original.remove;
    api.showProject = original.show;
    dispose();
    root.remove();
  }
});

check('right-click: after Rename from the keyboard, Enter and Esc both give focus back to the row', async () => {
  const root = stage();
  const original = api.rename;
  api.rename = async () => {};
  const dispose = sidebar(root, []);
  try {
    const item = root.querySelector<HTMLButtonElement>('.crow .item')!;
    for (const finish of ['Escape', 'Enter']) {
      const row = root.querySelector<HTMLButtonElement>('.crow .item')!;
      row.focus();
      key(row, { key: 'F10', shiftKey: true });
      await settle();
      entry(t('menu.rename')).click();
      await settle();
      const box = root.querySelector<HTMLInputElement>('input.rename')!;
      equal(document.activeElement, box);
      key(box, { key: finish });
      await settle();
      equal(root.querySelector('input.rename'), null);
      equal(document.activeElement?.className, item.className, `${finish} leaves focus on the row`);
      ok(document.activeElement?.closest('.crow'), `${finish} leaves focus in the row`);
    }
  } finally {
    api.rename = original;
    dispose();
    root.remove();
  }
});

check('right-click: holding Enter on Delete arms it but the key repeat never confirms, a fresh press does', async () => {
  const root = stage();
  const calls: string[] = [];
  const dispose = sidebar(root, calls);
  try {
    rightClick(root.querySelector('.crow')!);
    await settle();
    const button = entry(t('menu.delete'));
    button.focus();
    // The first press arms (Enter clicks on key down); the key stays down.
    key(button, { key: 'Enter' });
    button.click();
    await deliberate();
    // The auto repeat: the page stops it, and even a click that gets through confirms nothing while the key is down.
    equal(key(button, { key: 'Enter', repeat: true }), false, 'a repeat is stopped');
    button.click();
    await settle();
    deepEqual(calls, [], 'holding the key never deletes');
    equal(menus().length, 1);
    // Released, and pressed again: a separate press confirms.
    button.dispatchEvent(new KeyboardEvent('keyup', { bubbles: true, cancelable: true, key: 'Enter' }));
    key(button, { key: 'Enter' });
    button.click();
    await settle();
    deepEqual(calls, ['remove c1']);
  } finally {
    dispose();
    root.remove();
  }
});
