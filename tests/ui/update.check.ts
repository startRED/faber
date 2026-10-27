import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { UpdateStatus } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { en } from '../../ui/locales/en.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { UpdateLine, UpdateStage, updateText } from '../../ui/UpdateStage.tsx';
import { check, equal, ok } from './check.ts';

// The update's progress (ui/UpdateStage.tsx): the sidebar's quiet line and the opening screen, in every state.

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

/** Draws `Part` on the page for a status the test can change, and takes it away after. */
async function drawn(Part: typeof UpdateLine | typeof UpdateStage, first: UpdateStatus, body: (host: HTMLElement, set: (s: UpdateStatus) => void) => Promise<void>) {
  const host = document.createElement('div');
  document.body.append(host);
  const [status, set] = createSignal(first);
  const dispose = render(() => createComponent(Part, { get status() { return status(); } }), host);
  try {
    await settle();
    await body(host, set);
    await settle();
  } finally {
    dispose();
    host.remove();
  }
}

check('update: the words say a percent when the size is known, the megabytes when it is not, and Installing at the end', () => {
  equal(updateText({ phase: 'checking' }), t('update.checking'));
  equal(updateText({ phase: 'downloading' }), t('update.downloading'));
  equal(updateText({ phase: 'downloading', percent: 42 }), t('update.downloadingPercent', { percent: 42 }));
  ok(updateText({ phase: 'downloading', percent: 42 }).includes('42'), 'the percent is in the line');
  equal(updateText({ phase: 'downloading', mb: 12 }), t('update.downloadingSize', { mb: 12 }));
  ok(updateText({ phase: 'downloading', mb: 12 }).includes('12'), 'the megabytes are in the line');
  equal(updateText({ phase: 'downloading', percent: 0 }), t('update.downloadingPercent', { percent: 0 }), '0% is a percent, not "no size"');
  equal(updateText({ phase: 'installing' }), t('update.installing'));
  equal(updateText({ phase: 'ready' }), '');
  equal(updateText({ phase: 'none' }), '');
});

check('update: the sidebar line follows the download and the install, and is quiet before and after', () =>
  drawn(UpdateLine, { phase: 'none' }, async (host, set) => {
    equal(host.querySelector('.upd'), null, 'nothing when there is no update');
    set({ phase: 'checking' });
    await settle();
    equal(host.querySelector('.upd'), null, 'looking for one says nothing');
    set({ phase: 'downloading', percent: 42 });
    await settle();
    const line = host.querySelector<HTMLElement>('.upd')!;
    ok(line, 'the line is there');
    equal(line.getAttribute('role'), 'status');
    equal(line.textContent, t('update.downloadingPercent', { percent: 42 }));
    set({ phase: 'downloading', mb: 12 });
    await settle();
    equal(host.querySelector('.upd')!.textContent, t('update.downloadingSize', { mb: 12 }));
    set({ phase: 'installing' });
    await settle();
    equal(host.querySelector('.upd')!.textContent, t('update.installing'));
    set({ phase: 'ready' });
    // The line shrinks away over --dur-exit (180 ms) before it leaves the page.
    await new Promise((resolve) => setTimeout(resolve, 400));
    equal(host.querySelector('.upd'), null, 'a kept update has its own control, not this line');
  }));

check('update: the opening screen shows the mark and the words of each state, with a bar only when the size is known', () =>
  drawn(UpdateStage, { phase: 'checking' }, async (host, set) => {
    const stage = host.querySelector<HTMLElement>('.ustage')!;
    ok(stage, 'the screen is on the page');
    ok(stage.querySelector('svg'), 'the mark');
    equal(stage.querySelector('.ustage-text')!.textContent, t('update.checking'));
    equal(stage.querySelector('.ubar'), null, 'no bar while looking');
    set({ phase: 'downloading', percent: 42 });
    await settle();
    equal(stage.querySelector('.ustage-text')!.textContent, t('update.downloadingPercent', { percent: 42 }));
    const bar = stage.querySelector<HTMLElement>('.ubar')!;
    equal(bar.getAttribute('role'), 'progressbar');
    equal(bar.getAttribute('aria-valuenow'), '42');
    equal(bar.firstElementChild!.getAttribute('style')?.replace(/\s/g, ''), 'transform:scaleX(0.42);');
    set({ phase: 'downloading', mb: 12 });
    await settle();
    equal(stage.querySelector('.ustage-text')!.textContent, t('update.downloadingSize', { mb: 12 }));
    equal(stage.querySelector('.ubar'), null, 'an unknown size has no bar to fill');
    set({ phase: 'installing' });
    await settle();
    equal(stage.querySelector('.ustage-text')!.textContent, t('update.installing'));
    equal(stage.querySelector('.ubar'), null);
  }));

check('update: the opening screen covers the window below the title bar and the bar moves by transform only', () =>
  drawn(UpdateStage, { phase: 'downloading', percent: 10 }, async (host) => {
    const stage = host.querySelector<HTMLElement>('.ustage')!;
    const style = getComputedStyle(stage);
    equal(style.position, 'fixed');
    ok(Number(style.zIndex) < 60, 'the window buttons stay in reach');
    const rect = stage.getBoundingClientRect();
    ok(rect.width >= innerWidth - 1 && rect.height >= innerHeight - 1, 'it covers the whole window');
    equal(getComputedStyle(host.querySelector('.ubar > span')!).transitionProperty, 'transform', 'only transform moves');
  }));

check('update: "Update ready, restart" is one press, never greyed out, and says the work picks up again', () => {
  useLanguage('en');
  const host = document.createElement('div');
  document.body.append(host);
  let restarts = 0;
  const dispose = render(() => createComponent(Sidebar, {
    conversations: [], projects: [], vendors: ['codex'], version: 'test', place: { area: 'chat' }, running: [], quotas: {}, now: 0,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
    update: { phase: 'ready' }, onRestart: () => void restarts++,
  }), host);
  try {
    const button = host.querySelector<HTMLButtonElement>('.upd.act')!;
    ok(button, 'the control is there');
    equal(button.disabled, false, 'nothing greys it out: work that runs is cut and picks up again after');
    equal(button.title, en['update.readyHint']);
    equal(button.textContent, en['update.ready']);
    button.click();
    equal(restarts, 1, 'one press restarts, with no question');
  } finally { dispose(); host.remove(); }
});
