import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { FaberAct, Message } from '../../ui/api.ts';
import { lang, useLanguage, t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

check('faber-act: automatic messages and decisions have their own rows', () => {
  const wake: FaberAct = { kind: 'wake', actor: { who: 'faber' }, what: 'wake.keepGoing', why: 'reason.autoDefault', change: 'keepGoing', sent: 'Please continue.' };
  const route: FaberAct = { kind: 'route', actor: { who: 'ai', vendor: 'codex', role: 'router' }, what: 'act.route.chose', params: { model: 'A', effort: 'medium' } };
  const messages: Message[] = [
    { id: 'w', role: 'user', at: 1, text: 'private', wake: { why: 'wake.keepGoing' }, acts: [wake] },
    { id: 'r', role: 'assistant', at: 2, text: 'Answer here', acts: [route] },
    { id: 'u', role: 'user', at: 3, text: 'My words' },
  ];
  const root = document.createElement('div'); root.style.width = '720px'; document.body.append(root);
  const old = lang(); useLanguage('de');
  const changed: string[] = [];
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'test', options: [], onBranch: () => {}, onActChange: (target) => changed.push(target) }), root);
  try {
    const rows = root.querySelectorAll<HTMLElement>('.faber-act');
    equal(rows.length, 2);
    ok(rows[0]!.textContent!.includes(t('act.faber')));
    ok(rows[0]!.textContent!.includes(t('act.why', { why: t('reason.autoDefault') })));
    rows[0]!.querySelector('button')!.click(); equal(changed[0], 'keepGoing');
    const disclosure = rows[0]!.querySelector('details')!; ok(!disclosure.open); disclosure.open = true;
    equal(disclosure.querySelector('.faber-act-sent')!.textContent, wake.sent);
    ok(rows[1]!.compareDocumentPosition(root.querySelector('.ai > .md')!) & Node.DOCUMENT_POSITION_FOLLOWING);
    ok(root.querySelector('.you')!.textContent!.includes('My words'));
    for (const row of rows) ok(row.getBoundingClientRect().right <= root.getBoundingClientRect().right + 1, 'row fits the narrow window');
  } finally { dispose(); root.remove(); useLanguage(old); }
});
