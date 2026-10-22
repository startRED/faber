import type { Message, ToolUse } from './api.ts';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';

// How a turn's work log reads on screen: reasoning summaries as separate short paragraphs and tool rows as a label plus
// the command itself. The engine keeps what the CLI sent; the tidying lives here so older conversations get it too.

export interface ThinkingPart {
  text: string;
  title: boolean;
}

/** Reasoning summaries arrive as `**Title**` blocks glued together; split them, drop the markers, mark the titles. */
export function thinkingParts(text: string): ThinkingPart[] {
  return text
    .replace(/\*\*\s*\*\*/g, '**\n\n**')
    .split(/\n{2,}/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const bare = part.replace(/\*\*/g, '').trim();
      return { text: bare, title: /^\*\*[^*]+\*\*$/.test(part) };
    })
    .filter((part) => part.text);
}

const SHELLS: Record<string, string> = { powershell: 'PowerShell', pwsh: 'PowerShell', cmd: 'cmd', bash: 'Bash', zsh: 'Zsh' };
const WRAPPED = /^\s*(?:"([^"]+)"|(\S+))\s+(?:-No\w+\s+)*(?:-Command|-c|-lc|\/c)\s+([\s\S]+)$/i;
// Every step reads as what it does, in plain words: Codex names its item kinds in camelCase and Claude Code names its
// tools, and both map to the same words. A tool missing here keeps its own name.
const KINDS: Record<string, Key> = {
  commandExecution: 'tool.command',
  fileChange: 'tool.fileChange',
  webSearch: 'tool.webSearch',
  mcpToolCall: 'tool.mcp',
  Bash: 'tool.command',
  PowerShell: 'tool.command',
  Read: 'tool.read',
  Write: 'tool.write',
  Edit: 'tool.edit',
  MultiEdit: 'tool.edit',
  NotebookEdit: 'tool.edit',
  Glob: 'tool.glob',
  Grep: 'tool.grep',
  WebSearch: 'tool.webSearch',
  WebFetch: 'tool.webFetch',
  Browser: 'tool.browse',
  Conversation: 'tool.conversation',
  Agent: 'tool.agent',
  Helper: 'tool.agent',
  Task: 'tool.agent',
  TodoWrite: 'tool.plan',
  AskUserQuestion: 'tool.ask',
  Skill: 'tool.skill',
  ToolSearch: 'tool.toolSearch',
};
/** Claude Code's call that starts a subagent; it reads as the few words it was given. */
const SUBAGENT = new Set(['Agent', 'Task']);

export interface ToolRow {
  label: string;
  detail: string;
}

