export type Area = 'chat' | 'project';

export interface ClaudeLaunch {
  area: Area;
  model?: string;
  effort?: string;
  resume?: string;
  userHooks?: boolean;
}

// Chat stays lean (about 3k tokens of setup instead of the user's full configuration) and runs in --restricted mode
// inside its own workspace folder: file tools cannot leave it, edits there are accepted, and any command asks first.
const CHAT_TOOLS = 'Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch';

// Arguments for one long-lived Claude Code process that takes many turns over stdin.
// Projects get the user's own setup (skills, MCP servers, plugins), like a direct session; the user's hooks stay
// off unless they turn them on, because hooks can rewrite answers. Permission prompts come back to Faber over stdio.
export function claudeArgs({ area, model, effort, resume, userHooks }: ClaudeLaunch): string[] {
  const args = [
    '-p',
    '--input-format', 'stream-json',
    '--output-format', 'stream-json',
    '--verbose',
    '--include-partial-messages',
    '--permission-prompt-tool', 'stdio',
    '--exclude-dynamic-system-prompt-sections',
  ];
  if (area === 'chat') args.push('--restricted', '--strict-mcp-config', '--tools', CHAT_TOOLS, '--permission-mode', 'acceptEdits');
  else if (!userHooks) args.push('--settings', JSON.stringify({ disableAllHooks: true }));
  if (model) args.push('--model', model);
  if (effort) args.push('--effort', effort);
  if (resume) args.push('--resume', resume);
  return args;
}
