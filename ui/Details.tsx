import { createMemo, createSignal, For, Show, type ComponentProps } from 'solid-js';
import { api, type BackgroundCommand, type Checks, type Follow, type Message, type ToolUse } from './api.ts';
import { ToolChip } from './AgentChat.tsx';
import { CommandRow } from './Commands.tsx';
import { ChangesFiles } from './Changes.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { agentState, agentSteps, isAgentCall, type AgentState } from './worklog.ts';

// Faber's own checks of a reply that changed files (R146): one quiet line while they run and once they ended, the failed
// checks' first failures behind a disclosure. Checks that still fail after Faber's fixes wait for the person, in amber.
export function ChecksLine(props: { c: Checks }) {
  const failed = () => props.c.failed ?? [];
  const words = () => t(`checks.${props.c.state}` as Parameters<typeof t>[0], { n: props.c.passed, checks: failed().map((f) => f.command).join(', ') });
  return (
    <div class="pace checks" classList={{ need: props.c.state === 'needsYou' }} data-state={props.c.state}>
      <p>{words()}</p>
      <Show when={props.c.state !== 'running' && failed().length}>
        <details class="disclosure">
          <summary>{t('checks.why')}</summary>
          <For each={failed()}>
            {(f) => (
              <div class="checks-failed">
                <code>{f.command}</code>
                <Show when={(f.lines ?? []).join('').trim()} fallback={<p>{t('checks.noDetails')}</p>}>
                  <pre>{(f.lines ?? []).join('')}</pre>
                </Show>
              </div>
            )}
          </For>
        </details>
      </Show>
    </div>
  );
}

// The pull request a reply opened or pushed (R200): one quiet line while Faber follows its checks and after it ends, the
// number a link to the page, and one way to stop. Checks that failed on a tight quota wait for the person, in amber.
export function FollowLine(props: { conversationId: string; f: Follow }) {
  const words = () => {
    const f = props.f;
    const vars = { pr: `#${f.number}`, checks: (f.failed ?? []).join(', ') };
    if (f.held && f.state === 'failing') return t('follow.held', vars);
    return t(`follow.${f.problem?.replace('follow.', '') ?? f.state}` as Parameters<typeof t>[0], vars);
  };
  const open = () => ['pending', 'failing', 'conflict'].includes(props.f.state);
  return (
    <p class="pace follow" classList={{ need: !!props.f.held && open() }} data-state={props.f.state}>
      <a href={props.f.url} title={`${props.f.repo}#${props.f.number}`} onClick={(e) => (e.preventDefault(), void api.openLink(props.f.url))}>
        {words()}
      </a>
      <Show when={open()}>
        <button type="button" class="btn" onClick={() => void api.unfollowPr(props.conversationId)}>
          {t('follow.stop')}
        </button>
      </Show>
    </p>
  );
}


const [opened, setOpened] = createSignal<string>();
export const openedDetails = opened;
export const showDetails = (conversationId?: string) => setOpened(conversationId);

type DetailTool = ToolUse & { detailState: AgentState | 'unknown' };
interface ToolIndex {
  length: number;
  first?: string;
  last?: string;
  byId: Map<string, number>;
  checks: number[];
  reviews: number[];
}

// Inputs and kinds are fixed when a call starts; text/output ticks only refresh the selected calls' outcomes.
function indexed(m: Message, cache: Map<string, ToolIndex>): ToolIndex {
  const tools = m.tools ?? [];
  const old = cache.get(m.id);
  if (old?.length === tools.length && old.first === tools[0]?.id && old.last === tools.at(-1)?.id) return old;
  const next: ToolIndex = { length: tools.length, first: tools[0]?.id, last: tools.at(-1)?.id, byId: new Map(), checks: [], reviews: [] };
  tools.forEach((tool, i) => {
    next.byId.set(tool.id, i);
    if (tool.check || tool.name === 'Verify') next.checks.push(i);
    if (isAgentCall(tool)) next.reviews.push(i);
  });
  cache.set(m.id, next);
  return next;
}

function prune(cache: Map<string, ToolIndex>, messages: Message[]) {
  const ids = new Set(messages.map((m) => m.id));
  for (const id of cache.keys()) if (!ids.has(id)) cache.delete(id);
}

/** Existing parsed facts, newest first. File contents/counts come from the shared Changes view, never inferred edits. */
export function detailsOf(messages: Message[], cache = new Map<string, ToolIndex>()) {
  prune(cache, messages);
  const replies = messages.filter((m) => m.role === 'assistant').toReversed();
  const stopped = (m: Message) => m.status === 'stopped' || m.status === 'error';
  const tools = (m: Message, indexes: number[]): DetailTool[] => indexes.toReversed().map((i) => m.tools![i]!).filter((tool) => !tool.declined).map((tool) => ({
    ...tool, id: `${m.id}:${tool.id}`,
    detailState: tool.ok === true ? 'done' : tool.ok === false ? 'failed' : stopped(m) ? 'stopped' : m.status === 'streaming' ? 'running' : 'unknown',
  }));
  return {
    saved: replies.flatMap((m) => tools(m, (m.commits ?? []).flatMap((id) => { const i = indexed(m, cache).byId.get(id); return i === undefined ? [] : [i]; }))),
    checks: replies.flatMap((m) => tools(m, indexed(m, cache).checks)),
    reviews: replies.flatMap((m) => indexed(m, cache).reviews.toReversed().map((i) => agentSteps([m.tools![i]!])[0]!).filter((step) => !step.call.declined).map((step) => ({ ...step, call: {
      ...step.call, id: `${m.id}:${step.call.id}`,
      detailState: step.call.ok === undefined && stopped(m) ? 'stopped' as const : agentState(step, m.status === 'streaming'),
    } }))),
    replies,
  };
}

