import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import { ToolGlyph } from '../../ui/icons/kinds.tsx';
import { keepParts, timeline } from '../../ui/timeline.ts';
import type { Message, ToolUse } from '../../ui/api.ts';
import { applyLook } from '../../ui/appearance.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { changeOf, groupLabel, groupSteps, sourcesOf, stepLine, stepsSummary, toolLine, toolRow } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';
import { sala } from './appearance.check.ts';

check('worklog: file paths use the conversation folder and keep the full detail', () => {
  useLanguage('en');
  const folder = 'C:\\Projects\\app';
  const read = (path: string): ToolUse => ({ id: 'path', name: 'Read', input: { file_path: path }, ok: true });
  equal(toolRow(read('c:\\Projects\\app\\src\\b.txt'), true, folder).detail, 'src/b.txt');
  equal(toolRow(read('C:/Projects/application/b.txt'), true, folder).detail, 'application/b.txt');
  equal(toolRow(read('/tmp/proj-cwd/b.txt'), true, folder).detail, 'proj-cwd/b.txt');
  equal(toolRow(read('\\\\server\\share\\app\\src\\lib\\a.ts'), true, '\\\\SERVER\\Share\\app').detail, 'src/lib/a.ts', 'a network share ignores letter case');
  equal(toolRow(read('/Home/app/a.ts'), true, '/home/app').detail, 'app/a.ts', 'elsewhere letter case counts');
  equal(toolRow({ id: 'n', name: 'NotebookEdit', input: { notebook_path: 'C:\\Projects\\app\\book.ipynb' } }, false, folder).detail, 'book.ipynb');
  equal(toolRow({ id: 'f', name: 'fileChange', input: [{ path: 'C:\\Projects\\app\\a.txt' }, { path: 'D:\\other\\b.txt' }] }, false, folder).detail, 'a.txt, other/b.txt');
  equal(toolRow({ id: 'f', name: 'fileChange', input: { 'C:\\Projects\\app\\a.txt': {} } }, false, folder).detail, 'a.txt');
  equal(toolLine({ id: 'c', name: 'Bash', input: { command: 'cat C:\\Projects\\app\\a.txt' } }, folder), 'Running · cat C:\\Projects\\app\\a.txt');
  const root = document.createElement('div'); document.body.append(root);
  const path = 'c:\\Projects\\app\\src\\b.txt';
  const dispose = render(() => createComponent(Thread, { messages: [{ id: 'paths', role: 'assistant', at: 0, status: 'done', text: '', tools: [read(path)] }], workingFolder: folder, from: 0, conversationId: 'paths', options: [], onBranch: () => {} }), root);
  try {
    const button = root.querySelector<HTMLButtonElement>('.step-line')!;
    equal(button.querySelector('i')!.textContent, 'src/b.txt');
    equal(button.title, path);
    const icon = document.createElement('div');
    const disposeIcon = render(() => createComponent(ToolGlyph, { kind: 'read', small: true }), icon);
    equal(button.querySelector('.step-icon svg')?.innerHTML, icon.querySelector('svg')?.innerHTML, 'a read uses the shared read drawing');
    disposeIcon();
    button.click();
    equal(root.querySelector('.step-what')!.textContent, path);
  } finally { dispose(); root.remove(); }
});

