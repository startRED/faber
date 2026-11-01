import type { Message, ToolUse } from '../../ui/api.ts';
import { liveWork } from '../../ui/livework.ts';
import { stepLine, groupSteps } from '../../ui/worklog.ts';
import { check, deepEqual, equal } from './check.ts';

const START = 1_700_000_000_000;
const reply = (over: Partial<Message> = {}): Message => ({ id: 'r', role: 'assistant', at: START, status: 'streaming', text: '', ...over });
const ask: Message = { id: 'u', role: 'user', at: START, text: 'Go' };
const kind = (m: Partial<Message>, extra: { keepWait?: { why: 'retry' | 'question'; until: number } } = {}) =>
  liveWork({ messages: [ask, reply(m)], ...extra })?.kind;

check('live work: the person needed comes first, then a failure, then work, then done', () => {
  const permission = { requestId: 'q', tool: 'Bash', input: {} };
  equal(kind({ status: 'stopped', permissions: [permission] }), 'needs', 'a question beats a stop');
  equal(kind({ status: 'done', needsYou: true }), 'needs', "the engine's needs-you counts");
  equal(kind({ status: 'done' }, { keepWait: { why: 'question', until: 0 } }), 'needs', 'keep going waits on a question');
  equal(kind({ status: 'done', checks: { state: 'failed', passed: 0 } }), 'failed', 'a failed check beats Faber still at work');
  equal(kind({ status: 'error', error: 'error.spawn' }), 'failed');
  equal(kind({ status: 'streaming' }), 'working');
  equal(kind({ status: 'streaming', finishedAt: START + 1 }), 'done', 'the agent finished while the CLI closes its turn');
  equal(kind({ status: 'done' }, { keepWait: { why: 'retry', until: 0 } }), 'working', 'keep going waits to send the next turn');
  equal(kind({ status: 'error', error: 'error.quotaWait' }), 'working', 'a wait for quota is not a failure');
  equal(kind({ status: 'error', error: 'error.quotaWait', checks: { state: 'failed', passed: 0 } }), 'failed', 'a failed check is not hidden by a quota wait');
  const follow = (state: 'pending' | 'failing', held?: boolean) => ({ repo: 'o/r', number: 7, url: 'u', state, held, since: START });
  equal(kind({ status: 'done', follow: follow('failing', true) }), 'needs', 'checks that failed on a tight quota wait for the person');
  equal(kind({ status: 'done', follow: follow('failing') }), 'failed', "a followed pull request's failure shows");
  equal(kind({ status: 'done', follow: follow('pending') }), 'working', 'Faber still follows its checks');
  equal(kind({ status: 'done' }), 'done');
  equal(liveWork({ messages: [ask] }), undefined, 'no reply, no state');
});

check('live work: a state belongs to its own answer, never to a newer turn', () => {
  const messages = [reply({ id: 'old', status: 'stopped' }), ask, reply({ id: 'new', at: START + 5 })];
  deepEqual(liveWork({ messages }), { kind: 'working', answer: 'new', since: START + 5, what: undefined, helpers: [] });
  deepEqual(liveWork({ messages, at: 0 }), { kind: 'failed', answer: 'old', since: START, helpers: [] }, 'the old stop stays its own');
  equal(liveWork({ messages: [reply({ id: 'old', status: 'done' }), ask, reply({ id: 'new' })], at: 0 })?.kind, 'done', "the newer turn's work is not the old answer's");
});

check('live work: what it does is in plain words, and never a shell line', () => {
  const read: ToolUse = { id: 'a', name: 'Read', input: { file_path: 'src/a.ts' } };
  const bash: ToolUse = { id: 'b', name: 'Bash', input: { command: 'sleep 580; tail -8 log' } };
  const what = (tools: ToolUse[], now?: string) => liveWork({ messages: [reply({ tools, now })] })?.what;
  equal(what([read]), stepLine(groupSteps([read]), read), 'a running read says what it reads');
  equal(what([read, bash]), undefined, 'a running command gives no words');
  equal(what([{ id: 'c', name: 'commandExecution', input: 'npm test' }]), undefined, "Codex's command gives no words");
  equal(what([{ id: 'm', name: 'mcp__terminal__run', input: { command: 'Remove-Item C:/demo' } }]), undefined, "a tool server's terminal gives no words");
  equal(what([{ ...read, ok: true }]), undefined, 'nothing open, nothing to say');
  equal(what([read, bash], 'Checking the build'), 'Checking the build', "the agent's own words come first");
  equal(liveWork({ messages: [reply({ status: 'done', tools: [read] })] })?.what, undefined, "Faber's own work is not a step");
});

check('live work: helpers list in the order they started, each with its own state', () => {
  const agent = (id: string, started: number | undefined, over: Partial<ToolUse> = {}): ToolUse => ({ id, name: 'Agent', input: { description: `look ${id}` }, started, ...over });
  const tools = [agent('late', START + 20), agent('first', START + 10, { ok: true }), agent('ask', START + 15, { sign: 'waiting' }), agent('old', undefined)];
  const work = liveWork({ messages: [reply({ tools })] })!;
  deepEqual(work.helpers.map((h) => [h.id, h.state]), [['first', 'done'], ['ask', 'waiting'], ['late', 'running'], ['old', 'running']]);
  equal(work.helpers[0]!.title, 'look first');
  equal(work.kind, 'needs', 'a helper waiting on the person needs them');
  const stopped = liveWork({ messages: [reply({ status: 'stopped', tools: [agent('bg', START, { input: { description: 'bg', run_in_background: true } })] })] })!;
  deepEqual(stopped.helpers.map((h) => h.state), ['stopped'], 'a stop ends a helper in the background too');
});
