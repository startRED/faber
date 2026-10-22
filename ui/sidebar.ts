import type { CheckResult, QuotaReading, RunningItem, Vendor, VendorUsage } from './api.ts';
import type { Key } from './locales/en.ts';

// The data behind the sidebar: how the chats are grouped, which quota reading each vendor shows, and what the tail of a
// project says. Pure functions, so the screen's checks run them with a fixed clock.

export type ChatGroup = 'pinned' | 'today' | 'week' | 'earlier';

/** Pinned conversations keep their own order, independent of new messages. */
export const pinned = <T extends { pinnedAt?: number }>(rows: T[]): T[] =>
  rows.filter((c) => c.pinnedAt !== undefined).sort((a, b) => b.pinnedAt! - a.pinnedAt!);

const fold = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();

/** Whether a title holds what the user typed in the search box, ignoring case and accents. An empty search holds them all. */
export const matches = (title: string, query: string) => fold(title).includes(fold(query.trim()));

const startOfDay = (ms: number, back = 0) => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
};

/** Pinned first, then chats as given in their local date groups. A search uses the date groups alone. */
export function groupChats<T extends { updatedAt: number; pinnedAt?: number }>(chats: T[], now: number, groupPins = true): { group: ChatGroup; chats: T[] }[] {
  const today = startOfDay(now);
  const week = startOfDay(now, 6);
  const groups: Record<ChatGroup, T[]> = { pinned: groupPins ? pinned(chats) : [], today: [], week: [], earlier: [] };
  for (const chat of chats) {
    if (groupPins && chat.pinnedAt !== undefined) continue;
    groups[chat.updatedAt >= today ? 'today' : chat.updatedAt >= week ? 'week' : 'earlier'].push(chat);
  }
  return (['pinned', 'today', 'week', 'earlier'] as const).filter((g) => groups[g].length > 0).map((g) => ({ group: g, chats: groups[g] }));
}

/** Conversations in the order given, except that each one an agent started follows the conversation that started it (in
 * the order given too). One whose starter is not in the list stays where it was. The rows are the same objects, so the
 * sidebar keeps their nodes. */
export function nest<T extends { id: string; startedBy?: string }>(rows: T[]): T[] {
  const present = new Set(rows.map((r) => r.id));
  const under = (id: string) => rows.filter((r) => r.startedBy === id);
  const placed = (r: T): T[] => [r, ...under(r.id).flatMap(placed)];
  return rows.filter((r) => !r.startedBy || !present.has(r.startedBy)).flatMap(placed);
}

/** A refreshed list with every item that did not change replaced by the one already held, so the sidebar keeps the nodes of the
 * rows that are the same (a refresh while a rename or a hover is open leaves them alone, and no row is built and animated again). */
export function keepRows<T extends { id: string }>(held: readonly T[], fresh: T[]): T[] {
  const known = new Map(held.map((item) => [item.id, item]));
  const rows = fresh.map((item) => {
    const old = known.get(item.id);
    return old && JSON.stringify(old) === JSON.stringify(item) ? old : item;
  });
  return rows.length === held.length && rows.every((row, i) => row === held[i]) ? (held as T[]) : rows;
}

/** Whether a project shows its conversations: the user opened it, or the search found some in it. The sidebar draws the list
 * and sets `aria-expanded` from this one answer, so a screen reader never hears a different one. */
export const showsChats = (opened: boolean, searching: boolean, found: number) => opened || (searching && found > 0);

/** The latest reading of each window, by vendor and window name. */
export type Quotas = Record<string, QuotaReading>;

export const seedQuotas = (readings: QuotaReading[]): Quotas => Object.fromEntries(readings.map((q) => [`${q.vendor}:${q.window}`, q]));

export const withQuota = (held: Quotas, q: QuotaReading): Quotas => ({ ...held, [`${q.vendor}:${q.window}`]: q });

/** A reading whose reset time has passed says nothing about now. A reading with no reset time (0) cannot be told to be
 * old, so it stands until a newer one replaces it. */
const expired = (q: QuotaReading, now: number) => q.resetsAt > 0 && q.resetsAt * 1000 <= now;

/** The vendor's tightest window: the reading with the highest used share among those that still count, or undefined when
 * there is none (the vendor has not reported yet, or every reading is old). */
export function tightest(quotas: Quotas, vendor: Vendor, now: number): QuotaReading | undefined {
  const live = Object.values(quotas).filter((q) => q.vendor === vendor && !expired(q, now));
  return live.reduce<QuotaReading | undefined>((best, q) => (best && best.used >= q.used ? best : q), undefined);
}

/** What the sidebar's line shows for a vendor (R177): its weekly window. A vendor that has never reported one (a plan with
 * no weekly cap) shows its tightest window instead; one whose weekly reading has passed its reset shows nothing known. */
export function lineReading(quotas: Quotas, vendor: Vendor, now: number): QuotaReading | undefined {
  const weekly = quotas[`${vendor}:seven_day`];
  if (!weekly) return tightest(quotas, vendor, now);
  return expired(weekly, now) ? undefined : weekly;
}

