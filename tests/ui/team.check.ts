import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, TeamMessage, ToolUse } from '../../ui/api.ts';
import { openedAgent, showAgent } from '../../ui/AgentChat.tsx';
import { Team } from '../../ui/Team.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { agentSteps } from '../../ui/worklog.ts';
import { keepParts, timeline } from '../../ui/timeline.ts';
import { teamCounts, teamOutcome, teamTime, teamWire } from '../../ui/team.ts';
import { applyLook } from '../../ui/appearance.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import themes from '../../src-tauri/src/themes.json';
import { check, deepEqual, equal, finished, frames, ok } from './check.ts';
import { keepRoot, sala } from './appearance.check.ts';

const ask: TeamMessage = { id: 'q', question: 'q', from: 'h1', to: 'lead', fromName: 'Read config', toName: 'lead', kind: 'ask', text: 'Can I keep the old configuration format?', at: 1200, state: 'waiting' };
const tell: TeamMessage = { id: 't', from: 'h2', to: 'h3', fromName: 'Build screen', toName: 'Review changes', kind: 'tell', text: 'The configuration reader is ready for your checks.', at: 1100 };
export const teamFixture = (): Message => ({ id: 'team-reply', role: 'assistant', status: 'streaming', vendor: 'claude', at: 1000, text: '',
  tools: [...['Read config', 'Build screen', 'Review changes'].map((title, i): ToolUse => ({ id: `h${i + 1}`, name: 'Helper', input: { title, brief: title }, started: 1000 + i * 100,
    team: { name: title, title, job: ['read', 'build', 'review'][i]!, files: [], state: i === 0 ? 'waitingLead' : 'running', messages: i === 0 ? [ask] : [tell] } })),
    { id: 'build-activity', name: 'Edit', parent: 'h2', input: { file_path: 'ui/Team.tsx' } },
    { id: 'review-activity', name: 'Bash', parent: 'h3', input: { command: 'npm run typecheck' } }],
  acts: ['claude', 'codex', 'claude'].map((vendor, i) => ({ kind: 'helper', actor: { who: 'ai', vendor: vendor as 'claude' | 'codex', model: ['claude-haiku-5-5', 'gpt-6.1-terra', 'claude-sonnet-5-5'][i], role: 'helper' }, what: 'act.helper.started', params: { call: `h${i + 1}`, effort: ['medium', 'low', 'high'][i]!, job: ['read', 'build', 'review'][i]! } })) });
const members = (message: Message) => agentSteps(message.tools, '', undefined, [], message.acts);
const ended = (message: Message): Message => ({ ...message, status: 'done', tools: message.tools!.map((tool, i) => ({ ...tool, ok: i !== 1, ended: 5000 + i * 1000, team: tool.team ? { ...tool.team, state: 'ended' } : undefined })) });

check('team block: pure run, mirrored tells, answers, counts and last end', () => {
  const message = teamFixture();
  const steps = members(message);
  const parts = timeline(message, '');
  equal(parts.length, 1);
  equal(parts[0]!.kind, 'team');
  equal(keepParts(parts, timeline(message, '')), parts, 'unchanged runs keep their place');
  deepEqual(teamCounts(steps, true), { working: 2, waiting: 1, done: 0, ended: false });
  const answer: TeamMessage = { id: 'a', from: 'lead', to: 'h1', fromName: 'lead', toName: 'Read config', kind: 'answer', text: 'Keep it compatible.', at: 1300, question: 'q' };
  const updated = members({ ...message, tools: message.tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, messages: [{ ...ask, state: 'answered' }, answer] } }) });
  const wire = teamWire(updated);
  deepEqual(wire.map((line) => line.message.id), ['t', 'q']);
  deepEqual(wire[1]!.replies.map((reply) => reply.id), ['a']);
  deepEqual(teamTime(members(ended(message))), { start: 1000, end: 7000 });
  equal(teamCounts(members(ended(message)), false).ended, true);
  const cli = { id: 'cli', name: 'Agent', input: { description: 'Read tests' } };
  const mixed = timeline({ ...message, tools: [cli, message.tools![0]!] }, '');
  equal(mixed[0]!.kind, 'team', 'CLI members join a Faber helper run');
  equal(timeline({ ...message, tools: [cli] }, '')[0]!.kind, 'agent', 'CLI-only replies keep rows');
  deepEqual(timeline({ ...message, tools: [{ ...message.tools![0]!, at: 0 }, { ...message.tools![1]!, at: 5 }] }, 'hello').map((part) => part.kind), ['team', 'text', 'team'], 'words stay between separate runs');
  equal(timeline({ ...message, tools: [{ ...message.tools![0]!, ended: 1100 }, { ...message.tools![1]!, started: 1200 }] }, '').length, 2, 'a later completed batch is a separate run');
});

check('team block: one helper carries model, effort, job and piece title, opens its own view', () => {
  const root = document.createElement('div'); document.body.append(root);
  const message = teamFixture(); message.tools = message.tools!.slice(0, 1);
  const dispose = render(() => createComponent(Thread, { messages: [message], from: 0, conversationId: 'team', options: [], onBranch: () => {} }), root);
  try {
    equal(root.querySelectorAll('.team').length, 1);
    const card = root.querySelector<HTMLButtonElement>('.team-member')!;
    ok(card.textContent!.includes('Haiku 5.5 · Medium'));
    equal(card.querySelector('.team-job')!.textContent, 'Read');
    ok(card.getAttribute('aria-label')!.includes('Haiku 5.5 · Medium. Read.'), 'model, effort and job stay in the accessible name');
    equal(card.querySelector('.team-title')!.textContent, 'Read config');
    equal(card.querySelector('.mk')?.classList.contains('claude'), true, 'selected AI, not the opposite of the lead');
    equal(card.tabIndex, 0);
    card.focus(); equal(document.activeElement, card);
    card.click(); deepEqual(openedAgent(), { conversationId: 'team', messageId: 'team-reply', callId: 'h1' });
    equal(card.getAttribute('aria-current'), 'true');
    equal(root.querySelectorAll('.agents').length, 0);
    equal(root.querySelector('.team')?.getAttribute('role'), 'group');
  } finally { dispose(); root.remove(); showAgent(); }
});