check('worklog: every Claude Code tool reads as what it does, in words', () => {
  equal(toolRow({ id: '1', name: 'Read', input: { file_path: 'C:\\p\\a.txt' } }).label, t('tool.read'));
  equal(toolRow({ id: '2', name: 'Bash', input: { command: 'npm test', description: 'Run the tests' } }).detail, 'npm test', 'a command shows itself');
  deepEqual(toolRow({ id: '3', name: 'WebFetch', input: { url: 'https://akitaonrails.com/en/' } }), { label: t('tool.read'), detail: 'akitaonrails.com/en/' });
  deepEqual(toolRow({ id: '4', name: 'mcp__github__get_issue', input: {} }), { label: t('tool.mcp'), detail: 'github' });
  // A helper started by a helper shows as a step inside the first one's row (R161), named by the words it was given.
  deepEqual(toolRow({ id: '5', name: 'Agent', input: { description: 'reader-a', prompt: 'Read a.txt' }, parent: 'x' }), { label: t('tool.agent'), detail: 'reader-a' });
  equal(toolRow({ id: '6', name: 'SomeNewTool', input: {} }).label, 'SomeNewTool', 'a tool Faber does not know keeps its name');
  // A conversation the agent starts by itself reads as its title, and one it reads back as its id.
  deepEqual(toolRow({ id: '7', name: 'Conversation', input: { title: 'Docs', brief: 'Write the page.' } }), { label: t('tool.conversation'), detail: 'Docs' });
  equal(toolRow({ id: '8', name: 'Conversation', input: { id: 'c1' } }).detail, 'c1');
  deepEqual(toolRow({ id: '9', name: 'Conversation', input: { action: 'compact', next: 'Go on with the docs.' } }), { label: t('tool.compact'), detail: '' });
  deepEqual(toolRow({ id: '10', name: 'Conversation', input: { action: 'clear', next: 'Docs next.' } }), { label: t('tool.clear'), detail: '' });
  deepEqual(toolRow({ id: '10', name: 'Conversation', input: { action: 'context' } }), { label: t('tool.context'), detail: '' });
  // Faber's checks (R190) read as the level they run, with nothing after.
  deepEqual(toolRow({ id: '9', name: 'Verify', input: {} }), { label: t('tool.verify'), detail: '' });
  deepEqual(toolRow({ id: '10', name: 'Verify', input: { level: 'full' } }), { label: t('tool.verifyFull'), detail: '' });
  deepEqual(toolRow({ id: '11', name: 'Map', input: {} }), { label: t('tool.map'), detail: '' });
  deepEqual(toolRow({ id: '12', name: 'Map', input: { kind: 'setting', query: 'theme' } }), { label: t('tool.map'), detail: 'setting theme' });
  // The limits and the catalog search read as what they do; the search shows its words, in every language of the screen.
  deepEqual(toolRow({ id: 'creative', name: 'CreativeJudge', input: { first: 'Private draft', second: 'Other draft' } }), { label: t('tool.creativeJudge'), detail: '' });
  deepEqual(toolRow({ id: '13', name: 'Quota', input: {} }), { label: t('tool.quota'), detail: '' });
  deepEqual(toolRow({ id: '14', name: 'Catalog', input: { query: 'fix a bug' } }), { label: t('tool.catalog'), detail: 'fix a bug' });
  deepEqual(toolRow({ id: '15', name: 'Catalog', input: {} }), { label: t('tool.catalog'), detail: '' });
  // The project's memory search reads as that, with its words, in every language.
  deepEqual(toolRow({ id: '17', name: 'MemorySearch', input: { query: 'database choice' } }), { label: t('tool.memorySearch'), detail: 'database choice' });
  deepEqual(toolRow({ id: '18', name: 'MemorySearch', input: {} }), { label: t('tool.memorySearch'), detail: '' });
  // The larger-version offer is a question to the person: the same row as any other question, with no new words.
  deepEqual(toolRow({ id: '19', name: 'OfferBigger', input: { question: 'Keep it free of crashes?', asked: 'Fix this', bigger: 'No crashes' } }), { label: t('tool.ask'), detail: '' });
  // An install by id reads as installing from the catalog, with the id.
  deepEqual(toolRow({ id: '16', name: 'Catalog', input: { install: 'ponytail' } }), { label: t('tool.catalogInstall'), detail: 'ponytail' });
  for (const lang of ['pt-BR', 'es', 'fr', 'de'] as const) {
    useLanguage(lang);
    ok(t('tool.creativeJudge') !== 'Comparing drafts independently', `${lang} names independent draft comparisons in its own words`);
    deepEqual(toolRow({ id: 'creative', name: 'CreativeJudge', input: { first: 'Private draft' } }), { label: t('tool.creativeJudge'), detail: '' });
    ok(t('tool.quota') !== 'Checking the AI limits' && t('tool.catalog') !== 'Searching the catalog', `${lang} has its own words`);
    ok(t('tool.catalogInstall') !== 'Installing from the catalog', `${lang} names the install in its own words`);
    ok(t('tool.memorySearch') !== "Searching the project's memory", `${lang} names the memory search in its own words`);
  }
  useLanguage('en');
});

