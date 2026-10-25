import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import type { Message } from '../../ui/api.ts';
import { quoted, ToEnd } from '../../ui/Floating.tsx';
import { LANGUAGES, useLanguage, t } from '../../ui/i18n.ts';
import { spoken, voiceFor } from '../../ui/speech.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

// The pictures of a conversation, the full-screen viewer, the floating buttons, the code blocks and reading aloud (the owner,
// 2026-10-17): the small things around a message that a person meets by pointing.

const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';
const GIF = 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
const settle = (ms = 40) => new Promise((resolve) => setTimeout(resolve, ms));
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

function surface(width = 760, motion = 'none') {
  const root = document.createElement('div');
  root.style.cssText = `position:fixed;left:0;top:0;width:${width}px`;
  root.dataset.motion = motion;
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  return root;
}

function thread(messages: Message[], width = 760, motion = 'none') {
  const root = surface(width, motion);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'viewer', options: [], onBranch: () => {} }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

// Closes a viewer a failed check left open, the way a person would, so the next check can open one.
const leaveViewer = () => document.querySelector('.viewer')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));

const mine = (extra: Partial<Message>): Message => ({ id: 'u', role: 'user', text: 'Look at these', at: 1_760_000_000_000, ...extra });
const reply = (extra: Partial<Message> = {}): Message => ({ id: 'a', role: 'assistant', text: 'Done.', at: 1_760_000_010_000, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', ...extra });

check('thumbs: expanding many attachments finishes within 300ms and respects reduced motion', async () => {
  const images = Array.from({ length: 30 }, () => ({ mediaType: 'image/png', base64: PNG }));
  for (const motion of ['full', 'reduced', 'none']) {
    const view = thread([mine({ images })], 760, motion);
    try {
      view.root.querySelector<HTMLButtonElement>('.thumb-more')!.click();
      const slots = [...view.root.querySelectorAll<HTMLElement>('.thumb-slot.late')];
      equal(slots.length, 26);
      for (const slot of slots) {
        const style = getComputedStyle(slot);
        if (motion !== 'full') {
          equal(style.animationName, 'none', `${motion} has no attachment animation`);
        } else {
          const total = (parseFloat(style.animationDuration) + parseFloat(style.animationDelay)) * 1000;
          ok(total > 0 && total <= 300, `an attachment takes ${total}ms including its delay`);
        }
      }
    } finally { view.close(); }
  }
});

check('thumbs: frequent message actions only fade within 150ms', () => {
  const view = thread([mine({}), reply()], 760, 'full');
  try {
    for (const el of view.root.querySelectorAll<HTMLElement>('.bar, .ai .actions > :not(.foot)')) {
      const style = getComputedStyle(el);
      equal(style.transform, 'none', 'message actions never travel on hover or focus');
      equal(style.transitionProperty, 'opacity');
      ok(parseFloat(style.transitionDuration) <= 0.15, 'frequent action feedback is at most 150ms');
    }
    const sheets = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]);
    const shows = sheets.filter((r): r is CSSStyleRule => r instanceof CSSStyleRule && r.selectorText.includes('.bar') && r.style.opacity === '1');
    const hover = shows.find((r) => r.selectorText.includes(':hover') && r.selectorText.includes(':focus-within'))!;
    ok(hover, 'pointer and keyboard keep the same feedback');
    const probe = document.createElement('div');
    view.root.append(probe);
    probe.style.cssText = hover.style.cssText;
    ok(parseFloat(getComputedStyle(probe).transitionDuration) <= 0.15, 'showing the actions also stays within 150ms');
  } finally { view.close(); }
});

check('thumbs: seven pictures and two files are small rounded thumbnails above the text, with +N, not stacked full-width in the bubble', async () => {
  const images = Array.from({ length: 7 }, () => ({ mediaType: 'image/png', base64: PNG }));
  const view = thread([mine({ images, attachments: ['report.pdf', 'data.csv'] })]);
  try {
    const bubble = view.root.querySelector<HTMLElement>('.you')!;
    equal(bubble.textContent, 'Look at these');
    equal(bubble.querySelector('img'), null, 'no picture sits inside the bubble');
    const strip = view.root.querySelector<HTMLElement>('.thumbs')!;
    ok(strip.compareDocumentPosition(bubble) & Node.DOCUMENT_POSITION_FOLLOWING, 'the thumbnails come before the text');
    equal(strip.querySelectorAll('.thumb img').length, 4, 'four pictures and the +N tile');
    equal(strip.querySelector('.thumb-more')!.textContent, '+5');
    for (const img of strip.querySelectorAll<HTMLElement>('.thumb img')) {
      const box = img.closest('.thumb')!.getBoundingClientRect();
      ok(box.width >= 56 && box.width <= 96 && box.height <= 96, `a thumbnail is ${box.width}x${box.height}`);
    }
    ok(strip.getBoundingClientRect().height <= 96 * 2 + 8, 'a row or two, not a column of pictures');
    strip.querySelector<HTMLButtonElement>('.thumb-more')!.click();
    await settle();
    equal(strip.querySelectorAll('.thumb').length, 9, 'the rest open when the tile is pressed');
    equal(strip.querySelector('.thumb-more'), null);
    const cards = [...strip.querySelectorAll('.thumb.file')];
    equal(cards.map((c) => c.querySelector('small')!.textContent).join(), 'PDF,CSV', 'a file card says what kind of file it is');
    equal(cards[0]!.querySelector('b')!.textContent, 'report.pdf');
  } finally {
    view.close();
  }
});

