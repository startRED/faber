import { createEffect, createMemo, createSignal, createUniqueId, For, on, Show } from 'solid-js';
import type { TeamMessage, Vendor } from './api.ts';
import { agentActivity, agentMeta, openedAgent, showAgent } from './AgentChat.tsx';
import { askedQuestions } from './Ask.tsx';
import { Collapse } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { ActorMark, StateGlyph, stateKind, ToolGlyph } from './icons/kinds.tsx';
import { t } from './i18n.ts';
import { prettyModel } from './models.ts';
import { elapsed } from './running.ts';
import { teamCounts, teamOutcome, teamTime, teamWire, type WireLine } from './team.ts';
import { agentState, agentWord, type AgentStep } from './worklog.ts';

const pieceTitle = (step: AgentStep) => step.call.team?.title || step.title || t('agent.untitled');
const modelLine = (step: AgentStep) => [step.model ? prettyModel(step.model) : '', step.effort && t(`effort.${step.effort as 'low' | 'medium' | 'high'}`)].filter(Boolean).join(' · ');
const jobWord = (step: AgentStep) => ['read', 'build', 'review'].includes(step.job ?? '') ? step.job === 'review' ? t('review.kind') : t(`team.job.${step.job as 'read' | 'build'}`) : '';
const duration = (step: AgentStep) => {
  const { started, ended, declined } = step.call;
  const took = started !== undefined && ended !== undefined && ended - started >= 1000 && !declined ? elapsed(started, ended) : undefined;
  return took ? t(took.key, { n: took.n }) : '';
};

/** A new sender id replaces one decorative ring. No timer or animation listener stays alive. */
function SenderRing(props: { message?: string }) {
  const message = createMemo(() => props.message);
  const [pulse, setPulse] = createSignal<string>();
  createEffect(on(message, (id) => setPulse(id), { defer: true }));
  return <Show when={pulse()} keyed>{(id) => <span class="team-pulse" data-sender-message={id} aria-hidden="true" />}</Show>;
}

function Member(props: { step: AgentStep; streaming: boolean; delivery: boolean; pulse?: string; lead?: Vendor; conversationId: string; messageId: string }) {
  const step = () => props.step;
  const state = () => agentState(step(), props.streaming);
  const title = () => pieceTitle(step());
  const actor = () => step().vendor ?? (!step().helper ? props.lead : undefined);
  const current = () => openedAgent();
  const activity = () => ['running', 'background'].includes(state()) ? agentActivity(step(), props.streaming) : '';
  const outcome = createMemo(() => ['done', 'failed', 'stopped', 'declined'].includes(state()) ? teamOutcome(step().report) : '');
  const question = () => state() === 'waitingLead'
    ? step().call.team?.messages.findLast((line) => line.from === step().call.id && line.kind === 'ask' && line.state === 'waiting')?.text ?? ''
    : state() === 'waiting' ? askedQuestions(step().tools.findLast((tool) => tool.name === 'AskUserQuestion' && tool.ok === undefined && !tool.declined)?.input)[0]?.question ?? '' : '';
  const detail = () => activity() || question() || outcome();
  const checkOnly = () => props.delivery && state() === 'done';
  return <button type="button" class="team-member" data-state={state()} data-call={step().call.id}
    aria-controls="running" aria-current={current()?.conversationId === props.conversationId && current()?.callId === step().call.id ? 'true' : undefined}
    aria-label={[title(), modelLine(step()), jobWord(step()), agentMeta({ ...step(), kind: '' }, props.streaming), detail()].filter(Boolean).join('. ')}
    onClick={() => showAgent({ conversationId: props.conversationId, messageId: props.messageId, callId: step().call.id })}>
    <Show when={!props.delivery}><span class="team-drop" aria-hidden="true" /><SenderRing message={props.pulse} /></Show>
    <span class="team-piece">
      <Show when={actor()} fallback={<ToolGlyph kind="helper" small />}>{(vendor) => <ActorMark actor={vendor()} />}</Show>
      <b class="team-title">{title()}</b>
    </span>
    <Show when={modelLine(step()) || !props.delivery && jobWord(step())}><span class="team-meta">
      <span class="team-model" title={modelLine(step())}>{modelLine(step())}</span>
      <Show when={!props.delivery && jobWord(step())}><span class="team-job">{jobWord(step())}</span></Show>
    </span></Show>
    <span class="team-state" classList={{ 'team-complete': checkOnly() }}>
      <StateGlyph state={state() === 'waitingLead' ? 'working' : stateKind(state())} small label={checkOnly() ? t('agent.done') : undefined} />
      <Show when={!checkOnly()}><span>{t(state() === 'waitingLead' ? 'team.waitingLead' : agentWord(step(), props.streaming))}</span></Show>
    </span>
    <Show when={detail() || !props.delivery && duration(step())}><span class="team-detail">
      <Show when={detail()}><span class={activity() ? 'team-activity' : question() ? 'team-member-question' : 'team-outcome'} title={detail()}>{detail()}</span></Show>
      <Show when={!props.delivery && duration(step())}><span class="team-duration">{duration(step())}</span></Show>
    </span></Show>
    <Show when={props.delivery && duration(step())}><span class="team-duration">{duration(step())}</span></Show>
  </button>;
}

