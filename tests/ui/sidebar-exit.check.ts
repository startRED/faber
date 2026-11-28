import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type AppEvent, type Conversation } from '../../ui/api.ts';
import { createInbox } from '../../ui/inbox.ts';
import { App } from '../../ui/App.tsx';
import { closeMenu } from '../../ui/contextmenu.ts';
import { LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { hasKeptAttachments } from '../../ui/unsent.ts';
import { check, equal, frames, ok } from './check.ts';

check('motion: narrow sidebar stays visible until its exit finishes', async () => {
  const frame = document.createElement('iframe');
  frame.style.cssText = 'position:fixed;left:0;top:0;width:720px;height:480px;border:0';
  document.body.append(frame);
  try {
    const page = frame.contentDocument!;
    const view = frame.contentWindow!;
    const style = page.createElement('style');
    // Use the loaded product rules, including the checks page's system-motion override.
    style.textContent = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules].map((rule) => rule.cssText)).join('\n');
    page.head.append(style);
    page.body.innerHTML = '<div class="shell"><aside class="side shown"></aside></div>';
    const side = page.querySelector<HTMLElement>('.side')!;
    for (const mode of ['full', 'reduced', 'none']) {
      page.documentElement.dataset.motion = mode;
      side.classList.add('shown');
      equal(view.getComputedStyle(side).visibility, 'visible', `${mode}: opening is immediately visible`);
      side.getAnimations().forEach((animation) => animation.finish());
      await frames();
      side.classList.remove('shown');
      view.getComputedStyle(side).opacity;
      const animations = side.getAnimations();
      if (mode === 'full') {
        ok(animations.some((animation) => (animation as CSSTransition).transitionProperty === 'transform'), 'full motion must actually transition');
        for (const animation of animations) {
          animation.pause();
          animation.currentTime = Number(animation.effect!.getComputedTiming().endTime) / 2;
        }
        equal(view.getComputedStyle(side).visibility, 'visible', 'the closing sidebar must remain painted during its exit');
        const opacity = Number(view.getComputedStyle(side).opacity);
        ok(opacity > 0 && opacity < 1, 'the checked frame is inside the fade');
        for (const animation of animations) animation.finish();
      } else {
        equal(animations.length, 0, `${mode}: closing must be instant`);
      }
      equal(view.getComputedStyle(side).visibility, 'hidden', `${mode}: a completed exit is hidden`);
      equal(view.getComputedStyle(side).opacity, '0');
    }
  } finally {
    frame.contentDocument?.getAnimations().forEach((animation) => animation.cancel());
    frame.remove();
  }
});

