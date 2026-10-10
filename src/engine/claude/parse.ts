import type { EngineEvent } from '../events.ts';

// Claude Code stream-json is loosely typed JSON, so fields are read defensively.
type Json = Record<string, any>;

function blockText(content: unknown): string {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.map((c: Json) => (typeof c?.text === 'string' ? c.text : '')).join('');
}

// Turns one stream-json line from `claude -p --output-format stream-json --include-partial-messages`
// into engine events. Lines that carry nothing the app shows return [].
export function parseClaudeMessage(msg: Json): EngineEvent[] {
  switch (msg.type) {
    case 'system':
      return msg.subtype === 'init' ? [{ type: 'started', sessionId: msg.session_id, model: msg.model }] : [];
    case 'stream_event': {
      if (msg.event?.type === 'content_block_start' && msg.event.content_block?.type === 'text') return [{ type: 'break' }];
      const delta = msg.event?.type === 'content_block_delta' ? msg.event.delta : undefined;
      if (delta?.type === 'text_delta') return [{ type: 'text', delta: delta.text }];
      if (delta?.type === 'thinking_delta') return [{ type: 'thinking', delta: delta.thinking }];
      return [];
    }
    case 'assistant':
      // Text already arrived as deltas; only finished tool calls are new here.
      return (msg.message?.content ?? [])
        .filter((c: Json) => c.type === 'tool_use')
        .map((c: Json) => ({ type: 'tool', id: c.id, name: c.name, input: c.input }));
    case 'user':
      return (Array.isArray(msg.message?.content) ? msg.message.content : [])
        .filter((c: Json) => c.type === 'tool_result')
        .map((c: Json) => ({ type: 'tool-result', id: c.tool_use_id, ok: !c.is_error, text: blockText(c.content) }));
    case 'control_request':
      return msg.request?.subtype === 'can_use_tool'
        ? [{ type: 'permission', requestId: msg.request_id, tool: msg.request.tool_name, input: msg.request.input }]
        : [];
    case 'rate_limit_event':
      return Object.entries(msg.rate_limit_info?.unifiedWindows ?? {}).map(([window, w]) => ({
        type: 'quota',
        window,
        used: (w as Json).utilization,
        resetsAt: (w as Json).resetsAt,
      }));
    case 'result': {
      const u = msg.usage ?? {};
      return [
        {
          type: 'done',
          ok: msg.subtype === 'success' && !msg.is_error,
          reason: msg.terminal_reason ?? msg.subtype,
          costUsd: msg.total_cost_usd ?? 0,
          inputTokens: (u.input_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + (u.cache_read_input_tokens ?? 0),
          outputTokens: u.output_tokens ?? 0,
        },
      ];
    }
    default:
      return [];
  }
}