check('thumbs: the bar under a message shows only for hover or the keyboard (always on a touch screen), and has copy, edit and the time sent', () => {
  const view = thread([mine({}), reply({ took: 12_000 })]);
  try {
    const bar = view.root.querySelector<HTMLElement>('.mine-bar')!;
    equal(getComputedStyle(bar).opacity, '0', 'hidden until the message is pointed at');
    equal(bar.querySelectorAll('button').length, 2, 'copy and edit');
    ok(bar.querySelector('time')!.getAttribute('datetime') === new Date(1_760_000_000_000).toISOString(), 'the time it was sent');
    ok(bar.querySelector<HTMLElement>('.when')!.title.length > 20, 'the whole date is its tooltip');
    // The reply's state line stays in view; its buttons, time and cost are what wait for the pointer.
    const answerBar = view.root.querySelector<HTMLElement>('.ai .actions')!;
    const copy = answerBar.querySelector<HTMLElement>('.iconbtn')!;
    equal(getComputedStyle(copy).opacity, '0', 'not even the last answer shows its actions until pointed at');
    equal(getComputedStyle(copy).visibility, 'visible', 'it is hidden by opacity, so the keyboard still reaches it');
    equal(answerBar.querySelector('.took')!.textContent!.trim().replace(/^·\s*/, ''), '12 sec', 'and how long the answer took');
    // A hidden test window never has the focus or a pointer, so the rules that show the bar are read instead.
    const sheets = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]);
    const shows = sheets.filter((r): r is CSSStyleRule => r instanceof CSSStyleRule && r.selectorText.includes('.bar') && r.style.opacity === '1');
    ok(shows.some((r) => r.selectorText.includes(':hover') && r.selectorText.includes(':focus-within')), 'the pointer and the keyboard show it');
    const touch = sheets.find((r): r is CSSMediaRule => r instanceof CSSMediaRule && r.conditionText.replace(/\s/g, '') === '(hover:none)');
    ok(touch && [...touch.cssRules].some((r) => (r as CSSStyleRule).selectorText.includes('.bar') && (r as CSSStyleRule).style.opacity === '1'), 'a screen with no hover always shows it');
  } finally {
    view.close();
  }
});

check('thumbs: an answer from before durations were kept shows its time and no length', () => {
  const view = thread([mine({}), reply()]);
  try {
    ok(view.root.querySelector('.ai .when time'));
    equal(view.root.querySelector('.ai .took'), null);
  } finally {
    view.close();
  }
});

