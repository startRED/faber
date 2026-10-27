import type { BackgroundCommand, Checks, Follow, Message, ToolUse } from './api.ts';
import { agentState, agentSteps, isAgentCall, toolRow, type AgentStep } from './worklog.ts';
import type { Key } from './locales/en.ts';
import { lang } from './i18n.ts';

export type WorkState = 'running' | 'passed' | 'failed' | 'waiting' | 'stuck' | 'unreported' | 'stopped';
export interface BackgroundWork {
  id: string;
  title: string;
  state: WorkState;
  messageId?: string;
  agent?: AgentStep;
  command?: BackgroundCommand;
  tool?: ToolUse;
  follow?: Follow;
  checks?: Checks;
  streaming?: boolean;
  interrupted?: boolean;
}

/** The list's words/tone and the thread's running count use the same observed-state rule. */
export function workStatus(row: Omit<BackgroundWork, 'id' | 'title' | 'state'>) {
  let state: WorkState;
  let key: Key | undefined;
  let reason: Key | undefined;
  let vars: Record<string, string> | undefined;
  if (row.follow) {
    const f = row.follow;
    const open = ['pending', 'failing', 'conflict'].includes(f.state);
    state = open && f.held ? 'waiting' : f.state === 'pending' ? 'running'
      : f.state === 'failing' || f.state === 'conflict' ? 'failed'
        : f.state === 'merged' || f.state === 'passing' ? 'passed' : 'stopped';
    key = `follow.${f.held && f.state === 'failing' ? 'held' : f.problem?.replace('follow.', '') ?? f.state}` as Key;
    vars = { pr: `#${f.number}`, checks: (f.failed ?? []).join(', ') };
  } else if (row.command) {
    state = row.command.state === 'done' ? 'passed' : row.command.state;
    key = `commands.${row.command.state}`;
  } else if (row.checks) {
    state = row.checks.state === 'needsYou' ? 'waiting' : row.checks.state;
  } else if (row.agent) {
    const agent = row.interrupted && row.agent.call.ok === undefined ? 'stopped' : agentState(row.agent, !!row.streaming);
    state = agent === 'done' ? 'passed' : agent === 'background' ? 'running' : agent === 'declined' ? 'stopped' : agent;
    key = `agent.${agent === 'declined' ? 'stopped' : agent}`;
    if (agent === 'stuck') reason = `agent.stuck.${row.agent.call.sign as 'repeat' | 'failing' | 'silent'}`;
  } else {
    state = row.tool?.ok === true ? 'passed' : row.tool?.ok === false ? 'failed' : row.interrupted ? 'stopped' : row.streaming ? 'running' : 'unreported';
  }
  return { state, key: key ?? `background.${state}` as Key, vars, reason,
    running: state === 'running', need: state === 'failed' || state === 'stuck' || state === 'waiting' };
}

export interface ToolIndex {
  length: number;
  first?: string;
  last?: string;
  byId: Map<string, number>;
  checks: number[];
  reviews: number[];
  background: number[];
  shells: Map<string, number[]>;
  projected?: Omit<BackgroundWork, 'state'>[];
  language?: string;
}

// Task 020's per-message classification: input/kind are fixed at launch; outcomes refresh only selected calls.
export function indexed(m: Message, cache: Map<string, ToolIndex>): ToolIndex {
  const tools = m.tools ?? [];
  const old = cache.get(m.id);
  if (old?.length === tools.length && old.first === tools[0]?.id && old.last === tools.at(-1)?.id) return old;
  const next: ToolIndex = { length: tools.length, first: tools[0]?.id, last: tools.at(-1)?.id, byId: new Map(), checks: [], reviews: [], background: [], shells: new Map() };
  tools.forEach((tool, i) => {
    next.byId.set(tool.id, i);
    if (tool.check || tool.name === 'Verify') next.checks.push(i);
    const agent = isAgentCall(tool);
    if (agent) next.reviews.push(i);
    const shell = ['Bash', 'PowerShell', 'commandExecution'].includes(tool.name);
    if (shell) {
      const command = toolRow(tool).detail;
      next.shells.set(command, [...(next.shells.get(command) ?? []), i]);
    }
    if (agent || (!tool.parent && (['Monitor', 'Verify'].includes(tool.name) || (shell && (tool.input as { run_in_background?: unknown } | undefined)?.run_in_background === true)))) next.background.push(i);
  });
  cache.set(m.id, next);
  return next;
}

