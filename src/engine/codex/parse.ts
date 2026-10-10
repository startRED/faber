import type { EngineEvent } from '../events.ts';

type Json = Record<string, any>;

const NOT_TOOLS = new Set(['userMessage', 'agentMessage', 'reasoning']);

// Codex reports a limit window by its length; Faber names the ones it knows.
function windowName(mins: number): string {
  if (mins === 300) return 'five_hour';
  if (mins === 10080) return 'seven_day';
  return `${mins}_minutes`;
}

// What parsing remembers across notifications: the latest usage, which Codex sends before turn/completed,
// and which messages already streamed, so a message that arrives whole is still shown.
export interface CodexTurnState {
  input: number;
  output: number;
  streamed: Set<string>;
}

// Turns one `codex app-server` notification into engine events.
export function parseCodexNotification(msg: Json, tokens: CodexTurnState): EngineEvent[] {
  const p = msg.params ?? {};
  switch (msg.method) {
    case 'item/agentMessage/delta':
      tokens.streamed.add(p.itemId);
      return [{ type: 'text', delta: p.delta }];
    case 'item/reasoning/summaryTextDelta':
      return [{ type: 'thinking', delta: p.delta }];
    case 'item/started':
      if (NOT_TOOLS.has(p.item?.type)) return [];
      return [{ type: 'tool', id: p.item.id, name: p.item.type, input: p.item.command ?? p.item.query ?? p.item.changes ?? p.item }];
    case 'item/completed': {
      const item = p.item ?? {};
      if (item.type === 'agentMessage' && item.text && !tokens.streamed.has(item.id)) return [{ type: 'text', delta: item.text }];
      if (NOT_TOOLS.has(item.type)) return [];
      const ok = item.status === 'completed' && (item.exitCode == null || item.exitCode === 0);
      return [{ type: 'tool-result', id: item.id, ok, text: item.aggregatedOutput ?? item.status ?? '' }];
    }
    case 'thread/tokenUsage/updated':
      tokens.input = p.tokenUsage?.last?.inputTokens ?? 0;
      tokens.output = p.tokenUsage?.last?.outputTokens ?? 0;
      return [];
    case 'account/rateLimits/updated':
      return [p.rateLimits?.primary, p.rateLimits?.secondary]
        .filter(Boolean)
        .map((w: Json) => ({ type: 'quota', window: windowName(w.windowDurationMins), used: w.usedPercent / 100, resetsAt: w.resetsAt }));
    case 'turn/completed': {
      const status = p.turn?.status;
      return [{ type: 'done', ok: status === 'completed', reason: status, inputTokens: tokens.input, outputTokens: tokens.output }];
    }
    case 'error':
      return [{ type: 'error', message: p.error?.message ?? JSON.stringify(p).slice(0, 300) }];
    default:
      return [];
  }
}
