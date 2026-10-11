import type { Conversation, Message } from './api.ts';

/** Puts a message into a conversation: it replaces the one with the same id, or goes last. */
export function upsert(c: Conversation, m: Message): Conversation {
  const i = c.messages.findIndex((x) => x.id === m.id);
  const messages = i < 0 ? [...c.messages, m] : c.messages.map((x, j) => (j === i ? m : x));
  return { ...c, messages };
}

/**
 * Opening a conversation asks the engine for a snapshot, and the answer takes a while to arrive. Messages the engine
 * emits meanwhile must not be lost (an answer that finishes in that gap would stay "streaming" for good), so they are
 * kept and applied on top of the snapshot, in the order they came.
 */
export function createOpening() {
  let id: string | undefined;
  let loading = false;
  let kept: Message[] = [];
  // Every open gets its own number. The conversation id is not enough to tell opens apart: A, then B, then A again must
  // not accept the answer of the first A.
  let generation = 0;
  return {
    /** The conversation the screen is on, or is opening. */
    get id() {
      return id;
    },
    /** A new open starts; whatever an earlier one was waiting for is dropped. Returns the number to give back to `finish`. */
    begin(next: string | undefined): number {
      id = next;
      loading = next !== undefined;
      kept = [];
      return ++generation;
    },
    /** A message arrived from the engine. `keep`: hold it for the snapshot. `apply`: put it in the open conversation now. */
    hear(conversationId: string, message: Message): 'keep' | 'apply' | 'ignore' {
      if (conversationId !== id) return 'ignore';
      if (!loading) return 'apply';
      kept.push(message);
      return 'keep';
    },
    /** The snapshot of the open numbered `token` arrived. `stale` when another open began meanwhile; otherwise the snapshot with the kept messages in it. */
    finish(token: number, snapshot: Conversation | undefined): { stale: true } | { stale: false; conversation: Conversation | undefined } {
      if (token !== generation) return { stale: true };
      loading = false;
      const held = kept;
      kept = [];
      return { stale: false, conversation: snapshot && held.reduce(upsert, snapshot) };
    },
  };
}
