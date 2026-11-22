import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message } from '../../ui/api.ts';
import { mediaSession, ReplyGallery } from '../../ui/Media.tsx';
import { Thread } from '../../ui/Thread.tsx';
import claudeRecording from '../fixtures/claude/reply-media.jsonl?raw';
import { check, equal, frames, ok } from './check.ts';

check('reply media: more pictures remain reachable when every gallery preview fails', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const hidden = Object.getOwnPropertyDescriptor(document, 'hidden');
  Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  const session = mediaSession('failed-gallery', '');
  const media = Array.from({ length: 6 }, (_, i) => ({
    path: `https://example.test/picture-${i + 1}.png`, name: `Picture ${i + 1}`, kind: 'picture' as const,
  }));
  session.observe = (_element, visible) => { visible(true); return () => {}; };
  session.grantWeb = async () => { throw 'error.media.webUnreachable'; };
  const dispose = render(() => createComponent(ReplyGallery, { media, session, webAllowed: () => true }), host);
  try {
    await waitForMedia(host, 'figure[aria-label="Picture 4"] a');
    equal(host.querySelectorAll('figure a').length, 4);
    const more = host.querySelector<HTMLButtonElement>('.reply-gallery-more');
    ok(more, 'the gallery has an independent entry point');
    more.focus();
    more.click();
    const viewer = await waitForMedia(document.body, '.viewer');
    await waitForMedia(viewer, 'a[href="https://example.test/picture-5.png"]');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    await waitForMedia(viewer, 'a[href="https://example.test/picture-6.png"]');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    await waitForMedia(viewer, 'a[href="https://example.test/picture-5.png"]');
  } finally {
    dispose();
    host.remove();
    if (hidden) Object.defineProperty(document, 'hidden', hidden);
    else Reflect.deleteProperty(document, 'hidden');
  }
});

check('reply media: a gallery and two players are built only from standalone Markdown references', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'reply-media',
    options: [],
    from: 0,
    onBranch: () => {},
    messages: [{
      id: 'reply',
      role: 'assistant',
      at: 0,
      files: [{ path: 'clip.mp4', size: 900 }],
      text: [
        '![First picture](first.png)', '![Second picture](second.png)', '[Clip](clip.mp4)', '`tone.wav`',
        'See [the clip](clip.mp4) here.',
        '<details open>', '<summary>Details</summary>', 'Inside the disclosure.', '</details>',
        '<img src="raw.png" alt="Raw picture">',
        '<video src="https://example.test/raw.mp4" onplay="alert(1)"></video>',
        '<audio src="https://example.test/raw.wav"></audio>',
        '<iframe src="https://example.test"></iframe>',
      ].join('\n\n'),
    }],
  }), host);
  try {
    equal(host.querySelectorAll('.reply-media-gallery figure').length, 2);
    equal(host.querySelectorAll('figure.reply-media-video').length, 1);
    equal(host.querySelectorAll('figure.reply-media-audio').length, 1);
    equal(host.querySelector('a[data-file="clip.mp4"]')?.textContent, 'the clip');
    equal(host.querySelector('details p')?.textContent, 'Inside the disclosure.');
    equal(host.querySelector('a[data-file="raw.png"]')?.textContent, 'Raw picture');
    equal(host.querySelector('img[src="raw.png"]'), null);
    equal(host.querySelector('[onplay], video[src^="https://"], audio[src^="https://"], iframe'), null);
  } finally {
    dispose();
    host.remove();
  }
});

check('reply media: streaming after raw HTML keeps the player and its temporary refusal', async () => {
  const original = { mediaEnter: api.mediaEnter, mediaGrant: api.mediaGrant, mediaLeave: api.mediaLeave };
  const grants: string[] = [];
  Object.assign(api, {
    mediaEnter: async () => 'stream-media-test',
    mediaGrant: async (_id: string, path: string) => { grants.push(path); throw 'error.media.limit'; },
    mediaLeave: async () => {},
  });
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;top:0;left:0;width:400px';
  document.body.append(host);
  const base = '<details>Notes</details>\n\n[Clip](clip.mp4)\n\nThe answer';
  const [text, setText] = createSignal(base);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'stream-reply-media', options: [], from: 0, onBranch: () => {},
    get messages(): Message[] { return [{ id: 'reply', role: 'assistant', at: 0, text: text() }]; },
  }), host);
  try {
    await waitForMedia(host, '[data-status="error"]');
    const player = host.querySelector('figure.reply-media-video');
    setText(base + ' continues.');
    await frames(3);
    equal(host.querySelector('figure.reply-media-video'), player);
    equal(grants.join(','), 'clip.mp4');
  } finally {
    dispose();
    host.remove();
    await Promise.resolve();
    Object.assign(api, original);
  }
});

