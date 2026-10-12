import type { Appearance, Experience, Settings, Vendor } from './api.ts';

export type Subscription = 'claude' | 'codex' | 'none';

export type AccountStatus = 'signedIn' | 'signedOut' | 'unknown';

export interface Check {
  vendor: Vendor;
  installed: boolean;
  account: AccountStatus;
  canInstall: boolean;
}

export interface SetupProgress {
  type: 'setup';
  vendor: Vendor;
  step: string;
  error?: string;
  detail?: string;
}

export interface CardInput {
  check?: Check;
  progress?: SetupProgress;
  waiting?: boolean;
  error?: { key: string; detail?: string } | string;
}

export interface Card {
  status: string;
  actions: string[];
  error?: { message: string; detail?: string };
}

export const OFFICIAL_PAGES: Record<Vendor, string> = {
  claude: 'https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/overview',
  codex: 'https://github.com/openai/codex',
};

/**
 * The setup guide opens automatically only when there is no saved setup decision
 * (neither 'done' nor 'skipped') and there are no saved conversations.
 */
export function shouldOpenSetup(
  settings: { setup?: 'done' | 'skipped' } | undefined,
  conversations: readonly unknown[],
): boolean {
  if (settings?.setup === 'done' || settings?.setup === 'skipped') {
    return false;
  }
  return conversations.length === 0;
}

/** The two steps that come before the first conversation. Step 3 is the new conversation itself. */
export type FirstStep = 'accounts' | 'look';

/** `firstRun` is step 1 of 3 and moves on to step 2. `manage` is only the account cards with a Close button: it saves
 * `setup` and nothing else, so reconnecting an account later can neither restart the first run nor reset the tutorial. */
export type SetupMode = 'firstRun' | 'manage';

/** Step 2 is due until an appearance with a `base` is saved. That covers new users after step 1 and, once, an install
 * that was saved by an older version (whose appearance has no `base`, so the engine hands the screen none). */
export function lookDue(settings: Pick<Settings, 'appearance'> | undefined): boolean {
  return !settings?.appearance?.base;
}

/** The step the window opens at: step 1 under the old rule, else step 2 when it is due, else none. */
export function firstRunStep(settings: Settings | undefined, conversations: readonly unknown[]): FirstStep | undefined {
  if (shouldOpenSetup(settings, conversations)) return 'accounts';
  return lookDue(settings) ? 'look' : undefined;
}

/** What `setup` saves, in either mode. Nothing else about the first run changes with it. */
export function setupPatch(result: 'done' | 'skipped'): Settings {
  return { setup: result };
}

export type SizeChoice = 'small' | 'normal' | 'large';
/** "Normal" is the theme's own size, so it is no saved value at all. */
const SIZES: Record<SizeChoice, number | undefined> = { small: 13, normal: undefined, large: 16 };

/** The appearance with a text size chosen; normal clears the field. */
export function withSize(appearance: Appearance, size: SizeChoice): Appearance {
  const next = { ...appearance };
  const px = SIZES[size];
  if (px === undefined) delete next.textSize;
  else next.textSize = px;
  return next;
}

/** Which "Aa" is on: none when the size was set to something else, such as 14.5 in Settings. */
export function sizeOf(appearance: Appearance): SizeChoice | undefined {
  return (Object.keys(SIZES) as SizeChoice[]).find((k) => SIZES[k] === appearance.textSize);
}

/** What Continue on step 2 saves, in one call. `tutorialDone` is left out when it is already true, so an install that
 * finished the tutorial is not taught again; no answer to the question leaves `experience` out, and counts as a beginner.
 * `name` is how the person asked to be called; empty leaves it out. */
export function continuePatch(
  appearance: Appearance,
  experience: Experience | undefined,
  tutorialDone: boolean | undefined,
  name?: string,
): Settings {
  const patch: Settings = { appearance };
  if (experience) patch.experience = experience;
  // The name is optional: a field left empty is a skip, and nothing is saved for it.
  if (name?.trim()) patch.userName = name;
  if (tutorialDone !== true) patch.tutorialDone = false;
  return patch;
}

/**
 * Pure selection logic:
 * Choosing 'none' deselects both subscriptions.
 * Choosing a subscription clears 'none' and toggles the subscription.
 */
export function toggleSubscription(
  selected: readonly Subscription[],
  option: Subscription,
): Subscription[] {
  if (option === 'none') {
    return selected.includes('none') ? [] : ['none'];
  }
  const withoutNone = selected.filter((s) => s !== 'none');
  if (withoutNone.includes(option)) {
    return withoutNone.filter((s) => s !== option);
  }
  return [...withoutNone, option];
}

/**
 * Computes the card state according to the setup contract:
 * - checking: check is not yet available
 * - notInstalled: not installed, action is ['install'] or ['officialPage'] based on canInstall
 * - installing: installation in progress
 * - signedOut: installed but not signed in, action is ['signIn']
 * - waiting: sign-in initiated, waiting for completion, action is ['checkAgain']
 * - connected: installed and signed in, no actions
 * Technical errors are translated with error.detail preserved for disclosure.
 */
export function setupCard(
  input: CardInput,
  translate: (key: string) => string,
): Card {
  let error: { message: string; detail?: string } | undefined;
  if (input.progress?.error) {
    error = {
      message: translate(input.progress.error),
      detail: input.progress.detail,
    };
  } else if (input.progress?.step === 'failed') {
    error = {
      message: translate('error.setup.install'),
      detail: input.progress.detail,
    };
  } else if (input.error) {
    const key = typeof input.error === 'string' ? input.error : input.error.key;
    const detail = typeof input.error === 'string' ? undefined : input.error.detail;
    error = { message: translate(key), detail };
  }

  const { check, progress, waiting } = input;

  if (progress && progress.step !== 'done' && progress.step !== 'failed') {
    return { status: 'installing', actions: [], error };
  }

  if (!check) {
    return { status: 'checking', actions: [], error };
  }

  if (!check.installed) {
    const actions = check.canInstall ? ['install'] : ['officialPage'];
    return { status: 'notInstalled', actions, error };
  }

  if (check.account === 'signedIn') {
    return { status: 'connected', actions: [], error };
  }

  if (waiting) {
    return { status: 'waiting', actions: ['checkAgain'], error };
  }

  return { status: 'signedOut', actions: ['signIn'], error };
}
