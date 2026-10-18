import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Checks, Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

// R146: Faber's own checks of a reply that changed files are one quiet line under it; what failed opens below, and only
// checks that wait for the person are amber. Faber's fix turn is its quiet line in place of the person's bubble.
const question: Message = { id: 'q', role: 'user', text: 'Fix the login page', at: 0 };
const answer = (id: string, checks: Checks): Message => ({ id, role: 'assistant', text: 'Fixed it.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', checks });
const lint = { command: 'npm run lint', lines: ["ui/App.tsx:3:7: error TS2322: Type 'string' is not assignable to type 'number'.\n"] };

function mount(messages: Message[]) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'checks-check', options: [], onBranch: () => {} }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('checks: a reply that changed files shows Faber checking it, then what passed or failed', async () => {
  const running = mount([question, answer('a', { state: 'running', passed: 0 })]);
  try {
    const line = running.root.querySelector<HTMLElement>('.checks')!;
    ok(line, 'the reply shows its checks');
    equal(line.querySelector('p')!.textContent, t('checks.running'));
    equal(line.querySelector('details'), null, 'nothing to open while they run');
  } finally {
    running.close();
  }

  const passed = mount([question, answer('a', { state: 'passed', passed: 3 })]);
  try {
    equal(passed.root.querySelector('.checks p')!.textContent, t('checks.passed', { n: 3 }));
    ok(!passed.root.querySelector('.checks')!.classList.contains('need'));
  } finally {
    passed.close();
  }

  const fix: Message = { id: 'w', role: 'user', text: 'Faber ran the checks…', at: 2, wake: { why: 'wake.checksFixing', checks: ['npm run lint'] } };
  const failed = mount([question, answer('a', { state: 'failed', passed: 1, failed: [lint] }), fix]);
  try {
    const line = failed.root.querySelector<HTMLElement>('.checks')!;
    equal(line.querySelector('p')!.textContent, t('checks.failed', { n: 1, checks: 'npm run lint' }));
    ok(!line.classList.contains('need'), 'a failure Faber asked to fix is quiet');
    const why = line.querySelector<HTMLDetailsElement>('details')!;
    equal(why.querySelector('summary')!.textContent, t('checks.why'));
    ok(why.querySelector('pre')!.textContent!.includes('error TS2322'), 'the short failure is behind it');
    equal(failed.root.querySelector('.woke')!.textContent, t('wake.checksFixing', { checks: 'npm run lint' }), 'the fix turn is a quiet line');
  } finally {
    failed.close();
  }

  const needs = mount([question, answer('a', { state: 'needsYou', passed: 1, failed: [lint] })]);
  try {
    const line = needs.root.querySelector<HTMLElement>('.checks')!;
    ok(line.classList.contains('need'), 'checks that wait for the person are amber');
    equal(line.querySelector('p')!.textContent, t('checks.needsYou', { n: 1, checks: 'npm run lint' }));
  } finally {
    needs.close();
  }
});

check('checks: while Faber asks git whether a reply changed files, the newest reply says so, and only it', () => {
  const done: Message = { id: 'a', role: 'assistant', text: 'Ran the script.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5' };
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => createComponent(Thread, { messages: [question, { ...done, id: 'old' }, question, done], from: 0, conversationId: 'checks-check', options: [], onBranch: () => {}, weighing: true }),
    root,
  );
  try {
    const lines = root.querySelectorAll('.checks[data-state="weighing"]');
    equal(lines.length, 1, 'one line, on the newest reply');
    equal(lines[0]!.textContent, t('checks.weighing'));
    ok(root.querySelectorAll('article.ai')[1]!.contains(lines[0]!));
  } finally {
    dispose();
    root.remove();
  }
  const quiet = mount([question, done]);
  try {
    equal(quiet.root.querySelector('.checks'), null, 'nothing once git answered');
  } finally {
    quiet.close();
  }
});
