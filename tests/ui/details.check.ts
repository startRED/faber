import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Conversation, type FileChange, type Message } from '../../ui/api.ts';
import { DetailsLine, detailsOf, openedDetails, showDetails } from '../../ui/Details.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { showAside } from '../../ui/QuickQuestion.tsx';
import { store } from '../../ui/running.ts';
import { t } from '../../ui/i18n.ts';
import recorded from '../fixtures/conversation-changes.json';
import { check, deepEqual, equal, ok } from './check.ts';

const messages: Message[] = [{ id: 'a1', role: 'assistant', text: 'The greeting is fixed and saved.', at: 1, status: 'done', commits: ['save'], tools: [
  { id: 'save', name: 'Bash', input: { command: 'git commit -m "Fix greeting"' }, ok: true, output: '[work abc1234] Fix greeting' },
  { id: 'check', name: 'Bash', input: { command: 'npm run lint' }, check: true, ok: false, output: 'A check failed.' },
  { id: 'review', name: 'Helper', input: { title: 'Review the greeting', brief: 'Check the changes without editing.' }, ok: true, output: 'No findings.' },
] }];
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('details: parsed facts are newest first, refused work and prose are not saved changes', () => {
  const newer: Message = { ...messages[0]!, id: 'a2', commits: ['save'], tools: [
    { id: 'save', name: 'commandExecution', input: 'git commit -m "Document greeting"', ok: true },
    { id: 'refused', name: 'Bash', input: { command: 'git commit -m refused' }, declined: true },
    { id: 'read', name: 'Read', input: { file_path: 'git commit instructions.md' }, ok: true },
    { id: 'echo', name: 'Bash', input: { command: 'echo "git commit -m example"' }, ok: true },
  ] };
  const facts = detailsOf([...messages, newer]);
  deepEqual(facts.saved.map((tool) => tool.id), ['a2:save', 'a1:save']);
  deepEqual(facts.checks.map((tool) => tool.ok), [false]);
  deepEqual(facts.reviews.map((step) => step.report), ['No findings.']);
  deepEqual(detailsOf([]).saved, []);
});

check('details: only the interrupted reply stops its unresolved tools; older results stay unknown', () => {
  const tools: Message['tools'] = [
    { id: 'check', name: 'Bash', input: { command: 'cargo test' }, check: true },
    { id: 'review', name: 'Agent', input: { description: 'Review the greeting', run_in_background: true } },
  ];
  const reply = (status: Message['status']): Message => ({ ...messages[0]!, status, tools });
  equal(detailsOf([reply('streaming')]).checks[0]!.detailState, 'running');
  equal(detailsOf([reply('stopped')]).checks[0]!.detailState, 'stopped');
  equal(detailsOf([reply('error')]).checks[0]!.detailState, 'stopped');
  equal(detailsOf([reply('done')]).checks[0]!.detailState, 'unknown');
  equal(detailsOf([reply('done')]).reviews[0]!.call.detailState, 'background');
  equal(detailsOf([reply('stopped')]).reviews[0]!.call.detailState, 'stopped');
  const laterStop = detailsOf([reply('done'), { ...reply('stopped'), id: 'later-stop' }]);
  equal(laterStop.reviews.at(-1)!.call.detailState, 'background');
  equal(laterStop.checks.at(-1)!.detailState, 'unknown');
});

check('details: saved changes use only engine commit receipts, never quoted examples or follow commands', () => {
  const commands = ['git commit -m fix', 'git -C "a folder" push', 'gh pr create', 'gh pr ready 2', 'gh pr reopen 2',
    'node scripts/land.mjs 2', 'gh pr merge 2', 'git merge feature', 'echo "example && git commit -m demo"'];
  const reply = { ...messages[0]!, commits: ['0'], tools: commands.map((command, i) => ({ id: String(i), name: 'Bash', input: { command }, ok: true })) };
  deepEqual(detailsOf([reply]).saved.map((tool) => tool.id), ['a1:0']);
  deepEqual(detailsOf([{ ...reply, commits: [] }]).saved, [], 'the screen never parses command text into receipts');
});

check('details: reviews include Work log agent reports in every language', () => {
  const titles = ['Revisar as alterações', 'Revisar los cambios', 'Vérifier les modifications', 'Änderungen prüfen'];
  const reports = ['Sem problemas.', 'Sin problemas.', 'Aucun problème.', 'Keine Probleme.'];
  const facts = detailsOf([{ ...messages[0]!, tools: titles.map((description, i) => ({
    id: String(i), name: 'Agent', input: { description, subagent_type: 'general-purpose' }, ok: true, output: reports[i],
  })) }]);
  deepEqual(facts.reviews.map((step) => step.report), reports.toReversed());
});

