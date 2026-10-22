import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api, errorText, type ChildWords, type ToolUse } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { Markdown, openLinks } from './markdown.tsx';
import { animateList } from './motion.ts';
import { elapsed } from './running.ts';
import { useDraft } from './unsent.ts';
import { agentKind, agentState, changeOf, middle, toolRow, type AgentStep, type Change, type Sources } from './worklog.ts';

// A subagent opened in the running column (R161 step 3): its brief word for word, the steps it ran and its report, drawn
// with the conversation's own parts. The column is the one place on the right, so this replaces its list until "back".

/** A part of the subagent's view: a run of its steps (their ids), words it wrote (their text), or the line saying later words are not kept. */
interface Part {
  key: string;
  ids?: string[];
  text?: string;
}
const sameIds = (a: string[] = [], b: string[] = []) => a.length === b.length && a.every((id, i) => id === b[i]);

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
export const showAgent = (agent?: OpenAgent) => {
  setOpened(agent);
  if (!agent) setWide(false);
};

const [wide, setWide] = createSignal(false);
/** The open subagent fills the stage as a full thread instead of sitting in the column. */
export const agentWide = wide;
export const toggleAgentWide = () => setWide((on) => !on);

/** A path or command that may wrap after each slash, so a long one breaks between folders before it breaks inside a name. The
 * text stays the same for a screen reader and for copying. */
export function Breakable(props: { text: string }) {
  return <For each={props.text.split(/(?<=[/\\])/)}>{(part) => <>{part}<wbr /></>}</For>;
}

/** A subagent's tool in its own view, kept by the tool's id: the reply's progress replaces the tool objects, and a line the
 * person opened stays open, with its focus, instead of being made again. */
function StepChip(props: { id: string; tools: () => ToolUse[] }) {
  const tool = createMemo<ToolUse>((last) => props.tools().find((t) => t.id === props.id) ?? last, props.tools().find((t) => t.id === props.id)!);
  return <ToolChip tool={tool()} full />;
}

/** One tool a turn or a subagent ran: its kind and what it ran on, struck through when it failed. In the subagent's view
 * (`full`) the line wraps to two lines at most and a click or Enter shows the rest, since there is room to read it; elsewhere
 * it is one line with the middle cut. */
export function ToolChip(props: { tool: ToolUse; full?: boolean }) {
  const [more, setMore] = createSignal(false);
  const toggle = () => setMore((on) => !on);
  return (
    <span
      classList={{ bad: props.tool.ok === false, full: props.full, more: more() }}
      title={toolRow(props.tool).detail || props.tool.output}
      tabindex={props.full ? 0 : undefined}
      role={props.full ? 'button' : undefined}
      aria-expanded={props.full ? more() : undefined}
      onClick={() => props.full && toggle()}
      onKeyDown={(e) => props.full && (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), toggle())}
    >
      <Show when={props.tool.ok === true}>
        <Icon name="check" small />
      </Show>
      <b>{toolRow(props.tool).label}</b>
      <Show when={toolRow(props.tool).detail}><i>{props.full ? <Breakable text={toolRow(props.tool).detail} /> : middle(toolRow(props.tool).detail)}</i></Show>
      <Show when={changeOf(props.tool)}>{(change) => <Delta change={change()} />}</Show>
    </span>
  );
}