/** Every project conversation has an entry, even before its first saved change. */
export function DetailsLine(props: { conversationId: string; messages: Message[]; busy?: boolean; weighing?: boolean }) {
  const state = createMemo(() => {
    const latest = props.messages.at(-1);
    const reply = latest?.role === 'assistant' ? latest : undefined;
    const saved = props.messages.reduce((n, m) => n + (m.commits?.length ?? 0), 0);
    const busy = props.busy ?? (reply?.status === 'streaming' && reply.finishedAt === undefined);
    const checks = reply?.checks;
    const follow = reply?.follow;
    let key: Parameters<typeof t>[0] = saved ? 'details.saved' : 'details.line';
    let need = false;
    if (reply?.needsYou || checks?.state === 'needsYou') key = 'run.needsYou';
    else if (checks?.state === 'failed') key = busy ? 'details.fixing' : 'details.checkFailed';
    else if (follow?.state === 'failing' || follow?.state === 'conflict') key = busy ? 'details.changeFixing' : 'details.changeFailed';
    else if (reply?.error || reply?.status === 'error' || reply?.wrapUp?.done === false) key = 'details.unfinished';
    else if (reply?.status === 'stopped') return { saved, key: 'details.unfinished' as const, need };
    else if (props.weighing) key = 'details.weighing';
    need = !['details.saved', 'details.line', 'details.weighing'].includes(key);
    return { saved, key, need };
  });
  return <p class="det-line" classList={{ need: state().need }} data-state={state().key}><span>{t(state().key, { n: state().saved })}</span>{' · '}
    <button type="button" aria-controls="running" onClick={() => showDetails(props.conversationId)}>{t('details.open')}</button>
  </p>;
}

function Facts(props: { tools: DetailTool[] }) {
  const byId = createMemo(() => new Map(props.tools.map((tool) => [tool.id, tool])));
  const ids = createMemo(() => [...byId().keys()], [], { equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]) });
  return <ul class="det-facts"><For each={ids()}>{(id) => {
    const tool = () => byId().get(id)!;
    const [outputOpen, setOutputOpen] = createSignal(false);
    return <li data-state={tool().detailState}>
      <div class="tools"><ToolChip tool={tool()} full /></div>
      <span class="det-state">{t(tool().detailState === 'unknown' ? 'details.state.unknown' : `agent.${tool().detailState as AgentState}`)}</span>
      <Show when={tool().output}><details class="disclosure" onToggle={(e) => setOutputOpen(e.currentTarget.open)}><summary>{t('details.output')}</summary><Show when={outputOpen()}><pre class="command-output">{tool().output}</pre></Show></details></Show>
    </li>;
  }}</For></ul>;
}

/** A view of facts in the existing column, mounted only while open. Task 019 extends the live running list here. */
export function DetailsPane(props: {
  conversationId: string; messages: Message[]; background: BackgroundCommand[];
  changes: ComponentProps<typeof ChangesFiles>;
  onBack: () => void; onClose: () => void;
}) {
  const cache = new Map<string, ToolIndex>();
  const d = createMemo(() => detailsOf(props.messages, cache));
  const commands = createMemo(() => new Map(props.background.filter((command) => command.conversationId === props.conversationId).toReversed().map((command) => [command.id, command])));
  const follows = createMemo(() => new Map(d().replies.filter((m) => m.follow).map((m) => [m.id, m.follow!])));
  const checks = createMemo(() => new Map(d().replies.filter((m) => m.checks).map((m) => [m.id, m.checks!])));
  return <>
    <div class="rc-head side-head">
      <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
      <h2 id="running-title" class="display">{t('details.title')}</h2>
      <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
    </div>
    <div class="rc-body side-body details-view">
      <Show when={commands().size}><section class="det-g"><h3>{t('commands.title')}</h3>
        <ul class="ritems"><For each={[...commands().keys()]}>{(id) => <CommandRow command={commands().get(id)!} title="" />}</For></ul>
      </section></Show>
      <Show when={d().saved.length || follows().size}><section class="det-g"><h3>{t('details.group.saved')}</h3>
        <Facts tools={d().saved} />
        <For each={[...follows().keys()]}>{(id) => <FollowLine conversationId={props.conversationId} f={follows().get(id)!} />}</For>
      </section></Show>
      <Show when={d().checks.length || checks().size}><section class="det-g"><h3>{t('details.group.checks')}</h3>
        <Facts tools={d().checks} />
        <For each={[...checks().keys()]}>{(id) => <ChecksLine c={checks().get(id)!} />}</For>
      </section></Show>
      <section class="det-g changes-view"><h3>{t('details.group.files')}</h3><ChangesFiles {...props.changes} /></section>
      <Show when={d().reviews.length}><section class="det-g"><h3>{t('details.group.reviews')}</h3><Facts tools={d().reviews.map((step) => ({ ...step.call, output: step.report }))} /></section></Show>
    </div>
  </>;
}
