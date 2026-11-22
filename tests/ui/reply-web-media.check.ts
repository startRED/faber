import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation, type WebSearch } from '../../ui/api.ts';
import { appliedLook, applyLook } from '../../ui/appearance.ts';
import { closeMenu, menu } from '../../ui/contextmenu.ts';
import { inlineMediaPaths } from '../../ui/markdown.tsx';
import { mediaSession, ReplyGallery } from '../../ui/Media.tsx';
import { ReplyWebPictures, webReply } from '../../ui/ReplyWebMedia.tsx';
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
    ok(host.querySelector('.reply-web-reason svg[aria-hidden="true"]'), 'the app notice has a decorative icon');
    equal(host.querySelectorAll('ol[start="0"] > li .reply-web-notice').length, 2);
    const items = [...host.querySelectorAll('ol[start="0"] > li')];
    const standalone = host.querySelector<HTMLElement>('.reply-web-notice')!;
    for (const item of items) {
      const notice = item.querySelector<HTMLElement>('.reply-web-notice')!;
      equal(getComputedStyle(notice).borderBlockStartWidth, '0px');
      ok(item.getBoundingClientRect().height <= standalone.getBoundingClientRect().height, 'a media item adds no height to the standalone notice');
    }
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

check('reply web media: adjacent pictures share galleries and inline images follow their paragraph', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const photo = (name: string) => `![${name}](https://example.test/${name}.jpg)`;
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'web-picture-groups', options: [], from: 0, onBranch: () => {},
    replyWeb: { mode: 'live', blocked: [], canOpen: false, open: async () => false },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text: [
      photo('First'), photo('Second'), 'Hard breaks:', `${photo('Third')}  \n${photo('Fourth')}`,
      'List:', `1. ${photo('Fifth')}\n2. ${photo('Sixth')}`, 'Ordinary list:', '1. Ordinary\n2. List',
      `Here is **${photo('In-sentence')}** the photo. A [link](https://example.test/regular.jpg) stays here.`,
      'Here is the local ![Local](./aurora.png) image.',
    ].join('\n\n') }],
  }), host);
  try {
    deepEqual([...host.querySelectorAll('.reply-media-gallery')].map(gallery =>
      [...gallery.querySelectorAll('figure')].map(figure => figure.getAttribute('aria-label'))),
    [['First', 'Second'], ['Third', 'Fourth'], ['Fifth', 'Sixth']]);
    const paragraph = [...host.querySelectorAll('.md p')].find(p => p.textContent?.startsWith('Here is '))!;
    equal(paragraph.querySelector('a')?.getAttribute('href'), 'https://example.test/regular.jpg');
    equal(paragraph.querySelector('img'), null);
    ok(paragraph.textContent?.includes('In-sentence'));
    equal(paragraph.nextElementSibling?.querySelector('figure')?.getAttribute('aria-label'), 'In-sentence');
    const local = [...host.querySelectorAll('.md p')].find(p => p.textContent?.startsWith('Here is the local'))!;
    equal(local.nextElementSibling?.querySelector('figure')?.getAttribute('aria-label'), 'Local');
    equal(host.querySelector('ol > li')?.textContent, 'Ordinary');
    equal(host.querySelector('ol[data-reply-picture-list]'), null);
  } finally {
    dispose();
    host.remove();
  }
});

check('reply web media: sentence pictures stay inside their list item or quote', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const photo = 'https://example.test/extensionless';
  const text = [
    `- Tight **![Web](${photo})** after [link](${link}).\n  - [x] Nested ![Local](./nested.png) after.`,
    '3. Loose ![Loose](./loose.png) after.\n\n   Next paragraph.',
    `> Quote [![Quoted](${photo})](${link}) after.\n>\n> Next quote paragraph.`,
    `<div data-reply-media="fake"><img src="${photo}" alt="Raw" onerror="alert(1)"></div>`,
  ].join('\n\n');
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'web-picture-containers', options: [], from: 0, onBranch: () => {},
    replyWeb: { mode: 'live', blocked: [], canOpen: false, open: async () => false },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text }],
  }), host);
  try {
    const tight = host.querySelector('.md ul > li')!;
    const nested = tight.querySelector('ul > li')!;
    equal(tight.firstChild?.textContent, 'Tight ');
    equal(tight.querySelector('strong')?.textContent, 'Web');
    equal(tight.querySelector(':scope > ul')?.previousElementSibling?.querySelector('figure')?.getAttribute('aria-label'), 'Web',
      'the tight item keeps its picture after the words and before the nested list');
    equal(tight.querySelector('a')?.getAttribute('href'), link);
    equal(nested.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked, true);
    equal(nested.lastElementChild?.querySelector('figure')?.getAttribute('aria-label'), 'Local');
    const loose = host.querySelector('.md ol[start="3"] > li')!;
    const quote = host.querySelector('.md blockquote')!;
    for (const [container, name] of [[loose, 'Loose'], [quote, 'Quoted']] as const) {
      equal(container.firstElementChild?.tagName, 'P');
      const picture = container.firstElementChild?.nextElementSibling;
      equal(picture?.querySelector('figure')?.getAttribute('aria-label'), name);
      equal(picture?.nextElementSibling?.tagName, 'P');
    }
    equal(quote.querySelector('p > a')?.getAttribute('href'), link);
    equal(quote.querySelector('p > a')?.textContent, 'Quoted');
    deepEqual([...inlineMediaPaths(text, 'C:/workspace')], ['nested.png', 'loose.png']);
    equal(host.querySelectorAll('.md figure').length, 4);
    equal(host.querySelector('.md img[src], .md [onerror]'), null);
  } finally {
    dispose();
    host.remove();
  }
});

