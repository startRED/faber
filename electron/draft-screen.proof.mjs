// Bundled only by the hidden regression proof, never by the shipped screen.
import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Composer } from '../ui/Composer.tsx';
import { App } from '../ui/App.tsx';
import { api } from '../ui/api.ts';
import { dirtyCount, kept } from '../ui/unsent.ts';
import { store } from '../ui/running.ts';

export { dirtyCount, kept };

export function mountComposer(key) {
  api.draftDirty = async () => {};
  const root = document.body.appendChild(document.createElement('div'));
  return render(() => createComponent(Composer, {
    draftKey: key, vendors: [], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice() {}, onStop() {}, onWarm() {}, onPause() {}, onSend() {},
  }), root);
}

export async function restartWithFailedDraft() {
  localStorage.removeItem('faber.place');
  const initial = {
    conversations: [], projects: [], vendors: ['claude'], models: [], quotas: [{ vendor: 'claude', window: 'five_hour', used: 0.1, resetsAt: Date.now() / 1000 + 3600 }], usage: [], version: 'test',
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 100, items: [] }, background: { revision: 100, items: [] },
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  };
  let reads = 0;
  let hostDirty = false;
  let waitForState = false;
  let releaseState;
  Object.assign(api, {
    state: async () => {
      reads++;
      const snapshot = { ...initial, quotas: initial.quotas.map((q) => ({ ...q, used: 0.8 })), running: { revision: 0, items: [] }, background: { revision: 0, items: [] } };
      if (waitForState) await new Promise((resolve) => { releaseState = resolve; });
      return snapshot;
    },
    draftDirty: async (dirty) => { hostDirty = dirty; }, warm: async () => {}, routeDraft: async () => {}, setClock: async () => {},
    setShellTexts: async () => {}, asks: async () => [],
  });
  store.offer(initial.running);
  let hear;
  const root = document.body.appendChild(document.createElement('div'));
  const dispose = render(() => createComponent(App, { initial, inbox: { take(listener) { hear = listener; } } }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 50));
  const original = Storage.prototype.setItem;
  try {
    await settle();
    const field = root.querySelector('.composer textarea');
    if (!field) throw new Error('the real composer must mount');
    Storage.prototype.setItem = function(key, value) {
      if (key === 'faber.unsent.new-chat-') throw new Error('injected draft quota failure');
      return original.call(this, key, value);
    };
    field.value = 'newer words that could not be saved';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    if (kept('new-chat-') !== field.value || !dirtyCount()) throw new Error('fixture must establish the failed in-memory write');
    const before = reads;
    hostDirty = false; // The replacement engine has no renderer draft state yet.
    hear({ type: 'lagged' });
    await settle();
    if (reads <= before) throw new Error('replacement engine state must refresh without reloading');
    if (root.querySelector('.composer textarea') !== field || field.value !== 'newer words that could not be saved') throw new Error('engine recovery must retain the live composer and newer text');
    if (kept('new-chat-') !== field.value || !dirtyCount()) throw new Error('engine recovery must retain failed-write protection');
    if (!hostDirty) throw new Error('replacement updater must learn about the retained unsaved draft');
    if (store.list().revision !== 0) throw new Error('replacement engine revisions must replace the old process revisions');
    if (!root.querySelector('.vendor')?.textContent.includes('80%')) throw new Error('recovery must refresh visible quota readings too');
    waitForState = true;
    hear({ type: 'lagged' });
    hear({ type: 'quota', ...initial.quotas[0], used: 0.9 });
    releaseState();
    await settle();
    if (!root.querySelector('.vendor')?.textContent.includes('90%')) throw new Error('a newer quota event must win over the recovery snapshot');
  } finally { Storage.prototype.setItem = original; dispose(); root.remove(); }
}
