import { createComponent } from 'solid-js';
import { api } from '../../ui/api.ts';
import { render } from 'solid-js/web';
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { noteSetupProgress, Setup } from '../../ui/Setup.tsx';
import { continuePatch, firstRunStep, lookDue, setupCard, setupPatch, shouldOpenSetup, toggleSubscription, type Subscription } from '../../ui/setup.ts';
import { check, deepEqual, doesNotMatch, equal, notEqual, ok } from './check.ts';
import { t } from '../../ui/i18n.ts';

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

check('setup C4.5: the empty state tells people to connect without asking them to reopen Faber or naming a Settings path', () => {
  for (const [language, messages] of Object.entries(languages)) {
    const body = text(messages, 'empty.noAiBody');
    doesNotMatch(body, /reopen|restart|relaunch|reiniciar|redémarrer|neu starten|reabra|reabrir|rouvrez/i, `${language}: connecting a subscription must work in the running app`);
    ok(!body.includes('→'), `${language}: no path through Settings`);
    ok(!body.includes(text(messages, 'settings.open')) && !body.includes(text(messages, 'set.subscriptions')), `${language}: the button goes to Settings, the line does not name it`);
  }
});

check('setup C4.6: all five languages fill every guide key and preserve English placeholders', () => {
  const mandatory = [
    'setup.title', 'setup.body', 'setup.claude', 'setup.chatgpt', 'setup.none',
    'setup.checking', 'setup.notInstalled', 'setup.install', 'setup.retry', 'reason.setup.connection', 'reason.setup.blocked', 'reason.setup.checksum', 'reason.setup.write', 'reason.setup.timeout', 'setup.officialPage', 'setup.installing',
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

check('setup: the AI that is picked is filled and ticked, and the other ones are not; one name for Codex', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Setup, { mode: 'manage', onFinish: () => {} }), host);
  try {
    const picks = [...host.querySelectorAll<HTMLButtonElement>('.pick')];
    equal(picks.length, 2, 'opened later, the cards offer the two AIs and no "None for now": Close says it');
    ok(picks.every((b) => b.getAttribute('aria-pressed') === 'false' && !b.querySelector('.tick')), 'nothing is picked at first');
    const codex = picks.find((b) => b.textContent!.includes('Codex'))!;
    ok(codex, 'Codex is called Codex');
    codex.click();
    const now = [...host.querySelectorAll<HTMLButtonElement>('.pick')];
    const on = now.filter((b) => b.getAttribute('aria-pressed') === 'true');
    equal(on.length, 1);
    ok(on[0]!.querySelector('.tick svg, .tick .i'), 'the picked AI shows a check, not only a colour');
    equal(now.filter((b) => b.querySelector('.tick')).length, 1);
    // The AI is Codex everywhere; only the line naming the plan it runs on says ChatGPT.
    const names = [...host.querySelectorAll('.pick, .setup-card h2')].map((e) => e.textContent).join(' ');
    ok(!/ChatGPT/.test(names), `no ChatGPT as a name on the cards: ${names}`);
  } finally { dispose(); host.remove(); }
});

check('setup: closing the cards while an AI installs and opening them again shows it still installing', () => {
  noteSetupProgress({ type: 'setup', vendor: 'codex', step: 'download' });
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Setup, { mode: 'manage', onFinish: () => {} }), host);
  try {
    const codex = [...host.querySelectorAll<HTMLButtonElement>('.pick')].find((b) => b.textContent!.includes('Codex'))!;
    equal(codex.getAttribute('aria-pressed'), 'true', 'the AI still installing is picked already');
    ok(host.querySelector('.setup-card[data-vendor="codex"] .setup-badge.installing'), 'its card says it is installing');
    ok(host.querySelector('.setup-card[data-vendor="codex"] .setup-step'), 'with the step it reached');
  } finally {
    dispose();
    host.remove();
    noteSetupProgress({ type: 'setup', vendor: 'codex', step: 'done' });
  }
});

check('setup first run: "None for now" sits beside the two AIs, and the line says which plan each one uses', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Setup, { mode: 'firstRun', onFinish: () => {} }), host);
  try {
    const picks = [...host.querySelectorAll<HTMLButtonElement>('.pick')];
    equal(picks.length, 3);
    equal(picks[2]!.textContent!.trim(), t('setup.none'));
    const line = host.querySelector('.sub')?.textContent ?? '';
    ok(line.includes('Pro') && line.includes('ChatGPT'), `the line names the plans: ${line}`);
  } finally {
    dispose();
    host.remove();
  }
});

check('setup: a failed install says what to do and offers the official page beside trying again', () => {
  const check = { vendor: 'claude', installed: false, canInstall: true, account: 'unknown' } as unknown as Parameters<typeof setupCard>[0]['check'];
  deepEqual(setupCard({ check }, (k) => k).actions, ['install'], 'before any failure, one action');
  const failed = setupCard({ check, error: { key: 'error.setup.download' } }, (k) => k);
  deepEqual(failed.actions, ['install', 'officialPage']);
  for (const key of ['error.setup.download', 'error.setup.install'] as const) {
    ok(en[key].includes('official page'), `the message says how to go on: ${en[key]}`);
  }
});

check('setup: each install failure shows its message and retry starts again in both modes', async () => {
  const oldCheck = api.setupCheck, oldInstall = api.setupInstall;
  let attempts = 0;
  api.setupCheck = async (vendor) => ({ vendor, installed: false, account: 'unknown', canInstall: true });
  api.setupInstall = async () => { attempts++; };
  try {
    for (const mode of ['firstRun', 'manage'] as const) {
      const host = document.createElement('div');
      document.body.append(host);
      noteSetupProgress({ type: 'setup', vendor: 'claude', step: 'done' });
      const dispose = render(() => createComponent(Setup, { mode, onFinish: () => {} }), host);
      try {
        host.querySelector<HTMLButtonElement>('.pick')!.click();
        await Promise.resolve();
        for (const key of ['reason.setup.connection', 'reason.setup.blocked', 'reason.setup.checksum', 'reason.setup.write', 'reason.setup.timeout'] as const) {
          noteSetupProgress({ type: 'setup', vendor: 'claude', step: 'failed', error: key });
          equal(host.querySelector('.setup-error-msg')?.textContent, t(key));
          const retry = host.querySelector<HTMLButtonElement>('.setup-card-actions .primary')!;
          equal(retry.textContent?.trim(), t('setup.retry'));
          const before = attempts;
          retry.click();
          equal(attempts, before + 1);
          equal(host.querySelector('.setup-error-msg'), null);
        }
      } finally { dispose(); host.remove(); noteSetupProgress({ type: 'setup', vendor: 'claude', step: 'done' }); }
    }
  } finally { api.setupCheck = oldCheck; api.setupInstall = oldInstall; }
});
