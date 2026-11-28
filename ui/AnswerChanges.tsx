import { createEffect, createMemo, createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type AnswerChanges as Review, type AnswerChangePatch as ReviewPatch, type FileChange, type RewindPoint } from './api.ts';
import { Delta } from './AgentChat.tsx';
import { ChangesPreview } from './Changes.tsx';
import { failureText, t } from './i18n.ts';

const FILES = 50;

function finishRetry(button: HTMLButtonElement | undefined) {
  if (button && document.activeElement === button) button.closest('details')?.querySelector<HTMLElement>(':scope > summary')?.focus();
}

function Patch(props: { id: string; message: string; file: FileChange }) {
  const [offsets, setOffsets] = createSignal([0]);
  const offset = () => offsets().at(-1)!;
  const [page, setPage] = createSignal<ReviewPatch>();
  const [total, setTotal] = createSignal(0);
  const [error, setError] = createSignal<string>();
  const [loading, setLoading] = createSignal(false);
  const [attempt, retry] = createSignal(0);
  let retryButton: HTMLButtonElement | undefined;
  createEffect(() => {
    const at = offset(); attempt();
    let current = true;
    setPage(undefined); setLoading(true);
    void api.answerChangePatch(props.id, props.message, props.file.path, at).then(
      (value) => { if (current) { finishRetry(retryButton); setTotal(value.total); setPage(value); setError(undefined); setLoading(false); } },
      (reason: unknown) => { if (current) { setError(failureText(reason)); setLoading(false); } },
    );
    onCleanup(() => { current = false; });
  });
  return <div class="answer-change-content">
    <Show when={!page() && !error()}><p class="side-meta" role="status">{t('changes.loading')}</p></Show>
    <Show when={error()}>{(reason) => <><p class="side-meta" role="status">{reason()}</p><button ref={retryButton} class="btn ghost" aria-disabled={loading()} onClick={() => { if (!loading()) retry((n) => n + 1); }}>{t(loading() ? 'changes.loading' : 'changes.retry')}</button></>}</Show>
    <Show when={page()}>{(data) => <>
      <Show when={data().notice}>{(key) => <p class="side-meta">{t(key())}</p>}</Show>
      <Show when={data().tooLarge}><p class="side-meta">{t('changes.tooLarge')}</p></Show>
      <Show when={data().binary}><p class="side-meta">{t('changes.binary')}</p></Show>
      <Show when={!data().tooLarge && !data().binary && !data().notice}>
        <Show when={data().lines.length} fallback={<p class="side-meta">{t('changes.empty')}</p>}>
          <pre class="change-patch answer-change-patch" role="region" tabIndex={0} aria-label={t('answerChanges.diff', { path: props.file.path })}><For each={data().lines}>{(line) => <span class="patch-line" classList={{ add: line.startsWith('+') && !line.startsWith('+++'), del: line.startsWith('-') && !line.startsWith('---'), hunk: line.startsWith('@@') }}>{line || ' '}</span>}</For></pre>
        </Show>
      </Show>
    </>}</Show>
    <Show when={total() > (page()?.lines.length ?? 0) || offset() > 0}>
      <nav class="answer-change-pages" aria-label={t('answerChanges.diffPages')}>
        <button class="btn ghost" data-answer-patch-previous aria-disabled={offsets().length === 1 || loading()} onClick={() => { if (!loading() && offsets().length > 1) setOffsets((before) => before.slice(0, -1)); }}>{t('answerChanges.previous')}</button>
        <span role="status">{page() ? t('answerChanges.lines', { first: offset() + 1, last: offset() + page()!.lines.length, total: total() }) : ''}</span>
        <button class="btn ghost" data-answer-patch-next aria-disabled={!page()?.lines.length || offset() + page()!.lines.length >= total()} onClick={() => { const count = page()?.lines.length; if (count && offset() + count < total()) setOffsets((before) => [...before, offset() + count]); }}>{t('answerChanges.next')}</button>
      </nav>
    </Show>
  </div>;
}

