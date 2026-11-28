import { createComponent, createSignal } from 'solid-js';
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
    // The effort reads in the screen's language, mid-sentence: "Aufwand mittel", never the raw "medium".
    ok(rows[1]!.textContent!.includes(t('act.route.chose', { model: 'A', effort: t('effort.medium').toLocaleLowerCase('de') })), rows[1]!.textContent!);
    ok(!rows[1]!.textContent!.includes('medium'));
    rows[0]!.querySelector('button')!.click(); equal(changed[0], 'keepGoing');
    const disclosure = rows[0]!.querySelector('details')!; ok(!disclosure.open); disclosure.open = true;
    equal(disclosure.querySelector('.faber-act-sent')!.textContent, wake.sent);
    ok(rows[1]!.compareDocumentPosition(root.querySelector('.ai > .md')!) & Node.DOCUMENT_POSITION_FOLLOWING);
    ok(root.querySelector('.you')!.textContent!.includes('My words'));
    for (const row of rows) ok(row.getBoundingClientRect().right <= root.getBoundingClientRect().right + 1, 'row fits the narrow window');
  } finally { dispose(); root.remove(); useLanguage(old); }
});

check('faber-act: helpers handed out together fold into one line that opens to each', () => {
  const helper = (call: string, model: string): FaberAct => ({ kind: 'helper', actor: { who: 'ai', vendor: 'claude', model, role: 'helper' }, what: 'act.helper.started', params: { call }, sent: `Brief ${call}` });
  const route: FaberAct = { kind: 'route', actor: { who: 'ai', vendor: 'codex', role: 'router' }, what: 'act.route.chose', params: { model: 'A', effort: 'medium' } };
  const messages: Message[] = [{ id: 'r', role: 'assistant', at: 2, text: 'Answer here', acts: [route, helper('h1', 'claude-opus-5-5'), helper('h2', 'claude-haiku-5-5'), helper('h3', 'claude-opus-5-5')] }];
  const root = document.createElement('div'); root.style.width = '720px'; document.body.append(root);
  const old = lang(); useLanguage('en');
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'test', options: [], onBranch: () => {} }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('details.faber-act-group')!;
    ok(group, 'consecutive helpers share one line');
    equal(group.querySelector('summary')!.textContent, t('act.helper.group', { n: 3 }));
    ok(!group.open);
    equal(root.querySelectorAll('.ai > .faber-act:not(.faber-act-group)').length, 1, 'the route row stays on its own');
    group.open = true;
    equal(group.querySelectorAll(':scope > .faber-act').length, 3);
    equal(group.querySelector(':scope > .faber-act .faber-act-line')!.textContent, t('act.helper', { name: 'Opus 5.5' }), 'inside the group a row only names its helper');
    equal(group.querySelectorAll('.faber-act-sent')[1]!.textContent, 'Brief h2');
  } finally { dispose(); root.remove(); useLanguage(old); }
});

check('faber-act: a helper joining a reply keeps an open group open, in place and focused', () => {
  const helper = (call: string): FaberAct => ({ kind: 'helper', actor: { who: 'ai', vendor: 'codex', model: 'gpt-6.1-terra', role: 'helper' }, what: 'act.helper.started', params: { call }, sent: `Brief ${call}` });
  const reply = (acts: FaberAct[]): Message[] => [{ id: 'r', role: 'assistant', status: 'streaming', at: 2, text: 'Working', acts }];
  const [messages, setMessages] = createSignal(reply([helper('h1')]));
  const root = document.createElement('div'); root.style.width = '720px'; document.body.append(root);
  const dispose = render(() => createComponent(Thread, { get messages() { return messages(); }, from: 0, conversationId: 'test', options: [], onBranch: () => {} }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('details.faber-act-group')!;
    equal(group.querySelector('summary')!.textContent, t('act.helper.group', { n: 1 }), 'one helper is already the group line');
    group.open = true;
    const brief = group.querySelector<HTMLDetailsElement>('.faber-act details.disclosure')!; brief.open = true;
    const inner = brief.querySelector<HTMLElement>('summary')!; inner.focus();
    setMessages(reply([helper('h1'), helper('h2')]));
    ok(group.isConnected && brief.isConnected && brief.open, 'one to two keeps the group and its open brief');
    equal(document.activeElement, inner, 'focus stays on the open brief');
    const summary = group.querySelector<HTMLElement>('summary')!; summary.focus();
    setMessages(reply([helper('h1'), helper('h2'), helper('h3')]));
    equal(root.querySelector('details.faber-act-group'), group, 'the same element');
    ok(group.open, 'still open');
    equal(document.activeElement, summary, 'focus stays on the line');
    equal(summary.textContent, t('act.helper.group', { n: 3 }));
    equal(group.querySelectorAll(':scope > .faber-act').length, 3);
  } finally { dispose(); root.remove(); }
});

check('faber-act: a command the AI ran without asking in Full access says what was done or tried, why, and opens the mode', () => {
  const ran: FaberAct = { kind: 'reach', actor: { who: 'faber' }, what: 'act.reach.settings', why: 'act.why.fullAccess', change: 'settings' };
  const failed: FaberAct = { kind: 'reach', actor: { who: 'faber' }, what: 'act.reach.failed.rewind', why: 'act.why.fullAccess', change: 'settings' };
  const messages: Message[] = [{ id: 'r', role: 'assistant', at: 2, text: 'Done.', acts: [ran, failed] }];
  const root = document.createElement('div'); root.style.width = '720px'; document.body.append(root);
  const old = lang(); useLanguage('pt-BR');
  const changed: string[] = [];
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'test', options: [], onBranch: () => {}, onActChange: (target) => changed.push(target) }), root);
  try {
    const rows = root.querySelectorAll<HTMLElement>('.faber-act');
    equal(rows.length, 2);
    equal(rows[1]!.querySelector('.faber-act-line > span')!.textContent, `${t('act.faber')} · ${t('act.reach.failed.rewind')} · ${t('act.why', { why: t('act.why.fullAccess') })}`, 'a failed try is said too');
    const line = rows[0]!.querySelector('.faber-act-line > span')!.textContent!;
    equal(line, `${t('act.faber')} · ${t('act.reach.settings')} · ${t('act.why', { why: t('act.why.fullAccess') })}`);
    ok(line.includes(t('set.permissions.dontAsk')), 'it names the mode as Settings does');
    ok(!/\bact\.|settings|Full access/.test(line), `only the screen's own words, in its language: ${line}`);
    const change = rows[0]!.querySelector('button')!;
    equal(change.textContent, t('act.change')); change.click(); equal(changed[0], 'settings');
    for (const row of rows) ok(row.getBoundingClientRect().right <= root.getBoundingClientRect().right + 1, 'the line fits the narrow window');
  } finally { dispose(); root.remove(); useLanguage(old); }
});
