import { createComponent, createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type BackgroundCommand, type Follow, type Message } from '../../ui/api.ts';
import { backgroundOf, createBackgroundCache, followedIn, hasReference, liveReferences, referenceLines, workStatus } from '../../ui/background.ts';
import { DetailsLine, DetailsPane, showDetails } from '../../ui/Details.tsx';
import { openedAgent, showAgent } from '../../ui/AgentChat.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import fixture from '../fixtures/background-live.json';
import { check, deepEqual, equal, ok } from './check.ts';

const reply = (): Message => ({ id: 'reply', role: 'assistant', at: 1, status: 'streaming', text: fixture.text, tools: structuredClone(fixture.tools), follow: structuredClone(fixture.follow) as Follow });
const command = (): BackgroundCommand => structuredClone(fixture.command) as BackgroundCommand;
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

function pane(messages: () => Message[], commands: () => BackgroundCommand[] = () => []) {
  const host = document.createElement('div');
  host.style.setProperty('--amber-ink', 'rgb(200, 90, 0)');
  host.style.setProperty('--ink-3', 'rgb(90, 100, 110)');
  document.body.append(host);
  const dispose = render(() => createComponent(DetailsPane, {
    conversationId: 'background-live', get messages() { return messages(); }, get background() { return commands(); },
    changes: { id: 'background-live', files: [], loading: false }, onBack: () => {}, onClose: () => {},
  }), host);
  return { host, close: () => { dispose(); host.remove(); } };
}

check('background review 1: R174 helper words and neutral missing reports share the running-state rule', async () => {
  const [messages, setMessages] = createSignal<Message[]>([]);
  const view = pane(messages);
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(DetailsLine, { conversationId: 'background-live', get messages() { return messages(); } }), host);
  try {
    for (const sign of ['repeat', 'failing', 'silent', 'waiting'] as const) {
      setMessages([{ ...reply(), needsYou: true, follow: undefined, tools: [{ id: 'helper', name: 'Agent', input: { description: 'Check the preview' }, sign }] }]);
      await settle();
      const row = view.host.querySelector<HTMLElement>('[data-work="reply:helper"]')!;
      ok(row.textContent!.includes(t(sign === 'waiting' ? 'agent.waiting' : 'agent.stuck')));
      if (sign !== 'waiting') ok(row.textContent!.includes(t(`agent.stuck.${sign}`)), 'R174 says what was observed');
      equal(getComputedStyle(row.querySelector('.work-state')!).color, 'rgb(200, 90, 0)');
      ok(host.querySelector('.det-line.need'), 'the engine needs-you stays visible without the pane');
      equal(backgroundOf(messages(), [], 'background-live').filter((r) => workStatus(r).running).length, 0, 'needing an answer never claims to be running');
    }
    setMessages([{ ...reply(), status: 'done', follow: undefined, tools: [
      { id: 'monitor', name: 'Monitor', input: {} },
      { id: 'shell', name: 'Bash', input: { command: 'long task', run_in_background: true } },
    ] }, { id: 'newer', role: 'assistant', at: 2, status: 'done', text: '', needsYou: false }]);
    await settle();
    for (const row of view.host.querySelectorAll('[data-state="unreported"]')) {
      ok(row.textContent!.includes(t('background.unreported')));
      equal(row.getAttribute('data-need'), 'false');
      equal(getComputedStyle(row.querySelector('.work-state')!).color, 'rgb(90, 100, 110)', 'no problem was observed');
    }
    equal(view.host.querySelectorAll('[data-state="unreported"]').length, 2);
    equal(host.querySelector('.det-line.need'), null);
    equal(host.querySelector('.det-line')!.getAttribute('data-state'), 'details.line', 'unknown old work is not counted as running');
    for (const state of ['pending', 'failing', 'conflict'] as const) {
      const f: Follow = { ...fixture.follow, state, held: state !== 'pending', failed: ['gate'] };
      setMessages([{ ...reply(), tools: [], follow: f, needsYou: state !== 'pending' }]);
      await settle();
      const row = view.host.querySelector('[data-work="reply:follow"]')!;
      ok(row.textContent!.includes(t(state === 'failing' ? 'follow.held' : `follow.${state}`, { pr: '#93', checks: 'gate' })));
      equal(row.getAttribute('data-need'), String(workStatus({ follow: f }).need));
    }
  } finally { dispose(); host.remove(); view.close(); }
});