check('viewer: a thumbnail opens the picture full screen; the arrows move between the conversation\'s pictures; Esc and a click outside close it', async () => {
  const images = [PNG, GIF, PNG].map((base64, i) => ({ mediaType: i === 1 ? 'image/gif' : 'image/png', base64 }));
  const view = thread([mine({ images })]);
  try {
    const open = view.root.querySelector<HTMLButtonElement>('.thumb-open')!;
    equal(document.querySelector('.viewer'), null, 'the viewer has no DOM until it opens');
    open.focus();
    open.click();
    const viewer = document.querySelector<HTMLElement>('.viewer')!;
    ok(viewer, 'it opens');
    equal(viewer.getAttribute('role'), 'dialog');
    equal(document.activeElement, viewer, 'focus moves into it');
    const src = () => viewer.querySelector<HTMLImageElement>('.viewer-img')!.src;
    ok(src().startsWith('data:image/png'));
    equal(viewer.querySelector('.viewer-count')!.textContent, t('viewer.count', { n: 1, total: 3 }));
    equal(viewer.querySelector('.viewer-btn.prev'), null, 'nothing before the first');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
    ok(src().startsWith('data:image/gif'), 'the right arrow shows the next');
    equal(viewer.querySelector('.viewer-count')!.textContent, t('viewer.count', { n: 2, total: 3 }));
    const next = viewer.querySelector<HTMLButtonElement>('.viewer-btn.next')!;
    next.focus();
    next.click();
    equal(viewer.querySelector('.viewer-btn.next'), null, 'nothing after the last');
    ok(viewer.contains(document.activeElement), 'and the focus stays in the viewer when the button that had it goes');
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }));
    ok(src().startsWith('data:image/gif'));
    // Tab never leaves the dialog: past the last stop it goes back to the first.
    const stops = [...viewer.querySelectorAll<HTMLButtonElement>('button')];
    stops.at(-1)!.focus();
    ok(document.activeElement === stops.at(-1), `the last stop takes the focus: ${(document.activeElement as HTMLElement | null)?.className}; inert ${viewer.inert}; ${stops.map((s) => s.className).join('|')}`);
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    viewer.dispatchEvent(tab);
    ok(tab.defaultPrevented && document.activeElement === stops[0], `focus is kept inside: prevented ${tab.defaultPrevented}, focus on ${(document.activeElement as HTMLElement | null)?.className}, ${stops.length} stops`);
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await settle();
    equal(document.querySelector('.viewer'), null, 'Esc closes it and takes its DOM away');
    equal(document.activeElement, open, 'focus goes back to the thumbnail');
    open.click();
    document.querySelector<HTMLElement>('.viewer-stage')!.click();
    await settle();
    equal(document.querySelector('.viewer'), null, 'a click outside the picture closes it');
  } finally {
    leaveViewer();
    view.close();
  }
});

check('viewer: with motion on it grows from the thumbnail and leaves by fading, then its DOM goes', async () => {
  if (reduced) return;
  const view = thread([mine({ images: [{ mediaType: 'image/png', base64: PNG }] })], 760, 'full');
  try {
    view.root.querySelector<HTMLButtonElement>('.thumb-open')!.click();
    const viewer = document.querySelector<HTMLElement>('.viewer')!;
    await settle(80);
    viewer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    ok(document.querySelector('.viewer.leaving'), 'it leaves with a short exit, not at once');
    await settle(700);
    equal(document.querySelector('.viewer'), null);
  } finally {
    leaveViewer();
    view.close();
  }
});

check('viewer: a picture inside an answer opens the same viewer', () => {
  const view = thread([reply({ text: `Here it is: ![chart](data:image/png;base64,${PNG})` })]);
  try {
    const picture = view.root.querySelector<HTMLImageElement>('.md img')!;
    picture.click();
    ok(document.querySelector('.viewer'), 'it opens');
    leaveViewer();
    equal(picture.getAttribute('tabindex'), '0', 'the keyboard can reach the picture');
    picture.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    ok(document.querySelector('.viewer'), 'and Enter opens it');
    const box = picture.getBoundingClientRect();
    ok(box.width <= 280 && box.height <= 200, 'and the picture in the answer is a thumbnail');
  } finally {
    leaveViewer();
    view.close();
  }
});

check('floating: "go to the end" shows when the thread is scrolled up, scrolls down and goes away', async () => {
  const root = surface(600);
  root.innerHTML = '<div class="work" style="height:300px"><div class="thread"><div class="col" style="height:1200px"></div></div></div>';
  const work = root.querySelector<HTMLElement>('.work')!;
  const el = root.querySelector<HTMLElement>('.thread')!;
  const dispose = render(() => createComponent(ToEnd, { thread: () => el }), work);
  try {
    await settle();
    const button = () => work.querySelector<HTMLButtonElement>('.toend');
    ok(button(), 'scrolled up, it shows');
    equal(button()!.getAttribute('aria-label'), t('thread.toEndLabel'));
    const at = button()!.getBoundingClientRect();
    const box = work.getBoundingClientRect();
    ok(Math.abs(at.left + at.width / 2 - (box.left + box.width / 2)) < 4, 'centered over the thread');
    ok(at.bottom <= box.bottom && at.bottom > box.bottom - 80, 'at its foot');
    button()!.click();
    // A hidden test window draws no frames, so the scroll events the page would send are sent here.
    el.dispatchEvent(new Event('scroll'));
    ok(el.scrollHeight - el.scrollTop - el.clientHeight < 4, 'it scrolls to the end');
    equal(button(), null, 'and goes away');
    el.scrollTop = 0;
    el.dispatchEvent(new Event('scroll'));
    await settle();
    ok(button(), 'back when scrolled up again');
  } finally {
    dispose();
    root.remove();
  }
});

