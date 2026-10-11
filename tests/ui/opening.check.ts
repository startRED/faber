import type { Conversation, Message } from '../../ui/api.ts';
import { createOpening, upsert } from '../../ui/opening.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const message = (id: string, text: string, status: Message['status']): Message => ({ id, role: 'assistant', text, at: 1, status });
const conversation = (id: string, messages: Message[]): Conversation => ({
  id,
  title: 'A chat',
  area: 'chat',
  sessions: {},
  createdAt: 1,
  updatedAt: 1,
  messages,
});

check('opening: an answer that finishes while the snapshot is on its way ends up finished, not streaming', () => {
  const opening = createOpening();
  const token = opening.begin('a');
  // The engine took its snapshot while the answer was still streaming; the end of the answer then reached the screen
  // before the snapshot did.
  const snapshot = conversation('a', [message('q', 'hi', 'done'), message('r', 'Hel', 'streaming')]);
  equal(opening.hear('a', message('r', 'Hello', 'streaming')), 'keep');
  equal(opening.hear('a', message('r', 'Hello there', 'done')), 'keep');
  const done = opening.finish(token, snapshot);
  ok(!done.stale, 'the open was not replaced');
  if (done.stale) return;
  deepEqual(done.conversation?.messages.map((m) => [m.id, m.text, m.status]), [
    ['q', 'hi', 'done'],
    ['r', 'Hello there', 'done'],
  ]);
  equal(snapshot.messages[1]?.status, 'streaming', 'the snapshot itself is not changed');
});

check('opening: messages apply at once once the snapshot is in, and only for the conversation on screen', () => {
  const opening = createOpening();
  opening.finish(opening.begin('a'), conversation('a', []));
  equal(opening.hear('a', message('r', 'x', 'streaming')), 'apply');
  equal(opening.hear('b', message('r', 'x', 'streaming')), 'ignore', 'another conversation');
  opening.begin(undefined);
  equal(opening.hear('a', message('r', 'x', 'streaming')), 'ignore', 'a new, empty conversation has no id yet');
});

check('opening: an open that another open replaced is stale and its kept messages are dropped', () => {
  const opening = createOpening();
  const first = opening.begin('a');
  equal(opening.hear('a', message('r', 'old', 'done')), 'keep');
  const second = opening.begin('b');
  ok(opening.finish(first, conversation('a', [])).stale, 'the answer for a arrives after b began');
  const done = opening.finish(second, conversation('b', []));
  ok(!done.stale);
  if (!done.stale) equal(done.conversation?.messages.length, 0, 'nothing from a leaked into b');
});

check('opening: A, then B, then A again: the answer of the first A is stale even though the id is the same', () => {
  const opening = createOpening();
  const firstA = opening.begin('a');
  opening.begin('b');
  const secondA = opening.begin('a');
  // The newer snapshot of A has the finished answer; the older one, still streaming, is slower and arrives last.
  equal(opening.hear('a', message('r', 'Done', 'done')), 'keep');
  const newer = opening.finish(secondA, conversation('a', [message('r', 'Do', 'streaming')]));
  ok(!newer.stale);
  if (!newer.stale) equal(newer.conversation?.messages[0]?.status, 'done');
  ok(opening.finish(firstA, conversation('a', [message('r', 'D', 'streaming')])).stale, 'the first A must not be applied');
});

check('opening: upsert replaces a message by id and appends a new one', () => {
  const c = conversation('a', [message('r', 'one', 'streaming')]);
  deepEqual(upsert(c, message('r', 'two', 'done')).messages.map((m) => m.text), ['two']);
  deepEqual(upsert(c, message('s', 'next', 'done')).messages.map((m) => m.id), ['r', 's']);
});