export function prune(cache: Map<string, ToolIndex>, messages: Message[]) {
  const ids = new Set(messages.map((m) => m.id));
  for (const id of cache.keys()) if (!ids.has(id)) cache.delete(id);
}

export const createBackgroundCache = () => ({ tools: new Map<string, ToolIndex>(), commands: new Map<string, { title: string; sources: string[] }>(), lines: new Map<string, { text: string; urls: string; lines: string[] }>() });

export function backgroundCalls(m: Message, index: ToolIndex) {
  const old = index.projected;
  if (old && index.language === lang() && index.background.every((i, n) => {
    const call = m.tools![i]!, before = old[n]!.agent?.call ?? old[n]!.tool!;
    return call.ok === before.ok && call.declined === before.declined && call.sign === before.sign && call.interrupted === before.interrupted && call.output === before.output;
  })) return old;
  index.language = lang();
  return index.projected = index.background.map((i) => {
    const tool = { ...m.tools![i]! };
    const agent = isAgentCall(tool) ? agentSteps([tool])[0] : undefined;
    const row = toolRow(tool);
    return { id: `${m.id}:${tool.id}`, messageId: m.id, agent, tool: agent ? undefined : tool,
      title: agent?.title || (row.detail ? `${row.label} · ${row.detail}` : row.label) };
  });
}

/** One projection of existing parsers. The cache never reclassifies tools or shell descriptions on text ticks. */
export function backgroundOf(messages: Message[], commands: BackgroundCommand[], conversationId: string, cache = createBackgroundCache()): BackgroundWork[] {
  prune(cache.tools, messages);
  const rows: BackgroundWork[] = [];
  const latest = messages.findLast((m) => m.role === 'assistant');
  const stopped = messages.findLastIndex((m) => m.role === 'assistant' && (m.status === 'stopped' || m.status === 'error'));
  const sources = new Set<string>();
  const commandIds = new Set(commands.map((c) => c.id));
  for (const id of cache.commands.keys()) if (!commandIds.has(id)) cache.commands.delete(id);
  for (const command of commands.filter((c) => c.conversationId === conversationId).toReversed()) {
    let named = cache.commands.get(command.id);
    if (!named) {
      const calls = messages.filter((m) => m.role === 'assistant').flatMap((m) => (indexed(m, cache.tools).shells.get(command.command) ?? []).map((i) => ({ m, tool: m.tools![i]! })));
      const call = calls.length === 1 ? calls[0] : undefined;
      const description = (call?.tool.input as { description?: unknown } | undefined)?.description;
      named = { title: typeof description === 'string' ? description : command.command, sources: calls.map((call) => `${call.m.id}:${call.tool.id}`) };
      cache.commands.set(command.id, named);
    }
    for (const source of named.sources) sources.add(source);
    rows.push({ id: `command:${command.id}`, title: named.title, state: workStatus({ command }).state, command });
  }
  const seen = new Set<string>();
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i]!;
    if (m.role !== 'assistant') continue;
    const current = m.id === latest?.id && m.status === 'streaming';
    for (const item of backgroundCalls(m, indexed(m, cache.tools)).toReversed()) {
      if ((item.agent?.call ?? item.tool)?.declined || sources.has(item.id)) continue;
      const row = { ...item, streaming: m.status === 'streaming', interrupted: i <= stopped };
      const status = workStatus(row);
      if (current || status.running || ['waiting', 'stuck', 'unreported'].includes(status.state)) rows.push({ ...row, state: status.state });
    }
    if (m.checks && (current || (m.id === latest?.id && (m.checks.state === 'running' || m.checks.state === 'needsYou')))) {
      rows.push({ id: `${m.id}:checks`, title: '', state: workStatus({ checks: m.checks }).state, checks: m.checks });
    }
    if (m.follow) {
      const key = `${m.follow.repo.toLowerCase()}#${m.follow.number}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (['pending', 'failing', 'conflict'].includes(m.follow.state) || (current && !['merged', 'closed'].includes(m.follow.state))) {
        rows.push({ id: `${m.id}:follow`, title: `#${m.follow.number}`, state: workStatus({ follow: m.follow }).state, follow: m.follow });
      }
    }
  }
  return rows;
}

