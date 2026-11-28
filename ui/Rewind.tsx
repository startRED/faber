import { createEffect, createResource, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api, type RewindPoint, type RewindRedoInfo, type RewindRequest, type RewindResult } from './api.ts';
import { ChangesPreview } from './Changes.tsx';
import { StopAsk } from './Composer.tsx';
import { keepFocusIn } from './focus.ts';
import { failureText, fromEngine, lang, t } from './i18n.ts';
import { pressedAny } from './keys.ts';

/** A restore is always a deliberate confirmation; opening the preview never stops work or changes files. */
export function Rewind(props: { conversationId: string; messageId: string; point?: RewindPoint; redo?: RewindRedoInfo; conversationNames?: Record<string, string>; leaving?: boolean; onClose: () => void; onChanged: () => void | Promise<void> }) {
  const [mode, setMode] = createSignal<RewindRequest['mode']>(props.redo?.mode ?? 'both');
  const [saved, setSaved] = createSignal<string[]>([]);
  const [working, setWorking] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  const [result, setResult] = createSignal<RewindResult>();
  const [stopping, setStopping] = createSignal<string>();
  const [recovered, setRecovered] = createSignal(false);
  const newExpiry = Date.now() + 30 * 24 * 60 * 60 * 1000;
  let dialog: HTMLDialogElement | undefined;
  let cancel: HTMLButtonElement | undefined;
  let generation = 0;
  const request = (): RewindRequest => ({ conversationId: props.conversationId, messageId: props.messageId, mode: mode(), savedVersions: saved() });
  const [preview, { refetch }] = createResource(() => `${props.conversationId}:${props.messageId}:${mode()}:${props.redo?.id}`, async () => {
    const ticket = ++generation;
    setSaved([]);
    try {
      return props.redo ? await api.rewindRedoPreview({ conversationId: props.conversationId, id: props.redo.id, savedVersions: [] }) : await api.rewindPreview({ ...request(), savedVersions: [] });
    } catch (err) {
      if (ticket === generation) setError(err);
      return undefined;
    }
  });
  const [points] = createResource(() => props.conversationId, (id) => api.rewindPoints(id).catch(() => []));
  const point = () => points()?.find((p) => p.messageId === props.messageId);
  const expired = () => props.redo ? !props.redo.recovery && props.redo.expiresAt <= Date.now() : !!props.point && !props.point.coverage.unavailable && !!point()?.coverage.unavailable;
  const missing = () => mode() !== 'conversation' && (preview()?.coverage.unavailable || !props.point && !point() && !props.redo);
  const blocked = () => !!preview()?.running.length;
  const recovery = () => !recovered() && (!!preview()?.recovery || !!result()?.recovery || !!props.redo?.recovery);
  const ready = () => !!preview() && !preview.loading && !working() && !result() && !blocked() && !recovery() && !(props.redo && expired()) && !(mode() === 'files' && (missing() || !preview()?.files.some((f) => !f.unavailable)));
  const refresh = () => { if (!working()) { setError(undefined); void refetch(); } };
  const finish = () => { if (!working()) props.onClose(); };
  onMount(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    dialog?.showModal();
    cancel?.focus();
    // The shell's shortcuts must not open another surface over a native modal dialog.
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (stopping()) setStopping(undefined);
        else finish();
      } else if (e.key === 'Tab') keepFocusIn(e, dialog);
      else if (pressedAny(e)) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener('keydown', key, true);
    onCleanup(() => {
      window.removeEventListener('keydown', key, true);
      dialog?.close();
      queueMicrotask(() => {
        if (before?.isConnected) before.focus({ preventScroll: true });
        else document.querySelector<HTMLElement>('.rewind-empty, .mine, .composer textarea')?.focus({ preventScroll: true });
      });
    });
  });
  // A changed scope invalidates every earlier per-file consent, even if its path appears in both previews.
  createEffect(() => { mode(); setStopping(undefined); setError(undefined); });
  createEffect(() => { if (result() && !working()) queueMicrotask(() => cancel?.focus()); });
  const restore = async () => {
    if (!ready()) return;
    setWorking(true);
    setError(undefined);
    try {
      const next = props.redo ? await api.rewindRedo({ conversationId: props.conversationId, id: props.redo.id, savedVersions: saved() }) : await api.rewindRestore(request());
      setResult(next);
      await props.onChanged();
    } catch (err) {
      setError(err);
      // Admission checks run again in the engine; work can start after a preview was read.
      void refetch();
    } finally { setWorking(false); }
  };
  const recover = async () => {
    if (working()) return;
    setWorking(true);
    setError(undefined);
    try {
      await api.rewindRecover(props.conversationId);
      setRecovered(true);
      setResult((old) => ({ id: props.redo?.id ?? '', restored: [], kept: [], failed: [], partial: true, ...old, recovery: false, notice: 'note.rewind.savedMessages' }));
      await props.onChanged();
    } catch (err) { setError(err); }
    finally { setWorking(false); }
  };
  const stop = async (id: string) => {
    if (working()) return;
    setWorking(true);
    setError(undefined);
    setStopping(undefined);
    try { await api.interrupt(id); await refetch(); }
    catch (err) { setError(err); }
    finally { setWorking(false); }
  };
  const writer = (name: string) => {
    const key = name.replace(/^helper:/, '');
    if (key === 'turn') return t('rewind.writer.turn');
    if (key === 'checks') return t('rewind.writer.checks');
    if (key === 'admission') return t('rewind.writer.admission');
    if (key === 'compaction') return t('rewind.writer.compaction');
    if (key === 'process-tree' || key.startsWith('process-tree:')) return t('rewind.writer.background');
    return name.startsWith('helper') || key === 'helpers' ? t('rewind.writer.helpers') : t('rewind.writer.command');
  };
  return (
    <Portal>
      <dialog ref={(el) => (dialog = el)} class="rewind-confirm" classList={{ leaving: props.leaving }} inert={props.leaving} aria-labelledby="rewind-title" aria-modal="true" onCancel={(e) => { e.preventDefault(); finish(); }}>
        <h2 id="rewind-title" class="display">{t(recovery() ? 'rewind.recovery' : props.redo ? 'rewind.redo' : 'rewind.title')}</h2>
        <fieldset class="rewind-scopes" disabled={working() || !!result() || !!props.redo}>
          <legend>{t('rewind.scope')}</legend>
          <For each={['files', 'conversation', 'both'] as const}>{(scope) => <label><input type="radio" name="rewind-scope" value={scope} checked={mode() === scope} onChange={() => setMode(scope)} />{t(`rewind.${scope}`)}</label>}</For>
        </fieldset>
        <div class="rewind-state" role="status" aria-live="polite" aria-atomic="true">
          <Show when={working()} fallback={<Show when={preview.loading}><p>{t('rewind.preparing')}</p></Show>}><p>{t('rewind.restoring')}</p></Show>
          <Show when={recovery()} fallback={<Show when={result()}>{(done) => <p>{t(done().partial ? 'rewind.partial' : 'rewind.done')}</p>}</Show>}><p>{t('rewind.recovery')}</p></Show>
          <Show when={!preview.loading && mode() !== 'conversation'}>
            <Show when={missing() || expired()}><p>{t(expired() ? 'rewind.expired' : 'rewind.unavailable')}</p></Show>
            <Show when={preview()?.coverage.incomplete || preview()?.coverage.skipped}><p>{t('rewind.coverage')}</p></Show>
            <Show when={preview() && !preview()?.files.length && !missing() && !recovery()}><p>{t('rewind.noFiles')}</p></Show>
          </Show>
          <Show when={result()?.notice}>{(notice) => <p>{fromEngine(notice())}</p>}</Show>
        </div>
        <Show when={error()}>{(err) => <p class="note err" role="alert">{failureText(err())}</p>}</Show>
        <Show when={!preview.loading ? preview() : undefined}>{(p) => <>
          <Show when={!recovery()}><p class="side-meta">{t('rewind.counts', { files: p().files.length, messages: p().messages })}</p></Show>
          <ul class="rewind-counts">
            <Show when={p().replacements}><li>{t('rewind.replacements', { n: p().replacements })}</li></Show>
            <Show when={p().recreations}><li>{t('rewind.recreations', { n: p().recreations })}</li></Show>
            <Show when={p().removals}><li>{t('rewind.removals', { n: p().removals })}</li></Show>
            <Show when={p().conflicts}><li>{t('rewind.conflicts', { n: p().conflicts })}</li></Show>
          </ul>
          <div class="rewind-files" inert={working() || !!result()}>
            <ChangesPreview files={p().files} summary={(file) => <span class="change-counts">{file.unavailable ? t('rewind.unavailable') : file.conflict ? t('rewind.conflicts', { n: 1 }) : t(`rewind.fileAction.${file.action}`)}</span>}>
              {(file) => <>
                <p class="side-meta">{t(`rewind.fileAction.${file.action}`)}</p>
                <Show when={file.unavailable} fallback={<Show when={file.conflict}><label class="rewind-conflict"><span>{file.path}</span><select aria-label={file.path} value={saved().includes(file.path) ? 'saved' : 'keep'} onChange={(e) => setSaved((paths) => e.currentTarget.value === 'saved' ? [...paths.filter((path) => path !== file.path), file.path] : paths.filter((path) => path !== file.path))}><option value="keep">{t('rewind.keep')}</option><option value="saved">{t('rewind.saved')}</option></select></label></Show>}><p>{t('rewind.coverage')}</p></Show>
              </>}
            </ChangesPreview>
          </div>
          <Show when={p().running.length}>
            <p>{t('rewind.running')}</p>
            <For each={p().running}>{(run) => <div class="rewind-running">
              <p>{props.conversationNames?.[run.conversationId] ?? run.conversationId}: {[...new Set(run.writers.map(writer))].join(', ')}</p>
              <Show when={run.unknown}><p class="hint">{t('error.rewind.stopUnconfirmed')}</p></Show>
              <Show when={stopping() === run.conversationId} fallback={<button type="button" class="btn" disabled={working()} onClick={() => setStopping(run.conversationId)}>{t('composer.stop')}</button>}>
                <StopAsk helpers={run.writers.filter((w) => w.includes('helper')).length} onStop={() => void stop(run.conversationId)} onKeep={() => setStopping(undefined)} back={() => cancel} />
              </Show>
            </div>}</For>
          </Show>
          <Show when={!props.redo?.recovery}><p class="side-meta">{t('rewind.redoExpiry', { when: new Intl.DateTimeFormat(lang(), { dateStyle: 'medium', timeStyle: 'short' }).format(props.redo?.expiresAt ?? newExpiry) })}</p></Show>
        </>}</Show>
        <details class="disclosure rewind-limits"><summary>{t('rewind.details')}</summary><p>{t('rewind.limits')}</p><p>{t('rewind.exclusions')}</p></details>
        <p class="side-meta">{t('rewind.oldHandle')}</p>
        <Show when={recovery()}><button type="button" class="btn" disabled={working()} onClick={() => void recover()}>{t('rewind.recover')}</button></Show>
        <div class="rewind-actions">
          <Show when={!result()}><button type="button" class="btn primary" disabled={!ready()} onClick={() => void restore()}>{t(props.redo ? 'rewind.redo' : 'rewind.go')}</button></Show>
          <Show when={blocked() || error()}><button type="button" class="btn" disabled={working() || preview.loading} onClick={refresh}>{t('rewind.refresh')}</button></Show>
          <button type="button" class="btn" ref={(el) => (cancel = el)} disabled={working()} onClick={finish}>{t('rewind.cancel')}</button>
        </div>
      </dialog>
    </Portal>
  );
}
