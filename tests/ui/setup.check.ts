import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { setupCard, shouldOpenSetup, toggleSubscription, type Subscription } from '../../ui/setup.ts';
import { check, deepEqual, doesNotMatch, equal, notEqual, ok } from './check.ts';

const languages: Record<string, Record<string, string | { one: string; other: string }>> = { en, 'pt-BR': ptBR, es, fr, de };
function text(messages: Record<string, unknown>, key: string): string {
  const value = messages[key];
  equal(typeof value, 'string', `${key} must have a translated sentence`);
  ok((value as string).trim(), `${key} must not be empty`);
  return value as string;
}

check('setup C4.1: only a first launch without saved conversations automatically opens the guide', () => {
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
    equal(shouldOpenSetup(settings, conversations), want, JSON.stringify({ settings, conversations }));
  }
});

check('setup C4.2: choosing none clears both subscriptions and choosing either subscription clears none', () => {
  const both: readonly Subscription[] = Object.freeze(['claude', 'codex'] as const);
  const none = toggleSubscription(both, 'none');
  deepEqual(none, ['none']);
  deepEqual(both, ['claude', 'codex'], 'selection must not mutate its input');
  deepEqual(toggleSubscription(none, 'claude'), ['claude']);
  deepEqual(toggleSubscription(none, 'codex'), ['codex']);
  deepEqual(toggleSubscription(['claude'], 'codex'), ['claude', 'codex']);
  deepEqual(toggleSubscription(['claude', 'codex'], 'claude'), ['codex']);
});

check('setup C4.3: an installed signed-in subscription has a connected card without install or sign-in actions', () => {
  for (const vendor of ['claude', 'codex'] as const) {
    for (const canInstall of [true, false]) {
      const card = setupCard({ check: { vendor, installed: true, account: 'signedIn', canInstall } }, (key) => text(en, key));
      equal(card.status, 'connected');
      deepEqual(card.actions, [], 'an already connected subscription needs no setup action');
      equal(card.error, undefined);
    }
  }
});

check('setup C4.4: a failed installation translates the error key and keeps technical detail separate', () => {
  const detail = 'checksum mismatch: expected abcd, received 1234; C:\\temporary\\download.exe';
  for (const [language, messages] of Object.entries(languages)) {
    const key = 'error.setup.checksum';
    const sentence = text(messages, key);
    const card = setupCard(
      {
        check: { vendor: 'claude', installed: false, account: 'unknown', canInstall: true },
        progress: { type: 'setup', vendor: 'claude', step: 'failed', error: key, detail },
      },
      (translationKey) => text(messages, translationKey),
    );
    ok(card.error, `${language}: the failed step must expose an error`);
    equal(card.error.message, sentence, `${language}: show the translated sentence`);
    notEqual(card.error.message, key);
    equal(card.error.message.includes(detail), false, 'technical diagnostics belong in the details disclosure');
    equal(card.error.detail, detail, 'preserve the diagnostic for the details disclosure');
  }
});

check('setup C4.5: the empty state directs people to Settings subscriptions without asking them to reopen Faber', () => {
  const body = en['empty.noAiBody'];
  doesNotMatch(body, /reopen|restart|relaunch/i, 'connecting a subscription must work in the running app');
  ok(body.includes(en['settings.open']), 'the empty state must name Settings');
  ok(body.includes(text(en, 'set.subscriptions')), 'the empty state must name the Subscriptions section');
});

check('setup C4.6: all five languages fill every guide key and preserve English placeholders', () => {
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
    ok(source[key] !== undefined, `English must define ${key}`);
    const forms = (entry: unknown): string[] => {
      if (typeof entry === 'string') return [entry];
      ok(entry && typeof entry === 'object', `${key} must contain text`);
      const plural = entry as { one: unknown; other: unknown };
      equal(typeof plural.one, 'string', `${key}.one must contain text`);
      equal(typeof plural.other, 'string', `${key}.other must contain text`);
      return [plural.one as string, plural.other as string];
    };
    const englishForms = forms(source[key]);
    for (const [language, messages] of Object.entries(languages)) {
      const translated = forms(messages[key]);
      equal(translated.length, englishForms.length, `${language} ${key}: preserve plural forms`);
      for (const [index, sentence] of translated.entries()) {
        ok(sentence.trim(), `${language} ${key} is empty`);
        deepEqual(slots(sentence), slots(englishForms[index]), `${language} ${key}: preserve every placeholder`);
      }
    }
  }
});
