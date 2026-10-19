import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Empty } from '../../ui/Empty.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { type Box, placeCard, placeStep, tourFor } from '../../ui/tour.ts';
import { check, equal, ok } from './check.ts';

// Delivery 3a of the calm screens (docs/superpowers/specs/2026-10-16-calm-screens-design.md): one empty state, a tutorial
// card that points at its target at every width, and two what's-new entries that each say only their own news.

const languages = { en, 'pt-BR': ptBR, es, fr, de } as Record<string, Record<string, unknown>>;
const words = (text: unknown) =>
  String(text)
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);

check('homes: the empty state is the peeking mark, a title in the heading size, one line and one button', () => {
  useLanguage('en');
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  let clicks = 0;
  const dispose = render(
    () => createComponent(Empty, { title: t('empty.noAiTitle'), line: t('empty.noAiBody'), button: { label: t('empty.noAiButton'), onClick: () => clicks++, restore: 'connect' } }),
    root,
  );
  try {
    const box = root.querySelector<HTMLElement>('.empty')!;
    ok(box.querySelector('.peek svg'), 'the peeking mark');
    const title = box.querySelector<HTMLElement>('h1')!;
    equal(title.textContent, t('empty.noAiTitle'));
    const size = parseFloat(getComputedStyle(title).fontSize);
    const heading = parseFloat(themes.css.base['fs-h2']);
    const display = parseFloat(themes.css.base['fs-display']);
    ok(size <= heading + 0.5, `the title is the heading size (${size} px, at most ${heading})`);
    ok(size < display / 2, `and not the display size (${size} px against ${display})`);
    equal(box.querySelectorAll('p').length, 1, 'one line');
    const buttons = box.querySelectorAll('button');
    equal(buttons.length, 1, 'one button');
    buttons[0]!.click();
    equal(clicks, 1, 'the button does what it says');
    equal(buttons[0]!.dataset.restore, 'connect', 'and gives focus back to itself after the dialog it opens');
  } finally {
    dispose();
    root.remove();
  }
});

check('homes: an empty state may have no line and no button', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Empty, { title: 'A project' }), root);
  try {
    equal(root.querySelectorAll('p').length, 0);
    equal(root.querySelectorAll('button').length, 0);
  } finally {
    dispose();
    root.remove();
  }
});

check('homes: no empty-state line names a path through Settings, in any language', () => {
  for (const [code, messages] of Object.entries(languages)) {
    const settings = String(messages['settings.open']);
    const section = String(messages['set.subscriptions']);
    for (const key of ['empty.noAiBody', 'empty.openProjectBody', 'empty.projectBody']) {
      const line = String(messages[key]);
      ok(!line.includes('→'), `${code} ${key} has no arrow path`);
      ok(!line.includes(settings) && !line.includes(section), `${code} ${key} does not name Settings or its section`);
      ok(line.length <= 60, `${code} ${key} is one short line (${line.length} characters)`);
    }
  }
});

// The window is 720 px wide or more. The running button sits at the right of the top bar; the greeting sits under it and at
// the narrowest width reaches the same corner, so no side of the button was clear of it.
const card = { w: 340, h: 190 };

check('homes: the third tutorial card sits under the running button, with its pointer on it, at every width from 720 px', () => {
  const follow = tourFor('beginner')[2]!;
  equal(follow.id, 'follow');
  for (const width of [720, 800, 899, 1100, 1440]) {
    const view = { w: width, h: 480 };
    const run: Box = { x: width - 150, y: 8, w: 118, h: 32 };
    // The greeting reaches under the button's corner, where the card would be.
    const greeting: Box = { x: (width - 660) / 2 + 40, y: 90, w: Math.min(560, width * 0.8), h: 150 };
    const plain = placeCard(run, card, view, follow.sides, [greeting]);
    const at = placeStep(follow, run, card, view, [greeting]);
    ok(at.side === 'top', `${width}: the pointer is on the card's top edge (got ${at.side}${plain.side === undefined ? ', it floated before' : ''})`);
    ok(at.y >= run.y + run.h, `${width}: the card starts under the button`);
    ok(at.x >= 12 && at.x + card.w <= width - 12, `${width}: the card is inside the window`);
    const pointer = at.x + (at.pointer ?? 0);
    ok(pointer >= run.x && pointer <= run.x + run.w, `${width}: the pointer lands on the button (${pointer})`);
  }
  // The one-column Chat home at 1100 x 700: the greeting is clear of the room left of the button but not of the room under
  // it. The card still goes under, never beside.
  const run: Box = { x: 930, y: 40, w: 130, h: 44 };
  const greeting: Box = { x: 330, y: 240, w: 600, h: 70 };
  const at = placeStep(follow, run, card, { w: 1100, h: 700 }, [greeting]);
  equal(at.side, 'top', 'with the room beside the button clear, the card still sits under it');
});

check('homes: the experienced running card points the same way, and an unpinned card still floats rather than cover the greeting', () => {
  const running = tourFor('experienced')[1]!;
  const view = { w: 720, h: 480 };
  const run: Box = { x: 570, y: 8, w: 118, h: 32 };
  const greeting: Box = { x: 70, y: 90, w: 560, h: 150 };
  equal(placeStep(running, run, card, view, [greeting]).side, 'top');
  const chat = tourFor('beginner')[0]!;
  const composer: Box = { x: 30, y: 300, w: 660, h: 130 };
  equal(placeStep(chat, composer, card, view, [{ x: 30, y: 100, w: 660, h: 400 }]).side, undefined, 'the composer card keeps the old rule');
  equal(placeStep(running, undefined, card, view, []).side, undefined, 'no target on screen: centred, no pointer');
});

check('homes: the Follow along card says it once and briefly, in every language', () => {
  for (const [code, messages] of Object.entries(languages)) {
    const body = String(messages['tour.follow.body']);
    ok(body.length <= 105, `${code}: ${body.length} characters`);
    const count = (text: string) => body.toLowerCase().split(text).length - 1;
    ok(count('running') <= 1 && count('rodando') <= 1 && count('marcha') <= 1 && count('laufend') <= 1, `${code}: the running list is not named twice`);
  }
});

check('homes: the two helper entries of what is new each say only their own news, in every language', () => {
  for (const [code, messages] of Object.entries(languages)) {
    const helpers = words(messages['notes.22.new']);
    const other = words(messages['notes.19.new']);
    ok(String(messages['notes.22.new']) !== String(messages['notes.19.new']), `${code}: the entries differ`);
    const runs = new Set<string>();
    for (let i = 0; i + 3 <= helpers.length; i++) runs.add(helpers.slice(i, i + 3).join(' '));
    const shared: string[] = [];
    for (let i = 0; i + 3 <= other.length; i++) if (runs.has(other.slice(i, i + 3).join(' '))) shared.push(other.slice(i, i + 3).join(' '));
    equal(shared.length, 0, `${code}: no run of three words is said twice (${shared.join(', ')})`);
  }
  // Only the second is about a project and the other AI; only the first is about a step in the answer.
  ok(!/project/i.test(en['notes.22.new']) && !/other AI/.test(en['notes.22.new']), 'the first is about helpers the AI starts');
  ok(/project/.test(en['notes.19.new']) && /other AI/.test(en['notes.19.new']) && !/step/.test(en['notes.19.new']), 'the second is about asking the other AI');
});
