import { createSignal, For, Show } from 'solid-js';
import type { ToolUse } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { Markdown, openLinks } from './markdown.tsx';
import { agentKind, agentState, changeOf, middle, toolRow, type AgentStep, type Change } from './worklog.ts';

// A subagent opened in the running column (R161 step 3): its brief word for word, the steps it ran and its report, drawn
// with the conversation's own parts. The column is the one place on the right, so this replaces its list until "back".

/** The subagent the person opened: the reply it belongs to and its agent call. */
export interface OpenAgent {
  conversationId: string;
  messageId: string;
  callId: string;
}

const [opened, setOpened] = createSignal<OpenAgent>();
/** The subagent open in the running column, if any. */
export const openedAgent = opened;
/** Opens a subagent in the running column, or goes back to the running list. */
export const showAgent = (agent?: OpenAgent) => setOpened(agent);

/** One tool a turn or a subagent ran: its kind and what it ran on, struck through when it failed. */
export function ToolChip(props: { tool: ToolUse }) {
  return (
    <span class={props.tool.ok === false ? 'bad' : ''} title={toolRow(props.tool).detail || props.tool.output}>
      <Show when={props.tool.ok === true}>
        <Icon name="check" small />
      </Show>
      <b>{toolRow(props.tool).label}</b>
      <Show when={toolRow(props.tool).detail}><i>{middle(toolRow(props.tool).detail)}</i></Show>
      <Show when={changeOf(props.tool)}>{(change) => <Delta change={change()} />}</Show>
    </span>
  );
}

/** How many lines an edit added and removed, said in words for a screen reader. */
export function Delta(props: { change: Change }) {
  const said = () => [props.change.added && t('tool.added', { n: props.change.added }), props.change.removed && t('tool.removed', { n: props.change.removed })].filter(Boolean).join(', ');
  return (
    <em class="delta" role="img" aria-label={said()} title={said()}>
      <Show when={props.change.added}><span class="add">+{props.change.added}</span></Show>
      <Show when={props.change.removed}><span class="del">−{props.change.removed}</span></Show>
    </em>
  );
}

/** The few lines an edit is changing, while it runs (R161 step 9): removed lines first, then added, past four the count of the rest. */
export function ChangedLines(props: { change: Change }) {
  const SHOWN = 4;
  const rest = () => props.change.lines.length - SHOWN;
  return (
    <div class="changed" aria-hidden="true">
      <For each={props.change.lines.slice(0, SHOWN)}>{(line) => <div classList={{ add: line.added, del: !line.added }}><b>{line.added ? '+' : '−'}</b>{line.text || ' '}</div>}</For>
      <Show when={rest() > 0}><p>{t('tool.moreLines', { n: rest() })}</p></Show>
    </div>
  );
}

/** "Tell it…": the conversation's own box, started with the step's name. A CLI subagent cannot be written to while it runs,
 * so the message goes to the main agent: while Codex works it goes into its turn; Claude Code takes it after the turn, or at
 * once with "Send now". */
export function tellAbout(title: string) {
  const box = document.querySelector<HTMLTextAreaElement>('.composer textarea');
  if (!box) return;
  const text = t('agent.tellAbout', { title });
  box.focus();
  if (box.value.startsWith(text)) return;
  box.setSelectionRange(0, 0);
  // insertText keeps the field's undo history; the fallback writes the text itself and tells the page.
  if (document.execCommand('insertText', false, text)) return;
  box.value = text + box.value;
  box.dispatchEvent(new Event('input', { bubbles: true }));
}

/** The words under a subagent's name: its kind, where it is and how many steps it ran. */
export function agentMeta(step: AgentStep, streaming: boolean): string {
  return [agentKind(step.kind), t(`agent.${agentState(step, streaming)}`), t('activity.steps', { n: step.tools.length })].filter(Boolean).join(' · ');
}

/** What an ended subagent did, counted by the engine from its steps (R161 step 4): never the main agent's word for it. */
function Facts(props: { facts: NonNullable<ToolUse['facts']> }) {
  const edited = () => props.facts.edited ?? [];
  const unnamed = () => props.facts.unnamed ?? [];
  const checks = () =>
    props.facts.checks
      ? `${t('agent.facts.checks', { n: props.facts.checks })} · ${props.facts.failed ? t('agent.facts.failed', { n: props.facts.failed }) : t('agent.facts.passed')}`
      : t('agent.facts.none');
  return (
    <section class="side-facts" aria-label={t('agent.facts')}>
      <h3>{t('agent.facts')}</h3>
      <Show when={edited().length}>
        <p>{t('agent.facts.edited', { n: edited().length })}</p>
        <ul class="paths">
          <For each={edited()}>{(path) => <li title={path}>{middle(path)}</li>}</For>
        </ul>
      </Show>
      <p classList={{ bad: props.facts.failed > 0 }}>{checks()}</p>
      <Show when={unnamed().length}>
        <p class="unnamed">{t('agent.facts.unnamed')}</p>
        <ul class="paths">
          <For each={unnamed()}>{(path) => <li title={path}>{middle(path)}</li>}</For>
        </ul>
      </Show>
    </section>
  );
}

/** The open subagent, as the running column's content: a head with the way back, then the subagent's own thread. */
export function AgentChat(props: { step: AgentStep; streaming: boolean; onBack: () => void; onTell?: (title: string) => void; onClose: () => void }) {
  const state = () => agentState(props.step, props.streaming);
  const title = () => props.step.title || t('agent.untitled');
  return (
    <>
      <div class="rc-head side-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={() => props.onBack()}>
          <Icon name="left" />
        </button>
        <h2 id="running-title" class="display" title={title()}>
          {title()}
        </h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div class="rc-body side-body" data-state={state()}>
        <p class="side-meta">{agentMeta(props.step, props.streaming)}</p>
        <Show when={state() === 'stuck' && props.step.call.sign}>
          {(sign) => (
            <p class="agent-why" role="status">
              {t(`agent.stuck.${sign() as 'repeat' | 'failing' | 'silent'}`)}
            </p>
          )}
        </Show>
        <Show when={props.step.brief}>
          <section class="side-brief" aria-label={t('agent.brief')}>
            <div class="you" onClick={openLinks}>
              <Markdown text={props.step.brief} />
            </div>
          </section>
        </Show>
        <Show when={props.step.tools.length}>
          <div class="tools" aria-label={t('agent.steps')}>
            <For each={props.step.tools}>{(tool) => <ToolChip tool={tool} />}</For>
          </div>
        </Show>
        <Show when={props.step.call.facts}>{(facts) => <Facts facts={facts()} />}</Show>
        <Show when={props.step.report}>
          <div class="md agent-report" onClick={openLinks}>
            <Markdown text={props.step.report} />
          </div>
        </Show>
        <Show when={['running', 'waiting', 'stuck'].includes(state())}>
          <div class="side-tell">
            <p>{t('agent.side.note')}</p>
            <button type="button" class="btn" onClick={() => (props.onTell ?? tellAbout)(title())}>
              {t('agent.tell')}
            </button>
          </div>
        </Show>
      </div>
    </>
  );
}
