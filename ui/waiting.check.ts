import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from './Thread.tsx';
import type { RunningItem, Summary } from './api.ts';
import { tailOf } from './sidebar.ts';
import { check, ok } from '../tests/ui/check.ts';
import { waitWords } from './running.ts';
import { setAgeNow } from './foot.ts';
import { LANGUAGES, t, useLanguage } from './i18n.ts';

check('running: wait blockers and missing deadlines never promise an automatic return', () => {
  for (const language of LANGUAGES) {
    useLanguage(language.code);
    const blocked = waitWords({ note: 'Synthetic checks', until: Date.now() + 300000, problem: 'reason.wait.delivery' })!;
    ok(blocked.includes(t('reason.wait.delivery')));
    ok(!blocked.includes(t('wait.until', { when: '' })), 'a blocked wait must not promise another check');
    ok(waitWords({ note: 'Synthetic checks' })!.includes(t('wait.noDeadline')));
    ok(waitWords({ note: 'Synthetic checks', until: 1 })!.includes(t('wait.overdue', { when: '' }).trim()));
    ok(waitWords({ problem: 'unknown.future.key' })!.includes(t('wait.unknownProblem')));
  }
  useLanguage('en');
  ok(waitWords(undefined) === undefined, 'a resumed conversation has no stale waiting line');
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
    setAgeNow(Date.now() + 600000);
    ok(row.textContent!.includes('Return check overdue'), 'the mounted row updates when time advances');
    setAgeNow(Date.now());
  } finally { dispose(); root.remove(); }
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
