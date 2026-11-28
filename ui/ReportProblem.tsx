import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api } from './api.ts';
import { keepFocusIn } from './focus.ts';
import { failureText, lang, t } from './i18n.ts';
import { pressedAny } from './keys.ts';

type Report = Awaited<ReturnType<typeof api.problemReport>>;

function Preview(props: { report: Report; close: () => void }) {
  const [openError, setOpenError] = createSignal<unknown>();
  let dialog: HTMLDialogElement | undefined;
  let cancel: HTMLButtonElement | undefined;
  onMount(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    dialog?.showModal();
    cancel?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); props.close(); }
      else if (e.key === 'Tab') keepFocusIn(e, dialog);
      else if (pressedAny(e)) { e.preventDefault(); e.stopPropagation(); }
    };
    window.addEventListener('keydown', key, true);
    onCleanup(() => {
      window.removeEventListener('keydown', key, true);
      dialog?.close();
      queueMicrotask(() => before?.isConnected && before.focus({ preventScroll: true }));
    });
  });
  return <Portal><dialog ref={(el) => { dialog = el; }} class="cookies" aria-modal="true" aria-labelledby="report-title" onCancel={(e) => { e.preventDefault(); props.close(); }}>
    <h2 id="report-title" class="display">{t('report.title')}</h2>
    <p>{t('report.review')}</p>
    <p>{t('report.version')}: {props.report.version}</p>
    <pre style={{ 'white-space': 'pre-wrap', 'overflow-wrap': 'anywhere' }}>{props.report.environment}</pre>
    <Show when={openError()}><p role="alert">{failureText(openError())}</p></Show>
    <div class="cookies-actions">
      <button type="button" class="btn" ref={(el) => { cancel = el; }} onClick={props.close}>{t('report.cancel')}</button>
      <button type="button" class="btn primary" onClick={() => { void api.openLink(props.report.url).then(props.close, setOpenError); }}>{t('report.open')}</button>
    </div>
  </dialog></Portal>;
}

export function ReportProblem(props: { crash?: boolean }) {
  const [report, setReport] = createSignal<Report>();
  const [error, setError] = createSignal<unknown>();
  const [busy, setBusy] = createSignal(false);
  const prepare = async () => {
    setBusy(true); setError(undefined);
    try { setReport(await api.problemReport(lang(), props.crash === true)); }
    catch (err) { setError(err); }
    finally { setBusy(false); }
  };
  return <>
    <button type="button" class="btn" disabled={busy()} onClick={() => void prepare()}>{t('report.action')}</button>
    <Show when={error()}><span role="alert">{failureText(error())}</span></Show>
    <Show when={report()}>{(value) => <Preview report={value()} close={() => setReport(undefined)} />}</Show>
  </>;
}