async function temporaryChatScreen(vendors: AppState['vendors'], run: (root: HTMLElement, records: Map<string, Conversation>, refuse: (on: boolean) => void) => Promise<void>) {
  const original = { open: api.open, startTemporaryChat: api.startTemporaryChat, closeChat: api.closeChat, send: api.send, setClock: api.setClock, warm: api.warm, routeDraft: api.routeDraft };
  const storage = new Map(Object.entries(localStorage));
  const ordinary: Conversation = { id: 'ordinary-temp-proof', title: 'Ordinary conversation', area: 'chat', sessions: {}, createdAt: 0, updatedAt: 1, messages: [] };
  const records = new Map([[ordinary.id, ordinary]]);
  const inbox = createInbox<AppEvent>();
  let serial = 0;
  let refused = false;
  const initial: AppState = {
    vendors, models: [], quotas: [], projects: [], version: '0.0.0', running: { revision: 0, items: [] },
    conversations: [{ id: ordinary.id, title: ordinary.title, area: 'chat', updatedAt: 1 }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: { vars: themes.css.base, scheme: 'dark', motion: 'none', effects: { ambient: false, texture: false, glow: false }, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  };
  api.startTemporaryChat = async () => {
    const id = 'temporary-proof-' + ++serial;
    for (const [key, c] of records) if (c.temporary) records.delete(key);
    records.set(id, { ...ordinary, id, temporary: true, title: 'Private conversation' });
    return id;
  };
  api.closeChat = async (id) => { if (refused) throw { key: 'error.chat.deleteFailed' }; if (records.get(id)?.temporary) records.delete(id); };
  api.open = async (id) => {
    if (!records.has(id)) return null;
    for (const [key, c] of records) if (c.temporary && key !== id) records.delete(key);
    return structuredClone(records.get(id)!);
  };
  api.send = async (req) => {
    const c = records.get(req.conversationId!)!;
    c.messages = [{ id: 'private-prompt', role: 'user', text: req.text, at: 1 }, { id: 'private-reply', role: 'assistant', text: 'Synthetic reply', at: 2, status: 'done', vendor: 'codex' }];
    c.queued = [{ id: 'private-queue', text: 'Synthetic queued message', images: [], attachments: [], choice: { vendor: 'auto' }, at: 2 }];
    for (const message of c.messages) inbox.push({ type: 'message', conversationId: c.id, message });
    inbox.push({ type: 'queue', conversationId: c.id, queued: c.queued });
    return c.id;
  };
  api.setClock = async () => {};
  api.warm = async () => {};
  api.routeDraft = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
  localStorage.removeItem('faber.layout');
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty('--' + key, value);
  document.body.append(root);
  const dispose = render(() => createComponent(App, { initial, inbox }), root);
  try { await frames(3); await run(root, records, (on) => { refused = on; }); }
  finally {
    closeMenu(); dispose(); root.remove(); Object.assign(api, original); useLanguage('en');
    localStorage.clear(); for (const [key, value] of storage) localStorage.setItem(key, value);
  }
}

async function startTemporary(root: HTMLElement) {
  root.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')!.click();
  await frames();
  const item = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((el) => el.textContent?.includes(t('chat.temporary')));
  ok(item, 'the new-conversation menu offers Temporary chat');
  item!.click();
  await frames(3);
  equal(root.querySelector('[data-temporary-chat]')?.textContent, t('chat.temporaryLine'));
}

check('t520: switching conversations awaits temporary deletion and keeps its draft on failure', async () => {
  await temporaryChatScreen(['codex'], async (root, records) => {
    await startTemporary(root);
    const id = [...records.values()].find((c) => c.temporary)!.id;
    const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Synthetic private unsent words'; field.dispatchEvent(new Event('input', { bubbles: true }));
    let reject!: (error: unknown) => void;
    let closes = 0;
    const close = api.closeChat;
    api.closeChat = () => { closes++; return new Promise((_resolve, fail) => { reject = fail; }); };
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    equal(closes, 1, 'even a destination with an id must await close');
    ok(root.querySelector('[data-temporary-chat]'), 'the private screen remains while deletion waits');
    reject({ key: 'error.chat.deleteFailed' }); await frames(3);
    ok(records.has(id) && root.querySelector('[data-temporary-chat]'), 'failed deletion keeps the private conversation');
    equal(root.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'Synthetic private unsent words');
    api.closeChat = close;
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    ok(!records.has(id));
    ok(root.querySelector('.crumb')?.textContent?.includes('Ordinary conversation'));
  });
});

check('temporary chat: menu trigger has a distinct accessible name in every language', async () => {
  await temporaryChatScreen(['codex'], async (root) => {
    for (const language of LANGUAGES) {
      useLanguage(language.code);
      await frames();
      const buttons = [...root.querySelectorAll<HTMLButtonElement>('.side-start button')];
      const matches = buttons.filter((button) => (button.getAttribute('aria-label') ?? button.textContent ?? '').toLocaleLowerCase().includes(t('side.new').toLocaleLowerCase()));
      equal(matches.length, 1, language.code + ': only the primary button matches New conversation');
      equal(root.querySelector('[aria-haspopup="menu"]')?.getAttribute('aria-label'), t('side.newMenu'));
    }
  });
});

check('temporary chat: starts from the menu, keeps private drafts out of storage and is deleted on leaving', async () => {
  await temporaryChatScreen(['codex'], async (root, records, refuse) => {
    localStorage.setItem('faber.unsent.ordinary-temp-proof', 'ORDINARY-SAVED-PROOF');
    const home = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    home.value = 'ORDINARY-HOME-PROOF';
    home.dispatchEvent(new Event('input', { bubbles: true }));
    await startTemporary(root);
    const id = [...records.values()].find((c) => c.temporary)!.id;
    ok(!root.querySelector('[data-conversation-id="' + id + '"]'), 'no temporary row in recents');
    for (const language of LANGUAGES) {
      useLanguage(language.code);
      await frames();
      const mark = root.querySelector<HTMLElement>('[data-temporary-chat]')!;
      equal(mark.textContent, t('chat.temporaryLine'));
      ok(mark.scrollWidth <= mark.clientWidth + 1, 'the mark wraps in ' + language.code);
    }
    useLanguage('en');
    const input = (field: HTMLTextAreaElement, value: string) => { field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })); };
    input(root.querySelector('.composer textarea')!, 'PRIVATE-DRAFT-PROOF');
    await frames();
    ok(!JSON.stringify(Object.entries(localStorage)).includes('PRIVATE-DRAFT-PROOF'));
    root.querySelector<HTMLFormElement>('.composer')!.requestSubmit();
    await frames(4);
    ok(root.querySelector('[data-temporary-chat]'), 'the mark survives the first send');
    const mine = root.querySelector<HTMLElement>('.mine:not(.queued)')!;
    ok(mine, 'the synthetic engine events draw the sent private prompt');
    mine.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 200, clientY: 200 }));
    await frames();
    [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((el) => el.textContent?.includes(t('mine.edit')))?.click();
    const edit = root.querySelector<HTMLTextAreaElement>('.you.editing textarea');
    ok(edit, 'editing a sent private prompt stays available');
    input(edit!, 'PRIVATE-EDIT-PROOF');
    root.querySelector<HTMLButtonElement>('[aria-label="' + t('queue.editLabel') + '"]')!.click();
    await frames();
    input(root.querySelector<HTMLTextAreaElement>('.mine.queued textarea')!, 'PRIVATE-QUEUE-PROOF');
    root.querySelector<HTMLButtonElement>('[aria-label="' + t('aside.title') + '"]')!.click();
    await frames(3);
    const aside = root.querySelector<HTMLTextAreaElement>('#aside-box');
    ok(aside, 'the quick question stays available in temporary Chat');
    input(aside!, 'PRIVATE-ASIDE-PROOF');
    root.querySelector<HTMLButtonElement>('[aria-label="' + t('aside.title') + '"]')!.click();
    input(root.querySelector('.composer textarea')!, 'PRIVATE-UNSENT-PROOF');
    await frames();
    ok(!JSON.stringify(Object.entries(localStorage)).includes('PRIVATE-'), 'sent edits and queued edits never persist either');
    ok(!hasKeptAttachments(id), 'temporary composer has no attachment persistence key');
    refuse(true);
    root.querySelector<HTMLButtonElement>('.newchat')!.click();
    await frames(3);
    ok(records.has(id) && root.querySelector('[data-temporary-chat]'), 'failed close keeps the private conversation');
    equal(root.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'PRIVATE-UNSENT-PROOF');
    refuse(false);
    root.querySelector<HTMLButtonElement>('.newchat')!.click();
    await frames(3);
    ok(!records.has(id) && !root.querySelector('[data-temporary-chat]'), 'new conversation deletes the temporary chat');
    ok(records.has('ordinary-temp-proof'), 'ordinary history survives');
    equal(root.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'ORDINARY-HOME-PROOF', 'leaving temporary Chat preserves the home draft');
    await startTemporary(root);
    const switched = [...records.values()].find((c) => c.temporary)!.id;
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    ok(!records.has(switched) && !root.querySelector('[data-temporary-chat]'), 'opening another conversation leaves the temporary chat');
    equal(root.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'ORDINARY-SAVED-PROOF', 'opening ordinary Chat preserves its saved draft');
    root.querySelectorAll<HTMLButtonElement>('.area')[1]!.click();
    await frames(3);
    ok(!root.querySelector('[aria-label="' + t('side.newMenu') + '"]'), 'Projects has no temporary-chat entry');
  });
});

