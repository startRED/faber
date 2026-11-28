import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AnswerPage, type Message } from '../../ui/api.ts';
import { Thread } from '../../ui/Thread.tsx';
import { t } from '../../ui/i18n.ts';
import { check, equal, frames, ok } from './check.ts';

// A picture the fake host gives for each name: a small coloured square, so each shown picture says which file it is.
const picture = (name: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><title>${name}</title><rect width="160" height="100" fill="${name.startsWith('before') ? '#c33' : '#3c3'}"/></svg>`)}`;

const answer = (id = 'reply-1', status: Message['status'] = 'done'): Message => ({ id, role: 'assistant', at: 1, text: 'The header is fixed.', status });

/** Draws a project thread over a fake host whose answers kept `pictured` (each read of a conversation gives the next map,
 * the last one stays; a conversation with no list never answers), and records what it asked for. */
async function thread(
  pictured: Record<string, AnswerPage[]>[] | Record<string, Record<string, AnswerPage[]>[]>,
  messages: Message[] = [answer()],
  options: { conversation?: () => string; style?: string } = {},
) {
  const original = { answerPictures: api.answerPictures, sitePicture: api.sitePicture };
  const asked: string[] = [];
  const shown: string[] = [];
  const lists = Array.isArray(pictured) ? { 'before-after': pictured } : pictured;
  Object.assign(api, {
    answerPictures: (id: string) => {
      asked.push(id);
      const list = lists[id];
      if (!list) return new Promise(() => {});
      const n = asked.filter((one) => one === id).length;
      return Promise.resolve(list[Math.min(n, list.length) - 1]!);
    },
    sitePicture: async (id: string, name: string) => { shown.push(`${id}/${name}`); return picture(name); },
  });
  const host = document.createElement('div');
  host.className = 'col';
  // Rows load their pictures only in view: the thread sits at the top of the screen unless a check places it elsewhere.
  host.style.cssText = options.style ?? 'position:absolute;left:0;top:0;width:600px';
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    get conversationId() { return options.conversation?.() ?? 'before-after'; },
    area: 'project', options: [], from: 0, onBranch: () => {},
    messages: [{ id: 'q', role: 'user', at: 0, text: 'Fix the header.' }, ...messages],
  }), host);
  await frames(2);
  return {
    host, asked, shown,
    close: () => { dispose(); host.remove(); Object.assign(api, original); },
  };
}

async function until(found: () => unknown, what: string, tries = 100) {
  for (let i = 0; i < tries; i++) {
    if (found()) return;
    await new Promise((done) => setTimeout(done, 20));
  }
  throw new Error(`${what} never came`);
}

check('before-after: a stored pair shows before then after for each page and opens the viewer', async () => {
  const view = await thread([{
    'reply-1': [
      { page: 1, before: 'before-reply-1-1.jpg', after: 'after-reply-1-1.jpg' },
      { page: 2, before: null, after: 'after-reply-1-2.jpg' },
      { page: 3, before: 'before-reply-1-3.jpg', after: 'after-reply-1-3.jpg' },
    ],
    'reply-0': [{ page: 1, before: 'before-reply-0-1.jpg', after: 'after-reply-0-1.jpg' }],
  }], [answer('reply-0'), answer('reply-1')]);
  try {
    await until(() => view.host.querySelectorAll('.before-after img').length === 6, 'the six pictures');
    equal(view.asked.join(','), 'before-after', 'one read for the whole thread, not one per answer');
    const rows = view.host.querySelectorAll<HTMLElement>('.before-after');
    equal(rows.length, 2, 'each pictured answer has its row');
    equal(rows[0]!.querySelectorAll('img').length, 2, 'the older answer shows its own pair');
    const row = rows[1]!;
    ok(row.getAttribute('aria-labelledby'), 'the row is named');
    equal(document.getElementById(row.getAttribute('aria-labelledby')!)?.textContent, t('answer.pictures.label'));
    const images = [...row.querySelectorAll<HTMLImageElement>('img')];
    equal(images.length, 4);
    const names = images.map((img) => decodeURIComponent(img.src).match(/<title>(.*?)<\/title>/)?.[1]);
    equal(names.join(','), 'before-reply-1-1.jpg,after-reply-1-1.jpg,before-reply-1-3.jpg,after-reply-1-3.jpg', 'before then after, page by page; a page without both is left out');
    equal(images[0]!.alt, t('answer.pictures.pageBefore', { page: 1 }));
    equal(images[3]!.alt, t('answer.pictures.pageAfter', { page: 3 }));
    equal([...row.querySelectorAll('figcaption')].map((c) => c.textContent).join(','), [1, 2, 3, 4].map((i) => t(i % 2 ? 'answer.pictures.before' : 'answer.pictures.after')).join(','));
    ok(!view.shown.includes('before-after/after-reply-1-2.jpg'), 'a page without a pair loads no picture');
    const second = images[1]!.closest('button')!;
    second.click();
    const viewer = document.querySelector<HTMLElement>('.viewer');
    ok(viewer, 'the picture viewer opened');
    await until(() => viewer.querySelector<HTMLImageElement>('.viewer-img')?.src === images[1]!.src, 'the after picture in the viewer');
    equal(viewer.querySelector('.viewer-count')?.textContent, t('viewer.count', { n: 2, total: 4 }), 'the viewer moves through this answer\'s pictures');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    await until(() => viewer.querySelector<HTMLImageElement>('.viewer-img')?.src === images[2]!.src, 'the next page\'s before picture');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until(() => !document.querySelector('.viewer'), 'the viewer closing');
  } finally {
    document.querySelector('.viewer')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    view.close();
  }
});

