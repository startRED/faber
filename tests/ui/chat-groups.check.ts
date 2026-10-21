import { createComponent, createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppEvent, type AppState, type ChatGroup as Group, type Conversation, type Summary } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { ChatGroup } from '../../ui/ChatGroup.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { closeMenu, installContextMenu } from '../../ui/contextmenu.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
// Menus belong to the page's first ContextMenuHost, which can be outside a check's root.

for (const action of ['button', 'shortcut'] as const) {
  check(`chat groups: deleting the open group clears its saved selection before New conversation by ${action}`, async () => {
    const root = document.createElement('div'); document.body.append(root);
    const original = { state: api.state, newGroupChat: api.newGroupChat, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
    const storage = localStorage.getItem('faber.place');
    const initial = {
      conversations: [], groups: [{ id: 'deleted-group', name: 'Notes', instructions: '', files: [], conversationIds: [] }], projects: [], vendors: ['claude'],
      settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
      models: [], running: { revision: 0, items: [] }, quotas: [], version: 'test',
      look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
        appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
    } as AppState;
    let hear: ((event: AppEvent) => void) | undefined;
    const starts: string[] = [];
    api.state = async () => structuredClone(initial);
    api.newGroupChat = async (id) => { starts.push(id); throw 'error.groupGone'; };
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', groupId: 'deleted-group' }));
    const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener) => { hear = listener; } } }), root);
    try {
      await settle();
      ok(root.querySelector('.chat-group-page'), 'the group is open before deletion');
      initial.groups = [];
      hear?.({ type: 'list' }); await settle();
      equal(root.querySelector('.chat-group-page'), null);
      equal(JSON.parse(localStorage.getItem('faber.place') ?? '{}').groupId, undefined, 'the deleted group is removed from persisted selection');
      if (action === 'button') root.querySelector<HTMLButtonElement>('.newchat')!.click();
      else window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, bubbles: true, cancelable: true }));
      await settle();
      deepEqual(starts, [], 'a fresh outside chat never calls the deleted group');
      ok(root.querySelector('.composer textarea'), 'New conversation opens the ordinary Chat composer');
      equal(root.querySelector('.work [role=alert]'), null);
      equal(JSON.parse(localStorage.getItem('faber.place') ?? '{}').groupId, undefined);
    } finally {
      dispose(); root.remove(); Object.assign(api, original);
      if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
    }
  });
}