function detailOf(input: unknown): string {
  if (typeof input === 'string') return input;
  if (Array.isArray(input)) return input.map((c) => (c as { path?: string })?.path).filter(Boolean).join(', ');
  const o = input as Record<string, unknown> | undefined;
  const detail = o && (o.query ?? o.url ?? o.target ?? o.file_path ?? o.notebook_path ?? o.command ?? o.pattern ?? o.skill ?? o.plan);
  return typeof detail === 'string' ? detail.replace(/^https?:\/\//, '') : '';
}

export function toolRow(tool: ToolUse): ToolRow {
  const input = tool.input as { description?: unknown; title?: unknown; id?: unknown; level?: unknown; kind?: unknown; query?: unknown } | undefined;
  // Faber's checks (R190) read as what they run, with nothing after: the folder is the conversation's own.
  if (tool.name === 'Verify') return { label: t(input?.level === 'full' ? 'tool.verifyFull' : 'tool.verify'), detail: '' };
  // The project map (R146) reads as reading it, with what was looked up.
  if (tool.name === 'Map') return { label: t('tool.map'), detail: [input?.kind, input?.query].filter((v) => typeof v === 'string' && v).join(' ') };
  // Faber's two read-only lookups: the limits of the AIs (nothing after) and a catalog search (with its words).
  if (tool.name === 'Quota') return { label: t('tool.quota'), detail: '' };
  if (tool.name === 'Catalog') return { label: t('tool.catalog'), detail: typeof input?.query === 'string' ? input.query : '' };
  // Its own context, and compacting or clearing it, read as what they are, with nothing after.
  const action = tool.name === 'Conversation' ? (tool.input as { action?: unknown } | undefined)?.action : undefined;
  if (action === 'compact' || action === 'clear' || action === 'context') return { label: t(`tool.${action}`), detail: '' };
  // A conversation the agent starts reads as its title; one it reads back, as its id.
  const named = SUBAGENT.has(tool.name) ? input?.description : tool.name === 'Helper' ? input?.title : tool.name === 'Conversation' ? (input?.title ?? input?.id) : undefined;
  const raw = typeof named === 'string' ? named : detailOf(tool.input);
  const camel = /^[a-z]+[A-Z]/.test(tool.name);
  // An MCP tool (`mcp__server__tool`) reads as using a tool, with the server's name when the call names nothing else.
  const mcp = /^mcp__(.+?)__/.exec(tool.name);
  const label = KINDS[tool.name] ? t(KINDS[tool.name]!) : mcp ? t('tool.mcp') : camel ? t('tool.tool') : tool.name;
  if (mcp && !raw) return { label, detail: mcp[1]! };
  const wrapped = KINDS[tool.name] === 'tool.command' && WRAPPED.exec(raw);
  if (wrapped) {
    const exe = (wrapped[1] ?? wrapped[2] ?? '').split(/[\\/]+/).pop()!.replace(/\.exe$/i, '').toLowerCase();
    const shell = exe === 'sh' ? t('tool.shell') : SHELLS[exe];
    if (shell) {
      let rest = wrapped[3]!.trim();
      if (rest.length > 1 && rest.startsWith('"') && rest.endsWith('"')) rest = rest.slice(1, -1).replace(/\\"/g, '"');
      return { label: shell, detail: rest };
    }
  }
  // The path of the program is the only part Codex escapes twice.
  return { label, detail: raw.replace(/^"([^"]+)"/, (_, path: string) => `"${path.replace(/\\\\/g, '\\')}"`) };
}

/** One line for a row or a status: newlines folded, the middle cut so the start and the end both stay readable. */
export function middle(text: string, max = 96): string {
  const line = text.replace(/\s+/g, ' ').trim();
  if (line.length <= max) return line;
  const head = Math.ceil((max - 1) * 0.6);
  return `${line.slice(0, head)}…${line.slice(line.length - (max - 1 - head))}`;
}

export const toolLine = (tool: ToolUse): string => {
  const row = toolRow(tool);
  return row.detail ? `${row.label} · ${middle(row.detail)}` : row.label;
};

// Steps of one kind in a row read as one phrase with a count ("Changing 3 files"), opening into the single steps. A tool not
// listed here (a tool server's, a plan, a question) is always its own step.
type GroupKind = 'change' | 'read' | 'page' | 'search' | 'web' | 'command';
const GROUPED: Record<string, GroupKind> = {
  Edit: 'change', MultiEdit: 'change', NotebookEdit: 'change', Write: 'change', fileChange: 'change',
  Read: 'read', WebFetch: 'page', Glob: 'search', Grep: 'search', WebSearch: 'web', webSearch: 'web',
  Bash: 'command', PowerShell: 'command', commandExecution: 'command',
};

/** One step, or two or more of one kind in a row (`kind` and `n`, how many things the phrase counts). */
export interface StepGroup {
  tools: ToolUse[];
  kind?: GroupKind;
  n: number;
}

/** The files a step names: Claude Code's `file_path`, Codex's list of changes or its changes keyed by path. */
function filesOf(tool: ToolUse): string[] {
  const input = tool.input as Record<string, unknown> | unknown[] | null | undefined;
  if (Array.isArray(input)) return input.map((c) => (c as { path?: unknown } | null)?.path).filter((p): p is string => typeof p === 'string');
  if (!input || typeof input !== 'object') return [];
  const one = input.file_path ?? input.notebook_path;
  return typeof one === 'string' ? [one] : tool.name === 'fileChange' ? Object.keys(input) : [];
}

/** What a phrase counts: files for file steps (the same file twice is one), steps for anything else. */
function countOf(kind: GroupKind, tools: ToolUse[]): number {
  if (kind !== 'change' && kind !== 'read') return tools.length;
  return new Set(tools.flatMap((tool) => { const files = filesOf(tool); return files.length ? files : [`#${tool.id}`]; })).size;
}

const groupPhrase = (kind: GroupKind, n: number) => t(`tool.group.${kind}`, { n });

/** The steps in order, a run of one kind that touches at least two things as one group. A run that counts only one thing
 * (three edits to one file) stays as the single steps. */
export function groupSteps(tools: ToolUse[]): StepGroup[] {
  const runs: ToolUse[][] = [];
  for (const tool of tools) {
    const last = runs.at(-1);
    if (GROUPED[tool.name] && last && GROUPED[last[0]!.name] === GROUPED[tool.name]) last.push(tool);
    else runs.push([tool]);
  }
  return runs.flatMap((run) => {
    const kind = GROUPED[run[0]!.name];
    const n = kind ? countOf(kind, run) : 0;
    return kind && run.length > 1 && n >= 2 ? [{ tools: run, kind, n }] : run.map((tool) => ({ tools: [tool], n: 1 }));
  });
}

/** What a group is called: its phrase with the count. */
export const groupLabel = (group: StepGroup): string => (group.kind ? groupPhrase(group.kind, group.n) : '');

/** The living sentence for a step: inside a group it reads the group so far and then the step ("Reading 3 files · a.ts"), so
 * a burst of reads does not flash by file name; anywhere else it is the step's own line. */
export function stepLine(groups: StepGroup[], tool: ToolUse): string {
  const group = groups.find((g) => g.kind && g.tools.some((s) => s.id === tool.id));
  if (!group) return toolLine(tool);
  const n = countOf(group.kind!, group.tools.slice(0, group.tools.findIndex((s) => s.id === tool.id) + 1));
  if (n < 2) return toolLine(tool);
  const { detail } = toolRow(tool);
  return detail ? `${groupPhrase(group.kind!, n)} · ${middle(detail)}` : groupPhrase(group.kind!, n);
}

/** A piece of work the agent handed on: a subagent of its own CLI (`Agent`, `Task`), or a helper from the other AI (`Helper`,
 * Faber's own tool, R161). */
/** A call of the plan, the now line or the closing box (task 017): drawn as themselves, never as a step. */
export const isProgressCall = (tool: ToolUse): boolean => tool.name === 'Plan' || tool.name === 'Status' || tool.name === 'WrapUp';

export const isAgentCall = (tool: ToolUse): boolean => !tool.parent && (SUBAGENT.has(tool.name) || tool.name === 'Helper');

export interface AgentStep {
  call: ToolUse;
  /** The few words the agent gave the piece of work, else the start of its brief. */
  title: string;
  /** The kind of subagent (`Explore`, `Plan`, a person's own agent), when the agent named one; for a helper, the AI that ran it. */
  kind: string;
  /** A helper from the other AI, not a subagent of the agent's own CLI. */
  helper: boolean;
  /** The whole brief the agent sent it, word for word; empty when the CLI gives none (Codex names its subagents only). */
  brief: string;
  /** What the subagent ran, in order. */
  tools: ToolUse[];
  report: string;
  /** Sent to the background: its call returned when it started, and it finishes later. */
  background: boolean;
}

// Claude Code frames a subagent's report as model output and indents every line of it (2026-10 recording).
const HANDBACK = /^\[Subagent hand-back\][\s\S]*?The report follows:\n/;

/** The subagent's report without the frame Claude Code puts around it, its lines back at their own indent. */
export function agentReport(output = ''): string {
  const frame = HANDBACK.exec(output);
  if (!frame) return output.trim();
  return output
    .slice(frame[0].length)
    .split('\n')
    .map((line) => line.replace(/^ {2}/, ''))
    .join('\n')
    .trim();
}

/** A kind of subagent as words: the CLI's general one is "General", and a person's own agent (`code-reviewer`) reads "Code reviewer". The
 * names Explore and Plan, and the AI that ran a helper, are words already. */
export function agentKind(kind: string): string {
  if (kind === 'general-purpose') return t('agent.kind.general');
  const words = kind
    .replace(/[-_]+/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Where a subagent is, in one word: still at work (in the background, waiting on the person, or seemingly stuck), or how it
 * ended. A call left open after its reply ended was stopped with it, unless it was sent to the background. */
export type AgentState = 'running' | 'background' | 'waiting' | 'stuck' | 'stopped' | 'declined' | 'done' | 'failed';
export function agentState(step: AgentStep, streaming: boolean): AgentState {
  const { ok, declined, sign } = step.call;
  if (ok === undefined) {
    if (!streaming) return step.background ? 'background' : 'stopped';
    if (sign) return sign === 'waiting' ? 'waiting' : 'stuck';
    return step.background ? 'background' : 'running';
  }
  if (declined) return 'declined';
  return ok ? 'done' : 'failed';
}

/** How many subagents and helpers are still at work, nested ones each counted: what a Stop would end with the turn. A reply
 * that ended (a message taken into the turn ends the one before it) still holds the background ones it sent and theirs;
 * the last Stop or error ended everything before it. */
export function helpersAtWork(messages: Message[] = []): number {
  const ended = messages.findLastIndex((m) => m.role === 'assistant' && (m.status === 'stopped' || m.status === 'error'));
  const agent = (tool: ToolUse) => SUBAGENT.has(tool.name) || tool.name === 'Helper';
  const replies = messages.slice(ended + 1).filter((m) => m.role === 'assistant');
  // Only agent calls: a long conversation's other steps are never walked. A subagent's own steps land in the reply under
  // way, so its parent may sit in an earlier one; a subagent's parent is an agent call.
  const calls = replies.flatMap((m) => (m.tools ?? []).filter(agent).map((tool) => ({ tool, streaming: m.status === 'streaming' })));
  const open = calls.filter((c) => c.tool.ok === undefined);
  if (!open.length) return 0;
  const byId = new Map(calls.map((c) => [c.tool.id, c.tool]));
  const background = (tool: ToolUse) => {
    for (let at: ToolUse | undefined = tool, hops = 0; at && hops <= byId.size; at = at.parent ? byId.get(at.parent) : undefined, hops++) {
      if ((at.input as { run_in_background?: unknown } | undefined)?.run_in_background === true) return true;
    }
    return false;
  };
  return open.filter((c) => c.streaming || background(c.tool)).length;
}

/** Whether two lists hold the same tools in the same state, so a message that only grew in text leaves the rows of its tools
 * alone while it streams. A tool is the same while its id, name, outcome, sign and output length are. */
export function sameTools(a: ToolUse[], b: ToolUse[]): boolean {
  return a.length === b.length && a.every((tool, i) => tool.id === b[i]!.id && tool.name === b[i]!.name && tool.ok === b[i]!.ok && tool.sign === b[i]!.sign && (tool.output?.length ?? 0) === (b[i]!.output?.length ?? 0));
}

/** Each subagent the agent started, with the tools it ran under it. A subagent's own helper folds into it: its tools count
 * for the agent call the whole chain started from. */
export function agentSteps(tools: ToolUse[] = [], helperKind = ''): AgentStep[] {
  const text = (v: unknown) => (typeof v === 'string' ? v : '');
  const parents = new Map(tools.map((tool) => [tool.id, tool.parent]));
  const root = (tool: ToolUse) => {
    let id = tool.parent;
    for (let hops = 0; id && parents.get(id) && hops < tools.length; hops++) id = parents.get(id);
    return id;
  };
  return tools.filter(isAgentCall).map((call) => {
    const input = (call.input ?? {}) as Record<string, unknown>;
    const helper = call.name === 'Helper';
    return {
      call,
      title: text(helper ? input.title : input.description).trim() || middle(text(helper ? input.brief : input.prompt), 80),
      kind: helper ? helperKind : text(input.subagent_type),
      helper,
      brief: text(helper ? input.brief : input.prompt).trim(),
      tools: tools.filter((tool) => tool.parent && root(tool) === call.id),
      report: agentReport(call.output),
      background: input.run_in_background === true,
    };
  });
}

/** A line an edit step added or removed. */
export interface ChangedLine {
  added: boolean;
  text: string;
}
/** The sites a reply opened, the one it opened last at the end, and the one it reads now (R161 step 9). Only a page read
 * names a site: a search names a query, and Codex reports its searches only. */
export interface Sources {
  hosts: string[];
  now?: string;
}

function hostOf(url: unknown): string {
  if (typeof url !== 'string') return '';
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

export function sourcesOf(tools: ToolUse[]): Sources {
  const hosts: string[] = [];
  let now: string | undefined;
  for (const tool of tools) {
    if (tool.name !== 'WebFetch' || tool.declined) continue;
    const host = hostOf((tool.input as { url?: unknown } | undefined)?.url);
    if (!host) continue;
    // Opened again, it moves to the end, so the one it reads now is always in view.
    const at = hosts.indexOf(host);
    if (at >= 0) hosts.splice(at, 1);
    hosts.push(host);
    if (tool.ok === undefined) now = host;
  }
  // Still loading while later ones ended, it moves to the end too, so it never falls out of the four in view.
  if (now) hosts.push(...hosts.splice(hosts.indexOf(now), 1));
  return { hosts, now };
}

/** What an edit step changed, read from its own input with no model call (R161 step 9): how many lines it added and
 * removed, and the changed lines themselves. */
export interface Change {
  added: number;
  removed: number;
  lines: ChangedLine[];
}

const lines = (text: string) => (text ? text.replace(/\r\n/g, '\n').replace(/\n$/, '').split('\n') : []);

/** Past this many lines on a side, a replacement is compared only at its ends: the longest common run is quadratic. */
const COMPARED = 400;

/** One replacement as the lines it changed: a line both sides keep, in order, is not a change. */
function replaced(before: string, after: string): ChangedLine[] {
  const a = lines(before);
  const b = lines(after);
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let end = 0;
  while (end < a.length - start && end < b.length - start && a[a.length - 1 - end] === b[b.length - 1 - end]) end++;
  const x = a.slice(start, a.length - end);
  const y = b.slice(start, b.length - end);
  const gone = (text: string) => ({ added: false, text });
  const came = (text: string) => ({ added: true, text });
  if (x.length > COMPARED || y.length > COMPARED) return [...x.map(gone), ...y.map(came)];
  // keep[i][j]: how many lines x[i..] and y[j..] keep in common.
  const keep = Array.from({ length: x.length + 1 }, () => Array.from<number>({ length: y.length + 1 }).fill(0));
  for (let i = x.length - 1; i >= 0; i--)
    for (let j = y.length - 1; j >= 0; j--) keep[i]![j] = x[i] === y[j] ? keep[i + 1]![j + 1]! + 1 : Math.max(keep[i + 1]![j]!, keep[i]![j + 1]!);
  const out: ChangedLine[] = [];
  let i = 0;
  let j = 0;
  while (i < x.length || j < y.length) {
    if (i < x.length && j < y.length && x[i] === y[j]) {
      i++;
      j++;
    } else if (j < y.length && (i === x.length || keep[i]![j + 1]! >= keep[i + 1]![j]!)) out.push(came(y[j++]!));
    else out.push(gone(x[i++]!));
  }
  // Removed lines read before the lines that replace them, as in a diff.
  return [...out.filter((line) => !line.added), ...out.filter((line) => line.added)];
}

/** A unified diff's own lines. Its file headers (`---`, `+++`) come before the first hunk; inside a hunk every `+` or `-`
 * line is content, even one that starts with three of them. */
function diffed(diff: string): ChangedLine[] {
  const all = lines(diff);
  let inHunk = !all.some((line) => line.startsWith('@@'));
  const out: ChangedLine[] = [];
  for (const line of all) {
    if (line.startsWith('@@')) inHunk = true;
    else if (inHunk && /^[+-]/.test(line)) out.push({ added: line[0] === '+', text: line.slice(1) });
  }
  return out;
}

/** One of Codex's file changes: a list item with its `diff`, or a value keyed by its path that holds a unified diff or,
 * for a new file, its content, directly or under its kind (`{ update: { unified_diff } }`, `{ add: { content } }`). */
function codexChange(change: unknown): ChangedLine[] {
  const o = change as Record<string, unknown> | undefined;
  if (!o || typeof o !== 'object') return [];
  const diff = o.diff ?? o.unified_diff;
  if (typeof diff === 'string') return diffed(diff);
  if (typeof o.content === 'string') return lines(o.content).map((text) => ({ added: true, text }));
  return Object.values(o).flatMap((inner) => (inner && typeof inner === 'object' && !Array.isArray(inner) ? codexChange(inner) : []));
}

/** The lines an edit step changed: Claude Code's Edit, MultiEdit and Write, and Codex's file changes. Anything else, or an
 * edit that changed nothing, has none. */
export function changeOf(tool: ToolUse): Change | undefined {
  const text = (v: unknown) => (typeof v === 'string' ? v : '');
  const input = tool.input as Record<string, unknown> | unknown[] | undefined;
  let changed: ChangedLine[] = [];
  if (tool.name === 'Edit' && input && !Array.isArray(input)) changed = replaced(text(input.old_string), text(input.new_string));
  else if (tool.name === 'MultiEdit' && input && !Array.isArray(input) && Array.isArray(input.edits))
    changed = (input.edits as Record<string, unknown>[]).flatMap((edit) => replaced(text(edit?.old_string), text(edit?.new_string)));
  else if (tool.name === 'Write' && input && !Array.isArray(input)) changed = lines(text(input.content)).map((line) => ({ added: true, text: line }));
  else if (tool.name === 'fileChange' && input) changed = (Array.isArray(input) ? input : Object.values(input)).flatMap(codexChange);
  if (!changed.length) return undefined;
  const added = changed.filter((line) => line.added).length;
  return { added, removed: changed.length - added, lines: changed };
}
