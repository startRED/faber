import { createEffect, createMemo, createResource, createSignal, For, Index, onCleanup, Show, type Accessor } from 'solid-js';
import { api, type Checks, type Choice, type Follow, type Message, type QueuedTurn, type VendorUsage } from './api.ts';
import { Asks, Parkeds } from './Ask.tsx';
import { copyItem, openMenu } from './contextmenu.ts';
import { keep, kept, release, useDraft } from './unsent.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { QuotaCard } from './QuotaCard.tsx';
import { html, Markdown, openLinks } from './markdown.tsx';
import { agentMeta, ChangedLines, openedAgent, SourceList, showAgent, tellAbout, ToolChip } from './AgentChat.tsx';
import { untilReset } from './Sidebar.tsx';
import { AwayPage, awaySites, SiteCard, sitesOf, stepsOf } from './Site.tsx';
import { bytes, compact, fromEngine, money, t } from './i18n.ts';
import { prettyModel, vendorName, type Option } from './models.ts';
import { animateList } from './motion.ts';
import { windowHidden } from './appearance.ts';
import { elapsed, warmLeft } from './running.ts';
import { agePending, ageNow, footText, tickWhile } from './foot.ts';
import { quotaPending } from './QuotaCard.tsx';
import { agentState, agentSteps, changeOf, isAgentCall, middle, sameTools, sourcesOf, thinkingParts, toolLine, type AgentStep } from './worklog.ts';

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

// A file the answer created in the chat workspace: images show inline; anything can be saved to the computer.
function FileCard(props: { conversationId: string; file: { path: string; size: number } }) {
  const [preview] = createResource(
    () => (IMAGE.test(props.file.path) ? props.file.path : undefined),
    (path) => api.readImage(props.conversationId, path),
  );
  return (
    <div class="file">
      <Show when={preview()}>{(src) => <img src={src()} alt={props.file.path} />}</Show>
      <div>
        <span>{props.file.path}</span>
        <small>{bytes(props.file.size)}</small>
        <button class="btn" onClick={() => void api.saveFile(props.conversationId, props.file.path)}>
          {t('files.save')}
        </button>
        <button class="btn" onClick={() => void api.showFile(props.conversationId, props.file.path)}>
          {t('files.show')}
        </button>
      </div>
    </div>
  );
}

// What an answer cost at API prices, small: the visible number is the cost, and the tooltip holds the rest (what it counted and,
// on the latest answer, how long the prompt cache stays warm).
const costOf = (m: Message) => (m.costUsd ? money(m.costUsd, m.costUsd < 0.01 ? 4 : 2) : '');
function costTitle(m: Message, warm: string): string {
  const parts = [t('answer.cost', { cost: money(m.costUsd ?? 0, 4) })];
  if (m.inputTokens) parts.push(t('answer.tokens', { input: compact(m.inputTokens), output: compact(m.outputTokens ?? 0) }));
  if (warm) parts.push(warm);
  return parts.join(' · ');
}

// The picker names Claude models by alias ("opus"), so the full id of the next rung is matched to its entry.
function rung(nudge: NonNullable<Message['nudge']>, options: Option[]): Choice {
  const entry = options.find((o) => o.choice.vendor === nudge.vendor && (o.choice.model === nudge.model || o.choice.model === nudge.model.split('-')[1]));
  return { ...(entry?.choice ?? { vendor: nudge.vendor, model: nudge.model }), effort: nudge.effort };
}

