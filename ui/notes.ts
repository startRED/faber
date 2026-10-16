import { en, type Key } from './locales/en.ts';

// "What's new": the short list shown once after an update (docs/design.md). The entries live in every `ui/locales/*.ts`
// as `notes.<id>.<kind>`, written by hand in each language. The id only grows; a change a person would notice adds the
// next one in the same pull request, so the list never depends on the version number. The screen keeps the last id seen
// (the shell's `notes_seen`) and shows only newer ones. `scripts/release-notes.mjs` builds the release body from the same
// English lines.

export type Kind = 'new' | 'better' | 'fixed';

/** The order the groups are shown in. */
export const KINDS: readonly Kind[] = ['new', 'better', 'fixed'];

export interface Note {
  id: number;
  kind: Kind;
  key: Key;
}

const PATTERN = /^notes\.(\d+)\.(new|better|fixed)$/;

/** Every entry, oldest first, read from the English file: its keys are the list. */
export function notesOf(keys: readonly string[]): Note[] {
  return keys
    .flatMap((key) => {
      const found = PATTERN.exec(key);
      return found ? [{ id: Number(found[1]), kind: found[2] as Kind, key: key as Key }] : [];
    })
    .sort((a, b) => a.id - b.id);
}

export const NOTES: readonly Note[] = notesOf(Object.keys(en));

export interface Due {
  /** The entries to show, oldest first; none means no screen. */
  show: Note[];
  /** An id to store now, without showing anything: a first install starts from the newest entry. */
  remember?: number;
}

/**
 * What to do at start. `seen` is the last id the person saw, none when nothing was ever stored. A first install (the first
 * run is still ahead) has seen everything there is; an install that was updated from before this screen existed has seen
 * nothing, so it gets the whole list once.
 */
export function whatsNew(notes: readonly Note[], seen: number | undefined, firstInstall: boolean): Due {
  const newest = notes.reduce((top, n) => Math.max(top, n.id), 0);
  if (seen === undefined && firstInstall) return newest > 0 ? { show: [], remember: newest } : { show: [] };
  const show = notes.filter((n) => n.id > (seen ?? 0));
  return { show };
}

/** The newest id of `notes`: what closing the screen stores. */
export const newestId = (notes: readonly Note[]): number => notes.reduce((top, n) => Math.max(top, n.id), 0);

/** The entries grouped by kind, in `KINDS` order, leaving out a kind with none. */
export function grouped(notes: readonly Note[]): Array<{ kind: Kind; notes: Note[] }> {
  return KINDS.map((kind) => ({ kind, notes: notes.filter((n) => n.kind === kind) })).filter((g) => g.notes.length > 0);
}

/**
 * A first install has no data at all: nothing saved in the settings and no conversation. An install that has either is an
 * existing one, even when it never saved an appearance (an old version) or never saw an entry: it is shown the newest ones.
 */
export function isFirstInstall(settings: object | undefined, conversations: readonly unknown[]): boolean {
  return conversations.length === 0 && Object.values(settings ?? {}).every((value) => value === undefined);
}