check('technical details shows the newest session start card as sent, folded until opened', async () => {
  const sent = '```faber-start-card\n<b>Rules</b> of this project\n```';
  const [messages] = createSignal<Message[]>([
    { ...reply(), id: 'old', status: 'done', tools: [], toolKit: { ids: [], jobs: [], method: 'x', card: 'older card' } },
    { ...reply(), status: 'done', tools: [], toolKit: { ids: [], jobs: [], method: 'x', card: sent } },
  ]);
  const view = pane(messages);
  try {
    const group = view.host.querySelector('.start-card')!;
    equal(group.querySelector('h3')!.textContent, t('details.group.startCard'));
    equal(group.querySelector('pre'), null, 'folded until opened');
    const details = group.querySelector<HTMLDetailsElement>('details')!;
    details.open = true; details.dispatchEvent(new Event('toggle')); await settle();
    equal(group.querySelector('pre')!.textContent, sent, 'word for word, never as markup');
    equal(group.querySelector('b'), null);
  } finally { view.close(); }
  const [bare] = createSignal<Message[]>([{ ...reply(), status: 'done', tools: [] }]);
  const none = pane(bare);
  try { equal(none.host.querySelector('.start-card'), null, 'no card, no group'); } finally { none.close(); }
});

check('background review 2 and 4: terminal follows move to saved changes; reply references cannot claim a save', async () => {
  const [messages, setMessages] = createSignal<Message[]>([{ ...reply(), status: 'done', tools: [], text: 'Read issue #12 and step #2.', follow: undefined }]);
  const view = pane(messages);
  try {
    equal(view.host.querySelector('.saved-changes'), null, 'no receipt or follow means nothing saved');
    const references = view.host.querySelector('.reply-references')!;
    equal(references.querySelector('h3')!.textContent, t('background.references'));
    const details = references.querySelector<HTMLDetailsElement>('details')!;
    details.open = true; details.dispatchEvent(new Event('toggle')); await settle();
    ok(references.textContent!.includes('#12'));
    equal(references.querySelector('a'), null, 'unknown references stay literal in their own group');
    for (const state of ['merged', 'closed'] as const) {
      setMessages([{ ...reply(), tools: [], text: fixture.follow.url, follow: { ...fixture.follow, state } }]);
      await settle();
      equal(view.host.querySelector('.background-work [data-work="reply:follow"]'), null, `${state} is no longer background work, even before its reply ends`);
      ok(view.host.querySelector(`.saved-changes .follow[data-state="${state}"]`));
      ok(references.querySelector('.live-reference'), 'bare known PR URLs are discovered before LiveText renders them');
    }
  } finally { view.close(); }
});

check('background review 3 and 8: pending means checks running and counts mean running in every language', async () => {
  const view = pane(() => [{ ...reply(), tools: [] }]);
  try {
    for (const language of LANGUAGES) {
      useLanguage(language.code); await settle();
      ok(view.host.querySelector('.follow')!.textContent!.includes(t('follow.pending', { pr: '#93' })));
      ok(t('follow.pending', { pr: '#93' }).includes(t('background.link.pending')));
    }
    useLanguage('en');
    equal(t('background.count', { n: 2 }), '2 things still running');
  } finally { view.close(); useLanguage('en'); }
});

