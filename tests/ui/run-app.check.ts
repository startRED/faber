import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type RunAppView } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { openedSite, showSite } from '../../ui/Site.tsx';
import { check, deepEqual, equal, frames, ok } from './check.ts';
import { quietTabs } from './site-tabs.check.ts';

check('page in the browser: Run my app asks first, opens the local page and shows recent output on exit', async () => {
  const undoTabs = quietTabs();
  const saved = {
    runAppStart: api.runAppStart, runAppConfirm: api.runAppConfirm, runAppStatus: api.runAppStatus,
    siteGo: api.siteGo, sitePlace: api.sitePlace, sitePark: api.sitePark,
  };
  const calls: string[][] = [];
  const command = 'pnpm run dev';
  const seen = 'a'.repeat(64);
  const url = 'http://localhost:5173/';
  let state: RunAppView = { phase: 'idle' };
  api.runAppStatus = async () => state;
  api.runAppStart = async (id) => {
    calls.push(['start', id]);
    return { phase: 'confirm', command, scripts: [['dev', 'vite']], seen };
  };
  api.runAppConfirm = async (id, stamp) => {
    calls.push(['confirm', id, stamp]);
    return { phase: 'starting' };
  };
  api.siteGo = async (id, to) => { calls.push(['browser', id, to]); return true; };
  api.sitePlace = async () => true;
  api.sitePark = async () => {};
  const [project, setProject] = createSignal(false);
  const root = document.createElement('div');
  document.body.append(root);
  showSite({ conversationId: 'app-test', url: '', title: '' });
  const dispose = render(() => createComponent(Running, {
    items: [], projects: [], modal: false, onOpen: () => {}, onClose: () => {}, ref: () => {},
    get site() { return openedSite(); },
    get runApp() { return project(); },
  }), root);
  const find = (label: string) => [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.trim() === label);
  const click = (label: string) => {
    const button = find(label);
    ok(button && !button.disabled, label);
    button.click();
  };
  try {
    await frames();
    equal(find(t('site.runApp')), undefined, 'Chat has no project app button');
    setProject(true);
    await frames();
    click(t('site.runApp'));
    await frames();
    ok(root.querySelector('.site-run-confirm')?.textContent?.includes(command), 'the command is shown before it runs');
    deepEqual(calls, [['start', 'app-test']]);
    click(t('catalog.cancel'));
    await frames();
    equal(calls.length, 1, 'cancel starts nothing');
    click(t('site.runApp'));
    await frames();
    state = { phase: 'running', url };
    click(t('site.runApp.allow'));
    await frames();
    ok(find(t('commands.stop')), 'Stop is available during startup');
    await new Promise((done) => setTimeout(done, 500));
    await frames();
    deepEqual(calls.slice(-2), [['confirm', 'app-test', seen], ['browser', 'app-test', url]]);
    equal(openedSite()?.url, url);
    state = { phase: 'notice', reason: 'reason.runApp.exited', output: 'EADDRINUSE\nPort 5173 is in use.' };
    await new Promise((done) => setTimeout(done, 1_100));
    await frames();
    equal(root.querySelector('.site-run-notice')?.textContent, t('reason.runApp.exited'));
    const details = root.querySelector<HTMLDetailsElement>('.site-run-output');
    ok(details && !details.open, 'the notice keeps the output behind a disclosure');
    details.open = true;
    equal(details.querySelector('pre')?.textContent, state.output);
  } finally {
    dispose();
    root.remove();
    showSite(undefined);
    Object.assign(api, saved);
    undoTabs();
  }
});
