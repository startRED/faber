import { fold } from './palette.ts';

// Ctrl+F: finding a word in the open conversation. The matches are ranges over the text that is already on screen and the
// browser paints them (CSS Custom Highlight API), so no message is ever rewritten or wrapped.

/** The most matches kept; more than this is a letter or two typed, not a word looked for. */
export const LIMIT = 1000;

// Only what a person reads in the thread: their messages and the answers. A button's label or a hidden line is not a match.
const READ = '.md, .you, .queue-text';
const SKIP = 'button, textarea, input, select, svg, .sr, .sr-only';

/** Every place `query` appears in the readable text under `root`, in reading order, ignoring case and accents.
 * ponytail: a match is inside one text node, so a phrase split by bold or a link is not found; join the nodes of a block if that matters. */
export function rangesIn(root: Element, query: string): { ranges: Range[]; capped: boolean } {
  const ranges: Range[] = [];
  const typed = fold(query);
  if (!typed.trim()) return { ranges, capped: false };
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const folded = fold(node.nodeValue ?? '');
    let at = folded.indexOf(typed);
    if (at < 0) continue;
    const parent = node.parentElement;
    if (!parent?.closest(READ) || parent.closest(SKIP) || !parent.checkVisibility()) continue;
    for (; at >= 0; at = folded.indexOf(typed, at + typed.length)) {
      if (ranges.length >= LIMIT) return { ranges, capped: true };
      const range = new Range();
      range.setStart(node, at);
      range.setEnd(node, at + typed.length);
      ranges.push(range);
    }
  }
  return { ranges, capped: false };
}

/** Paints the matches, and the one in view stronger. A webview without the highlight API shows no paint; the count and the
 * jump still work. */
export function paint(ranges: Range[], now?: Range): void {
  if (!('highlights' in CSS)) return;
  if (ranges.length) CSS.highlights.set('find', new Highlight(...ranges));
  else CSS.highlights.delete('find');
  if (now) {
    const strong = new Highlight(now);
    strong.priority = 1;
    CSS.highlights.set('find-now', strong);
  } else CSS.highlights.delete('find-now');
}

export function unpaint(): void {
  if (!('highlights' in CSS)) return;
  CSS.highlights.delete('find');
  CSS.highlights.delete('find-now');
}

/** Scrolls `root` so the match sits in the middle of it, unless it is already in plain view. Smooth only in full motion. */
export function bringIntoView(root: HTMLElement, range: Range, smooth: boolean): void {
  const box = range.getBoundingClientRect();
  const view = root.getBoundingClientRect();
  // The find bar covers the top right of the thread, so a match under it counts as out of view.
  if (box.height && box.top >= view.top + 64 && box.bottom <= view.bottom - 24) return;
  const top = root.scrollTop + box.top - view.top - (root.clientHeight - box.height) / 2;
  root.scrollTo({ top: Math.max(0, top), behavior: smooth ? 'smooth' : 'auto' });
}
