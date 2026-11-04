import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from '../../ui/api.ts';
import { DetailsPane } from '../../ui/Details.tsx';
import { LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
// The driver-capture engine checks assert this same response equals the actual CLI input.
const sent = '```faber-start-card\n## Rules\n- Keep ação and public exports.\nAnd 2 more rule lines; search them with memory_search.\nAnd 3 older decisions; search them with memory_search.\nAnd 4 more taste lines; search them with memory_search.\n```';

check('start card details: exact sent text, lazy fold, no-card sessions and narrow long languages', async () => {
  const before = { ...api };
  const [id, setId] = createSignal('received');
  let reads = 0;
  Object.assign(api, { startCards: async (id: string) => {
    reads++;
    const card = { vendor: 'claude', text: sent, bytes: new TextEncoder().encode(sent).length, omittedRules: 2, omittedDecisions: 3, omittedTaste: 4 };
    return id === 'received' ? [card] : id === 'two' ? [card, { ...card, vendor: 'codex', text: 'Another exact sent card.' }] : [];
  } });
  const host = document.createElement('div');
  host.style.width = '720px';
  host.style.maxWidth = '100%';
  document.body.append(host);
  const dispose = render(() => createComponent(DetailsPane, {
    get conversationId() { return id(); }, messages: [], background: [],
    copy: { branch: 'work', path: 'E:/synthetic/project' },
    changes: { id: 'received', files: [], loading: false }, onBack() {}, onClose() {},
  }), host);
  try {
    await settle();
    const fold = host.querySelector<HTMLDetailsElement>('[data-testid=start-card]');
    ok(fold, 'a received card has one quiet details row');
    ok(host.querySelector('[data-testid=details-copy]')!.compareDocumentPosition(fold) & Node.DOCUMENT_POSITION_FOLLOWING, 'the separate copy is still named first');
    equal(fold.querySelector('summary')!.textContent, t('details.startCard'));
    equal(fold.querySelector('pre'), null, 'the closed fold creates no card text nodes');
    fold.open = true; fold.dispatchEvent(new Event('toggle'));
    await settle();
    equal(fold.querySelector('pre')!.textContent, sent, 'the exact sent bytes are shown literally');
    equal(fold.querySelector('.side-meta')!.textContent, t('details.startCardSize', { n: new TextEncoder().encode(sent).length }));
    ok(fold.textContent!.includes(t('details.startCardOmitted', { rules: 2, decisions: 3, taste: 4 })));
    equal(fold.querySelector('textarea, input, [contenteditable]'), null, 'the record is read only');
    for (const language of LANGUAGES) {
      useLanguage(language.code);
      equal(fold.querySelector('summary')!.textContent, t('details.startCard'));
      ok(fold.getBoundingClientRect().right <= host.getBoundingClientRect().right + 1, `the row fits at 720 px in ${language.code}`);
    }
    equal(reads, 1, 'translation and fold toggles never reread the project');
    setId('no-card');
    await settle();
    equal(host.querySelector('[data-testid=start-card]'), null, 'a session with no card has no row');
    equal(host.querySelector('pre'), null, 'switching conversations releases the old text');
    setId('two'); await settle();
    const multiple = host.querySelector<HTMLDetailsElement>('[data-testid=start-card]')!;
    multiple.open = true; multiple.dispatchEvent(new Event('toggle')); await settle();
    equal(multiple.querySelectorAll('pre').length, 2, 'both native sessions retain their own text');
    equal(multiple.querySelectorAll('.det-state').length, 2, 'several sessions name the AI beside each card');
    multiple.open = false; multiple.dispatchEvent(new Event('toggle')); await settle();
    equal(multiple.querySelector('pre'), null, 'closing releases the text nodes');
  } finally {
    dispose(); host.remove(); Object.assign(api, before); useLanguage('en');
  }
});

check('start card details: late responses never cross conversations and read errors can be retried', async () => {
  const before = api.startCards;
  let finish!: (cards: Awaited<ReturnType<typeof api.startCards>>) => void;
  let retry = false;
  const card = { vendor: 'claude' as const, text: sent, bytes: new TextEncoder().encode(sent).length, omittedRules: 2, omittedDecisions: 3, omittedTaste: 4 };
  api.startCards = (id) => id === 'slow' ? new Promise((resolve) => { finish = resolve; }) : id === 'failed' ? retry ? Promise.resolve([card]) : Promise.reject(new Error('read failed')) : Promise.resolve([]);
  const [id, setId] = createSignal('slow');
  const host = document.createElement('div'); document.body.append(host);
  const dispose = render(() => createComponent(DetailsPane, {
    get conversationId() { return id(); }, messages: [], background: [],
    changes: { id: 'slow', files: [], loading: false }, onBack() {}, onClose() {},
  }), host);
  try {
    await settle();
    setId('no-card'); await settle();
    finish([card]);
    await settle();
    equal(host.querySelector('[data-testid=start-card]'), null, 'a late old response cannot appear in the new conversation');
    setId('failed'); await settle();
    equal(host.querySelector('[role=status]')!.textContent, t('details.startCardError'), 'read failure is explicit and recoverable');
    setId('no-card'); await settle();
    equal(host.querySelector('[role=status]'), null, 'reopening retries without keeping the previous error');
    retry = true; setId('failed'); await settle();
    ok(host.querySelector('[data-testid=start-card]'), 'reopening the same failed conversation retries its request');
    equal(host.querySelector('[role=status]'), null, 'a successful retry clears the failure');
  } finally { dispose(); host.remove(); api.startCards = before; }
});
