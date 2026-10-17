import type { ToolUse } from './api.ts';
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
  Agent: 'tool.agent',
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
  const detail = o && (o.query ?? o.url ?? o.file_path ?? o.notebook_path ?? o.command ?? o.pattern ?? o.skill);
  return typeof detail === 'string' ? detail.replace(/^https?:\/\//, '') : '';
}

export function toolRow(tool: ToolUse): ToolRow {
  const named = SUBAGENT.has(tool.name) && (tool.input as { description?: unknown } | undefined)?.description;
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

/** A piece of work the agent handed on: a subagent of its own CLI (`Agent`, `Task`), or a helper from the other AI (`Helper`,
 * Faber's own tool, R161). */
export const isAgentCall = (tool: ToolUse): boolean => !tool.parent && (SUBAGENT.has(tool.name) || tool.name === 'Helper');

export interface AgentStep {
  call: ToolUse;
  /** The few words the agent gave the piece of work, else the start of its brief. */
  title: string;
  /** The kind of subagent (`Explore`, `Plan`, a person's own agent), when the agent named one; for a helper, the AI that ran it. */
  kind: string;
  /** A helper from the other AI, not a subagent of the agent's own CLI. */
  helper: boolean;
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

/** Whether two lists hold the same tools in the same state, so a message that only grew in text leaves the rows of its tools
 * alone while it streams. A tool is the same while its id, name, outcome and output length are. */
export function sameTools(a: ToolUse[], b: ToolUse[]): boolean {
  return a.length === b.length && a.every((tool, i) => tool.id === b[i]!.id && tool.name === b[i]!.name && tool.ok === b[i]!.ok && (tool.output?.length ?? 0) === (b[i]!.output?.length ?? 0));
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
      tools: tools.filter((tool) => tool.parent && root(tool) === call.id),
      report: agentReport(call.output),
      background: input.run_in_background === true,
    };
  });
}
