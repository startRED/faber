import { createEffect, createSignal, onCleanup, type Accessor } from 'solid-js';
import type { Message } from './api.ts';
import { windowHidden } from './appearance.ts';
import { t } from './i18n.ts';
import { elapsed } from './running.ts';

// The quiet line in the foot of a reply (docs/design/layout.md, "Answers"): whether the agent is still at work or
// finished, how long it worked, and, on the newest reply only, how long ago it last changed.

/** A reply that worked less than this says nothing of its work: "Done" next to a short answer would only be noise. */
const WORK_MIN_MS = 60_000;
/** Past a day the age is the sidebar's to tell, not the foot's. */
const AGE_MAX_MS = 24 * 3_600_000;
const [now, setNow] = createSignal(Date.now());

/** The time the foot counts its age against. It moves once a minute while a newest reply shows an age and the window is shown;
 * a check sets it by hand. */
export const ageNow: Accessor<number> = now;
export const setAgeNow = setNow;

/** The agent said all it will say (or the turn ended well) while the CLI may still be closing its turn. */
export const finished = (m: Pick<Message, 'status' | 'finishedAt'>) => m.status === 'done' || (m.status === 'streaming' && m.finishedAt !== undefined);

/** True while the newest reply shows an age that can still change. */
export function agePending(m: Message, last: boolean, at: number): boolean {
  const from = ageFrom(m);
  return last && from !== undefined && at - from < AGE_MAX_MS && (m.status === 'streaming' || m.status === 'done');
}

/** When the reply last changed: its words or tools while it works, the moment the agent finished once it did. */
function ageFrom(m: Message): number | undefined {
  return finished(m) ? (m.finishedAt ?? m.movedAt) : (m.movedAt ?? m.at);
}

/** The foot line of a reply, or '' when there is nothing to say (an error, a stop, or a reply from before Faber kept the times). */
export function footText(m: Message, last: boolean, at: number): string {
  if (m.status !== 'streaming' && m.status !== 'done') return '';
  if (m.movedAt === undefined && m.finishedAt === undefined) return '';
  const done = finished(m);
  const parts = [t(done ? 'foot.done' : 'foot.working')];
  const worked = (m.finishedAt ?? m.movedAt ?? m.at) - m.at;
  if (done && worked >= WORK_MIN_MS) {
    const e = elapsed(0, worked);
    parts.push(t('foot.work', { time: t(e.key, { n: e.n }) }));
  }
  const from = last ? ageFrom(m) : undefined;
  if (from !== undefined && at - from < AGE_MAX_MS) {
    const e = elapsed(from, at);
    parts.push(e.key === 'time.seconds' ? t('foot.justNow') : t('foot.updated', { time: t(e.key, { n: e.n }) }));
  }
  return parts.join(' · ');
}

/** Moves `ageNow` once a minute while `enabled` and the window is shown. A hidden window has no timer; the time catches up the
 * moment it comes back. */
export function tickWhile(enabled: Accessor<boolean>): void {
  createEffect(() => {
    if (!enabled() || windowHidden()) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    onCleanup(() => clearInterval(timer));
  });
}