for (const focus of ['destination', 'another control', 'another control then body'] as const) {
  check(`chat groups: keyboard Move to group preserves focus in ${focus}`, async () => {
    installContextMenu();
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;inset:0;overflow:auto'; root.dataset.motion = 'none'; document.body.append(root);
    const original = api.moveChatToGroup;
    const [groups, setGroups] = createSignal<Group[]>([
      { id: 'source', name: 'Source', instructions: '', files: [], conversationIds: ['moved-chat'] },
      { id: 'destination', name: 'Destination', instructions: '', files: [], conversationIds: [] },
    ]);
    let finish: (() => void) | undefined;
    api.moveChatToGroup = async (id, groupId) => {
      equal(id, 'moved-chat'); equal(groupId, 'destination');
      await new Promise<void>((resolve) => { finish = resolve; });
      setGroups((all) => all.map((g) => ({ ...g, conversationIds: g.id === 'destination' ? [id] : [] })));
    };
    const dispose = render(() => [
      createComponent(Sidebar, {
        conversations: [{ id: 'moved-chat', title: 'Notes', area: 'chat', updatedAt: Date.now() }],
        get groups() { return groups(); }, place: { area: 'chat', id: 'moved-chat' },
        projects: [], vendors: [], version: 'test', running: [], quotas: {}, now: Date.now(),
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
        onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
      }), createComponent(ContextMenuHost, {}),
    ], root);
    try {
      await settle();
      const row = root.querySelector<HTMLButtonElement>('[data-chat-group-id=source] .crow .item')!;
      row.focus({ preventScroll: true });
      row.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true }));
      await settle();
      const move = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((el) => el.textContent === t('menu.moveToGroup'));
      ok(move, 'the chat keyboard menu offers Move to group'); move.focus(); move.click(); await settle();
      const picker = root.querySelector<HTMLSelectElement>('.crow select')!;
      equal(document.activeElement, picker, 'the keyboard action focuses the destination picker');
      // Synthetic keys do not perform a select's native action; deliver the selection's change event.
      picker.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
      picker.value = 'destination'; picker.dispatchEvent(new Event('change', { bubbles: true }));
      ok(finish, 'the selected destination reached the engine');
      const elsewhere = root.querySelector<HTMLInputElement>('.search input')!;
      if (focus !== 'destination') {
        elsewhere.focus();
        equal(document.activeElement, elsewhere, 'the person moves focus to the search field before the reply');
        // An unfocused hidden WebView changes activeElement without emitting the user's native focusin event.
        if (!document.hasFocus()) elsewhere.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
      }
      if (focus === 'another control then body') {
        elsewhere.blur();
        equal(document.activeElement, document.body, 'the search field has lost focus before the reply');
      }
      finish(); await settle();
      const destination = root.querySelector<HTMLButtonElement>('[data-chat-group-id=destination] [data-conversation-id=moved-chat] .item');
      ok(destination, 'the row exists in its expanded destination group');
      equal(root.querySelector('[data-chat-group-id=source] .crow'), null);
      equal(document.activeElement, focus === 'destination' ? destination : focus === 'another control' ? elsewhere : document.body, `a delayed move restores only the focus the picker lost (active element: ${document.activeElement?.outerHTML.slice(0, 300)})`);
    } finally { finish?.(); closeMenu(); dispose(); root.remove(); api.moveChatToGroup = original; }
  });
}

for (const update of ['list refresh', 'minute clock tick'] as const) {
  check(`chat groups: chat rename survives a ${update}`, async () => {
    installContextMenu();
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;inset:0;overflow:auto';
    root.dataset.motion = 'none';
    document.body.append(root);
    const now = new Date(2026, 9, 20, 12).getTime();
    const [clock, setClock] = createSignal(now);
    const chat: Summary = { id: 'editing', title: 'Chat notes', area: 'chat', updatedAt: now };
    const [chats, setChats] = createSignal<Summary[]>([chat]);
    const group: Group = { id: 'folder', name: 'Notes', instructions: '', files: [], conversationIds: ['editing', 'arrived'] };
    const dispose = render(() => [
      createComponent(Sidebar, {
        get conversations() { return chats(); }, get now() { return clock(); }, groups: [group],
        place: { area: 'chat', id: chat.id }, projects: [], vendors: [], version: 'test', running: [], quotas: {},
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
        onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
      }),
      createComponent(ContextMenuHost, {}),
    ], root);
    try {
      await settle();
      const row = root.querySelector<HTMLButtonElement>('.chat-groups [data-conversation-id="editing"] .item')!;
      ok(row, 'the conversation is inside its expanded group');
      row.focus({ preventScroll: true });
      row.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true }));
      await settle();
      const rename = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((el) => el.textContent === t('menu.rename'));
      ok(rename, 'the chat keyboard menu offers Rename'); rename.click(); await settle();
      const field = root.querySelector<HTMLInputElement>('.chat-groups .crow input.rename')!;
      ok(field);
      field.value = 'Unfinished chat title';
      field.setSelectionRange(4, 10);
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
      equal(document.activeElement, field);
      // A refresh keeps the edited conversation object, as App's keepRows does, and adds a Pinned section before it.
      if (update === 'list refresh') setChats([chat, { id: 'arrived', title: 'Finished chat', area: 'chat', updatedAt: now, pinnedAt: now }]);
      else setClock(now + 60_000);
      await settle();
      equal(root.querySelector('.chat-groups .crow input.rename'), field, `${update} keeps the same editor node`);
      equal(field.value, 'Unfinished chat title');
      equal(document.activeElement, field, `${update} preserves focus`);
      equal(field.selectionStart, 4); equal(field.selectionEnd, 10);
    } finally {
      root.querySelector('.chat-groups .crow input.rename')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      closeMenu(); dispose(); root.remove();
    }
  });
}

