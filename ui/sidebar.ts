import type { QuotaReading, RunningItem, Vendor } from './api.ts';

// The data behind the sidebar: how the chats are grouped, which quota reading each vendor shows, and what the tail of a
// project says. Pure functions, so the screen's checks run them with a fixed clock.

export type ChatGroup = 'today' | 'week' | 'earlier';

const startOfDay = (ms: number, back = 0) => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
};

/** Chats, newest first as given, in three groups: today, the six days before today ("this week"), and earlier. Empty
 * groups are left out. Local midnight draws the lines, so the groups follow the user's own days. */
export function groupChats<T extends { updatedAt: number }>(chats: T[], now: number): { group: ChatGroup; chats: T[] }[] {
  const today = startOfDay(now);
  const week = startOfDay(now, 6);
  const groups: Record<ChatGroup, T[]> = { today: [], week: [], earlier: [] };
  for (const chat of chats) groups[chat.updatedAt >= today ? 'today' : chat.updatedAt >= week ? 'week' : 'earlier'].push(chat);
  return (['today', 'week', 'earlier'] as const).filter((g) => groups[g].length > 0).map((g) => ({ group: g, chats: groups[g] }));
}

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

/** What a project's row says about its running conversations: how many wait for the user, and whether any runs. */
export function tailOf(items: RunningItem[], projectId: string): { needs: number; running: boolean } {
  const mine = items.filter((i) => i.area === 'project' && i.projectId === projectId);
  return { needs: mine.filter((i) => i.state === 'needsYou').length, running: mine.length > 0 };
}
