import { createEffect, createSignal, on, Show } from 'solid-js';
import { api, type SharingState } from './api.ts';
import { Group, Opt, Toggle } from './controls.tsx';
import { Stage, Title } from './FirstRun.tsx';
import { failureText, t } from './i18n.ts';

/** Both buttons are equally available; no choice is made until one is pressed. */
export function SharingQuestion(props: { onPick: (yes: boolean) => Promise<void> }) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal('');
  const pick = async (yes: boolean) => {
    if (busy()) return;
    setBusy(true);
    setError('');
    try { await props.onPick(yes); } catch (err) { setError(failureText(err)); } finally { setBusy(false); }
  };
  return (
    <Stage labelledby="sharing-title">
      <div data-testid="sharing-question">
        <Title id="sharing-title" text={t('sharing.title')} />
        <p class="sub">{t('sharing.question')}</p>
        <p class="hint">{t('sharing.explain')}</p>
        <div class="setup-card-actions">
          <button type="button" class="btn" disabled={busy()} onClick={() => void pick(true)}>{t('sharing.yes')}</button>
          <button type="button" class="btn" disabled={busy()} onClick={() => void pick(false)}>{t('sharing.no')}</button>
        </div>
        <Show when={error()}><p class="note err" role="alert">{error()}</p></Show>
      </div>
    </Stage>
  );
}

export function SharingSettings(props: { state: SharingState; onChange: (yes: boolean) => Promise<boolean>; onError?: (error: unknown) => void }) {
  const [busy, setBusy] = createSignal(false);
  const [report, setReport] = createSignal<string>();
  createEffect(on(() => props.state.choice, () => setReport(undefined), { defer: true }));
  const change = async (yes: boolean) => {
    if (busy()) return false;
    setReport(undefined);
    setBusy(true);
    try {
      return await props.onChange(yes);
    } finally { setBusy(false); }
  };
  const show = async () => {
    if (report() !== undefined) { setReport(undefined); return; }
    setBusy(true);
    try { setReport((await api.sharingPreview()) ?? undefined); } catch (error) { props.onError?.(error); } finally { setBusy(false); }
  };
  return (
    <Group>
      <Toggle id="set-sharing" label={t('sharing.setting')} hint={t('sharing.hint')} more={t('sharing.more')} checked={props.state.choice === true} disabled={busy()} onChange={change} />
      <Show when={props.state.choice === true}>
        <Opt id="sharing-preview" label={t('sharing.show')}>
          <button type="button" class="btn" disabled={busy()} onClick={() => void show()}>{t(report() === undefined ? 'sharing.show' : 'sharing.hide')}</button>
        </Opt>
        <Show when={report() !== undefined}><pre data-testid="sharing-report" style={{ 'white-space': 'pre-wrap', 'overflow-wrap': 'anywhere' }}>{report()}</pre></Show>
      </Show>
    </Group>
  );
}
