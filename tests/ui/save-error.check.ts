import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('save error: a conversation the engine could not write shows in the same toast as a refused command', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    equal(host.querySelector('.toast'), null, 'nothing is shown while every save works');
    hear({ type: 'saveFailed', key: 'error.chat.saveFailed' });
    await settle();
    equal(host.querySelector('.toast span')?.textContent, t('error.chat.saveFailed'), 'the toast says what the engine key says');
    ok(host.querySelector('.toast[role="alert"]'), 'it is the one alert toast, not a second one');
    host.querySelector<HTMLButtonElement>('.toast button')!.click();
    await settle();
    equal(host.querySelector('.toast'), null, 'dismissing it closes it');
    hear({ type: 'saveFailed', key: 'error.chat.saveFull' });
    await settle();
    equal(host.querySelector('.toast span')?.textContent, t('error.chat.saveFull'), 'a full disk has its own words');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
  }
});