// R161 step 9: while a reply runs, the checks it ran tick up under the status line, marked by the engine, never guessed here.
check('worklog: the checks a running reply ran tick up, with how many failed', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const run = (id: string, ok?: boolean) => ({ id, name: 'Bash', input: { command: 'cargo test' }, check: true, ok });
  const [message, setMessage] = createSignal<Message>({
    id: 'tally', role: 'assistant', at: 0, status: 'streaming', text: '',
    tools: [{ id: 'i', name: 'Bash', input: { command: 'npm install' }, ok: true }, run('a')],
  });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'tally', options: [], onBranch: () => {} }), root);
  try {
    const tally = () => root.querySelector('.tally');
    equal(tally(), null, 'a check still running has not ended, and a command that is no check never counts');
    setMessage({ ...message(), tools: [message().tools![0]!, run('a', true)] });
    equal(tally()!.textContent, 'Ran 1 check · none failed');
    ok(tally()!.querySelector('svg'), 'none failed reads with a check mark');
    setMessage({ ...message(), tools: [...message().tools!, run('b', false)] });
    equal(tally()!.textContent, 'Ran 2 checks · 1 failed');
    ok(tally()!.classList.contains('bad') && !tally()!.querySelector('svg'), 'a failed check takes the error edge');
    setMessage({ ...message(), status: 'done' });
    equal(tally(), null, 'the tally is for a reply at work');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 9: a file change reads as the lines it changed, from the step's own input: the lines both sides share are not
// a change, a new file is all added, and Codex's diff keeps its own lines without its headers.
check('worklog: an edit step counts the lines it added and removed', () => {
  const edit = changeOf({ id: 'e', name: 'Edit', input: { file_path: 'a.rs', old_string: 'fn a() {\n  1\n}', new_string: 'fn a() {\n  2\n  3\n}' } });
  deepEqual(edit, { added: 2, removed: 1, lines: [{ added: false, text: '  1' }, { added: true, text: '  2' }, { added: true, text: '  3' }] });
  equal(changeOf({ id: 'w', name: 'Write', input: { file_path: 'b.md', content: '# B\n\nText\n' } })?.added, 3, 'a written file is all added');
  equal(changeOf({ id: 'm', name: 'MultiEdit', input: { edits: [{ old_string: 'a', new_string: 'b' }, { old_string: 'c\nd', new_string: 'c' }] } })?.removed, 2);
  deepEqual(changeOf({ id: 'c', name: 'fileChange', input: [{ path: 'n.txt', diff: '--- a/n.txt\n+++ b/n.txt\n@@ -1 +1 @@\n-baseline\n+escaped' }] })?.lines, [{ added: false, text: 'baseline' }, { added: true, text: 'escaped' }]);
  equal(changeOf({ id: 'k', name: 'fileChange', input: { 'n.txt': { update: { unified_diff: '@@ -1 +1 @@\n-a\n+b\n+c' } }, 'new.txt': { add: { content: 'x' } } } })?.added, 3, 'Codex keyed by path');
  deepEqual(changeOf({ id: 'i', name: 'Edit', input: { old_string: 'old A\nkeep\nold B', new_string: 'new A\nkeep\nnew B' } })?.lines.map((l) => l.text), ['old A', 'old B', 'new A', 'new B'], 'a line kept inside is not a change');
  equal(changeOf({ id: 'h', name: 'fileChange', input: [{ path: 'x', diff: '--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- old\n+++ new' }] })?.added, 1, 'inside a hunk, three signs are content');
  equal(changeOf({ id: 's', name: 'Edit', input: { old_string: 'same', new_string: 'same' } }), undefined, 'an edit that changed nothing has no change');
  equal(changeOf({ id: 'r', name: 'Read', input: { file_path: 'a.rs' } }), undefined);
});

check('worklog: an edit step reads what the engine kept, with no input left', () => {
  useLanguage('en');
  // The engine keeps the counts and only the first 40 lines (engine/src/change.rs), so a step may carry no input at all.
  const kept = { added: 50, removed: 3, lines: Array.from({ length: 40 }, (_, i) => ({ added: i >= 3, text: `line ${i}` })) };
  equal(changeOf({ id: 'k', name: 'Edit', input: {}, change: kept }), kept, 'the engine reading wins');
  const root = document.createElement('div');
  document.body.append(root);
  const message: Message = { id: 'edit-kept', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [{ id: 'e', name: 'Write', input: {}, change: kept }] };
  const dispose = render(() => createComponent(Thread, { messages: [message], from: 0, conversationId: 'edit-kept', options: [], onBranch: () => {} }), root);
  try {
    deepEqual([...root.querySelectorAll('.changed div')].map((d) => d.textContent), ['−line 0', '−line 1', '−line 2', '+line 3']);
    equal(root.querySelector('.changed p')!.textContent, '49 more lines', 'the rest counts from the totals, not the kept lines');
  } finally {
    dispose();
    root.remove();
  }
});

check('worklog: an edit in flight shows its lines under the status line, and its step counts them', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const body = Array.from({ length: 6 }, (_, i) => `line ${i}`).join('\n');
  const [message, setMessage] = createSignal<Message>({
    id: 'edit-view', role: 'assistant', at: 0, status: 'streaming', text: '',
    tools: [{ id: 'w', name: 'Write', input: { file_path: 'notes.md', content: body } }],
  });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'edit-view', options: [], onBranch: () => {} }), root);
  try {
    const shown = () => [...root.querySelectorAll('.changed div')].map((d) => d.textContent);
    deepEqual(shown(), ['+line 0', '+line 1', '+line 2', '+line 3'], 'the first four lines show');
    equal(root.querySelector('.changed p')!.textContent, '2 more lines');
    equal(root.querySelector('.changed')!.getAttribute('aria-hidden'), 'true', 'the status line already says it; the lines are for the eye');
    equal(root.querySelector('.step .delta')!.getAttribute('aria-label'), '6 lines added');
    setMessage({ ...message(), tools: [{ ...message().tools![0]!, ok: true }] });
    ok(root.querySelector('.changed'), 'a finished edit stays until the next step or the answer');
    const said = 'Writing the notes.\n\n';
    setMessage({ ...message(), text: said, textBreaks: [said.length], activityEnd: said.length });
    ok(root.querySelector('.changed'), 'words before the edit are not the answer');
    setMessage({ ...message(), text: said + 'Done.' });
    equal(root.querySelector('.changed'), null, 'the answer takes its place');
    setMessage({ ...message(), text: said, tools: [...message().tools!, { id: 'b', name: 'Bash', input: { command: 'npm test' } }] });
    equal(root.querySelector('.changed'), null, 'the next step takes its place');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 9: research reads as the sites it opened, from its own page reads: a site opened again moves to the end, and
// the one still loading is the one it reads now.
check('worklog: a reply that reads the web lists the sites it opened and the one it reads now', () => {
  const read = (id: string, url: string, ok?: boolean) => ({ id, name: 'WebFetch', input: { url }, ok });
  deepEqual(sourcesOf([read('a', 'https://www.docs.rs/tokio', true), { id: 's', name: 'WebSearch', input: { query: 'tokio' }, ok: true }, read('b', 'https://github.com/x', true), read('c', 'https://docs.rs/serde')]), { hosts: ['github.com', 'docs.rs'], now: 'docs.rs' }, 'a search names no site, and www is the same site');
  deepEqual(sourcesOf([read('a', 'not a url', true), { ...read('b', 'https://x.org'), declined: true }]), { hosts: [], now: undefined }, 'a page never read names nothing');
  deepEqual(sourcesOf([read('s', 'https://slow.dev'), ...['a', 'b', 'c', 'd'].map((h) => read(h, `https://${h}.dev`, true))]).hosts.at(-1), 'slow.dev', 'a slow page still loading stays in view');
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const reads = Array.from({ length: 5 }, (_, i) => read(`r${i}`, `https://site${i}.dev/page`, true));
  const [message, setMessage] = createSignal<Message>({ id: 'sources', role: 'assistant', at: 0, status: 'streaming', text: '', tools: reads.slice(0, 1) });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'sources', options: [], onBranch: () => {} }), root);
  try {
    const shown = () => [...root.querySelectorAll('.sources span')].map((s) => s.textContent);
    deepEqual(shown(), ['Opened 1 site', 'site0.dev']);
    setMessage({ ...message(), tools: [...reads.slice(0, 4), read('r5', 'https://site5.dev/now')] });
    deepEqual(shown(), ['Opened 5 sites', '+1', 'site1.dev', 'site2.dev', 'site3.dev', 'site5.dev'], 'at most four, the newest last');
    equal(root.querySelector('.sources .now')!.textContent, 'site5.dev', 'the page still loading is the one it reads now');
    equal(root.querySelector('.sources')!.getAttribute('aria-hidden'), 'true', 'the status line already says the page');
    setMessage({ ...message(), status: 'done' });
    equal(root.querySelector('.sources'), null, 'the list is for a reply at work');
  } finally {
    dispose();
    root.remove();
  }
});

