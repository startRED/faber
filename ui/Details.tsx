import { createMemo, createSignal, For, Show, type ComponentProps } from 'solid-js';
import { api, type BackgroundCommand, type Checks, type Follow, type Message, type ToolUse, type WorkCopy } from './api.ts';
import { ToolChip } from './AgentChat.tsx';
import { CommandStop } from './Commands.tsx';
import { ChangesFiles } from './Changes.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { agentState, type AgentState } from './worklog.ts';
import { showAgent } from './AgentChat.tsx';
import { backgroundCalls, backgroundOf, createBackgroundCache, followedIn, indexed, referenceLines, prune, workStatus, type BackgroundWork, type ToolIndex } from './background.ts';
import { LiveText } from './Background.tsx';

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
  const status = () => workStatus({ follow: props.f });
  const words = () => t(status().key, status().vars);
  const open = () => ['pending', 'failing', 'conflict'].includes(props.f.state);
  return (
    <p class="pace follow" classList={{ need: status().need }} data-state={props.f.state}>
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

type DetailTool = ToolUse & { detailState: AgentState | 'unknown'; agentAt?: { messageId: string; callId: string } };
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
    reviews: replies.flatMap((m) => backgroundCalls(m, indexed(m, cache)).flatMap((row) => row.agent ? [row.agent] : []).toReversed().filter((step) => !step.call.declined).map((step) => ({ ...step, call: {
      ...step.call, id: `${m.id}:${step.call.id}`,
      agentAt: { messageId: m.id, callId: step.call.id },
      detailState: step.call.ok === undefined && stopped(m) ? 'stopped' as const : agentState(step, m.status === 'streaming'),
    } }))),
    replies,
  };
}

/** Every project conversation has an entry, even before its first saved change. */
export function DetailsLine(props: { conversationId: string; messages: Message[]; busy?: boolean; weighing?: boolean; background?: BackgroundCommand[] }) {
  const cache = createBackgroundCache();
  const active = createMemo(() => backgroundOf(props.messages, props.background ?? [], props.conversationId, cache).filter((row) => workStatus(row).running).length);
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
    else if (reply?.status === 'stopped') return { saved, n: saved, key: 'details.unfinished' as const, need };
    else if (props.weighing) key = 'details.weighing';
    need = !['details.saved', 'details.line', 'details.weighing'].includes(key);
    // Observed running work replaces only neutral words; engine attention and the person's neutral Stop still win.
    if (!need && key !== 'details.weighing' && active()) return { saved, n: active(), key: 'background.count' as const, need };
    return { saved, n: saved, key, need };
  });
  return <p class="det-line" classList={{ need: state().need }} data-state={state().key}><span>{t(state().key, { n: state().n })}</span>{' · '}
    <button type="button" aria-controls="running" onClick={() => showDetails(props.conversationId)}>{t('details.open')}</button>
  </p>;
}

function Facts(props: { tools: DetailTool[]; follows?: Follow[]; conversationId?: string }) {
  const byId = createMemo(() => new Map(props.tools.map((tool) => [tool.id, tool])));
  const ids = createMemo(() => [...byId().keys()], [], { equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]) });
  return <ul class="det-facts"><For each={ids()}>{(id) => {
    const tool = () => byId().get(id)!;
    const [outputOpen, setOutputOpen] = createSignal(false);
    return <li data-state={tool().detailState}>
      <div class="tools"><ToolChip tool={tool()} /></div>
      <span class="det-state">{t(tool().detailState === 'unknown' ? 'details.state.unknown' : `agent.${tool().detailState as AgentState}`)}</span>
      <Show when={tool().agentAt && props.conversationId}><button type="button" class="btn" aria-controls="running" onClick={() => showAgent({ conversationId: props.conversationId!, ...tool().agentAt! })}>{t('agent.look')}</button></Show>
      <Show when={tool().output}><details class="disclosure" onToggle={(e) => setOutputOpen(e.currentTarget.open)}><summary>{t('details.output')}</summary><Show when={outputOpen()}><pre class="command-output"><LiveText text={tool().output!} follows={props.follows ?? []} /></pre></Show></details></Show>
    </li>;
  }}</For></ul>;
}