check('reply web media: isolated pictures stay in lists and quotes', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const text = [
    '- ![Local one](./one.png)\n- ![Local two](./two.png)',
    'Mixed:',
    '- ![Mixed local](./mixed.png)\n- ![Mixed web](https://example.test/mixed.jpg)',
    'With words:',
    '- Prose with [link](https://example.test/page)\n- [x] ![Checked](./checked.png)',
    '> ![Quoted alone](./quote.png)',
    'Linked:',
    '> [![Linked alone](./linked.png)](https://example.test/page)',
    '- ![Web one](https://example.test/one.jpg)\n- ![Web two](https://example.test/two.jpg)',
  ].join('\n\n');
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'isolated-pictures', options: [], from: 0, onBranch: () => {},
    replyWeb: { mode: 'live', blocked: [], canOpen: false, open: async () => false },
    messages: [{ id: 'reply', role: 'assistant', at: 0, text }],
  }), host);
  try {
    const lists = host.querySelectorAll('.md ul');
    for (const [index, names] of [[0, ['Local one', 'Local two']], [1, ['Mixed local', 'Mixed web']]] as const) {
      const items = lists[index]!.querySelectorAll(':scope > li');
      names.forEach((name, i) => {
        equal(items[i]!.querySelectorAll(`figure[aria-label="${name}"]`).length, 1);
        equal(items[i]!.querySelector('figcaption')?.textContent, name);
        equal(items[i]!.textContent?.split(name).length, 2);
        equal(items[i]!.querySelector('p'), null);
      });
    }
    const items = lists[2]!.querySelectorAll(':scope > li');
    equal(items[0]!.querySelector('a')?.getAttribute('href'), 'https://example.test/page');
    equal(items[1]!.querySelector<HTMLInputElement>('input')?.checked, true);
    equal(items[1]!.querySelectorAll('figure[aria-label="Checked"]').length, 1);
    equal(items[1]!.textContent?.split('Checked').length, 2);
    equal(host.querySelectorAll('blockquote figure[aria-label="Quoted alone"]').length, 1);
    equal(host.querySelector('blockquote')?.textContent?.split('Quoted alone').length, 2);
    equal(host.querySelector('blockquote')?.querySelector('p'), null);
    equal(host.querySelectorAll('blockquote')[1]?.querySelector('a')?.textContent, 'Linked alone');
    deepEqual([...inlineMediaPaths(text, 'C:/workspace')], ['one.png', 'two.png', 'mixed.png', 'checked.png', 'quote.png']);
    ok([...host.querySelectorAll('.reply-media-gallery')].some(gallery =>
      gallery.querySelectorAll('figure').length === 2 && gallery.querySelector('figure[aria-label="Web one"]')));
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

check('reply web media: the shared viewer releases each full web picture when navigating', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const hidden = Object.getOwnPropertyDescriptor(document, 'hidden');
  Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  const pixel = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==';
  const session = mediaSession('web-gallery', '');
  const requests: string[] = [];
  const released: string[] = [];
  session.observe = (_element, visible) => { visible(true); return () => {}; };
  session.grantWeb = async (path, eligible) => {
    ok(eligible(), 'web media rechecks eligibility');
    if (path.endsWith('/Missing')) throw 'error.media.webUnreachable';
    requests.push(path);
    return `${pixel}#${requests.length}`;
  };
  session.release = url => { released.push(url); };
  const media = ['First', 'Second', 'Missing'].map(name => ({ path: 'https://example.test/' + name, name, kind: 'picture' as const }));
  const dispose = render(() => createComponent(ReplyGallery, { media, session, webAllowed: () => true }), host);
  try {
    await waitFor(host, 'a[href="https://example.test/Missing"]');
    const button = await waitFor(host, 'figure[aria-label="First"] button:not([hidden])');
    button.click();
    const viewer = await waitFor(document.body, '.viewer');
    const first = await waitFor(viewer, 'img[alt="First"]');
    const firstSource = first.getAttribute('src')!;
    viewer.querySelector<HTMLButtonElement>('.viewer-btn.next')!.click();
    await waitFor(viewer, 'img[alt="Second"]');
    ok(released.includes(firstSource), 'leaving a picture releases its full grant');
    viewer.querySelector<HTMLButtonElement>('.viewer-btn.next')!.click();
    const missing = await waitFor(viewer, 'a[href="https://example.test/Missing"]');
    equal(missing.textContent, 'Missing');
    viewer.querySelector<HTMLButtonElement>('.viewer-btn.prev')!.click();
    await waitFor(viewer, 'img[alt="Second"]');
    viewer.querySelector<HTMLButtonElement>('.viewer-btn.prev')!.click();
    await waitFor(viewer, 'img[alt="First"]');
    deepEqual(requests.slice(2), [media[0]!.path, media[1]!.path, media[1]!.path, media[0]!.path]);
  } finally {
    dispose();
    host.remove();
    if (hidden) Object.defineProperty(document, 'hidden', hidden);
    else Reflect.deleteProperty(document, 'hidden');
  }
});