// R161 step 9: long or hidden work reads as the living sentence and its time, from when the engine saw the step start.
check('worklog: a step that runs long says for how long, and a quick one never does', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const build = (ago: number, ok?: boolean, id = 'quick') => ({ id, name: 'Bash', input: { command: 'cargo build' }, started: Date.now() - ago, ok });
  const [message, setMessage] = createSignal<Message>({ id: 'long', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [build(5_000)] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'long', options: [], onBranch: () => {} }), root);
  try {
    const long = () => root.querySelector('.step .long');
    equal(long(), null, 'five seconds is no long step');
    setMessage({ ...message(), tools: [build(150_000, undefined, 'slow')] });
    equal(long()!.textContent, '· 2 min');
    equal(long()!.getAttribute('aria-hidden'), 'true', 'the live region says the step, not a clock');
    ok(root.querySelector('.step-line')!.textContent!.includes('cargo build'), 'the step keeps its words');
    setMessage({ ...message(), tools: [build(150_000, true, 'slow')] });
    equal(long(), null, 'an ended step has no running time');
    setMessage({ ...message(), tools: [{ id: 'old', name: 'Bash', input: { command: 'cargo build' } }] });
    equal(long(), null, 'a step recorded before the stamp existed shows none');
  } finally {
    dispose();
    root.remove();
  }
});

