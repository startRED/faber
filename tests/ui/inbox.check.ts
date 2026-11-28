import { createInbox } from '../../ui/inbox.ts';
import { check, deepEqual } from './check.ts';

check('inbox: events that come before anyone listens are handed over first, in order, then the live ones', () => {
  const inbox = createInbox<number>();
  inbox.push(1);
  inbox.push(2);
  const heard: number[] = [];
  inbox.take((n) => heard.push(n));
  inbox.push(3);
  deepEqual(heard, [1, 2, 3]);
});

check('inbox: after the listener stops, events are kept again and nothing is handed over twice', () => {
  const inbox = createInbox<number>();
  const heard: number[] = [];
  inbox.take((n) => heard.push(n));
  inbox.push(1);
  inbox.take(undefined);
  inbox.push(2);
  const later: number[] = [];
  inbox.take((n) => later.push(n));
  deepEqual(heard, [1]);
  deepEqual(later, [2]);
});
