import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { createEffect, createResource, createSignal, For, Index, onCleanup, Show, type Accessor } from 'solid-js';
import { api, type Choice, type Message, type QueuedTurn, type VendorUsage } from './api.ts';
import { Asks, Parkeds } from './Ask.tsx';
import { copyItem, openMenu } from './contextmenu.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { QuotaCard } from './QuotaCard.tsx';
import { untilReset } from './Sidebar.tsx';
import { fromEngine, t } from './i18n.ts';
import { prettyModel, vendorName, type Option } from './models.ts';
import { animateList } from './motion.ts';
import { warmLeft } from './running.ts';
import { middle, thinkingParts, toolLine, toolRow } from './worklog.ts';

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// Model output is untrusted: rendered as Markdown, then stripped of anything that could run.
// The last two words of a plain heading go in `em`, which the look draws in its quieter tone.
const quiet = (h3: string, text: string) => {
  const words = text.split(' ');
  return words.length < 3 ? h3 : `<h3>${words.slice(0, -2).join(' ')} <em>${words.slice(-2).join(' ')}</em></h3>`;
};
const html = (text: string) => DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true })).replace(/<h3>([^<]*)<\/h3>/g, quiet);

// Links in answers open in the user's browser, never inside Faber.
function openLinks(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest('a');
  if (!a) return;
  e.preventDefault();
  if (/^https?:\/\//.test(a.href)) void api.openLink(a.href);
}

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
        <small>{size(props.file.size)}</small>
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

function usage(m: Message): string {
  const parts: string[] = [];
  if (m.costUsd) parts.push(t('answer.cost', { cost: m.costUsd.toFixed(4) }));
  if (m.inputTokens) parts.push(t('answer.tokens', { input: (m.inputTokens / 1000).toFixed(1), output: m.outputTokens ?? 0 }));
  return parts.join(' · ');
}

// The picker names Claude models by alias ("opus"), so the full id of the next rung is matched to its entry.
function rung(nudge: NonNullable<Message['nudge']>, options: Option[]): Choice {
  const entry = options.find((o) => o.choice.vendor === nudge.vendor && (o.choice.model === nudge.model || o.choice.model === nudge.model.split('-')[1]));
  return { ...(entry?.choice ?? { vendor: nudge.vendor, model: nudge.model }), effort: nudge.effort };
}

type OnBranch = (messageId: string, text: string, choice?: Choice) => void;

interface Shared {
  conversationId: string;
  options: Option[];
  onBranch: OnBranch;
  usage?: VendorUsage[];
}

function Answer(props: Shared & { m: Accessor<Message>; prompt?: Message; last?: boolean }) {
  const m = () => props.m();
  // A quiet note on the latest reply only (a later turn overtakes an older one): how long the CLI keeps its prompt cache warm,
  // hidden while unknown and once cold. One timer, and only while the note is shown.
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
  const waiting = () => m().permissions ?? [];
  // Unknown legacy boundaries and tool-terminated turns keep the entire answer visible.
  const boundary = () => {
    const end = m().activityEnd;
    return m().tools?.length && Number.isInteger(end) && end! > 0 && end! < m().text.length && m().text.slice(end).trim() ? end! : 0;
  };
  // The answer is always kept whole. A status sentence that is exactly the answer's opening paragraph is dropped from the
  // activity instead, so it shows once.
  const finalText = () => m().text.slice(boundary());
  const narration = () => {
    if (!boundary()) return [];
    const paragraphs = m().text.slice(0, boundary()).trimEnd().split(/\n{2,}/);
    const last = paragraphs.at(-1)?.trim();
    const answer = finalText().trimStart();
    if (last && answer.startsWith(last) && (answer.length === last.length || /\s/.test(answer[last.length]!))) paragraphs.pop();
    const rest = paragraphs.join('\n\n').trim();
    return rest ? [rest] : [];
  };
  const thinking = () => thinkingParts(m().thinking ?? '');
  const steps = () => narration().length + (m().tools?.length ?? 0) + (thinking().length ? 1 : 0);
  const commands = () => m().tools?.filter((tool) => /bash|command|exec|shell/i.test(tool.name)).length ?? 0;
  // The answer on screen is never repeated here: once it has text, the status line steps aside.
  const latest = () => {
    const tool = m().tools?.at(-1);
    if (tool && tool.ok === undefined) return toolLine(tool);
    if (finalText().trim()) return '';
    return tool ? toolLine(tool) : middle(thinking().at(-1)?.text ?? '');
  };
  // The name slot: while Auto is still choosing there is none (the reason says so), then the model's name from the moment
  // it is known to the end and after reopening; a model the CLI never named reads as the default one all along.
  const label = () => (streaming() && m().waitUntil ? t('run.waiting', { when: untilReset(m().waitUntil!) }) : m().vendor ? prettyModel(m().model) : '');
  return (
    <article
      class="ai"
      aria-label={t('answer.label')}
      onContextMenu={(e) =>
        openMenu(e, [copyItem(m().text), ...(!streaming() && props.prompt ? [{ id: 'redo', label: t('menu.redo'), run: () => props.onBranch(props.prompt!.id, props.prompt!.text) }] : [])])
      }
    >
      <div class="model">
        <span class="vm" classList={{ claude: m().vendor === 'claude', codex: m().vendor === 'codex' }}>
          <VendorMark vendor={m().vendor} />
          <b>{m().vendor ? vendorName(m().vendor!) : t('model.auto')}</b>
        </span>
        <Show when={label()}><span>{label()}</span></Show>
        <Show when={m().effort}>{(effort) => <span>{t('effort.label')}: {t(`effort.${effort()}`)}</span>}</Show>
        <Show when={m().reason}>
          <em>· {fromEngine(m().reason, { when: m().waitUntil ? untilReset(m().waitUntil!) : t('quota.unknown'), from: m().movedFrom ? vendorName(m().movedFrom!) : '', to: m().vendor ? vendorName(m().vendor!) : '' })}</em>
        </Show>
      </div>
      <Show when={m().pace}>{(pace) => <p class="pace">{t(`reason.pace.${pace().kind}`, { vendor: vendorName(pace().vendor), other: m().vendor ? vendorName(m().vendor!) : t('model.auto') })}</p>}</Show>
      <QuotaCard id={props.conversationId} m={m()} usage={props.usage ?? []} />
      <Show when={steps()}>
        <details class="activity">
          <summary>{t(streaming() ? 'activity.working' : 'activity.done', { steps: t('activity.steps', { n: steps() }), commands: t('activity.commands', { n: commands() }) })}</summary>
          <div class="activity-body">
            <Show when={thinking().length}>
              <div class="activity-thinking">
                <For each={thinking()}>{(part) => <p>{part.title ? <b>{part.text}</b> : part.text}</p>}</For>
              </div>
            </Show>
            <For each={narration()}>{(text) => <div class="md" onClick={openLinks} innerHTML={html(text)} />}</For>
            <div class="tools">
              <For each={m().tools}>
                {(tool) => (
                  <span class={tool.ok === false ? 'bad' : ''} title={toolRow(tool).detail || tool.output}>
                    <Show when={tool.ok === true}>
                      <Icon name="check" small />
                    </Show>
                    <b>{toolRow(tool).label}</b>
                    <Show when={toolRow(tool).detail}><i>{middle(toolRow(tool).detail)}</i></Show>
                  </span>
                )}
              </For>
            </div>
          </div>
        </details>
        <Show when={streaming() && latest()}><div class="activity-latest" role="status" aria-live="polite" aria-atomic="true" title={latest()}>{latest()}</div></Show>
      </Show>
      <Asks conversationId={props.conversationId} turnId={m().id} permissions={waiting()} surface="ask" />
      <Parkeds parked={m().parked ?? []} more={m().parkedMore} surface="ask" />
      <div class="md" aria-busy={streaming()} onClick={openLinks}>
        <div innerHTML={html(finalText())} />
        <Show when={streaming() && !waiting().length && !m().waitUntil}>
          <span class="cursor" aria-hidden="true" />
        </Show>
      </div>
      <Show when={m().files?.length}>
        <div class="files" aria-label={t('files.label')} ref={animateList}>
          <For each={m().files}>{(f) => <FileCard conversationId={props.conversationId} file={f} />}</For>
        </div>
      </Show>
      <Show when={m().status === 'stopped'}>
        <div class="note">{t('answer.stopped')}</div>
      </Show>
      <Show when={m().error}>
        <div class="note err">{fromEngine(m().error, { when: m().waitUntil ? untilReset(m().waitUntil!) : t('quota.unknown') })}</div>
      </Show>
      <Show when={m().incomplete}><p class="note">{t('answer.meterIncomplete')}</p></Show>
      <Show when={!streaming()}>
        <div class="actions">
          <button onClick={() => void navigator.clipboard.writeText(m().text)}>{t('answer.copy')}</button>
          <Show when={props.prompt && m().nudge} keyed>
            {(nudge) => (
              <button
                class="nudge"
                title={fromEngine(nudge.reason, { to: `${prettyModel(nudge.model)} · ${t(`effort.${nudge.effort}`)}` })}
                onClick={() => props.onBranch(props.prompt!.id, props.prompt!.text, rung(nudge, props.options))}
              >
                {t('answer.nudge')}
              </button>
            )}
          </Show>
          <Show when={props.prompt} keyed>
            {(prompt) => (
              <select
                class="redo"
                aria-label={t('answer.redoLabel')}
                onChange={(e) => {
                  const picked = props.options[Number(e.currentTarget.value)]!.choice;
                  e.currentTarget.value = '';
                  props.onBranch(prompt.id, prompt.text, picked);
                }}
              >
                <option value="" disabled selected>
                  {t('answer.redo')}
                </option>
                <For each={props.options}>{(o, i) => <option value={i()}>{o.label}</option>}</For>
              </select>
            )}
          </Show>
          <button onClick={() => void api.openTerminal(props.conversationId)} title={t('answer.terminalHint')}>
            {t('answer.terminal')}
          </button>
          <span>{[usage(m()), warm()].filter(Boolean).join(' · ')}</span>
        </div>
      </Show>
    </article>
  );
}

// The user's message; "Edit" sends the new text as a branch, keeping the original conversation as it was.
function Mine(props: { m: Accessor<Message>; onBranch: OnBranch }) {
  const [editing, setEditing] = createSignal<string>();
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
          if (text.trim()) props.onBranch(props.m().id, text);
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
export function Thread(props: Shared & { messages: Message[]; queued?: QueuedTurn[]; busy?: boolean; onError?: (err: unknown) => void; from: number }) {
  const prompt = (at: number) => (props.messages[at - 1]?.role === 'user' ? props.messages[at - 1] : undefined);
  return (
    <>
      <Index each={props.messages.slice(props.from)}>
        {(m, i) => (
          <Show when={m().role === 'user'} fallback={<Answer {...props} m={m} prompt={prompt(props.from + i)} last={props.from + i === props.messages.length - 1} />}>
            <Mine m={m} onBranch={props.onBranch} />
          </Show>
        )}
      </Index>
      <For each={props.queued}>{(q) => <Queued conversationId={props.conversationId} q={q} busy={props.busy ?? false} onError={props.onError} />}</For>
    </>
  );
}
