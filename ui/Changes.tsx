import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api, type FileChange } from './api.ts';
import { Breakable, Delta } from './AgentChat.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

/** Mounted only while the file is open, so closing it releases its patch and its lines. */
function Patch(props: { id: string; file: FileChange }) {
  const [patch, { refetch }] = createResource(() => props.file.path, (path) => api.changePatch(props.id, path).catch(() => ({ text: '', tooLarge: false, binary: false, error: 'error.changes.read' })));
  const lines = createMemo(() => {
    const text = patch()?.text ?? '';
    const start = /^@@ /m.exec(text)?.index ?? -1;
    return start < 0 ? [] : text.slice(start).replace(/\n$/, '').split('\n');
  });
  return (
    <Show when={!patch.loading} fallback={<p class="side-meta" role="status">{t('changes.loading')}</p>}>
      <Show when={!patch()?.error} fallback={<><p class="side-meta" role="status">{t('error.changes.read')}</p><button class="btn ghost" onClick={() => void refetch()}>{t('changes.retry')}</button></>}>
        <Show when={!patch()?.tooLarge} fallback={<p class="side-meta">{t('changes.tooLarge')}</p>}>
          <Show when={!props.file.binary && !patch()?.binary} fallback={<p class="side-meta">{t('changes.binary')}</p>}>
            <Show when={lines().length} fallback={<p class="side-meta">{t('changes.empty')}</p>}>
              <pre class="change-patch"><For each={lines()}>{(line) => <span class="patch-line" classList={{ add: line.startsWith('+'), del: line.startsWith('-'), hunk: line.startsWith('@@') }}>{line || ' '}</span>}</For></pre>
            </Show>
          </Show>
        </Show>
      </Show>
    </Show>
  );
}

function ChangedFile(props: { id: string; file: FileChange; open: boolean; onToggle: (open: boolean) => void }) {
  return (
    <details class="disclosure change-file" open={props.open} onToggle={(event) => props.onToggle(event.currentTarget.open)}>
      <summary><span class="change-path"><Breakable text={props.file.path} /></span><Show when={props.file.countsKnown !== false && !props.file.binary} fallback={<span class="change-counts">{t(props.file.binary ? 'changes.binary' : 'changes.countsUnknown')}</span>}><Delta change={props.file} /></Show></summary>
      <Show when={props.open}><div class="change-content"><Show when={!props.file.tooLarge} fallback={<p class="side-meta">{t('changes.tooLarge')}</p>}><Patch id={props.id} file={props.file} /></Show></div></Show>
    </details>
  );
}

/** The conversation's whole change, in the running column's existing side view. */
export function Changes(props: { id: string; files: FileChange[]; loading: boolean; incomplete?: boolean; loadingMore?: boolean; onMore?: () => void; onReview?: (security: boolean) => void; onBack: () => void; onClose: () => void }) {
  const [open, setOpen] = createSignal<string>();
  return (
    <>
      <div class="rc-head side-head changes-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
        <h2 id="running-title" class="display">{t('changes.title')}</h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
        <Show when={props.files.length && props.onReview}>
          <div class="changes-review">
            <button class="btn primary" onClick={() => props.onReview?.(false)}>{t('changes.review')}</button>
            <button class="btn ghost" onClick={() => props.onReview?.(true)}>{t('changes.reviewSecurity')}</button>
          </div>
        </Show>
      </div>
      <div class="rc-body side-body changes-view">
        <Show when={!props.loading} fallback={<p class="side-meta" role="status">{t('changes.loading')}</p>}>
          <Show when={props.files.length} fallback={!props.incomplete && <p class="side-meta">{t('changes.empty')}</p>}>
            <For each={props.files}>{(file) => <ChangedFile id={props.id} file={file} open={open() === file.path} onToggle={(expanded) => setOpen((path) => expanded ? file.path : path === file.path ? undefined : path)} />}</For>
          </Show>
          <Show when={props.incomplete}><p class="side-meta" role="status">{t('changes.incomplete')}</p><button class="btn ghost changes-more" disabled={props.loadingMore} onClick={props.onMore}>{t(props.loadingMore ? 'changes.loading' : 'changes.more')}</button></Show>
        </Show>
      </div>
    </>
  );
}