check('details: text-only updates do not reclassify a conversation history', async () => {
  let reads = 0;
  const tools = Array.from({ length: 100 }, (_, i) => ({ id: String(i), get name() { reads++; return 'Read'; }, input: {}, ok: true }));
  const [list, setList] = createSignal<Message[]>([{ ...messages[0]!, tools }]);
  const cache = new Map();
  detailsOf(list(), cache);
  const classified = reads;
  detailsOf([{ ...list()[0]!, text: 'More words.' }], cache);
  equal(reads, classified, 'the details pane reuses each message’s classification');
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(DetailsLine, { conversationId: 'cached', get messages() { return list(); } }), host);
  try {
    const initialReads = reads;
    setList([{ ...list()[0]!, text: 'More words again.' }]);
    await settle();
    equal(reads, initialReads, 'the thread line does not scan old tools on each token');
    setList([{ ...list()[0]!, commits: ['saved'], tools: [...tools, { id: 'saved', name: 'Bash', input: { command: 'git commit -m fix' }, ok: true }] }]);
    await settle();
    ok(host.textContent!.includes(t('details.saved', { n: 1 })), 'engine receipts refresh the saved count');
  } finally { dispose(); host.remove(); }
});

function mountThread(messages: () => Message[], busy: () => boolean) {
  const host = document.createElement('div');
  host.dataset.motion = 'none';
  host.style.width = '720px';
  host.style.maxWidth = '100%';
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    area: 'project', conversationId: 'plain-problems', options: [], from: 0, onBranch: () => {},
    get messages() { return messages(); }, get busy() { return busy(); },
  }), host);
  return { host, close: () => { dispose(); host.remove(); } };
}

check('details: failures, pending work and decisions stay visible in plain words with the column closed', async () => {
  const [list, setList] = createSignal<Message[]>([]);
  const [busy, setBusy] = createSignal(false);
  const view = mountThread(list, busy);
  const follow = { repo: 'acme/app', number: 93, url: 'https://github.com/acme/app/pull/93', state: 'pending' as const, since: 1 };
  const cases: [Partial<Message>, Parameters<typeof t>[0]][] = [
    [{ checks: { state: 'failed', passed: 0 } }, 'details.checkFailed'],
    [{ checks: { state: 'needsYou', passed: 0 } }, 'run.needsYou'],
    [{ follow: { ...follow, state: 'failing' } }, 'details.changeFailed'],
    [{ follow: { ...follow, state: 'conflict' } }, 'details.changeFailed'],
    [{ needsYou: true, follow: { ...follow, held: true } }, 'run.needsYou'],
    [{ wrapUp: { recap: 'More work remains.', done: false } }, 'details.unfinished'],
    [{ status: 'error' }, 'details.unfinished'],
    [{ needsYou: true, wrapUp: { recap: 'Pick a colour.', ask: 'Which colour?', actions: ['Blue', 'Green'] } }, 'run.needsYou'],
  ];
  try {
    for (const [extra, key] of cases) {
      setList([{ ...messages[0]!, tools: [], ...extra }]);
      await settle();
      const line = view.host.querySelector<HTMLElement>('.det-line')!;
      ok(line.textContent!.includes(t(key)), key);
      ok(line.classList.contains('need'), 'problems use the amber state');
      equal(view.host.querySelector('#running'), null, 'the problem never depends on an open column');
      ok(!line.textContent!.includes('#93') && !line.textContent!.includes('cargo'), 'technical names stay out of the plain line');
      const button = line.querySelector<HTMLButtonElement>('button')!;
      button.focus(); equal(document.activeElement, button, 'details remains keyboard reachable');
    }
    setBusy(true);
    setList([{ ...messages[0]!, checks: { state: 'failed', passed: 0 }, status: 'streaming' }]);
    await settle();
    ok(view.host.querySelector('.det-line')!.textContent!.includes(t('details.fixing')));
    setList([{ ...list()[0]!, checks: { state: 'passed', passed: 1 }, status: 'done' }]);
    setBusy(false);
    await settle();
    ok(!view.host.querySelector('.det-line')!.classList.contains('need'), 'the authoritative passing result clears the failure');
  } finally { view.close(); }
});