check('team block: three cards fit reading width, stack narrowly, largest text and every look stay inside', async () => {
  const restore = keepRoot();
  const root = document.createElement('div'); root.style.cssText = 'position:fixed;left:0;top:0;width:660px'; document.body.append(root);
  const message = teamFixture();
  const dispose = render(() => createComponent(Team, { members: members(message), streaming: true, lead: 'claude', conversationId: 'team', messageId: message.id }), root);
  try {
    applyLook(sala({ motion: 'none' })); await frames();
    const cards = [...root.querySelectorAll<HTMLElement>('.team-member')];
    const rects = cards.map((card) => card.getBoundingClientRect());
    equal(new Set(rects.map((rect) => rect.top)).size, 1, 'three sit in one row at 660px');
    const looks = [themes.css.base, ...Object.values(themes.css.themes), ...Object.values(themes.css.presets)];
    for (const look of looks) for (const width of [660, 280, 180]) {
      root.style.width = `${width}px`;
      applyLook(sala({ vars: { ...themes.css.base, ...look, 'fs-xs': '15px', 'fs-sm': '17px', 'fs-ui': '18px' }, motion: 'none' }));
      for (const language of ['en', 'pt-BR', 'es', 'fr', 'de']) {
        useLanguage(language);
        ok(root.scrollWidth <= root.clientWidth, `${language} at ${width}: no horizontal overflow`);
        if (width < 400) equal(new Set(cards.map((card) => card.getBoundingClientRect().top)).size, 3, 'one card per row');
      }
    }
  } finally { dispose(); root.remove(); restore(); useLanguage('en'); }
});

check('team block: question, answer, lapsed question, earlier wire and distinct attention inks', async () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const [message, setMessage] = createSignal(teamFixture());
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'team', options: [], onBranch: () => {} }), root);
  try {
    const card = () => root.querySelector<HTMLButtonElement>('[data-call=h1]')!;
    const wire = () => root.querySelector<HTMLButtonElement>('[data-message=q] .team-wire')!;
    equal(card().dataset.state, 'waitingLead');
    ok(card().textContent!.includes('waiting for lead'));
    ok(wire().textContent!.includes('asked the lead; waiting'));
    const leadInk = getComputedStyle(card().querySelector('.st')!).color;
    const style = getComputedStyle(document.documentElement);
    const normal = style.getPropertyValue('--ink-2').trim();
    const reference = document.createElement('span'); reference.style.color = normal; root.append(reference);
    equal(leadInk, getComputedStyle(reference).color, 'waiting for lead uses second ink');
    equal(getComputedStyle(card().querySelector('.st svg')!).animationName, 'none', 'still ring');
    const focus = card(); focus.focus();
    setMessage({ ...message(), tools: message().tools!.map((tool, i) => i === 1 ? { ...tool, sign: 'waiting' } : tool) });
    const person = root.querySelector<HTMLElement>('[data-call=h2]')!;
    ok(getComputedStyle(person).borderTopColor !== getComputedStyle(card()).borderTopColor, 'person needs amber edge');
    ok(getComputedStyle(person.querySelector('.team-state')!).color !== leadInk, 'person needs amber words');
    const answer: TeamMessage = { id: 'a', from: 'lead', to: 'h1', fromName: 'lead', toName: 'Read config', kind: 'answer', text: 'Keep it compatible.', at: 1400, question: 'q' };
    setMessage({ ...message(), tools: message().tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, state: 'running', messages: [{ ...ask, state: 'answered' }, answer] } }) });
    equal(document.activeElement, focus, 'streaming keeps focused member');
    equal(root.querySelector('[data-message=q] .team-answer')?.textContent?.includes(answer.text), true);
    ok(root.querySelector('.tally-live')!.textContent!.includes('Read config: working'));
    equal(root.querySelectorAll('.tally-live[aria-live=polite]').length, 1, 'reuse reply announcements');
    const announcement = root.querySelector('.tally-live .sr-only')!.textContent;
    setMessage({ ...message(), text: 'More words.' });
    equal(root.querySelector('.tally-live .sr-only')!.textContent, announcement, 'no announcement per token');
    wire().focus(); wire().click(); equal(wire().getAttribute('aria-expanded'), 'true');
    equal(getComputedStyle(wire().querySelector('.team-wire-text')!).whiteSpace, 'pre-wrap');
    const notes = Array.from({ length: 4 }, (_, i): TeamMessage => ({ ...tell, id: `note${i}`, at: 1500 + i, text: `Note ${i}` }));
    setMessage({ ...message(), tools: message().tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, messages: [{ ...ask, state: 'lapsed' }, ...notes] } }) });
    equal(root.querySelectorAll('.team-exchange').length, 3);
    const earlier = root.querySelector<HTMLButtonElement>('.team-earlier')!;
    equal(earlier.getAttribute('aria-expanded'), 'false'); earlier.click();
    equal(root.querySelectorAll('.team-exchange').length, 6, 'mirrored tell appears once');
    ok(root.querySelector('[data-message=q] .team-lapsed')!.textContent!.includes('Nobody answered'));
    await frames();
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: tally updates do not repeat a helper transition', () => {
  const root = document.createElement('div'); document.body.append(root);
  const [message, setMessage] = createSignal(teamFixture());
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'team', area: 'project', options: [], onBranch: () => {} }), root);
  try {
    const region = root.querySelector('.tally-live')!;
    const helper = region.querySelector('.sr-only')!;
    const observer = new MutationObserver(() => {});
    observer.observe(helper, { childList: true, characterData: true, subtree: true });
    try {
      setMessage({ ...message(), tools: message().tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, state: 'running' } }) });
      equal(helper.textContent, 'Read config: working');
      ok(observer.takeRecords().length, 'a helper transition changes its announcement');
      equal(region.getAttribute('aria-atomic'), 'false', 'a tally update must not reread the whole region');
      equal(helper.getAttribute('aria-atomic'), 'true', 'read the whole named transition');
      for (const ok of [true, false]) {
        setMessage({ ...message(), tools: [...message().tools!, { id: `check-${ok}`, name: 'Bash', input: { command: 'test' }, check: true, ok }] });
        const tally = region.querySelector('.tally')!;
        equal(tally.closest('[aria-atomic="true"]'), tally, 'checks announce only their own tally');
        equal(tally.textContent!.includes('Read config'), false);
        equal(observer.takeRecords().length, 0, 'a check never mutates the helper announcement');
      }
      setMessage({ ...message(), text: 'More words.' });
      equal(observer.takeRecords().length, 0, 'streamed words never repeat a transition');
      setMessage({ ...message(), tools: message().tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, state: 'waitingLead' } }) });
      equal(helper.textContent, 'Read config: asked the lead; waiting');
      ok(observer.takeRecords().length, 'the next transition still announces');
    } finally { observer.disconnect(); }
  } finally { dispose(); root.remove(); }
});

