// The data behind the Ctrl+K palette: how loosely a few typed letters match a name, and in which order the matches come.
// Pure functions, so the screen's checks run them without a window.

/** One thing the palette can open or do. `hint` is the quiet word beside the name (the project a conversation belongs to). */
export interface Entry<I = string> {
  id: string;
  kind: 'conversation' | 'project' | 'action';
  label: string;
  hint?: string;
  icon: I;
  run: () => void;
}

// One character folds to one character, so a match found in the folded text sits at the same place in the original. Only
// capitals and non-ASCII characters are looked at, which keeps a long conversation fast.
const one = (c: string) => {
  const folded = c.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
  return folded.length === c.length ? folded : c;
};
export const fold = (text: string) => text.replace(/[^\0-\x7f]|[A-Z]/gu, one);

/** How well one word of the search matches a text, 0 for not at all: at the start of the text 6, at the start of a word 4,
 * inside a word 2, and its letters in order with others between ("stg" in "Settings") 1. */
function word(text: string, typed: string): number {
  const at = text.indexOf(typed);
  if (at === 0) return 6;
  if (at > 0) return /[\p{L}\p{N}]/u.test(text[at - 1]!) ? 2 : 4;
  if (typed.length < 2) return 0;
  let next = 0;
  for (const c of text) if (c === typed[next] && ++next === typed.length) return 1;
  return 0;
}

/** The entries that hold every word of the search, best first. Equal scores keep the order given, so what was passed first
 * (the actions, then the newest conversations) wins a tie. The name counts fully, the hint half. */
export function rank<E extends { label: string; hint?: string }>(entries: readonly E[], query: string, limit = 8): E[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const scored: Array<[number, E]> = [];
  for (const entry of entries) {
    const name = fold(entry.label);
    const note = entry.hint ? fold(entry.hint) : '';
    let total = 0;
    for (const typed of words) {
      const best = Math.max(word(name, typed), word(note, typed) / 2);
      if (!best) {
        total = 0;
        break;
      }
      total += best;
    }
    if (total) scored.push([total, entry]);
  }
  return scored.sort((a, b) => b[0] - a[0]).slice(0, limit).map(([, entry]) => entry);
}