check('details: only the latest reply’s authoritative needs-you survives unrelated successful tools', async () => {
  const failed: Message = { ...messages[0]!, needsYou: true, checks: { state: 'needsYou', passed: 0 } };
  const [list, setList] = createSignal<Message[]>([failed]);
  const view = mountThread(list, () => false);
  try {
    setList([{ ...failed, tools: [...failed.tools!, { id: 'format', name: 'Bash', input: { command: 'cargo fmt --check' }, check: true, ok: true }] }]);
    await settle();
    equal(view.host.querySelector('.det-line')!.getAttribute('data-state'), 'run.needsYou', 'an unrelated pass cannot clear Faber’s result');
    setList([failed, { id: 'question', role: 'user', at: 2, text: 'Leave it, explain what broke.' },
      { id: 'explanation', role: 'assistant', at: 3, text: 'The greeting could not be read.', status: 'done', needsYou: false }]);
    await settle();
    ok(!view.host.querySelector('.det-line')!.classList.contains('need'), 'an old needs-you cannot outlive the latest reply');
    equal(view.host.querySelector('.det-line')!.getAttribute('data-state'), 'details.saved');
    setList([{ ...list().at(-1)!, needsYou: true, tools: [
      { id: 'format', name: 'Bash', input: { command: 'cargo fmt --check' }, check: true, ok: true },
    ] }]);
    await settle();
    equal(view.host.querySelector('.det-line')!.getAttribute('data-state'), 'run.needsYou', 'engine attention needs no UI reconstruction of tool calls');
  } finally { view.close(); }
});

check('details: a failed then passing tool check and agent child failures do not invent a Faber failure', async () => {
  const [list, setList] = createSignal<Message[]>([{ ...messages[0]!, needsYou: false, tools: [
    { id: 'red', name: 'Bash', input: { command: 'cargo test' }, check: true, ok: false },
    { id: 'green', name: 'Bash', input: { command: 'cargo test' }, check: true, ok: true },
    { id: 'agent', name: 'Agent', input: { description: 'Check a detail' }, ok: true },
    { id: 'child', name: 'Bash', input: { command: 'cargo test' }, parent: 'agent', check: true, ok: false },
  ] }]);
  const view = mountThread(list, () => false);
  try {
    equal(view.host.querySelector('.det-line')!.getAttribute('data-state'), 'details.saved');
    ok(!view.host.querySelector('.det-line')!.classList.contains('need'));
    setList([{ ...list()[0]!, checks: { state: 'needsYou', passed: 0 }, needsYou: true }]);
    await settle();
    equal(view.host.querySelector('.det-line')!.getAttribute('data-state'), 'run.needsYou', 'Faber alone decides whether the reply needs a fix');
  } finally { view.close(); }
});

check('details: the person’s Stop leaves a neutral unfinished line; an error or unfinished wrap-up is amber', async () => {
  const [list, setList] = createSignal<Message[]>([{ ...messages[0]!, status: 'stopped', needsYou: false }]);
  const view = mountThread(list, () => false);
  try {
    const line = () => view.host.querySelector('.det-line')!;
    equal(line().getAttribute('data-state'), 'details.unfinished');
    ok(!line().classList.contains('need'), 'Stop is the person’s choice');
    for (const extra of [{ status: 'error' as const }, { wrapUp: { recap: 'Still working.', done: false } }]) {
      setList([{ ...messages[0]!, ...extra }]);
      await settle();
      equal(line().getAttribute('data-state'), 'details.unfinished');
      ok(line().classList.contains('need'), 'an actual problem stays amber');
    }
  } finally { view.close(); }
});

check('details: project checks announce each result politely in plain words', async () => {
  const [list, setList] = createSignal<Message[]>([{ ...messages[0]!, status: 'streaming', text: 'Checking it.', tools: [
    { id: 'first', name: 'Bash', input: { command: 'cargo test --test private-name' }, check: true },
  ] }]);
  const view = mountThread(list, () => true);
  try {
    const region = view.host.querySelector('.tally-live')!;
    ok(region, 'a project keeps the polite live region');
    equal(region.getAttribute('aria-live'), 'polite');
    setList([{ ...list()[0]!, tools: [{ ...list()[0]!.tools![0]!, ok: true }] }]);
    await settle();
    ok(region.textContent!.includes(t('details.checked', { n: 1 })));
    setList([{ ...list()[0]!, tools: [...list()[0]!.tools!, { id: 'second', name: 'Bash', input: { command: 'npm run private-lint' }, check: true, ok: false }] }]);
    await settle();
    ok(region.textContent!.includes(t('details.checked', { n: 2 })));
    ok(region.textContent!.includes(t('details.checksFailed', { n: 1 })));
    ok(!region.textContent!.includes('private-'), 'the announcement never reads command names');
    const tally = view.host.querySelector<HTMLElement>('.tally.project.bad')!;
    const style = getComputedStyle(tally);
    const probe = document.createElement('span');
    probe.style.color = 'var(--amber-ink)'; tally.append(probe);
    const amber = getComputedStyle(probe).color;
    probe.remove();
    equal(style.color, amber, 'failed project checks use amber text');
    ok(style.boxShadow.includes(amber), 'the marker uses the same amber as the text');
  } finally { view.close(); }
});

