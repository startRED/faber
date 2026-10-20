import { createEffect, createMemo, createResource, For, Show } from 'solid-js';
import { api, type FileView } from './api.ts';
import { Breakable } from './AgentChat.tsx';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';

// A file a reply named (parity P2), opened in the running column like the changes view: read only, at most 4,000 lines around
// the line the agent pointed at, that line marked and scrolled into the middle. The engine reads it, inside the conversation's
// folder only; nothing is kept once it closes.

const failed = (key: string): FileView => ({ path: '', first: 1, text: '', total: 0, tooLarge: false, binary: false, error: key });

export function FilePane(props: { conversationId: string; at: string; onBack: () => void; onClose: () => void }) {
  // One string, so the file is read again only when another one opens, never on a change elsewhere in the conversation.
  const [view, { refetch }] = createResource(
    () => JSON.stringify([props.conversationId, props.at]),
    (key) => {
      const [id, at] = JSON.parse(key) as [string, string];
      return api.readFileAt(id, at).catch(() => failed('error.file.read'));
    },
  );
  const lines = createMemo(() => (view()?.text ? view()!.text.split('\n') : []));
  const name = () => (view()?.path || props.at).split(/[\\/]/).pop() || props.at;
  const marked = (n: number) => {
    const v = view();
    return !!v?.line && n >= v.line && n <= (v.end ?? v.line);
  };
  let body: HTMLDivElement | undefined;
  createEffect(() => {
    if (!lines().length || !view()?.line) return;
    queueMicrotask(() => body?.querySelector('.file-line.mark')?.scrollIntoView({ block: 'center' }));
  });
  const where = () => {
    const v = view();
    if (!v?.line) return '';
    return v.end ? t('file.lines', { from: v.line, to: v.end }) : t('file.line', { line: v.line });
  };
  const part = () => {
    const v = view();
    return v && v.total > lines().length ? t('file.part', { from: v.first, to: v.first + lines().length - 1, total: v.total }) : '';
  };
  return (
    <>
      <div class="rc-head side-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
        <h2 id="running-title" class="display" title={view()?.path || props.at}>{name()}</h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
      </div>
      {/* The path and its action stay above the lines, which scroll to the marked one. */}
      <Show when={view() && !view.loading && !view()!.error}>
        <div class="file-meta">
          <span class="side-meta change-path"><Breakable text={view()!.path} /></span>
          <Show when={where()}><span class="side-meta">{where()}</span></Show>
          <button class="btn ghost" onClick={() => void api.showFile(props.conversationId, view()!.path)}>{t('files.show')}</button>
        </div>
      </Show>
      <div class="rc-body side-body file-view" ref={(el) => (body = el)}>
        <Show when={!view.loading} fallback={<p class="side-meta" role="status">{t('file.loading')}</p>}>
          <Show
            when={!view()?.error}
            fallback={<>
              <p class="side-meta" role="status">{fromEngine(view()?.error ?? 'error.file.read')}</p>
              <Show when={view()?.error === 'error.file.read'}><button class="btn ghost" onClick={() => void refetch()}>{t('changes.retry')}</button></Show>
            </>}
          >
            <Show when={!view()!.tooLarge} fallback={<p class="side-meta">{t('file.tooLarge')}</p>}>
              <Show when={!view()!.binary} fallback={<p class="side-meta">{t('file.binary')}</p>}>
                <Show when={lines().length} fallback={<p class="side-meta">{t('file.empty')}</p>}>
                  <Show when={part()}><p class="side-meta">{part()}</p></Show>
                  <pre class="file-text" aria-label={view()!.path}>
                    <For each={lines()}>{(line, i) => <span class="file-line" classList={{ mark: marked(view()!.first + i()) }} data-n={view()!.first + i()}>{line || ' '}</span>}</For>
                  </pre>
                </Show>
              </Show>
            </Show>
          </Show>
        </Show>
      </div>
    </>
  );
}