check('team block: folds only after reply and members end, reopens at rest, background stays open', async () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); document.body.append(root);
  const [message, setMessage] = createSignal(teamFixture());
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'team', options: [], onBranch: () => {} }), root);
  try {
    setMessage({ ...ended(message()), status: 'streaming' });
    equal(root.querySelectorAll('.team-member').length, 3, 'lead still working');
    const firstCard = root.querySelector<HTMLButtonElement>('.team-member')!;
    firstCard.focus();
    equal(document.activeElement, firstCard, `focus before folding: inert ${firstCard.closest('[inert]')?.className}, active ${document.activeElement?.tagName}.${document.activeElement?.className}`);
    setMessage(ended(message())); await frames();
    const head = root.querySelector<HTMLButtonElement>('.team-fold')!;
    equal(document.activeElement, head, 'automatic folding preserves the reader keyboard position');
    equal(head.getAttribute('aria-expanded'), 'false');
    equal(root.querySelectorAll('.team-member').length, 0, 'folded cards leave the DOM');
    ok(head.textContent!.includes('Team finished · 3 pieces · 2 messages · 6 s'));
    ok(head.querySelector('.team-failure')!.textContent!.includes('Build screen: failed'));
    head.focus(); head.click();
    equal(head.getAttribute('aria-expanded'), 'true'); equal(root.querySelectorAll('.team-member').length, 3);
    equal(getComputedStyle(root.querySelector('.team-member')!).animationName, 'none');
    head.click(); equal(root.querySelectorAll('.team-member').length, 0);
    setMessage({ ...message(), tools: message().tools!.map((tool, i) => i ? tool : { ...tool, ok: undefined, team: { ...tool.team!, state: 'running' } }) });
    equal(root.querySelector('.team-fold'), null); equal(root.querySelectorAll('.team-member').length, 3);
    const elsewhere = document.createElement('button'); root.append(elsewhere); elsewhere.focus();
    setMessage(ended(message())); await frames();
    equal(document.activeElement, elsewhere, 'completion never steals a newer focus outside the team');
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: entrance tokens, reduced and none at rest, hidden work is paused', async () => {
  const restore = keepRoot();
  const root = document.createElement('div'); document.body.append(root);
  applyLook(sala());
  const dispose = render(() => createComponent(Team, { members: members(teamFixture()), streaming: true, lead: 'claude', conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    equal(getComputedStyle(root.querySelector('.team-member')!).animationName, 'arrive');
    equal(getComputedStyle(root.querySelector('.team-wire')!).animationName, 'arrive');
    for (const motion of ['reduced', 'none'] as const) {
      applyLook(sala({ motion })); await frames();
      for (const element of root.querySelectorAll('.team-member, .team-wire, .st-working svg')) equal(getComputedStyle(element).animationName, 'none');
    }
    applyLook(sala()); document.documentElement.classList.add('hidden-win');
    equal(getComputedStyle(root.querySelector('[data-call=h2] .st-working svg')!).animationPlayState, 'paused');
    await finished(root.getAnimations({ subtree: true }));
  } finally { document.documentElement.classList.remove('hidden-win'); dispose(); root.remove(); restore(); }
});

check('team block: a reopened reply with background members still gives new wire lines their entrance', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Team, { members: members(teamFixture()), streaming: false, lead: 'claude', conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    equal(root.querySelector<HTMLElement>('.team')!.dataset.rest, 'false');
    equal(root.querySelector('.team-fold'), null);
    equal(root.querySelectorAll('.team-member').length, 3);
  } finally { dispose(); root.remove(); }
});

