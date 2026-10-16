import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import recording from '../fixtures/claude/subagents.jsonl?raw';
import { Thread } from '../../ui/Thread.tsx';
import type { Message, ToolUse } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { atWork, stepsOf, toolRow } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// The calls of a real run with two subagents (tests/fixtures/claude/subagents.jsonl), in the order they arrived and with the
// step each one ran in, as the engine keeps them, and how each call ended.
function recorded(): { tools: ToolUse[]; results: { id: string; ok: boolean }[] } {
  const tools: ToolUse[] = [];
  const results: { id: string; ok: boolean }[] = [];
  for (const line of recording.trim().split('\n')) {
    const { dir, msg } = JSON.parse(line);
    if (dir !== 'out') continue;
    for (const c of msg.message?.content ?? []) {
      if (msg.type === 'assistant' && c.type === 'tool_use') tools.push({ id: c.id, name: c.name, input: c.input, ...(msg.parent_tool_use_id ? { parent: msg.parent_tool_use_id } : {}) });
      if (msg.type === 'user' && c.type === 'tool_result') results.push({ id: c.tool_use_id, ok: c.is_error !== true });
    }
  }
  return { tools, results };
}

check('worklog: every Claude Code tool reads as what it does, in words', () => {
  equal(toolRow({ id: '1', name: 'Read', input: { file_path: 'C:\\p\\a.txt' } }).label, t('tool.read'));
  equal(toolRow({ id: '2', name: 'Bash', input: { command: 'npm test', description: 'Run the tests' } }).detail, 'npm test', 'a command shows itself');
  deepEqual(toolRow({ id: '3', name: 'WebFetch', input: { url: 'https://akitaonrails.com/en/' } }), { label: t('tool.read'), detail: 'akitaonrails.com/en/' });
  deepEqual(toolRow({ id: '4', name: 'mcp__github__get_issue', input: {} }), { label: t('tool.mcp'), detail: 'github' });
  deepEqual(toolRow({ id: '5', name: 'Agent', input: { description: 'reader-a', prompt: 'Read a.txt' } }), { label: t('tool.agent'), detail: 'reader-a' });
  equal(toolRow({ id: '6', name: 'SomeNewTool', input: {} }).label, 'SomeNewTool', 'a tool Faber does not know keeps its name');
});

check('worklog: a subagent call goes under its step, and one whose step is missing stays visible', () => {
  const { tools } = recorded();
  const steps = stepsOf(tools);
  deepEqual(steps.map((s) => s.tool.name), ['Agent', 'Agent']);
  deepEqual(steps.map((s) => s.children.map((c) => c.name)), [['Read'], ['Read']]);
  ok(steps.every((s) => s.children.every((c) => c.parent === s.tool.id)));
  const nested = stepsOf([{ id: 'a', name: 'Agent', input: {} }, { id: 'b', name: 'Agent', input: {}, parent: 'a' }, { id: 'c', name: 'Read', input: {}, parent: 'b' }]);
  deepEqual(nested.map((s) => s.children.map((c) => c.id)), [['b', 'c']], 'a subagent started by a subagent stays under the first step');
  deepEqual(stepsOf([{ id: 'x', name: 'Read', input: {}, parent: 'gone' }]).map((s) => s.tool.id), ['x']);
});

check('worklog: a subagent sent to the background stays at work while a call of its own runs', () => {
  const step = (childOk?: boolean, background = true) => stepsOf([
    { id: 'a', name: 'Agent', input: { description: 'reader', run_in_background: background }, ok: true },
    { id: 'r', name: 'Read', input: {}, parent: 'a', ok: childOk },
  ])[0]!;
  ok(atWork(step()), 'its step returned at once, but its read still runs');
  ok(!atWork(step(true)), 'its last call ended');
  ok(!atWork(step(undefined, false)), 'a step that returned is done when it was not sent to the background');
  ok(!atWork(stepsOf([{ id: 'b', name: 'Bash', input: {} }])[0]!), 'a plain command is not a subagent');
});

check('worklog: while subagents work, each has a line with its name and what it does now', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const { tools, results } = recorded();
  const [message, setMessage] = createSignal<Message>({ id: 'subagents', role: 'assistant', at: 0, status: 'streaming', text: '', tools });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'subagents', options: [], onBranch: () => {},
  }), root);
  try {
    const status = () => root.querySelector<HTMLElement>('.activity-latest');
    const lines = () => [...status()!.querySelectorAll('span')].map((s) => s.textContent);
    equal(lines().length, 2, 'one line per subagent at work');
    ok(lines()[0]!.startsWith('reader-a'), lines()[0]);
    ok(lines()[0]!.includes(`${t('tool.read')} · `) && lines()[0]!.includes('a.txt'), lines()[0]);
    ok(lines()[1]!.startsWith('reader-b') && lines()[1]!.includes('b.txt'), lines()[1]);
    // Four at work show three and fold the rest.
    const more = ['c', 'd'].map((n) => ({ id: n, name: 'Agent', input: { description: `reader-${n}` } }));
    setMessage({ ...message(), tools: [...tools, ...more] });
    deepEqual(lines().at(-1), t('tool.more', { n: 1 }));
    equal(lines().length, 4);
    // A finished subagent leaves the status; its calls stay folded under its step in the log.
    const done = new Map(results.map((r) => [r.id, r.ok]));
    setMessage({ ...message(), tools: tools.map((tool) => ({ ...tool, ok: done.get(tool.id) })) });
    ok(!status()!.classList.contains('subagents'), 'with nothing at work the status is the latest step again');
    const log = root.querySelector('.activity .tools')!;
    equal(log.querySelectorAll(':scope > span').length, 2, 'the reply took two steps');
    equal(log.querySelectorAll('.tool-children').length, 2);
    equal(log.querySelectorAll('.tool-children > span').length, 2, 'each subagent read one file');
    setMessage({ ...message(), status: 'done' });
    equal(status(), null);
  } finally {
    dispose();
    root.remove();
  }
});
