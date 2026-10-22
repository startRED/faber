import { createEffect, createMemo, createResource, createSignal, For, Index, on, onCleanup, Show, type Accessor } from 'solid-js';
import {
  api,
  errorText,
  type Choice,
  type Message,
  type Plan,
  type QueuedTurn,
  type RewindRedoInfo,
  type ToolUse,
  type VendorUsage,
  type VersionAt,
  type Versions,
} from './api.ts';
import { Asks, Parkeds } from './Ask.tsx';
import { copyItem, openMenu } from './contextmenu.ts';
import { keep, kept, recoverQueuedDraft, release, useDraft } from './unsent.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { QuotaCard } from './QuotaCard.tsx';
import { ChecksLine, DetailsLine, FollowLine } from './Details.tsx';
import { PlanList, WrapUpBox } from './Plan.tsx';
import { Rewind } from './Rewind.tsx';
import { AnswersCard, answeredIn, folded, RunBlock, runsOf, type Answered } from './Run.tsx';
import { html, openLinks, pressImage } from './markdown.tsx';
import { PieceReply } from './VisualPiece.tsx';
import { VisualFiles } from './VisualAnswer.tsx';
import type { VisualAnswer } from './api.ts';
import { agentMeta, ChangedLines, openedAgent, SourceList, showAgent, startMessage, tellAbout, ToolChip } from './AgentChat.tsx';
import { untilReset } from './Sidebar.tsx';
import { appsOf, AwayPage, awaySites, SiteCard, sitesOf, stepsOf } from './Site.tsx';
import { Quote } from './Floating.tsx';
import { Strip, Thumb } from './Thumbs.tsx';
import { canSpeak, speaking, stopSpeech, toggleSpeech } from './speech.ts';
import { catalogName, bytes, compact, duration, failureText, fromEngine, lang, money, t } from './i18n.ts';
import { prettyModel, vendorName, type Option } from './models.ts';
import { animateList, keepForExit } from './motion.ts';
import { windowHidden } from './appearance.ts';
import { elapsed, warmLeft } from './running.ts';
import { agePending, ageNow, footText, tickWhile } from './foot.ts';
import { quotaPending } from './QuotaCard.tsx';
import { agentState, agentSteps, changeOf, groupLabel, groupSteps, isAgentCall, isProgressCall, middle, sameTools, sourcesOf, stepLine, thinkingParts, type AgentStep, type StepGroup } from './worklog.ts';

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