// Steps of one kind in a row read as one phrase with a count, opening into the single steps (Watching the agent work, step 1).
check('worklog: steps of one kind in a row group into one phrase, and a run that counts one thing stays as it is', () => {
  useLanguage('en');
  const edit = (id: string, file: string) => ({ id, name: 'Edit', input: { file_path: file, old_string: 'a', new_string: 'b' } });
  const read = (id: string, file: string) => ({ id, name: 'Read', input: { file_path: file } });
  const run = (id: string) => ({ id, name: 'Bash', input: { command: 'ls' } });
  const label = (tools: Parameters<typeof groupSteps>[0]) => groupSteps(tools).map((g) => (g.kind ? groupLabel(g) : g.tools[0]!.id));
  deepEqual(label([edit('a', 'a.rs'), edit('b', 'b.rs'), { id: 'w', name: 'Write', input: { file_path: 'c.rs', content: 'x' } }]), ['Changing 3 files'], 'a write joins the edits');
  deepEqual(label([edit('a', 'a.rs'), edit('b', 'a.rs'), edit('c', 'a.rs')]), ['a', 'b', 'c'], 'three edits to one file are not three files');
  deepEqual(label([edit('a', 'a.rs'), edit('b', 'a.rs'), edit('c', 'b.rs')]), ['Changing 2 files'], 'the steps stay three, the files are two');
  deepEqual(label([read('a', 'a.rs'), read('b', 'b.rs'), run('x'), read('c', 'c.rs')]), ['Reading 2 files', 'x', 'c'], 'another kind between ends the run');
  deepEqual(label([run('a'), run('b'), run('c')]), ['Running 3 commands'], 'commands count each run, even the same command');
  deepEqual(
    label([
      { id: 'p', name: 'WebFetch', input: { url: 'https://a.dev' } },
      { id: 'q', name: 'WebFetch', input: { url: 'https://b.dev' } },
      { id: 's', name: 'Grep', input: { pattern: 'x' } },
      { id: 't', name: 'Glob', input: { pattern: '*.rs' } },
      { id: 'u', name: 'WebSearch', input: { query: 'x' } },
      { id: 'v', name: 'WebSearch', input: { query: 'y' } },
    ]),
    ['Reading 2 pages', 'Searching 2 times', 'Searching the web 2 times'],
  );
  deepEqual(label([{ id: 'm1', name: 'mcp__github__get_issue', input: {} }, { id: 'm2', name: 'mcp__github__get_issue', input: {} }, { id: 'k1', name: 'Skill', input: { skill: 'x' } }]), ['m1', 'm2', 'k1'], 'a tool server or a skill is always its own step');
  // Codex names its changes in one step, as a list or keyed by path.
  deepEqual(label([{ id: 'f1', name: 'fileChange', input: [{ path: 'a.txt', diff: '' }, { path: 'b.txt', diff: '' }] }]), ['f1'], 'one step is no run');
  deepEqual(label([{ id: 'f1', name: 'fileChange', input: [{ path: 'a.txt' }, { path: 'b.txt' }] }, { id: 'f2', name: 'fileChange', input: { 'c.txt': { add: { content: 'x' } } } }]), ['Changing 3 files']);
  deepEqual(groupSteps([]), []);
  // The living sentence follows: inside a run it counts so far and names the newest step, elsewhere it is the step's own line.
  const reads = [read('a', 'a.rs'), read('b', 'b.rs'), read('c', 'c.rs')];
  equal(stepLine(groupSteps(reads), reads[0]!), 'Reading · a.rs', 'the first is a single step');
  equal(stepLine(groupSteps(reads), reads[2]!), 'Reading 3 files · c.rs');
  equal(stepLine(groupSteps([run('x')]), run('x')), toolLine(run('x')));
  equal(stepLine([], run('lost')), 'Running · ls', 'a step no group holds reads on its own');
});

