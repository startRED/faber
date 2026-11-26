import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type SharingState } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { SharingQuestion, SharingSettings } from '../../ui/Sharing.tsx';
import { finishSharingChoice, firstRunStep, type FirstStep } from '../../ui/setup.ts';
import { en } from '../../ui/locales/en.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { de } from '../../ui/locales/de.ts';
import { check, deepEqual, equal, frames, ok } from './check.ts';

check('sharing: first start asks once on new and updated installs, with two equal unselected choices', async () => {
  equal(firstRunStep({}, [], { choice: null }), 'sharing');
  equal(firstRunStep({ setup: 'done', appearance: { base: 'dark-room' } }, [{ id: 'existing' }], { choice: null }), 'sharing');
  equal(firstRunStep({ setup: 'done', appearance: { base: 'dark-room' } }, [{ id: 'existing' }], { choice: false }), undefined);
  equal(firstRunStep({}, [], undefined), 'accounts');
  const host = document.createElement('div');
  document.body.append(host);
  const choices: boolean[] = [];
  const dispose = render(() => createComponent(SharingQuestion, { onPick: async (yes) => { choices.push(yes); } }), host);
  try {
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('[data-testid="sharing-question"] button')];
    equal(buttons.length, 2);
    equal(buttons[0].className, buttons[1].className);
    ok(buttons.every((button) => !button.hasAttribute('aria-pressed') && !button.disabled));
    equal(choices.length, 0);
    buttons[1].click();
    await Promise.resolve();
    deepEqual(choices, [false]);
  } finally { dispose(); host.remove(); }
  for (const locale of [en, ptBR, es, fr, de]) {
    for (const key of ['sharing.question', 'sharing.yes', 'sharing.no', 'sharing.setting', 'sharing.show', 'sharing.on', 'sharing.off'] as const) ok(locale[key].trim());
  }
});

check('sharing: a failed choice save shows an error and continues, then asks on the next start', async () => {
  const settings = { setup: 'done' as const, appearance: { base: 'dark-room' } };
  const sharing: SharingState = { choice: null };
  let step: FirstStep | undefined = firstRunStep(settings, [], sharing);
  let reported: unknown;
  const failure = new Error('disk failure');
  await finishSharingChoice(async () => { throw failure; }, (error) => { reported = error; }, () => { step = firstRunStep(settings, []); });
  equal(reported, failure);
  equal(step, undefined, 'the shell can open after a failed save');
  equal(sharing.choice, null, 'the failed save did not change the saved choice');
  equal(firstRunStep(settings, [], sharing), 'sharing', 'the next start asks again');
});

check('sharing: a failed first-start save does not trap the real app', async () => {
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors: [], models: [], quotas: [], projects: [], conversations: [], version: '0.0.0', running: { revision: 0, items: [] },
    sharing: { choice: null }, settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: { vars: {}, scheme: 'dark', motion: 'none', effects, checks: [], notices: [], appearance: {
      base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14,
      density: 1, corners: 1, motion: 'none', effects,
    } },
  };
  const original = { sharingSet: api.sharingSet, setClock: api.setClock, recents: api.recents };
  api.sharingSet = async () => { throw new Error('disk failure'); };
  api.setClock = async () => {};
  api.recents = async () => ({ folders: [], environments: [] });
  const host = document.createElement('div');
  document.body.append(host);
  let dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    const button = host.querySelector<HTMLButtonElement>('[data-testid="sharing-question"] button');
    ok(button);
    button.click();
    await frames();
    equal(host.querySelector('[data-testid="sharing-question"]'), null, 'the app moves on');
    ok(host.querySelector('.toast[role="alert"]'), 'the error is shown normally');
    dispose();
    dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
    ok(host.querySelector('[data-testid="sharing-question"]'), 'the unsaved choice is asked on the next start');
  } finally {
    dispose(); host.remove();
    api.sharingSet = original.sharingSet; api.setClock = original.setClock; api.recents = original.recents;
  }
});

check('sharing: Privacy changes the choice and shows the literal next report', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const original = api.sharingPreview;
  const body = '{"version":"1.2.3","counters":{"appStarts":1}}';
  api.sharingPreview = async () => body;
  const [state, setState] = createSignal<SharingState>({ choice: false });
  const seen: boolean[] = [];
  const dispose = render(() => createComponent(SharingSettings, {
    get state() { return state(); },
    onChange: async (yes) => { seen.push(yes); setState({ choice: yes }); return true; },
  }), host);
  try {
    const toggle = host.querySelector<HTMLInputElement>('[role="switch"]');
    ok(toggle && !toggle.checked);
    toggle.click();
    await Promise.resolve();
    await Promise.resolve();
    deepEqual(seen, [true]);
    const button = [...host.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === en['sharing.show']);
    ok(button);
    button.click();
    await Promise.resolve();
    await Promise.resolve();
    equal(host.querySelector('[data-testid="sharing-report"]')?.textContent, body);
    toggle.click();
    await Promise.resolve();
    await Promise.resolve();
    deepEqual(seen, [true, false]);
    equal(host.querySelector('[data-testid="sharing-report"]'), null);
  } finally { api.sharingPreview = original; dispose(); host.remove(); }
});

check('sharing: changing the switch clears the preview even when saving fails, and reopening fetches again', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const original = api.sharingPreview;
  let reads = 0;
  api.sharingPreview = async () => `report-${++reads}`;
  const dispose = render(() => createComponent(SharingSettings, {
    state: { choice: true },
    onChange: async () => false,
  }), host);
  try {
    const show = () => [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === en['sharing.show']);
    show()!.click();
    await Promise.resolve(); await Promise.resolve();
    equal(host.querySelector('[data-testid="sharing-report"]')?.textContent, 'report-1');
    host.querySelector<HTMLInputElement>('[role="switch"]')!.click();
    await Promise.resolve(); await Promise.resolve();
    equal(host.querySelector('[data-testid="sharing-report"]'), null);
    show()!.click();
    await Promise.resolve(); await Promise.resolve();
    equal(host.querySelector('[data-testid="sharing-report"]')?.textContent, 'report-2');
    equal(reads, 2);
  } finally { api.sharingPreview = original; dispose(); host.remove(); }
});