check('temporary chat: with no AI connected the quiet mark remains visible', async () => {
  await temporaryChatScreen([], async (root, records) => {
    await startTemporary(root);
    ok([...records.values()].some((c) => c.temporary));
    root.querySelector<HTMLButtonElement>('.newchat')!.click();
    await frames(3);
    ok(![...records.values()].some((c) => c.temporary));
  });
});

check('temporary chat: a delayed opening never saves private typing before the snapshot arrives', async () => {
  await temporaryChatScreen(['codex'], async (root) => {
    const open = api.open;
    let finish: (() => void) | undefined;
    api.open = async (id) => {
      const c = await open(id);
      return c?.temporary ? new Promise<Conversation>((resolve) => { finish = () => resolve(c); }) : c;
    };
    await startTemporary(root);
    const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'PRIVATE-OPENING-PROOF';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await frames();
    ok(!JSON.stringify(Object.entries(localStorage)).includes('PRIVATE-OPENING-PROOF'));
    ok(finish, 'the temporary snapshot is still delayed');
    finish!();
    await frames(3);
    equal(root.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'PRIVATE-OPENING-PROOF');
    root.querySelector<HTMLButtonElement>('.newchat')!.click();
    await frames(3);
    ok(!root.querySelector('[data-temporary-chat]'));
  });
});