function Wire(props: { line: WireLine; members: AgentStep[] }) {
  const [open, setOpen] = createSignal(false);
  const [answerOpen, setAnswerOpen] = createSignal(false);
  const message = () => props.line.message;
  const name = (id: string, fallback: string) => id === 'lead' ? t('agent.team.lead') : id === 'engine' ? t('act.faber') : props.members.find((step) => step.call.id === id) ? pieceTitle(props.members.find((step) => step.call.id === id)!) : fallback;
  const address = (line: TeamMessage) => `${name(line.from, line.fromName)} → ${name(line.to, line.toName)}`;
  const content = (line: TeamMessage) => <span class="team-wire-body">
    <span class="team-chip" data-kind={line.kind}>{t(`team.chip.${line.kind}`)}</span>{' '}
    <span class="team-address" title={address(line)}>{address(line)}</span>{' '}
    <span class="team-wire-text">{line.text}</span>
  </span>;
  return <div class="team-exchange" data-message={message().id}>
    <button type="button" class="team-wire" aria-expanded={open()} onClick={() => setOpen(!open())}>
      {content(message())}
      <Show when={message().kind === 'ask' && message().state === 'waiting'}><span class="team-question">{t('agent.waitingLead')}</span></Show>
    </button>
    <For each={props.line.replies.map((reply) => reply.id)}>{(id) => {
      const reply = () => props.line.replies.find((reply) => reply.id === id)!;
      return <button type="button" class="team-wire team-answer" aria-expanded={answerOpen()} onClick={() => setAnswerOpen(!answerOpen())}>{content(reply())}</button>;
    }}</For>
    <Show when={message().state === 'lapsed' && !props.line.replies.some((reply) => reply.kind === 'noAnswer')}><p class="team-lapsed"><span class="team-chip" data-kind="noAnswer">{t('team.chip.noAnswer')}</span><span class="team-address">{t('act.faber')} → {name(message().from, message().fromName)}</span><span>{t('team.noAnswer')}</span></p></Show>
  </div>;
}

