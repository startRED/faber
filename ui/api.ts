import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// The shapes the engine sends (engine/src/model.rs).
export type Vendor = 'claude' | 'codex';

/**
 * The shell's view of the update. The first three phases belong to the install at open: looking, downloading (with a whole
 * `percent` when the size is known, else the `mb` so far) and installing. `ready` is one kept for a restart.
 */
export interface UpdateStatus {
  phase: 'none' | 'checking' | 'downloading' | 'installing' | 'ready';
  percent?: number;
  mb?: number;
}
export type Effort = 'low' | 'medium' | 'high';
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
/** A request the CLI denied for want of anyone to answer it. */
export interface Parked {
  toolUseId: string;
  tool: string;
  /** "command", "file", "page" or "other": the request itself is never kept. */
  kind: "command" | "file" | "page" | "other";
  /** A file path, or the host of a page; empty otherwise. */
  label: string;
  at: number;
}
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  at: number;
  textBreaks?: number[];
  activityEnd?: number;
  images?: Image[];
  attachments?: string[];
  vendor?: Vendor;
  model?: string;
  reason?: string;
  effort?: Effort;
  thinking?: string;
  tools?: ToolUse[];
  permissions?: Permission[];
  files?: { path: string; size: number }[];
  status?: Status;
  error?: string;
  costUsd?: number;
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  cacheWrite1hTokens?: number;
  /** When the prompt cache of this reply goes cold, in milliseconds. Claude only; absent when unknown. */
  cacheWarmUntil?: number;
  /** Requests the CLI denied in this reply because nobody could answer them; they were not done. */
  parked?: Parked[];
  /** How many more were denied beyond the few kept. */
  parkedMore?: number;
  incomplete?: boolean;
  pace?: { kind: 'step' | 'low'; vendor: Vendor };
  /** The mode that really ran this reply, as an engine key, only when it is not the one chosen (`mode.fallback.auto`). */
  mode?: string;
  /** A note under the reply, as an engine key (`note.historyResent`). */
  note?: string;
  waitUntil?: number;
  movedFrom?: Vendor;
  /** The next rung up, offered when the turn's own tools show it stopped early. */
  nudge?: { reason: string; vendor: Vendor; model: string; effort: Effort };
}
export interface QueuedTurn {
  id: string;
  text: string;
  images?: Image[];
  attachments?: string[];
  choice: Choice;
  at: number;
  /** Handed to the running turn and not yet taken in: it can no longer be removed. */
  sending?: boolean;
}
export interface Conversation {
  id: string;
  title: string;
  choice?: Choice;
  area: Area;
  projectId?: string;
  workingFolder?: string;
  sessions: Partial<Record<Vendor, string>>;
  allowAlways?: string[];
  branchOf?: { conversationId: string; messageId: string };
  createdAt: number;
  updatedAt: number;
  queued?: QueuedTurn[];
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

export type GitMode = 'seguro' | 'equilibrado';
export type RunMode = 'normal' | 'autonomous';
export type MissionState = 'planning' | 'running' | 'paused' | 'blocked' | 'done';
export type TaskState = 'ready' | 'running' | 'review' | 'done' | 'blocked';
export type DecisionKind = 'direction' | 'taste' | 'money' | 'legal' | 'technical';

export interface Author {
  name: string;
  email: string;
}

export interface Grant {
  repo: string;
  mode: GitMode;
  revision: number;
  grantedAt: number;
  revokedAt?: number;
  author?: Author;
  selfUpdate: boolean;
  remote?: string;
  remoteAccount?: string;
}

export interface EnrollView {
  projectId: string;
  repo: string;
  base: string;
  author?: Author;
  mode: GitMode;
  revision: number;
  selfUpdate: boolean;
  remote?: string;
  remoteAccount?: string;
}

export interface TaskView {
  id: string;
  title: string;
  state: TaskState;
  reason?: string;
  builder?: Vendor;
  reviewer?: Vendor;
  detail?: { scope: string[]; proofs: string[]; artifacts: ArtifactView[] };
}

export interface ArtifactView {
  id: string; summary: string; changed: string[]; files: string[]; tree?: string;
  commands: { proof: string; argv: string[]; exit?: number; log?: string }[];
  notRun: string[]; assumptions: string[]; risks: string[];
}

export interface CostView {
  ownMicros?: number | null;
  capMicros?: number | null;
  complete: boolean;
}

export interface PaceView {
  vendor: Vendor;
  window: string;
  used?: number | null;
  room?: number | null;
  readAt?: number | null;
}

export interface DecisionView {
  id: string;
  question: string;
  kind: DecisionKind;
  options: string[];
  default?: string | null;
  task?: string | null;
}

export interface BuildView {
  current?: string | null;
  next?: string | null;
  lastGood?: string | null;
  waiting?: string | null;
}

export interface MissionView {
  missionId: string;
  revision: number;
  state: MissionState;
  mode: RunMode;
  gitMode: GitMode;
  goal: string;
  plan?: string | null;
  tasks: TaskView[];
  cost: CostView;
  pace: PaceView[];
  decisions: DecisionView[];
  build?: BuildView | null;
  reason?: string | null;
  progress?: ProgressView | null;
  report?: ReportView | null;
  needs?: string | null;
}

/** What a project mission proves its work with (R137): the commands, where they were declared, and whether the owner confirmed them. */
export interface ProofView {
  commands: string[];
  declared: string[];
  source: 'agents' | 'claude' | 'detected' | 'none';
  confirmed: boolean;
  changed: boolean;
  /** What installs the dependencies first, found from the lock files; shown with the checks, not confirmed separately. */
  setup?: string[];
}

/** How the check of the agents goes (Faber earns its own role receipts before a mission, hidden). */
export interface AgentsView {
  state: 'checking' | 'ready' | 'failed';
  step: number;
  total: number;
  vendor?: Vendor | null;
  estimateSeconds: number;
  detail?: string | null;
}

export interface ProgressView {
  step: string;
  number: number;
  total: number;
  role?: string | null;
  vendor?: Vendor | null;
  model?: string | null;
}

export interface ReportView {
  branch: string;
  head?: string | null;
  checks: { id: string; result: string }[];
  seconds?: number | null;
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

/** The answer to "Do you already use AI?". No answer counts as a beginner. */
export type Experience = 'beginner' | 'experienced';
/** How a CLI may act in Chat (R140). `dontAsk` is Full access and the default when nothing is saved. */
export type PermissionMode = 'ask' | 'acceptEdits' | 'plan' | 'auto' | 'dontAsk';
/** The modes in the order they are listed, strictest first; full access is the default. */
export const PERMISSION_MODES: PermissionMode[] = ['ask', 'acceptEdits', 'plan', 'auto', 'dontAsk'];
export type WebSearch = 'off' | 'cached' | 'live';
export type ReasoningSummary = 'default' | 'auto' | 'concise' | 'detailed' | 'none';
export type Verbosity = 'default' | 'low' | 'medium' | 'high';
export interface Settings {
  appearance?: Appearance;
  userHooks?: boolean;
  permissions?: PermissionMode;
  /** Missing means live. */
  webSearch?: WebSearch;
  /** Codex only; missing means the model's own. */
  reasoningSummary?: ReasoningSummary;
  verbosity?: Verbosity;
  notifications?: boolean;
  notificationSound?: boolean;
  language?: string;
  setup?: 'done' | 'skipped';
  experience?: Experience;
  /** False once the first run asked for the tutorial; true once it ended or was skipped. */
  tutorialDone?: boolean;
  /** How the person asked to be called (at most 40 characters); the engine keeps it trimmed and none when cleared. A save with
   * an empty text clears it. */
  userName?: string;
  keepAwake?: boolean;
  autostart?: boolean;
  /** Receive previews: update checks read the preview channel. Off by default; turning it off never downgrades. */
  previews?: boolean;
}
export interface ModelInfo {
  id: string;
  label: string;
  description?: string;
  isDefault: boolean;
}
/** One conversation that is answering now or waits for the user: a row of the running list (engine/src/model.rs). */
export interface RunningItem {
  conversationId: string;
  /** The id of the turn's reply message. With the request id it names a question: request ids alone repeat. */
  turnId: string;
  /** Empty while the conversation has none. */
  title: string;
  area: Area;
  projectId?: string;
  /** None while Auto is still choosing the model. */
  vendor?: Vendor;
  model?: string;
  state: 'working' | 'needsYou';
  /** The `at` of the turn's reply message, in milliseconds. */
  since: number;
  permissions: Permission[];
  parked?: Parked[];
  /** How many more were denied beyond the few kept. */
  parkedMore?: number;
  waitUntil?: number;
}
/** The running list in the engine's order, with a revision that grows by one with every list the engine sends. */
export interface RunningList {
  revision: number;
  items: RunningItem[];
}
/** The latest reading of one quota window of one vendor. `resetsAt` is in seconds, and 0 when the CLI did not say. */
export interface QuotaReading {
  vendor: Vendor;
  window: string;
  used: number;
  resetsAt: number;
}
/** `used` is the reading as the vendor gave it; `readAt` is when (seconds). `uncounted` is the share Faber's own recent work adds that the reading does not hold yet, present only where a measured conversion exists. */
export interface WindowUsage { name: string; used: number; elapsed: number; expected: number; resetsAt: number; readAt: number; uncounted?: number | null }
export interface VendorUsage {
  vendor: Vendor; plan?: string | null; state: 'ok' | 'spare' | 'fast' | 'low' | 'out' | 'unknown';
  learning: boolean; relearning: boolean; outsideShare?: number | null; foreignUse: boolean;
  overageOn: boolean; billedPerUse: boolean; billingUnknown: boolean; askForPlan: boolean; paidPossible: boolean;
  ownSpendUsd: number; unmetered: number; counting: boolean; windows: WindowUsage[];
}
export interface AppState {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
  settings: Settings;
  look: Look;
  models: ModelInfo[];
  running: RunningList;
  quotas: QuotaReading[];
  usage?: VendorUsage[];
  version: string;
}
export interface SavedSettings {
  settings: Settings;
  look: Look;
}
export interface Choice {
  vendor: Vendor | 'auto';
  model?: string;
  effort?: Effort;
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
  | { type: 'queue'; conversationId: string; queued: QueuedTurn[] }
  | ({ type: 'quota' } & QuotaReading)
  | ({ type: 'running' } & RunningList)
  | { type: 'usage'; vendors: VendorUsage[] }
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

/** A text in two forms: `one` for a count of 1, `other` for any other count. */
export interface Plural {
  one: string;
  other: string;
}
/** The words the shell shows by itself. Counts are `{n}` and the conversation is `{title}`; the shell fills them. */
export interface ShellTexts {
  open: string;
  quit: string;
  running: Plural;
  needsYou: Plural;
  notify: string;
  notifyMany: string;
}

/** A recent folder as the engine judged it just now: `reason` is a translation key when it cannot be picked. */
export interface FolderView {
  path: string;
  name: string;
  parent: string;
  reason?: string | null;
}
export interface EnvironmentView {
  id: string;
  name: string;
  folders: FolderView[];
}
export interface RecentsView {
  folders: FolderView[];
  environments: EnvironmentView[];
}

// Typed calls into the engine (the commands registered in src-tauri/src/main.rs and src-tauri/src/setup.rs).
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
  removeQueued: (id: string, turnId: string) => invoke<void>('remove_queued', { id, turnId }),
  sendQueued: (id: string, turnId: string) => invoke<void>('send_queued', { id, turnId }),
  removeProject: (id: string) => invoke<void>('remove_project', { id }),
  showProject: (id: string) => invoke<void>('show_project', { id }),
  addProject: () => invoke<Project | null>('add_project'),
  chatFolder: (id: string | undefined, scratch = false, path?: string) =>
    invoke<Conversation | null>('chat_folder', { id: id ?? null, scratch, path: path ?? null }),
  recents: () => invoke<RecentsView>('recents'),
  openFolders: (paths: string[]) => invoke<Project[]>('open_folders', { paths }),
  forgetRecent: (path: string) => invoke<void>('forget_recent', { path }),
  saveEnvironment: (name: string, folders: string[]) => invoke<void>('save_environment', { name, folders }),
  forgetEnvironment: (id: string) => invoke<void>('forget_environment', { id }),
  startEnvironment: (id: string) => invoke<Project[]>('start_environment', { id }),
  previewLook: (appearance: Appearance) => invoke<Look>('preview_look', { appearance }),
  saveSettings: (patch: Settings) => invoke<SavedSettings>('save_settings', { patch }),
  saveTheme: (appearance: Appearance, name: string) => invoke<boolean>('save_theme', { appearance, name }),
  openTheme: () => invoke<ThemeFile | null>('open_theme'),
  saveFile: (id: string, path: string) => invoke<boolean>('save_file', { id, path }),
  showFile: (id: string, path: string) => invoke<void>('show_file', { id, path }),
  readImage: (id: string, path: string) => invoke<string | null>('read_image', { id, path }),
  openTerminal: (id: string) => invoke<boolean>('open_terminal', { id }),
  openLink: (url: string) => invoke<void>('open_link', { url }),
  setShellTexts: (texts: ShellTexts) => invoke<void>('set_shell_texts', { texts }),
  answerPlan: (vendor: Vendor, plan: string) => invoke<void>('answer_plan', { vendor, plan }),
  approveCredits: (id: string, turnId: string, vendor: Vendor) => invoke<void>('approve_credits', { id, turnId, vendor }),
  moveTurn: (id: string, turnId: string) => invoke<void>('move_turn', { id, turnId }),
  waitTurn: (id: string, turnId: string) => invoke<void>('wait_turn', { id, turnId }),
  setClock: (utcOffset: number) => invoke<void>('set_clock', { utcOffset }),
  /** Tells the engine whether the window is open and focused: it reads the quota every 5 minutes while open, every 15 while hidden, and at once on focus. */
  quotaAttention: (visible: boolean, focused: boolean) => invoke<void>('quota_attention', { visible, focused }),
  setupCheck: (vendor: Vendor) => invoke<CheckResult>('setup_check', { vendor }),
  setupInstall: (vendor: Vendor) => invoke<void>('setup_install', { vendor }),
  setupSignIn: (vendor: Vendor) => invoke<void>('setup_sign_in', { vendor }),
  projectEnroll: (projectId: string) => invoke<EnrollView>('project_enroll', { projectId }),
  localUpdateState: () => invoke<BuildView | null>('local_update_state'),
  updateStatus: () => invoke<UpdateStatus>('update_status'),
  /** The first frame is painted: the shell may show the window (it stays hidden until then). */
  screenReady: () => invoke<void>('screen_ready'),
  updateRestart: () => invoke<void>('update_restart'),
  inputSeen: () => invoke<void>('input_seen'),
  /** Whether any field holds something typed or attached and not sent: no unattended update closes the window then. */
  draftDirty: (dirty: boolean) => invoke<void>('draft_dirty', { dirty }),
  onUpdate: (listener: (status: UpdateStatus) => void) => listen<UpdateStatus>('faber:update', (e) => listener(e.payload)),
  notesSeen: () => invoke<number | null>('notes_seen').then((id) => id ?? undefined),
  saveNotesSeen: (id: number) => invoke<void>('save_notes_seen', { id }),
  projectGrant: (projectId: string, mode: GitMode, selfUpdate: boolean, revision: number) =>
    invoke<Grant>('project_grant', { projectId, mode, selfUpdate, revision }),
  projectRevoke: (projectId: string) => invoke<void>('project_revoke', { projectId }),
  projectProof: (projectId: string) => invoke<ProofView>('project_proof', { projectId }),
  projectProofConfirm: (projectId: string, commands: string[], setup: string[], declared: string[]) =>
    invoke<ProofView>('project_proof_confirm', { projectId, commands, setup, declared }),
  missionStart: (projectId: string, goal: string) => invoke<string>('mission_start', { projectId, goal }),
  missionState: (missionId: string) => invoke<MissionView>('mission_state', { missionId }),
  missionMode: (missionId: string, mode: RunMode) => invoke<MissionView>('mission_mode', { missionId, mode }),
  missionStop: (missionId: string) => invoke<MissionView>('mission_stop', { missionId }),
  missionResume: (missionId: string) => invoke<MissionView>('mission_resume', { missionId }),
  missionAnswer: (missionId: string, decisionId: string, answer: string) =>
    invoke<MissionView>('mission_answer', { missionId, decisionId, answer }),
  onAgents: (listener: (view: AgentsView) => void) => listen<AgentsView>('agents', (e) => listener(e.payload)),
  onMission: (listener: (view: MissionView) => void) => listen<MissionView>('mission', (e) => listener(e.payload)),
  on: (listener: (event: AppEvent) => void) => listen<AppEvent>('faber:event', (e) => listener(e.payload)),
};

/** The engine reports user-facing errors as translation keys; Tauri wraps them as plain strings. The look engine
 * rejects with a `Failure` instead, so its error can name the field or the pairs. */
export const failureOf = (err: unknown): Failure => {
  if (typeof err === 'object' && err !== null && 'key' in err && typeof err.key === 'string') return err as Failure;
  return { key: typeof err === 'string' ? err : err instanceof Error ? err.message : String(err) };
};
export const errorText = (err: unknown) => failureOf(err).key;
