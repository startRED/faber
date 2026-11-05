import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { AnswerChanges } from '../../ui/AnswerChanges.tsx';
import { api, type AnswerChanges as Review, type FileChange, type Message, type RewindPoint } from '../../ui/api.ts';
import { Thread } from '../../ui/Thread.tsx';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import themes from '../../src-tauri/src/themes.json';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const point: RewindPoint = { id: 'saved-point', messageId: 'question', turnId: 'answer', at: 1, sealed: true, cursor: { entry: null, turn: null }, coverage: { saved: 2, skipped: 0, incomplete: false, unavailable: false } };
const file = (path = 'source.txt'): FileChange => ({ path, added: 0, removed: 0, binary: false, countsKnown: false, tooLarge: false, status: 'modified' });
const review = (files = [file()], total = files.length): Review => ({ files, total, coverage: point.coverage, sealed: true, shared: false });
const stage = () => { const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root); return root; };
const open = async (root: HTMLElement) => { root.querySelector<HTMLElement>('.answer-changes > summary')!.click(); await settle(); };

check('answer changes: a sealed point reads only metadata until opened and closing releases all file and patch DOM', async () => {
  const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
  let lists = 0; let patches = 0;
  api.answerChanges = async () => { lists++; return review(); };
  api.answerChangePatch = async () => { patches++; return { lines: ['@@ -1 +1 @@', '-saved before', '+saved after'], total: 3, binary: false, tooLarge: false }; };
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); equal(lists, 1); equal(patches, 0); equal(root.querySelector('.change-file'), null);
    await open(root); equal(lists, 2); equal(patches, 0);
    root.querySelector<HTMLElement>('.change-file summary')!.click(); await settle();
    equal(patches, 1); ok(root.querySelector('pre')?.textContent?.includes('+saved after'));
    equal(root.querySelector('pre')?.getAttribute('aria-hidden'), null);
    equal(root.querySelector('pre')?.getAttribute('tabindex'), '0');
    root.querySelector<HTMLElement>('.answer-changes > summary')!.click(); await settle();
    equal(root.querySelector('.change-file'), null); equal(root.querySelector('pre'), null);
    equal(root.querySelectorAll('button').length, 0, 'this is a review card, not another restore control');
  } finally { dispose(); root.remove(); Object.assign(api, before); }
});

check('answer changes: complete zero-change points stay quiet; partial, expired and failed reads explain themselves', async () => {
  const before = api.answerChanges;
  let value = review([], 0);
  let failed = false;
  api.answerChanges = async () => { if (failed) throw 'error.changes.read'; return value; };
  const root = stage();
  const [current, setCurrent] = createSignal(point);
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', get point() { return current(); } }), root);
  try {
    await settle(); equal(root.querySelector('.answer-changes'), null);
    value = { ...value, coverage: { ...point.coverage, incomplete: true, skipped: 2 } };
    setCurrent({ ...point, id: 'partial' }); await settle(); await open(root);
    ok(root.textContent?.includes(t('rewind.coverage')));
    value = { ...value, coverage: { ...point.coverage, unavailable: true } };
    setCurrent({ ...point, id: 'expired' }); await settle(); await open(root);
    ok(root.textContent?.includes(t('rewind.unavailable')));
    failed = true; setCurrent({ ...point, id: 'failure' }); await settle(); await open(root);
    ok(root.textContent?.includes(t('error.changes.read')));
    failed = false; value = review();
    [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('changes.retry'))!.click(); await settle();
    ok(root.querySelector('.answer-changes'), 'retry reloads the actual saved metadata');
  } finally { dispose(); root.remove(); api.answerChanges = before; }
});

check('answer changes: a copied or expired saved point explains its absence without an endless retry', async () => {
  const before = api.answerChanges;
  let calls = 0;
  api.answerChanges = async () => { calls++; throw 'error.rewind.unavailable'; };
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'branch', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    ok(root.textContent?.includes(t('error.rewind.unavailable')));
    equal(root.querySelectorAll('button').length, 0, 'an absent source point cannot be recovered by retrying the copied conversation');
    equal(calls, 1);
  } finally { dispose(); root.remove(); api.answerChanges = before; }
});

