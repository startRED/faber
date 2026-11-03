import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AgentAsk } from '../../ui/api.ts';
import { AgentAskCard } from '../../ui/Ask.tsx';
import { LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('agent permission card: both answers update the list and words fit every language', async () => {
  const original = api.answerAsk;
  const calls: unknown[][] = [];
  api.answerAsk = async (...args) => { calls.push(args); return true; };
  const root = document.createElement('div');
  root.style.width = '720px';
  document.body.append(root);
  const [asks, setAsks] = createSignal<AgentAsk[]>([
    { id: 'one', conversation: 'c', label: 'settings', state: 'pending' },
    { id: 'two', conversation: 'c', label: 'tools', state: 'pending' },
  ]);
  const dispose = render(() => createComponent(AgentAskCard, {
    get ask() { return asks()[0] ?? { id: 'gone', conversation: 'c', label: 'tools', state: 'pending' }; },
    onAnswered: (id) => setAsks((list) => list.filter((ask) => ask.id !== id)),
  }), root);
  try {
    for (const { code } of LANGUAGES) {
      useLanguage(code);
      const card = root.querySelector<HTMLElement>('[data-testid="agent-ask"]')!;
      equal(card.querySelector('b')!.textContent, t('agentAsk.settings'));
      ok(!card.querySelector('pre'), 'no raw command');
      const bounds = card.getBoundingClientRect();
      for (const button of card.querySelectorAll('button')) {
        const box = button.getBoundingClientRect();
        ok(box.left >= bounds.left - 0.5 && box.right <= bounds.right + 0.5, `${code}: button fits`);
      }
    }
    useLanguage('en');
    root.querySelectorAll('button')[0]!.click();
    await new Promise((done) => setTimeout(done, 20));
    equal(asks().length, 1);
    equal(root.querySelector('b')!.textContent, t('agentAsk.tools'));
    root.querySelectorAll('button')[1]!.click();
    await new Promise((done) => setTimeout(done, 20));
    deepEqual(calls, [['one', true], ['two', false]]);
    equal(asks().length, 0);
  } finally { dispose(); root.remove(); api.answerAsk = original; useLanguage('en'); }
});

import { Running } from '../../ui/Running.tsx';

check('agent permission list: notice opens the newest conversation and clears with the requests', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [asks, setAsks] = createSignal<AgentAsk[]>([
    { id: 'one', conversation: 'first', label: 'organize', state: 'pending' },
    { id: 'two', conversation: 'newest', label: 'rewind', state: 'pending' },
  ]);
  let opened = '';
  const dispose = render(() => createComponent(Running, {
    items: [], projects: [], modal: false, onOpen: () => {}, onClose: () => {}, ref: () => {},
    get agentAsks() { return asks(); }, onOpenAsk: (id) => { opened = id; },
  }), root);
  try {
    const notice = root.querySelector<HTMLElement>('[data-testid="agent-ask-notice"]');
    ok(notice, 'a pending request is visible');
    notice.querySelector('button')!.click();
    equal(opened, 'newest');
    setAsks([]);
    ok(!root.querySelector('[data-testid="agent-ask-notice"]'), 'the notice leaves with the list');
  } finally { dispose(); root.remove(); }
});