check('code: a block has its language and a button that copies that block only', async () => {
  const original = navigator.clipboard.writeText;
  const copied: string[] = [];
  navigator.clipboard.writeText = async (value) => { copied.push(value); };
  const view = thread([reply({ text: 'Run:\n\n```ts\nconst a = 1;\n```\n\nThen:\n\n```\nplain\n```\n' })]);
  try {
    const blocks = [...view.root.querySelectorAll<HTMLElement>('.codeblock')];
    equal(blocks.length, 2);
    equal(blocks[0]!.querySelector('.cb-lang')!.textContent, 'ts', 'the language it was written in');
    equal(blocks[1]!.querySelector('.cb-lang')!.textContent, '', 'a block with no language names none');
    const second = blocks[1]!.querySelector<HTMLButtonElement>('.cb-copy')!;
    equal(second.getAttribute('aria-label'), t('code.copy'));
    second.click();
    await settle();
    equal(copied.join('|'), 'plain\n', 'only that block');
    ok(second.querySelector('.swap.done'), 'the icon turns into a check');
    blocks[0]!.querySelector<HTMLButtonElement>('.cb-copy')!.click();
    await settle();
    equal(copied.at(-1), 'const a = 1;\n');
    equal(view.root.querySelectorAll('.ai > .md pre').length, 2, 'the blocks are still drawn as code');
  } finally {
    navigator.clipboard.writeText = original;
    view.close();
  }
});

check('quote: selecting words in an answer offers "Ask about this", which quotes them into the message box and focuses it', async () => {
  const box = document.createElement('div');
  box.className = 'composer';
  box.innerHTML = '<textarea></textarea>';
  document.body.append(box);
  const field = box.querySelector('textarea')!;
  const view = thread([reply({ text: 'Alpha beta gamma.' })]);
  const select = (node: Node, from: number, to: number) => {
    const range = document.createRange();
    range.setStart(node, from);
    range.setEnd(node, to);
    getSelection()!.removeAllRanges();
    getSelection()!.addRange(range);
    document.dispatchEvent(new Event('pointerup'));
  };
  try {
    view.root.style.top = '200px';
    const word = view.root.querySelector('.ai .md p')!.firstChild!;
    select(word, 6, 10);
    await settle();
    const quote = document.querySelector<HTMLButtonElement>('.quote')!;
    equal(quote.textContent, t('quote.ask'));
    const words = getSelection()!.getRangeAt(0).getBoundingClientRect();
    ok(quote.getBoundingClientRect().bottom <= words.top + 1, 'it sits over the selection');
    // Words at the top edge of the window have no room above, so the button goes under them.
    view.root.style.top = '0px';
    select(word, 6, 10);
    await settle();
    const under = getSelection()!.getRangeAt(0).getBoundingClientRect();
    ok(document.querySelector('.quote')!.getBoundingClientRect().top >= under.bottom - 1, 'and under them when the top edge is too near');
    const again = document.querySelector<HTMLButtonElement>('.quote')!;
    const down = new Event('pointerdown', { bubbles: true, cancelable: true });
    again.dispatchEvent(down);
    ok(down.defaultPrevented, 'pressing it keeps the selection');
    field.value = 'Why';
    again.click();
    equal(field.value, 'Why\n\n> beta\n\n', 'the words are a quote under what was written');
    equal(document.activeElement, field, 'and the box is focused');
    await settle(400);
    equal(document.querySelector('.quote'), null);
    // Words outside any answer are not offered.
    const outside = document.createElement('p');
    outside.textContent = 'Not an answer';
    view.root.append(outside);
    select(outside.firstChild!, 0, 3);
    await settle();
    equal(document.querySelector('.quote'), null, 'only an answer can be quoted');
  } finally {
    getSelection()!.removeAllRanges();
    box.remove();
    view.close();
  }
});

check('quote: every line of the selection gets its own mark', () => {
  equal(quoted('one\n\ntwo'), '> one\n>\n> two\n\n');
  equal(quoted('  one line  '), '> one line\n\n');
});