check('background review 5: token and output updates reuse 020 classification and each command description', async () => {
  let reads = 0;
  const tools: NonNullable<Message['tools']> = Array.from({ length: 300 }, (_, i) => ({ id: `read-${i}`, get name() { reads++; return 'Read'; }, input: {}, ok: true }));
  tools.push({ id: 'helper', name: 'Agent', input: { get description() { reads++; return 'Check the preview'; } } });
  tools.push({ id: 'shell', name: 'Bash', input: { command: fixture.command.command, get description() { reads++; return 'Check the preview'; }, run_in_background: true } });
  const m: Message = { ...reply(), tools };
  const cache = createBackgroundCache();
  backgroundOf([m], [command()], 'background-live', cache);
  const before = reads;
  for (let i = 0; i < 20; i++) backgroundOf([{ ...m, text: `Words ${i}` }], [{ ...command(), output: `Output ${i}` }], 'background-live', cache);
  equal(reads, before, 'no old tool-name/title walk or shell description match on a token/output event');
  const [lineMessages, setLineMessages] = createSignal([m]);
  const lineHost = document.createElement('div');
  document.body.append(lineHost);
  const lineCommands = [command()];
  const disposeLine = render(() => createComponent(DetailsLine, { conversationId: 'background-live', get messages() { return lineMessages(); }, get background() { return lineCommands; } }), lineHost);
  try {
    await settle();
    const lineReads = reads;
    for (let i = 0; i < 5; i++) { setLineMessages([{ ...m, text: `Words ${i}` }]); await settle(); }
    equal(reads, lineReads, 'review 4: the always-mounted thread line shares its index too, so a text tick walks no tools');
  } finally { disposeLine(); lineHost.remove(); }
  const [messages, setMessages] = createSignal([m]);
  const view = pane(messages, () => [command()]);
  try {
    const names = reads;
    setMessages([{ ...m, text: 'More words.' }]); await settle();
    equal(reads, names, 'the real pane shares the index with its existing facts');
  } finally { view.close(); }
  const refs = new Map<string, { text: string; urls: string; lines: string[] }>();
  const first = referenceLines([{ ...reply(), text: 'See #93.\nNothing here.' }], refs);
  deepEqual(first, ['See #93.']);
  const cached = refs.get('reply')!.lines;
  referenceLines([{ ...reply(), text: 'See #93.\nNothing here.' }], refs);
  ok(refs.get('reply')!.lines === cached, 'unchanged text keeps its cached lines');
  deepEqual(referenceLines([{ ...reply(), text: 'No references.' }], refs), [], 'a streaming reply is searched again when its text changes');
  deepEqual([...refs.keys()], ['reply']);
  referenceLines([], refs);
  equal(refs.size, 0, 'replies that left the conversation leave the cache');
  const bare = pane(() => [{ ...reply(), text: 'Nothing to point at.', tools: [], follow: undefined }]);
  try {
    equal(bare.host.querySelector('.reply-references'), null, 'no reference line, no empty group');
  } finally { bare.close(); }
  const translated = pane(() => [{ ...reply(), tools: [{ id: 'verify', name: 'Verify', input: { level: 'full' } }] }]);
  try {
    useLanguage('pt-BR'); await settle();
    ok(translated.host.querySelector('[data-work="reply:verify"]')!.textContent!.includes(t('tool.verifyFull')), 'a cached label follows the screen language');
  } finally { translated.close(); useLanguage('en'); }
});

check('background review 7: Projects keeps its R161/R174 helper rows and only the new list stays out of Thread', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'background-live', area: 'project', options: [], from: 0, onBranch: () => {},
    messages: [{ id: 'q', role: 'user', at: 1, text: 'Check the preview.' }, { ...reply(), needsYou: true, tools: [
      { id: 'helper', name: 'Agent', input: { description: 'Check the preview' }, sign: 'repeat' },
    ] }], background: [command()],
  }), host);
  try {
    await settle();
    equal(host.querySelectorAll('.agents .agent[data-state="stuck"]').length, 1, 'fails with task 019’s Projects-only hiding condition');
    ok(host.querySelector('.agent-stuck')!.textContent!.includes(t('agent.stuck.repeat')));
    equal(host.querySelectorAll('.det-line').length, 1);
    equal(host.querySelector('.background-work'), null, 'the new list exists only in Technical details');
    ok(host.querySelector('.det-line.need'));
    ok(fixture.provenance.monitorTiming.includes('unverified'), 'the synthetic Monitor result is not presented as a recording');
  } finally { dispose(); host.remove(); }
});

