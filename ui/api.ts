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
// The shapes the look engine sends and takes (src-tauri/src/look.rs). The screen never computes a colour.
export type Motion = 'full' | 'reduced' | 'none';
export interface Effects {
  ambient: boolean;
  texture: boolean;
  glow: boolean;
}
/** The user's choices behind a look. Only `base` is required; the rest falls back to the base's own value. */
export interface Appearance {
  base: string;
  ground?: string;
  accent?: string;
  textFont?: string;
  headingFont?: string;
  textSize?: number;
  density?: number;
  corners?: number;
  motion?: Motion;
  effects?: Effects;
  colors?: Record<string, string>;
}
export interface Check {
  a: string;
  b: string;
  ratio: number;
  min: number;
  pass: boolean;
}
export interface Look {
  /** CSS custom properties, without the leading dashes. */
  vars: Record<string, string>;
  scheme: 'dark' | 'light';
  motion: Motion;
  effects: Effects;
  checks: Check[];
  /** Translation keys, such as `look.accentAdjusted`. */
  notices: string[];
  /** The choices with every field filled in from the base, so a control can show what is on. */
  /** Every choice filled in; a look with a heading of its own (Cobalt) has no `headingFont` until one is chosen. */
  appearance: Required<Omit<Appearance, 'colors' | 'headingFont'>> & { headingFont?: string; colors?: Record<string, string> };
}
/** A refusal from the look engine: a translation key and the field or pairs it is about. */
export interface Failure {
  key: string;
  detail?: string;
}
export type ThemeFile = Appearance & { name: string };

export interface Settings {
  appearance?: Appearance;
  userHooks?: boolean;
  language?: string;
  setup?: 'done' | 'skipped';
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
  look: Look;
  models: ModelInfo[];
  version: string;
}
export interface SavedSettings {
  settings: Settings;
  look: Look;
}
export interface Choice {
  vendor: Vendor | 'auto';
  model?: string;
}
export interface Attachment {
  name: string;
  base64: string;
}
export type AccountStatus = 'signedIn' | 'signedOut' | 'unknown';
export interface CheckResult {
  vendor: Vendor;
  installed: boolean;
  account: AccountStatus;
  canInstall: boolean;
}
export interface SetupProgressEvent {
  type: 'setup';
  vendor: Vendor;
  step: string;
  error?: string;
  detail?: string;
}
export type AppEvent =
  | { type: 'message'; conversationId: string; message: Message }
  | { type: 'quota'; vendor: Vendor; window: string; used: number; resetsAt: number }
  | { type: 'list' }
  | SetupProgressEvent;

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
  routeDraft: (req: { conversationId?: string; area: Area; projectId?: string; choice: Choice }, text: string) =>
    invoke<void>('route_draft', { req, text }),
  interrupt: (id: string) => invoke<void>('interrupt', { id }),
  answer: (id: string, turnId: string, requestId: string, allow: boolean, always = false) =>
    invoke<void>('answer', { id, turnId, requestId, allow, always }),
  rename: (id: string, title: string) => invoke<void>('rename', { id, title }),
  remove: (id: string) => invoke<void>('remove', { id }),
  removeProject: (id: string) => invoke<void>('remove_project', { id }),
  addProject: () => invoke<Project | null>('add_project'),
  previewLook: (appearance: Appearance) => invoke<Look>('preview_look', { appearance }),
  saveSettings: (patch: Settings) => invoke<SavedSettings>('save_settings', { patch }),
  saveTheme: (appearance: Appearance, name: string) => invoke<boolean>('save_theme', { appearance, name }),
  openTheme: () => invoke<ThemeFile | null>('open_theme'),
  saveFile: (id: string, path: string) => invoke<boolean>('save_file', { id, path }),
  showFile: (id: string, path: string) => invoke<void>('show_file', { id, path }),
  readImage: (id: string, path: string) => invoke<string | null>('read_image', { id, path }),
  openTerminal: (id: string) => invoke<boolean>('open_terminal', { id }),
  openLink: (url: string) => invoke<void>('open_link', { url }),
  setTray: (open: string, quit: string) => invoke<void>('set_tray', { open, quit }),
  setupCheck: (vendor: Vendor) => invoke<CheckResult>('setup_check', { vendor }),
  setupInstall: (vendor: Vendor) => invoke<void>('setup_install', { vendor }),
  setupSignIn: (vendor: Vendor) => invoke<void>('setup_sign_in', { vendor }),
  on: (listener: (event: AppEvent) => void) => listen<AppEvent>('faber:event', (e) => listener(e.payload)),
};

/** The engine reports user-facing errors as translation keys; Tauri wraps them as plain strings. The look engine
 * rejects with a `Failure` instead, so its error can name the field or the pairs. */
export const failureOf = (err: unknown): Failure => {
  if (typeof err === 'object' && err !== null && 'key' in err && typeof err.key === 'string') return err as Failure;
  return { key: typeof err === 'string' ? err : err instanceof Error ? err.message : String(err) };
};
export const errorText = (err: unknown) => failureOf(err).key;
