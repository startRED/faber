import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Permission } from '../../ui/api.ts';
import { Asks } from '../../ui/Ask.tsx';
import { catalogName, LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// Faber's own self-change card (guardrail 3): the agent asks to install a tool from the catalog, and the person decides with the
// source and the licence in front of them (docs/product-map.md, "Permission card, held-back request").

const settle = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));

const INSTALL: Permission = {
  requestId: 'toolu_i1',
  tool: 'faber.tools.install',
  input: { kind: 'skill', id: 'ponytail', repo: 'DietrichGebert/ponytail', license: 'MIT', version: '09c434a', server: false },
};

async function withCard(permission: Permission, body: (root: HTMLElement, calls: unknown[][]) => Promise<void>, width = '720px') {
  const originals = { ...api };
  const calls: unknown[][] = [];
  api.answer = async (...args) => void calls.push(args);
  const root = document.createElement('div');
  root.style.width = width;
  document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId: 'c1', turnId: 't1', permissions: [permission] }), root);
  try {
    await settle();
    await body(root, calls);
  } finally {
    dispose();
    root.remove();
    Object.assign(api, originals);
  }
}

const buttons = (root: HTMLElement) => [...root.querySelectorAll<HTMLButtonElement>('.ask .btn')];

check('install card: the licence and the source come before the buttons, and the buttons answer once, always or no', () => withCard(INSTALL, async (root, calls) => {
  useLanguage('en');
  await settle();
  const card = root.querySelector<HTMLElement>('.ask')!;
  ok(card, 'a card');
  equal(card.getAttribute('role'), 'alertdialog');
  equal(card.querySelector('b')!.textContent, t('ask.self.install', { name: catalogName('ponytail') }), 'named from the catalog, not by the agent');
  const line = card.querySelector<HTMLElement>('[data-testid="self-line"]')!;
  equal(line.textContent, 'Skill 09c434a from DietrichGebert/ponytail, licence MIT');
  ok(!card.querySelector('[data-testid="self-code"]'), 'a skill runs no code of its own: no code line');
  const labels = buttons(root).map((b) => b.textContent!.trim());
  deepEqual(labels, [t('ask.self.once'), t('ask.self.always'), t('ask.deny')]);
  buttons(root).forEach((button) => ok(line.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING, 'the licence and source precede every button'));
  ok(!card.querySelector('pre'), 'no raw input of the agent is shown');
  buttons(root)[0]!.click();
  await settle();
  buttons(root)[1]!.click();
  await settle();
  buttons(root)[2]!.click();
  await settle();
  deepEqual(calls, [
    ['c1', 't1', 'toolu_i1', true, false],
    ['c1', 't1', 'toolu_i1', true, true],
    ['c1', 't1', 'toolu_i1', false, false],
  ]);
}));

check('install card: written in every language, and every button and line stays inside a 720 px column', async () => {
  for (const { code } of LANGUAGES) {
    await withCard(INSTALL, async (root) => {
      useLanguage(code);
      await settle();
      const card = root.querySelector<HTMLElement>('.ask')!;
      const bounds = card.getBoundingClientRect();
      ok(bounds.width > 0 && bounds.width <= 720, `${code}: the card fits its column (${bounds.width}px)`);
      const title = t('ask.self.install', { name: catalogName('ponytail') });
      equal(card.querySelector('b')!.textContent, title, `${code} title`);
      ok(card.textContent!.includes('DietrichGebert/ponytail') && card.textContent!.includes('MIT'), `${code}: source and licence are shown`);
      for (const button of buttons(root)) {
        const box = button.getBoundingClientRect();
        ok(box.right <= bounds.right + 0.5 && box.left >= bounds.left - 0.5, `${code}: "${button.textContent}" stays inside the card`);
      }
      if (code !== 'en') ok(!card.textContent!.includes('Allow once'), `${code} is not English`);
    });
  }
  useLanguage('en');
});

check('install card: every word is the screen’s own, so what the agent sends is shown as text and never as the card’s words', () => withCard(
  { requestId: 'r', tool: 'faber.tools.install', input: { kind: '<img src=x>', id: '<img src=x onerror=alert(1)>', repo: '<b>x</b>', license: '<script>', server: 'yes' } },
  async (root) => {
    useLanguage('en');
    await settle();
    ok(!root.querySelector('img, script') && root.querySelectorAll('.ask b').length === 1, 'nothing the input holds becomes an element');
    ok(root.querySelector('.ask')!.textContent!.includes('<b>x</b>'), 'it is shown as the text it is');
    ok(root.querySelector('[data-testid="self-line"]')!.textContent!.startsWith('Skill '), 'an unknown kind reads as a skill');
    ok(!root.querySelector('[data-testid="self-code"]'), 'only a true flag says it runs code');
  },
));

