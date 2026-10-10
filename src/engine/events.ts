// What every driver reports to the app, whatever CLI is behind it.
export type EngineEvent =
  | { type: 'started'; sessionId: string; model: string }
  | { type: 'text'; delta: string }
  | { type: 'thinking'; delta: string }
  | { type: 'tool'; id: string; name: string; input: unknown }
  | { type: 'tool-result'; id: string; ok: boolean; text: string }
  | { type: 'permission'; requestId: string; tool: string; input: unknown }
  | { type: 'quota'; window: string; used: number; resetsAt: number }
  // costUsd is the CLI's own API-priced figure when it reports one (Claude Code does, Codex does not).
  | { type: 'done'; ok: boolean; reason: string; costUsd?: number; inputTokens: number; outputTokens: number }
  // fatal: the CLI process is gone and the next turn needs a new one.
  | { type: 'error'; message: string; fatal?: boolean };

// One live conversation with a CLI, whatever the vendor.
export interface Session {
  send(turn: UserTurn): void;
  answer(requestId: string, allow: boolean): void;
  interrupt(): void;
  close(): void;
}

// A user turn: plain text plus optional attachments.
export interface UserTurn {
  text: string;
  images?: { mediaType: string; base64: string }[];
}