for (const action of ['refresh', 'keyboard', 'enter'] as const) {
  check(`chat groups: renaming keeps its draft and focus through ${action}`, async () => {
    installContextMenu();
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;inset:0;overflow:auto';
    root.dataset.motion = 'none';
    document.body.append(root);
    const original = { state: api.state, saveChatGroup: api.saveChatGroup, setClock: api.setClock, setShellTexts: api.setShellTexts };
    const storage = localStorage.getItem('faber.place');
    const initial = {
      conversations: [], groups: [{ id: 'rename-group', name: 'Notes', instructions: '', files: [], conversationIds: [] }], projects: [], vendors: [],
      settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
      models: [], running: { revision: 0, items: [] }, quotas: [], version: 'test',
      look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
        appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
    } as AppState;
    let hear: ((event: AppEvent) => void) | undefined;
    let refreshes = 0;
    let saves = 0;
    api.state = async () => { refreshes++; return structuredClone(initial); };
    api.saveChatGroup = async (id, name) => {
      equal(id, 'rename-group'); equal(name, 'Unfinished group name');
      saves++;
      const saved = { ...initial.groups![0]!, name: name! };
      initial.groups = [saved];
      return structuredClone(saved);
    };
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
    const dispose = render(() => [
      createComponent(App, { initial, inbox: { take: (listener) => { hear = listener; } } }),
      createComponent(ContextMenuHost, {}),
    ], root);
    const key = (el: Element, value: KeyboardEventInit) => el.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...value }));
    try {
      await settle();
      const button = root.querySelector<HTMLButtonElement>('.chat-groups .prow .item')!;
      const groupRow = button.closest('.prow');
      button.focus({ preventScroll: true });
      key(button, { key: 'F10', shiftKey: true }); await settle();
      const rename = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((el) => el.textContent === t('menu.rename'));
      ok(rename, 'the group has its keyboard menu'); rename.click(); await settle();
      const field = root.querySelector<HTMLInputElement>('.chat-groups input.rename')!;
      equal(document.activeElement, field);
      field.value = 'Unfinished group name';
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
      if (action === 'refresh') {
        const before = refreshes;
        hear?.({ type: 'list' }); await settle();
        ok(refreshes > before, 'a completed chat refreshes the state');
        equal(root.querySelector('.chat-groups input.rename'), field, 'an unchanged group keeps its editor node');
        equal(field.value, 'Unfinished group name');
        equal(document.activeElement, field, 'a background refresh keeps keyboard focus');
      } else if (action === 'keyboard') {
        key(field, { key: 'Escape' }); await settle();
        equal(root.querySelector('.chat-groups input.rename'), null);
        equal(document.activeElement, root.querySelector('.chat-groups .prow .item'), 'Escape gives focus back to the group row');
        equal(root.querySelector('.chat-groups .prow .item .t')?.textContent, 'Notes', 'Escape cancels the unfinished name');
      } else {
        const before = refreshes;
        key(field, { key: 'Enter' }); await settle();
        equal(saves, 1, 'Enter saves the changed name');
        ok(refreshes > before, 'the save has refreshed the list');
        equal(root.querySelector('.chat-groups input.rename'), null);
        equal(root.querySelector('.chat-groups .prow .item .t')?.textContent, 'Unfinished group name');
        equal(root.querySelector('.chat-groups .prow'), groupRow, 'the changed group keeps its row through refresh');
        equal(document.activeElement, root.querySelector('.chat-groups .prow .item'), 'Enter keeps focus on the group button after the save refresh');
      }
    } finally {
      closeMenu(); dispose(); root.remove(); Object.assign(api, original);
      if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
    }
  });
}