/** How many lines an edit added and removed, said in words for a screen reader. */
export function Delta(props: { change: Pick<Change, 'added' | 'removed'> }) {
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

/** The sites a running reply opened, the newest last and the one it reads now in the first ink (R161 step 9). The status
 * line already says the page it reads, so the list is for the eye. */
export function SourceList(props: { sources: Sources }) {
  const SHOWN = 4;
  const rest = () => props.sources.hosts.length - SHOWN;
  return (
    <p class="sources" aria-hidden="true">
      <span>{t('tool.sources', { n: props.sources.hosts.length })}</span>
      <Show when={rest() > 0}>
        <span>+{rest()}</span>
      </Show>
      <For each={props.sources.hosts.slice(-SHOWN)}>{(host) => <span classList={{ now: host === props.sources.now }}>{host}</span>}</For>
    </p>
  );
}

/** "Tell it…": the conversation's own box, started with the step's name. A CLI subagent cannot be written to while it runs,
 * so the message goes to the main agent: while Codex works it goes into its turn; Claude Code takes it after the turn, or at
 * once with "Send now". */
export const tellAbout = (title: string) => startMessage(t('agent.tellAbout', { title }));

/** Starts the conversation's box with `text`, ahead of anything already written there. "Continue with a helper" (R161
 * step 5) starts it the same way, asking the main agent to hand the rest of a piece of work to a helper from the other AI. */
export function startMessage(text: string) {
  const box = document.querySelector<HTMLTextAreaElement>('.composer textarea');
  if (!box) return;
  box.focus();
  if (box.value.startsWith(text)) return;
  box.setSelectionRange(0, 0);
  // insertText keeps the field's undo history; the fallback writes the text itself and tells the page.
  if (document.execCommand('insertText', false, text)) return;
  box.value = text + box.value;
  box.dispatchEvent(new Event('input', { bubbles: true }));
}

/** The words under a subagent's name: its kind, where it is, how many steps it ran and, once it ended, how long it took (a
 * second or more; a refused one never ran). */
export function agentMeta(step: AgentStep, streaming: boolean): string {
  const { started, ended, declined } = step.call;
  const took = started !== undefined && ended !== undefined && ended - started >= 1000 && !declined ? elapsed(started, ended) : undefined;
  return [agentKind(step.kind), t(`agent.${agentState(step, streaming)}`), t('activity.steps', { n: step.tools.length }), took && t(took.key, { n: took.n })].filter(Boolean).join(' · ');
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
    <section class="side-facts" aria-label={t('agent.facts')} ref={animateList}>
      <h3>{t('agent.facts')}</h3>
      <Show when={edited().length}>
        <p>{t('agent.facts.edited', { n: edited().length })}</p>
        <ul class="paths">
          <For each={edited()}>{(path) => <li title={path}><Breakable text={path} /></li>}</For>
        </ul>
      </Show>
      <p classList={{ bad: props.facts.failed > 0 }}>{checks()}</p>
      <Show when={unnamed().length}>
        <p class="unnamed">{t('agent.facts.unnamed')}</p>
        <ul class="paths">
          <For each={unnamed()}>{(path) => <li title={path}><Breakable text={path} /></li>}</For>
        </ul>
      </Show>
    </section>
  );
}

/** What the person wrote to a helper at work, each with where it got to. */
function Told(props: { told: NonNullable<ToolUse['told']> }) {
  return (
    <For each={props.told}>
      {(told) => (
        <section class="side-told" aria-label={t('agent.helper.you')}>
          <div class="you">{told.text}</div>
          <p classList={{ bad: told.taken === false }}>
            {t(told.taken === undefined ? 'agent.helper.sent' : told.taken ? 'agent.helper.taken' : 'agent.helper.missed')}
          </p>
        </section>
      )}
    </For>
  );
}

/** Unsent words per helper's call, so switching to another helper or closing the view never loses them or sends them elsewhere. */
const helperDrafts = new Map<string, string>();

/** A helper at work is a session of its own (R161 step 5), so this box writes to it: Enter sends, Shift+Enter breaks the line.
 * It is made again for each helper (keyed by its call). */
function HelperBox(props: { callId: string }) {
  const [text, setTextNow] = createSignal(helperDrafts.get(props.callId) ?? '');
  const setText = (words: string) => {
    setTextNow(words);
    if (words) helperDrafts.set(props.callId, words);
    else helperDrafts.delete(props.callId);
  };
  const [error, setError] = createSignal('');
  const [sending, setSending] = createSignal(false);
  // Like every box, unsent words keep an update from closing the window over them.
  useDraft(() => text().trim().length > 0);
  const send = async () => {
    const words = text().trim();
    const open = opened();
    if (!words || !open || sending()) return;
    setSending(true);
    setError('');
    try {
      await api.tellHelper(open.conversationId, props.callId, words);
      setText('');
    } catch (err) {
      setError(fromEngine(errorText(err)));
    } finally {
      setSending(false);
    }
  };
  return (
    <form
      class="side-write"
      ref={animateList}
      onSubmit={(e) => {
        e.preventDefault();
        void send();
      }}
    >
      <label for="helper-box">{t('agent.helper.note')}</label>
      <textarea
        id="helper-box"
        class="field"
        rows={2}
        value={text()}
        placeholder={t('agent.helper.placeholder')}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key !== 'Enter' || e.shiftKey || e.isComposing) return;
          e.preventDefault();
          void send();
        }}
      />
      <button type="submit" class="btn" disabled={!text().trim() || sending()}>
        {t('agent.helper.send')}
      </button>
      <Show when={error()}>
        <p class="bad" role="alert">
          {error()}
        </p>
      </Show>
    </form>
  );
}

/** The open subagent, as the running column's content: a head with the way back, then the subagent's own thread. `helpable`:
 * a helper from the other AI can take on a piece of work here (a project with both AIs connected). `onTell` gets the words
 * that start the conversation's box. */