check('worklog: a fold summarizes steps, opens them, and stays open while they grow', () => {
  useLanguage('en');
  const root = document.createElement('div'); document.body.append(root);
  const read = (id: string, ok?: boolean) => ({ id, name: 'Read', input: { file_path: `${id}.ts` }, ok });
  const [message, setMessage] = createSignal<Message>({ id: 'grouped', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [read('a', true), read('b', true)] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'grouped', options: [], onBranch: () => {} }), root);
  try {
    const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
    ok(head.textContent!.includes('Read 2 files'));
    equal(head.getAttribute('aria-expanded'), 'false');
    head.click();
    equal(root.querySelectorAll('.steps-list .step').length, 2);
    setMessage({ ...message(), tools: [...message().tools!, read('c')] });
    equal(head.getAttribute('aria-expanded'), 'true');
    ok(head.textContent!.includes('Reading · c.ts'));
    equal(root.querySelectorAll('.steps-list .step').length, 3);
    setMessage({ ...message(), tools: [read('a', true), read('b', false), read('c', true)] });
    ok(head.textContent!.includes('1 failed'));
    equal(root.querySelectorAll('.step[data-state="failed"]').length, 1);
  } finally { dispose(); root.remove(); }
});

check('worklog: folded steps fit a 720 px window in English and German', () => {
  try {
    for (const language of ['en', 'de']) {
      useLanguage(language);
      const root = document.createElement('div'); applyLook(sala({ motion: 'none' }), root); root.style.width = '720px'; document.body.append(root);
      const tools: ToolUse[] = [{ id: 'a', name: 'Read', input: { file_path: 'a.ts' }, ok: true }, { id: 'b', name: 'Read', input: { file_path: 'b.ts' }, ok: false }];
      const dispose = render(() => createComponent(Thread, { messages: [{ id: 'look', role: 'assistant', at: 0, status: 'done', text: 'Done.', tools }], from: 0, conversationId: 'look', options: [], onBranch: () => {} }), root);
      try {
        const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
        head.click();
        ok(root.querySelector('.step[data-state=failed] .st-failed'), 'a failed step uses the shared failure mark');
        head.click();
        for (const open of [false, true]) {
          equal(head.getAttribute('aria-expanded'), String(open));
          ok(head.getBoundingClientRect().right <= root.getBoundingClientRect().right + 1, language);
          if (!open) head.click();
        }
        equal(root.querySelectorAll('.steps-list .step').length, 2);
      } finally { dispose(); root.remove(); }
    }
  } finally { useLanguage('en'); }
});