check('chat groups: sidebar and New conversation wait for every save and refuse a failed save', async () => {
  const root = document.createElement('div'); document.body.append(root);
  const original = { state: api.state, open: api.open, saveChatGroup: api.saveChatGroup, newGroupChat: api.newGroupChat, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const storage = localStorage.getItem('faber.place');
  const group: Group = { id: 'group-barrier', name: 'Notes', instructions: '', files: [], conversationIds: [] };
  const initial = {
    conversations: [], groups: [group], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    models: [], running: { revision: 0, items: [] }, quotas: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  } as AppState;
  const releases: (() => void)[] = [];
  let starts = 0;
  let reject = false;
  api.saveChatGroup = async (_id, _name, instructions) => {
    if (reject) throw 'error.groupGone';
    await new Promise<void>((resolve) => releases.push(resolve));
    group.instructions = instructions!;
    return { ...group };
  };
  const conversation: Conversation = { id: 'group-chat', title: '', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [] };
  api.newGroupChat = async () => { starts++; group.conversationIds = [conversation.id]; return conversation; };
  api.state = async () => initial;
  api.open = async () => conversation;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', groupId: group.id }));
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
  try {
    await settle();
    const field = root.querySelector<HTMLTextAreaElement>('[data-testid=group-instructions]')!;
    for (const value of ['A', 'B']) { field.value = value; field.dispatchEvent(new InputEvent('input', { bubbles: true })); }
    await settle();
    root.querySelector<HTMLButtonElement>('.chat-groups .prow .plus')!.click();
    await settle(); equal(starts, 0, 'sidebar + waits for the first save');
    releases.shift()!(); await settle(); equal(starts, 0, 'it also waits for the queued newer save');
    releases.shift()!(); await settle(); equal(starts, 1); equal(group.instructions, 'B');
    root.querySelector<HTMLButtonElement>('.chat-groups .prow .item')!.click(); await settle();
    reject = true;
    const again = root.querySelector<HTMLTextAreaElement>('[data-testid=group-instructions]')!;
    again.value = 'unsaved'; again.dispatchEvent(new InputEvent('input', { bubbles: true })); await settle();
    root.querySelector<HTMLButtonElement>('.newchat')!.click(); await settle();
    equal(starts, 1, 'New conversation cannot bypass a failed save');
    equal(again.value, 'unsaved');
    equal(root.querySelector('.chat-group-page [role=alert]')?.textContent, t('error.groupGone'));
  } finally {
    for (const release of releases) release();
    dispose(); root.remove(); Object.assign(api, original);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('chat groups: create a folder, save its words and files, start a chat and move it out', async () => {
  installContextMenu();
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0;overflow:auto';
  root.dataset.motion = 'none';
  document.body.append(root);
  const original = { createChatGroup: api.createChatGroup, saveChatGroup: api.saveChatGroup, addChatGroupFiles: api.addChatGroupFiles, moveChatToGroup: api.moveChatToGroup };
  const [groups, setGroups] = createSignal<Group[]>([]);
  const [selected, select] = createSignal<string>();
  const [chats, setChats] = createSignal<Summary[]>([]);
  const words: string[] = [];
  const uploads: string[] = [];
  const update = (g: Group) => setGroups((all) => all.map((old) => old.id === g.id ? g : old));
  api.createChatGroup = async (name) => {
    const group = { id: 'group', name, instructions: '', files: [], conversationIds: [] };
    setGroups([group]); return group;
  };
  api.saveChatGroup = async (id, name, instructions) => {
    equal(id, 'group');
    if (instructions !== undefined) words.push(instructions);
    return { ...groups()[0]!, name: name ?? groups()[0]!.name, instructions: instructions ?? groups()[0]!.instructions };
  };
  api.addChatGroupFiles = async (_id, files) => {
    for (const f of files) { equal(atob(f.base64), 'sky notes'); uploads.push(f.name); }
    return { ...groups()[0]!, files: files.map((f) => ({ id: f.name, name: f.name, size: 9 })) };
  };
  api.moveChatToGroup = async (id, groupId) => {
    equal(id, 'chat'); equal(groupId, undefined);
    update({ ...groups()[0]!, conversationIds: [] });
  };
  const dispose = render(() => [
    createComponent(Sidebar, {
      get conversations() { return chats(); }, get groups() { return groups(); },
      get place() { return { area: 'chat' as const, groupId: selected() }; },
      projects: [], vendors: [], version: 'test', running: [], quotas: {}, now: Date.now(),
      onGroup: select, onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
      onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
    }),
    createComponent(Show, { get when() { return selected(); }, keyed: true, children: (id) => createComponent(ChatGroup, {
      get group() { return groups().find((g) => g.id === id)!; }, onChanged: update,
      onNew: async () => { update({ ...groups()[0]!, conversationIds: ['chat'] }); setChats([{ id: 'chat', title: 'Sky', area: 'chat', updatedAt: Date.now() }]); select(undefined); },
    }) }),
    createComponent(ContextMenuHost, {}),
  ], root);
  try {
    root.querySelector<HTMLButtonElement>('[data-action=new-group]')!.click();
    const name = root.querySelector<HTMLInputElement>('input.rename')!;
    name.value = 'Sky notes';
    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    equal(root.querySelector('h1')?.textContent, 'Sky notes');
    const field = root.querySelector<HTMLTextAreaElement>('[data-testid=group-instructions]')!;
    field.focus();
    for (const text of ['Be brief', 'Be brief. Use my notes.']) { field.value = text; field.dispatchEvent(new InputEvent('input', { bubbles: true })); }
    await settle();
    deepEqual(words, ['Be brief', 'Be brief. Use my notes.']);
    equal(document.activeElement, field, 'saving never replaces the editor');
    const dropped = new DataTransfer();
    dropped.items.add(new File(['sky notes'], 'sky.txt', { type: 'text/plain' }));
    root.querySelector('.chat-group-page')!.dispatchEvent(new DragEvent('drop', { dataTransfer: dropped, bubbles: true, cancelable: true }));
    await settle();
    deepEqual(uploads, ['sky.txt']);
    equal(root.querySelectorAll('.group-files li').length, 1);
    root.querySelector<HTMLButtonElement>('[data-action=new-group-chat]')!.click();
    await settle();
    const row = root.querySelector('.chat-groups .crow');
    ok(row, 'the new chat is nested in its group');
    equal(root.querySelector('.list .crow'), null, 'it is not also in the outside list');
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 }));
    await settle();
    const menu = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
    deepEqual(menu.map((el) => el.textContent), [t('menu.rename'), t('menu.pin'), t('menu.export'), t('menu.moveToGroup'), t('menu.removeFromGroup'), t('menu.archive'), t('menu.delete')]);
    menu.find((el) => el.textContent === t('menu.removeFromGroup'))!.click();
    await settle();
    equal(root.querySelector('.chat-groups .crow'), null);
    ok(root.querySelector('.list .crow'), 'removing membership keeps the chat');
  } finally { closeMenu(); dispose(); root.remove(); Object.assign(api, original); }
});

check('chat groups: file bounds reject before reading bytes and a failed instruction save keeps the draft', async () => {
  const root = document.createElement('div'); document.body.append(root);
  const original = api.saveChatGroup;
  api.saveChatGroup = async () => { throw 'error.groupGone'; };
  const group: Group = { id: 'g', name: 'Notes', instructions: '', files: [], conversationIds: [] };
  const dispose = render(() => createComponent(ChatGroup, { group, onChanged: () => {}, onNew: async () => {} }), root);
  try {
    const field = root.querySelector<HTMLTextAreaElement>('textarea')!;
    field.value = 'Keep these words'; field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    equal(field.value, 'Keep these words');
    equal(root.querySelector('[role=alert]')?.textContent, t('error.groupGone'));
    equal(root.querySelector<HTMLButtonElement>('[data-action=new-group-chat]')!.disabled, true);
    const data = new DataTransfer();
    for (let n = 0; n < 11; n++) data.items.add(new File(['x'], `${n}.txt`));
    root.querySelector('section')!.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }));
    equal(root.querySelector('[role=alert]')?.textContent, t('error.groupFiles'));
    equal(group.files.length, 0);
  } finally { dispose(); root.remove(); api.saveChatGroup = original; }
});
