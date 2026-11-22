import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation, type WebSearch } from '../../ui/api.ts';
import { appliedLook, applyLook } from '../../ui/appearance.ts';
import { closeMenu, menu } from '../../ui/contextmenu.ts';
import { inlineMediaPaths } from '../../ui/markdown.tsx';
import { showSite } from '../../ui/Site.tsx';
import { store } from '../../ui/running.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, frames, ok } from './check.ts';

const video = 'https://www.youtube.com/watch?v=lesson';
const link = 'https://example.test/lesson';
const reply = `[Lesson video](${video})\n\nRead [the lesson](${link}) here.`;

check('reply web media: real response line breaks, photo lists and extensionless image addresses', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const photo = 'https://images.example.test/photo-123?w=800';
  const clip = 'https://media.example.test/aurora.webm';
  const files = '[PNG](image.png)  \n[WAV](audio.wav)  \n[MP4](video.mp4)';
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'web-reply-real-format', options: [], from: 0, onBranch: () => {},
    replyWeb: { mode: 'live', blocked: ['images.example.test', 'media.example.test'], canOpen: true, open: async () => true },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text:
      `![Cover](${photo})\n\n0. ![First](${photo})\n1. ![Second](${photo})\n\nTimelapse:\n${clip}\n\n[Ordinary link](${photo})` }],
  }), host);
  try {
    const notices = 'This site is blocked in Settings.';
    equal(host.querySelector('.reply-web-notice span')?.textContent, notices);
    equal(host.querySelectorAll('ol[start="0"] > li .reply-web-notice').length, 2);
    equal(host.querySelectorAll('.reply-web-notice').length, 4);
    equal(host.querySelector(`.reply-web-notice a[href="${clip}"]`)?.getAttribute('href'), clip);
    ok(host.querySelector('.md')?.textContent?.includes('Timelapse:'));
    ok([...host.querySelectorAll(`a[href="${photo}"]`)].some((a) => a.textContent === 'Ordinary link' && !a.closest('.reply-web-notice')));
    deepEqual([...inlineMediaPaths(files, 'C:/workspace')], ['image.png', 'audio.wav', 'video.mp4']);
  } finally {
    dispose();
    host.remove();
  }
});

function waitFor(host: HTMLElement, selector: string) {
  return new Promise<HTMLElement>((resolve, reject) => {
    const stop = () => { clearTimeout(deadline); observer.disconnect(); };
    const read = () => {
      const element = host.querySelector<HTMLElement>(selector);
      if (element) { stop(); resolve(element); }
    };
    const observer = new MutationObserver(read);
    const deadline = setTimeout(() => { stop(); reject(new Error(`Missing ${selector}`)); }, 5000);
    observer.observe(host, { childList: true, subtree: true, attributes: true });
    read();
  });
}

