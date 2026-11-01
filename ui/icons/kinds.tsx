import { Icon, type GlyphName } from '../Glyphs.tsx';
import { Mark } from '../Mark.tsx';
import type { Vendor } from '../api.ts';
import './icons.css';

// The visual language of the work (docs/design/icons.md): which drawing means which kind of step, which means which state
// of a conversation, and whose mark stands for who is speaking. The screens that show a step or a state take their drawing
// from here, so the same thing looks the same everywhere. Decoration only: the words beside a drawing carry the meaning.

/** The kinds of step a work log shows. */
export type ToolKind = 'read' | 'edit' | 'write' | 'run' | 'search' | 'web' | 'subagent' | 'helper' | 'decision' | 'plan' | 'ask' | 'skill' | 'mcp' | 'conversation' | 'other';

const GLYPH_OF_TOOL: Record<ToolKind, GlyphName> = {
  read: 'read',
  edit: 'edit',
  write: 'changes',
  run: 'terminal',
  search: 'search',
  web: 'globe',
  subagent: 'subagent',
  helper: 'helper',
  decision: 'decision',
  plan: 'list',
  ask: 'ask',
  skill: 'wand',
  mcp: 'plug',
  conversation: 'chat',
  other: 'bolt',
};

// The names both CLIs give their steps (Claude Code names its tools, Codex names its item kinds) and the kinds they mean.
// A name missing here is `other`, never a wrong picture.
const KIND_OF_NAME: Record<string, ToolKind> = {
  Read: 'read',
  Edit: 'edit',
  MultiEdit: 'edit',
  NotebookEdit: 'edit',
  fileChange: 'edit',
  Write: 'write',
  Bash: 'run',
  PowerShell: 'run',
  commandExecution: 'run',
  Glob: 'search',
  Grep: 'search',
  ToolSearch: 'search',
  WebSearch: 'web',
  WebFetch: 'web',
  Browser: 'web',
  webSearch: 'web',
  Agent: 'subagent',
  Task: 'subagent',
  Helper: 'helper',
  TodoWrite: 'plan',
  AskUserQuestion: 'ask',
  OfferBigger: 'ask',
  Skill: 'skill',
  mcpToolCall: 'mcp',
  Conversation: 'conversation',
};

/** The kind of step a CLI's tool name stands for. */
export function toolKind(name: string): ToolKind {
  return Object.hasOwn(KIND_OF_NAME, name) ? KIND_OF_NAME[name]! : name.startsWith('mcp__') ? 'mcp' : 'other';
}

/** The drawing beside a step's words. */
export function ToolGlyph(props: { kind: ToolKind; small?: boolean }) {
  return <Icon name={GLYPH_OF_TOOL[props.kind]} small={props.small} />;
}

/** The states a conversation, a helper or a plan step can be in. `stuck` is work that goes on and gets nowhere (docs/design/icons.md). */
export type StateKind = 'working' | 'waiting' | 'stuck' | 'concluded' | 'ended' | 'interrupted' | 'stopped' | 'failed';

export const STATE_KINDS: readonly StateKind[] = ['working', 'waiting', 'stuck', 'concluded', 'ended', 'interrupted', 'stopped', 'failed'];

// What the engine's own words for a state mean here. `unreported` and `unknown` are work that ended and said nothing: the
// broken ring, never the check.
const STATE_OF_WORD: Record<string, StateKind> = {
  running: 'working',
  background: 'working',
  streaming: 'working',
  pending: 'working',
  waiting: 'waiting',
  stuck: 'stuck',
  needsYou: 'waiting',
  done: 'concluded',
  passed: 'concluded',
  passing: 'concluded',
  merged: 'concluded',
  concluded: 'concluded',
  unconcluded: 'ended',
  unreported: 'ended',
  unknown: 'ended',
  interrupted: 'interrupted',
  closed: 'interrupted',
  stopped: 'stopped',
  declined: 'stopped',
  error: 'failed',
  failed: 'failed',
  failing: 'failed',
  conflict: 'failed',
};

/** The state an engine word stands for; a word nobody mapped is `ended`, which claims nothing good. */
export function stateKind(word: string): StateKind {
  return Object.hasOwn(STATE_OF_WORD, word) ? STATE_OF_WORD[word]! : 'ended';
}

/**
 * One state as a drawing. The colour is the state's (`.st-*` in `ui/styles.css`), the silhouette is its own, so a person who
 * cannot tell the colours apart still can tell the states. Give it a `label` where nothing beside it says the state; beside
 * the state's words it stays decoration.
 */
export function StateGlyph(props: { state: StateKind; small?: boolean; label?: string }) {
  return (
    <span class={`st st-${props.state}`} role={props.label ? 'img' : undefined} aria-label={props.label} aria-hidden={props.label ? undefined : 'true'}>
      <Icon name={`state-${props.state}`} small={props.small} />
    </span>
  );
}

/** Who is speaking: a vendor's shape and colour, or Faber's own mark when Faber, not an AI, said or chose it. */
export function ActorMark(props: { actor: Vendor | 'faber' }) {
  return props.actor === 'faber' ? <Mark size={12} class="mk-faber" /> : <span class={`mk ${props.actor}`} aria-hidden="true" />;
}