export function AgentChat(props: {
  step: AgentStep;
  streaming: boolean;
  helpable?: boolean;
  onBack: () => void;
  onTell?: (text: string) => void;
  onClose: () => void;
  /** The subagent fills the stage as a full thread; `onWide` swaps between that and the column. */
  wide?: boolean;
  onWide?: () => void;
}) {
  const state = () => agentState(props.step, props.streaming);
  const title = () => props.step.title || t('agent.untitled');
  const working = () => ['running', 'waiting', 'stuck'].includes(state());
  // What the subagent wrote between its steps comes from the engine while this view is open (the reply's own updates carry only
  // how much there is), and again whenever that changes.
  const [loaded] = createResource(
    () => {
      const open = opened();
      const size = props.step.call.wordsChars;
      // A string, so an update that adds no words (the call object is new on every update) does not read them again.
      return open && size ? JSON.stringify([open.conversationId, props.step.call.id, size]) : undefined;
    },
    (key) => {
      const [id, call] = JSON.parse(key) as [string, string, number];
      return api.childWords(id, call).then((words) => ({ call, words }), () => ({ call, words: [] as ChildWords[] }));
    },
  );
  // The subagent's steps in runs, with its own words between the runs (the engine counts `at` over the steps it ran itself), so
  // the words are drawn outside the steps' chip rules. Keys, so a line the person opened stays open while more words and steps
  // arrive. The last words give way to the report only when the report starts with them; otherwise they say what it does not.
  const parts = createMemo<Part[]>(
    () => {
      const got = loaded.latest;
      const words = got?.call === props.step.call.id ? got.words : [];
      const report = props.step.report.trim();
      const out: Part[] = [];
      let run: string[] = [];
      let steps = 0;
      const flush = () => {
        if (run.length) out.push({ key: `s${run[0]}`, ids: run });
        run = [];
      };
      const said = (at: number, last = false) => {
        const found = words.find((w) => w.at === at);
        if (!found || (last && report && report.startsWith(found.text.trim()))) return;
        flush();
        out.push({ key: `w${at}`, text: found.text });
      };
      for (const tool of props.step.tools) {
        if (tool.parent === props.step.call.id) said(steps++);
        run.push(tool.id);
      }
      said(steps, true);
      flush();
      // Where the engine stopped keeping words: right after the last it kept.
      if (props.step.call.wordsCut) out.splice(out.findLastIndex((part) => part.text !== undefined) + 1, 0, { key: 'cut' });
      return out;
    },
    [],
    { equals: (a, b) => a.length === b.length && a.every((part, i) => part.key === b[i]!.key && part.text === b[i]!.text && sameIds(part.ids, b[i]!.ids)) },
  );
  const keys = createMemo(() => parts().map((part) => part.key), [], { equals: (a, b) => a.length === b.length && a.every((key, i) => key === b[i]) });
  const start = (text: string) => (props.onTell ? props.onTell(text) : startMessage(text));
  return (
    <>
      <div class="rc-head side-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={() => props.onBack()}>
          <Icon name="left" />
        </button>
        <h2 id="running-title" class="display" title={title()}>
          {title()}
        </h2>
        <Show when={props.onWide}>
          <button class="iconbtn agent-wide" aria-label={t(props.wide ? 'agent.narrow' : 'agent.widen')} title={t(props.wide ? 'agent.narrow' : 'agent.widen')} onClick={() => props.onWide!()}>
            <Icon name={props.wide ? 'shrink' : 'expand'} />
          </button>
        </Show>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div class="rc-body side-body" data-state={state()} ref={animateList}>
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
        <For each={keys()}>
          {(key) => {
            const part = () => parts().find((p) => p.key === key);
            const ids = createMemo(() => part()?.ids ?? [], [], { equals: sameIds });
            return key === 'cut' ? (
              <p class="side-meta side-cut">{t('agent.words.cut')}</p>
            ) : key[0] === 'w' ? (
              <div class="md side-said" onClick={openLinks}>
                <Markdown text={part()?.text ?? ''} />
              </div>
            ) : (
              <div class="tools" aria-label={t('agent.steps')} ref={animateList}>
                <For each={ids()}>{(id) => <StepChip id={id} tools={() => props.step.tools} />}</For>
              </div>
            );
          }}
        </For>
        <Show when={props.step.call.told?.length}>
          <Told told={props.step.call.told!} />
        </Show>
        <Show when={props.step.call.facts}>{(facts) => <Facts facts={facts()} />}</Show>
        <Show when={props.step.report}>
          <div class="md agent-report" onClick={openLinks}>
            <Markdown text={props.step.report} />
          </div>
        </Show>
        <Show when={working() && props.step.helper && props.step.call.id} keyed>
          {(callId) => <HelperBox callId={callId} />}
        </Show>
        <Show when={working() && !props.step.helper}>
          <div class="side-tell">
            <p>{t('agent.side.note')}</p>
            <button type="button" class="btn" onClick={() => start(t('agent.tellAbout', { title: title() }))}>
              {t('agent.tell')}
            </button>
          </div>
        </Show>
        <Show when={!working() && state() !== 'background' && props.helpable}>
          <div class="side-tell">
            <p>{t('agent.continue.note')}</p>
            <button type="button" class="btn" onClick={() => start(t('agent.continueAbout', { title: title() }))}>
              {t('agent.continue')}
            </button>
          </div>
        </Show>
      </div>
    </>
  );
}
