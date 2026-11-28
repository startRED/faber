import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from './Thread.tsx';
import type { RunningItem, Summary } from './api.ts';
import { tailOf } from './sidebar.ts';
import { check, ok } from '../tests/ui/check.ts';
import { waitNeedsAttention, waitWords } from './running.ts';
import { setAgeNow } from './foot.ts';
import { LANGUAGES, t, useLanguage } from './i18n.ts';

check('running: wait blockers and missing deadlines never promise an automatic return', () => {
  for (const language of LANGUAGES) {
    useLanguage(language.code);
    const blocked = waitWords({ note: 'Synthetic checks', until: Date.now() + 300000, problem: 'reason.wait.delivery' })!;
    ok(blocked.includes(t('reason.wait.delivery')));
    ok(blocked.startsWith('Synthetic checks · '), 'the saved note stays beside the blocker');
    ok(!blocked.includes(t('wait.until', { when: '' })), 'a blocked wait must not promise another check');
    ok(waitWords({ note: 'Synthetic checks' })!.includes(t('wait.noDeadline')));
    ok(waitWords({ note: 'Synthetic checks', until: 1 })!.includes(t('wait.overdue', { when: '' }).trim()));
    ok(waitWords({ problem: 'unknown.future.key' })!.includes(t('wait.unknownProblem')));
    const followed = { ids: ['synthetic-child'], until: Date.now() + 300000 };
    ok(waitWords(followed)!.includes(t('run.waitingOn', { n: 1 })), 'a healthy wait names the conversation count');
    ok(
      waitWords({ ...followed, problem: 'reason.budget.other' }) === t('reason.budget.other'),
      'a blocked return must not present saved references as active waits',
    );
  }
  useLanguage('en');
  ok(waitWords(undefined) === undefined, 'a resumed conversation has no stale waiting line');
  ok(waitNeedsAttention({ problem: 'reason.wait.delivery' }));
  ok(waitNeedsAttention({ until: 1 }));
  ok(waitNeedsAttention({ until: Infinity }), 'a wait with an invalid deadline needs attention');
  ok(waitNeedsAttention({ note: 'Synthetic checks' }), 'a wait with no recorded time needs attention');
  ok(!waitNeedsAttention({ until: 1, problem: 'reason.wait.stopped' }));
  ok(!waitNeedsAttention({ until: 1, problem: 'reason.wait.archived' }));
});

check('running: a blocked wait updates its recovery action and hides it when stopped or archived', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [problem, setProblem] = createSignal('reason.wait.delivery');
  const [archived, setArchived] = createSignal(false);
  const [connected, setConnected] = createSignal(false);
  const actions: string[] = [];
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'synthetic-wait', area: 'project', from: 0, options: [], onBranch: () => {}, messages: [],
    get archived() { return archived(); },
    get waitingOn() { return { problem: problem(), since: 1, until: 2 }; },
    onOpenWaitAccounts: () => { actions.push('accounts'); },
    get onWriteWaitMessage() { return connected() ? () => { actions.push('write'); } : undefined; },
  }), root);
  try {
    const button = root.querySelector<HTMLButtonElement>('.wait-message')!;
    ok(button.textContent === t('empty.noAiButton') && button.dataset.restore === 'wait-recovery');
    button.click();
    ok(actions.pop() === 'accounts');
    setConnected(true);
    ok(button.textContent === t('wait.write') && button.dataset.restore === 'wait-recovery');
    button.click();
    ok(actions.pop() === 'write');
    for (const key of ['reason.budget.other', 'error.noSubscription']) {
      setProblem(key);
      ok(button.textContent === t('set.openSetup'));
      button.click();
      ok(actions.pop() === 'accounts');
    }
    setConnected(false);
    ok(root.querySelector('.wait-message')?.textContent === t('empty.noAiButton'));
    setProblem('reason.wait.delivery');
    setArchived(true);
    ok(!root.querySelector('.wait-message'));
    ok(!root.querySelector('.wait-attention'));
    setArchived(false);
    setProblem('reason.wait.stopped');
    ok(!root.querySelector('.wait-message'));
    ok(!root.querySelector('.wait-attention'));
  } finally { dispose(); root.remove(); }
});

check('running: declared child wait shows its return deadline without a working spinner', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const child = { id: 'synthetic-child', title: 'Synthetic checks', area: 'project', updatedAt: 0,
    startedBy: 'synthetic-parent', childState: 'waiting', state: 'waiting',
    waitingOn: { note: 'Synthetic remote checks', since: Date.now(), until: Date.now() + 300000 } } as Summary;
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'synthetic-parent', area: 'project', from: 0, options: [], onBranch: () => {}, onOpenChild: () => {},
    conversations: [child], messages: [{ id: 'answer', role: 'assistant', text: 'Waiting.', at: 0, status: 'done',
      tools: [{ id: 'start', name: 'Conversation', input: { action: 'start' }, ok: true, output: '{"childId":"synthetic-child"}' }] }],
  }), root);
  try {
    const row = root.querySelector('.child-conversation');
    ok(row, 'the child conversation is visible');
    ok(row.textContent!.includes('Synthetic remote checks'));
    ok(row.textContent!.includes('Check again by'), 'a wait must show its finite return time');
    ok(!row.querySelector('.working'), 'waiting must not imply a live worker');
    ok(!row.querySelector('.wait-attention'), 'a healthy wait is calm');
    setAgeNow(Date.now() + 600000);
    ok(row.textContent!.includes('Return check overdue'), 'the mounted row updates when time advances');
    ok(row.querySelector('.wait-attention'), 'an overdue check is visually distinct from a healthy wait');
  } finally { setAgeNow(Date.now()); dispose(); root.remove(); }
});


check('running: child wait rows contain wrapped text', () => {
  const root = document.createElement('div');
  root.innerHTML = `<div class="steps"><div class="steps-list"><div class="step"><button class="step-line">Working</button><button class="btn child-conversation"><b>Synthetic child conversation with a long title</b><span>Waiting for synthetic checks to finish before the next automatic return time arrives.</span></button></div><div class="step"><button class="step-line">Next row</button></div></div></div>`;
  document.body.append(root);
  try {
    for (const width of [390, 1280]) {
      root.style.width = `${width}px`;
      const button = root.querySelector<HTMLElement>('.child-conversation')!;
      const step = button.closest<HTMLElement>('.step')!;
      const next = step.nextElementSibling!.getBoundingClientRect();
      const box = button.getBoundingClientRect();
      for (const child of button.children) ok(child.getBoundingClientRect().bottom <= box.bottom + 0.5, `text escapes button at ${width}px`);
      ok(box.bottom <= step.getBoundingClientRect().bottom + 0.5 && box.bottom <= next.top + 0.5, `rows overlap at ${width}px`);
    }
  } finally { root.remove(); }
});

check('running: a project waiting between turns has no working spinner', () => {
  const item = { area: 'project', projectId: 'synthetic', state: 'working', waitingOn: { since: 1, until: 300001 } } as RunningItem;
  ok(!tailOf([item], 'synthetic').running, 'a declared wait is not a live worker');
  ok(tailOf([{ ...item, waitingOn: undefined }], 'synthetic').running, 'a real worker keeps its spinner');
});