check('reply media: failed writes and commands do not reserve thumbnails, and a refused read stays hidden', async () => {
  const original = { mediaEnter: api.mediaEnter, mediaGrant: api.mediaGrant, mediaLeave: api.mediaLeave };
  const grants: string[] = [];
  Object.assign(api, {
    mediaEnter: async () => 'thumbnail-test',
    mediaGrant: async (_id: string, path: string) => { grants.push(path); throw 'error.media.gone'; },
    mediaLeave: async () => {},
  });
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;top:0;left:0;width:400px';
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'thumbnail-media-test', options: [], from: 0, onBranch: () => {},
    messages: [{
      id: 'reply', role: 'assistant', at: 0, text: 'Done.',
      tools: [
        { id: 'command', name: 'Bash', input: { command: 'rm output.png' }, ok: true },
        { id: 'failed', name: 'Write', input: { file_path: 'failed.png' }, ok: false },
        { id: 'read', name: 'Read', input: { file_path: 'missing.png' }, ok: true },
      ],
    }],
  }), host);
  try {
    host.querySelector<HTMLButtonElement>('.steps-head')!.click();
    equal(host.querySelectorAll('.reply-media-thumb').length, 1);
    await frames(3);
    equal(grants.join(','), 'missing.png');
    equal(host.querySelector('.reply-media-thumb:not([hidden])'), null);
  } finally {
    dispose();
    host.remove();
    await Promise.resolve();
    Object.assign(api, original);
  }
});

check('reply media: the gallery opens on the chosen picture and requests later pictures one at a time', async () => {
  const original = { mediaEnter: api.mediaEnter, mediaGrant: api.mediaGrant, mediaRelease: api.mediaRelease, mediaLeave: api.mediaLeave };
  const requests: string[] = [];
  const released: string[] = [];
  const pixel = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==';
  Object.assign(api, {
    mediaEnter: async () => 'gallery-test',
    mediaGrant: async (_id: string, path: string) => { requests.push(path); return `${pixel}#${requests.length}`; },
    mediaRelease: async (_lease: string, url: string) => { released.push(url); },
    mediaLeave: async () => {},
  });
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;top:0;left:0;width:400px';
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'lazy-gallery-test', options: [], from: 0, onBranch: () => {},
    messages: [{ id: 'reply', role: 'assistant', at: 0, text: Array.from({ length: 6 }, (_, i) => `![Picture ${i + 1}](picture-${i + 1}.png)`).join('\n\n') }],
  }), host);
  try {
    const fourth = await waitForMedia(host, 'button[aria-label="Open Picture 4"]:not([hidden])');
    equal(requests.length, 4);
    fourth.click();
    const viewer = await waitForMedia(document.body, '.viewer');
    await waitForMedia(viewer, 'img[alt="Picture 4"]');
    equal(requests.slice(4).join(','), 'picture-4.png');
    viewer.querySelector<HTMLButtonElement>('.viewer-btn.next')!.click();
    await waitForMedia(viewer, 'img[alt="Picture 5"]');
    equal(requests.slice(4).join(','), 'picture-4.png,picture-5.png');
    ok(released.includes(`${pixel}#5`), 'moving releases the previously displayed full picture');
  } finally {
    dispose();
    host.remove();
    await Promise.resolve();
    Object.assign(api, original);
  }
});

function waitForMedia(host: HTMLElement, selector: string) {
  return new Promise<HTMLElement>((resolve, reject) => {
    const stop = () => { clearTimeout(deadline); observer.disconnect(); };
    const read = () => {
      const found = host.querySelector<HTMLElement>(selector);
      if (found) { stop(); resolve(found); }
    };
    const observer = new MutationObserver(read);
    const deadline = setTimeout(() => { stop(); reject(new Error(`Missing ${selector}`)); }, 5000);
    observer.observe(host, { childList: true, subtree: true, attributes: true });
    read();
  });
}

