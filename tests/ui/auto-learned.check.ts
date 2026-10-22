import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Look, type Message } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { Settings } from '../../ui/Settings.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

const reply = (pace?: Message['pace']): Message =>
  ({ id: 'reply', role: 'assistant', text: 'Answer', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', effort: 'high', reason: 'reason.route.code.medium', pace }) as Message;

const show = (m: Message) => {
  const host = document.createElement('div');
  const dispose = render(() => createComponent(Thread, { conversationId: 'chat', messages: [m], from: 0, options: [], onBranch: () => {} }), host);
  return { host, dispose };
};

check('auto learned: an answer whose pick was lifted says so in its pace line, in every language, and no other does', () => {
  const words = { en: 'stronger model', 'pt-BR': 'mais forte', es: 'más fuerte', fr: 'plus fort', de: 'stärkeres' } as const;
  for (const [lang, word] of Object.entries(words)) {
    useLanguage(lang);
    const lifted = show(reply({ kind: 'learned', vendor: 'claude' }));
    const plain = show(reply());
    try {
      const line = lifted.host.querySelector('.pace');
      ok(line, `${lang}: the notice shows`);
      equal(line!.textContent, t('reason.pace.learned'));
      ok(line!.textContent!.includes(word) && !line!.textContent!.includes('{'), `${lang}: ${line!.textContent}`);
      // Where the model is already shown: after the header that names it, never as a panel of its own.
      ok(lifted.host.querySelector('.model')!.compareDocumentPosition(line!) & Node.DOCUMENT_POSITION_FOLLOWING);
      ok(lifted.host.querySelectorAll('.pace').length === 1);
      equal(plain.host.querySelector('.pace'), null, `${lang}: a pick nobody changed says nothing`);
    } finally {
      lifted.dispose();
      plain.dispose();
    }
  }
  useLanguage('en');
});

const look = {
  vars: {}, scheme: 'dark', motion: 'full', effects: { ambient: false, texture: false, glow: false }, checks: [], notices: [],
  appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'full', effects: { ambient: false, texture: false, glow: false } },
} as Look;

// The accounts section asks each CLI who is signed in when it opens; no real CLI is asked here.
const open = () => {
  const was = api.setupCheck;
  api.setupCheck = async (vendor) => ({ vendor, installed: true, account: 'signedIn', canInstall: false });
  // Solid hears clicks at the document, so the page has to hold the screen.
  const host = document.createElement('div');
  document.body.append(host);
  const props = { start: 'accounts', look, saved: {}, vendors: [], quotas: {}, now: 0, onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true };
  const stop = render(() => createComponent(Settings, props as never), host);
  return { host, dispose: () => { stop(); host.remove(); api.setupCheck = was; } };
};

check('auto learned: Accounts and limits has one Forget row for what Auto learned, and the button calls the engine once', async () => {
  useLanguage('en');
  const before = api.forgetAutoLearned;
  let calls = 0;
  api.forgetAutoLearned = async () => { calls += 1; };
  const { host, dispose } = open();
  try {
    const rows = host.querySelectorAll('[data-opt="set-auto-learned"]');
    equal(rows.length, 1, 'one row, with the limits Auto follows');
    equal(host.querySelector('[data-opt="set-auto-learned"]')!.closest('.st-group')!.querySelector('.st-title')!.textContent, 'Auto');
    ok(rows[0]!.textContent!.includes(t('set.autoLearned')) && rows[0]!.textContent!.includes(t('set.autoLearned.line')));
    ok(!rows[0]!.querySelector('input'), 'a button, not a switch: learning is on, with a notice');
    const button = rows[0]!.querySelector<HTMLButtonElement>('button.btn')!;
    equal(button.textContent, 'Forget');
    button.click();
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(calls, 1);
    equal(button.textContent, 'Forgotten');
    ok(button.disabled, 'it cannot be pressed twice in a row');
  } finally {
    dispose();
    api.forgetAutoLearned = before;
  }
});

check('auto learned: a refused Forget shows the error and the button stays', async () => {
  useLanguage('en');
  const before = api.forgetAutoLearned;
  api.forgetAutoLearned = async () => { throw 'error.autoLearned.forget'; };
  const { host, dispose } = open();
  try {
    const button = host.querySelector<HTMLButtonElement>('[data-opt="set-auto-learned"] button.btn')!;
    button.click();
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(host.querySelectorAll('[data-opt="set-auto-learned"] ~ .note.err').length, 1, 'the error shows under the row');
    equal(host.querySelectorAll('.note.err[role="alert"]').length, 1, 'and is announced');
    equal(host.querySelector('.note.err')!.textContent, t('error.autoLearned.forget'), 'with its own sentence, not a raw key');
    equal(button.textContent, 'Forget');
    ok(!button.disabled);
  } finally {
    dispose();
    api.forgetAutoLearned = before;
  }
});