// The time a subagent took, from the engine's own stamps on its call: shown once it ended, never for a quick or a refused one.
check('worklog: a subagent that ended says how long it took, and a quick or unfinished one does not', () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const agent = (over: object) => ({ id: 'ag', name: 'Agent', input: { description: 'Read the docs', subagent_type: 'Explore', prompt: 'Read.' }, started: 1_000_000, ...over });
  const [message, setMessage] = createSignal<Message>({ id: 'took', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [agent({})] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'took', options: [], onBranch: () => {} }), root);
  try {
    const meta = () => root.querySelector('.agents .agent-meta')!.textContent;
    equal(meta(), 'Explore · working · 0 steps', 'a running subagent has no time');
    setMessage({ ...message(), tools: [agent({ ok: true, ended: 1_000_000 + 150_000 })] });
    equal(meta(), 'Explore · finished · 0 steps · 2 min');
    setMessage({ ...message(), tools: [agent({ ok: false, ended: 1_000_000 + 45_000 })] });
    equal(meta(), 'Explore · failed · 0 steps · 45 s');
    setMessage({ ...message(), tools: [agent({ ok: true, ended: 1_000_400 })] });
    equal(meta(), 'Explore · finished · 0 steps', 'under a second is no time worth saying');
    setMessage({ ...message(), tools: [agent({ ok: false, declined: true, ended: 1_000_000 + 45_000 })] });
    equal(meta(), 'Explore · declined · 0 steps', 'a refused one never ran');
    setMessage({ ...message(), tools: [{ id: 'ag', name: 'Agent', input: { description: 'Old', prompt: 'x' }, ok: true }] });
    equal(meta(), 'finished · 0 steps', 'a subagent recorded before the stamps existed shows none');
  } finally {
    dispose();
    root.remove();
  }
});

check('settings_search: the settings operation reads as the changed choice without raw input', () => {
  useLanguage('en');
  deepEqual(toolRow({ id: 'settings-search', name: 'Settings', input: { web_search: 'cached' } }), {
    label: 'Changing web search', detail: t('set.webSearch.cached'),
  });
});

