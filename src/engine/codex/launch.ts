import type { Area } from '../claude/args.ts';

const CHAT_INSTRUCTIONS =
  'You are a helpful, precise assistant in a chat app. Answer in the language of the user. Use Markdown when it helps. ' +
  'Your working folder is a private workspace for this conversation: run code and create files there when it helps, ' +
  'and mention the files you create.';

// Arguments for `codex app-server`. Projects keep the user's own setup, like a direct session, except hooks.
// Chat stays lean (no plugins, apps, MCP servers or AGENTS.md) and works inside its own workspace folder.
// On Windows the elevated sandbox fails to start commands in Codex 0.161 ("apply deny-read ACLs"),
// so Faber uses the unelevated one.
export function codexArgs(area: Area, platform: NodeJS.Platform, userHooks = false): string[] {
  const args = ['app-server'];
  if (!userHooks) args.push('--disable', 'hooks');
  if (platform === 'win32') args.push('-c', 'windows.sandbox="unelevated"');
  if (area === 'chat') {
    for (const feature of ['apps', 'plugins', 'computer_use', 'browser_use']) args.push('--disable', feature);
    args.push('-c', 'mcp_servers={}', '-c', 'project_doc_max_bytes=0');
  }
  return args;
}

// Parameters for thread/start and thread/resume. The sandbox keeps writes inside the working folder;
// anything beyond it comes back as an approval request.
export function codexThread(area: Area, model?: string): Record<string, unknown> {
  const base = { ...(model ? { model } : {}), sandbox: 'workspace-write', approvalPolicy: 'on-request' };
  return area === 'chat' ? { ...base, baseInstructions: CHAT_INSTRUCTIONS } : base;
}