check('reply web media: equivalent policy updates keep the open gallery viewer', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const hidden = Object.getOwnPropertyDescriptor(document, 'hidden');
  Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  const [blocked, setBlocked] = createSignal<string[]>([]);
  const session = mediaSession('gallery-policy', '');
  session.observe = (_element, visible) => { visible(true); return () => {}; };
  session.grantWeb = async () => { throw 'error.media.webUnreachable'; };
  const pictures = Array.from({ length: 5 }, (_, i) => webReply(`https://example.test/${i}.jpg`, `Picture ${i + 1}`, true)!);
  const dispose = render(() => createComponent(ReplyWebPictures, {
    pictures, session, mode: 'live', canOpen: false, open: async () => false,
    get blocked() { return blocked(); },
  }), host);
  try {
    await waitFor(host, '.reply-gallery-more').then(button => button.click());
    const gallery = host.querySelector('.reply-media-gallery');
    const viewer = await waitFor(document.body, '.viewer');
    await waitFor(viewer, 'a[href="https://example.test/4.jpg"]');
    setBlocked(['unrelated.test']);
    equal(host.querySelector('.reply-media-gallery'), gallery);
    equal(document.querySelector('.viewer'), viewer);
    setBlocked(['example.test']);
    equal(host.querySelector('.reply-media-gallery'), null);
    equal(document.querySelector('.viewer'), null);
  } finally {
    dispose();
    host.remove();
    if (hidden) Object.defineProperty(document, 'hidden', hidden);
    else Reflect.deleteProperty(document, 'hidden');
  }
});

check('reply web media: a busy preview retries once at the site delay while visible', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const hidden = Object.getOwnPropertyDescriptor(document, 'hidden');
  Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  const session = mediaSession('busy-gallery', '');
  const names = ['Busy', 'Blocked', 'Long', 'Scrolled away'];
  const pictures = names.map((name, i) => ({ path: `https://example.test/${i}`, name, kind: 'picture' as const }));
  const requests = new Map<string, number>();
  let hideLast: (() => void) | undefined;
  session.observe = (element, visible) => {
    if (element.querySelector('figure')?.getAttribute('aria-label') === names[3]) hideLast = () => visible(false);
    visible(true);
    return () => {};
  };
  session.grantWeb = async (path, eligible) => {
    ok(eligible(), 'every attempt rechecks the preview');
    requests.set(path, (requests.get(path) ?? 0) + 1);
    if (path === pictures[1]!.path) throw 'error.media.webBlocked';
    throw { key: 'error.media.webBusy', detail: path === pictures[2]!.path ? '60' : '1' };
  };
  const dispose = render(() => createComponent(ReplyGallery, { media: pictures, session, webAllowed: () => true }), host);
  try {
    await waitFor(host, 'figure[aria-label="Blocked"] a');
    ok(hideLast, 'registered the last preview');
    hideLast();
    equal(requests.get(pictures[0]!.path), 1);
    await waitFor(host, 'figure[aria-label="Busy"] a');
    deepEqual(pictures.map(({ path }) => requests.get(path)), [2, 1, 1, 1]);
  } finally {
    dispose();
    host.remove();
    if (hidden) Object.defineProperty(document, 'hidden', hidden);
    else Reflect.deleteProperty(document, 'hidden');
  }
});