check('reply media: an in-sentence clip opens a player and a permanent refusal stays unavailable', async () => {
  const original = { enter: api.mediaEnter, grant: api.mediaGrant, leave: api.mediaLeave };
  let grants = 0;
  api.mediaEnter = async () => 'reply-media-test';
  api.mediaGrant = async () => {
    grants += 1;
    throw { key: grants === 1 ? 'error.media.limit' : 'error.media.gone' };
  };
  api.mediaLeave = async () => {};
  const host = document.createElement('div');
  document.body.append(host);
  const [messages, setMessages] = createSignal<Message[]>([{ id: 'reply', role: 'assistant', at: 0, text: 'Use [the clip](clip.mp4) here.' }]);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'reply-media-refusal', options: [], from: 0, onBranch: () => {},
    get messages() { return messages(); },
  }), host);
  try {
    const link = host.querySelector<HTMLAnchorElement>('a[data-file="clip.mp4"]')!;
    link.click();
    const dialog = await waitForMedia(document.body, '.reply-media-dialog[open]');
    equal(dialog.querySelector('figure')?.className, 'reply-media-video');
    const retry = await waitForMedia(dialog, '.reply-media-status .btn');
    retry.click();
    await waitForMedia(dialog, '[data-status="gone"]');
    equal(dialog.querySelector('.reply-media-status .btn'), null);
    equal(grants, 2);
    dialog.querySelector<HTMLButtonElement>('button')!.click();
    setMessages([{ id: 'reply', role: 'assistant', at: 0, text: '[The same clip](clip.mp4)' }]);
    const slot = host.querySelector('figure.reply-media-video')!;
    slot.scrollIntoView({ block: 'center' });
    await waitForMedia(host, '[data-status="gone"]');
    await frames(3);
    equal(grants, 2);
    ok(host.textContent?.includes('File unavailable'));
  } finally {
    dispose();
    host.remove();
    await Promise.resolve();
    api.mediaEnter = original.enter;
    api.mediaGrant = original.grant;
    api.mediaLeave = original.leave;
  }
});

check('reply media: the recorded Claude reply puts only bare image paragraphs in the gallery', () => {
  const text = claudeRecording.trim().split('\n').map((line) => JSON.parse(line))
    .find((event) => event.msg.type === 'assistant').msg.message.content[0].text as string;
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'recorded-reply-media',
    options: [],
    from: 0,
    onBranch: () => {},
    messages: [{ id: 'reply', role: 'assistant', vendor: 'claude', at: 0, text }],
  }), host);
  try {
    equal(host.querySelectorAll('.reply-media-gallery figure').length, 2);
    equal(host.querySelectorAll('figure.reply-media-video, figure.reply-media-audio').length, 0);
    equal(host.querySelector('a[data-file="clip.mp4"]')?.textContent, 'clip.mp4');
    equal(host.querySelector('a[data-file="tone.wav"]')?.textContent, 'tone.wav');
  } finally {
    dispose();
    host.remove();
  }
});

check('reply media: Projects show media from successful writes even with no Markdown reference', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    area: 'project',
    conversationId: 'project-reply-media',
    options: [],
    from: 0,
    onBranch: () => {},
    messages: [{
      id: 'reply', role: 'assistant', at: 0, status: 'done', text: 'The export is ready.',
      tools: [
        { id: 'write', name: 'fileChange', input: [{ path: 'tone.wav' }], ok: true },
        { id: 'pictures', name: 'fileChange', input: [{ path: 'one.png' }, { path: 'two.png' }], ok: true },
        { id: 'failed', name: 'Write', input: { file_path: 'failed.wav' }, ok: false },
        { id: 'read', name: 'Read', input: { file_path: 'existing.wav' }, ok: true },
      ],
    }],
  }), host);
  try {
    equal(host.querySelectorAll('figure.reply-media-audio').length, 1);
    equal(host.querySelector('figure.reply-media-audio')?.getAttribute('aria-label'), 'tone.wav');
    equal(host.querySelectorAll('.files .reply-media-gallery figure').length, 2);
  } finally {
    dispose();
    host.remove();
  }
});