check('background: shared wire fixture keeps launch acknowledgements open and terminal facts in history', () => {
  const m = reply();
  const initial = backgroundOf([m], [command(), { ...command(), id: 'other', conversationId: 'other' }], 'background-live');
  equal(initial.length, 4);
  ok(initial.every((row) => row.state === 'running'));
  equal(initial.find((row) => row.command)!.title, 'Check the preview');
  equal(m.tools!.find((tool) => tool.id === 'helper')!.ok, undefined, 'the fixture’s launch result is not completion');
  for (const notification of fixture.finish) {
    if (notification.subtype !== 'task_notification' || notification.tool_use_id === 'shell') continue;
    const tool = m.tools!.find((tool) => tool.id === notification.tool_use_id)!;
    tool.ok = notification.status === 'completed';
    tool.output = notification.summary;
  }
  const helper = backgroundOf([m], [{ ...command(), state: 'failed' }], 'background-live');
  equal(helper.find((row) => row.agent)!.state, 'passed');
  equal(helper.find((row) => row.command)!.state, 'failed');
  m.status = 'done';
  const after = backgroundOf([m], [], 'background-live');
  equal(after.some((row) => row.agent), false, 'completed helpers move to their existing reports when the reply ends');
  equal(after.find((row) => row.tool?.name === 'Monitor')!.state, 'unreported', 'no invented live state after the reply');
  m.tools!.find((tool) => tool.id === 'helper')!.ok = undefined;
  const stop: Message = { id: 'stop', role: 'assistant', at: 2, text: '', status: 'stopped' };
  equal(backgroundOf([m, stop], [], 'background-live').some((row) => row.agent), false, 'Stop ends earlier helpers as helpersAtWork does');
  equal(backgroundOf([{ ...m, status: 'stopped' }], [], 'background-live').some((row) => row.agent || row.tool), false, 'the interrupted reply ends its unresolved calls');
  const older: Message = { ...m, status: 'done', follow: undefined, tools: [{ id: 'monitor', name: 'Monitor', input: {} }, { id: 'shell', name: 'Bash', input: { command: 'long task', run_in_background: true } }] };
  equal(backgroundOf([older], [], 'background-live').length, 2, 'before a Stop they wait for a report');
  equal(backgroundOf([older, stop], [], 'background-live').length, 0, 'review 3: the last Stop ends every earlier background call, not only helpers');
  const checks: Message = { ...reply(), tools: [{ id: 'verify', name: 'Verify', input: { level: 'full' } }], checks: { state: 'running', passed: 0 } };
  equal(backgroundOf([checks], [], 'background-live').filter((row) => row.id === 'reply:verify' || row.id === 'reply:checks').length, 2, 'Verify and the automatic checks reuse their separate parsed facts');
  checks.tools![0]!.ok = false;
  checks.checks = { state: 'needsYou', passed: 0 };
  const results = backgroundOf([checks], [], 'background-live');
  equal(results.find((row) => row.id === 'reply:verify')!.state, 'failed');
  equal(results.find((row) => row.id === 'reply:checks')!.state, 'waiting');
  equal(workStatus({ follow: { ...fixture.follow, state: 'stopped', problem: 'follow.gh' } }).state, 'stopped', 'a stopped follow never counts as running or waiting');
});