// A file the answer created in the chat workspace: images show as a small picture that opens full screen; anything can be saved to
// the computer.
function FileCard(props: { conversationId: string; file: { path: string; size: number } }) {
  const [preview] = createResource(
    () => (IMAGE.test(props.file.path) ? props.file.path : undefined),
    (path) => api.readImage(props.conversationId, path),
  );
  return (
    <div class="file">
      <Show when={preview()}>{(src) => <Thumb name={props.file.path} src={src()} />}</Show>
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
// A turn whose meters came back partly empty shows "≈" before the number; the reason is in the tooltip, never a line of its own.
// With no number at all, the mark stays alone, so the reason is still one hover away.
const costOf = (m: Message) => (m.costUsd ? `${m.incomplete ? '≈' : ''}${money(m.costUsd, m.costUsd < 0.01 ? 4 : 2)}` : m.incomplete ? '≈' : '');
function costTitle(m: Message, warm: string): string {
  const parts = m.costUsd ? [t('answer.cost', { cost: money(m.costUsd, 4) })] : [];
  if (m.inputTokens) parts.push(t('answer.tokens', { input: compact(m.inputTokens), output: compact(m.outputTokens ?? 0) }));
  if (warm) parts.push(warm);
  if (m.incomplete) parts.push(t('answer.meterIncomplete'));
  return parts.join(' · ');
}

// Copy under a message. The icon turns into a check for a moment once the words are on the clipboard: one timer, started by the click.
function CopyButton(props: { text: () => string }) {
  const [copied, setCopied] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  const copy = () =>
    void Promise.resolve(navigator.clipboard.writeText(props.text())).then(
      () => {
        setCopied(true);
        clearTimeout(timer);
        timer = setTimeout(() => setCopied(false), 1600);
      },
      () => {},
    );
  const label = () => t(copied() ? 'answer.copied' : 'answer.copy');
  return (
    <button type="button" class="iconbtn" title={label()} aria-label={label()} onClick={copy}>
      <span class="swap" classList={{ done: copied() }}>
        <Icon name="copy" small />
        <Icon name="check" small />
      </span>
    </button>
  );
}

// When a message was sent (the short time; the whole date is the tooltip) and, on an answer, how long it took. Old replies carry
// no duration, and then the time stands alone.
function When(props: { at: number; took?: number }) {
  const took = () => (props.took && props.took >= 500 ? duration(Math.max(1, Math.round(props.took / 1000))) : '');
  const date = () => new Date(props.at).toLocaleString(lang(), { dateStyle: 'full', timeStyle: 'medium' });
  return (
    <Show when={props.at > 0}>
      <span class="when num" title={took() ? `${date()} · ${t('answer.took', { time: took() })}` : date()}>
        <time datetime={new Date(props.at).toISOString()}>{new Date(props.at).toLocaleTimeString(lang(), { hour: 'numeric', minute: '2-digit' })}</time>
        <Show when={took()}>
          <span class="took"> · {took()}</span>
        </Show>
      </span>
    </Show>
  );
}

// Keep going's stop line (engine STOP) is a signal for Faber, never words: the engine takes it out when the turn ends, and until
// then it is blanked where the engine would cut it (a line of its own with any marks around it, the end of the text, and while
// the reply streams a last line that is still arriving as one). Spaces of the same length keep every place the engine marked in
// the text (activity_end, text_breaks) where it was.
const STOP = '[keep going: stop]';
const STOP_LINE = /^[ \t\x60*_"'.]*\[keep going: stop\][ \t\x60*_"'.]*\r?$/gim;
const STOP_END = /\[keep going: stop\][\s\x60*_"'.]*$/i;
const blank = (s: string) => ' '.repeat(s.length);
function hideStop(text: string, streaming: boolean): string {
  if (!text.includes('[')) return text;
  const out = text.replace(STOP_LINE, blank).replace(STOP_END, blank);
  if (!streaming) return out;
  const last = out.slice(out.lastIndexOf('\n') + 1);
  const bare = last.trim().replace(/^[\x60*_"'.]+/, '').trim().toLowerCase();
  return bare.length > 1 && STOP.startsWith(bare) ? out.slice(0, out.length - last.length) + blank(last) : out;
}

// The picker names Claude models by alias ("opus"), so the full id of the next rung is matched to its entry.
/** Look around: starts the box with the request (and the places Faber found, when it found some), a blank line apart
 * from a draft already there, and only once. */
function lookAround(look: NonNullable<Message['look']>) {
  const places = look.places ?? [];
  const ask = places.length ? `${t('look.ask')} ${t('look.askPlaces', { places: places.join(', ') })}` : t('look.ask');
  const draft = document.querySelector<HTMLTextAreaElement>('.composer textarea')?.value ?? '';
  startMessage(draft.startsWith(ask) || !draft.trim() ? ask : `${ask}\n\n`);
}

/** Why "Look around" is offered and what it does, with how many places Faber found. */
function lookTitle(look: NonNullable<Message['look']>): string {
  const n = look.places?.length ?? 0;
  const why = fromEngine(look.reason, { n });
  return look.reason === 'look.fixed' && n ? `${why} ${t('look.found', { n })}` : why;
}

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

// One step of the work log, or a run of the same kind as one phrase with a count ("Changing 3 files") that opens into its steps.
// A failed step inside says so beside the phrase, since the phrase itself cannot be struck through.
function StepRow(props: { group: Accessor<StepGroup> }) {
  const group = () => props.group();
  const failed = () => group().tools.filter((tool) => tool.ok === false && !tool.declined).length;
  return (
    <Show when={group().kind} fallback={<ToolChip tool={group().tools[0]!} />}>
      <details class="step-group">
        <summary>
          <b>{groupLabel(group())}</b>
          <Show when={failed()}><span class="sr-only">, </span><em>{t('agent.facts.failed', { n: failed() })}</em></Show>
        </summary>
        <div class="tools">
          <For each={group().tools}>{(tool) => <ToolChip tool={tool} />}</For>
        </div>
      </details>
    </Show>
  );
}

type OnBranch =(messageId: string, text: string, choice?: Choice) => void;

interface Shared {
  pieceRevision?: number;
  visuals?: VisualAnswer[];
  conversationId: string;
  area?: 'chat' | 'project';
  options: Option[];
  /** What the composer has chosen now: the model a retry goes to. */
  choice?: Choice;
  onBranch: OnBranch;
  /** "Branch from here" on an answer: a new conversation with every message up to it. */
  onBranchHere?: (messageId: string) => void;
  /** The messages with other versions, and opening one (the message keeps its place there). */
  versions?: Versions[];
  onVersion?: (to: VersionAt, messageIndex: number, by: number) => void;
  usage?: VendorUsage[];
  /** Faber asks git whether the last reply changed files (the running row's `weighing`). */
  weighing?: boolean;
  background?: import('./api.ts').BackgroundCommand[];
  /** The agent's plan for the request, drawn under the newest reply (task 017). */
  plan?: Plan;
  /** A button of a closing box: its words go as the person's message. */
  onReply?: (text: string) => void;
}

function Answer(props: Shared & { m: Accessor<Message>; messages?: Message[]; prompt?: Message; last?: boolean; answered?: Answered[]; messageIndex?: number }) {
  // A turn Faber started by itself: its message takes no row (Faber's own plumbing never does, the 2026-10-19 design), so the
  // reply's header carries it as a small loop mark whose tooltip says what Faber asked.
  const woke = () => props.prompt?.wake;
  const m = () => props.m();
  // While an answer streams the engine sends the whole message every 33 ms. Each memo below compares by value, so a tick that
  // changed only the tools never touches the text, and one that changed only the text never touches the tools.
  const text = createMemo(() => hideStop(m().text, m().status === 'streaming'));
  // An answer read aloud stops with its message: when the person opens another conversation, the voice does not go on.
  createEffect(() => {
    const id = m().id;
    onCleanup(() => stopSpeech(id));
  });
  const tools = createMemo(() => (m().tools ?? []).filter((tool) => !isProgressCall(tool)), [], { equals: sameTools });
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
  // The caret means words are arriving. It shows before the first word, and after that only while the text grew in the last
  // moment: never while the agent runs a step or thinks after writing (a reply on 2026-10-19 blinked for nine minutes while a
  // command waited, and looked finished and stuck at once). One timer, reset by each change of the text.
  const TYPING = 1_500;
  const [typing, setTyping] = createSignal(false);
  let typingTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(
      text,
      () => {
        clearTimeout(typingTimer);
        if (!working()) return setTyping(false);
        setTyping(true);
        typingTimer = setTimeout(() => setTyping(false), TYPING);
      },
    ),
  );
  onCleanup(() => clearTimeout(typingTimer));
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
    // Visual ranges refer to the saved text: keep its spacing intact inside the work log too.
    const hasVisual = text().slice(0, boundary()).includes('faber-piece') || props.visuals?.some((a) => a.versions.some((v) => v.messageId === m().id && v.end <= boundary()));
    const prefix = text().slice(0, boundary()).trimEnd();
    const paragraphs = prefix.split(/\n{2,}/);
    const last = paragraphs.at(-1)?.trim();
    const answer = finalText().trimStart();
    const duplicate = last && answer.startsWith(last) && (answer.length === last.length || /\s/.test(answer[last.length]!));
    if (duplicate) paragraphs.pop();
    const rest = hasVisual ? (duplicate ? prefix.slice(0, prefix.lastIndexOf(last!)).trimEnd() : prefix) : paragraphs.join('\n\n').trim();
    return rest ? [rest] : [];
  });
  const thinkingText = createMemo(() => m().thinking ?? '');
  const thinking = createMemo(() => thinkingParts(thinkingText()));
  // The agent's own tools; each subagent and what it ran show as their own step below (AgentRow).
  const own = createMemo(() => tools().filter((tool) => !tool.parent && !isAgentCall(tool)), [], { equals: sameTools });
  // A helper always comes from the AI that is not running the reply.
  const helperKind = () => (m().vendor ? vendorName(m().vendor === 'claude' ? 'codex' : 'claude') : '');
  const agents = createMemo(() => agentSteps(tools(), helperKind()));
  const groups = createMemo(() => groupSteps(own()));
  // The living sentence reads a run of one kind as its phrase and the newest step ("Reading 3 files · a.ts").
  const lineOf = (tool: ToolUse) => stepLine(groups(), tool);
  const steps = createMemo(() => narration().length + own().length + (thinking().length ? 1 : 0));
  const hasKit = () => !!(m().toolKit?.ids.length || m().toolKit?.suggested || m().toolKit?.error);
  const kitLine = () => {
    const kit = m().toolKit;
    if (!kit) return '';
    if (kit.error) return fromEngine(kit.error);
    const reason = t(kit.method === 'catalog' ? 'kit.catalog' : 'kit.code');
    const line = t('kit.line', { tools: kit.ids.map(catalogName).join(', ') || t('kit.none'), reason });
    if (!kit.suggested) return line;
    return `${line} ${t('kit.suggest', { tool: catalogName(kit.suggested) })}`;
  };
  // The pages the agent opened in Faber's browser and the apps it started on this computer, each a card beside the files.
  const sites = createMemo(() => [...sitesOf(own()), ...appsOf(own())]);
  const pageSteps = createMemo(() => stepsOf(own()));
  // The questions Faber answered for the person, as one card (task 018): a run's on its newest reply.
  const answered = createMemo(() => props.answered ?? answeredIn([m()]));
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
  // What the agent said it is doing now (task 017) stands in for the newest step's words while it works.
  const nowLine = () => (working() ? m().now : undefined);
  const latest = createMemo(() => {
    const now = nowLine();
    if (now && !finalText().trim()) return now;
    const tool = tools().at(-1);
    if (tool && tool.ok === undefined) return lineOf(tool);
    if (finalText().trim()) return '';
    return tool ? lineOf(tool) : middle(thinking().at(-1)?.text ?? '');
  });
  // A long step says how long it has run, from when the engine saw it start (R161 step 9). Only past half a minute, so a
  // quick step never shows a number that changes before it can be read; the clock ticks only while such a step runs and
  // the window shows.
  const LONG = 30_000;
  // The newest of the agent's own steps still open, wherever it is in the list: a later one may already have ended.
  const runningStep = createMemo(() => (working() ? tools().findLast((tool) => !tool.parent && tool.ok === undefined) : undefined));
  // Under the words: the step it is on, in the status line's words, or that it is thinking (only once words stop). Never
  // while a question waits on the person.
  const after = () => {
    if (!working() || !finalText().trim() || waiting().length || m().waitUntil) return '';
    // A running step shows at once (the caret never blinks over one); "Thinking…" waits until the words stop.
    const tool = runningStep();
    if (tool) return nowLine() || lineOf(tool);
    return typing() ? '' : nowLine() || t('activity.thinkingNow');
  };
  const [clock, setClock] = createSignal(Date.now());
  // A memo, so the timer is made again only when another step starts, not on every tick of the stream.
  const runningSince = () => runningStep()?.started;
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
  const wokeWhy = () => {
    const wake = woke();
    return wake ? fromEngine(wake.why, { pr: wake.pr ? `#${wake.pr}` : '', checks: (wake.checks ?? []).join(', ') }) : '';
  };
  // What Faber did around this turn by itself: the message it sent, a history it sent again. A tooltip, never a row.
  const byFaber = () => [wokeWhy(), m().note ? fromEngine(m().note) : ''].filter(Boolean).join(' · ');
  const headerTitle = () =>
    [m().vendor ? vendorName(m().vendor!) : '', m().vendor && !standIn() ? prettyModel(m().model) : '', reason(), byFaber()]
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
      data-message-index={props.messageIndex}
      tabindex="-1"
      aria-label={t('answer.label')}
      onContextMenu={(e) =>
        openMenu(e, [
          copyItem(m().text),
          ...(!working() && props.prompt ? [{ id: 'redo', label: retry(), run: () => props.onBranch(props.prompt!.id, props.prompt!.text) }] : []),
          ...(!working() && props.onBranchHere ? [{ id: 'branch', label: t('answer.branchHere'), run: () => props.onBranchHere!(m().id) }] : []),
        ])
      }
    >
      <div class="model" title={headerTitle() || undefined}>
        <span class="vm" classList={{ claude: m().vendor === 'claude', codex: m().vendor === 'codex' }}>
          <VendorMark vendor={m().vendor} />
          <b>{shown()}</b>
        </span>
        <Show when={m().vendor && m().effort}><span class="effort">· {t(`answer.effort.${m().effort!}`)}</span></Show>
        <Show when={showsReason()}><em>· {reason()}</em></Show>
        <Show when={byFaber()}>
          <span class="by-faber" role="img" aria-label={byFaber()}>
            <Icon name="loop" small />
          </span>
        </Show>
      </div>
      {/* After half an hour on its own, "keep going" tells at the top of the newest reply what happened while the person was away. */}
      <Show when={m().away || answered().length}>
        <div class="away-lines">
          <Show when={m().away}>{(away) => <AwayLine away={away()} />}</Show>
          <Show when={awayPages().length}>
            <ul class="away-pages" aria-label={t('away.pages')}>
              <For each={awayPages()}>{(step) => <AwayPage conversationId={props.conversationId} step={step} />}</For>
            </ul>
          </Show>
          <Show when={answered().length}>
            <AnswersCard answers={answered()} onReply={props.onReply} />
          </Show>
        </div>
      </Show>
      <Show when={m().pace}>{(pace) => <p class="pace">{t(`reason.pace.${pace().kind}`, { vendor: vendorName(pace().vendor), other: m().vendor ? vendorName(m().vendor!) : t('model.auto') })}</p>}</Show>
      <Show when={m().mode}><p class="pace">{fromEngine(m().mode)}</p></Show>
      <QuotaCard id={props.conversationId} m={m()} usage={props.usage ?? []} />
      <Show when={steps() || hasKit()}>
        <details class="activity disclosure">
          <summary>{steps() ? t(working() ? 'activity.working' : 'activity.done', { steps: t('activity.steps', { n: steps() }), commands: t('activity.commands', { n: commands() }) }) : t('kit.details')}</summary>
          <div class="activity-body">
            <Show when={hasKit()}><p class="pace kit">{kitLine()}</p></Show>
            <Show when={thinking().length}>
              <div class="activity-thinking">
                <For each={thinking()}>{(part) => <p>{part.title ? <b>{part.text}</b> : part.text}</p>}</For>
              </div>
            </Show>
            <For each={narration()}>{(text) => <Narration text={text} conversationId={props.conversationId} messageId={m().id} visuals={props.visuals} pieceRevision={props.pieceRevision} />}</For>
            <Show when={own().length}>
              <div class="tools">
                <Index each={groups()}>{(group) => <StepRow group={group} />}</Index>
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
      <Show when={(steps() || agents().length) && working() && latest() && !finalText().trim()}>
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
            <p class="tally" classList={{ bad: tally().failed > 0, project: props.area === 'project' }}>
              <Show when={!tally().failed}>
                <Icon name="check" small />
              </Show>
              {t(props.area === 'project' ? 'details.checked' : 'agent.facts.checks', { n: tally().ran })} · {tally().failed ? t(props.area === 'project' ? 'details.checksFailed' : 'agent.facts.failed', { n: tally().failed }) : t('agent.facts.passed')}
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
      <div class="md" aria-busy={working()} onClick={openLinks} onKeyDown={pressImage}>
        <PieceReply text={finalText()} offset={boundary()} messageId={m().id} conversationId={props.conversationId} answers={props.visuals ?? []} revision={props.pieceRevision} />
        <VisualFiles answers={props.visuals ?? []} messageId={m().id} conversationId={props.conversationId} />
        <Show when={working() && !waiting().length && !m().waitUntil && !runningStep() && (typing() || !finalText().trim())}>
          <span class="cursor" aria-hidden="true" />
        </Show>
      </div>
      {/* Words on screen and the agent still at work: the step it is on sits right under them, where the eye is. */}
      <Show when={after()}>
        <div class="activity-latest after" role="status" aria-live="polite" aria-atomic="true">
          <span class="step">{after()}</span>
          <Show when={longFor()}>
            <span class="long" aria-hidden="true"> · {longFor()}</span>
          </Show>
        </div>
      </Show>
      {/* The plan sits under the newest reply while the work goes on, and until every step is done. */}
      <Show when={props.last && props.plan?.steps.length && (working() || props.plan.steps.some((s) => s.state !== 'done')) ? props.plan : undefined}>
        {(plan) => <PlanList plan={plan()} />}
      </Show>
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
      <Show when={props.area !== 'project' && m().checks}>{(c) => <ChecksLine c={c()} />}</Show>
      {/* The moment git is asked whether the reply changed files, before checks would start: one quiet line, Stop ends it. */}
      <Show when={props.area !== 'project' && props.last && props.weighing && !m().checks}>
        <div class="pace checks" data-state="weighing">
          <p>{t('checks.weighing')}</p>
        </div>
      </Show>
      <Show when={props.area !== 'project' && m().follow}>{(f) => <FollowLine conversationId={props.conversationId} f={f()} />}</Show>
      {/* Only the person's own Stop is said: a turn Faber started and cut needs no word, and one that ended on a reason says that. */}
      <Show when={m().status === 'stopped' && !m().error?.startsWith('reason.') && props.prompt && !props.prompt.wake}>
        <div class="note">{t('answer.stopped')}</div>
      </Show>
      <Show when={m().error && !quotaPending(m())}>
        {/* A limit is told calmly (C001); a failure keeps the red bar. */}
        <div class="note" classList={{ err: m().error !== 'reason.turnLimit' && m().error !== 'reason.budget' }}>{fromEngine(m().error, { when: m().waitUntil ? untilReset(m().waitUntil!) : t('quota.unknown') })}</div>
      </Show>
      {/* "Keep going" stopped by itself after this reply: why, in one calm line (C001). */}
      <Show when={m().halt}>
        <p class="woke halt">
          <Icon name="loop" small />
          {fromEngine(m().halt, { ai: m().vendor ? vendorName(m().vendor!) : t('model.auto') })}
        </p>
      </Show>
      <Show when={m().wrapUp}>{(box) => <WrapUpBox box={box()} open={!!props.last && !working() && !m().permissions?.some((p) => p.tool === 'ExitPlanMode')} onReply={props.onReply} />}</Show>
      <Show when={!working() || foot()}>
        {/* While the agent works the row holds only the foot line; `.actions` is the finished reply's row, which screen tests wait for.
            Its buttons, time and cost show on hover or focus (the hover bar); the foot line, which says the reply is done, stays. */}
        <div class={working() ? 'live-foot' : 'actions'}>
          <Show when={!working()}>
          <CopyButton text={() => m().text} />
          <Show when={canSpeak() && finalText().trim()}>
            <button
              type="button"
              class="iconbtn"
              aria-pressed={speaking() === m().id}
              title={t(speaking() === m().id ? 'answer.listenStop' : 'answer.listen')}
              aria-label={t(speaking() === m().id ? 'answer.listenStop' : 'answer.listen')}
              onClick={() => toggleSpeech(m().id, finalText())}
            >
              <span class="swap" classList={{ done: speaking() === m().id }}>
                <Icon name="speaker" small />
                <Icon name="stop" small />
              </span>
            </button>
          </Show>
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
          {/* Look around (R369): a reply that fixed something offers to find the same mistake elsewhere. It only starts the box
              with the request, as "Continue with a helper" does, and never shows beside a step up or a closing question. */}
          <Show when={props.last && props.onReply && !m().nudge && m().look} keyed>
            {(look) => (
              <button type="button" class="nudge" title={lookTitle(look)} onClick={() => lookAround(look)}>
                {t('answer.look')}
              </button>
            )}
          </Show>
          </Show>
          <Show when={foot()}>{(line) => <span class="foot" title={line()}>{line()}</span>}</Show>
          <Show when={!working()}>
            <When at={m().at} took={m().took} />
            <Show when={costOf(m())}>{(cost) => <span class="cost num" title={costTitle(m(), warm())}>{cost()}</span>}</Show>
          </Show>
        </div>
      </Show>
    </article>
  );
}

// A message Faber sent by itself (R200): what it is about, as one quiet line; the words the agent got are its tooltip.
// The user's message; "Edit" sends the new text as a branch, keeping the original conversation as it was.
/** The arrows between the versions of a message (parity T7); each version is a conversation of its own, opened at the same
 * place. The left and right arrow keys walk them too while focus is on them. */
function VersionArrows(props: { v: Versions; onGo: (to: VersionAt, by: number) => void }) {
  const count = () => props.v.list.length;
  const step = (by: number) => {
    const to = props.v.list[props.v.at + by];
    if (to) props.onGo(to, by);
  };
  const key = (e: KeyboardEvent) => {
    const by = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0;
    if (!by || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    step(by);
  };
  return (
    <div class="versions" role="group" aria-label={t('mine.versionOf', { at: props.v.at + 1, count: count() })} onKeyDown={key}>
      <button type="button" class="iconbtn" title={t('mine.versionBack')} aria-label={t('mine.versionBack')} disabled={props.v.at === 0} onClick={() => step(-1)}>
        <Icon name="left" small />
      </button>
      <span aria-hidden="true">
        {props.v.at + 1}/{count()}
      </span>
      <button
        type="button"
        class="iconbtn"
        title={t('mine.versionNext')}
        aria-label={t('mine.versionNext')}
        disabled={props.v.at === count() - 1}
        onClick={() => step(1)}
      >
        <Icon name="right" small />
      </button>
    </div>
  );
}

function Mine(props: {
  editKey: string;
  m: Accessor<Message>;
  onBranch: OnBranch;
  messageIndex: number;
  onRewind: () => void;
  onRedo?: () => void;
  recovery?: boolean;
  versions?: Versions;
  onVersion?: (to: VersionAt, messageIndex: number, by: number) => void;
}) {
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
          data-message-index={props.messageIndex}
          tabindex="0"
          onContextMenu={(e) => openMenu(e, [copyItem(props.m().text), { id: 'edit', label: t('mine.edit'), run: () => setEditing(props.m().text) }, ...(props.onRedo ? [{ id: 'rewind-redo', label: t(props.recovery ? 'rewind.recover' : 'rewind.redo'), run: props.onRedo }] : []), { id: 'rewind', label: t('rewind.title'), danger: true, run: props.onRewind }], e.currentTarget)}
        >
          <Strip images={props.m().images} files={props.m().attachments} />
          <Show when={props.m().text}>
            <div class="you">{props.m().text}</div>
          </Show>
          <div class="bar mine-bar">
            <When at={props.m().at} />
            <CopyButton text={() => props.m().text} />
            <button type="button" class="iconbtn edit" onClick={() => setEditing(props.m().text)} title={t('mine.editLabel')} aria-label={t('mine.editLabel')}>
              <Icon name="edit" small />
            </button>
          </div>
          <Show when={props.m().rewind?.coverage.incomplete || props.m().rewind?.coverage.unavailable}>
            <p class="side-meta rewind-coverage">{t('rewind.coverage')}</p>
          </Show>
          <Show when={props.versions}>{(v) => <VersionArrows v={v()} onGo={(to, by) => props.onVersion?.(to, props.messageIndex, by)} />}</Show>
        </div>
      }
    >
      <form
        class="you editing"
        data-message-index={props.messageIndex}
        tabindex="-1"
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

/** A reply with nothing for the person: no words, no steps, nothing waiting on them. One that answered a message Faber sent by
 * itself is not drawn at all. */
const empty = (m: Message) =>
  !m.text.trim() && !m.tools?.length && !m.thinking && !m.error && !m.checks && !m.follow && !m.halt && !m.permissions?.length && !m.parked?.length && !m.files?.length;

// A message written while an answer was running. The person can take it back until it is sent; one already handed to the
// running answer (`sending`) cannot be, and one that waits for the next turn can be sent by hand once nothing runs.
function Queued(props: { conversationId: string; q: QueuedTurn; busy: boolean; onRecovered?: (id: string, text: string) => void; onError?: (err: unknown) => void }) {
  const editKey = `queued-edit.${props.conversationId}.${props.q.id}`;
  const [editing, setEditing] = createSignal<string | undefined>(kept(editKey) || undefined);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal('');
  let editButton: HTMLButtonElement | undefined;
  let disposed = false;
  onCleanup(() => { disposed = true; release(editKey); });
  useDraft(() => editing() !== undefined);
  const write = (text: string) => {
    setEditing(text);
    keep(editKey, text);
  };
  const restoreFocus = () => queueMicrotask(() => {
    const target = editButton?.isConnected && !editButton.disabled ? editButton
      : document.querySelector<HTMLElement>('.composer textarea:not(:disabled)') ?? document.querySelector<HTMLElement>('.run-btn');
    target?.focus();
  });
  const cancel = () => {
    if (saving()) return;
    setEditing(undefined);
    keep(editKey, '');
    setError('');
    restoreFocus();
  };
  const save = async () => {
    const text = editing();
    if (text === undefined || saving()) return;
    if (!text.trim()) {
      setError(t('error.queueEmpty'));
      return;
    }
    setSaving(true);
    setError('');
    try {
      await api.editQueued(props.conversationId, props.q.id, text);
      // A different mounted editor may now own newer words under this key.
      if (disposed) return;
      setEditing(undefined);
      keep(editKey, '');
      restoreFocus();
    } catch (err) {
      if (disposed) return;
      if (errorText(err) === 'error.queueChanged') {
        const restored = recoverQueuedDraft(props.conversationId, props.q.id);
        setEditing(undefined);
        if (restored !== undefined) props.onRecovered?.(props.conversationId, restored);
      } else setError(failureText(err));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div class="mine queued">
      <Strip images={props.q.images} files={props.q.attachments} />
      <div class="you queued" classList={{ editing: editing() !== undefined }}>
        <span class="queue-badge">{t('queue.label')}</span>
        <Show
          when={editing() !== undefined}
          fallback={
            <>
              <Show when={props.q.text}>
                <div class="queue-text">{props.q.text}</div>
              </Show>
              <div class="actions">
                <Show when={!props.q.sending}>
                  <button type="button" ref={(el) => { editButton = el; }} aria-label={t('queue.editLabel')} onClick={() => {
                    write(props.q.text);
                  }}>{t('queue.edit')}</button>
                </Show>
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
            </>
          }
        >
          <form onSubmit={(e) => { e.preventDefault(); void save(); }} onKeyDown={(e) => {
            if (!e.isComposing && e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); cancel(); }
          }}>
            <textarea
              aria-label={t('queue.editLabel')}
              value={editing()}
              rows={Math.min(10, (editing() ?? '').split('\n').length + 1)}
              disabled={saving()}
              onInput={(e) => write(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.isComposing) return;
                if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void save(); }
              }}
              ref={(el) => queueMicrotask(() => el.focus())}
            />
            <Show when={error()}><p class="queue-error" role="alert">{error()}</p></Show>
            <div class="actions">
              <button type="submit" disabled={saving()}>{t('queue.save')}</button>
              <button type="button" disabled={saving()} onClick={cancel}>{t('queue.cancel')}</button>
            </div>
          </form>
        </Show>
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

const PARAGRAPHS = /\n{2,}/;
const BLANK = '\n\n';
/** The agent's words between its steps, in the work log: the last two paragraphs, the earlier ones under "N more". */
function Narration(props: { text: string; conversationId: string; messageId: string; visuals?: VisualAnswer[]; pieceRevision?: number }) {
  const [all, setAll] = createSignal(false);
  const parts = createMemo(() => (props.text.includes('```') ? [props.text] : props.text.split(PARAGRAPHS)));
  const hidden = () => (all() || parts().length < 3 ? 0 : parts().length - 2);
  return (
    <>
      <Show when={hidden()}>
        <button type="button" class="more-narration" onClick={() => setAll(true)}>
          {t('narration.more', { n: hidden() })}
        </button>
      </Show>
      <Show when={props.text.includes('faber-piece') || props.visuals?.some((a) => a.versions.some((v) => !v.file && v.messageId === props.messageId && v.end <= props.text.length))}
        fallback={<div class="md" onClick={openLinks} innerHTML={html(parts().slice(hidden()).join(BLANK))} />}>
        <div class="md" onClick={openLinks} onKeyDown={pressImage}><PieceReply text={props.text} offset={0} conversationId={props.conversationId} messageId={props.messageId} answers={props.visuals ?? []} revision={props.pieceRevision} /></div>
      </Show>
    </>
  );
}

export function Thread(props: Shared & { messages: Message[]; queued?: QueuedTurn[]; busy?: boolean; onError?: (err: unknown) => void; onQueuedRecovered?: (id: string, text: string) => void; from: number; until?: number; searchIndex?: number; rewindNotice?: string; conversationNames?: Record<string, string>; onRewind?: () => Promise<void> }) {
  const [restore, setRestore] = createSignal<{ message: Message; redo?: RewindRedoInfo }>();
  const [open, setOpen] = createSignal(false);
  const shown = keepForExit(open);
  const [revision, setRevision] = createSignal(0);
  // A completed turn refreshes discovery; text streaming never asks the disk for history.
  const [redos] = createResource(() => `${props.conversationId}:${props.messages.length}:${props.messages.at(-1)?.status}:${revision()}`, () => api.rewindRedos(props.conversationId).catch(() => []));
  const redo = () => redos()?.[0];
  const lastMine = createMemo(() => props.messages.findLast((m) => m.role === 'user' && !m.wake)?.id);
  const redoAt = createMemo(() => props.messages.some((m) => m.id === redo()?.messageId && m.role === 'user' && !m.wake) ? redo()?.messageId : lastMine());
  const showRestore = (message: Message, redo?: RewindRedoInfo) => { setRestore({ message, redo }); setOpen(true); };
  const queuedIds = createMemo(() => (props.queued ?? []).map((q) => q.id));
  const prompt = (at: number) => (props.messages[at - 1]?.role === 'user' ? props.messages[at - 1] : undefined);
  // Turns Faber sent by itself, one after another, read as one run (task 018): its earlier turns fold under one header.
  const runs = createMemo(() => runsOf(props.messages));
  const hidden = createMemo(() => folded(new Map([...runs()].filter(([last]) => last < (props.until ?? Infinity)))));
  // The questions Faber answered in a run, on its newest reply.
  const answered = (at: number) => {
    const run = runs().get(at);
    return run ? answeredIn(run.replies.map((r) => props.messages[r]!)) : undefined;
  };
  const drawn = (m: Message, at: number) => !(prompt(at)?.wake && m.status !== 'streaming' && empty(m));
  const earlier = (at: number) => (
    <Show when={drawn(props.messages[at]!, at)}>
      <Answer {...props} pieceRevision={revision()} m={() => props.messages[at]!} prompt={prompt(at)} last={false} answered={[]} messageIndex={at} />
    </Show>
  );
  return (
    <>
      <Show when={props.rewindNotice}>
        <p class="woke" classList={{ 'rewind-empty': !props.messages.length }} tabindex={props.messages.length ? undefined : 0} onContextMenu={(e) => { const available = redo(); if (!props.messages.length && available) openMenu(e, [{ id: 'rewind-redo', label: t(available.recovery ? 'rewind.recover' : 'rewind.redo'), run: () => showRestore({ id: available.messageId, role: 'user', text: '', at: 0 }, available) }], e.currentTarget); }}>
          {fromEngine(props.rewindNotice)}
        </p>
      </Show>
      <Index each={props.messages.slice(props.from, props.until)}>
        {(m, i) => (
          <Show
            when={m().role === 'user'}
            fallback={
              <Show when={!hidden().has(props.from + i)}>
                <Show when={runs().get(props.from + i)}>
                  {(run) => <RunBlock messages={props.messages} run={run()} plan={props.plan} earlier={earlier} reveal={props.searchIndex} from={props.from} />}
                </Show>
                {/* The newest turn of a run is drawn even with nothing of its own when it holds the run's answers. */}
                <Show when={drawn(m(), props.from + i) || answered(props.from + i)?.length}>
                  <Answer
                    {...props}
                    pieceRevision={revision()}
                    m={m}
                    prompt={prompt(props.from + i)}
                    last={props.from + i === props.messages.length - 1}
                    answered={answered(props.from + i)}
                    messageIndex={props.from + i}
                  />
                </Show>
              </Show>
            }
          >
            {/* A message Faber sent by itself takes no row: the reply's header carries it (`woke` in Answer). */}
            <Show when={!m().wake}>
              <Show when={`edit.${props.conversationId}.${m().id}`} keyed>
                {(editKey) => (
                  <Mine
                    editKey={editKey}
                    m={m}
                    onBranch={props.onBranch}
                    messageIndex={props.from + i}
                    onRewind={() => showRestore(m())}
                    onRedo={m().id === redoAt() && redo() ? () => showRestore(m(), redo()) : undefined}
                    recovery={redo()?.recovery}
                    versions={props.versions?.find((v) => v.messageId === m().id)}
                    onVersion={props.onVersion}
                  />
                )}
              </Show>
            </Show>
          </Show>
        )}
      </Index>
      <Show when={props.area === 'project' && props.until === undefined}>
        <DetailsLine conversationId={props.conversationId} messages={props.messages} busy={props.busy} weighing={props.weighing} background={props.background} />
      </Show>
      <Show when={props.conversationId} keyed>{(conversationId) =>
        <For each={queuedIds()}>{(id) => {
          const initial = props.queued!.find((q) => q.id === id)!;
          return <Queued conversationId={conversationId} q={props.queued?.find((q) => q.id === id) ?? initial}
            busy={props.busy ?? false} onRecovered={props.onQueuedRecovered} onError={props.onError} />;
        }}</For>
      }</Show>
      <Show when={shown() ? restore() : undefined} keyed>{(target) => <Rewind conversationId={props.conversationId} messageId={target.message.id} point={target.message.rewind} redo={target.redo} conversationNames={props.conversationNames} leaving={!open()} onClose={() => setOpen(false)} onChanged={async () => { await props.onRewind?.(); setRevision((n) => n + 1); }} />}</Show>
      <Quote />
    </>
  );
}
