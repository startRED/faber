import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Area, type Summary } from '../../ui/api.ts';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { closeMenu, installContextMenu } from '../../ui/contextmenu.ts';
import { t } from '../../ui/i18n.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

for (const area of ['chat', 'project'] as const) {
  check(`pin: ${area} menu preserves keyboard focus, names groups, pins newest first, searches and unpins`, async () => {
    installContextMenu();
    const now = Date.now();
    const make = (id: string, updatedAt: number, pinnedAt?: number): Summary => ({
      id, title: `Notes ${id}`, area, projectId: area === 'project' ? 'p' : undefined, updatedAt, pinnedAt,
    });
    const [rows, setRows] = createSignal<Summary[]>([
      make('recent', now), make('older', 1), make('pinned', 0, 10),
      { ...make('child', 0), startedBy: 'older' },
      { ...make('archived', 0), archived: true },
      { ...make('elsewhere', now, 50), area: area === 'chat' ? 'project' : 'chat' },
      ...(area === 'project' ? [{ ...make('other-project', now, 60), projectId: 'other' }] : []),
    ]);
    const original = { pin: api.pin, searchMessages: api.searchMessages };
    const calls: [string, boolean][] = [];
    const errors: unknown[] = [];
    let clock = 20;
    api.pin = async (id, on) => {
      calls.push([id, on]);
      // The engine's list event reaches the screen after the menu has closed and restored focus.
      await settle();
      setRows((held) => held.map((c) => c.id === id ? { ...c, pinnedAt: on ? clock++ : undefined } : c));
    };
    api.searchMessages = async () => ({ ready: true, hits: [] });
    const root = document.createElement('div');
    // Like the menu checks, keep the fixture in view: focus must not scroll the page and dismiss the menu.
    root.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:520px';
    root.dataset.motion = 'none';
    document.body.append(root);
    const dispose = render(() => [
      createComponent(Sidebar, {
        get conversations() { return rows(); },
        projects: [{ id: 'p', name: 'Garden', path: 'X:/garden' }], vendors: [], version: 'test',
        place: { area: area as Area, projectId: area === 'project' ? 'p' : undefined }, running: [], quotas: {}, now,
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
        onOpen: () => {}, onRemove: () => {}, onSettings: () => {}, onError: (err) => errors.push(err),
      }),
      createComponent(ContextMenuHost, {}),
    ], root);
    const titles = () => [...root.querySelectorAll('.crow .item > span:first-child')].map((el) => el.textContent);
    const groupCount = () => [...root.querySelectorAll('.group-h')].filter((el) => el.textContent === t('side.pinned')).length;
    const labelledGroups = () => [...root.querySelectorAll('.list > section, .proj-chats > section')].map((group) => {
      const heading = group.querySelector(':scope > h2');
      ok(heading?.id, 'every conversation group has a semantic heading');
      equal(group.getAttribute('aria-labelledby'), heading.id, 'the group is named by its own heading');
      equal(document.getElementById(heading.id), heading, 'the heading id is unique');
      return heading.textContent;
    });
    const conversationButton = (id: string) => root.querySelector<HTMLButtonElement>(`[data-conversation-id="${id}"] .item`);
    const menu = async (id: string, action: 'pin' | 'unpin', moveAway = false, from = '.item') => {
      const row = [...root.querySelectorAll('.crow')].find((el) => el.querySelector('.item > span')?.textContent === `Notes ${id}`);
      ok(row, `${id} is listed`);
      const before = row.querySelector<HTMLButtonElement>(from)!;
      before.focus({ preventScroll: true });
      equal(document.activeElement, before, `the conversation's ${from} control has focus before Shift+F10`);
      before.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'F10', shiftKey: true }));
      await settle();
      const panel = document.querySelector<HTMLElement>('.cmenu:not(.leaving)');
      ok(panel, 'Shift+F10 opens a menu on the focused conversation');
      const entries = [...panel.querySelectorAll<HTMLButtonElement>('[role=menuitem]')];
      deepEqual(entries.map((el) => el.textContent), [t('menu.rename'), t(`menu.${action}`), t('menu.export'), t('menu.archive'), t('menu.delete')]);
      const item = entries.find((el) => el.textContent === t(`menu.${action}`));
      ok(item, `the menu offers ${action}`);
      item.focus({ preventScroll: true });
      item.click();
      const otherControl = root.querySelector<HTMLInputElement>('input[type=search]')!;
      if (moveAway) otherControl.focus({ preventScroll: true });
      await settle();
      equal(document.activeElement, moveAway ? otherControl : conversationButton(id), moveAway
        ? 'a delayed refresh does not take focus from another control'
        : `${action} keeps keyboard focus on the replacement conversation button`);
    };
    const search = async (value: string) => {
      const input = root.querySelector<HTMLInputElement>('input[type=search]')!;
      input.value = value;
      input.dispatchEvent(new InputEvent('input', { bubbles: true }));
      await settle();
    };
    try {
      await settle();
      const normal = ['Notes recent', 'Notes older', 'Notes child'];
      deepEqual(titles(), ['Notes pinned', ...normal]);
      equal(groupCount(), 1);
      deepEqual(labelledGroups(), area === 'chat' ? [t('side.pinned'), t('side.today'), t('side.earlier')] : [t('side.pinned')]);
      const pinnedSection = conversationButton('pinned')!.closest('section');
      equal(document.getElementById(pinnedSection?.getAttribute('aria-labelledby') ?? '')?.textContent, t('side.pinned'), 'pinned rows belong to the named group');
      await menu('older', 'pin');
      deepEqual(titles(), ['Notes older', 'Notes pinned', 'Notes recent', 'Notes child']);
      // Started from the row's × button, focus still lands on the recreated row's title.
      await menu('recent', 'pin', false, '.x');
      deepEqual(titles(), ['Notes recent', 'Notes older', 'Notes pinned', 'Notes child']);
      await menu('child', 'pin');
      deepEqual(titles(), ['Notes child', 'Notes recent', 'Notes older', 'Notes pinned'], 'even an agent-started conversation follows pin order');
      equal(root.querySelector('.crow')?.classList.contains('child'), false, 'the pinned group is flat');
      await search('older');
      deepEqual(titles(), ['Notes older'], 'search finds a pinned conversation normally');
      equal(groupCount(), 0, 'search does not introduce a pin filter or group');
      await search('');
      await menu('child', 'unpin');
      await menu('recent', 'unpin');
      await menu('older', 'unpin');
      deepEqual(titles(), ['Notes pinned', ...normal]);
      await menu('pinned', 'unpin');
      deepEqual(titles(), [...normal.slice(0, 2), ...(area === 'project' ? ['Notes child', 'Notes pinned'] : ['Notes pinned', 'Notes child'])]);
      equal(groupCount(), 0, 'the empty Pinned heading goes away');
      if (area === 'chat') {
        setRows((held) => [...held, make('week', now - 2 * 24 * 3600 * 1000)]);
        deepEqual(labelledGroups(), [t('side.today'), t('side.week'), t('side.earlier')], 'all date groups use the same accessible grouping');
      }
      deepEqual(calls, [['older', true], ['recent', true], ['child', true], ['child', false], ['recent', false], ['older', false], ['pinned', false]]);
      // In a list taller than its room, the conversation that moves is scrolled back into view with its focus.
      const scroller = root.querySelector<HTMLElement>('.side-scroll')!;
      scroller.style.height = scroller.style.maxHeight = '64px';
      scroller.scrollTop = scroller.scrollHeight;
      await settle();
      await menu('child', 'pin');
      const shown = conversationButton('child')!.getBoundingClientRect(), room = scroller.getBoundingClientRect();
      ok(shown.top >= room.top - 1 && shown.bottom <= room.bottom + 1, 'the refocused conversation is scrolled into view');
      scroller.style.height = scroller.style.maxHeight = '';
      // Restoring the height scrolls the list, and a scroll closes menus: let that settle before the next one.
      await new Promise((resolve) => setTimeout(resolve, 50));
      await menu('child', 'unpin');
      await menu('older', 'pin', true);
      await menu('older', 'unpin', true);
      api.pin = async () => { throw 'error.conversationGone'; };
      await menu('older', 'pin');
      deepEqual(errors, ['error.conversationGone'], 'a refused pin uses the existing error path');
    } finally {
      closeMenu();
      Object.assign(api, original);
      dispose();
      root.remove();
    }
  });
}