function Files(props: { id: string; message: string; total: number }) {
  const [offset, setOffset] = createSignal(0);
  const [page, setPage] = createSignal<Review>();
  const [error, setError] = createSignal<string>();
  const [loading, setLoading] = createSignal(false);
  const [attempt, retry] = createSignal(0);
  let retryButton: HTMLButtonElement | undefined;
  createEffect(() => {
    const at = offset(); attempt();
    let current = true;
    setPage(undefined); setLoading(true);
    void api.answerChanges(props.id, props.message, at).then(
      (value) => { if (current) { finishRetry(retryButton); setPage(value); setError(undefined); setLoading(false); } },
      (reason: unknown) => { if (current) { setError(failureText(reason)); setLoading(false); } },
    );
    onCleanup(() => { current = false; });
  });
  return <>
    <Show when={!page() && !error()}><p class="side-meta" role="status">{t('changes.loading')}</p></Show>
    <Show when={error()}>{(reason) => <><p class="side-meta" role="status">{reason()}</p><button ref={retryButton} class="btn ghost" aria-disabled={loading()} onClick={() => { if (!loading()) retry((n) => n + 1); }}>{t(loading() ? 'changes.loading' : 'changes.retry')}</button></>}</Show>
    <Show when={page()}>{(data) => <>
      <Show when={data().coverage.unavailable}><p class="side-meta">{t('rewind.unavailable')}</p></Show>
      <Show when={data().coverage.incomplete || data().coverage.skipped}><p class="side-meta">{t('rewind.coverage')}</p></Show>
      <Show when={data().files.length} fallback={!data().coverage.unavailable && <p class="side-meta">{t('changes.empty')}</p>}>
        <ChangesPreview files={data().files} summary={(file) => <Show when={file.countsKnown !== false && !file.binary} fallback={<span class="change-counts">{t(file.binary ? 'changes.binary' : 'changes.countsUnknown')}</span>}><Delta change={file} /></Show>}>
          {(file) => <Patch id={props.id} message={props.message} file={file} />}
        </ChangesPreview>
      </Show>
    </>}</Show>
    <Show when={props.total > FILES}>
      <nav class="answer-change-pages" aria-label={t('answerChanges.filePages')}>
        <button class="btn ghost" aria-disabled={offset() === 0 || loading()} onClick={() => { if (!loading() && offset() > 0) setOffset((n) => Math.max(0, n - FILES)); }}>{t('answerChanges.previous')}</button>
        <span role="status">{page() ? t('answerChanges.range', { first: offset() + 1, last: Math.min(offset() + FILES, page()!.total), total: page()!.total }) : ''}</span>
        <button class="btn ghost" data-answer-files-next aria-disabled={!page() || offset() + FILES >= page()!.total} onClick={() => { if (page() && offset() + FILES < page()!.total) setOffset((n) => n + FILES); }}>{t('answerChanges.next')}</button>
      </nav>
    </Show>
  </>;
}

/** One bounded metadata read discovers the card. File rows and saved patch pages live only while opened. */
export function AnswerChanges(props: { conversationId: string; messageId: string; point: RewindPoint }) {
  const [open, setOpen] = createSignal(false);
  const [metadata, setMetadata] = createSignal<Omit<Review, 'files'>>();
  const [error, setError] = createSignal<{ text: string; unavailable: boolean }>();
  const [loading, setLoading] = createSignal(false);
  const [attempt, retry] = createSignal(0);
  let retryButton: HTMLButtonElement | undefined;
  const identity = createMemo(() => `${props.conversationId}:${props.messageId}:${props.point.id}`);
  createEffect(() => { identity(); setOpen(false); setMetadata(undefined); setError(undefined); });
  const request = createMemo(() => `${identity()}:${props.point.sealed}:${props.point.coverage.unavailable}:${props.point.coverage.incomplete}`);
  createEffect(() => {
    request(); attempt();
    let current = true;
    setLoading(true);
    void api.answerChanges(props.conversationId, props.messageId, 0).then(
      ({ files: _files, ...summary }) => { if (current) { finishRetry(retryButton); setMetadata(summary); setError(undefined); setLoading(false); } },
      (reason: unknown) => { if (current) { setError({ text: failureText(reason), unavailable: reason === 'error.rewind.unavailable' }); setLoading(false); } },
    );
    onCleanup(() => { current = false; });
  });
  const visible = () => metadata() && (metadata()!.sealed && (metadata()!.total > 0 || metadata()!.coverage.incomplete) || metadata()!.coverage.unavailable) || error();
  return <Show when={visible()}>
    <details class="answer-changes disclosure" open={open()} onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>{metadata()?.total ? t('answerChanges.files', { n: metadata()!.total }) : t('answerChanges.title')}</summary>
      <Show when={open()}>
        <div class="answer-change-body">
          <Show when={metadata()?.shared}><p class="side-meta">{t('answerChanges.shared')}</p></Show>
          <Show when={error()}>{(reason) => <><p class="side-meta" role="status">{reason().text}</p><Show when={!reason().unavailable}><button ref={retryButton} class="btn ghost" aria-disabled={loading()} onClick={() => { if (!loading()) retry((n) => n + 1); }}>{t(loading() ? 'changes.loading' : 'changes.retry')}</button></Show></>}</Show>
          <Show when={!error() && metadata()}>{(data) => <Files id={props.conversationId} message={props.messageId} total={data().total} />}</Show>
        </div>
      </Show>
    </details>
  </Show>;
}