check('settings_search: unknown choices never leak into the settings work log', () => {
  useLanguage('en');
  deepEqual(toolRow({ id: 'bad-settings-search', name: 'Settings', input: { web_search: '<script>not a choice</script>' } }), { label: t('tool.settingsWebSearch'), detail: '' });
  deepEqual(
    toolRow({
      id: 'ps-wrapped',
      name: 'commandExecution',
      input: 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoProfile -Command \'[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $env:RUSTUP_HOME = "E:\\Apps\\rust\\rustup"; cargo t --suite ui\'',
    }),
    { label: 'PowerShell', detail: 'cargo t --suite ui' },
  );
});

check('timeline: words, thoughts, steps and agents keep their places and unchanged objects', () => {
  const tools: ToolUse[] = [
    { id: 'read', name: 'Read', input: {}, at: 3, ok: true },
    { id: 'agent', name: 'Agent', input: {}, at: 3, ok: true },
    { id: 'run', name: 'Bash', input: {}, at: 6, ok: true },
  ];
  const m: Message = { id: 't', role: 'assistant', at: 0, text: 'oneTWOend', tools, thinking: 'firstsecond',
    thinkingMarks: [{ from: 0, at: 3, steps: 0 }, { from: 5, at: 3, steps: 1 }] };
  const parts = timeline(m, m.text);
  deepEqual(parts.map((p) => p.kind), ['text', 'steps', 'agent', 'text', 'steps', 'text']);
  const first = parts[1]!;
  ok(first.kind === 'steps');
  if (first.kind === 'steps') deepEqual(first.items.map((item) => item.kind === 'thought' ? item.text : item.tool.id), ['first', 'read', 'second']);
  const kept = keepParts(parts, timeline(m, m.text));
  equal(kept, parts, 'an unchanged timeline retains its array');
  const grown = keepParts(parts, timeline({ ...m, text: m.text + '!' }, m.text + '!'));
  equal(grown[1], parts[1], 'unchanged steps keep their object');
  equal(grown[2], parts[2], 'the agent keeps its object');
  ok(grown[5] !== parts[5], 'the grown words get a new object');
  for (const end of [undefined, 3]) {
    const old = timeline({ ...m, thinking: '', thinkingMarks: [], activityEnd: end, tools: tools.filter((tool) => tool.name !== 'Agent').map(({ at: _at, ...tool }) => tool) }, m.text);
    deepEqual(old.map((p) => p.kind), end ? ['text', 'steps', 'text'] : ['steps', 'text']);
    const steps = old.find((p) => p.kind === 'steps');
    ok(steps?.kind === 'steps' && steps.items.length === 2, 'all old steps stay together at the old boundary');
  }
});

check('timeline: nested helpers stay beside their ancestor before the next step', () => {
  const m: Message = { id: 'nested', role: 'assistant', at: 0, text: 'before after', tools: [
    { id: 'parent', name: 'Agent', input: {}, at: 6 },
    { id: 'next', name: 'Read', input: {}, at: 6 },
    { id: 'child', name: 'Agent', input: {}, parent: 'parent' },
    { id: 'grandchild', name: 'Task', input: {}, parent: 'child' },
    { id: 'child-work', name: 'Read', input: {}, parent: 'child' },
  ] };
  const parts = timeline(m, m.text);
  deepEqual(parts.map((part) => part.kind === 'agent' ? part.tool.id : part.kind), ['text', 'parent', 'child', 'grandchild', 'steps', 'text']);
});

check('worklog: completed steps use past tense and summaries count ended work', () => {
  useLanguage('en');
  const read: ToolUse = { id: 'r', name: 'Read', input: { file_path: 'a' }, ok: true };
  const run: ToolUse = { id: 'c', name: 'Bash', input: { command: 'test' }, ok: true };
  equal(toolRow(read).label, t('tool.read'));
  equal(toolRow(read, true).label, t('step.read'));
  equal(toolRow(run, true).label, t('step.command'));
  equal(stepsSummary([read, run, { ...read, id: 'pending', ok: undefined }, { id: 'other', name: 'Unknown', input: {}, ok: false }]),
    'Read 1 file · Ran 1 command · 1 other step');
  equal(stepsSummary([{ ...run, ok: undefined }]), '');
});
