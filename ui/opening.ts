import type { Conversation, HeardMessage, Message, ToolUse } from './api.ts';

/** A reply as an update brought it, made whole from the version the screen holds: the steps it left out are taken from
 * `before`, by place for the first `toolsFrom` and by id after them, so a step that did not change keeps its own object
 * (and its row on screen); words and thinking sent from a point (`textFrom`, `thinkingFrom`) go after what `before`
 * holds up to it. `undefined` when `before` lacks one of them: the screen then asks for the reply again. */
export function whole(before: Message | undefined, heard: HeardMessage): Message | undefined {
  if (heard.toolsFrom === undefined && heard.textFrom === undefined && heard.thinkingFrom === undefined) return heard as Message;
  const { toolsFrom, tools, textFrom, thinkingFrom, ...rest } = heard;
  if (textFrom !== undefined) {
    if (before?.text.length !== textFrom) return undefined;
    rest.text = before.text + rest.text;
  }
  if (thinkingFrom !== undefined) {
    if ((before?.thinking ?? '').length !== thinkingFrom) return undefined;
    rest.thinking = (before?.thinking ?? '') + (rest.thinking ?? '');
  }
  if (toolsFrom === undefined) return tools === undefined ? (rest as Message) : ({ ...rest, tools } as Message);
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

/** Two updates of one reply as one, as if the screen had heard both in turn: what a screen held back (`ui/quiet.ts`) keeps
 * one update per reply. A part the later one sends whole replaces the earlier one's; a part it sends from a point the
 * earlier one reached goes after it. */
export function combine(earlier: HeardMessage, later: HeardMessage): HeardMessage {
  if (earlier.id !== later.id) return later;
  const out: HeardMessage = { ...later };
  const grow = (key: 'text' | 'thinking', from: 'textFrom' | 'thinkingFrom') => {
    const next = later[from];
    if (next === undefined) return;
    const base = earlier[from] ?? 0;
    const held = earlier[key] ?? '';
    // The later one starts where the earlier one ended: one piece from the earlier start (or whole, when that was whole).
    if (base + held.length !== next) return;
    out[key] = held + (later[key] ?? '');
    if (earlier[from] === undefined) delete out[from];
    else out[from] = base;
  };
  grow('text', 'textFrom');
  grow('thinking', 'thinkingFrom');
  if (later.toolsFrom !== undefined && earlier.tools) {
    const from = earlier.toolsFrom ?? 0;
    const held = earlier.tools;
    const byId = new Map(held.filter((tool): tool is ToolUse => !('same' in tool)).map((tool) => [tool.id, tool]));
    const after = (later.tools ?? []).map((tool) => ('same' in tool ? (byId.get(tool.id) ?? tool) : tool));
    if (later.toolsFrom >= from) {
      out.tools = [...held.slice(0, later.toolsFrom - from), ...after];
      if (earlier.toolsFrom === undefined) delete out.toolsFrom;
      else out.toolsFrom = from;
    } else out.tools = after;
  }
  return out;
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
