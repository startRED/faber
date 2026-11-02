import type { Message, RunningItem } from './api.ts';
import { workStatus } from './background.ts';
import { quotaPending } from './QuotaCard.tsx';
import { agentState, agentSteps, groupSteps, isAgentCall, isProgressCall, stepLine, type AgentState } from './worklog.ts';

// The one answer to "what is happening now" for a reply (task live-work-state). The foot line, the work log's summary, the
// latest-step lines, the run block and the away line each decided it for themselves and disagreed; the screen reads it here.
// Pure: no rendering, no timer, and no sentence of its own (the words are the caller's).

export type LiveKind = 'needs' | 'failed' | 'working' | 'done';

export interface LiveHelper {
  id: string;
  title: string;
  state: AgentState;
  since?: number;
}

export interface LiveWork {
  kind: LiveKind;
  /** The id of the reply the state belongs to: a state is never read from another turn. */
  answer: string;
  /** When the reply began, for "working for 15 min" and "worked for 15 min". */
  since?: number;
  /** The newest running step in the person's words; absent when no plain description is known, and for any shell line. */
  what?: string;
  /** Its helpers and subagents, oldest start first. */
  helpers: LiveHelper[];
}

export interface LiveInput {
  messages: Message[];
  /** The reply to read, by index; the newest reply when absent. */
  at?: number;
  /** "Keep going" waits to send the next turn. */
  keepWait?: RunningItem['keepWait'];
}

const COMMAND = /bash|command|exec|shell/i;

/** The state of one reply, or undefined when there is no reply. Highest first: the person is needed > it failed or stopped >
 * it works > it is done. */
export function liveWork(input: LiveInput): LiveWork | undefined {
  const at = input.at ?? input.messages.findLastIndex((m) => m.role === 'assistant');
  const m = input.messages[at];
  if (!m || m.role !== 'assistant') return undefined;
  const tools = m.tools ?? [];
  const streaming = m.status === 'streaming';
  const ended = m.status === 'stopped' || m.status === 'error';
  // A stop or an error ends its helpers too, even one that ran in the background (as `helpersAtWork` counts them).
  const helpers = agentSteps(tools)
    .map((step) => ({ id: step.call.id, title: step.title, state: ended && step.call.ok === undefined ? 'stopped' as const : agentState(step, streaming), since: step.call.started }))
    .sort((a, b) => (a.since ?? Infinity) - (b.since ?? Infinity));
  const base = { answer: m.id, since: m.at, helpers };
  // Only the newest reply carries what is still going on around it: an older one is what it was.
  const newest = at === input.messages.findLastIndex((x) => x.role === 'assistant');
  const follow = m.follow && workStatus({ follow: m.follow }).state;
  const asks =
    !!m.permissions?.length ||
    follow === 'waiting' ||
    !!m.needsYou ||
    m.checks?.state === 'needsYou' ||
    helpers.some((h) => h.state === 'waiting') ||
    (newest && input.keepWait?.why === 'question');
  if (asks) return { kind: 'needs', ...base };
  // A wait for quota is not a failure: the reply goes on when the limit resets.
  const quota = newest && quotaPending(m);
  if ((m.status === 'error' && !quota) || m.status === 'stopped' || m.checks?.state === 'failed' || follow === 'failed') return { kind: 'failed', ...base };
  const agentWorks = streaming && m.finishedAt === undefined;
  const faberWorks = follow === 'running' || (newest && (quota || m.checks?.state === 'running' || !!input.keepWait));
  if (!agentWorks && !faberWorks) return { kind: 'done', ...base };
  return { kind: 'working', ...base, what: agentWorks ? whatOf(m) : undefined };
}

/** What the agent said it is doing, else its newest open step of its own, unless that step is a command. */
function whatOf(m: Message): string | undefined {
  if (m.now?.trim()) return m.now.trim();
  const own = (m.tools ?? []).filter((tool) => !tool.parent && !isAgentCall(tool) && !isProgressCall(tool));
  const open = own.findLast((tool) => tool.ok === undefined);
  // A command, by its tool's name or by what it was given (a tool server's terminal), never shows as words.
  const input = open?.input as { command?: unknown; cmd?: unknown } | null | undefined;
  if (!open || COMMAND.test(open.name) || input?.command !== undefined || input?.cmd !== undefined) return undefined;
  return stepLine(groupSteps(own), open);
}