check('details: the project thread opens the real column, sharing changes, checks and follow controls', async () => {
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, changes: api.changes, changePatch: api.changePatch };
  const storage = localStorage.getItem('faber.place');
  const c = structuredClone(recorded.conversation) as Conversation;
  c.messages = structuredClone(messages);
  c.messages[0]!.checks = { state: 'failed', passed: 0, failed: [{ command: 'cargo test', lines: [] }] };
  c.messages[0]!.follow = { repo: 'acme/app', number: 93, url: 'https://github.com/acme/app/pull/93', state: 'pending', since: 1 };
  c.copy = { branch: 'faber/fix-the-greeting-ab12', path: 'E:/faber-data/copies/ab12' };
  const initial = {
    conversations: [{ id: c.id, title: c.title, area: 'project', projectId: 'p1', updatedAt: c.updatedAt }],
    projects: [{ id: 'p1', name: 'Greeting', path: 'E:/greeting' }], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: store.list().revision + 1, items: [] },
    quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  } as AppState;
  let reads = 0;
  api.open = async () => c; api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.changes = async () => { reads++; return { files: recorded.changes as FileChange[], incomplete: false }; };
  api.changePatch = async () => ({ text: '@@ -1 +1 @@\n-old\n+new\n', tooLarge: false, binary: false });
  localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'p1', id: c.id }));
  store.offer(initial.running);
  const host = document.createElement('div');
  document.body.append(host);
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    equal(host.querySelectorAll('.det-line').length, 1, 'one quiet entry for the whole conversation');
    ok(host.querySelector('.det-line')!.textContent!.includes(t('details.checkFailed')), 'a saved change never masks a failed check');
    equal(host.querySelector('.ai .follow'), null, 'technical follow status is in the pane');
    equal(host.querySelector('.ai .checks'), null, 'technical check results are in the pane');
    host.querySelector<HTMLButtonElement>('.det-line button')!.click();
    await settle();
    equal(openedDetails(), c.id);
    equal(host.querySelector('#running-title')!.textContent, t('details.title'));
    equal(host.querySelector('.crumb')!.textContent!.includes('faber/'), false, 'the topbar keeps the title only');
    const copy = host.querySelector('[data-testid=details-copy]')!;
    equal(copy.firstChild!.textContent, t('details.copy', { branch: 'faber/fix-the-greeting-ab12' }), 'the separate copy is named in the details');
    equal(copy.querySelector('span')!.textContent, 'E:/faber-data/copies/ab12', 'with its folder');
    ok(host.querySelector('.details-view')!.textContent!.includes('git commit -m "Fix greeting"'));
    ok(host.querySelector('.details-view')!.textContent!.includes('npm run lint'));
    ok(host.querySelector('.det-facts li[data-state="failed"]'));
    equal(host.querySelector('.checks-failed p')!.textContent, t('checks.noDetails'));
    ok(host.querySelector('.details-view .follow button'), 'the existing stop-following control is retained');
    ok(host.querySelector('.details-view .change-file'), 'the existing file disclosures are used');
    const review = host.querySelector<HTMLDetailsElement>('.det-g:last-child .det-facts .disclosure')!;
    ok(review, 'the review report is available without a second chat');
    ok(host.querySelector('.det-g:last-child')!.textContent!.includes('Review the greeting'));
    equal(review.querySelector('pre'), null, 'closed reports hold no output elements');
    review.open = true; review.dispatchEvent(new Event('toggle'));
    await settle();
    ok(review.textContent!.includes('No findings.'));
    equal(reads, 1, 'opening details reuses the existing file list');
    const file = host.querySelector<HTMLDetailsElement>('.details-view .change-file')!;
    file.open = true; file.dispatchEvent(new Event('toggle'));
    await settle();
    ok(host.querySelector('.change-patch'), 'the shared diff opens inside the technical pane');
    const checks = host.querySelector<HTMLDetailsElement>('.details-view .checks .disclosure')!;
    checks.open = true;
    const next = structuredClone(c.messages[0]!);
    next.text += ' Everything is ready.';
    hear({ type: 'message', conversationId: c.id, message: next });
    await settle();
    ok(host.querySelector<HTMLDetailsElement>('.details-view .checks .disclosure')!.open, 'a text update keeps open evidence and focus');
    showAside({ conversationId: c.id });
    await settle();
    equal(host.querySelector('.details-view'), null, 'side views exclude each other');
    equal(host.querySelector('.change-patch'), null, 'leaving releases the patch');
    showDetails(c.id);
    await settle();
    host.querySelector<HTMLButtonElement>('#running .back')!.click();
    await settle();
    equal(openedDetails(), undefined);
    equal(host.querySelector('.details-view'), null);
    equal(host.querySelector('#running-title')!.textContent, t('run.title'));
  } finally {
    showDetails(undefined); showAside(undefined);
    dispose(); host.remove(); Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
    store.offer({ revision: store.list().revision + 1, items: [] });
  }
});