check('reply web media: notices preserve links and opening the reply assigns no remote source', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const [mode, setMode] = createSignal<WebSearch | undefined>('off');
  const [canOpen, setCanOpen] = createSignal(true);
  const [blocked, setBlocked] = createSignal(['example.test']);
  const addresses = ['cover.png', 'clip.mp4', 'track.wav'].map((name) => 'https://art.example.test/' + name);
  const names = ['Picture', 'Clip', 'Track'];
  let paneOpens = 0;
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'web-reply-notice', options: [], from: 0, onBranch: () => {},
    get replyWeb() { return { mode: mode(), blocked: blocked(), canOpen: canOpen(), open: async () => { paneOpens++; return false; } }; },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text: [
      ...addresses.map((address, i) => `${i === 0 ? '!' : ''}[${names[i]}](${address})`), reply,
      '<img alt="Do not load" src="//example.test/raw.png" srcset="https://example.test/big.png 2x">',
      '<img alt="Do not load" srcset="https://example.test/only-srcset.png 2x">',
    ].join('\n\n') }],
  }), host);
  try {
    for (const [current, hosts, words] of [
      ['off', [], 'Web search is set to Off.'],
      ['off', ['example.test'], 'Web search is set to Off.'],
      ['cached', [], 'Web search is set to Stored copy.'],
      ['cached', ['example.test'], 'Web search is set to Stored copy.'],
      ['live', ['example.test'], 'This site is blocked in Settings.'],
      [undefined, [], 'Set web search to Live to preview media from the web.'],
    ] as const) {
      setMode(current);
      setBlocked([...hosts]);
      const notices = [...host.querySelectorAll('.reply-web-notice')];
      deepEqual(notices.map((notice) => notice.querySelector('span')?.textContent), addresses.map(() => words));
      deepEqual(notices.map((notice) => notice.querySelector('a')?.getAttribute('href')), addresses);
      deepEqual(notices.map((notice) => notice.querySelector('a')?.textContent), names);
      equal(host.querySelector('.md :is([src], [srcset], [poster], [data], [background], [action], [formaction], [xlink\\:href], :not(a)[href], video, audio, source, iframe)'), null);
      equal(host.querySelectorAll('.reply-web-video').length, 1);
      equal(paneOpens, 0, 'rendering a reply never opens an external page');
    }
    setBlocked(['example.test', 'youtube.com']);
    equal(host.querySelector<HTMLButtonElement>('.reply-web-play')?.disabled, true);
    ok(host.querySelector('.reply-web-video')?.textContent?.includes('This site is blocked in Settings.'));
    setCanOpen(false);
    equal(host.querySelector('.reply-web-video'), null);
    equal(host.querySelector(`a[href="${video}"]`)?.textContent, 'Lesson video');
  } finally {
    dispose();
    host.remove();
  }
});

