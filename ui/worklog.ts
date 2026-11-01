import type { Change, ChangedLine, Message, ToolUse } from './api.ts';
import { lang, t } from './i18n.ts';
import { readsIn } from './language.ts';
import type { Key } from './locales/en.ts';

export type { Change, ChangedLine };

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
const BOILERPLATE = /^(?:\s*(?:\[Console\]::OutputEncoding\s*=\s*[^;]+|\$OutputEncoding\s*=\s*[^;]+|chcp(?:\.com)?\s+\d+(?:\s*>\s*\$null|\s*>\s*NUL)?|\$env:(?:RUSTUP_HOME|CARGO_HOME|PATH)\s*=\s*(?:'[^']*'|"[^"]*"|[^\s;]+)|export\s+(?:RUSTUP_HOME|CARGO_HOME|PATH)=(?:'[^']*'|"[^"]*"|[^\s;]+))\s*(?:;|&&)\s*)+/i;

function stripBoilerplate(cmd: string): string {
  const stripped = cmd.replace(BOILERPLATE, '').trim();
  return stripped || cmd;
}
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
  // The larger-version offer is a question to the person, shown as the card it draws.
  OfferBigger: 'tool.ask',
  Skill: 'tool.skill',
  ToolSearch: 'tool.toolSearch',
};
/** Claude Code's call that starts a subagent; it reads as the few words it was given. */
const SUBAGENT = new Set(['Agent', 'Task']);
/** The words of a step that ended, where they differ from the words of one at work. */
const PAST: Partial<Record<Key, Key>> = {
  'tool.command': 'step.command',
  'tool.fileChange': 'step.fileChange',
  'tool.webSearch': 'step.webSearch',
  'tool.mcp': 'step.mcp',
  'tool.tool': 'step.mcp',
  'tool.read': 'step.read',
  'tool.write': 'step.write',
  'tool.edit': 'step.edit',
  'tool.glob': 'step.glob',
  'tool.grep': 'step.grep',
  'tool.webFetch': 'step.read',
  'tool.browse': 'step.browse',
  'tool.skill': 'step.skill',
  'tool.toolSearch': 'step.toolSearch',
  'tool.plan': 'step.plan',
  'tool.verify': 'step.verify',
  'tool.verifyFull': 'step.verifyFull',
  'tool.map': 'step.map',
  'tool.memorySearch': 'step.memorySearch',
  'tool.catalog': 'step.catalog',
  'tool.quota': 'step.quota',
};

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

export function toolRow(tool: ToolUse, done = false): ToolRow {
  // A step that ended reads in the past ("Read", "Ran"), one still at work in the present ("Reading", "Running").
  const say = (key: Key) => t(done && PAST[key] ? PAST[key]! : key);
  if (tool.name === 'Settings') {
    const value = (tool.input && typeof tool.input === 'object' ? tool.input as Record<string, unknown> : {}).web_search;
    return { label: say('tool.settingsWebSearch'), detail: value === 'off' || value === 'cached' || value === 'live' ? say(`set.webSearch.${value}`) : '' };
  }
  const input = tool.input as { description?: unknown; title?: unknown; id?: unknown; level?: unknown; kind?: unknown; query?: unknown; install?: unknown; set_enabled?: unknown; remove?: unknown } | undefined;
  // Faber's checks (R190) read as what they run, with nothing after: the folder is the conversation's own.
  if (tool.name === 'Verify') return { label: say(input?.level === 'full' ? 'tool.verifyFull' : 'tool.verify'), detail: '' };
  // The project map (R146) reads as reading it, with what was looked up.
  if (tool.name === 'Map') return { label: say('tool.map'), detail: [input?.kind, input?.query].filter((v) => typeof v === 'string' && v).join(' ') };
  // Faber's two read-only lookups: the limits of the AIs (nothing after) and a catalog search (with its words).
  if (tool.name === 'CreativeJudge') return { label: say('tool.creativeJudge'), detail: '' };
  if (tool.name === 'Quota') return { label: say('tool.quota'), detail: '' };
  // A search of the project's own past conversations and notes reads as that, with its words.
  if (tool.name === 'MemorySearch') return { label: say('tool.memorySearch'), detail: typeof input?.query === 'string' ? input.query : '' };
  // With an id to install it reads as installing, with the id; the person's card comes before anything is installed.
  if (tool.name === 'Catalog' && typeof input?.remove === 'string') return { label: say('tool.catalogRemove'), detail: input.remove };
  if (tool.name === 'Catalog' && typeof input?.install === 'string') return { label: say('tool.catalogInstall'), detail: input.install };
  if (tool.name === 'Catalog' && input?.set_enabled && typeof input.set_enabled === 'object') {
    const change = input.set_enabled as Record<string, unknown>;
    return { label: t(change.enabled === true ? 'tool.catalogEnable' : 'tool.catalogDisable'), detail: typeof change.id === 'string' ? change.id : '' };
  }
  if (tool.name === 'Catalog') return { label: say('tool.catalog'), detail: typeof input?.query === 'string' ? input.query : '' };
  // Its own context, and compacting or clearing it, read as what they are, with nothing after.
  const action = tool.name === 'Conversation' ? (tool.input as { action?: unknown } | undefined)?.action : undefined;
  if (action === 'compact' || action === 'clear' || action === 'context') return { label: say(`tool.${action}`), detail: '' };
  // A conversation the agent starts reads as its title; one it reads back, as its id.
  const named = SUBAGENT.has(tool.name) ? input?.description : tool.name === 'Helper' ? input?.title : tool.name === 'Conversation' ? (input?.title ?? input?.id) : undefined;
  // A helper's few words show only in the person's language (task 130); its own view keeps them.
  if (isAgentCall(tool) && !(typeof named === 'string' && readsIn(named, lang()))) return { label: say(KINDS[tool.name]!), detail: '' };
  const raw = typeof named === 'string' ? named : detailOf(tool.input);
  const camel = /^[a-z]+[A-Z]/.test(tool.name);
  // An MCP tool (`mcp__server__tool`) reads as using a tool, with the server's name when the call names nothing else.
  const mcp = /^mcp__(.+?)__/.exec(tool.name);
  const label = KINDS[tool.name] ? say(KINDS[tool.name]!) : mcp ? say('tool.mcp') : camel ? say('tool.tool') : tool.name;
  if (mcp && !raw) return { label, detail: mcp[1]! };
  const wrapped = KINDS[tool.name] === 'tool.command' && WRAPPED.exec(raw);
  if (wrapped) {
    const exe = (wrapped[1] ?? wrapped[2] ?? '').split(/[\\/]+/).pop()!.replace(/\.exe$/i, '').toLowerCase();
    const shell = exe === 'sh' ? say('tool.shell') : SHELLS[exe];
    if (shell) {
      let rest = wrapped[3]!.trim();
      if (rest.length > 1 && rest.startsWith('"') && rest.endsWith('"')) rest = rest.slice(1, -1).replace(/\\"/g, '"');
      else if (rest.length > 1 && rest.startsWith("'") && rest.endsWith("'")) rest = rest.slice(1, -1).replace(/''/g, "'");
      return { label: shell, detail: stripBoilerplate(rest) };
    }
  }
  // The path of the program is the only part Codex escapes twice.
  const detail = raw.replace(/^"([^"]+)"/, (_, path: string) => `"${path.replace(/\\\\/g, '\\')}"`);
  return { label, detail: KINDS[tool.name] === 'tool.command' ? stripBoilerplate(detail) : detail };
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

export const isAgentCall = (tool: ToolUse): boolean => SUBAGENT.has(tool.name) || tool.name === 'Helper';

export interface AgentStep {
  call: ToolUse;
  /** The task that handed it work, when that task is in this reply. */
  parent?: { id: string; title: string };
  /** Its name on screen: the few words it was given when they read in the person's language, else "Helper 1", "Helper 2"…
   * by its place among the reply's helpers (task 130). Faber never guesses a translation. */
  title: string;
  /** The few words the CLI or the agent gave it, as they came; empty when none. Its view shows them when `title` differs. */
  named: string;
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
    if (step.call.interrupted) return 'stopped';
    if (!streaming) return step.background ? 'background' : 'stopped';
    if (sign) return sign === 'waiting' ? 'waiting' : 'stuck';
    return step.background ? 'background' : 'running';
  }
  if (declined) return 'declined';
  return ok ? 'done' : 'failed';
}

/** The word for where a helper is. "Done" is kept for one whose checks all passed; one that only ended reads "finished",
 * since its call returning proves nothing about its work (task 130). */
export function agentWord(step: AgentStep, streaming: boolean): Key {
  const state = agentState(step, streaming);
  const facts = step.call.facts;
  return state === 'done' && facts?.checks && !facts.failed ? 'agent.checked' : `agent.${state}`;
}

function agentBackground(tool: ToolUse, byId: Map<string, ToolUse>): boolean {
  for (let at: ToolUse | undefined = tool, hops = 0; at && hops <= byId.size; at = at.parent ? byId.get(at.parent) : undefined, hops++) {
    if ((at.input as { run_in_background?: unknown } | undefined)?.run_in_background === true) return true;
  }
  return false;
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
  const open = calls.filter((c) => c.tool.ok === undefined && !c.tool.interrupted);
  if (!open.length) return 0;
  const byId = new Map(calls.map((c) => [c.tool.id, c.tool]));
  return open.filter((c) => c.streaming || agentBackground(c.tool, byId)).length;
}

/** Whether two lists hold the same tools in the same state, so a message that only grew in text leaves the rows of its tools
 * alone while it streams. A tool is the same while its id, name, outcome, sign and output length are. */
export function sameTools(a: ToolUse[], b: ToolUse[]): boolean {
  return a.length === b.length && a.every((tool, i) => tool.id === b[i]!.id && tool.name === b[i]!.name && tool.ok === b[i]!.ok && tool.sign === b[i]!.sign && tool.interrupted === b[i]!.interrupted && tool.wordsChars === b[i]!.wordsChars && tool.transcript?.pages === b[i]!.transcript?.pages && tool.transcript?.failed === b[i]!.transcript?.failed && (tool.output?.length ?? 0) === (b[i]!.output?.length ?? 0));
}

/** Each helper's place among the agent calls of one reply, from 1: what "Helper 1" counts. A view that reads the steps of
 * several replies passes the numbers of the reply the helper belongs to, so it has the same name everywhere. */
export function helperNumbers(tools: ToolUse[] = []): Map<string, number> {
  return new Map(tools.filter(isAgentCall).map((call, i) => [call.id, i + 1]));
}

/** Each subagent, including nested ones, with its own tools. The parent keeps the handoff, the child keeps its work. */
export function agentSteps(tools: ToolUse[] = [], helperKind = '', numbers = helperNumbers(tools)): AgentStep[] {
  const text = (v: unknown) => (typeof v === 'string' ? v : '');
  const byId = new Map(tools.map((tool) => [tool.id, tool]));
  const parentOf = (tool: ToolUse) => {
    let at = tool.parent ? byId.get(tool.parent) : undefined;
    for (let hops = 0; at && hops < tools.length; hops++) {
      if (at.id === tool.id) return undefined;
      if (isAgentCall(at)) return at;
      at = at.parent ? byId.get(at.parent) : undefined;
    }
    return undefined;
  };
  const namedOf = (call: ToolUse) => {
    const input = (call.input ?? {}) as Record<string, unknown>;
    return text(call.name === 'Helper' ? input.title : input.description).trim();
  };
  const titleOf = (call: ToolUse) => {
    const named = namedOf(call);
    return named && readsIn(named, lang()) ? named : t('agent.numbered', { n: numbers.get(call.id) ?? 1 });
  };
  const own = new Map<string, ToolUse[]>();
  for (const tool of tools) {
    const parent = parentOf(tool);
    if (!parent) continue;
    const group = own.get(parent.id) ?? [];
    group.push(tool);
    own.set(parent.id, group);
  }
  return tools.filter(isAgentCall).map((call) => {
    const input = (call.input ?? {}) as Record<string, unknown>;
    const helper = call.name === 'Helper';
    const parent = parentOf(call);
    return {
      call,
      parent: parent ? { id: parent.id, title: titleOf(parent) } : undefined,
      title: titleOf(call),
      named: namedOf(call),
      kind: helper ? helperKind : text(input.subagent_type),
      helper,
      brief: text(helper ? input.brief : input.prompt).trim(),
      tools: own.get(call.id) ?? [],
      report: agentReport(call.output),
      background: agentBackground(call, byId),
    };
  });
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
 * edit that changed nothing, has none. The engine's own reading comes first; reading the input is for older conversations. */
export function changeOf(tool: ToolUse): Change | undefined {
  if (tool.change) return tool.change;
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

/** What a kind of step is called on its row's icon: the kind it acts on, one per family of tools. */
export type StepIcon = 'read' | 'change' | 'command' | 'search' | 'web' | 'tool' | 'thought';
export function stepIcon(tool: ToolUse): StepIcon {
  const kind = GROUPED[tool.name];
  if (kind === 'page' || kind === 'web' || tool.name === 'Browser') return 'web';
  if (kind) return kind;
  return 'tool';
}

/** A run of steps that ended, as one short line: a phrase with a count per kind, in the order the kinds first came ("Read 3
 * files · Ran 2 commands"), and the rest counted as other steps. Empty when none ended. */
export function stepsSummary(tools: ToolUse[]): string {
  const ended = tools.filter((tool) => tool.ok !== undefined || tool.interrupted);
  const kinds: GroupKind[] = [];
  const byKind = new Map<GroupKind, ToolUse[]>();
  let others = 0;
  for (const tool of ended) {
    const kind = GROUPED[tool.name];
    if (!kind) {
      others++;
      continue;
    }
    if (!byKind.has(kind)) {
      kinds.push(kind);
      byKind.set(kind, []);
    }
    byKind.get(kind)!.push(tool);
  }
  const phrases = kinds.map((kind) => t(`step.group.${kind}`, { n: countOf(kind, byKind.get(kind)!) }));
  if (others) phrases.push(t(phrases.length ? 'step.others' : 'step.only', { n: others }));
  return phrases.join(' · ');
}