check('install card: a tool server, or a plugin that brings one, says once that it runs code on this PC, in every language', async () => {
  const server: Permission = { requestId: 's', tool: 'faber.tools.install', input: { kind: 'server', id: 'ponytail', repo: 'a/b', license: 'MIT', version: '1.2.3', server: true, code: true } };
  for (const { code } of LANGUAGES) {
    await withCard(server, async (root) => {
      useLanguage(code);
      await settle();
      const line = root.querySelector<HTMLElement>('[data-testid="self-code"]');
      ok(line && line.textContent === t('ask.self.code'), `${code}: the code line is shown`);
      ok(root.querySelector('[data-testid="self-line"]')!.textContent!.includes('1.2.3'), `${code}: the version is shown`);
      const bounds = root.querySelector('.ask')!.getBoundingClientRect();
      ok(bounds.width > 0 && bounds.width <= 720, `${code}: the card fits its column`);
    });
  }
  useLanguage('en');
});

check('install card: another tool’s request keeps the ordinary card with its own words', () => withCard(
  { requestId: 'r2', tool: 'Bash', input: { command: 'ls' } },
  async (root) => {
    useLanguage('en');
    await settle();
    deepEqual(buttons(root).map((b) => b.textContent!.trim()), [t('ask.allow'), t('ask.always'), t('ask.deny')]);
    ok(!root.querySelector('[data-testid="self-line"]'));
  },
));

check('catalog_switch: explicit state, separate consent and new-conversation notice in every language', async () => {
  for (const language of LANGUAGES) {
    useLanguage(language.code);
    for (const enabled of [true, false]) {
      for (const width of [360, 720]) {
      await withCard({ ...INSTALL, tool: 'faber.tools.switch', input: { ...INSTALL.input as Record<string, unknown>, enabled } }, async (root) => {
        const card = root.querySelector<HTMLElement>('.ask')!;
        equal(card.querySelector('b')!.textContent, t(enabled ? 'ask.self.enable' : 'ask.self.disable', { name: catalogName('ponytail') }));
        equal(card.querySelector('[data-testid=self-effect]')?.textContent, t('ask.self.newConversations'), 'the effect is its own paragraph, separate from the catalog description');
        const bounds = card.getBoundingClientRect();
        ok(bounds.width > 0 && bounds.width <= width, `${language.code}: the card fits the ${width}px column`);
        for (const button of buttons(root)) {
          const box = button.getBoundingClientRect();
          ok(box.left >= bounds.left - 0.5 && box.right <= bounds.right + 0.5, `${language.code}: buttons stay inside the card`);
        }
        deepEqual(buttons(root).map((b) => b.textContent!.trim()), [t('ask.self.once'), t('ask.self.alwaysSwitch'), t('ask.deny')]);
        ok(!card.querySelector('pre'), 'the agent cannot write the consent text');
      }, `${width}px`);
      }
    }
  }
  useLanguage('en');
});

check('catalog_remove: card: its own consent names Undo and preserves current conversation tools in every language', async () => {
  for (const { code } of LANGUAGES) {
    for (const width of ['360px', '720px']) {
    await withCard({ ...INSTALL, tool: 'faber.tools.remove' }, async (root, calls) => {
      useLanguage(code);
      await settle();
      equal(root.querySelector('.ask b')!.textContent, t('ask.self.remove', { name: catalogName('ponytail') }));
      equal(root.querySelector('[data-testid="self-effect"]')!.textContent, t('ask.self.removeEffect'));
      deepEqual(buttons(root).map((b) => b.textContent!.trim()), [t('ask.self.once'), t('ask.self.alwaysRemove'), t('ask.deny')]);
      const bounds = root.querySelector<HTMLElement>('.ask')!.getBoundingClientRect();
      for (const button of buttons(root)) ok(button.getBoundingClientRect().right <= bounds.right + 1, `${code} ${width}: buttons fit`);
      buttons(root)[0]!.click();
      await settle();
      deepEqual(calls, [['c1', 't1', 'toolu_i1', true, false]]);
    }, width);
    }
  }
  useLanguage('en');
});