/** Newest known observation per repository and number, including completed follows. No new remote reader. */
export function followedIn(messages: Message[]): Follow[] {
  const seen = new Set<string>();
  return messages.toReversed().flatMap((m) => {
    if (!m.follow) return [];
    const key = `${m.follow.repo.toLowerCase()}#${m.follow.number}`;
    if (seen.has(key)) return [];
    seen.add(key);
    return [m.follow];
  });
}

export interface Reference { text: string; follow?: Follow }
// A URL ends on a word character or slash, so a sentence's closing punctuation stays outside the link.
const REFERENCES = /https?:\/\/[^\s<>()[\]`]*[^\s<>()[\]`.,;:!?'"]|(?:[\w.-]+\/[\w.-]+)?#\d+\b|\b[a-f\d]{7,40}\b/gi;

/** The followed pull requests a token names: its URL, number, owner/repo#number, or a head hash (prefix) or commit page. */
function known(token: string, follows: Follow[]): Follow[] {
  const lower = token.toLowerCase();
  return follows.filter((f) => {
    if (lower === f.url.toLowerCase() || lower === `${f.repo.toLowerCase()}#${f.number}` || token === `#${f.number}`) return true;
    const hash = lower.startsWith(`https://github.com/${f.repo.toLowerCase()}/commit/`) ? lower.split('/').at(-1)! : lower;
    return /^[a-f\d]{7,40}$/.test(hash) && !!f.head && f.head.toLowerCase().startsWith(hash);
  });
}

/** Numbers and hashes are candidates; a URL is one only when it names a followed pull request or its commit. */
export const hasReference = (text: string, follows: Follow[] = []): boolean =>
  text.match(REFERENCES)?.some((token) => !/^https?:/i.test(token) || known(token, follows).length > 0) ?? false;

/** A reply's reference lines, searched again only when its text or the followed pull requests changed, never on other ticks. */
export function referenceLines(replies: Message[], cache: Map<string, { text: string; urls: string; lines: string[] }>, follows: Follow[] = []): string[] {
  const ids = new Set(replies.map((m) => m.id));
  for (const id of cache.keys()) if (!ids.has(id)) cache.delete(id);
  const urls = follows.map((f) => `${f.url} ${f.head ?? ''}`).join(' ');
  return replies.flatMap((m) => {
    let hit = cache.get(m.id);
    if (hit?.text !== m.text || hit.urls !== urls) cache.set(m.id, hit = { text: m.text, urls, lines: m.text.split('\n').filter((line) => hasReference(line, follows)) });
    return hit.lines;
  });
}

/** Only unique, exact known references become links. Ambiguous bare numbers and hash prefixes remain literal. */
export function liveReferences(text: string, follows: Follow[]): Reference[] {
  const result: Reference[] = [];
  let end = 0;
  for (const match of text.matchAll(REFERENCES)) {
    if (match.index > end) result.push({ text: text.slice(end, match.index) });
    const matches = known(match[0], follows);
    result.push({ text: match[0], follow: matches.length === 1 ? matches[0] : undefined });
    end = match.index + match[0].length;
  }
  if (end < text.length) result.push({ text: text.slice(end) });
  return result;
}