check('answer changes: empty files and properties have an honest description instead of no changes', async () => {
  const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
  const notices = ['answerChanges.properties', 'answerChanges.emptyAdded', 'answerChanges.emptyDeleted'] as const;
  api.answerChanges = async () => review(notices.map((path) => file(path)));
  api.answerChangePatch = async (_id, _message, path) => ({ lines: [], total: 0, binary: false, tooLarge: false, notice: notices.find((key) => key === path) });
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    for (const [index, key] of notices.entries()) {
      root.querySelectorAll<HTMLElement>('.change-file summary')[index]!.click(); await settle();
      ok(root.textContent?.includes(t(key)));
      ok(!root.textContent?.includes(t('changes.empty')));
    }
  } finally { dispose(); root.remove(); Object.assign(api, before); }
});

check('answer changes: file pages replace rows, patch pages follow byte limits and stale replies never replace a newer point', async () => {
  const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
  const paths: number[] = []; const offsets: number[] = [];
  let finish: ((value: Awaited<ReturnType<typeof api.answerChangePatch>>) => void) | undefined;
  api.answerChanges = async (_id, _message, offset = 0) => { paths.push(offset); return review([file(offset ? 'last.txt' : 'first.txt')], 51); };
  api.answerChangePatch = async (_id, _message, path, offset = 0) => {
    offsets.push(offset);
    if (path === 'first.txt') return new Promise((resolve) => { finish = resolve; });
    return { lines: Array.from({ length: Math.min(63, 100 - offset) }, (_, index) => `+saved line ${offset + index}`), total: 100, binary: false, tooLarge: false };
  };
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    root.querySelector<HTMLElement>('.change-file summary')!.click(); await settle();
    root.querySelector<HTMLButtonElement>('[data-answer-files-next]')!.click(); await settle();
    deepEqual(paths, [0, 0, 50]); equal(root.querySelectorAll('.change-file').length, 1);
    root.querySelector<HTMLElement>('.change-file summary')!.click(); await settle();
    finish?.({ lines: ['+stale'], total: 1, binary: false, tooLarge: false }); await settle();
    ok(!root.textContent?.includes('+stale'));
    root.querySelector<HTMLButtonElement>('[data-answer-patch-next]')!.click(); await settle();
    deepEqual(offsets, [0, 0, 63]);
    ok(root.querySelector('pre')?.textContent?.includes('+saved line 99'));
    ok(!root.querySelector('pre')?.textContent?.includes('+saved line 0\n'));
    root.querySelector<HTMLButtonElement>('[data-answer-patch-previous]')!.click(); await settle();
    deepEqual(offsets, [0, 0, 63, 0]);
    equal(root.querySelectorAll('.answer-change-patch .patch-line').length, 63);
  } finally { dispose(); root.remove(); Object.assign(api, before); }
});

