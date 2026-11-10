import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

check('auto effort: the existing answer reason says raised or lowered only on a change in every language', () => {
  const previousLanguage = lang();
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'answer', role: 'assistant', text: 'Answer', at: 1,
    status: 'streaming', vendor: 'codex', model: 'gpt-6.1-sol', effort: 'high', reason: 'reason.autoEffortRaised' });
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'auto-effort', get messages() { return [message()]; }, from: 0, options: [], onBranch: () => {},
  }), root);
  try {
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language);
      for (const status of ['streaming', 'done'] as const) {
        for (const [reason, effort] of [['reason.autoEffortRaised', 'high'], ['reason.autoEffortLowered', 'low']] as const) {
          setMessage({ ...message(), reason, effort, status });
          const line = root.querySelector('.model em');
          ok(line, `${language}: effort change is visible in the existing header`);
          ok(line.textContent?.includes(t(reason)), language);
          ok(root.querySelector('.model .effort')?.textContent?.includes(t(`answer.effort.${effort}`)));
          equal(root.querySelectorAll('.model-efforts').length, 0, 'no extra picker');
        }
        setMessage({ ...message(), reason: 'reason.route.chat.easy', status });
        equal(root.querySelector('.model em'), null, 'unchanged effort has no change sentence');
      }
    }
  } finally {
    dispose(); root.remove(); useLanguage(previousLanguage);
  }
});

check('auto effort: unchanged later answers keep route metadata without drawing a route line', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const first: Message = { id: 'first', role: 'assistant', text: 'First', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', effort: 'high' };
  const [later, setLater] = createSignal<Message>({ ...first, id: 'later', text: 'Later', at: 2, reason: 'reason.route.code.hard',
    acts: [{ kind: 'route', actor: { who: 'faber' }, what: 'act.route.chose', why: 'reason.route.code.hard', params: { model: 'claude-sonnet-5-5', effort: 'high', wantedEffort: 'high' } }] });
  first.acts = later().acts;
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'auto-effort', get messages() { return [first, { id: 'question', role: 'user', text: 'Again', at: 2 } as Message, later()]; }, from: 0, options: [], onBranch: () => {},
  }), root);
  try {
    for (const status of ['streaming', 'done'] as const) {
      setLater({ ...later(), status, reason: 'reason.route.code.hard', nudge: undefined });
      equal(root.querySelectorAll('.faber-act').length, 1, 'only the first answer keeps its route line');
      equal(later().acts?.[0]?.params?.wantedEffort, 'high', 'the act stays on the message');
      for (const reason of ['reason.autoEffortRaised', 'reason.autoEffortLowered']) {
        setLater({ ...later(), reason });
        equal(root.querySelectorAll('.faber-act').length, 2, 'an effort change shows the route');
      }
    }
    setLater({ ...later(), reason: 'reason.route.code.hard', nudge: { reason: 'nudge.modelBar', vendor: 'claude', model: 'claude-opus-4-8', effort: 'high' } });
    equal(root.querySelectorAll('.faber-act').length, 2, 'a stronger-model offer shows the route');
  } finally {
    dispose(); root.remove();
  }
});
