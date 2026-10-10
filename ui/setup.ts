import type { Vendor } from './api.ts';

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
