import type { Area } from '../claude/args.ts';

const CHAT_INSTRUCTIONS =
  'You are a helpful, precise assistant in a chat app. Answer in the language of the user. Use Markdown when it helps.';

// Arguments for `codex app-server`. The user's own hooks, plugins, apps and MCP servers stay out, because Faber
// manages tools itself. Chat also loses the shell and every AGENTS.md, so it can never touch the computer.
// On Windows the elevated sandbox fails to start commands in Codex 0.161 ("apply deny-read ACLs"),
// so Faber uses the unelevated one.
export function codexArgs(area: Area, platform: NodeJS.Platform): string[] {
  const args = ['app-server'];
  for (const feature of ['hooks', 'apps', 'plugins', 'computer_use', 'browser_use']) args.push('--disable', feature);
  args.push('-c', 'mcp_servers={}');
  if (platform === 'win32') args.push('-c', 'windows.sandbox="unelevated"');
  if (area === 'chat') args.push('--disable', 'shell_tool', '-c', 'project_doc_max_bytes=0');
  return args;
}

// Parameters for thread/start and thread/resume.
export function codexThread(area: Area, model?: string): Record<string, unknown> {
  const base = model ? { model } : {};
  return area === 'chat'
    ? { ...base, sandbox: 'read-only', approvalPolicy: 'never', baseInstructions: CHAT_INSTRUCTIONS }
    : { ...base, sandbox: 'workspace-write', approvalPolicy: 'on-request' };
}
