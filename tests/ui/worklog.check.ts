import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { changeOf, toolRow } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('worklog: every Claude Code tool reads as what it does, in words', () => {
  equal(toolRow({ id: '1', name: 'Read', input: { file_path: 'C:\\p\\a.txt' } }).label, t('tool.read'));
  equal(toolRow({ id: '2', name: 'Bash', input: { command: 'npm test', description: 'Run the tests' } }).detail, 'npm test', 'a command shows itself');
  deepEqual(toolRow({ id: '3', name: 'WebFetch', input: { url: 'https://akitaonrails.com/en/' } }), { label: t('tool.read'), detail: 'akitaonrails.com/en/' });
  deepEqual(toolRow({ id: '4', name: 'mcp__github__get_issue', input: {} }), { label: t('tool.mcp'), detail: 'github' });
  // A helper started by a helper shows as a step inside the first one's row (R161), named by the words it was given.
  deepEqual(toolRow({ id: '5', name: 'Agent', input: { description: 'reader-a', prompt: 'Read a.txt' }, parent: 'x' }), { label: t('tool.agent'), detail: 'reader-a' });
  equal(toolRow({ id: '6', name: 'SomeNewTool', input: {} }).label, 'SomeNewTool', 'a tool Faber does not know keeps its name');
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
