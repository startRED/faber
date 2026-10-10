import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// The shapes the engine sends (engine/src/model.rs).
export type Vendor = 'claude' | 'codex';
export type Area = 'chat' | 'project';
export type Status = 'streaming' | 'done' | 'stopped' | 'error';
export interface Image {
  mediaType: string;
  base64: string;
}
export interface ToolUse {
  id: string;
  name: string;
  input: unknown;
  ok?: boolean;
  output?: string;
}
export interface Permission {
  requestId: string;
  tool: string;
  input: unknown;
}
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  at: number;
  images?: Image[];
  attachments?: string[];
  vendor?: Vendor;
  model?: string;
  reason?: string;
  thinking?: string;
  tools?: ToolUse[];
  permissions?: Permission[];
  files?: { path: string; size: number }[];
  status?: Status;
  error?: string;
  costUsd?: number;
  inputTokens?: number;
  outputTokens?: number;
}
export interface Conversation {
  id: string;
  title: string;
  area: Area;
  projectId?: string;
  sessions: Partial<Record<Vendor, string>>;
  allowAlways?: string[];
  branchOf?: { conversationId: string; messageId: string };
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}
export interface Summary {
  id: string;
  title: string;
  area: Area;
  projectId?: string;
  updatedAt: number;
}
export interface Project {
  id: string;
  name: string;
  path: string;
}
export interface Settings {
  appearance?: Record<string, string | null>;
  userHooks?: boolean;
  language?: string;
}
export interface ModelInfo {
  id: string;
  label: string;
  description?: string;
  isDefault: boolean;
}
export interface AppState {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
  settings: Settings;
  models: ModelInfo[];
  version: string;
}
export interface Choice {
  vendor: Vendor | 'auto';
  model?: string;
}
export interface Attachment {
  name: string;
  base64: string;
}
export type AppEvent =
  | { type: 'message'; conversationId: string; message: Message }
  | { type: 'quota'; vendor: Vendor; window: string; used: number; resetsAt: number }
  | { type: 'list' };

export interface SendRequest {
  conversationId?: string;
  area: Area;
  projectId?: string;
  text: string;
  images: Image[];
  files: Attachment[];
  choice: Choice;
}

// Typed calls into the engine (the commands in src-tauri/src/lib.rs).
export const api = {
  state: () => invoke<AppState>('state'),
  open: (id: string) => invoke<Conversation | null>('open', { id }),
  send: (req: SendRequest) => invoke<string>('send', { req }),
  branch: (req: { conversationId: string; messageId: string; text: string; choice: Choice }) => invoke<string>('branch', { req }),
  warm: (req: { conversationId?: string; area: Area; projectId?: string; choice: Choice }) => invoke<void>('warm', { req }),
  interrupt: (id: string) => invoke<void>('interrupt', { id }),
  answer: (id: string, requestId: string, allow: boolean, always = false) => invoke<void>('answer', { id, requestId, allow, always }),
  rename: (id: string, title: string) => invoke<void>('rename', { id, title }),
  remove: (id: string) => invoke<void>('remove', { id }),
  removeProject: (id: string) => invoke<void>('remove_project', { id }),
  addProject: () => invoke<Project | null>('add_project'),
  saveSettings: (patch: Settings) => invoke<Settings>('save_settings', { patch }),
  saveFile: (id: string, path: string) => invoke<boolean>('save_file', { id, path }),
  showFile: (id: string, path: string) => invoke<void>('show_file', { id, path }),
  readImage: (id: string, path: string) => invoke<string | null>('read_image', { id, path }),
  openTerminal: (id: string) => invoke<boolean>('open_terminal', { id }),
  openLink: (url: string) => invoke<void>('open_link', { url }),
  setTray: (open: string, quit: string) => invoke<void>('set_tray', { open, quit }),
  on: (listener: (event: AppEvent) => void) => listen<AppEvent>('faber:event', (e) => listener(e.payload)),
};

/** The engine reports user-facing errors as translation keys; Tauri wraps them as plain strings. */
export const errorText = (err: unknown) => (typeof err === 'string' ? err : err instanceof Error ? err.message : String(err));