// A subagent the agent handed part of the work to (R161): one quiet row in the reply with what it was asked and whether it
// is still working; opened, what it ran and the report it gave back. Never a board: the agent decides, this only shows it.
// One that seems stuck turns amber with what the engine saw and two ways in (R161 step 6); one waiting on the person's
// answer says so, and the card below the rows is that answer.
function AgentRow(props: { step: Accessor<AgentStep>; streaming: boolean; conversationId: string; messageId: string }) {
  const step = () => props.step();
  const state = () => agentState(step(), props.streaming);
  const live = () => ['running', 'background', 'waiting', 'stuck'].includes(state());
  const open = () => showAgent({ conversationId: props.conversationId, messageId: props.messageId, callId: step().call.id });
  const shown = () => {
    const agent = openedAgent();
    return agent?.conversationId === props.conversationId && agent.callId === step().call.id;
  };
  return (
    <div class="agent-item" role="listitem">
      <button type="button" class="agent" data-state={state()} aria-controls="running" aria-current={shown() ? 'true' : undefined} onClick={open}>
        <span class="agent-mark" aria-hidden="true">
          <Show when={live()} fallback={<Show when={state() === 'done' || state() === 'failed'}><Icon name={state() === 'done' ? 'check' : 'close'} small /></Show>}>
            <span class="live" />
          </Show>
        </span>
        <b>{step().title || t('agent.untitled')}</b>
        <span class="agent-meta">{agentMeta(step(), props.streaming)}</span>
      </button>
      <Show when={state() === 'stuck' && step().call.sign}>
        {(sign) => (
          <div class="agent-stuck" role="status">
            <span class="agent-why">{t(`agent.stuck.${sign() as 'repeat' | 'failing' | 'silent'}`)}</span>
            <button type="button" class="btn" onClick={open}>{t('agent.look')}</button>
            <button type="button" class="btn" onClick={() => tellAbout(step().title || t('agent.untitled'))}>{t('agent.tell')}</button>
          </div>
        )}
      </Show>
    </div>
  );
}

type OnBranch =(messageId: string, text: string, choice?: Choice) => void;

interface Shared {
  conversationId: string;
  options: Option[];
  /** What the composer has chosen now: the model a retry goes to. */
  choice?: Choice;
  onBranch: OnBranch;
  usage?: VendorUsage[];
}