/** The vendor's live readings other than `shown`, short windows first: the tooltip lists them so nothing is lost. */
export function otherReadings(quotas: Quotas, vendor: Vendor, now: number, shown?: QuotaReading): QuotaReading[] {
  const rank = (window: string) => (WINDOW_ORDER.includes(window) ? WINDOW_ORDER.indexOf(window) : WINDOW_ORDER.length);
  return Object.values(quotas)
    .filter((q) => q.vendor === vendor && q !== shown && !expired(q, now))
    .sort((a, b) => rank(a.window) - rank(b.window));
}

/** One line of the usage limits in Settings. A reading that still counts has `used` (a share from 0 to 1) and, when the CLI
 * said when the window resets, `resetsAt` (seconds). A window whose reading is old has only its name; a vendor that has
 * not reported at all has one line with no window. */
export interface UsageRow {
  window?: string;
  used?: number;
  resetsAt?: number;
}

const WINDOW_ORDER = ['five_hour', 'seven_day'];

/** The lines of one vendor: every window it has reported, the short ones first. */
export function usageRows(quotas: Quotas, vendor: Vendor, now: number): UsageRow[] {
  const mine = Object.values(quotas).filter((q) => q.vendor === vendor);
  if (mine.length === 0) return [{}];
  const rank = (window: string) => (WINDOW_ORDER.includes(window) ? WINDOW_ORDER.indexOf(window) : WINDOW_ORDER.length);
  return mine
    .sort((a, b) => rank(a.window) - rank(b.window) || (a.window < b.window ? -1 : 1))
    .map((q) => (expired(q, now) ? { window: q.window } : { window: q.window, used: q.used, resetsAt: q.resetsAt }));
}

/** How old a reading is, as the key of the words and the number to put in them: "now", "4 min ago", "2 h ago", "3 d ago". Under a
 * minute is "now"; a clock that went back is "now" too. */
export function ageOf(readAt: number, nowMs: number): { key: Key; n: number } {
  const secs = Math.max(0, nowMs / 1000 - readAt);
  if (secs < 60) return { key: 'quota.age.now', n: 0 };
  if (secs < 3600) return { key: 'quota.age.min', n: Math.floor(secs / 60) };
  if (secs < 86_400) return { key: 'quota.age.hour', n: Math.floor(secs / 3600) };
  return { key: 'quota.age.day', n: Math.floor(secs / 86_400) };
}

/** What the screen knows about how current a vendor's number is: when it was read (the newest of its windows, or of the one
 * asked for), whether Faber's own recent work may not be in it yet, and, where a measured conversion exists, how much that
 * work adds. A share under half a point is not shown as a number. */
export interface Freshness {
  readAt?: number;
  counting: boolean;
  /** Points (0 to 100, whole) that the vendor has not counted yet, when known. */
  uncountedPoints?: number;
}

export function freshness(usage: VendorUsage[] | undefined, vendor: Vendor, window?: string): Freshness {
  const u = usage?.find((x) => x.vendor === vendor);
  if (!u) return { counting: false };
  const mine = window ? u.windows.filter((w) => w.name === window) : u.windows;
  const readAt = mine.reduce<number | undefined>((newest, w) => (newest === undefined || w.readAt > newest ? w.readAt : newest), undefined);
  const extra = mine.find((w) => w.uncounted != null && w.uncounted >= 0.005)?.uncounted;
  return { readAt, counting: u.counting, uncountedPoints: extra == null ? undefined : Math.round(extra * 100) };
}

/** What Settings says about one account. An installed CLI is not a connected account: only the CLI saying it is signed in
 * is. `undefined` is a check still on its way, `'failed'` one that could not run. */
export type AccountLine = 'checking' | 'connected' | 'notConnected' | 'unsure';

export function accountLine(check: CheckResult | 'failed' | undefined): AccountLine {
  if (!check) return 'checking';
  if (check === 'failed') return 'unsure';
  if (check.account === 'signedIn') return 'connected';
  return check.account === 'unknown' && check.installed ? 'unsure' : 'notConnected';
}

/** What a project's row says about its running conversations: how many wait for the user, and whether any runs. */
export function tailOf(items: RunningItem[], projectId: string): { needs: number; running: boolean } {
  const mine = items.filter((i) => i.area === 'project' && i.projectId === projectId);
  return { needs: mine.filter((i) => i.state === 'needsYou').length, running: mine.length > 0 };
}

/** The conversation `step` rows from the open one in the sidebar's order (pinned, today, this week, earlier) within the
 * same list: the chat list, or the open project's. None at the ends, or when the open one is not in the list. */
export function neighbour<T extends { id: string; area: string; projectId?: string; updatedAt: number; pinnedAt?: number; archived?: boolean; startedBy?: string }>(
  all: T[], at: { id?: string; area: string; projectId?: string }, step: 1 | -1, now: number, apart: ReadonlySet<string> = new Set(),
): T | undefined {
  const here = all.filter((c) => !c.archived && !(at.area === 'chat' && apart.has(c.id)) && c.area === at.area && (at.area !== 'project' || c.projectId === at.projectId));
  const order = nest(groupChats(here, now).flatMap((g) => g.chats));
  const i = order.findIndex((c) => c.id === at.id);
  return i < 0 ? undefined : order[i + step];
}