check('team block: CLI-only nested rows stay rows and known CLI members join the helper run', () => {
  const root = document.createElement('div'); document.body.append(root);
  const cli: ToolUse = { id: 'cli', name: 'Agent', input: { description: 'Read tests', subagent_type: 'Explore' } };
  const [tools, setTools] = createSignal<ToolUse[]>([cli, { ...cli, id: 'nested', parent: 'cli' }]);
  const dispose = render(() => createComponent(Thread, { get messages(): Message[] { return [{ ...teamFixture(), tools: tools() }]; }, from: 0, conversationId: 'team', options: [], onBranch: () => {} }), root);
  try {
    equal(root.querySelector('.team'), null); equal(root.querySelectorAll('.agents .agent').length, 2);
    ok(root.querySelector('[data-parent=cli]'));
    setTools([cli, teamFixture().tools![0]!]);
    equal(root.querySelectorAll('.team-member').length, 2);
    equal(root.querySelector('[data-call=cli] .team-model'), null, 'a CLI kind is not a known model');
    equal(root.querySelectorAll('.agents .agent').length, 0);
  } finally { dispose(); root.remove(); }
});
check('team block: second and third ink contrast on darkest and lightest card surfaces', async () => {
  const luminance = (color: string) => {
    const channels = color.match(/[\d.]+/g)!.map(Number);
    ok(channels.length === 3 || channels[3] === 1, `opaque rendered color: ${color}`);
    const values = channels.slice(0, 3).map((channel) => channel / 255).map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return values[0]! * 0.2126 + values[1]! * 0.7152 + values[2]! * 0.0722;
  };
  const restore = keepRoot();
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const dispose = render(() => createComponent(Team, { members: members(teamFixture()), streaming: true, lead: 'claude', conversationId: 'team', messageId: 'team-reply' }), root);
  const reference = document.createElement('span'); root.append(reference);
  try {
    for (const vars of [themes.css.base, { ...themes.css.base, ...themes.css.presets.manha }]) {
      await applyLook(sala({ vars, motion: 'none' }));
      reference.style.backgroundColor = 'var(--s2)';
      const cards = root.querySelectorAll('.team-member'); equal(cards.length, 3);
      for (const card of cards) {
        const background = getComputedStyle(card).backgroundColor;
        equal(background, getComputedStyle(reference).backgroundColor, 'rendered card uses the second surface');
        for (const [selector, ink] of [['.team-model', 'ink-2'], ['.team-job', 'ink-3']] as const) {
          const element = card.querySelector(selector)!; ok(element, selector);
          const color = getComputedStyle(element).color;
          reference.style.color = `var(--${ink})`;
          equal(color, getComputedStyle(reference).color, `${selector} uses ${ink}`);
          const [a, b] = [luminance(color), luminance(background)].sort((a, b) => b - a);
          ok((a! + 0.05) / (b! + 0.05) >= 4.5, `${selector} against rendered card surface`);
        }
      }
    }
  } finally { dispose(); root.remove(); restore(); }
});

const looks = () => Object.values(themes.css.themes).flatMap((theme) => [{}, ...Object.values(themes.css.presets)].map((preset) => ({ ...themes.css.base, ...theme, ...preset })));
const answer: TeamMessage = { id: 'a', from: 'lead', to: 'h1', fromName: 'lead', toName: 'Read config', kind: 'answer', text: 'Keep it compatible.', at: 1400, question: 'q' };
const answered = (message: Message): Message => ({ ...message, tools: message.tools!.map((tool, i) => i ? tool : { ...tool, team: { ...tool.team!, state: 'running', messages: [{ ...ask, state: 'answered' }, answer] } }) });

check('team block: lead names its real model and uses urgent, running and ended sentences', () => {
  const root = document.createElement('div'); document.body.append(root);
  const [message, setMessage] = createSignal(teamFixture());
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, lead: 'claude', leadModel: 'claude-sonnet-5-5', conversationId: 'team', messageId: 'team-reply' }), root);
  const sentence = () => root.querySelector('.team-lead-sentence')!.textContent;
  try {
    equal(root.querySelector('.team-lead > span:not(.mk)')!.textContent, 'Lead');
    equal(root.querySelector('.team-lead .team-model')!.textContent, 'Sonnet 5.5');
    equal(root.querySelector<HTMLElement>('.team-lead')!.tabIndex, -1);
    equal(sentence(), 'Answering Read config');
    setMessage(answered(message())); equal(sentence(), 'Handed out 3 pieces');
    setMessage(ended(message())); equal(sentence(), 'Handed out 3 pieces · answered 1 question');
    setMessage(ended(teamFixture())); equal(sentence(), 'Handed out 3 pieces');
    const one = answered(teamFixture()); one.tools = one.tools!.slice(0, 1);
    setMessage(ended(one)); equal(sentence(), 'Handed out 1 piece · answered 1 question');
  } finally { dispose(); root.remove(); }
});

