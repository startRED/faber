import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { en } from '../ui/locales/en.ts';
import { ptBR } from '../ui/locales/pt-BR.ts';
import { es } from '../ui/locales/es.ts';
import { fr } from '../ui/locales/fr.ts';
import { de } from '../ui/locales/de.ts';

// Desired pure boundary for ui/setup.ts. Import the whole module only when present so a missing
// implementation fails an assertion, rather than preventing this file's other proofs from running.
type Subscription = 'claude' | 'codex' | 'none';
type Check = {
  vendor: 'claude' | 'codex';
  installed: boolean;
  account: 'signedIn' | 'signedOut' | 'unknown';
  canInstall: boolean;
};
type CardInput = {
  check?: Check;
  progress?: { type: 'setup'; vendor: 'claude' | 'codex'; step: string; error?: string; detail?: string };
};
type Card = {
  status: string;
  actions: string[];
  error?: { message: string; detail?: string };
};
interface SetupProofApi {
  shouldOpenSetup: (settings: { setup?: 'done' | 'skipped' }, conversations: readonly unknown[]) => boolean;
  toggleSubscription: (selected: readonly Subscription[], option: Subscription) => Subscription[];
  setupCard: (input: CardInput, translate: (key: string) => string) => Card;
}
const setupUrl = new URL('../ui/setup.ts', import.meta.url);
const setup: Partial<SetupProofApi> = existsSync(setupUrl) ? await import(setupUrl.href) : {};
function required<K extends keyof SetupProofApi>(name: K): SetupProofApi[K] {
  const fn = setup[name];
  assert.equal(typeof fn, 'function', `ui/setup.ts must export ${name} for the guide's pure logic`);
  return fn!;
}

const languages: Record<string, Record<string, string | { one: string; other: string }>> = { en, 'pt-BR': ptBR, es, fr, de };
function text(messages: Record<string, unknown>, key: string): string {
  const value = messages[key];
  assert.equal(typeof value, 'string', `${key} must have a translated sentence`);
  assert.ok((value as string).trim(), `${key} must not be empty`);
  return value as string;
}

test('C4.1: only a first launch without saved conversations automatically opens the guide', () => {
  const shouldOpenSetup = required('shouldOpenSetup');
  const savedChat = [{ id: 'saved-chat', title: 'A saved chat', area: 'chat', updatedAt: 1 }];
  const savedProject = [{ id: 'saved-project-chat', title: 'Project work', area: 'project', projectId: 'project', updatedAt: 1 }];
  for (const { settings, conversations, want } of [
    { settings: {}, conversations: [], want: true },
    { settings: { setup: 'done' as const }, conversations: [], want: false },
    { settings: { setup: 'skipped' as const }, conversations: [], want: false },
    { settings: {}, conversations: savedChat, want: false },
    { settings: {}, conversations: savedProject, want: false },
    { settings: { setup: 'done' as const }, conversations: savedChat, want: false },
    { settings: { setup: 'skipped' as const }, conversations: savedProject, want: false },
  ]) {
    assert.equal(shouldOpenSetup(settings, conversations), want, JSON.stringify({ settings, conversations }));
  }
});

test('C4.2: choosing none clears both subscriptions and choosing either subscription clears none', () => {
  const toggleSubscription = required('toggleSubscription');
  const both = Object.freeze(['claude', 'codex'] as const);
  const none = toggleSubscription(both, 'none');
  assert.deepEqual(none, ['none']);
  assert.deepEqual(both, ['claude', 'codex'], 'selection must not mutate its input');
  assert.deepEqual(toggleSubscription(none, 'claude'), ['claude']);
  assert.deepEqual(toggleSubscription(none, 'codex'), ['codex']);
  assert.deepEqual(toggleSubscription(['claude'], 'codex'), ['claude', 'codex']);
  assert.deepEqual(toggleSubscription(['claude', 'codex'], 'claude'), ['codex']);
});

