import type { Conversation, HeardMessage, Message, ToolUse } from './api.ts';

/** A reply as an update brought it, made whole from the version the screen holds: the steps it left out are taken from
 * `before`, by place for the first `toolsFrom` and by id after them, so a step that did not change keeps its own object
 * (and its row on screen). `undefined` when `before` lacks one of them: the screen then asks for the reply again. */
export function whole(before: Message | undefined, heard: HeardMessage): Message | undefined {
  const { toolsFrom, tools, ...rest } = heard;
  if (toolsFrom === undefined) return heard as Message;
  const held = before?.tools ?? [];
  if (toolsFrom > held.length) return undefined;
  let byId: Map<string, ToolUse> | undefined;
  const filled: ToolUse[] = held.slice(0, toolsFrom);
  for (const tool of tools ?? []) {
    const kept = 'same' in tool ? (byId ??= new Map(held.map((t) => [t.id, t]))).get(tool.id) : tool;
    if (!kept) return undefined;
    filled.push(kept);
  }
  return { ...rest, tools: filled };
}

/** Puts a message into a conversation: it replaces the one with the same id, or goes last. An update that cannot be made
 * whole (`whole`) leaves the conversation as it was. */
export function upsert(c: Conversation, heard: HeardMessage): Conversation {
  const i = c.messages.findIndex((x) => x.id === heard.id);
  const m = whole(c.messages[i], heard);
  if (!m) return c;
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
  let kept: HeardMessage[] = [];
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
    hear(conversationId: string, message: HeardMessage): 'keep' | 'apply' | 'ignore' {
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