function Answer(props: Shared & { m: Accessor<Message>; messages?: Message[]; prompt?: Message; last?: boolean }) {
  const m = () => props.m();
  // While an answer streams the engine sends the whole message every 33 ms. Each memo below compares by value, so a tick that
  // changed only the tools never touches the text, and one that changed only the text never touches the tools.
  const text = createMemo(() => m().text);
  const tools = createMemo(() => m().tools ?? [], [], { equals: sameTools });
  // A quiet note on the latest reply only (a later turn overtakes an older one): how long the CLI keeps its prompt cache warm,
  // shown in the cost's tooltip, hidden while unknown and once cold. One timer, and only while the note is shown.
  const [now, setNow] = createSignal(Date.now());
  createEffect(() => {
    const left = props.last ? warmLeft(m().cacheWarmUntil, Date.now()) : undefined;
    if (left === undefined) return;
    const timer = setTimeout(() => setNow(Date.now()), left + 50);
    onCleanup(() => clearTimeout(timer));
  });
  const warm = () => {
    const until = m().cacheWarmUntil;
    if (!props.last || warmLeft(until, Math.max(now(), Date.now())) === undefined) return '';
    return t('answer.cacheWarm', { time: new Date(until!).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) });
  };
  const streaming = () => m().status === 'streaming';
  // Streaming and not yet finished: the agent still writes or works. Once it said all it will say, the reply reads as done while the
  // CLI closes its turn (its Stop hooks, say), and more words after that would undo it.
  const working = () => streaming() && m().finishedAt === undefined;
  // The foot line says which of the two it is, how long the work took, and on the newest reply how long ago it last changed. One
  // timer, only for the newest reply, only while its age can change and the window is shown; a memo, so it is made again only
  // when that flips, never on the 33 ms ticks of the stream. It reads the ticking clock, so past a day the timer stops.
  tickWhile(createMemo(() => agePending(m(), !!props.last, ageNow())));
  const foot = () => footText(m(), !!props.last, ageNow());
  const waiting = () => m().permissions ?? [];
  // Unknown legacy boundaries and tool-terminated turns keep the entire answer visible. So does a reply that is still
  // streaming: the boundary moves with every step, and what the agent said between steps would fold away and come back
  // as the stream goes on. It folds into the activity once, when the answer ends.
  const boundary = createMemo(() => {
    const end = m().activityEnd;
    return !working() && tools().length && Number.isInteger(end) && end! > 0 && end! < text().length && text().slice(end).trim() ? end! : 0;
  });
  // The answer is always kept whole. A status sentence that is exactly the answer's opening paragraph is dropped from the
  // activity instead, so it shows once.
  const finalText = createMemo(() => text().slice(boundary()));
  const narration = createMemo(() => {
    if (!boundary()) return [];
    const paragraphs = text().slice(0, boundary()).trimEnd().split(/\n{2,}/);
    const last = paragraphs.at(-1)?.trim();
    const answer = finalText().trimStart();
    if (last && answer.startsWith(last) && (answer.length === last.length || /\s/.test(answer[last.length]!))) paragraphs.pop();
    const rest = paragraphs.join('\n\n').trim();
    return rest ? [rest] : [];
  });
  const thinkingText = createMemo(() => m().thinking ?? '');
  const thinking = createMemo(() => thinkingParts(thinkingText()));
  // The agent's own tools; each subagent and what it ran show as their own step below (AgentRow).
  const own = createMemo(() => tools().filter((tool) => !tool.parent && !isAgentCall(tool)), [], { equals: sameTools });
  // A helper always comes from the AI that is not running the reply.
  const helperKind = () => (m().vendor ? vendorName(m().vendor === 'claude' ? 'codex' : 'claude') : '');
  const agents = createMemo(() => agentSteps(tools(), helperKind()));
  const steps = createMemo(() => narration().length + own().length + (thinking().length ? 1 : 0));
  // The pages the agent opened in Faber's browser, each a card beside the files.
  const sites = createMemo(() => sitesOf(own()));
  const pageSteps = createMemo(() => stepsOf(own()));
  // Read only on a reply that says what happened while the person was away.
  const awayPages = createMemo(() => {
    const away = m().away;
    return away ? awaySites(props.messages ?? [], away.turns) : [];
  });
  const commands = createMemo(() => own().filter((tool) => /bash|command|exec|shell/i.test(tool.name)).length);
  // The answer on screen is never repeated here: once it has text, the status line steps aside.
  // The checks this reply ran, ticking up as each one ends (R161 step 9). The engine marks a check when it starts, so the
  // screen keeps no list of its own; a subagent's checks are its own facts.
  const tally = createMemo(() => {
    const ended = own().filter((tool) => tool.check && tool.ok !== undefined && !tool.declined);
    return { ran: ended.length, failed: ended.filter((tool) => tool.ok === false).length };
  });
  // The sites it opened, for research (R161 step 9); a subagent's reading stays in its own step.
  const sources = createMemo(() => sourcesOf(own()));
  const latest = createMemo(() => {
    const tool = tools().at(-1);
    if (tool && tool.ok === undefined) return toolLine(tool);
    if (finalText().trim()) return '';
    return tool ? toolLine(tool) : middle(thinking().at(-1)?.text ?? '');
  });
  // A long step says how long it has run, from when the engine saw it start (R161 step 9). Only past half a minute, so a
  // quick step never shows a number that changes before it can be read; the clock ticks only while such a step runs and
  // the window shows.
  const LONG = 30_000;
  const [clock, setClock] = createSignal(Date.now());
  // A memo, so the timer is made again only when another step starts, not on every tick of the stream.
  const runningSince = createMemo(() => {
    const tool = tools().at(-1);
    return working() && tool && tool.ok === undefined ? tool.started : undefined;
  });
  createEffect(() => {
    if (runningSince() === undefined || windowHidden()) return;
    setClock(Date.now());
    const timer = setInterval(() => setClock(Date.now()), 15_000);
    onCleanup(() => clearInterval(timer));
  });
  const longFor = () => {
    const since = runningSince();
    if (since === undefined || clock() - since < LONG) return '';
    const e = elapsed(since, clock());
    return t(e.key, { n: e.n });
  };
  // While the newest step is an edit, the lines it changed sit under the status line until the next step or the answer:
  // the view fits the work (R161 step 9). Text before the newest step's end is narration, not the answer.
  const answering = () => {
    const end = m().activityEnd;
    return Number.isInteger(end) ? !!text().slice(end).trim() : !!finalText().trim();
  };
  const changing = createMemo(() => {
    const tool = tools().at(-1);
    return working() && tool && (tool.ok === undefined || !answering()) ? changeOf(tool) : undefined;
  });
  // The header says which model answers and how hard it thinks: the vendor's mark, the model's name from the moment it is
  // known to the end and after reopening (a model the CLI never named reads as the vendor's name), and its effort when the
  // answer has one (the owner, 2026-10-17). Auto's reason sits in its tooltip; the reason shows only while Auto is still choosing and when the answer moved to the other AI. A wait for
  // quota is said once, by the card under it.
  const standIn = () => !m().model || m().model === 'codex';
  const shown = () => (m().vendor ? (standIn() ? vendorName(m().vendor!) : prettyModel(m().model)) : t('model.auto'));
  const reason = () =>
    m().reason ? fromEngine(m().reason, { when: m().waitUntil ? untilReset(m().waitUntil!) : t('quota.unknown'), from: m().movedFrom ? vendorName(m().movedFrom!) : '', to: m().vendor ? vendorName(m().vendor!) : '' }) : '';
  const showsReason = () => !!m().reason && m().reason !== 'reason.waiting' && (!m().vendor || m().reason!.startsWith('reason.moved'));
  const headerTitle = () =>
    [m().vendor ? vendorName(m().vendor!) : '', m().vendor && !standIn() ? prettyModel(m().model) : '', reason()]
      .filter(Boolean)
      .join(' · ');
  // A retry goes to the model the composer has now: the one place the model is chosen.
  const chosen = () => {
    const c = props.choice;
    if (!c || c.vendor === 'auto') return t('model.auto');
    return props.options.find((o) => o.choice.vendor === c.vendor && o.choice.model === c.model)?.label ?? prettyModel(c.model);
  };
  const retry = () => t('answer.retry', { model: chosen() });
  return (
    <article
      class="ai"
      aria-label={t('answer.label')}
      onContextMenu={(e) =>
        openMenu(e, [copyItem(m().text), ...(!working() && props.prompt ? [{ id: 'redo', label: retry(), run: () => props.onBranch(props.prompt!.id, props.prompt!.text) }] : [])])
      }
    >
      <div class="model" title={headerTitle() || undefined}>
        <span class="vm" classList={{ claude: m().vendor === 'claude', codex: m().vendor === 'codex' }}>
          <VendorMark vendor={m().vendor} />
          <b>{shown()}</b>
        </span>
        <Show when={m().vendor && m().effort}><span class="effort">· {t(`answer.effort.${m().effort!}`)}</span></Show>
        <Show when={showsReason()}><em>· {reason()}</em></Show>
      </div>
      {/* After half an hour on its own, "keep going" tells at the top of the newest reply what happened while the person was away. */}
      <Show when={m().away || m().autoAnswers?.length}>
        <div class="away-lines">
          <Show when={m().away}>{(away) => <AwayLine away={away()} />}</Show>
          <Show when={awayPages().length}>
            <ul class="away-pages" aria-label={t('away.pages')}>
              <For each={awayPages()}>{(step) => <AwayPage conversationId={props.conversationId} step={step} />}</For>
            </ul>
          </Show>
          <For each={m().autoAnswers}>
            {(a) => (
              <p class="woke">
                <Icon name="loop" small />
                {t('away.chose', { question: a.question, answer: a.answer })}
              </p>
            )}
          </For>
        </div>
      </Show>
      <Show when={m().pace}>{(pace) => <p class="pace">{t(`reason.pace.${pace().kind}`, { vendor: vendorName(pace().vendor), other: m().vendor ? vendorName(m().vendor!) : t('model.auto') })}</p>}</Show>
      <Show when={m().mode}><p class="pace">{fromEngine(m().mode)}</p></Show>
      <Show when={m().note}><p class="pace">{fromEngine(m().note)}</p></Show>
      <QuotaCard id={props.conversationId} m={m()} usage={props.usage ?? []} />
      <Show when={steps()}>
        <details class="activity disclosure">
          <summary>{t(working() ? 'activity.working' : 'activity.done', { steps: t('activity.steps', { n: steps() }), commands: t('activity.commands', { n: commands() }) })}</summary>
          <div class="activity-body">
            <Show when={thinking().length}>
              <div class="activity-thinking">
                <For each={thinking()}>{(part) => <p>{part.title ? <b>{part.text}</b> : part.text}</p>}</For>
              </div>
            </Show>
            <For each={narration()}>{(text) => <div class="md" onClick={openLinks} innerHTML={html(text)} />}</For>
            <Show when={own().length}>
              <div class="tools">
                <For each={own()}>{(tool) => <ToolChip tool={tool} />}</For>
              </div>
            </Show>
          </div>
        </details>
      </Show>
      <Show when={agents().length}>
        <div class="agents" role="list" aria-label={t('agent.label')}>
          <Index each={agents()}>{(step) => <AgentRow step={step} streaming={working()} conversationId={props.conversationId} messageId={m().id} />}</Index>
        </div>
      </Show>
      <Show when={(steps() || agents().length) && working() && latest()}>
        <div class="activity-latest" role="status" aria-live="polite" aria-atomic="true" title={latest()}>
          <span class="step">{latest()}</span>
          {/* The time is for the eye: a live region that changed every few seconds would keep talking. */}
          <Show when={longFor()}>
            <span class="long" aria-hidden="true"> · {longFor()}</span>
          </Show>
        </div>
      </Show>
      {/* One polite region for the whole reply, so a screen reader hears each check end and the first failure. */}
      <Show when={working()}>
        <div class="tally-live" role="status" aria-live="polite" aria-atomic="true">
          <Show when={tally().ran}>
            <p class="tally" classList={{ bad: tally().failed > 0 }}>
              <Show when={!tally().failed}>
                <Icon name="check" small />
              </Show>
              {t('agent.facts.checks', { n: tally().ran })} · {tally().failed ? t('agent.facts.failed', { n: tally().failed }) : t('agent.facts.passed')}
            </p>
          </Show>
        </div>
      </Show>
      <Show when={working() && sources().hosts.length}>
        <SourceList sources={sources()} />
      </Show>
      <Show when={changing()}>{(change) => <ChangedLines change={change()} />}</Show>
      <Asks conversationId={props.conversationId} turnId={m().id} permissions={waiting()} surface="ask" />
      <Parkeds parked={m().parked ?? []} more={m().parkedMore} surface="ask" />
      <div class="md" aria-busy={working()} onClick={openLinks}>
        <Markdown text={finalText()} />
        <Show when={working() && !waiting().length && !m().waitUntil}>
          <span class="cursor" aria-hidden="true" />
        </Show>
      </div>
      <Show when={m().files?.length}>
        <div class="files" aria-label={t('files.label')} ref={animateList}>
          <For each={m().files}>{(f) => <FileCard conversationId={props.conversationId} file={f} />}</For>
        </div>
      </Show>
      <Show when={sites().length}>
        <div class="files" aria-label={t('site.label')} ref={animateList}>
          <For each={sites()}>{(s) => <SiteCard conversationId={props.conversationId} tool={s.tool} site={s.site} steps={pageSteps} />}</For>
        </div>
      </Show>
      <Show when={m().checks}>{(c) => <ChecksLine c={c()} />}</Show>
      <Show when={m().follow}>{(f) => <FollowLine conversationId={props.conversationId} f={f()} />}</Show>
      <Show when={m().status === 'stopped'}>
        <div class="note">{t('answer.stopped')}</div>
      </Show>
      <Show when={m().error && !quotaPending(m())}>
        <div class="note err">{fromEngine(m().error, { when: m().waitUntil ? untilReset(m().waitUntil!) : t('quota.unknown') })}</div>
      </Show>
      <Show when={m().incomplete}><p class="note">{t('answer.meterIncomplete')}</p></Show>
      {/* "Keep going" stopped by itself after this reply: why, in one calm line (C001). */}
      <Show when={m().halt}>
        <p class="woke halt">
          <Icon name="loop" small />
          {fromEngine(m().halt, { ai: m().vendor ? vendorName(m().vendor!) : t('model.auto') })}
        </p>
      </Show>
      <Show when={!working() || foot()}>
        {/* While the agent works the row holds only the foot line; `.actions` is the finished reply's row, which screen tests wait for. */}
        <div class={working() ? 'live-foot' : 'actions'}>
          <Show when={!working()}>
          <button type="button" class="iconbtn" title={t('answer.copy')} aria-label={t('answer.copy')} onClick={() => void navigator.clipboard.writeText(m().text)}>
            <Icon name="copy" small />
          </button>
          <Show when={props.prompt} keyed>
            {(prompt) => (
              <button type="button" class="iconbtn" title={retry()} aria-label={retry()} onClick={() => props.onBranch(prompt.id, prompt.text)}>
                <Icon name="retry" small />
              </button>
            )}
          </Show>
          <button type="button" class="iconbtn" title={t('answer.terminalHint')} aria-label={t('answer.terminal')} onClick={() => void api.openTerminal(props.conversationId)}>
            <Icon name="terminal" small />
          </button>
          <Show when={props.prompt && m().nudge} keyed>
            {(nudge) => (
              <button
                type="button"
                class="nudge"
                title={fromEngine(nudge.reason, { to: `${prettyModel(nudge.model)} · ${t(`effort.${nudge.effort}`)}` })}
                onClick={() => props.onBranch(props.prompt!.id, props.prompt!.text, rung(nudge, props.options))}
              >
                {t('answer.nudge')}
              </button>
            )}
          </Show>
          </Show>
          <Show when={foot()}>{(line) => <span class="foot" title={line()}>{line()}</span>}</Show>
          <Show when={!working() && costOf(m())}>{(cost) => <span class="cost num" title={costTitle(m(), warm())}>{cost()}</span>}</Show>
        </div>
      </Show>
    </article>
  );
}