check('temporary chat: a delayed close never replaces a newer navigation', async () => {
  await temporaryChatScreen(['codex'], async (root) => {
    await startTemporary(root);
    const close = api.closeChat;
    let finish: (() => void) | undefined;
    api.closeChat = (id) => new Promise<void>((resolve, reject) => { finish = () => { void close(id).then(resolve, reject); }; });
    root.querySelector<HTMLButtonElement>('.newchat')!.click();
    await frames();
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    ok(finish);
    finish!();
    await frames(3);
    ok(root.querySelector('.crumb')?.textContent?.includes('Ordinary conversation'), 'the newer conversation remains open');
  });
});

check('temporary chat: a late ordinary send cannot reopen its chat and delete the newer temporary chat', async () => {
  await temporaryChatScreen(['codex'], async (root, records) => {
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    const send = api.send;
    let finish: (() => void) | undefined;
    api.send = async (req) => {
      const id = await send(req);
      return new Promise<string>((resolve) => { finish = () => resolve(id); });
    };
    const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Synthetic delayed ordinary send';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    root.querySelector<HTMLFormElement>('.composer')!.requestSubmit();
    await frames(3);
    await startTemporary(root);
    const id = [...records.values()].find((c) => c.temporary)!.id;
    ok(finish);
    finish!();
    await frames(3);
    ok(records.has(id) && root.querySelector('[data-temporary-chat]'), 'the newer temporary conversation stays open');
  });
});

check('temporary chat: a delayed start is discarded after a newer navigation', async () => {
  await temporaryChatScreen(['codex'], async (root, records) => {
    const start = api.startTemporaryChat;
    let finish: (() => void) | undefined;
    api.startTemporaryChat = async () => {
      const id = await start();
      return new Promise<string>((resolve) => { finish = () => resolve(id); });
    };
    root.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')!.click();
    await frames();
    [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((el) => el.textContent?.includes(t('chat.temporary')))!.click();
    await frames();
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames(3);
    ok(finish);
    finish!();
    await frames(3);
    ok(![...records.values()].some((c) => c.temporary));
    ok(root.querySelector('.crumb')?.textContent?.includes('Ordinary conversation'));
  });
});

check('temporary chat: a failed start does not strand an ordinary conversation that is still opening', async () => {
  await temporaryChatScreen(['codex'], async (root) => {
    const open = api.open;
    let finish: (() => void) | undefined;
    api.open = async (id) => {
      const c = await open(id);
      return new Promise<Conversation | null>((resolve) => { finish = () => resolve(c); });
    };
    root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-temp-proof"] .item')!.click();
    await frames();
    api.startTemporaryChat = async () => { throw { key: 'error.update.activeOperations' }; };
    root.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')!.click();
    await frames();
    [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((el) => el.textContent?.includes(t('chat.temporary')))!.click();
    await frames();
    ok(finish);
    finish!();
    await frames(3);
    const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Synthetic recovered draft';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await frames();
    ok(!root.querySelector<HTMLButtonElement>('.composer .send')!.disabled, 'ordinary Chat finishes opening and accepts a message');
  });
});
