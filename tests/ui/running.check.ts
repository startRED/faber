import type { QuotaReading, RunningItem } from '../../ui/api.ts';
import { widthClass } from '../../ui/layout.ts';
import { answerOutcome, askKey, blocksOf, createAutoOpen, createRunningStore, elapsed, newest, openPlace, placeOfRunning, QUIET_MS, waitingKeys } from '../../ui/running.ts';
import { groupChats, seedQuotas, tailOf, tightest, withQuota } from '../../ui/sidebar.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const item = (over: Partial<RunningItem> = {}): RunningItem => ({
  conversationId: 'c1',
  turnId: 'm1',
  title: 'A',
  area: 'chat',
  state: 'working',
  since: 0,
  permissions: [],
  ...over,
});
const ask = (requestId: string) => ({ requestId, tool: 'Bash', input: {} });

check('layout: the width decides where the column and the sidebar sit', () => {
  equal(widthClass(1440), 'wide');
  equal(widthClass(1100), 'wide');
  equal(widthClass(1099), 'mid');
  equal(widthClass(900), 'mid');
  equal(widthClass(899), 'narrow');
  equal(widthClass(720), 'narrow');
});

check('running: a list never goes back to an older one, in whatever order they arrive', () => {
  const store = createRunningStore();
  store.offer({ revision: 5, items: [item()] });
  store.offer({ revision: 4, items: [] });
  equal(store.list().revision, 5);
  equal(store.list().items.length, 1);
  store.offer({ revision: 5, items: [] });
  equal(store.list().items.length, 1, 'the same revision keeps the list already held');
  store.offer({ revision: 6, items: [] });
  equal(store.list().items.length, 0);
  equal(newest({ revision: 1, items: [] }, { revision: 2, items: [item()] }).revision, 2);
});

check('running: the first list from the engine is taken, even revision 0', () => {
  const store = createRunningStore();
  store.offer({ revision: 0, items: [item()] });
  equal(store.list().items.length, 1);
});

check('running: one block per project and one for Chat, in the engine order', () => {
  const blocks = blocksOf([
    item({ conversationId: 'a', area: 'project', projectId: 'p2', state: 'needsYou' }),
    item({ conversationId: 'b', area: 'project', projectId: 'p1' }),
    item({ conversationId: 'c', area: 'project', projectId: 'p2' }),
    item({ conversationId: 'd' }),
  ]);
  deepEqual(blocks.map((b) => [b.key, b.items.length, b.needs]), [['p2', 2, 1], ['p1', 1, 0], ['chat', 1, 0]]);
});

check('running: a question is named by conversation, turn and request together', () => {
  const a = item({ conversationId: 'c1', turnId: 'm1', permissions: [ask('0')] });
  const b = item({ conversationId: 'c2', turnId: 'm1', permissions: [ask('0')] });
  deepEqual(waitingKeys([a, b]), ['c1:m1:0', 'c2:m1:0']);
  equal(askKey(a, '7'), 'c1:m1:7');
});

check('auto-open: opens once for each question, even if the user closes the column', () => {
  const open = createAutoOpen();
  deepEqual(open.check([], 0), { open: false });
  deepEqual(open.check(['k1'], 0), { open: true });
  deepEqual(open.check(['k1'], 10), { open: false }, 'the same question does not open it again');
  deepEqual(open.check(['k1', 'k2'], 20), { open: true }, 'a new question does');
});

check('auto-open: waits until the user has stopped typing for 1.5 s', () => {
  const open = createAutoOpen();
  open.typed(1000);
  deepEqual(open.check(['k1'], 1200), { open: false, retryIn: QUIET_MS - 200 });
  open.typed(2000);
  deepEqual(open.check(['k1'], 2500), { open: false, retryIn: QUIET_MS - 500 });
  deepEqual(open.check(['k1'], 2000 + QUIET_MS), { open: true });
});

check('auto-open: a question the user has seen, by opening or closing the column, never opens it again', () => {
  const open = createAutoOpen();
  open.typed(1000);
  deepEqual(open.check(['k1'], 1200), { open: false, retryIn: QUIET_MS - 200 });
  open.seen(['k1']);
  deepEqual(open.check(['k1'], 1000 + QUIET_MS), { open: false }, 'closed during the wait: the pending check finds nothing new');
  deepEqual(open.check(['k1', 'k2'], 5000), { open: true }, 'a question that came later is new');
});