check('reply web media: the thread grants visible HTTPS media only in Live and releases stale grants', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const original = { ...api };
  const OriginalObserver = window.IntersectionObserver;
  const originalHidden = Object.getOwnPropertyDescriptor(document, 'hidden');
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
  const visible = new Map<Element, (shown: boolean) => void>();
  class TestObserver {
    private notify: IntersectionObserverCallback;
    constructor(notify: IntersectionObserverCallback) { this.notify = notify; }
    observe(target: Element) {
      visible.set(target, (isIntersecting) => this.notify([{ target, isIntersecting } as IntersectionObserverEntry], this as unknown as IntersectionObserver));
    }
    unobserve(target: Element) { visible.delete(target); }
    disconnect() { visible.clear(); }
  }
  window.IntersectionObserver = TestObserver as unknown as typeof IntersectionObserver;
  const [mode, setMode] = createSignal<WebSearch>();
  const [blocked, setBlocked] = createSignal<string[]>([]);
  const [thread, setThread] = createSignal('web-live');
  const kinds = ['img', 'video', 'audio'];
  const addresses = ['photo-123', 'clip.mp4', 'sound.wav'].map((path) => `https://example.test/${path}?download=1`);
  const urls = kinds.map((_, index) => 'faber-media://app/' + String(index + 1).repeat(32));
  const calls: string[][] = [];
  const released: string[][] = [];
  const leases: string[] = [];
  const pending = new Map<string, (url: string) => void>();
  let finishEnter: ((lease: string) => void) | undefined;
  let hold = false;
  let refuse = false;
  Object.assign(api, {
    mediaEnter: (id: string) => {
      leases.push(id);
      return new Promise<string>((resolve) => { finishEnter = resolve; });
    },
    mediaGrantWeb: async (id: string, url: string) => {
      calls.push([id, url]);
      if (refuse) throw { key: 'error.media.webSize' };
      if (hold) return new Promise<string>((resolve) => pending.set(url, resolve));
      return urls[addresses.indexOf(url)]!;
    },
    mediaRelease: async (lease: string, url: string) => { released.push([lease, url]); },
    mediaLeave: async () => {},
    rewindRedos: async () => [],
  });
  const dispose = render(() => createComponent(Thread, {
    get conversationId() { return thread(); }, options: [], from: 0, onBranch: () => {},
    get replyWeb() { return { mode: mode(), blocked: blocked(), canOpen: false, open: async () => false }; },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text: [
      ...addresses.map((url, index) => `${index === 0 ? '!' : ''}[Media ${index}](${url})`),
      '[Ordinary link](https://example.test/image)',
      '![Insecure](http://example.test/picture.png)',
    ].join('\n\n') }],
  }), host);
  const show = (value = true) => {
    for (const notify of visible.values()) notify(value);
  };
  const sources = () => kinds.map((kind) => host.querySelector(`.reply-media-slot > ${kind}[src], .reply-media-slot > button > ${kind}[src]`)?.getAttribute('src'));
  try {
    for (const disabled of [undefined, 'off', 'cached'] as const) {
      setMode(disabled);
      show();
      equal(visible.size, 0);
    }
    setBlocked(['example.test']);
    setMode('live');
    show();
    deepEqual(leases, []);
    deepEqual(calls, []);
    setBlocked([]);
    equal(visible.size, 3, 'only supported HTTPS references get players');
    show();
    deepEqual(leases, ['web-live']);
    setMode('cached');
    finishEnter!('first-lease');
    await frames();
    deepEqual(calls, [], 'the mode is rechecked after the lease arrives');

    setMode('live');
    show();
    for (const kind of kinds) await waitFor(host, `.reply-media-slot ${kind}[src]`);
    deepEqual(sources(), urls);
    deepEqual([...calls].sort(), addresses.map((url) => ['web-live', url]).sort());
    show(false);
    await frames();
    deepEqual(sources(), [undefined, undefined, undefined]);
    deepEqual(released.splice(0).sort(), urls.map((url) => ['first-lease', url]).sort());

    for (const invalidate of ['mode', 'host', 'viewport', 'thread']) {
      hold = true;
      show();
      await frames();
      equal(pending.size, 3);
      if (invalidate === 'mode') setMode('off');
      if (invalidate === 'host') setBlocked(['example.test']);
      if (invalidate === 'viewport') show(false);
      if (invalidate === 'thread') setThread('other-thread');
      addresses.forEach((address, index) => pending.get(address)!(urls[index]!));
      pending.clear();
      await frames();
      deepEqual(sources(), [undefined, undefined, undefined]);
      deepEqual(released.splice(0).sort(), urls.map((url) => ['first-lease', url]).sort(), invalidate);
      setMode('live');
      setBlocked([]);
    }

    hold = false;
    refuse = true;
    show();
    finishEnter!('other-lease');
    const reason = await waitFor(host, '.reply-media-status[role=status]');
    ok(reason.textContent?.includes('This media is too large to preview.'));
    equal(host.querySelector(`a[href="${addresses[0]}"]`)?.textContent, 'Media 0');
    deepEqual(sources(), [undefined, undefined, undefined]);
  } finally {
    dispose();
    host.remove();
    window.IntersectionObserver = OriginalObserver;
    if (originalHidden) Object.defineProperty(document, 'hidden', originalHidden);
    else Reflect.deleteProperty(document, 'hidden');
    Object.assign(api, original);
  }
});