check('team block: segments, drops, pill words and glyphs agree with every member state', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const initial = teamFixture();
  initial.tools = initial.tools!.map((tool, i) => i === 1 ? { ...tool, sign: 'waiting' } : i === 2 ? { ...tool, ok: true, ended: 5000, output: '**Delivered the screen.** All checks passed.' } : tool);
  const [message, setMessage] = createSignal(initial);
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    equal(root.querySelector('.team-segments')!.getAttribute('aria-hidden'), 'true');
    deepEqual([...root.querySelectorAll<HTMLElement>('.team-segments i')].map((segment) => segment.dataset.state), ['waitingLead', 'waiting', 'done']);
    equal(root.querySelectorAll('.team-drop[aria-hidden=true]').length, 3);
    const drops = [...root.querySelectorAll<HTMLElement>('.team-drop')];
    ok(getComputedStyle(drops[0]!).borderLeftColor !== getComputedStyle(drops[1]!).borderLeftColor, 'neutral and amber drops differ');
    ok(root.querySelector('[data-call=h3] .st-concluded'), 'finished has a check even without counted checks');
    equal(root.querySelector('[data-call=h3] .team-outcome')!.textContent, 'Delivered the screen.');
    equal(root.querySelector('[data-call=h3] .team-duration')!.textContent, '3 s');
    equal(root.querySelector('[data-call=h1] .team-state')!.textContent, 'waiting for lead');
    ok(root.querySelector('[data-call=h2] .st-waiting'));
    const rail = () => getComputedStyle(root.querySelector('.team-grid')!, '::before');
    equal(rail().borderTopWidth, '1px');
    root.style.width = '280px'; equal(rail().borderLeftWidth, '1px'); equal(rail().borderTopWidth, '0px');
    for (const drop of drops) equal(getComputedStyle(drop).borderTopWidth, '1px', 'stacked stub');
    setMessage(answered(message()));
    equal(root.querySelector<HTMLElement>('.team-segments i')!.dataset.state, 'running');
    equal(getComputedStyle(drops[0]!).borderTopColor, getComputedStyle(root.querySelector('[data-call=h1]')!).borderTopColor, 'answered drop and working edge share the accent tone');
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: visible named addresses and all typed chips, with answers nested once', () => {
  const root = document.createElement('div'); document.body.append(root);
  const message = answered(teamFixture());
  const dispose = render(() => createComponent(Team, { members: members(message), streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    deepEqual([...root.querySelectorAll('.team-chip')].map((chip) => chip.textContent), ['Note', 'Question', 'Answer']);
    equal(root.querySelector('[data-message=t] .team-address')!.textContent, 'Build screen → Review changes');
    equal(root.querySelector('[data-message=q] .team-address')!.textContent, 'Read config → Lead');
    equal(root.querySelector('[data-message=q] .team-answer .team-address')!.textContent, 'Lead → Read config');
    equal(root.querySelectorAll('[data-message=t]').length, 1);
    equal(root.querySelectorAll('[data-message=q] .team-answer').length, 1);
    ok(!root.querySelector('.team-address .sr-only'), 'names are visible');
  } finally { dispose(); root.remove(); }
  const lapsed = teamFixture();
  lapsed.tools![0]!.team!.messages = [{ ...ask, state: 'lapsed' }, { ...answer, id: 'no', kind: 'noAnswer', from: 'engine', text: 'Decide yourself.' }];
  const release = render(() => createComponent(Team, { members: members(lapsed), streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    equal(root.querySelectorAll('[data-kind=noAnswer]').length, 1);
    equal(root.querySelector('[data-kind=noAnswer]')!.textContent, 'No answer');
    equal(root.querySelector('.team-lapsed'), null, 'the recorded no-answer is not repeated');
  } finally { release(); root.remove(); }
});

check('team block: outcomes are plain first sentences, with no invented report', () => {
  equal(teamOutcome(''), '');
  equal(teamOutcome('**Kept** [old files](https://example.test). A second sentence.'), 'Kept old files.');
  equal(teamOutcome('Supports version 2.5. Next sentence.'), 'Supports version 2.5.');
  equal(teamOutcome('<script>bad()</script>Safe text. Next sentence.'), 'Safe text.');
  equal(teamOutcome('`config.ts` is compatible.\n\nMore details.'), 'config.ts is compatible.');
});

check('team block: opened receipt is deliveries then counted messages, and omits lead and branches', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); document.body.append(root);
  const message = ended(answered(teamFixture()));
  message.tools![0]!.output = 'Kept the old format. More detail.';
  const dispose = render(() => createComponent(Team, { members: members(message), streaming: false, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    const fold = root.querySelector<HTMLButtonElement>('.team-fold')!;
    ok(fold.textContent!.startsWith('Team finished · 3 pieces · 3 messages · 6 s'));
    equal(root.querySelector('.team-deliveries'), null);
    equal(fold.querySelectorAll('svg').length, 2, 'caret and receipt check');
    fold.click();
    equal(root.querySelectorAll('.team-deliveries[role=list] > [role=listitem]').length, 3);
    equal(root.querySelector('.team-lead'), null); equal(root.querySelector('.team-drop'), null);
    equal(root.querySelector('.team-grid'), null);
    equal(root.querySelector('.team-messages-label')!.textContent, 'Messages (3)');
    equal(root.querySelector('[data-call=h1] .team-outcome')!.textContent, 'Kept the old format.');
    equal(root.querySelector('[data-call=h3] .team-outcome'), null, 'missing report produces no placeholder');
    ok(root.querySelector('[data-call=h2] .st-failed'));
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: every theme, preset and language fits 280 and 180 at maximum text, live and delivered', () => {
  const restore = keepRoot();
  const root = document.createElement('div'); root.style.cssText = 'position:fixed;left:0;top:0'; document.body.append(root);
  const initial = teamFixture();
  initial.tools![0]!.team!.title = 'Read a very long configuration piece without losing its full accessible name';
  const [message, setMessage] = createSignal(initial);
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, get streaming() { return message().status === 'streaming'; }, lead: 'claude', leadModel: 'claude-sonnet-5-5', conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    for (const vars of looks()) for (const width of [280, 180]) {
      root.style.width = `${width}px`;
      applyLook(sala({ vars: { ...vars, 'fs-xs': '18px', 'fs-sm': '19px', 'fs-ui': '20px' }, motion: 'none' }));
      for (const language of ['en', 'pt-BR', 'es', 'fr', 'de']) {
        useLanguage(language);
        for (const state of ['live', 'folded', 'opened']) {
          setMessage(state === 'live' ? initial : ended(answered(initial)));
          if (state === 'opened') root.querySelector<HTMLButtonElement>('.team-fold')!.click();
          ok(root.scrollWidth <= root.clientWidth, `${language} ${width} ${state}: root overflow`);
          const bounds = root.getBoundingClientRect();
          for (const element of root.querySelectorAll<HTMLElement>('.team-lead, .team-member, .team-state, .team-chip, .team-address, .team-wire, .team-outcome')) {
            const box = element.getBoundingClientRect();
            ok(box.right <= bounds.right + 1 && box.left >= bounds.left - 1, `${language} ${width} ${state}: ${element.className} bounds`);
          }
          for (const element of root.querySelectorAll<HTMLElement>('.team-model, .team-title, .team-state > span:last-child, .team-address, .team-chip, .team-wire-text, .team-outcome')) {
            if (element.scrollWidth > element.clientWidth) ok(getComputedStyle(element).textOverflow === 'ellipsis' || getComputedStyle(element).webkitLineClamp !== 'none', `${element.className}: clipped text needs ellipsis`);
          }
        }
      }
    }
  } finally { dispose(); root.remove(); restore(); useLanguage('en'); }
});

check('team block: new messages ring their sender once; updates keep cards, rails and old lines still', async () => {
  const restore = keepRoot(); applyLook(sala());
  const root = document.createElement('div'); document.body.append(root);
  const [message, setMessage] = createSignal(teamFixture());
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, lead: 'claude', conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    equal(root.querySelector('.team-pulse'), null, 'restored messages do not ring');
    const member = root.querySelector('[data-call=h2]')!;
    const cardAnimation = member.getAnimations()[0];
    const rail = root.querySelector('.team-grid')!;
    const railAnimations = rail.getAnimations();
    const line = root.querySelector('[data-message=t] .team-wire')!;
    const note: TeamMessage = { ...tell, id: 'new', at: 1500, text: 'New note.' };
    setMessage({ ...message(), tools: message().tools!.map((tool, i) => i === 1 ? { ...tool, team: { ...tool.team!, messages: [tell, note] } } : tool) });
    const ring = root.querySelector('[data-call=h2] .team-pulse')!;
    ok(ring); equal(ring.getAttribute('data-sender-message'), 'new');
    equal(getComputedStyle(ring).animationName, 'team-ring');
    equal(getComputedStyle(ring).animationIterationCount, '1');
    equal(root.querySelectorAll('.team-pulse').length, 1, [...root.querySelectorAll('.team-pulse')].map((ring) => `${ring.closest('[data-call]')?.getAttribute('data-call') ?? 'lead'}:${ring.getAttribute('data-sender-message')}`).join(', '));
    equal(root.querySelector('[data-call=h2]'), member);
    equal(root.querySelector('[data-message=t] .team-wire'), line);
    equal(member.getAnimations()[0], cardAnimation, 'no repeated card entrance');
    deepEqual(rail.getAnimations(), railAnimations, 'no repeated rail entrance');
    setMessage({ ...message(), text: 'Another streamed word.' }); equal(root.querySelector('.team-pulse'), ring);
    for (const motion of ['reduced', 'none'] as const) {
      applyLook(sala({ motion }));
      equal(getComputedStyle(ring).display, 'none');
      for (const element of root.querySelectorAll('.team-drop, .team-pulse, .team-member, .team-wire')) equal(getComputedStyle(element).animationName, 'none');
      equal(getComputedStyle(rail, '::before').animationName, 'none');
    }
    applyLook(sala()); document.documentElement.classList.add('hidden-win');
    equal(getComputedStyle(ring).animationPlayState, 'paused');
    equal(getComputedStyle(rail, '::before').animationPlayState, 'paused');
    document.documentElement.classList.remove('hidden-win');
    await finished(root.getAnimations({ subtree: true }));
    equal(getComputedStyle(ring).opacity, '0');
    const extra = { ...message().tools![1]!, id: 'h4' };
    setMessage({ ...message(), tools: [...message().tools!, extra] });
    equal(root.querySelectorAll('.team-drop').length, 4);
    await finished(root.querySelector('[data-call=h4]')!.getAnimations());
    root.style.width = '500px';
    equal(new Set([...root.querySelectorAll('.team-member')].map((card) => card.getBoundingClientRect().top)).size, 1, 'four drops still meet a single rail at intermediate width');
    equal(getComputedStyle(root.querySelector('[data-call=h4] .team-drop')!).animationIterationCount, '1');
  } finally { document.documentElement.classList.remove('hidden-win'); dispose(); root.remove(); restore(); }
});

check('team block: rendered contrast of every state pill and typed chip in every theme and preset', () => {
  const luminance = (color: string) => {
    const channels = color.match(/[\d.]+/g)!.map(Number);
    ok(channels.length === 3 || channels[3] === 1, `opaque rendered color: ${color}`);
    const values = channels.slice(0, 3).map((c) => c / 255).map((c) => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return values[0]! * 0.2126 + values[1]! * 0.7152 + values[2]! * 0.0722;
  };
  const restore = keepRoot();
  const root = document.createElement('div'); document.body.append(root);
  const message = answered(teamFixture());
  const steps = members(message);
  const states = ['running', 'waitingLead', 'waiting', 'stuck', 'done', 'failed', 'stopped', 'declined'] as const;
  const fixture = states.map((state, i) => ({ ...steps[i % 3]!, call: { ...steps[i % 3]!.call, id: `contrast${i}`, team: { ...steps[i % 3]!.call.team!, state: state === 'waitingLead' ? 'waitingLead' as const : 'running' as const }, ok: state === 'done' ? true : ['failed', 'declined'].includes(state) ? false : undefined, declined: state === 'declined', interrupted: state === 'stopped', sign: state === 'waiting' ? 'waiting' as const : state === 'stuck' ? 'silent' as const : undefined } }));
  fixture[0]!.call.team!.messages = [{ ...ask, state: 'lapsed' }, answer, { ...tell, id: 'contrastNote' }, { ...answer, id: 'contrastNo', kind: 'noAnswer', question: undefined }];
  const dispose = render(() => createComponent(Team, { members: fixture, streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    for (const vars of looks()) {
      applyLook(sala({ vars, motion: 'none' }));
      for (const element of root.querySelectorAll('.team-state, .team-chip')) {
        const style = getComputedStyle(element);
        const [a, b] = [luminance(style.color), luminance(style.backgroundColor)].sort((a, b) => b - a);
        ok((a! + 0.05) / (b! + 0.05) >= 4.5, `${element.textContent}: ${style.color} on ${style.backgroundColor}`);
      }
      equal(root.querySelectorAll('.team-state').length, states.length);
      equal(root.querySelectorAll('.team-chip').length, 4);
    }
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: system reduced motion keeps branch, arrivals and sender ring at rest', () => {
  const rules = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]);
  const media = rules.find((rule) => rule instanceof CSSMediaRule && rule.cssText.includes('.team .team-pulse')) as CSSMediaRule;
  ok(media, 'the team has an explicit system preference rule');
  const restore = keepRoot();
  const previous = media.media.mediaText;
  const root = document.createElement('div'); document.body.append(root);
  applyLook(sala());
  const dispose = render(() => createComponent(Team, { members: members(teamFixture()), streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    media.media.mediaText = 'all';
    for (const element of root.querySelectorAll('.team-member, .team-wire, .team-drop, .st-working svg')) equal(getComputedStyle(element).animationName, 'none');
    equal(getComputedStyle(root.querySelector('.team-grid')!, '::before').animationName, 'none');
  } finally { media.media.mediaText = previous; dispose(); root.remove(); restore(); }
});

check('team block: live activity stays under its pill and is replaced by the delivered first sentence', () => {
  const root = document.createElement('div'); document.body.append(root);
  const initial = teamFixture();
  initial.tools!.push({ id: 'read-settings', name: 'Read', parent: 'h2', input: { file_path: 'src/settings.ts' } });
  const [message, setMessage] = createSignal(initial);
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    const card = root.querySelector('[data-call=h2]')!;
    ok(card.querySelector('.team-activity')!.textContent!.includes('src/settings.ts'));
    equal(card.querySelector('.team-state')!.nextElementSibling!.className, 'team-detail');
    equal(card.querySelector('.team-detail > .team-activity')!.textContent, card.querySelector('.team-activity')!.textContent);
    setMessage({ ...message(), tools: message().tools!.map((tool) => tool.id === 'h2' ? { ...tool, ok: true, ended: 5100, output: 'Updated the settings reader. More details follow.' } : tool) });
    equal(root.querySelector('[data-call=h2]'), card, 'completion keeps the card identity');
    equal(card.querySelector('.team-activity'), null);
    equal(card.querySelector('.team-outcome')!.textContent, 'Updated the settings reader.');
    equal(card.querySelector('.team-duration')!.textContent, '4 s');
  } finally { dispose(); root.remove(); }
});

check('team block: jobs never shrink beside ellipsized models and cards share a compact row height', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const steps = members(teamFixture()).map((step) => ({ ...step, model: 'a-very-long-model-name-that-must-give-room-to-the-job-word' }));
  const dispose = render(() => createComponent(Team, { members: steps, streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    const cards = [...root.querySelectorAll<HTMLElement>('.team-member')];
    equal(new Set(cards.map((card) => card.getBoundingClientRect().height)).size, 1, 'equal row height');
    for (const card of cards) {
      const model = card.querySelector<HTMLElement>('.team-model')!;
      const job = card.querySelector<HTMLElement>('.team-job')!;
      equal(model.parentElement, job.parentElement, 'job belongs on the model line');
      equal(model.getBoundingClientRect().top, job.getBoundingClientRect().top);
      equal(getComputedStyle(model).textOverflow, 'ellipsis');
      ok(model.scrollWidth > model.clientWidth, 'model takes the truncation');
      equal(getComputedStyle(job).flexShrink, '0');
      ok(job.scrollWidth <= job.clientWidth, 'the whole job stays visible');
      const pill = card.querySelector('.team-state')!.getBoundingClientRect();
      ok(pill.top - model.getBoundingClientRect().bottom <= 7, 'no extra job row');
    }
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: every locale starts live pill words lowercase, preserving native noun casing', () => {
  const root = document.createElement('div'); document.body.append(root);
  const initial = teamFixture();
  initial.tools![1]!.sign = 'waiting';
  initial.tools![2]!.ok = true;
  const dispose = render(() => createComponent(Team, { members: members(initial), streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de']) {
      useLanguage(language);
      equal(root.querySelector('[data-call=h1] .team-state')!.textContent, t('team.waitingLead'));
      for (const pill of root.querySelectorAll('.team-state')) {
        const first = pill.textContent!.charAt(0);
        equal(first, first.toLocaleLowerCase(language));
      }
    }
  } finally { dispose(); root.remove(); useLanguage('en'); }
});

check('team block: rail leaves the lead mark and ends at the last drop, working edges reset on finish', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const [message, setMessage] = createSignal(answered(teamFixture()));
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, lead: 'claude', conversationId: 'team', messageId: 'team-reply' }), root);
  const reference = document.createElement('span'); root.append(reference);
  try {
    for (const count of [1, 3, 4]) {
      const m = answered(teamFixture());
      m.tools = count === 1 ? m.tools!.slice(0, 1) : count === 4 ? [...m.tools!, { ...m.tools![2]!, id: 'h4' }] : m.tools;
      setMessage(m);
      const grid = root.querySelector('.team-grid')!;
      const rail = getComputedStyle(grid, '::before');
      const stem = getComputedStyle(grid, '::after');
      const left = grid.getBoundingClientRect().left;
      const mark = root.querySelector('.team-lead .mk')!.getBoundingClientRect();
      const drop = [...root.querySelectorAll('.team-drop')].at(-1)!.getBoundingClientRect();
      ok(Math.abs(left + parseFloat(stem.left) - (mark.left + mark.width / 2)) < 1, 'stem under lead mark');
      equal(stem.top, '-26px', 'stem leaves the bottom of the lead strip');
      ok(Math.abs(left + parseFloat(rail.left) - (mark.left + mark.width / 2)) < 1, 'rail starts at the stem');
      ok(Math.abs(left + parseFloat(rail.left) + parseFloat(rail.width) - drop.left) < 1, 'rail ends at last drop');
    }
    reference.style.borderColor = 'var(--accent-soft)';
    const working = root.querySelector('[data-call=h1]')!;
    equal(getComputedStyle(working).borderTopColor, getComputedStyle(reference).borderTopColor);
    equal(getComputedStyle(working.querySelector('.team-drop')!).borderLeftColor, getComputedStyle(reference).borderTopColor);
    setMessage({ ...message(), tools: message().tools!.map((tool) => ({ ...tool, ok: true })) });
    reference.style.borderColor = 'var(--line)';
    equal(getComputedStyle(root.querySelector('[data-call=h1]')!).borderTopColor, getComputedStyle(reference).borderTopColor, 'finished edge is plain');
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: one detail line follows the state, from activity to question to timed outcome', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); root.style.width = '660px'; document.body.append(root);
  const initial = teamFixture();
  initial.tools!.push({ id: 'edit', name: 'Edit', parent: 'h2', input: { file_path: 'ui/Team.tsx' } });
  initial.tools = initial.tools!.filter((tool) => tool.parent !== 'h3');
  const [message, setMessage] = createSignal(initial);
  const dispose = render(() => createComponent(Team, { get members() { return members(message()); }, streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    const card = () => root.querySelector('[data-call=h2]')!;
    equal(root.querySelector('[data-call=h1] .team-member-question')!.textContent, ask.text);
    ok(card().querySelector('.team-activity')!.textContent!.includes('ui/Team.tsx'));
    equal(root.querySelector('[data-call=h3] .team-detail'), null, 'no invented activity');
    const question = 'Should the screen keep the old configuration format?';
    setMessage({ ...message(), tools: [...message().tools!.map((tool) => tool.id === 'h2' ? { ...tool, sign: 'waiting' as const } : tool), { id: 'person-question', name: 'AskUserQuestion', parent: 'h2', input: { questions: [{ question, options: [] }] } }] });
    equal(card().querySelector('.team-member-question')!.textContent, question);
    equal(card().querySelector('.team-activity'), null, 'waiting replaces activity with the actual question');
    setMessage({ ...message(), tools: message().tools!.map((tool) => tool.id === 'h2' ? { ...tool, ok: true, ended: 5100, output: 'Built the requested screen. Extra detail.' } : tool) });
    equal(card().querySelector('.team-member-question'), null);
    equal(card().querySelectorAll('.team-detail').length, 1);
    const detail = card().querySelector('.team-detail')!;
    equal(detail.querySelector('.team-outcome')!.textContent, 'Built the requested screen.');
    const outcome = detail.querySelector('.team-outcome')!.getBoundingClientRect();
    const took = detail.querySelector('.team-duration')!.getBoundingClientRect();
    equal(outcome.top, took.top, 'duration shares outcome line');
    ok(took.left >= outcome.right, 'duration at the right end');
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: finished deliveries show a named check and duration beside the title at every width', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); document.body.append(root);
  const message = ended(teamFixture()); message.tools![0]!.output = 'Kept the old format.';
  const dispose = render(() => createComponent(Team, { members: members(message), streaming: false, conversationId: 'team', messageId: 'team-reply' }), root);
  try {
    root.querySelector<HTMLButtonElement>('.team-fold')!.click();
    for (const width of [660, 280, 180]) {
      root.style.width = `${width}px`;
      const card = root.querySelector('[data-call=h1]')!;
      const state = card.querySelector('.team-state')!;
      equal(state.textContent, ''); equal(state.querySelector('[role=img]')!.getAttribute('aria-label'), 'finished');
      equal(state.querySelectorAll('.st-concluded').length, 1);
      equal(getComputedStyle(state).backgroundColor, 'rgba(0, 0, 0, 0)', 'no finished pill surface');
      const title = card.querySelector('.team-piece')!.getBoundingClientRect();
      const took = card.querySelector('.team-duration')!.getBoundingClientRect();
      const check = state.getBoundingClientRect();
      ok(took.top >= title.top && took.bottom <= title.bottom + 1, 'duration on title line');
      ok(check.top >= title.top && check.bottom <= title.bottom + 1, 'check on title line');
      ok(took.right <= check.left, 'duration next to check');
      ok(card.querySelector('.team-detail')!.getBoundingClientRect().top > title.bottom, 'outcome below title');
      equal(root.querySelector('[data-call=h2] .team-state')!.textContent, 'failed', 'failures retain words');
    }
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: message text follows names inline and wraps naturally, using first and second inks', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); document.body.append(root);
  const message = teamFixture();
  message.tools![0]!.team!.messages = []; message.tools![0]!.team!.state = 'running';
  const note = { ...tell, text: 'Ready.' }; message.tools![1]!.team!.messages = [note]; message.tools![2]!.team!.messages = [note];
  const dispose = render(() => createComponent(Team, { members: members(message), streaming: true, conversationId: 'team', messageId: 'team-reply' }), root);
  const reference = document.createElement('span'); root.append(reference);
  try {
    const wire = root.querySelector<HTMLButtonElement>('.team-wire')!;
    const address = wire.querySelector('.team-address')!;
    const text = wire.querySelector('.team-wire-text')!;
    root.style.width = '660px';
    equal(address.getBoundingClientRect().top, text.getBoundingClientRect().top, 'names and short text share a line');
    reference.style.color = 'var(--ink)'; equal(getComputedStyle(text).color, getComputedStyle(reference).color);
    reference.style.color = 'var(--ink-2)'; equal(getComputedStyle(address).color, getComputedStyle(reference).color);
    for (const width of [280, 180]) {
      root.style.width = `${width}px`; wire.click();
      const range = document.createRange(); range.selectNodeContents(text);
      ok([...range.getClientRects()].every((rect) => rect.right <= root.getBoundingClientRect().right + 1), 'wrapped text fits');
      equal(getComputedStyle(wire.querySelector('.team-wire-body')!).webkitLineClamp, 'none', 'expanded text is complete');
      wire.click(); equal(getComputedStyle(wire.querySelector('.team-wire-body')!).webkitLineClamp, '2', 'collapsed exchange stays bounded');
    }
  } finally { dispose(); root.remove(); restore(); }
});

check('team block: lead facts have middle dots and the sentence uses the strongest ink', () => {
  const restore = keepRoot(); applyLook(sala({ motion: 'none' }));
  const root = document.createElement('div'); document.body.append(root);
  const [model, setModel] = createSignal<string | undefined>('claude-sonnet-5-5');
  const dispose = render(() => createComponent(Team, { members: members(teamFixture()), streaming: true, lead: 'claude', get leadModel() { return model(); }, conversationId: 'team', messageId: 'team-reply' }), root);
  const reference = document.createElement('span'); root.append(reference);
  try {
    const lead = root.querySelector('.team-lead')!;
    equal(lead.querySelectorAll('.team-dot[aria-hidden=true]').length, 2);
    reference.style.color = 'var(--ink)'; equal(getComputedStyle(lead.querySelector('.team-lead-sentence')!).color, getComputedStyle(reference).color);
    reference.style.color = 'var(--ink-2)'; equal(getComputedStyle(lead.querySelector('.team-model')!).color, getComputedStyle(reference).color);
    setModel(undefined); equal(lead.querySelectorAll('.team-dot').length, 1, 'no empty model fact');
  } finally { dispose(); root.remove(); restore(); }
});