check('before-after: an answer with no pair, or still running, shows nothing', async () => {
  const cases: [Record<string, AnswerPage[]>, Message[], boolean][] = [
    [{}, [answer()], true],
    [{ 'reply-1': [{ page: 1, before: 'before-reply-1-1.jpg', after: null }] }, [answer()], true],
    [{ 'reply-1': [{ page: 1, before: 'before-reply-1-1.jpg', after: 'after-reply-1-1.jpg' }] }, [answer('reply-1', 'streaming')], false],
  ];
  for (const [pictured, messages, asks] of cases) {
    const view = await thread([pictured], messages);
    try {
      if (asks) await until(() => view.asked.length, 'the ask');
      await frames(3);
      equal(view.asked.length, asks ? 1 : 0, 'a thread with no finished answer does not ask');
      equal(view.host.querySelector('.before-after'), null, 'no row');
      equal(view.shown.length, 0, 'no picture loads');
    } finally { view.close(); }
  }
});

check('before-after: the thread asks again while its newest answer waits for an after picture', async () => {
  const before = { page: 1, before: 'before-reply-1-1.jpg', after: null };
  const view = await thread([{ 'reply-1': [before] }, { 'reply-1': [{ ...before, after: 'after-reply-1-1.jpg' }] }]);
  try {
    await until(() => view.host.querySelectorAll('.before-after img').length === 2, 'the pair after the second read', 300);
    equal(view.asked.join(','), 'before-after,before-after', 'the whole thread was read again, once');
  } finally { view.close(); }
});

check('before-after: a row holds its pictures only while it is in view', async () => {
  const pair = { 'reply-1': [{ page: 1, before: 'before-reply-1-1.jpg', after: 'after-reply-1-1.jpg' }] };
  const view = await thread([pair], [answer()], { style: 'position:absolute;left:0;top:300vh;width:600px' });
  try {
    await until(() => view.host.querySelector('.before-after'), 'the row');
    await frames(4);
    equal(view.shown.length, 0, 'a row below the screen loads nothing');
    view.host.style.top = '0';
    await until(() => view.host.querySelectorAll('.before-after img').length === 2, 'the pictures once in view');
    equal(view.shown.join(','), 'before-after/before-reply-1-1.jpg,before-after/after-reply-1-1.jpg');
    // Leaving the screen lets the pictures go; coming back reads them again.
    view.host.style.top = '300vh';
    await until(() => view.host.querySelectorAll('.before-after img').length === 0, 'the pictures let go off screen');
    ok(view.host.querySelector('.before-after'), 'the row itself stays');
    view.host.style.top = '0';
    await until(() => view.host.querySelectorAll('.before-after img').length === 2, 'the pictures read again in view');
    equal(view.shown.length, 4, 'each picture read once more');
  } finally { view.close(); }
});

check("before-after: another conversation on screen never shows the last one's pictures", async () => {
  const [conversation, setConversation] = createSignal('first');
  const pair = { 'reply-1': [{ page: 1, before: 'before-reply-1-1.jpg', after: 'after-reply-1-1.jpg' }] };
  // The second conversation, a branch with the same message ids, has not answered yet.
  const view = await thread({ first: [pair] }, [answer()], { conversation });
  try {
    await until(() => view.host.querySelectorAll('.before-after img').length === 2, "the first conversation's pair");
    setConversation('second');
    await frames(3);
    equal(view.asked.join(','), 'first,second', 'the new conversation is read');
    equal(view.host.querySelector('.before-after'), null, "the first conversation's row is gone while the second is read");
  } finally { view.close(); }
});
