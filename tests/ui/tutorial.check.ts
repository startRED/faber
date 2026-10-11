import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { placeCard, tourFor, tutorialDue, type TourState } from '../../ui/tour.ts';
import { check, deepEqual, doesNotMatch, equal, ok } from './check.ts';

const due: TourState = { tutorialDone: false, vendors: 1, onNewChat: true, covered: false, endedHere: false };

check('tutorial: a beginner has 3 cards, an experienced person 2, and no answer counts as a beginner', () => {
  deepEqual(tourFor('beginner').map((s) => s.id), ['chat', 'projects', 'follow']);
  deepEqual(tourFor('experienced').map((s) => s.id), ['auto', 'running']);
  equal(tourFor(undefined).length, 3);
  // The experienced cards point at the model picker and the run button; the beginner's at the composer, Projects and the run button.
  deepEqual(tourFor('experienced').map((s) => s.anchor), ['auto', 'run']);
  deepEqual(tourFor('beginner').map((s) => s.anchor), ['composer', 'projects', 'run']);
});

check('tutorial: it starts only when the first run asked for it, at the new conversation, with an AI connected', () => {
  ok(tutorialDue(due));
  equal(tutorialDue({ ...due, tutorialDone: true }), false, 'a finished tutorial never shows again');
  equal(tutorialDue({ ...due, tutorialDone: undefined }), false, 'before the first run there is none');
  equal(tutorialDue({ ...due, vendors: 0 }), false, 'its first card teaches the composer, which cannot send with no AI');
  equal(tutorialDue({ ...due, onNewChat: false }), false);
  equal(tutorialDue({ ...due, covered: true }), false, 'a first-run step, the setup cards or Settings is open');
  equal(tutorialDue({ ...due, endedHere: true }), false, 'it ended this session, even if the save of tutorialDone failed');
  // Connecting an account later, from the no-AI state, is what makes it due.
  ok(tutorialDue({ ...due, vendors: 0 }) === false && tutorialDue({ ...due, vendors: 1 }));
});

const view = { w: 1280, h: 800 };
const card = { w: 320, h: 180 };

check('tutorial: a card with no anchor is centred and points at nothing', () => {
  const at = placeCard(undefined, card, view, ['bottom']);
  deepEqual(at, { x: 480, y: 310 });
});

check('tutorial: a card sits on the first side that has room, with its pointer on the edge that faces the anchor', () => {
  const run = { x: 1130, y: 10, w: 140, h: 32 };
  const below = placeCard(run, card, view, ['bottom', 'left']);
  equal(below.side, 'top');
  equal(below.y, 10 + 32 + 14);
  // The card is pushed in from the right edge, and the pointer still lands on the middle of the button.
  equal(below.x, 1280 - 320 - 12);
  equal(below.x + (below.pointer ?? 0), 1200);
  const composer = { x: 300, y: 600, w: 640, h: 90 };
  const above = placeCard(composer, card, view, ['bottom', 'top']);
  equal(above.side, 'bottom', 'no room below it, so the card goes above');
  equal(above.y, 600 - 14 - 180);
  const projects = { x: 12, y: 130, w: 224, h: 34 };
  const beside = placeCard(projects, card, view, ['right', 'bottom']);
  equal(beside.side, 'left');
  equal(beside.x, 12 + 224 + 14);
  equal(beside.y + (beside.pointer ?? 0), 147);
});

check('tutorial: when no side has room the card is centred and points at nothing', () => {
  const tiny = { w: 340, h: 200 };
  const anchor = { x: 100, y: 80, w: 140, h: 40 };
  const at = placeCard(anchor, card, tiny, ['bottom', 'top', 'right']);
  equal(at.side, undefined);
  equal(at.pointer, undefined);
  deepEqual([at.x, at.y], [10, 10]);
});

check('tutorial: a card never sits on the main heading, and slides along the anchor to clear it', () => {
  const heading = { x: 324, y: 215, w: 600, h: 100 };
  const composer = { x: 324, y: 380, w: 640, h: 130 };
  // Above the composer would cover the heading, so the card goes below it.
  const below = placeCard(composer, card, view, ['top', 'bottom'], [heading]);
  equal(below.side, 'top');
  equal(below.y, 380 + 130 + 14);
  // Beside the Projects button the centred spot touches the heading, so the card slides up above it.
  const projects = { x: 12, y: 130, w: 224, h: 34 };
  const side = placeCard(projects, { w: 340, h: 180 }, view, ['right', 'bottom'], [heading]);
  equal(side.side, 'left');
  equal(side.y + 180 <= 215 - 14, true, 'the card ends above the heading');
  equal(side.y + (side.pointer ?? 0), 147, 'the pointer still lands on the middle of the button');
  // With no clear side the card is centred, as before.
  deepEqual(placeCard(composer, card, view, ['top'], [heading]), { x: 480, y: 310 });
});

// A card whose anchor is missing is centred, so its words must make sense without pointing at anything.
check('tutorial: no card says "here", in any language', () => {
  const here: Record<string, RegExp> = { en: /\bhere\b/i, 'pt-BR': /\baqui\b/i, es: /\baqu[ií]\b/i, fr: /\bici\b/i, de: /\bhier\b/i };
  const languages = { en, 'pt-BR': ptBR, es, fr, de } as Record<string, Record<string, unknown>>;
  for (const [code, messages] of Object.entries(languages)) {
    for (const id of ['chat', 'projects', 'follow', 'auto', 'running']) {
      for (const part of ['title', 'body']) {
        const text = messages[`tour.${id}.${part}`];
        equal(typeof text, 'string', `${code} tour.${id}.${part}`);
        doesNotMatch(text as string, here[code]!, `${code} tour.${id}.${part}`);
      }
    }
  }
});
