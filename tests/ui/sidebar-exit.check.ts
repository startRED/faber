import { check, equal, frames, ok } from './check.ts';
import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation } from '../../ui/api.ts';
import { closeMenu, installContextMenu } from '../../ui/contextmenu.ts';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { forgetPrivateDrafts, keep, kept } from '../../ui/unsent.ts';

for (const [language, exit, connected] of [
  ['en', 'conversation', true], ['de', 'new', true], ['fr', 'shortcut', true],
  ['es', 'project', true], ['pt-BR', 'new', false], ['en', 'failure', true], ['en', 'loading', true],
] as const) {
  check(`temporary chat: ${language}, leave by ${exit}, connected ${connected}, at 720 px`, async () => {
    installContextMenu();
    useLanguage(language);
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;left:0;top:0;width:720px;height:800px';
    document.body.append(root);
    const before = { ...api };
    const storage = localStorage.getItem('faber.place');
    const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
    localStorage.removeItem('faber.place');
    const ordinary: Conversation = { id: 'ordinary-chat', area: 'chat', title: 'Saved chat', messages: [], sessions: {}, createdAt: 1, updatedAt: 1 };
    let temporary: Conversation | undefined;
    let starts = 0;
    let finishOpen: (() => void) | undefined;
    const closed: string[] = [];
    const initial = {
      conversations: [{ id: ordinary.id, title: ordinary.title, area: 'chat', updatedAt: 1 }], projects: [],
      vendors: connected ? ['claude'] : [], models: [], quotas: [], running: { revision: 0, items: [] }, version: 'test',
      settings: { setup: 'done', tutorialDone: true, language, appearance: { base: 'sala' } },
      look: { vars: {}, scheme: 'dark', motion: 'reduced', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
        appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 18, density: 1, corners: 1, motion: 'reduced', effects: { ambient: false, texture: false, glow: false } } },
    } as AppState;
    api.state = async () => structuredClone(initial);
    api.startTemporaryChat = async () => {
      starts++;
      temporary = { ...ordinary, id: 'private-chat', title: 'Private chat', temporary: true };
      return temporary.id;
    };
    api.open = async (id) => {
      if (id === ordinary.id) return ordinary;
      if (exit === 'loading' && temporary) {
        const snapshot = temporary;
        return new Promise<Conversation>((resolve) => { finishOpen = () => resolve(snapshot); });
      }
      return temporary ?? null;
    };
    api.closeChat = async (id) => {
      closed.push(id);
      if (exit === 'failure') throw 'error.chat.deleteFailed';
      temporary = undefined;
    };
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
    const dispose = render(() => [createComponent(App, { initial, inbox: { take: () => {} } }), createComponent(ContextMenuHost, {})], root);
    const openSide = async () => { root.querySelector<HTMLButtonElement>('.topbar .menu')!.click(); await settle(); };
    try {
      await settle();
      await openSide();
      const trigger = root.querySelector<HTMLButtonElement>('[data-action="new-chat-menu"]');
      ok(trigger, 'Chat offers its start menu beside New conversation');
      trigger.focus(); trigger.click(); await frames();
      const menu = document.querySelector<HTMLElement>('.cmenu:not(.leaving)')!;
      ok(menu, 'the existing Faber menu opens');
      equal(trigger.getAttribute('aria-expanded'), 'true');
      const items = menu.querySelectorAll<HTMLButtonElement>('[role=menuitem]');
      equal(items[0]!.textContent, t('side.new'));
      equal(items[1]!.textContent, t('chat.temporary'));
      await document.fonts.ready;
      const text = document.createRange(); text.selectNodeContents(items[1]!);
      const bounds = text.getBoundingClientRect();
      ok(bounds.left >= 0 && bounds.right <= 720, `${language}: menu words fit at 720 px`);
      items[0]!.focus();
      items[0]!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
      equal(document.activeElement, items[1], 'arrows reach the temporary choice');
      items[1]!.click(); await settle();
      equal(starts, 1);
      if (exit === 'loading') {
        ok(finishOpen, 'the temporary snapshot is still loading');
        ok(root.querySelector('[data-temporary-chat]'), 'the known temporary id shows its notice before loading finishes');
        const transfer = new DataTransfer();
        transfer.items.add(new File(['Synthetic private attachment'], 'private.pdf', { type: 'application/pdf' }));
        root.querySelector('.composer')!.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
        await settle();
        ok(root.querySelector('.attach-row'), 'attachments are usable while the temporary snapshot loads');
        equal(localStorage.getItem('faber.unsent-attachments.private-chat'), null, 'loading never gives private attachments a persistent draft key');
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, bubbles: true, cancelable: true }));
        await settle(); finishOpen(); await settle();
        equal(closed.join(','), 'private-chat', 'leaving closes the known temporary id before its snapshot arrives');
        equal(temporary, undefined);
        equal(root.querySelector('[data-temporary-chat]'), null, 'the late temporary snapshot never replaces the new chat');
        return;
      }
      const mark = root.querySelector<HTMLElement>('[data-temporary-chat]');
      ok(mark, 'the temporary chat explains when it is deleted, also without a connected AI');
      equal(mark.textContent, t('chat.temporaryNotice'));
      equal(root.querySelector('.side [data-conversation-id="private-chat"]'), null, 'temporary Chat is absent from recents');
      for (const key of ['aside.private-chat', 'edit.private-chat.user-message', 'queued-edit.private-chat.queued-message', `piece-edit.${JSON.stringify(['private-chat', 'answer', 0, { type: 'checklist', items: ['Synthetic private label'] }])}`]) {
        keep(key, 'Synthetic private edit');
        equal(kept(key), 'Synthetic private edit', 'private editors retain words in memory');
        equal(localStorage.getItem(`faber.unsent.${key}`), null, 'private editors never save words to disk');
      }
      const markText = document.createRange(); markText.selectNodeContents(mark);
      const markBounds = markText.getBoundingClientRect();
      ok(markBounds.width > 0 && markBounds.left >= 0 && markBounds.right <= 720, `${language}: notice words fit`);
      if (connected) {
        const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
        ok(field.getAttribute('aria-describedby')?.includes(mark.id), 'the field describes its temporary lifetime');
        field.value = 'Synthetic private draft'; field.dispatchEvent(new InputEvent('input', { bubbles: true }));
        await settle();
        equal(localStorage.getItem('faber.unsent.private-chat'), null, 'private draft text never reaches browser storage');
      }
      if (exit === 'shortcut') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, bubbles: true, cancelable: true }));
      else {
        await openSide();
        if (exit === 'conversation') root.querySelector<HTMLButtonElement>('[data-conversation-id="ordinary-chat"] .item')!.click();
        else if (exit === 'project') root.querySelector<HTMLButtonElement>('.area[data-tour="projects"]')!.click();
        else root.querySelector<HTMLButtonElement>('.newchat')!.click();
      }
      await settle();
      equal(closed.join(','), 'private-chat', 'leaving calls close_chat exactly once');
      if (exit === 'failure') {
        ok(root.querySelector('[data-temporary-chat]'), 'a refused close leaves the temporary chat on screen for retry');
        ok(root.querySelector('[role=alert]'), 'the deletion error is visible');
      } else {
        equal(temporary, undefined, 'the host removed the temporary record');
        equal(root.querySelector('[data-temporary-chat]'), null, 'the quiet mark goes with its conversation');
        equal(await api.open('private-chat'), null, 'the closed chat cannot be reopened');
        equal(kept('aside.private-chat'), '', 'leaving releases private editor drafts');
        if (exit === 'project') equal(root.querySelector('[data-action="new-chat-menu"]'), null, 'Projects never offers temporary Chat');
      }
      ok(!closed.includes(ordinary.id), 'ordinary conversations are never closed destructively');
    } finally {
      finishOpen?.(); closeMenu(); dispose(); forgetPrivateDrafts('private-chat'); root.remove(); Object.assign(api, before);
      Object.defineProperty(window, 'innerWidth', width);
      if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
      useLanguage('en');
    }
  });
}

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
