import type { Message, ToolUse } from './api.ts';
import { isAgentCall, isProgressCall } from './worklog.ts';

// A reply as it happened (W1, 2026-10-31): the agent's words, its thinking and its steps in the order they came, the way a
// harness shows them. The engine records where the words stood when each step started (`ToolUse.at`) and where each piece
// of thinking falls (`Message.thinkingMarks`); nothing here folds the words away or moves the answer.

/** One thing between two stretches of words: a step of the agent's own, or a piece of its thinking. */
export type Item = { kind: 'tool'; tool: ToolUse; key: string } | { kind: 'thought'; text: string; key: string };

/** A stretch of words (`from`..`to` in the reply's text, UTF-16 units), the steps and thinking that came together between
 * two stretches, or a subagent or helper the agent started, which always stands as its own row. */
export type Part =
  | { kind: 'text'; from: number; to: number; key: string }
  | { kind: 'steps'; items: Item[]; key: string }
  | { kind: 'agent'; tool: ToolUse; key: string };

/** The pieces of a reply's thinking, each with where it falls: before the agent's step number `steps`, `at` units into the
 * words. A reply recorded before the marks existed has one piece, at the start. */
function thoughts(m: Message): { text: string; at: number; steps: number }[] {
  const thinking = m.thinking ?? '';
  if (!thinking.trim()) return [];
  const marks = m.thinkingMarks?.length ? m.thinkingMarks : [{ from: 0, at: 0, steps: 0 }];
  return marks
    .map((mark, i) => ({ text: thinking.slice(mark.from, marks[i + 1]?.from ?? thinking.length), at: mark.at, steps: mark.steps }))
    .filter((piece) => piece.text.trim());
}

/** The reply's parts in order. `text` is the text as drawn (the same length as the saved one). A step recorded before the
 * engine kept its place sits where the answer began (`activityEnd`), or at the start. */
export function timeline(m: Message, text: string): Part[] {
  const own = (m.tools ?? []).filter((tool) => !tool.parent);
  const legacy = Number.isInteger(m.activityEnd) ? m.activityEnd! : 0;
  const clamp = (at: number) => Math.max(0, Math.min(text.length, at));
  type Event = { at: number; order: number; item?: Item; agent?: ToolUse };
  const events: Event[] = [];
  own.forEach((tool, i) => {
    if (isProgressCall(tool)) return;
    const at = clamp(tool.at ?? legacy);
    // A thought "before step n" comes before the step with index n, so a step's order is 2n + 1 and a thought's is 2n.
    if (isAgentCall(tool)) events.push({ at, order: 2 * i + 1, agent: tool });
    else events.push({ at, order: 2 * i + 1, item: { kind: 'tool', tool, key: tool.id } });
  });
  // Nested helpers share their top-level ancestor's place; their own work stays inside their row.
  const all = m.tools ?? [];
  const byId = new Map(all.map((tool) => [tool.id, tool]));
  all.forEach((tool, index) => {
    if (!tool.parent || !isAgentCall(tool)) return;
    let ancestor: ToolUse | undefined = tool;
    for (let hops = 0; ancestor?.parent && hops < all.length; hops++) ancestor = byId.get(ancestor.parent);
    const i = ancestor ? own.indexOf(ancestor) : -1;
    if (i >= 0) events.push({ at: clamp(ancestor!.at ?? legacy), order: 2 * i + 1 + (index + 1) / (all.length + 1), agent: tool });
  });
  for (const piece of thoughts(m)) {
    events.push({ at: clamp(piece.at), order: 2 * piece.steps, item: { kind: 'thought', text: piece.text, key: `thought:${piece.at}:${piece.steps}` } });
  }
  events.sort((a, b) => a.at - b.at || a.order - b.order);
  const parts: Part[] = [];
  let cursor = 0;
  const words = (to: number) => {
    if (to > cursor && text.slice(cursor, to).trim()) parts.push({ kind: 'text', from: cursor, to, key: `text:${cursor}` });
    cursor = Math.max(cursor, to);
  };
  for (const event of events) {
    words(event.at);
    if (event.agent) {
      parts.push({ kind: 'agent', tool: event.agent, key: event.agent.id });
      continue;
    }
    const last = parts.at(-1);
    if (last?.kind === 'steps') last.items.push(event.item!);
    else parts.push({ kind: 'steps', items: [event.item!], key: event.item!.key });
  }
  words(text.length);
  return parts;
}

/** Whether two parts draw the same thing. */
function samePart(part: Part, other: Part): boolean {
  if (part.kind !== other.kind || part.key !== other.key) return false;
  if (part.kind === 'text') return other.kind === 'text' && part.to === other.to;
  if (part.kind === 'agent') return other.kind === 'agent' && part.tool === other.tool;
  return (
    other.kind === 'steps' &&
    part.items.length === other.items.length &&
    part.items.every((item, j) => {
      const next = other.items[j]!;
      return item.key === next.key && (item.kind === 'tool' ? next.kind === 'tool' && item.tool === next.tool : next.kind === 'thought' && item.text === next.text);
    })
  );
}

/** `next` with each part that draws the same as the one in its place in `before` kept as that object, so a tick of the
 * stream updates only the part that grew and leaves every other row and stretch of words alone. */
export function keepParts(before: Part[] | undefined, next: Part[]): Part[] {
  if (!before) return next;
  const kept = next.map((part, i) => (before[i] && samePart(before[i]!, part) ? before[i]! : part));
  return kept.length === before.length && kept.every((part, i) => part === before[i]) ? before : kept;
}