check('background: only unique followed numbers and known hash prefixes get live state', () => {
  const a: Follow = { ...fixture.follow, state: 'passing', head: 'abcdef1234567890abcdef1234567890abcdef1234' };
  const text = '#93 #999 abcdef1234567 deadbee acme/app#93 other/app#93 https://github.com/acme/app/pull/93 https://elsewhere.test/#93';
  const parts = liveReferences(text, [a]);
  ok(hasReference(a.url, [a]), 'a bare PR URL uses the same discovery as its rendered reference');
  equal(parts.map((part) => part.text).join(''), text, 'text and unknown references are preserved exactly');
  deepEqual(parts.filter((part) => part.follow).map((part) => part.text), ['#93', 'abcdef1234567', 'acme/app#93', a.url]);
  const b: Follow = { ...a, repo: 'other/app', url: 'https://github.com/other/app/pull/93' };
  equal(liveReferences('#93 abcdef1234567', [a, b]).some((part) => part.follow), false, 'ambiguous numbers and hashes are literal');
  const old = { ...reply(), id: 'old', follow: { ...a, state: 'pending' as const } };
  deepEqual(followedIn([old, { ...reply(), follow: a }]), [a], 'newest observed state wins for every reference');
  for (const state of ['passing', 'merged', 'failing', 'closed', 'stopped'] as const) {
    equal(liveReferences('#93', [{ ...a, state }])[0]!.follow!.state, state);
  }
});

check('background: the technical pane updates from shared events, preserves focus and releases output on close', async () => {
  useLanguage('en');
  const [messages, setMessages] = createSignal<Message[]>([reply()]);
  const [commands, setCommands] = createSignal<BackgroundCommand[]>([{ ...command(), output: 'Ready' }]);
  const [visible, setVisible] = createSignal(true);
  const original = { openLink: api.openLink, stopBackground: api.stopBackground };
  const links: string[] = [], stops: string[] = [];
  api.openLink = async (url) => { links.push(url); };
  api.stopBackground = async (id) => { stops.push(id); };
  const host = document.createElement('div');
  host.style.width = '300px';
  document.body.append(host);
  const dispose = render(() => [
    createComponent(DetailsLine, { conversationId: 'background-live', get messages() { return messages(); }, get background() { return commands(); } }),
    createComponent(Show, { keyed: true, get when() { return visible(); }, get children() { return createComponent(DetailsPane, {
      conversationId: 'background-live', get messages() { return messages(); }, get background() { return commands(); },
      changes: { id: 'background-live', files: [], loading: false }, onBack: () => setVisible(false), onClose: () => setVisible(false),
    }); } }),
  ], host);
  try {
    await settle();
    equal(host.querySelectorAll('.det-line').length, 1);
    ok(host.querySelector('.det-line')!.textContent!.includes(t('background.count', { n: 4 })));
    equal(host.querySelectorAll('.background-work > li').length, 4);
    equal(host.querySelector('.command-output'), null, 'closed command disclosures hold no output');
    const shellRow = host.querySelector<HTMLElement>('[data-work="command:opaque-shell"]')!;
    const stop = shellRow.querySelector<HTMLButtonElement>('.command-stop')!;
    ok(stop && !stop.closest('details'), 'Stop is available on the row while Output is closed');
    equal(shellRow.querySelector('.command-row'), null, 'no second command row is mounted');
    equal(shellRow.querySelectorAll('.work-state').length, 1, 'one state word per command');
    const shell = shellRow.querySelector<HTMLDetailsElement>('details')!;
    shell.open = true; shell.dispatchEvent(new Event('toggle'));
    await settle();
    stop.focus();
    setCommands([{ ...command(), output: '<script>literal output</script>' }]);
    await settle();
    equal(document.activeElement, stop);
    equal(host.querySelector('.command-output script'), null);
    stop.click(); await settle();
    deepEqual(stops, ['opaque-shell']);
    host.querySelector<HTMLButtonElement>('[data-work="reply:helper"] button')!.click();
    equal(openedAgent()?.callId, 'helper', 'the chip opens the existing helper view');
    showAgent(undefined);
    const refs = host.querySelector<HTMLDetailsElement>('.reply-references details')!;
    refs.open = true; refs.dispatchEvent(new Event('toggle')); await settle();
    equal(refs.querySelectorAll('.live-reference').length, 1, 'the unobserved hash is plain text');
    const next = structuredClone(messages()[0]!);
    next.follow = { ...next.follow!, state: 'merged', head: 'abcdef1234567890abcdef1234567890abcdef1234' };
    setMessages([next]); await settle();
    equal(refs.querySelectorAll('.live-reference').length, 2);
    setCommands([{ ...command(), output: '#93 abcdef1234567 #999' }]); await settle();
    equal(shell.querySelectorAll('.command-output .live-reference').length, 2, 'the existing command output uses the same live references');
    equal(shell.querySelector('.command-output')!.textContent!.includes('#999'), true);
    const link = refs.querySelector<HTMLAnchorElement>('.live-reference')!;
    link.focus();
    setMessages([{ ...next, follow: { ...next.follow!, state: 'passing' } }]); await settle();
    equal(document.activeElement, link, 'a follow update retains the focused reference');
    ok(link.textContent!.includes(t('background.link.passing')));
    link.click(); await settle();
    deepEqual(links, [fixture.follow.url]);
    ok(refs.textContent!.includes('#999'));
    equal([...refs.querySelectorAll('a')].some((el) => el.textContent!.includes('#999')), false);
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language); await settle();
      ok(host.scrollWidth <= host.clientWidth + 1, `${language}: chips wrap in a narrow pane`);
    }
    console.info(`Background work: ${host.querySelectorAll('.background-work > li').length} rows; ${host.querySelectorAll('.background-work *').length} elements; no added timer or process.`);
    setVisible(false); await settle();
    equal(host.querySelector('.background-work'), null);
    equal(host.querySelector('.command-output'), null);
  } finally {
    dispose(); host.remove(); Object.assign(api, original); showDetails(undefined); showAgent(undefined); useLanguage('en');
  }
});