for (const area of ['chat', 'project'] as const) {
  check(`reply web media: ${area} opens the existing pane from the card and the link menu`, async () => {
    if (!navigator.userAgent.includes('Windows')) return;
    const previousLook = appliedLook();
    const previousPlace = localStorage.getItem('faber.place');
    const previous = { ...api };
    const projectId = area === 'project' ? 'web-reply-project' : undefined;
    const conversation: Conversation = {
      id: `web-reply-${area}`, title: 'Web reply', area, projectId, sessions: {}, createdAt: 1, updatedAt: 2,
      messages: [{ id: 'reply', role: 'assistant', text: reply, status: 'done', at: 2 }],
    };
    const initial = {
      conversations: [{ id: conversation.id, title: conversation.title, area, projectId, updatedAt: 2 }],
      projects: projectId ? [{ id: projectId, name: 'Video project', path: 'E:/video-project' }] : [],
      vendors: ['claude'], settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
      running: { revision: store.list().revision + 1, items: [] }, models: [], quotas: [], usage: [], version: 'test',
      look: {
        vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
        appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
      },
    } as AppState;
    const opened: Array<[string, string]> = [];
    const external: string[] = [];
    let rejectOpen = true;
    let pending: ((value: boolean) => void) | undefined;
    let deferOpen = false;
    Object.assign(api, {
      open: async () => conversation, state: async () => initial, warm: async () => {}, seen: async () => {},
      routeDraft: async () => {}, setClock: async () => {}, setShellTexts: async () => {},
      siteOpen: async (id: string, url: string) => {
        if (deferOpen) { deferOpen = false; return new Promise<boolean>((resolve) => { pending = resolve; }); }
        if (rejectOpen) throw { key: 'error.browser.blockedHost' };
        opened.push([id, url]); return true;
      },
      openLink: async (url: string) => { external.push(url); },
      siteTabs: async () => null, siteSize: async () => ({ now: null, sizes: [] }),
      sitePlace: async () => true, sitePark: async () => {}, notesSeen: async () => Number.MAX_SAFE_INTEGER,
    });
    localStorage.setItem('faber.place', JSON.stringify({ area, projectId, id: conversation.id }));
    store.offer(initial.running);
    await applyLook(initial.look);
    const host = document.createElement('div');
    document.body.append(host);
    const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
    try {
      const play = await waitFor(host, '.reply-web-play');
      ok(host.querySelector('.pane-site'), 'the browser is reachable again from this conversation’s top bar');
      equal(opened.length, 0, 'drawing the card does not open a site');
      play.click();
      const refusal = await waitFor(host, '.reply-web-video [role=status]');
      equal(refusal.textContent, 'This site is blocked in Settings.');
      equal(host.querySelector('.site-view'), null);
      rejectOpen = false;
      play.click();
      await waitFor(host, '.site-view');
      equal(host.querySelector<HTMLInputElement>('.site-address')?.value, video);
      showSite(undefined);
      const anchor = host.querySelector<HTMLAnchorElement>(`.md a[href="${link}"]`)!;
      anchor.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 }));
      const beside = menu()?.items.find((item) => item.id === 'open-beside');
      ok(beside);
      rejectOpen = true;
      beside.run();
      const menuRefusal = await waitFor(host, '.note.err[role=alert]');
      equal(menuRefusal.textContent, 'This site is blocked in Settings.');
      equal(host.querySelector('.site-view'), null);
      rejectOpen = false;
      beside.run();
      closeMenu();
      await waitFor(host, '.site-view');
      equal(host.querySelector<HTMLInputElement>('.site-address')?.value, link);
      deepEqual(opened, [[conversation.id, video], [conversation.id, link]]);
      showSite(undefined);
      deferOpen = true;
      play.click();
      beside.run();
      await waitFor(host, '.site-view');
      equal(host.querySelector<HTMLInputElement>('.site-address')?.value, link);
      pending!(false);
      await waitFor(host, '.reply-web-play:not([disabled])');
      equal(host.querySelector('.reply-web-video [role=status]'), null, 'a stale card request reports no failure');
      showSite(undefined);
      deferOpen = true;
      beside.run();
      play.click();
      await waitFor(host, '.site-view');
      equal(host.querySelector<HTMLInputElement>('.site-address')?.value, video);
      pending!(false);
      await frames();
      equal(menuRefusal.textContent, 'This site is blocked in Settings.', 'a stale menu request reports no failure');
      equal(external.length, 0, 'the card and the menu never fall back to the external browser');
      showSite(undefined);
      anchor.click();
      deepEqual(external, [link], 'a regular click still opens the link externally');
    } finally {
      closeMenu();
      dispose();
      host.remove();
      showSite(undefined);
      Object.assign(api, previous);
      store.offer({ revision: store.list().revision + 1, items: [] });
      if (previousPlace === null) localStorage.removeItem('faber.place');
      else localStorage.setItem('faber.place', previousPlace);
      if (previousLook) await applyLook(previousLook);
    }
  });
}