check('speech: an answer is read aloud in the screen language without its code, and the same press stops it', async () => {
  const synth = { spoken: [] as SpeechSynthesisUtterance[], cancelled: 0, getVoices: () => [] as SpeechSynthesisVoice[], speak(u: SpeechSynthesisUtterance) { this.spoken.push(u); }, cancel() { this.cancelled++; } };
  const real = Object.getOwnPropertyDescriptor(window, 'speechSynthesis');
  Object.defineProperty(window, 'speechSynthesis', { value: synth, configurable: true });
  const view = thread([mine({}), reply({ text: 'Hello there. General Kenobi.\n\n```js\nconst secret = 1;\n```\n\nBye now.' })]);
  try {
    const button = () => view.root.querySelector<HTMLButtonElement>('.ai .actions button[aria-label="' + t('answer.listen') + '"], .ai .actions button[aria-label="' + t('answer.listenStop') + '"]')!;
    button().click();
    const said = synth.spoken.map((u) => u.text).join(' ');
    ok(said.includes('Hello there.') && said.includes('Bye now.'), said);
    ok(!said.includes('secret'), 'code is not worth hearing');
    ok(synth.spoken.every((u) => u.lang === 'en'), 'in the language of the screen');
    equal(button().getAttribute('aria-pressed'), 'true');
    equal(button().getAttribute('aria-label'), t('answer.listenStop'));
    const before = synth.cancelled;
    button().click();
    ok(synth.cancelled > before, 'the same press stops it');
    equal(button().getAttribute('aria-pressed'), 'false');
    button().click();
    const again = synth.cancelled;
    view.close();
    ok(synth.cancelled > again, 'and it stops when the message goes away');
  } finally {
    if (real) Object.defineProperty(window, 'speechSynthesis', real);
    else delete (window as unknown as Record<string, unknown>).speechSynthesis;
    view.root.remove();
  }
});

check('speech: the voice matches the screen language, an exact one before a near one, a local one before one that needs the network', () => {
  const voice = (lang: string, localService: boolean) => ({ lang, localService, name: lang }) as SpeechSynthesisVoice;
  const all = [voice('en-US', true), voice('pt_PT', true), voice('pt-BR', false), voice('pt-BR', true)];
  const chosen = voiceFor('pt-BR', all)!;
  ok(chosen.lang === 'pt-BR' && chosen.localService, 'pt-BR on this PC');
  equal(voiceFor('pt-BR', all.slice(0, 3))!.lang, 'pt-BR', 'an exact voice that needs the network beats a near one on the PC');
  equal(voiceFor('pt-BR', all.slice(0, 2))!.lang, 'pt_PT', 'a near voice when there is no exact one');
  equal(voiceFor('de', all), undefined, 'none when the language has no voice');
  equal(spoken('One. Two.\n\n```\nskip\n```\n\n- three').join('|'), 'One. Two.|three');
  useLanguage('en');
});

check('reading: the thread column reads at about 760 px and the answers keep room between lines, also in a 720 px window', () => {
  for (const [width, least, most] of [[1200, 760, 760], [720, 640, 700]] as const) {
    const root = surface(width);
    root.innerHTML = '<div class="work" style="height:600px"><div class="thread"><div class="col"></div></div></div>';
    const dispose = render(() => createComponent(Thread, { messages: [mine({}), reply({ text: 'A paragraph. '.repeat(40) })], from: 0, conversationId: 'wide', options: [], onBranch: () => {} }), root.querySelector('.col')!);
    try {
      const col = root.querySelector<HTMLElement>('.col')!.getBoundingClientRect().width;
      ok(col >= least && col <= most, `${width}px window: the column is ${col}px`);
      const answer = root.querySelector<HTMLElement>('.ai')!;
      const style = getComputedStyle(answer);
      ok(parseFloat(style.lineHeight) / parseFloat(style.fontSize) >= 1.6, `line height ${style.lineHeight} on ${style.fontSize}`);
      const bubble = root.querySelector<HTMLElement>('.you')!.getBoundingClientRect().width;
      ok(bubble <= 620, `the bubble is ${bubble}px`);
    } finally {
      dispose();
      root.remove();
    }
  }
});

check('reading: the bar under an answer stays on one line in every language, at a 720 px window and in a narrow column', () => {
  const nudge = { reason: 'nudge.failedCheck', vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' } as const;
  try {
    for (const { code } of LANGUAGES) {
      useLanguage(code);
      for (const width of [640, 420]) {
        const view = thread([mine({}), reply({ costUsd: 0.0042, took: 3_725_000, nudge })], width);
        try {
          const bar = view.root.querySelector<HTMLElement>('.ai .actions')!;
          const middles = [...bar.children].map((el) => Math.round(el.getBoundingClientRect().top + el.getBoundingClientRect().height / 2));
          ok(Math.max(...middles) - Math.min(...middles) <= 3, `${code} ${width}px: nothing wraps: ${middles}`);
          ok(bar.scrollWidth <= bar.clientWidth + 1, `${code} ${width}px: nothing spills out of the bar`);
        } finally {
          view.close();
        }
      }
    }
  } finally {
    useLanguage('en');
  }
});
