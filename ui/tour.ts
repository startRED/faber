import type { Experience } from './api.ts';

// The tutorial's logic, apart from the screen: which cards a person gets, when the tutorial is due, and where a card goes.

/** The card's words are the keys `tour.<id>.title` and `tour.<id>.body`. */
export type TourId = 'chat' | 'projects' | 'follow' | 'auto' | 'running';
export type Side = 'top' | 'bottom' | 'left' | 'right';

export interface TourStep {
  id: TourId;
  /** The value of the `data-tour` attribute on the element the card points at. */
  anchor: string;
  /** Where the card likes to sit, most wanted first. */
  sides: Side[];
  /** The target is in a corner of the window and is the whole point of the card (the running button): when no side clears
   * the main heading the card still sits beside its target, over the heading, instead of floating in the middle. */
  pin?: boolean;
}

const CHAT: TourStep = { id: 'chat', anchor: 'composer', sides: ['bottom', 'top'] };
const PROJECTS: TourStep = { id: 'projects', anchor: 'projects', sides: ['right', 'bottom'] };
// The running button's cards sit only under it (R170): a card beside it would leave the corner it explains.
const FOLLOW: TourStep = { id: 'follow', anchor: 'run', sides: ['bottom'], pin: true };
const AUTO: TourStep = { id: 'auto', anchor: 'auto', sides: ['bottom', 'left', 'top'] };
const RUNNING: TourStep = { id: 'running', anchor: 'run', sides: ['bottom'], pin: true };

/** A beginner gets three cards. An experienced person gets two, for what is new to them: Auto and the running list. No
 * answer to the first-run question counts as a beginner. */
export function tourFor(experience: Experience | undefined): TourStep[] {
  return experience === 'experienced' ? [AUTO, RUNNING] : [CHAT, PROJECTS, FOLLOW];
}

export interface TourState {
  tutorialDone?: boolean;
  /** How many AIs are connected. */
  vendors: number;
  /** The new conversation (s1) is on screen. */
  onNewChat: boolean;
  /** A first-run step, the setup cards or Settings is open. */
  covered: boolean;
  /** The tutorial already ended in this session, whether or not the save of `tutorialDone` worked. */
  endedHere: boolean;
}

/** The tutorial starts when the first run asked for it (`tutorialDone` is false), at the new conversation, with an AI
 * connected, because its first card teaches the composer and that cannot send without one. */
export function tutorialDue(s: TourState): boolean {
  return s.tutorialDone === false && !s.endedHere && !s.covered && s.vendors > 0 && s.onNewChat;
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Size {
  w: number;
  h: number;
}
export interface Placement {
  x: number;
  y: number;
  /** The edge of the card that has the pointer. None when the card is centred. */
  side?: Side;
  /** Along that edge, from the card's corner, to the middle of the element pointed at. */
  pointer?: number;
}

const clamp = (n: number, low: number, high: number) => Math.min(Math.max(n, low), Math.max(low, high));
const meets = (a: Box, b: Box) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** Where a card of `card` size goes in a window of `view` size. With no anchor (it is missing or hidden), or no side with
 * room for it, the card is centred and points at nothing. A card points with the edge that faces the anchor. A card never
 * sits on an `avoid` box (the main heading): a side card slides along the anchor's edge to clear it, and a side that cannot
 * clear it is skipped. */
export function placeCard(
  anchor: Box | undefined,
  card: Size,
  view: Size,
  sides: readonly Side[],
  avoid: readonly Box[] = [],
  gap = 14,
  margin = 12,
): Placement {
  const centred: Placement = { x: Math.round((view.w - card.w) / 2), y: Math.round((view.h - card.h) / 2) };
  if (!anchor) return centred;
  const middle = { x: anchor.x + anchor.w / 2, y: anchor.y + anchor.h / 2 };
  const room = { x: [margin, view.w - card.w - margin], y: [margin, view.h - card.h - margin] } as const;
  const clear = (x: number, y: number) => !avoid.some((a) => meets({ x, y, ...card }, a));
  for (const side of sides) {
    // `side` is the side of the anchor where the card sits; the pointer is on the opposite edge of the card.
    if (side === 'bottom' || side === 'top') {
      const y = side === 'bottom' ? anchor.y + anchor.h + gap : anchor.y - gap - card.h;
      if (y < margin || y > room.y[1]) continue;
      const x = clamp(middle.x - card.w / 2, room.x[0], room.x[1]);
      if (!clear(x, y)) continue;
      return { x: Math.round(x), y: Math.round(y), side: side === 'bottom' ? 'top' : 'bottom', pointer: Math.round(clamp(middle.x - x, 20, card.w - 20)) };
    }
    const x = side === 'right' ? anchor.x + anchor.w + gap : anchor.x - gap - card.w;
    if (x < margin || x > room.x[1]) continue;
    // Centred on the anchor, else just above or just below the boxes to avoid, while the pointer still reaches the anchor.
    const wanted = clamp(middle.y - card.h / 2, room.y[0], room.y[1]);
    const shifts = avoid.flatMap((a) => [a.y - gap - card.h, a.y + a.h + gap]);
    const y = [wanted, ...shifts].find((c) => c >= room.y[0] && c <= room.y[1] && clear(x, c) && middle.y - c >= 20 && middle.y - c <= card.h - 20);
    if (y === undefined) continue;
    return { x: Math.round(x), y: Math.round(y), side: side === 'right' ? 'left' : 'right', pointer: Math.round(clamp(middle.y - y, 20, card.h - 20)) };
  }
  return centred;
}

/** Where the card of `step` goes: as `placeCard` says, and a pinned card that found no clear side sits under its target all the
 * same, over the heading, so it keeps its pointer at every width. */
export function placeStep(step: TourStep, anchor: Box | undefined, card: Size, view: Size, headings: readonly Box[]): Placement {
  const at = placeCard(anchor, card, view, step.sides, headings);
  return step.pin && anchor && at.side === undefined ? placeCard(anchor, card, view, step.sides) : at;
}
