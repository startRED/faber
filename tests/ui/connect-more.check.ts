import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { useLanguage } from '../../ui/i18n.ts';
import { en } from '../../ui/locales/en.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { check, equal, ok } from './check.ts';

check('sidebar: after a Skip on the account cards, one quiet line offers the AI still missing, and goes when it is no longer asked', () => {
  useLanguage('en');
  const host = document.createElement('div');
  document.body.append(host);
  const [remind, setRemind] = createSignal(true);
  let opened = 0;
  const dispose = render(() => createComponent(Sidebar, {
    conversations: [], projects: [], vendors: ['codex'], version: 'test', place: { area: 'chat' }, running: [], quotas: {}, now: 0,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
    get remindConnect() { return remind(); }, onConnect: () => void opened++,
  }), host);
  try {
    const line = () => host.querySelector<HTMLButtonElement>('.usage .vendor.more');
    ok(line(), 'the line is there');
    equal(line()!.textContent, en['side.connectMore']);
    line()!.click();
    equal(opened, 1, 'it opens the account cards');
    setRemind(false);
    equal(line(), null, 'closing the cards, or connecting the other AI, takes it away');
  } finally { dispose(); host.remove(); }
});