function BackgroundList(props: { rows: BackgroundWork[]; conversationId: string; follows: Follow[] }) {
  const byId = createMemo(() => new Map(props.rows.map((row) => [row.id, row])));
  const ids = createMemo(() => [...byId().keys()], [], { equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]) });
  return <ul class="background-work det-facts"><For each={ids()}>{(id) => {
    const row = () => byId().get(id)!;
    const status = () => workStatus(row());
    const [outputOpen, setOutputOpen] = createSignal(false);
    return <li data-work={id} data-state={status().state} data-need={status().need}>
      <Show when={row().follow} fallback={<Show when={row().checks} fallback={<>
        <div class="work-head">
        <Show when={row().agent} fallback={<span class="work-chip"><span>{row().title || t('background.checks')}</span><span class="work-state">{t(status().key, status().vars)}</span></span>}>
          <button type="button" class="work-chip" aria-controls="running" onClick={() => showAgent({ conversationId: props.conversationId, messageId: row().messageId!, callId: row().agent!.call.id })}>
            <span>{row().title}</span><span class="work-state">{t(status().key, status().vars)}</span>
          </button>
        </Show>
        <Show when={row().command}>{(command) => <CommandStop command={command()} />}</Show>
        </div>
        <Show when={status().reason}>{(reason) => <p class="work-reason">{t(reason())}</p>}</Show>
        <Show when={row().command?.output}><details class="disclosure" onToggle={(e) => setOutputOpen(e.currentTarget.open)}><summary>{t('details.output')}</summary><Show when={outputOpen()}><pre class="command-output"><LiveText text={row().command!.output} follows={props.follows} /></pre></Show></details></Show>
        <Show when={row().tool?.output}><details class="disclosure" onToggle={(e) => setOutputOpen(e.currentTarget.open)}><summary>{t('details.output')}</summary><Show when={outputOpen()}><pre class="command-output"><LiveText text={row().tool!.output!} follows={props.follows} /></pre></Show></details></Show>
      </>}>{(checks) => <ChecksLine c={checks()} />}</Show>}>
        {(follow) => <FollowLine conversationId={props.conversationId} f={follow()} />}
      </Show>
    </li>;
  }}</For></ul>;
}

/** A view of facts in the existing column, mounted only while open. Task 019 extends the live running list here. */
export function DetailsPane(props: {
  conversationId: string; messages: Message[]; background: BackgroundCommand[]; copy?: WorkCopy;
  changes: ComponentProps<typeof ChangesFiles>;
  onBack: () => void; onClose: () => void;
}) {
  const cache = createBackgroundCache();
  const d = createMemo(() => detailsOf(props.messages, cache.tools));
  const [referencesOpen, setReferencesOpen] = createSignal(false);
  const work = createMemo(() => backgroundOf(props.messages, props.background, props.conversationId, cache));
  const follows = createMemo(() => followedIn(props.messages));
  const checks = () => d().replies.filter((m) => m.checks && !work().some((row) => row.id === `${m.id}:checks`));
  const checkTools = () => d().checks.filter((tool) => !work().some((row) => row.id === tool.id));
  const reviews = () => d().reviews.filter((step) => !work().some((row) => row.id === step.call.id));
  const finishedFollows = () => follows().filter((f) => !work().some((row) => row.follow?.repo.toLowerCase() === f.repo.toLowerCase() && row.follow.number === f.number));
  const references = createMemo(() => referenceLines(d().replies, cache.lines, follows()));
  return <>
    <div class="rc-head side-head">
      <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
      <h2 id="running-title" class="display">{t('details.title')}</h2>
      <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
    </div>
    <div class="rc-body side-body details-view">
      {/* The copy's branch and folder stay here, for people who look, instead of in the topbar. */}
      <Show when={props.copy}>{(copy) => <p class="det-copy" data-testid="details-copy">{t('details.copy', { branch: copy().branch })}<span>{copy().path}</span></p>}</Show>
      <Show when={work().length}><section class="det-g"><h3>{t('background.title')}</h3>
        <BackgroundList rows={work()} conversationId={props.conversationId} follows={follows()} />
      </section></Show>
      <Show when={d().saved.length || finishedFollows().length}><section class="det-g saved-changes"><h3>{t('details.group.saved')}</h3>
        <Facts tools={d().saved} follows={follows()} />
        <For each={finishedFollows().map((f) => `${f.repo.toLowerCase()}#${f.number}`)}>{(id) => <FollowLine conversationId={props.conversationId} f={finishedFollows().find((f) => `${f.repo.toLowerCase()}#${f.number}` === id)!} />}</For>
      </section></Show>
      <Show when={references().length}><section class="det-g reply-references"><h3>{t('background.references')}</h3><details class="disclosure" onToggle={(e) => setReferencesOpen(e.currentTarget.open)}><summary>{t('details.open')}</summary><Show when={referencesOpen()}><For each={references()}>{(line) => <p class="technical-reference"><LiveText text={line} follows={follows()} /></p>}</For></Show></details></section></Show>
      <Show when={checkTools().length || checks().length}><section class="det-g"><h3>{t('details.group.checks')}</h3>
        <Facts tools={checkTools()} follows={follows()} />
        <For each={checks().map((m) => m.id)}>{(id) => <ChecksLine c={checks().find((m) => m.id === id)!.checks!} />}</For>
      </section></Show>
      <section class="det-g changes-view"><h3>{t('details.group.files')}</h3><ChangesFiles {...props.changes} /></section>
      <Show when={reviews().length}><section class="det-g"><h3>{t('details.group.reviews')}</h3><Facts tools={reviews().map((step) => ({ ...step.call, output: step.report }))} follows={follows()} conversationId={props.conversationId} /></section></Show>
    </div>
  </>;
}
