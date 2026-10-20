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
/** The level a model starts at; the engine keeps the same default (`Effort` in engine/src/router.rs). */
export const DEFAULT_EFFORT: Effort = 'medium';
export type Area = 'chat' | 'project';
export type Status = 'streaming' | 'done' | 'stopped' | 'error';
/** Message hits use UTF-16 offsets, matching JavaScript string slices. */
export interface MessageHit {
  conversationId: string;
  area: Area;
  title: string;
  messageIndex: number;
  snippet: string;
  highlights: { start: number; end: number }[];
}
export interface MessageSearch {
  ready: boolean;
  hits: MessageHit[];
}
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
  /** The agent call that started the subagent which ran this tool; absent for the agent's own tools. */
  parent?: string;
  /** Refused before it ran: not a failure of the work. */
  declined?: boolean;
  /** On a subagent's call while the reply runs: waiting for the person's answer, or a sign it seems stuck. */
  sign?: "waiting" | "repeat" | "failing" | "silent";
  /** On a subagent's call once it ended (R161 step 4). */
  facts?: { edited?: string[]; checks: number; failed: number; unnamed?: string[] };
  /** A command that runs the project's tests, lint or build, as the engine marks it when it starts. */
  check?: boolean;
  /** On a helper's call: what the person wrote to it while it worked (R161 step 5); `taken` is absent while on its way. */
  told?: { id: string; text: string; taken?: boolean }[];
  /** When the engine saw the call start, in Unix milliseconds; absent on older conversations. */
  started?: number;
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
  /** On a reply that ended well: how long it took from its start, in milliseconds; absent on older replies. */
  took?: number;
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
  /** The pull request this reply opened or pushed, followed until its checks pass or it closes. */
  follow?: Follow;
  /** A message Faber sent by itself, and why (`wake.keepGoing`, `wake.checksFailed`, `wake.conflict`): drawn as a quiet line, not as the person's words. */
  wake?: { why: string; pr?: number; checks?: string[] };
  /** On a reply: "keep going" stopped after it, and why (a `reason.keepGoing.*` key). */
  halt?: string;
  /** On the newest reply of a conversation that went on by itself for half an hour or more: what it did while the person was away. */
  away?: { minutes: number; turns: number; switches: number; answered: number };
  /** Questions Faber answered with their first choice because nobody answered them while "keep going" was on. */
  autoAnswers?: { question: string; answer: string; others?: string[] }[];
  /** On a project reply that changed files: Faber's own run of the fast checks once it ended (R146). */
  checks?: Checks;
  /** On a reply: when its words, thinking or tools last changed, in milliseconds. "Updated 3 min ago" counts from it. */
  movedAt?: number;
  /** On a reply: when the agent said all it will say, in milliseconds. Set while the reply still streams, it is finished for the
   * person while the CLI closes its turn. */
  finishedAt?: number;
  /** On a reply: what the agent said it is doing now, in a few words (task 017). */
  now?: string;
  /** On a reply: the closing box of the task it ended. */
  wrapUp?: WrapUp;
}
/** A project agent's plan for the request (engine `Plan`): it belongs to the conversation. */
export interface Plan {
  steps: { text: string; state: 'pending' | 'active' | 'done' }[];
  at: number;
}
/** "3 of 7": a plan's steps done and in all. */
export interface PlanCount {
  done: number;
  total: number;
}
/** The closing box of a task (engine `WrapUp`): a recap, and maybe one question with its replies, the recommendation first. */
export interface WrapUp {
  recap: string;
  ask?: string;
  actions?: string[];
  done?: boolean;
  /** The reply Faber gave for the person while Keep going ran and nobody answered. */
  answered?: string;
}
export type ChecksState = 'running' | 'passed' | 'failed' | 'needsYou';
export interface Checks {
  state: ChecksState;
  passed: number;
  /** The checks that failed, each with its first short failures. */
  failed?: { command: string; lines?: string[] }[];
}
export type FollowState = 'pending' | 'failing' | 'conflict' | 'passing' | 'merged' | 'closed' | 'stopped';
export interface Follow {
  repo: string;
  number: number;
  url: string;
  state: FollowState;
  failed?: string[];
  head?: string;
  /** The checks failed while the quota was tight: the person was told instead of the agent. */
  held?: boolean;
  /** Why following ended early, as an engine key. */
  problem?: string;
  since: number;
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
  wake?: Message['wake'];
}
export interface Conversation {
  id: string;
  title: string;
  choice?: Choice;
  area: Area;
  projectId?: string;
  workingFolder?: string;
  /** A project conversation's own copy (a git worktree on its own branch), made at its first message. */
  copy?: WorkCopy;
  sessions: Partial<Record<Vendor, string>>;
  allowAlways?: string[];
  branchOf?: { conversationId: string; messageId: string };
  createdAt: number;
  updatedAt: number;
  queued?: QueuedTurn[];
  /** "Keep going" is on: Faber sends the next turn by itself. */
  keepGoing?: { turns: number; failed: number };
  /** How full its own agent's context is, once the CLI has said so (the topbar's meter). */
  context?: ContextFill;
  /** The agent's plan for the request. */
  plan?: Plan;
  /** When it was archived: it is off the sidebar and its composer gives way to a line that brings it back. */
  archivedAt?: number;
  /** When the person pinned it to the top of its sidebar list. */
  pinnedAt?: number;
  messages: Message[];
}
/** The tokens a conversation's own agent holds, and the window they are counted against. */
export interface ContextFill {
  used: number;
  window: number;
}
export interface Summary {
  id: string;
  title: string;
  area: Area;
  projectId?: string;
  updatedAt: number;
  /** "Keep going" is on. */
  keepGoing?: boolean;
  archived?: boolean;
  pinnedAt?: number;
  /** The conversation whose agent started this one, which the sidebar shows it under. */
  startedBy?: string;
  /** Its last run ended and nobody has opened it since: the running column's done card and the sidebar's dot. */
  ended?: Ended;
  /** How far the agent's plan has come. */
  plan?: PlanCount;
}
/** How a run ended that nobody saw end (engine `Ended`). */
export interface Ended {
  at: number;
  kind: 'done' | 'needsYou' | 'failed';
  /** From its first turn to its end, in milliseconds. */
  workMs: number;
  /** The last line the agent wrote. */
  line?: string;
}
export interface WorkCopy {
  path: string;
  branch: string;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  /** Each new conversation works in a separate copy (Settings > Projects). The engine always sends it, default filled. */
  separateCopies?: boolean;
}

