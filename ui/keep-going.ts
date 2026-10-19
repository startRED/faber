import type { RunningItem, VendorUsage } from './api.ts';
import { t } from './i18n.ts';
import { untilReset } from './Sidebar.tsx';

/** A window above this share is out for the engine (`budget::ADMIT_LIMIT`). */
const OUT = 0.98;

/** How the quota looks for "keep going" (R199, R212): `out` every AI it may use is out (it waits for `when`), `billed`
 * only AIs Faber does not spend on by itself are left (it stops), `low` every AI it may use is near its limit, `fast`
 * every one is being spent faster than its daily line. Undefined while one AI is fine: keep going moves to it. */
export type Outlook = { kind: 'out' | 'billed' | 'low' | 'fast'; when?: number };

const billed = (u: VendorUsage) => u.billedPerUse || u.billingUnknown;

export function keepGoingOutlook(usage: readonly VendorUsage[]): Outlook | undefined {
  if (!usage.length) return undefined;
  const usable = usage.filter((u) => !billed(u) && u.state !== 'out');
  if (!usable.length) {
    const out = usage.filter((u) => !billed(u));
    if (!out.length) return { kind: 'billed' };
    // Each AI is back when its last full window resets; the first AI back is when keep going goes on.
    const back = out
      .map((u) => Math.max(0, ...u.windows.filter((w) => w.used > OUT && w.resetsAt > 0).map((w) => w.resetsAt)))
      .filter((at) => at > 0);
    return { kind: 'out', when: back.length ? Math.min(...back) : undefined };
  }
  if (usable.every((u) => u.state === 'low')) return { kind: 'low' };
  if (usable.every((u) => u.state === 'low' || u.state === 'fast')) return { kind: 'fast' };
  return undefined;
}

/** The words for an outlook, or for keep going that already waits (the running row's `waitUntil`), which comes first. */
export function keepGoingWarning(usage: readonly VendorUsage[], row?: RunningItem): string | undefined {
  if (row?.keepGoing && row.waitUntil) return t('keepGoing.warn.waiting', { when: untilReset(row.waitUntil) });
  const o = keepGoingOutlook(usage);
  if (!o) return undefined;
  if (o.kind === 'out') return o.when ? t('keepGoing.warn.out', { when: untilReset(o.when) }) : t('keepGoing.warn.outUnknown');
  return t(`keepGoing.warn.${o.kind}`);
}
