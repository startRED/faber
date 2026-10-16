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
/** Claude Code's own subagent step: the calls it makes carry this step's id as their `parent`. */
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

/** A step of the reply, with every call its subagent made when it is one. */
export interface Step {
  tool: ToolUse;
  children: ToolUse[];
}

/** The reply's own steps in order. A subagent's calls, and those of any subagent it started, go under its step; a call
 * whose step is not in the reply stays a step of its own, so nothing is ever hidden. */
export function stepsOf(tools: ToolUse[] = []): Step[] {
  const byId = new Map(tools.map((tool) => [tool.id, tool]));
  const top = (tool: ToolUse): ToolUse => {
    let step = tool;
    for (let hops = 0; step.parent && byId.has(step.parent) && hops < 16; hops++) step = byId.get(step.parent)!;
    return step;
  };
  const steps = new Map<string, Step>();
  for (const tool of tools) {
    const root = top(tool);
    if (root === tool) steps.set(tool.id, { tool, children: [] });
    else steps.get(root.id)?.children.push(tool);
  }
  return [...steps.values()];
}

export const isSubagent = (step: Step): boolean => SUBAGENT.has(step.tool.name) || step.children.length > 0;

/** A subagent still at work: its step has not returned, or it was sent to the background (whose step returns at once)
 * and a call of its own is still running. */
export function atWork(step: Step): boolean {
  if (!isSubagent(step)) return false;
  if (step.tool.ok === undefined) return true;
  return (step.tool.input as { run_in_background?: unknown } | undefined)?.run_in_background === true && step.children.some((c) => c.ok === undefined);
}

/** What a subagent does now, in one line: its latest call, or that it is starting. */
export function nowOf(step: Step): string {
  const last = step.children.at(-1);
  return last ? toolLine(last) : t('tool.starting');
}

/** The name the agent gave a subagent step, or the plain word for one. */
export const nameOf = (step: Step): string => middle(toolRow(step.tool).detail, 48) || t('tool.agent');