/** The checks Faber runs on a project's work (R137): the commands, where they were declared, and whether the owner confirmed them. */
export interface ProofView {
  commands: string[];
  declared: string[];
  source: 'agents' | 'claude' | 'detected' | 'none';
  confirmed: boolean;
  changed: boolean;
  /** What installs the dependencies first, found from the lock files; shown with the checks, not confirmed separately. */
  setup?: string[];
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
  /** The subagent calls of the reply that seem stuck: each needs the person like a question. */
  stuck?: string[];
  waitUntil?: number;
  /** A followed pull request whose checks failed while the quota was tight: its number and commit. */
  held?: [number, string];
  /** "Keep going" is on: it runs on its own. */
  keepGoing?: boolean;
  /** "Keep going" waits to send the next turn, for a reason of its own (a quota wait is `waitUntil`), until `until` (seconds). */
  keepWait?: { why: 'retry' | 'rest' | 'busy' | 'start' | 'question'; until: number };
  /** How far a conversation that goes on by itself has come: turns of work, when it began (ms, 0 unknown), the last line it wrote. */
  keepProgress?: { turns: number; since: number; last?: string };
  /** Faber asks git whether the last reply changed files, before its checks would start. */
  weighing?: boolean;
  /** The agent finished and only the CLI's turn is closing, with nothing else running: the screen leaves the row out. */
  settled?: boolean;
  /** How far the agent's plan has come. */
  plan?: PlanCount;
  /** What the agent said it is doing now, on the reply under way. */
  now?: string;
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
/** One skill of Faber's catalog (src-tauri/src/tools.rs). */
export interface ToolRow {
  id: string;
  job: string;
  license: string;
  repo: string;
  home: string;
  installed: boolean;
  /** A tool server (an npm package Node runs) rather than a skill. */
  server: boolean;
}

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
/** An entry of a working folder offered by `@` in the composer (parity T4): its path from the folder, `/` between parts. */
export interface Mention {
  path: string;
  dir: boolean;
}
/** A command or skill reported by an agent, offered by `/` without starting a process. */
export interface Command {
  vendor: Vendor;
  name: string;
  skill: boolean;
  note?: string | null;
}
/** Git's current changes in a conversation's working copy, against its starting branch. */
export interface FileChange {
  path: string;
  added: number;
  removed: number;
  binary: boolean;
  countsKnown?: boolean;
  tooLarge?: boolean;
  status: 'modified' | 'added' | 'deleted';
}
export interface ChangeList {
  files: FileChange[];
  incomplete: boolean;
  next?: string;
}
/** One file, read only when opened; oversized patches keep their counts in FileChange. */
export interface ChangePatch {
  text: string;
  tooLarge: boolean;
  binary: boolean;
  error?: string | null;
}
/** A quick question on the side of a conversation (R250), as the engine keeps it until the turn after it ends. */
export interface Aside {
  id: string;
  question: string;
  answer: string;
  vendor: Vendor;
  model?: string | null;
  state: 'running' | 'done' | 'stopped' | 'failed';
  /** A translation key when it failed. */
  error?: string | null;
}
export type AppEvent =
  | { type: 'commands'; conversationId: string; vendor: Vendor }
  | { type: 'message'; conversationId: string; message: Message }
  | { type: 'asides'; conversationId: string; asides: Aside[] }
  | { type: 'queue'; conversationId: string; queued: QueuedTurn[] }
  | ({ type: 'quota' } & QuotaReading)
  | ({ type: 'running' } & RunningList)
  | { type: 'usage'; vendors: VendorUsage[] }
  | { type: 'list' }
  | { type: 'context'; conversationId: string; context?: ContextFill }
  | { type: 'plan'; conversationId: string; plan?: Plan }
  | { type: 'halted'; conversationId: string; title: string; why: string; vendor?: Vendor }
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
/** Why "keep going" stops by itself: the last part of the engine's `reason.keepGoing.*` keys. */
export const STOPS = ['done', 'asked', 'failed', 'cap', 'pace', 'quota', 'billed'] as const;

export interface ShellTexts {
  open: string;
  quit: string;
  running: Plural;
  needsYou: Plural;
  notify: string;
  notifyMany: string;
  /** When "keep going" stops by itself, by reason, with `{title}` and `{ai}`. */
  stopped: Record<string, string>;
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

// Names this load of the screen to the shell's browser pane: a request of any other load (a screen that went away) is ignored.
const LOAD = crypto.getRandomValues(new Uint32Array(1))[0];

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
  keepGoing: (id: string, on: boolean) => invoke<void>('keep_going', { id, on }),
  /** The person has the conversation open on a visible window: the done card and the dot of its last run go. */
  seen: (id: string) => invoke<void>('seen', { id }),
  /** Archives a conversation, or brings an archived one back. */
  archive: (id: string, on: boolean) => invoke<void>('archive', { id, on }),
  pin: (id: string, on: boolean) => invoke<void>('pin', { id, on }),
  answer: (id: string, turnId: string, requestId: string, allow: boolean, always = false) =>
    invoke<void>('answer', { id, turnId, requestId, allow, always }),
  /** The choices for a question the assistant asked: each question's text to the label picked (several joined by a comma). */
  answerQuestion: (id: string, turnId: string, requestId: string, answers: Record<string, string>) =>
    invoke<void>('answer_question', { id, turnId, requestId, answers }),
  searchMessages: (query: string, area: Area) => invoke<MessageSearch>('search_messages', { query, area }),
  promptHistory: (conversationId: string | undefined, area: Area, projectId?: string, signal?: AbortSignal) => {
    if (signal?.aborted) return Promise.resolve([]);
    const requestId = crypto.randomUUID();
    const args = { requestId, conversationId, area, projectId };
    const cancel = () => { void invoke<string[]>('prompt_history', { ...args, cancel: true }).catch(() => {}); };
    signal?.addEventListener('abort', cancel, { once: true });
    return invoke<string[]>('prompt_history', args).finally(() => signal?.removeEventListener('abort', cancel));
  },
  rename: (id: string, title: string) => invoke<void>('rename', { id, title }),
  exportConversation: (id: string, words: { you: string; auto: string; attachments: string; files: string }) => invoke<boolean>('export_conversation', { id, words }),
  /** Refused with `error.copy.unsaved` while its copy holds work found nowhere else, unless `discard`. */
  remove: (id: string, discard = false) => invoke<void>('remove', { id, discard }),
  removeQueued: (id: string, turnId: string) => invoke<void>('remove_queued', { id, turnId }),
  editQueued: (id: string, turnId: string, text: string) => invoke<void>('edit_queued', { id, turnId, text }),
  sendQueued: (id: string, turnId: string) => invoke<void>('send_queued', { id, turnId }),
  tellHelper: (id: string, callId: string, text: string) => invoke<void>('tell_helper', { id, callId, text }),
  /** `@` in the composer: the working folder's entries that match `query`, at most 50; a new chat has none. */
  mentions: (id: string | undefined, projectId: string | undefined, query: string) => invoke<Mention[]>('mentions', { id, projectId, query }),
  commands: (id?: string, vendor?: Vendor) => invoke<Command[]>('commands', { id, vendor }),
  changes: (id: string, cursor?: string) => invoke<ChangeList>('changes', { id, cursor }),
  changePatch: (id: string, path: string) => invoke<ChangePatch>('change_patch', { id, path }),
  /** A quick question on the side (R250); refused with an `error.aside.*` key, `error.quotaOut` or `error.billedPerUse`. */
  askAside: (id: string, question: string, model?: string) => invoke<string>('ask_aside', { id, question, model }),
  stopAside: (asideId: string) => invoke<void>('stop_aside', { asideId }),
  asides: (id: string) => invoke<Aside[]>('asides', { id }),
  unfollowPr: (id: string) => invoke<void>('unfollow_pr', { id }),
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
  /** Faber's browser pane (R161 step 7): over a box of the screen, off it, a page the person opens, a call's picture. */
  sitePlace: (boxId: number, x: number, y: number, width: number, height: number) => invoke<boolean>('site_place', { load: LOAD, boxId, x, y, width, height }),
  sitePark: (boxId: number) => invoke<void>('site_park', { load: LOAD, boxId }),
  siteOpen: (id: string, url: string) => invoke<boolean>('site_open', { id, url }),
  sitePicture: (id: string, call: string) => invoke<string | null>('site_picture', { id, call }),
  /** Where the page in Faber's browser is now, at every page it finishes loading. */
  onSite: (listener: (url: string) => void) => listen<string>('faber:site', (e) => listener(e.payload)),
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
  /** The catalog's skills (R247) and whether each is installed; installing downloads it from its own repository. */
  toolsList: () => invoke<ToolRow[]>('tools_list'),
  toolsInstall: (id: string) => invoke<void>('tools_install', { id }),
  toolsRemove: (id: string) => invoke<void>('tools_remove', { id }),
  updateStatus: () => invoke<UpdateStatus>('update_status'),
  /** The first frame is painted: the shell may show the window (it stays hidden until then). */
  screenReady: () => invoke<void>('screen_ready'),
  updateRestart: () => invoke<void>('update_restart'),
  inputSeen: () => invoke<void>('input_seen'),
  /** Whether any field holds something typed or attached and not sent: no unattended update closes the window then. */
  draftDirty: (dirty: boolean) => invoke<void>('draft_dirty', { dirty }),
  /** Whether a game or another full-screen app is in front of the window (always no on a test build and off Windows). */
  gameInFront: () => invoke<boolean>('game_in_front'),
  onUpdate: (listener: (status: UpdateStatus) => void) => listen<UpdateStatus>('faber:update', (e) => listener(e.payload)),
  notesSeen: () => invoke<number | null>('notes_seen').then((id) => id ?? undefined),
  saveNotesSeen: (id: number) => invoke<void>('save_notes_seen', { id }),
  projectCopies: (projectId: string, on: boolean) => invoke<void>('project_copies', { projectId, on }),
  projectProof: (projectId: string) => invoke<ProofView>('project_proof', { projectId }),
  projectProofConfirm: (projectId: string, commands: string[], setup: string[], declared: string[]) =>
    invoke<ProofView>('project_proof_confirm', { projectId, commands, setup, declared }),
  on: (listener: (event: AppEvent) => void) => listen<AppEvent>('faber:event', (e) => listener(e.payload)),
};

/** The engine reports user-facing errors as translation keys; Tauri wraps them as plain strings. The look engine
 * rejects with a `Failure` instead, so its error can name the field or the pairs. */
export const failureOf = (err: unknown): Failure => {
  if (typeof err === 'object' && err !== null && 'key' in err && typeof err.key === 'string') return err as Failure;
  return { key: typeof err === 'string' ? err : err instanceof Error ? err.message : String(err) };
};
export const errorText = (err: unknown) => failureOf(err).key;