check('reply web media: notices preserve links and opening the reply assigns no remote source', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const [mode, setMode] = createSignal<WebSearch | undefined>('off');
  const [canOpen, setCanOpen] = createSignal(true);
  const [blocked, setBlocked] = createSignal(['example.test']);
  const addresses = ['cover.png', 'clip.mp4', 'track.wav'].map((name) => 'https://art.example.test/' + name);
  const names = ['Picture', 'Clip', 'Track'];
  const direct = addresses.map((address, i) => `${i === 0 ? '!' : ''}[${names[i]}](${address})`);
  const untitled = 'https://www.youtube.com/watch?v=untitled';
  const otherUntitled = 'https://www.youtube.com/watch?v=another';
  let paneOpens = 0;
  let settingsOpens = 0;
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'web-reply-notice', options: [], from: 0, onBranch: () => {},
    get replyWeb() { return { mode: mode(), blocked: blocked(), canOpen: canOpen(), onSettings: () => { settingsOpens++; }, open: async () => { paneOpens++; return false; } }; },
    messages: [{
      id: 'reply', role: 'assistant', at: 0, thinking: 'Checking sources.',
      thinkingMarks: [{ from: 0, at: direct[0]!.length, steps: 0 }],
      text: [
        ...direct, reply, untitled, otherUntitled,
        '<img alt="Do not load" src="//example.test/raw.png" srcset="https://example.test/big.png 2x">',
        '<img alt="Do not load" srcset="https://example.test/only-srcset.png 2x">',
      ].join('\n\n'),
    }, {
      id: 'ordinary', role: 'assistant', at: 1,
      text: '<p><a data-reply-web-off="" href="https://example.test/ordinary">Plain link</a></p>',
    }],
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
      if (current === 'off') {
        equal(notices.length, 0);
        const hints = [...host.querySelectorAll('.reply-web-off-summary')].filter(hint => getComputedStyle(hint).display !== 'none');
        equal(hints.length, 1);
        ok(hints[0]?.textContent?.includes(words));
        ok(hints[0]?.textContent?.includes('Live') && hints[0].textContent.includes('Web search'));
        const links = [...host.querySelectorAll<HTMLAnchorElement>('a[data-reply-web-off]')];
        deepEqual(links.map(link => link.getAttribute('href')), addresses);
        deepEqual(links.map(link => link.textContent), names);
        hints[0]?.querySelector('button')?.click();
        equal(settingsOpens, hosts.length ? 2 : 1);
        equal(mode(), 'off');
      } else {
        equal(host.querySelector('.reply-web-off-summary'), null);
        deepEqual(notices.map((notice) => notice.querySelector('span')?.textContent), addresses.map(() => words));
        deepEqual(notices.map((notice) => notice.querySelector('a')?.getAttribute('href')), addresses);
        deepEqual(notices.map((notice) => notice.querySelector('a')?.textContent), names);
      }
      equal(host.querySelector('.md :is([src], [srcset], [poster], [data], [background], [action], [formaction], [xlink\\:href], :not(a)[href], video, audio, source, iframe)'), null);
      equal(host.querySelectorAll('.reply-web-video').length, 3);
      for (const address of [untitled, otherUntitled]) {
        const link = host.querySelector(`.reply-web-video a[href="${address}"]`);
        equal(link?.textContent, 'Video on YouTube');
        equal(link?.getAttribute('title'), address);
        equal(link?.closest('.reply-web-video')?.querySelector('button')?.getAttribute('aria-description'), address);
      }
      equal(host.querySelector(`.reply-web-video a[href="${video}"]`)?.textContent, 'Lesson video');
      equal(paneOpens, 0, 'rendering a reply never opens an external page');
    }
    setBlocked(['example.test', 'youtube.com']);
    equal(host.querySelector<HTMLButtonElement>('.reply-web-play')?.disabled, true);
    ok(host.querySelector('.reply-web-video')?.textContent?.includes('This site is blocked in Settings.'));
    setCanOpen(false);
    equal(host.querySelector('.reply-web-video'), null);
    equal(host.querySelector(`a[href="${video}"]`)?.textContent, 'Lesson video');
    equal(host.querySelector(`a[href="${untitled}"]`)?.textContent, untitled);
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
      ...addresses.map((url, index) => index === 0 ? `- Before ![Media 0](${url}) after.` : `[Media ${index}](${url})`),
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
