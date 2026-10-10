// What every driver reports to the app, whatever CLI is behind it.
export type EngineEvent =
  | { type: 'started'; sessionId: string; model: string }
  | { type: 'text'; delta: string }
  | { type: 'thinking'; delta: string }
  | { type: 'tool'; id: string; name: string; input: unknown }
  | { type: 'tool-result'; id: string; ok: boolean; text: string }
  | { type: 'permission'; requestId: string; tool: string; input: unknown }
  | { type: 'quota'; window: string; used: number; resetsAt: number }
  | { type: 'done'; ok: boolean; reason: string; costUsd: number; inputTokens: number; outputTokens: number }
  | { type: 'error'; message: string };

// A user turn: plain text plus optional attachments.
export interface UserTurn {
  text: string;
  images?: { mediaType: string; base64: string }[];
}
