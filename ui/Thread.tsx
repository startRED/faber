import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { createResource, createSignal, For, Index, Show, type Accessor } from 'solid-js';
import { api, type Choice, type Message, type Permission, type ToolUse } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import { prettyModel, type Option } from './models.ts';

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// Model output is untrusted: rendered as Markdown, then stripped of anything that could run.
const html = (text: string) => DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true }));

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

function toolLabel(tool: ToolUse): string {
  const input = tool.input as Record<string, unknown> | string | undefined;
  const detail = typeof input === 'string' ? input : input && (input.query ?? input.url ?? input.file_path ?? input.command ?? input.pattern);
  return detail ? `${tool.name}: ${String(detail)}` : tool.name;
}

// What a permission request means, in plain words.
function describe(tool: string, input: unknown): { title: string; note?: string; body: string } {
  const i = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
  const raw = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
  if (tool === 'Bash' || tool === 'command') {
    return { title: t('ask.command'), note: typeof i.description === 'string' ? i.description : undefined, body: typeof input === 'string' ? input : String(i.command ?? raw) };
  }
  if (['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool)) return { title: t('ask.file'), body: String(i.file_path ?? i.path ?? raw) };
  if (tool === 'WebFetch') return { title: t('ask.page'), body: String(i.url ?? raw) };
  return { title: t('ask.other', { tool }), body: raw };
}

function usage(m: Message): string {
  const parts: string[] = [];
  if (m.costUsd) parts.push(t('answer.cost', { cost: m.costUsd.toFixed(4) }));
  if (m.inputTokens) parts.push(t('answer.tokens', { input: (m.inputTokens / 1000).toFixed(1), output: m.outputTokens ?? 0 }));
  return parts.join(' · ');
}

type OnAnswer = (requestId: string, allow: boolean, always?: boolean) => void;
type OnBranch = (messageId: string, text: string, choice?: Choice) => void;

interface Shared {
  conversationId: string;
  options: Option[];
  onAnswer: OnAnswer;
  onBranch: OnBranch;
}

function Ask(props: { permission: Permission; onAnswer: OnAnswer }) {
  const d = () => describe(props.permission.tool, props.permission.input);
  return (
    <div class="ask" role="alertdialog" aria-label={t('ask.label')}>
      <b>{d().title}</b>
      <Show when={d().note}>
        <span class="note">{d().note}</span>
      </Show>
      <pre>{d().body}</pre>
      <div>
        <button class="btn primary" onClick={() => props.onAnswer(props.permission.requestId, true)}>
          {t('ask.allow')}
        </button>
        <button class="btn" onClick={() => props.onAnswer(props.permission.requestId, true, true)}>
          {t('ask.always')}
        </button>
        <button class="btn" onClick={() => props.onAnswer(props.permission.requestId, false)}>
          {t('ask.deny')}
        </button>
      </div>
    </div>
  );
}

function Answer(props: Shared & { m: Accessor<Message>; prompt?: Message }) {
  const m = () => props.m();
  const streaming = () => m().status === 'streaming';
  const waiting = () => m().permissions ?? [];
  return (
    <article class="ai" aria-label={t('answer.label')} aria-busy={streaming()}>
      <div class="model">
        <b>{prettyModel(m().model)}</b>
        <Show when={m().reason}>
          <em>· {fromEngine(m().reason)}</em>
        </Show>
      </div>
      <Show when={m().thinking}>
        <details class="thinking">
          <summary>{t('answer.thinking')}</summary>
          <div>{m().thinking}</div>
        </details>
      </Show>
      <Show when={m().tools?.length}>
        <div class="tools">
          <For each={m().tools}>
            {(tool) => (
              <span class={tool.ok === false ? 'bad' : ''} title={tool.output ?? toolLabel(tool)}>
                {toolLabel(tool)}
              </span>
            )}
          </For>
        </div>
      </Show>
      <Show when={waiting()[0]} keyed>
        {(p) => <Ask permission={p} onAnswer={props.onAnswer} />}
      </Show>
      <Show when={waiting().length > 1}>
        <div class="note">{t('ask.moreWaiting', { n: waiting().length - 1 })}</div>
      </Show>
      <div class="md" onClick={openLinks}>
        <div innerHTML={html(m().text)} />
        <Show when={streaming() && !waiting().length}>
          <span class="cursor" aria-hidden="true" />
        </Show>
      </div>
      <Show when={m().files?.length}>
        <div class="files" aria-label={t('files.label')}>
          <For each={m().files}>{(f) => <FileCard conversationId={props.conversationId} file={f} />}</For>
        </div>
      </Show>
      <Show when={m().status === 'stopped'}>
        <div class="note">{t('answer.stopped')}</div>
      </Show>
      <Show when={m().error}>
        <div class="note err">{fromEngine(m().error)}</div>
      </Show>
      <Show when={!streaming()}>
        <div class="actions">
          <button onClick={() => void navigator.clipboard.writeText(m().text)}>{t('answer.copy')}</button>
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
          <span>{usage(m())}</span>
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
        <div class="you">
          <For each={props.m().images}>{(img) => <img alt={t('mine.image')} src={`data:${img.mediaType};base64,${img.base64}`} />}</For>
          <For each={props.m().attachments}>{(name) => <span class="chip attached">{name}</span>}</For>
          {props.m().text}
          <button class="edit" onClick={() => setEditing(props.m().text)} aria-label={t('mine.editLabel')}>
            {t('mine.edit')}
          </button>
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

// Messages only grow at the end, so each position keeps its component while the answer streams into it.
export function Thread(props: Shared & { messages: Message[] }) {
  return (
    <Index each={props.messages}>
      {(m, i) => (
        <Show when={m().role === 'user'} fallback={<Answer {...props} m={m} prompt={props.messages[i - 1]?.role === 'user' ? props.messages[i - 1] : undefined} />}>
          <Mine m={m} onBranch={props.onBranch} />
        </Show>
      )}
    </Index>
  );
}
