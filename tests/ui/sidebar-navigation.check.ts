import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Area, ChatGroup, Summary } from '../../ui/api.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { check, equal, frames, ok } from './check.ts';

check('sidebar: arrows move through visible rows without opening or deleting them', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0;width:288px';
  root.dataset.motion = 'none';
  document.body.append(root);
  const now = 1_800_000_000_000;
  const [area, setArea] = createSignal<Area>('chat');
  const [groups, setGroups] = createSignal<ChatGroup[]>([]);
  const [rows, setRows] = createSignal<Summary[]>([
    { id: 'a', title: 'Aleksandra Wiśniewska-Kowalczyk', area: 'chat', updatedAt: now },
    { id: 'b', title: 'Jo', area: 'chat', updatedAt: now - 1 },
    { id: 'c', title: 'A project conversation', area: 'project', projectId: 'p', updatedAt: now },
    { id: 'd', title: 'Another project conversation', area: 'project', projectId: 'q', updatedAt: now },
  ]);
  let opened = 0;
  let removed = 0;
  const dispose = render(() => createComponent(Sidebar, {
    get conversations() { return rows(); }, get groups() { return groups(); },
    projects: [{ id: 'p', name: 'First project', path: 'C:/project' }, { id: 'q', name: 'Second project', path: 'C:/other' }],
    vendors: [], version: '1', get place() { return { area: area() }; }, running: [], quotas: {}, now,
    onNew: () => {}, onArea: setArea, onProject: () => { opened++; }, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => { opened++; }, onRemove: () => { removed++; }, onSettings: () => {},
  }), root);
  const key = (node: Element, value: string, extra: KeyboardEventInit = {}) => {
    const e = new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true, ...extra });
    node.dispatchEvent(e);
    return e;
  };
  const chat = (id: string) => root.querySelector<HTMLButtonElement>(`[data-conversation-id="${id}"] .item`)!;
  const project = (id: string) => root.querySelector<HTMLButtonElement>(`[data-project-id="${id}"] > .prow > .item`)!;
  try {
    await frames();
    chat('a').focus();
    ok(key(chat('a'), 'ArrowDown').defaultPrevented, 'handled arrows do not scroll the page');
    equal(document.activeElement, chat('b'));
    key(chat('b'), 'ArrowDown');
    equal(document.activeElement, chat('b'), 'the last row does not wrap');
    key(chat('b'), 'Home');
    equal(document.activeElement, chat('a'));
    key(chat('a'), 'End');
    equal(document.activeElement, chat('b'));
    key(chat('b'), 'ArrowUp');
    equal(document.activeElement, chat('a'));
    ok(!key(chat('a'), 'ArrowDown', { altKey: true }).defaultPrevented, 'Alt+Down belongs to the existing global shortcut');
    equal(document.activeElement, chat('a'));
    const search = root.querySelector<HTMLInputElement>('input[type="search"]')!;
    search.focus();
    ok(!key(search, 'Home').defaultPrevented, 'editing keys stay with the search field');
    search.value = 'Jo';
    search.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await frames();
    chat('b').focus();
    key(chat('b'), 'ArrowUp');
    equal(document.activeElement, chat('b'), 'filtered-out rows are not focus targets');
    search.value = '';
    search.dispatchEvent(new InputEvent('input', { bubbles: true }));
    setArea('project');
    await frames();
    project('p').focus();
    key(project('p'), 'ArrowDown');
    equal(document.activeElement, project('q'), 'collapsed conversations are skipped');
    key(project('q'), 'ArrowUp');
    key(project('p'), 'ArrowRight');
    await frames();
    equal(project('p').getAttribute('aria-expanded'), 'true');
    equal(document.activeElement, project('p'), 'expanding does not activate the project');
    key(project('p'), 'ArrowRight');
    equal(document.activeElement, chat('c'), 'Right enters an expanded project');
    key(chat('c'), 'ArrowLeft');
    equal(document.activeElement, project('p'), 'Left returns to the parent');
    key(project('p'), 'ArrowLeft');
    equal(project('p').getAttribute('aria-expanded'), 'false');
    key(project('p'), 'ArrowDown');
    equal(document.activeElement, project('q'), 'departing rows are skipped before their exit animation ends');
    setGroups([{ id: 'g', name: 'A Chat group', instructions: '', files: [], conversationIds: ['a'] }]);
    setArea('chat');
    await frames();
    const group = root.querySelector<HTMLButtonElement>('[data-chat-group-id="g"] > .prow > .item')!;
    group.focus();
    key(group, 'ArrowRight');
    await frames();
    key(group, 'ArrowRight');
    equal(document.activeElement, chat('a'), 'Chat groups use the same parent/child navigation');
    key(chat('a'), 'ArrowLeft');
    equal(document.activeElement, group);
    key(group, 'ArrowLeft');
    key(group, 'ArrowDown');
    equal(document.activeElement, chat('b'), 'collapsed group children are skipped');
    equal(opened, 0, 'navigation moves focus, Enter and Space still open');
    equal(removed, 0, 'row actions are never triggered by arrows');
    setRows([]);
    setArea('chat');
    await frames();
    ok(!key(search, 'ArrowDown').defaultPrevented, 'an empty list keeps input keys native');
  } finally { dispose(); root.remove(); }
});
