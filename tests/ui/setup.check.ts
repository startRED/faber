import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { continuePatch, firstRunStep, lookDue, setupCard, setupPatch, shouldOpenSetup, sizeOf, toggleSubscription, withSize, type Subscription } from '../../ui/setup.ts';
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
    'setup.details', 'setup.skip', 'setup.close', 'set.subscriptions', 'set.openSetup',
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

check('setup first run: step 2 is due exactly when the saved appearance has no base', () => {
  equal(lookDue(undefined), true);
  equal(lookDue({}), true);
  equal(lookDue({ appearance: { base: 'sala' } }), false);
  equal(lookDue({ appearance: { base: 'oficina', textSize: 16 } }), false);
  const saved = [{ id: 'saved-chat', title: 'A saved chat', area: 'chat', updatedAt: 1 }];
  // A new user: step 1, then step 2.
  equal(firstRunStep({}, []), 'accounts');
  // An install saved by an older version, with conversations and a finished guide: step 2 once.
  equal(firstRunStep({ setup: 'done' }, saved), 'look');
  equal(firstRunStep({ setup: 'skipped' }, []), 'look');
  // Step 1 is not due with saved conversations, but step 2 still is, and it is never due twice.
  equal(firstRunStep({}, saved), 'look');
  equal(firstRunStep({ setup: 'done', appearance: { base: 'sala' } }, saved), undefined);
  equal(firstRunStep({ appearance: { base: 'sala' } }, saved), undefined);
});

check('setup first run: normal text size clears the field, and the other two sizes are 13 and 16', () => {
  const sized = withSize({ base: 'sala', density: 0.8 }, 'large');
  deepEqual(sized, { base: 'sala', density: 0.8, textSize: 16 });
  deepEqual(withSize(sized, 'small'), { base: 'sala', density: 0.8, textSize: 13 });
  const normal = withSize(sized, 'normal');
  deepEqual(normal, { base: 'sala', density: 0.8 });
  equal('textSize' in normal, false, 'the field is gone, not undefined');
  deepEqual(sized, { base: 'sala', density: 0.8, textSize: 16 }, 'the appearance it was given is not changed');
  equal(sizeOf({ base: 'sala' }), 'normal');
  equal(sizeOf({ base: 'sala', textSize: 13 }), 'small');
  equal(sizeOf({ base: 'sala', textSize: 16 }), 'large');
  equal(sizeOf({ base: 'sala', textSize: 14.5 }), undefined, 'a size set in Settings is none of the three');
});

check('setup first run: the optional name is saved only when something was typed', () => {
  const look = { base: 'sala' } as const;
  deepEqual(continuePatch(look, undefined, undefined, 'Erick'), { appearance: look, tutorialDone: false, userName: 'Erick' });
  for (const empty of [undefined, '', '   ']) equal('userName' in continuePatch(look, undefined, undefined, empty), false, `"${empty}" is a skip`);
});

check('setup first run: Continue sends the choices in one patch, and leaves tutorialDone out when it is already true', () => {
  const look = { base: 'oficina', textSize: 16 };
  deepEqual(continuePatch(look, 'experienced', undefined), { appearance: look, experience: 'experienced', tutorialDone: false });
  deepEqual(continuePatch(look, 'beginner', false), { appearance: look, experience: 'beginner', tutorialDone: false });
  const kept = continuePatch(look, 'experienced', true);
  deepEqual(kept, { appearance: look, experience: 'experienced' });
  equal('tutorialDone' in kept, false);
  // No answer to the question is no saved answer, and counts as a beginner.
  deepEqual(continuePatch(look, undefined, undefined), { appearance: look, tutorialDone: false });
});

check('setup first run: the setup cards save the setup decision and never the appearance, experience or tutorial', () => {
  for (const result of ['done', 'skipped'] as const) {
    const patch = setupPatch(result);
    deepEqual(patch, { setup: result });
    for (const field of ['appearance', 'experience', 'tutorialDone']) equal(field in patch, false, field);
  }
});