check('answer changes: German long paths fit at 720px and reduced motion keeps the saved text accessible', async () => {
  const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
  const language = lang();
  const path = 'customer-communications/international-notifications-and-subscription-renewals-with-long-localized-instructions.txt';
  api.answerChanges = async () => review([file(path)]);
  api.answerChangePatch = async () => ({ lines: ['@@ -1 +1 @@', '-before', '+' + 'synthetic_saved_identifier_'.repeat(100)], total: 3, binary: false, tooLarge: false });
  const root = stage(); root.dataset.motion = 'reduced'; root.style.cssText = 'width:720px';
  for (const [name, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${name}`, value);
  useLanguage('de');
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    const summary = root.querySelector<HTMLElement>('.change-file summary')!; summary.focus();
    equal(document.activeElement, summary);
    summary.click(); await settle();
    equal(root.querySelector('.change-path')?.textContent, path);
    ok(root.scrollWidth <= root.clientWidth + 1, 'the long diff scrolls in its own box');
    equal(root.getAnimations({ subtree: true }).filter((animation) => animation.playState === 'running').length, 0);
    ok(root.querySelector('pre')?.getAttribute('aria-label')?.includes(path));
    equal(root.querySelector('pre')?.getAttribute('role'), 'region', 'the focused saved diff has a named accessible role');
  } finally { dispose(); root.remove(); useLanguage(language); Object.assign(api, before); }
});

check('answer changes: Chat and Projects use the saved prompt point once for the last answer of a shared turn', async () => {
  const before = { answerChanges: api.answerChanges, rewindRedos: api.rewindRedos };
  const requests: string[] = [];
  api.answerChanges = async (_id, message) => { requests.push(message); return review(); };
  api.rewindRedos = async () => [];
  const messages: Message[] = [
    { id: 'earlier-question', role: 'user', text: 'Synthetic first request', at: 0, rewind: { ...point, messageId: 'earlier-question' } },
    { id: 'earlier-answer', role: 'assistant', text: 'Synthetic first answer', at: 1, status: 'done' },
    { id: 'later-question', role: 'user', text: 'Synthetic steered request', at: 2, rewind: { ...point, messageId: 'later-question' } },
    { id: 'later-answer', role: 'assistant', text: 'Synthetic final answer', at: 3, status: 'done' },
  ];
  try {
    for (const area of ['chat', 'project'] as const) {
      const root = stage();
      const dispose = render(() => createComponent(Thread, { area, conversationId: 'conversation', messages, from: 0, options: [], onBranch: () => {} }), root);
      try {
        await settle();
        equal(root.querySelectorAll('.answer-changes').length, 1);
        equal(root.querySelectorAll('article.ai')[0]!.querySelector('.answer-changes'), null);
        ok(root.querySelectorAll('article.ai')[1]!.querySelector('.answer-changes'));
        equal(requests.at(-1), 'later-question', 'the API receives the user prompt, never the assistant id');
        equal(root.querySelector('.answer-changes button'), null, 'Go back remains the existing message-menu action');
      } finally { dispose(); root.remove(); }
    }
  } finally { Object.assign(api, before); }
});

check('answer changes: a sealed prompt event makes its finished answer reviewable without reloading', async () => {
  const before = { answerChanges: api.answerChanges, rewindRedos: api.rewindRedos };
  let reads = 0;
  let sealed = false;
  api.answerChanges = async () => { reads++; return sealed ? review() : { ...review([], 0), sealed: false }; };
  api.rewindRedos = async () => [];
  const [messages, setMessages] = createSignal<Message[]>([
    { id: 'question', role: 'user', text: 'Synthetic request', at: 0, rewind: { ...point, sealed: false } },
    { id: 'answer', role: 'assistant', text: 'Synthetic finished answer', at: 1, status: 'done' },
  ]);
  const root = stage();
  const dispose = render(() => createComponent(Thread, { conversationId: 'conversation', get messages() { return messages(); }, from: 0, options: [], onBranch: () => {} }), root);
  try {
    await settle(); equal(reads, 1); equal(root.querySelector('.answer-changes'), null);
    sealed = true;
    setMessages([{ ...messages()[0]!, rewind: point }, messages()[1]!]); await settle();
    equal(reads, 2); ok(root.querySelector('.answer-changes'), 'the durable seal event is enough to discover the card');
    api.answerChanges = async () => { throw 'error.rewind.unavailable'; };
    setMessages([{ ...messages()[0]!, rewind: { ...point, sealed: false, coverage: { ...point.coverage, unavailable: true } } }, messages()[1]!]); await settle();
    await open(root); ok(root.textContent?.includes(t('error.rewind.unavailable')), 'an unavailable unsealed point explains its missing saved files');
  } finally { dispose(); root.remove(); Object.assign(api, before); }
});

check('answer changes: reopening an older unsealed marker reads durable history and names overlapping work', async () => {
  const before = api.answerChanges;
  api.answerChanges = async () => ({ ...review(), shared: true });
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point: { ...point, sealed: false } }), root);
  try {
    await settle(); ok(root.querySelector('.answer-changes'), 'stored before/after images remain discoverable after restart with an older message marker');
    await open(root);
    ok(root.textContent?.includes(t('answerChanges.shared')), 'overlap is disclosed instead of attributing every change to this answer');
    equal(root.querySelector('summary')?.tagName, 'SUMMARY', 'the disclosure keeps native keyboard activation');
  } finally { dispose(); root.remove(); api.answerChanges = before; }
});

check('answer changes: focused file paging keeps native button focus while loading and after the page arrives', async () => {
  const before = api.answerChanges;
  const offsets: number[] = [];
  const filesAt = (offset: number) => Array.from({ length: Math.min(50, 151 - offset) }, (_, index) => file(`saved-${offset + index}.txt`));
  let finish: ((value: Review) => void) | undefined;
  api.answerChanges = async (_id, _message, offset = 0) => {
    offsets.push(offset);
    if (offset === 50) return new Promise<Review>((resolve) => { finish = resolve; });
    return review(filesAt(offset), 151);
  };
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    const next = root.querySelector<HTMLButtonElement>('[data-answer-files-next]')!;
    next.focus(); equal(document.activeElement, next, 'the native paging button owns focus before activation');
    // Autochecks have no trusted-key bridge. Native click() shares Enter's activation handler;
    // this proves DOM focus across activation, not the browser's trusted Enter default action.
    next.click(); await settle();
    ok(finish, 'the next file page is still pending');
    deepEqual(offsets, [0, 0, 50]);
    equal(root.querySelector('[data-answer-files-next]'), next, 'loading retains the same paging control');
    equal(root.querySelectorAll('.change-file').length, 0, 'loading releases the old bounded page');
    equal(document.activeElement, next, 'loading must not drop keyboard focus to the document');
    next.click(); await settle(); deepEqual(offsets, [0, 0, 50], 'repeated activation during loading does not issue another file read');
    finish?.(review(filesAt(50), 151)); await settle();
    equal(root.querySelectorAll('.change-file').length, 50);
    ok(root.querySelector('.change-path')?.textContent?.includes('saved-50.txt'));
    equal(root.querySelector('[data-answer-files-next]'), next);
    ok(next.getAttribute('aria-disabled') !== 'true', 'the middle page still has a next page');
    equal(document.activeElement, next, 'arrival retains focus without another focus() call');
    next.click(); await settle(); next.click(); await settle();
    equal(root.querySelectorAll('.change-file').length, 1, 'the final bounded file page arrives');
    equal(next.getAttribute('aria-disabled'), 'true');
    equal(document.activeElement, next, 'the final page does not discard the person’s focus');
    const calls = offsets.length; next.click(); await settle();
    equal(offsets.length, calls, 'an unavailable next page cannot issue another read');
    root.querySelector<HTMLElement>('.answer-changes > summary')!.click(); await settle();
    equal(root.querySelector('.change-file'), null);
    equal(root.querySelector('.answer-change-pages'), null, 'closing releases the paging DOM');
  } finally { dispose(); root.remove(); api.answerChanges = before; finish?.(review(filesAt(50), 151)); }
});

check('answer changes: focused patch paging keeps native button focus while loading and after the page arrives', async () => {
  const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
  const offsets: number[] = [];
  type PatchPage = Awaited<ReturnType<typeof api.answerChangePatch>>;
  const patchAt = (offset: number): PatchPage => ({ lines: Array.from({ length: 200 }, (_, index) => `+saved line ${offset + index}`), total: 600, binary: false, tooLarge: false });
  let finish: ((value: PatchPage) => void) | undefined;
  api.answerChanges = async () => review();
  api.answerChangePatch = async (_id, _message, _path, offset = 0) => {
    offsets.push(offset);
    if (offset === 200) return new Promise<PatchPage>((resolve) => { finish = resolve; });
    return patchAt(offset);
  };
  const root = stage();
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
  try {
    await settle(); await open(root);
    root.querySelector<HTMLElement>('.change-file summary')!.click(); await settle();
    const next = root.querySelector<HTMLButtonElement>('[data-answer-patch-next]')!;
    next.focus(); equal(document.activeElement, next, 'the native patch button owns focus before activation');
    // As above, trusted Enter itself needs the screen driver; no focus repair is performed here.
    next.click(); await settle();
    ok(finish, 'the next patch page is still pending');
    deepEqual(offsets, [0, 200]);
    equal(root.querySelector('[data-answer-patch-next]'), next, 'loading retains the same paging control');
    equal(root.querySelector('.answer-change-patch'), null, 'loading unmounts the old patch text');
    equal(document.activeElement, next, 'loading must not drop keyboard focus to the document');
    next.click(); await settle(); deepEqual(offsets, [0, 200], 'repeated activation during loading does not issue another patch read');
    finish?.(patchAt(200)); await settle();
    equal(root.querySelectorAll('.answer-change-patch .patch-line').length, 200);
    ok(root.querySelector('.answer-change-patch')?.textContent?.startsWith('+saved line 200'));
    equal(root.querySelector('[data-answer-patch-next]'), next);
    ok(next.getAttribute('aria-disabled') !== 'true', 'the middle patch page still has a next page');
    equal(document.activeElement, next, 'arrival retains focus without another focus() call');
    next.click(); await settle();
    equal(next.getAttribute('aria-disabled'), 'true');
    equal(document.activeElement, next, 'the final patch page preserves focus');
    const calls = offsets.length; next.click(); await settle();
    equal(offsets.length, calls, 'the exhausted patch cannot issue another read');
    root.querySelector<HTMLElement>('.answer-changes > summary')!.click(); await settle();
    equal(root.querySelector('.change-file'), null);
    equal(root.querySelector('.answer-change-patch'), null, 'closing releases all patch DOM');
  } finally { dispose(); root.remove(); Object.assign(api, before); finish?.(patchAt(200)); }
});

for (const kind of ['files', 'patch'] as const) {
  check(`answer changes: ${kind} page errors allow going back and retain retry focus through failure and success`, async () => {
    const before = { answerChanges: api.answerChanges, answerChangePatch: api.answerChangePatch };
    const reads: number[] = [];
    const filesAt = (offset: number) => Array.from({ length: 50 }, (_, index) => file(`saved-${offset + index}.txt`));
    const firstFiles = review(filesAt(0), 151), secondFiles = review(filesAt(50), 151);
    const patchAt = (offset: number): Awaited<ReturnType<typeof api.answerChangePatch>> => ({ lines: Array.from({ length: 200 }, (_, i) => `+saved line ${offset + i}`), total: 600, binary: false, tooLarge: false });
    let holdBack = false, holdRetry = false;
    let releaseBack: (() => void) | undefined, resolveRetry: (() => void) | undefined, rejectRetry: (() => void) | undefined;
    const read = async <T,>(offset: number, first: T, second: T): Promise<T> => {
      reads.push(offset);
      if (offset === 0) return holdBack ? new Promise<T>((resolve) => { releaseBack = () => { holdBack = false; resolve(first); }; }) : first;
      equal(offset, kind === 'files' ? 50 : 200);
      if (!holdRetry) throw 'error.changes.read';
      return new Promise<T>((resolve, reject) => { resolveRetry = () => resolve(second); rejectRetry = () => reject('error.changes.read'); });
    };
    api.answerChanges = async (_id, _message, offset = 0) => kind === 'files' ? read(offset, firstFiles, secondFiles) : review();
    api.answerChangePatch = async (_id, _message, _path, offset = 0) => read(offset, patchAt(0), patchAt(200));
    const root = stage();
    const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', point }), root);
    const assertFailure = () => {
      ok(root.textContent?.includes(t('error.changes.read')));
      ok(!root.textContent?.includes(t('changes.loading')), 'a settled page error must not announce loading forever');
    };
    try {
      await settle(); await open(root);
      if (kind === 'patch') { root.querySelector<HTMLElement>('.change-file summary')!.click(); await settle(); }
      const next = root.querySelector<HTMLButtonElement>(kind === 'files' ? '[data-answer-files-next]' : '[data-answer-patch-next]')!;
      next.click(); await settle(); assertFailure();
      const previous = next.closest('nav')!.querySelector<HTMLButtonElement>('button')!;
      equal(previous.disabled, false); ok(previous.getAttribute('aria-disabled') !== 'true', 'Previous remains available after a later-page error');
      holdBack = true; previous.focus(); previous.click(); await settle();
      ok(releaseBack, 'Previous reads offset zero despite the failed current page');
      equal(reads.at(-1), 0); equal(document.activeElement, previous, 'Previous retains focus during its read');
      const backReads = reads.length; previous.click(); await settle(); equal(reads.length, backReads, 'Previous also guards its pending read');
      releaseBack?.(); await settle();
      equal(document.activeElement, previous, 'returning to the first page does not repair focus manually');
      equal(root.querySelectorAll(kind === 'files' ? '.change-file' : '.patch-line').length, kind === 'files' ? 50 : 200);
      next.click(); await settle(); assertFailure();
      const retry = [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('changes.retry'))!;
      const returnTarget = retry.closest('details')!.querySelector<HTMLElement>(':scope > summary')!;
      retry.focus(); holdRetry = true;
      const beforeRetry = reads.length; retry.click(); await settle();
      ok(rejectRetry); ok(retry.isConnected, 'retry keeps the same DOM node while pending');
      equal(document.activeElement, retry); retry.click(); retry.click(); await settle();
      equal(reads.length, beforeRetry + 1, 'repeated retry activation starts only one read');
      rejectRetry?.(); await settle(); assertFailure();
      ok(retry.isConnected); equal(document.activeElement, retry, 'another failure retains the focused retry node');
      const beforeSuccess = reads.length; retry.click(); await settle();
      equal(document.activeElement, retry); retry.click(); await settle(); equal(reads.length, beforeSuccess + 1);
      resolveRetry?.(); await settle();
      equal(document.activeElement, returnTarget, 'a successful retry returns focus to its nearest surviving disclosure');
      equal(root.querySelectorAll(kind === 'files' ? '.change-file' : '.patch-line').length, kind === 'files' ? 50 : 200);
      ok(!root.textContent?.includes(t('error.changes.read')));
      root.querySelector<HTMLElement>('.answer-changes > summary')!.click(); await settle();
      equal(root.querySelector('.change-file'), null); equal(root.querySelector('pre'), null, 'closing still releases saved-file and patch DOM');
    } finally { dispose(); root.remove(); Object.assign(api, before); releaseBack?.(); rejectRetry?.(); }
  });
}

check('answer changes: metadata retry preserves focus and guards pending requests, then respects the person’s focus on success', async () => {
  const before = api.answerChanges;
  let reads = 0, pending = false, succeeded = false;
  let finish: (() => void) | undefined, fail: (() => void) | undefined;
  api.answerChanges = async () => {
    reads++;
    if (!pending) { if (succeeded) return review(); throw 'error.changes.read'; }
    return new Promise<Review>((resolve, reject) => {
      finish = () => { succeeded = true; pending = false; resolve(review()); };
      fail = () => { pending = false; reject('error.changes.read'); };
    });
  };
  const root = stage(), elsewhere = document.createElement('input'); document.body.append(elsewhere);
  const [current, setCurrent] = createSignal(point);
  const dispose = render(() => createComponent(AnswerChanges, { conversationId: 'conversation', messageId: 'question', get point() { return current(); } }), root);
  const retryButton = () => [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('changes.retry'))!;
  try {
    await settle(); await open(root);
    const retry = retryButton(), summary = root.querySelector<HTMLElement>('.answer-changes > summary')!;
    retry.focus(); pending = true; const initial = reads; retry.click(); await settle();
    equal(document.activeElement, retry); ok(retry.isConnected); equal(retry.disabled, false);
    retry.click(); retry.click(); await settle(); equal(reads, initial + 1, 'metadata retry has one pending request');
    fail?.(); await settle(); equal(retryButton(), retry); equal(document.activeElement, retry);
    ok(!root.textContent?.includes(t('changes.loading')));
    pending = true; const beforeSuccess = reads; retry.click(); await settle();
    equal(document.activeElement, retry); retry.click(); await settle(); equal(reads, beforeSuccess + 1);
    finish?.(); await settle(); equal(document.activeElement, summary);
    equal(reads, beforeSuccess + 2, 'success makes only the one normal on-open file-list read');
    succeeded = false; setCurrent({ ...point, id: 'another-saved-point' }); await settle(); await open(root);
    const otherRetry = retryButton(); otherRetry.focus(); pending = true; otherRetry.click(); await settle();
    elsewhere.focus(); finish?.(); await settle();
    equal(document.activeElement, elsewhere, 'a late success must not steal focus after the person moved it');
  } finally { dispose(); root.remove(); elsewhere.remove(); api.answerChanges = before; fail?.(); }
});