test('C4.3: an installed signed-in subscription has a connected card without install or sign-in actions', () => {
  const setupCard = required('setupCard');
  for (const vendor of ['claude', 'codex'] as const) {
    for (const canInstall of [true, false]) {
      const card = setupCard({ check: { vendor, installed: true, account: 'signedIn', canInstall } }, (key) => text(en, key));
      assert.equal(card.status, 'connected');
      assert.deepEqual(card.actions, [], 'an already connected subscription needs no setup action');
      assert.equal(card.error, undefined);
    }
  }
});

test('C4.4: a failed installation translates the error key and keeps technical detail separate', () => {
  const setupCard = required('setupCard');
  const detail = 'checksum mismatch: expected abcd, received 1234; C:\\temporary\\download.exe';
  for (const [language, messages] of Object.entries(languages)) {
    const key = 'error.setup.checksum';
    const sentence = text(messages, key);
    const card = setupCard({
      check: { vendor: 'claude', installed: false, account: 'unknown', canInstall: true },
      progress: { type: 'setup', vendor: 'claude', step: 'failed', error: key, detail },
    }, (translationKey) => text(messages, translationKey));
    assert.ok(card.error, `${language}: the failed step must expose an error`);
    assert.equal(card.error.message, sentence, `${language}: show the translated sentence`);
    assert.notEqual(card.error.message, key);
    assert.equal(card.error.message.includes(detail), false, 'technical diagnostics belong in the details disclosure');
    assert.equal(card.error.detail, detail, 'preserve the diagnostic for the details disclosure');
  }
});

test('C4.5: the empty state directs people to Settings subscriptions without asking them to reopen Faber', () => {
  const body = en['empty.noAiBody'];
  assert.doesNotMatch(body, /reopen|restart|relaunch/i, 'connecting a subscription must work in the running app');
  assert.ok(body.includes(en['settings.open']), 'the empty state must name Settings');
  assert.ok(body.includes(text(en, 'set.subscriptions')), 'the empty state must name the Subscriptions section');
});

test('C4.6: all five languages fill every guide key and preserve English placeholders', () => {
  const mandatory = [
    'setup.title', 'setup.body', 'setup.claude', 'setup.chatgpt', 'setup.none',
    'setup.checking', 'setup.notInstalled', 'setup.install', 'setup.officialPage', 'setup.installing',
    'setup.step.prerequisite', 'setup.step.download', 'setup.step.verify', 'setup.step.install', 'setup.step.done',
    'setup.signedOut', 'setup.signIn', 'setup.waiting', 'setup.checkAgain', 'setup.connected',
    'setup.details', 'setup.skip', 'setup.done', 'set.subscriptions', 'set.openSetup',
    'error.setup.download', 'error.setup.checksum', 'error.setup.install',
    'error.setup.unsupported', 'error.setup.signIn', 'error.setup.check',
  ];
  const isGuideKey = (key: string) => key.startsWith('setup.') || key.startsWith('error.setup.') || key.startsWith('set.subscriptions') || key === 'set.openSetup';
  const keys = new Set([...mandatory, ...Object.values(languages).flatMap((messages) => Object.keys(messages).filter(isGuideKey))]);
  const slots = (sentence: string) => [...sentence.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
  for (const key of keys) {
    const source = en as Record<string, string | { one: string; other: string }>;
    assert.ok(source[key] !== undefined, `English must define ${key}`);
    const forms = (entry: unknown): string[] => {
      if (typeof entry === 'string') return [entry];
      assert.ok(entry && typeof entry === 'object', `${key} must contain text`);
      const plural = entry as { one: unknown; other: unknown };
      assert.equal(typeof plural.one, 'string', `${key}.one must contain text`);
      assert.equal(typeof plural.other, 'string', `${key}.other must contain text`);
      return [plural.one as string, plural.other as string];
    };
    const englishForms = forms(source[key]);
    for (const [language, messages] of Object.entries(languages)) {
      const translated = forms(messages[key]);
      assert.equal(translated.length, englishForms.length, `${language} ${key}: preserve plural forms`);
      for (const [index, sentence] of translated.entries()) {
        assert.ok(sentence.trim(), `${language} ${key} is empty`);
        assert.deepEqual(slots(sentence), slots(englishForms[index]), `${language} ${key}: preserve every placeholder`);
      }
    }
  }
});
