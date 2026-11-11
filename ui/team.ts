import type { TeamMessage } from './api.ts';
import { agentState, type AgentStep } from './worklog.ts';
import { lang } from './i18n.ts';
import { html } from './markdown.tsx';

export interface WireLine { message: TeamMessage; replies: TeamMessage[] }

/** Use the reply's safe Markdown reader, then keep only the first sentence as text. */
export function teamOutcome(report: string): string {
  if (!report.trim()) return '';
  const template = document.createElement('template');
  template.innerHTML = html(report);
  template.content.querySelectorAll('button').forEach((button) => button.remove());
  template.content.querySelectorAll('p, li, pre, h1, h2, h3, h4, br').forEach((block) => block.append(' '));
  const text = template.content.textContent!.replace(/\s+/g, ' ').trim();
  return new Intl.Segmenter(lang(), { granularity: 'sentence' }).segment(text)[Symbol.iterator]().next().value?.segment.trim() ?? '';
}

/** Tells are mirrored on two calls. Replies belong under their question, including timestamp ties. */
export function teamWire(members: AgentStep[]): WireLine[] {
  const messages = [...new Map(members.flatMap((step) => step.call.team?.messages ?? []).map((message) => [message.id, message])).values()]
    .sort((a, b) => a.at - b.at);
  const asks = new Set(messages.filter((message) => message.kind === 'ask').map((message) => message.question ?? message.id));
  return messages.filter((message) => !((message.kind === 'answer' || message.kind === 'noAnswer') && message.question && asks.has(message.question)))
    .map((message) => ({ message, replies: message.kind === 'ask' ? messages.filter((reply) => (reply.kind === 'answer' || reply.kind === 'noAnswer') && reply.question === (message.question ?? message.id)) : [] }));
}

export function teamCounts(members: AgentStep[], streaming: boolean) {
  const states = members.map((step) => agentState(step, streaming));
  const working = states.filter((state) => state === 'running' || state === 'background' || state === 'stuck' || state === 'waiting').length;
  const waiting = states.filter((state) => state === 'waitingLead').length;
  return { working, waiting, done: members.length - working - waiting, ended: !streaming && !working && !waiting };
}

export function teamTime(members: AgentStep[]): { start: number; end: number } | undefined {
  const starts = members.flatMap((step) => step.call.started === undefined ? [] : [step.call.started]);
  const ends = members.flatMap((step) => step.call.ended === undefined ? [] : [step.call.ended]);
  return starts.length && ends.length ? { start: Math.min(...starts), end: Math.max(...ends) } : undefined;
}