check('auto-open: a question that waits when Faber starts opens the column too', () => {
  const open = createAutoOpen();
  deepEqual(open.check(waitingKeys([item({ permissions: [ask('3')] })]), Date.now()), { open: true });
});

check('places: opening a conversation sets area, project and id together', () => {
  const from = { area: 'project' as const, projectId: 'p1' };
  deepEqual(openPlace(from, { id: 'x', area: 'chat' }), { area: 'chat', projectId: 'p1', id: 'x' });
  deepEqual(openPlace({ area: 'chat' }, { id: 'y', area: 'project', projectId: 'p9' }), { area: 'project', projectId: 'p9', id: 'y' });
  deepEqual(placeOfRunning({ area: 'chat' }, item({ conversationId: 'z', area: 'project', projectId: 'p2' })), { area: 'project', projectId: 'p2', id: 'z' });
});

check('answers: a stale question is dropped quietly; any other error is shown', () => {
  equal(answerOutcome('error.permission.stale'), 'stale');
  deepEqual(answerOutcome('error.notDelivered'), { problem: 'error.notDelivered' });
});

check('elapsed: seconds, minutes, then hours', () => {
  deepEqual(elapsed(0, 5_000), { key: 'time.seconds', n: 5 });
  deepEqual(elapsed(0, 59_999), { key: 'time.seconds', n: 59 });
  deepEqual(elapsed(0, 60_000), { key: 'time.minutes', n: 1 });
  deepEqual(elapsed(0, 3_600_000), { key: 'time.hours', n: 1 });
  deepEqual(elapsed(10_000, 0), { key: 'time.seconds', n: 0 }, 'a clock that runs behind never shows a negative time');
});

check('sidebar: chats are grouped by the user\'s own days', () => {
  const now = new Date(2026, 9, 11, 15, 0).getTime();
  const at = (days: number, hour = 12) => new Date(2026, 9, 11 - days, hour).getTime();
  const groups = groupChats(
    [
      { id: 'a', updatedAt: at(0, 0) },
      { id: 'b', updatedAt: at(1, 23) },
      { id: 'c', updatedAt: at(6, 0) },
      { id: 'd', updatedAt: at(7, 23) },
    ],
    now,
  );
  deepEqual(groups.map((g) => [g.group, g.chats.map((c) => c.id)]), [['today', ['a']], ['week', ['b', 'c']], ['earlier', ['d']]]);
  deepEqual(groupChats([], now), []);
});

check('quota: the tightest window of the vendor counts, and an old reading counts for nothing', () => {
  const now = 1_000_000;
  const reading = (window: string, used: number, resetsAt: number, vendor: QuotaReading['vendor'] = 'claude'): QuotaReading => ({ vendor, window, used, resetsAt });
  let quotas = seedQuotas([reading('five_hour', 0.2, 2000), reading('seven_day', 0.6, 5000), reading('five_hour', 0.9, 2000, 'codex')]);
  equal(tightest(quotas, 'claude', now)?.window, 'seven_day');
  equal(tightest(quotas, 'codex', now)?.used, 0.9);
  quotas = withQuota(quotas, reading('seven_day', 0.7, 900));
  equal(tightest(quotas, 'claude', now)?.window, 'five_hour', 'the 7-day reading has passed its reset time');
  quotas = withQuota(quotas, reading('five_hour', 0.1, 0));
  equal(tightest(quotas, 'claude', now)?.window, 'five_hour', 'a reading with no reset time stands');
  equal(tightest({}, 'claude', now), undefined);
});

check('sidebar: a project row says how many questions wait and whether anything runs', () => {
  const items = [item({ area: 'project', projectId: 'p1', state: 'needsYou' }), item({ conversationId: 'c2', area: 'project', projectId: 'p1' }), item({ conversationId: 'c3' })];
  deepEqual(tailOf(items, 'p1'), { needs: 1, running: true });
  deepEqual(tailOf(items, 'p2'), { needs: 0, running: false });
  ok(!tailOf([], 'p1').running);
});