// Faber's own checks of a reply that changed files (R146): one quiet line while they run and once they ended, the failed
// checks' first failures behind a disclosure. Checks that still fail after Faber's fixes wait for the person, in amber.
function ChecksLine(props: { c: Checks }) {
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
                <pre>{(f.lines ?? []).join('')}</pre>
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
function FollowLine(props: { conversationId: string; f: Follow }) {
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

// A message Faber sent by itself (R200): what it is about, as one quiet line; the words the agent got are its tooltip.
// The user's message; "Edit" sends the new text as a branch, keeping the original conversation as it was.
function Mine(props: { editKey: string; m: Accessor<Message>; onBranch: OnBranch }) {
  // One editor lives and dies with its (conversation, message) key (the parent re-creates it when the key changes), so
  // what it writes can never land on another message's saved edit.
  const key = () => props.editKey;
  onCleanup(() => release(key()));
  const original = props.m().text;
  const [editing, setEditing] = createSignal<string | undefined>(kept(key()) || undefined);
  useDraft(() => editing() !== undefined);
  // Only text that differs from the message is a draft.
  createEffect(() => keep(key(), editing() === original ? '' : (editing() ?? '')));
  return (
    <Show
      when={editing() !== undefined}
      fallback={
        <div
          class="mine"
          onContextMenu={(e) => openMenu(e, [copyItem(props.m().text), { id: 'edit', label: t('mine.edit'), run: () => setEditing(props.m().text) }])}
        >
          <Show when={props.m().attachments?.length}>
            <div class="attach">
              <For each={props.m().attachments}>
                {(name) => (
                  <span class="afile">
                    <span class="fi">
                      <Icon name="sheet" />
                    </span>
                    <span>{name}</span>
                  </span>
                )}
              </For>
            </div>
          </Show>
          <div class="you">
            <For each={props.m().images}>{(img) => <img alt={t('mine.image')} src={`data:${img.mediaType};base64,${img.base64}`} />}</For>
            {props.m().text}
            <button class="edit" onClick={() => setEditing(props.m().text)} aria-label={t('mine.editLabel')}>
              {t('mine.edit')}
            </button>
          </div>
        </div>
      }
    >
      <form
        class="you editing"
        onSubmit={(e) => {
          e.preventDefault();
          const text = editing() ?? '';
          if (text.trim()) {
            props.onBranch(props.m().id, text);
            keep(key(), '');
          }
        }}
      >
        <textarea
          aria-label={t('mine.editLabel')}
          value={editing()}
          rows={Math.min(10, (editing() ?? '').split('\n').length + 1)}
          onInput={(e) => setEditing(e.currentTarget.value)}
          ref={(el) => queueMicrotask(() => el.focus())}
        />
        <div class="actions">
          <button type="submit">{t('mine.sendBranch')}</button>
          <button type="button" onClick={() => setEditing(undefined)}>
            {t('mine.cancel')}
          </button>
        </div>
      </form>
    </Show>
  );
}

// A message Faber sent by itself ("keep going"): one quiet line where the person's bubble would be, since the words are
// not theirs. What the agent was told stays in the tooltip.
function Woke(props: { m: Accessor<Message> }) {
  const wake = () => props.m().wake!;
  return (
    <div class="mine">
      <p class="woke" title={props.m().text}>
        <Icon name="loop" small />
        {fromEngine(wake().why, { pr: wake().pr ? `#${wake().pr}` : '', checks: (wake().checks ?? []).join(', ') })}
      </p>
    </div>
  );
}

// A message written while an answer was running. The person can take it back until it is sent; one already handed to the
// running answer (`sending`) cannot be, and one that waits for the next turn can be sent by hand once nothing runs.
function Queued(props: { conversationId: string; q: QueuedTurn; busy: boolean; onError?: (err: unknown) => void }) {
  return (
    <div class="mine queued">
      <Show when={props.q.attachments?.length}>
        <div class="attach">
          <For each={props.q.attachments}>
            {(name) => (
              <span class="afile">
                <span class="fi">
                  <Icon name="sheet" />
                </span>
                <span>{name}</span>
              </span>
            )}
          </For>
        </div>
      </Show>
      <div class="you queued">
        <span class="queue-badge">{t('queue.label')}</span>
        <For each={props.q.images}>{(img) => <img alt={t('mine.image')} src={`data:${img.mediaType};base64,${img.base64}`} />}</For>
        <Show when={props.q.text}>
          <div class="queue-text">{props.q.text}</div>
        </Show>
        <div class="actions">
          <Show when={!props.q.sending}>
            <button type="button" onClick={() => api.removeQueued(props.conversationId, props.q.id).catch((err) => props.onError?.(err))} aria-label={t('queue.removeLabel')}>
              {t('queue.remove')}
            </button>
          </Show>
          <Show when={!props.busy}>
            <button type="button" onClick={() => api.sendQueued(props.conversationId, props.q.id).catch((err) => props.onError?.(err))} aria-label={t('queue.sendNowLabel')}>
              {t('queue.sendNow')}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

// Messages only grow at the end, so each position keeps its component while the answer streams into it.
// Only the messages from `from` on are drawn.
/** What "keep going" did while the person was away, in one calm line (C001): how long, turns of work, and only the parts that happened. */
function AwayLine(props: { away: NonNullable<Message['away']> }) {
  const time = () => {
    const e = elapsed(0, props.away.minutes * 60_000);
    return t(e.key, { n: e.n });
  };
  const parts = () =>
    [
      props.away.turns ? t('away.turns', { n: props.away.turns }) : '',
      props.away.switches ? t('away.switches', { n: props.away.switches }) : '',
      props.away.answered ? t('away.answered', { n: props.away.answered }) : '',
    ].filter(Boolean);
  return (
    <p class="woke away">
      <Icon name="loop" small />
      {parts().length ? t('away.summary', { title: t('away.title', { time: time() }), parts: parts().join(', ') }) : t('away.title', { time: time() })}
    </p>
  );
}

export function Thread(props: Shared & { messages: Message[]; queued?: QueuedTurn[]; busy?: boolean; onError?: (err: unknown) => void; from: number }) {
  const prompt = (at: number) => (props.messages[at - 1]?.role === 'user' ? props.messages[at - 1] : undefined);
  return (
    <>
      <Index each={props.messages.slice(props.from)}>
        {(m, i) => (
          <Show when={m().role === 'user'} fallback={<Answer {...props} m={m} prompt={prompt(props.from + i)} last={props.from + i === props.messages.length - 1} />}>
            <Show when={!m().wake} fallback={<Woke m={m} />}>
              <Show when={`edit.${props.conversationId}.${m().id}`} keyed>
                {(editKey) => <Mine editKey={editKey} m={m} onBranch={props.onBranch} />}
              </Show>
            </Show>
          </Show>
        )}
      </Index>
      <For each={props.queued}>{(q) => <Queued conversationId={props.conversationId} q={q} busy={props.busy ?? false} onError={props.onError} />}</For>
    </>
  );
}
