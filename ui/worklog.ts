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

const SHELLS: Record<string, string> = { powershell: 'PowerShell', pwsh: 'PowerShell', cmd: 'cmd', bash: 'Bash', sh: 'Shell', zsh: 'Zsh' };
const WRAPPED = /^\s*(?:"([^"]+)"|(\S+))\s+(?:-No\w+\s+)*(?:-Command|-c|-lc|\/c)\s+([\s\S]+)$/i;
// Codex names its internal item kinds in camelCase; the screen names them in words.
const KINDS: Record<string, Key> = {
  commandExecution: 'tool.command',
  fileChange: 'tool.fileChange',
  webSearch: 'tool.webSearch',
  mcpToolCall: 'tool.mcp',
};

export interface ToolRow {
  label: string;
  detail: string;
}

function detailOf(input: unknown): string {
  if (typeof input === 'string') return input;
  if (Array.isArray(input)) return input.map((c) => (c as { path?: string })?.path).filter(Boolean).join(', ');
  const o = input as Record<string, unknown> | undefined;
  const detail = o && (o.query ?? o.url ?? o.file_path ?? o.command ?? o.pattern);
  return typeof detail === 'string' ? detail : '';
}

export function toolRow(tool: ToolUse): ToolRow {
  const raw = detailOf(tool.input);
  const camel = /^[a-z]+[A-Z]/.test(tool.name);
  const label = KINDS[tool.name] ? t(KINDS[tool.name]!) : camel ? t('tool.tool') : tool.name;
  const wrapped = KINDS[tool.name] === 'tool.command' && WRAPPED.exec(raw);
  if (wrapped) {
    const exe = (wrapped[1] ?? wrapped[2] ?? '').split(/[\\/]+/).pop()!.replace(/\.exe$/i, '').toLowerCase();
    const shell = SHELLS[exe];
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