/** One helper run in its place in the reply. Details leave the DOM when folded; no clock or polling. */
export function Team(props: { members: AgentStep[]; streaming: boolean; lead?: Vendor; leadModel?: string; conversationId: string; messageId: string }) {
  const heading = createUniqueId();
  const counts = createMemo(() => teamCounts(props.members, props.streaming));
  const wire = createMemo(() => teamWire(props.members));
  const messages = createMemo(() => wire().flatMap((line) => [line.message, ...line.replies]).sort((a, b) => a.at - b.at));
  const pulse = (sender: string) => messages().filter((line) => line.from === sender).at(-1)?.id;
  const [expanded, setExpanded] = createSignal(false);
  const [earlier, setEarlier] = createSignal(false);
  const [rest, setRest] = createSignal(counts().ended);
  const open = () => !counts().ended || expanded();
  let root!: HTMLElement;
  createEffect(on(() => counts().ended, (ended) => {
    if (!ended) { setExpanded(false); setRest(false); return; }
    setRest(true);
  }));
  // The caret replaces a focused card; a newer focus outside the team wins.
  const focusFold = (button: HTMLButtonElement) => {
    if (!root?.contains(document.activeElement)) return;
    queueMicrotask(() => {
      if (button.isConnected && (document.activeElement === document.body || root.contains(document.activeElement))) button.focus();
    });
  };
  const leadSentence = () => {
    const waiting = props.members.find((step) => agentState(step, props.streaming) === 'waitingLead');
    if (waiting) return t('team.answering', { piece: pieceTitle(waiting) });
    const handed = t('team.handed', { n: props.members.length });
    const answered = messages().filter((line) => line.kind === 'answer' && line.from === 'lead').length;
    return !counts().working && answered ? `${handed} · ${t('team.answered', { n: answered })}` : handed;
  };
  const summary = () => {
    const time = teamTime(props.members);
    const took = time ? elapsed(time.start, time.end) : undefined;
    return [t('team.finished'), t('team.pieces', { n: props.members.length }), t('team.messages', { n: messages().length }), took && t(took.key, { n: took.n })].filter(Boolean).join(' · ');
  };
  const failures = () => props.members.filter((step) => ['failed', 'stopped', 'declined'].includes(agentState(step, props.streaming)));
  return <section ref={root} class="team" role="group" aria-labelledby={heading} data-rest={rest()}>
    <Show when={counts().ended} fallback={<div class="team-head" id={heading}>
      <span>{t('team.label')}</span>
      <span class="team-segments" aria-hidden="true"><For each={props.members.map((step) => step.call.id)}>{(id) => <i data-state={agentState(props.members.find((step) => step.call.id === id)!, props.streaming)} />}</For></span>
      <span class="team-count">{[counts().working && t('team.working', { n: counts().working }), counts().done && t('team.done', { n: counts().done }), counts().waiting && t('team.questions', { n: counts().waiting })].filter(Boolean).join(' · ')}</span>
    </div>}>
      <button ref={focusFold} type="button" class="team-head team-fold" id={heading} aria-expanded={open()} onClick={() => setExpanded(!expanded())}>
        <Icon name="down" small /><Icon name="check" small /><span>{summary()}<For each={failures()}>{(step) => <span class="team-failure"> · {pieceTitle(step)}: {t(agentWord(step, props.streaming))}</span>}</For></span>
      </button>
    </Show>
    <Collapse open={open()}>
      <Show when={!counts().ended}>
        <div class="team-lead">
          <Show when={props.lead} fallback={<ToolGlyph kind="helper" small />}>{(vendor) => <ActorMark actor={vendor()} />}</Show>
          <span>{t('agent.team.lead')}</span><span class="team-dot" aria-hidden="true">·</span><Show when={props.leadModel}><span class="team-model" title={prettyModel(props.leadModel!)}>{prettyModel(props.leadModel!)}</span><span class="team-dot" aria-hidden="true">·</span></Show>
          <span class="team-lead-sentence">{leadSentence()}</span><SenderRing message={pulse('lead')} />
        </div>
      </Show>
      <div class={counts().ended ? 'team-deliveries' : 'team-grid'} style={{ '--team-members': props.members.length }} role={counts().ended ? 'list' : undefined}>
        <For each={props.members.map((step) => step.call.id)}>{(id) => <div class="team-slot" role={counts().ended ? 'listitem' : undefined}>
          <Member step={props.members.find((step) => step.call.id === id)!} streaming={props.streaming} delivery={counts().ended} pulse={pulse(id)} lead={props.lead} conversationId={props.conversationId} messageId={props.messageId} />
        </div>}</For>
      </div>
      <Show when={wire().length}>
        <div class="team-wires">
          <Show when={counts().ended}><p class="team-messages-label">{t('team.messagesLabel', { n: messages().length })}</p></Show>
          <Show when={wire().length > 3}><button type="button" class="team-earlier" aria-expanded={earlier()} onClick={() => setEarlier(!earlier())}>{t('team.earlier', { n: wire().length - 3 })}</button></Show>
          <For each={(earlier() ? wire() : wire().slice(-3)).map((line) => line.message.id)}>{(id) => <Wire line={wire().find((line) => line.message.id === id)!} members={props.members} />}</For>
        </div>
      </Show>
    </Collapse>
  </section>;
}
