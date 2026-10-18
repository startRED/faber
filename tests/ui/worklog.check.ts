import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { changeOf, sourcesOf, toolRow } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

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
  // Faber's checks (R190) read as the level they run, with nothing after.
  deepEqual(toolRow({ id: '9', name: 'Verify', input: {} }), { label: t('tool.verify'), detail: '' });
  deepEqual(toolRow({ id: '10', name: 'Verify', input: { level: 'full' } }), { label: t('tool.verifyFull'), detail: '' });
  deepEqual(toolRow({ id: '11', name: 'Map', input: {} }), { label: t('tool.map'), detail: '' });
  deepEqual(toolRow({ id: '12', name: 'Map', input: { kind: 'setting', query: 'theme' } }), { label: t('tool.map'), detail: 'setting theme' });
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
    equal(root.querySelector('.tools .delta')!.getAttribute('aria-label'), '6 lines added');
    setMessage({ ...message(), tools: [{ ...message().tools![0]!, ok: true }] });
    ok(root.querySelector('.changed'), 'a finished edit stays until the next step or the answer');
    const said = 'Writing the notes.\n\n';
    setMessage({ ...message(), text: said, textBreaks: [said.length], activityEnd: said.length });
    ok(root.querySelector('.changed'), 'narration before the edit is not the answer');
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
    const long = () => root.querySelector('.activity-latest .long');
    equal(long(), null, 'five seconds is no long step');
    setMessage({ ...message(), tools: [build(150_000, undefined, 'slow')] });
    equal(long()!.textContent, ' · 2 min');
    equal(long()!.getAttribute('aria-hidden'), 'true', 'the live region says the step, not a clock');
    equal(root.querySelector('.activity-latest .step')!.textContent, 'Running · cargo build', 'the step keeps its words');
    setMessage({ ...message(), tools: [build(150_000, true, 'slow')] });
    equal(long(), null, 'an ended step has no running time');
    setMessage({ ...message(), tools: [{ id: 'old', name: 'Bash', input: { command: 'cargo build' } }] });
    equal(long(), null, 'a step recorded before the stamp existed shows none');
  } finally {
    dispose();
    root.remove();
  }
});
