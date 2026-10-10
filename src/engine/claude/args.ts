export type Area = 'chat' | 'project';

export interface ClaudeLaunch {
  area: Area;
  model?: string;
  resume?: string;
}

// Arguments for one long-lived Claude Code process that takes many turns over stdin.
// Chat runs in --restricted mode with only web tools, so it can never change files on the computer.
// Projects load only the project's own settings: the user's global hooks and MCP servers stay out,
// because Faber manages tools itself. Permission prompts come back to Faber over stdio.
export function claudeArgs({ area, model, resume }: ClaudeLaunch): string[] {
  const args = [
    '-p',
    '--input-format', 'stream-json',
    '--output-format', 'stream-json',
    '--verbose',
    '--include-partial-messages',
    '--permission-prompt-tool', 'stdio',
    '--strict-mcp-config',
    '--exclude-dynamic-system-prompt-sections',
  ];
  if (area === 'chat') args.push('--restricted', '--tools', 'WebSearch,WebFetch');
  else args.push('--setting-sources', 'project,local');
  if (model) args.push('--model', model);
  if (resume) args.push('--resume', resume);
  return args;
}