check('background review 2: chip words are each language’s existing words, and one shell reads the same in both lists', async () => {
  const stem = (word: string) => word.toLocaleLowerCase().slice(0, 4);
  try {
    for (const language of LANGUAGES) {
      useLanguage(language.code);
      for (const state of ['merged', 'closed'] as const) {
        ok(t(`follow.${state}`, { pr: '#93' }).toLocaleLowerCase().includes(stem(t(`background.link.${state}`))), `${language.code}: ${state} keeps the word of the follow line`);
      }
      equal(t('background.stopped'), t('agent.stopped'), `${language.code}: one word for stopped`);
    }
    useLanguage('es');
    ok(['merged', 'closed'].every((state) => t(`background.link.${state}` as 'background.link.merged').endsWith('a')), 'es: agrees with "la pull request"');
    useLanguage('fr');
    ok(['merged', 'closed'].every((state) => t(`background.link.${state}` as 'background.link.merged').endsWith('e')), 'fr: agrees with "la pull request"');
  } finally { useLanguage('en'); }
  const [state, setState] = createSignal<BackgroundCommand['state']>('running');
  const view = pane(() => [{ ...reply(), tools: [], follow: undefined }], () => [{ ...command(), state: state() }]);
  try {
    for (const next of ['running', 'done', 'stopped', 'failed'] as const) {
      setState(next); await settle();
      equal(view.host.querySelector('[data-work="command:opaque-shell"] .work-state')!.textContent, t(`commands.${next}`), `${next}: the Commands list's word`);
    }
  } finally { view.close(); }
});

check('background review 5: a link closing a sentence keeps its chip, and an unrelated URL opens no reference group', async () => {
  const a: Follow = { ...fixture.follow, state: 'passing' };
  for (const end of ['.', ',', ';', ':', '!', '?']) {
    const parts = liveReferences(`Opened ${a.url}${end}`, [a]);
    deepEqual(parts.map((part) => part.text), ['Opened ', a.url, end]);
    ok(parts[1]!.follow, `a link followed by "${end}" still gets its chip`);
    ok(hasReference(`Opened ${a.url}${end}`, [a]));
  }
  const docs = 'Docs: https://tokio.rs/tokio/tutorial';
  equal(hasReference(docs, [a]), false, 'a URL that names no followed pull request is no reference');
  ok(hasReference('See https://github.com/acme/app/commit/abcdef1234567.', [{ ...a, head: 'abcdef1234567890' }]), 'a followed commit page is one');
  const view = pane(() => [{ ...reply(), text: docs, tools: [], follow: undefined }]);
  try {
    equal(view.host.querySelector('.reply-references'), null);
  } finally { view.close(); }
});
