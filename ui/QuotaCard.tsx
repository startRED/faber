import { createSignal, Show } from 'solid-js';
import { api, type Message, type VendorUsage } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import { vendorName } from './models.ts';

/** The second click is consent for this reply only. No setting or next message inherits it. */
export function QuotaCard(props: { id: string; m: Message; usage: VendorUsage[] }) {
  const [confirming, setConfirming] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const vendor = () => props.m.vendor;
  const account = () => props.usage.find((u) => u.vendor === vendor());
  const pending = () => props.m.status !== 'done' && props.m.status !== 'stopped' && (props.m.reason === 'reason.waiting' || props.m.error === 'error.quotaOut' || props.m.error === 'error.quotaWait');
  const run = async (request: () => Promise<void>) => {
    if (sending()) return;
    setSending(true); setError(undefined);
    try { await request(); setConfirming(false); } catch (e) { setError(fromEngine(String(e))); }
    finally { setSending(false); }
  };
  return <Show when={pending()}>
    <div class="quota-card" aria-live="polite">
      <Show when={props.m.error === 'error.quotaOut'}>
        <p>{t('quota.chosen.out', { vendor: vendor() ? vendorName(vendor()!) : t('model.auto') })}</p>
        <button class="btn secondary" disabled={sending()} onClick={() => void run(() => api.moveTurn(props.id, props.m.id))}>{t('quota.chosen.move')}</button>
        <button class="btn secondary" disabled={sending()} onClick={() => void run(() => api.waitTurn(props.id, props.m.id))}>{t('quota.chosen.wait')}</button>
      </Show>
      <Show when={account()?.paidPossible && !account()?.billingUnknown && !account()?.billedPerUse && vendor()}>
        <Show when={confirming()} fallback={<button class="btn secondary" onClick={() => setConfirming(true)}>{t('quota.credits.offer')}</button>}>
          <p>{t('quota.credits.confirm', { vendor: vendorName(vendor()!) })}</p>
          <button class="btn" disabled={sending()} onClick={() => void run(() => api.approveCredits(props.id, props.m.id, vendor()!))}>{t('quota.credits.send')}</button>
          <button class="btn secondary" disabled={sending()} onClick={() => setConfirming(false)}>{t('mine.cancel')}</button>
        </Show>
      </Show>
      <Show when={props.m.status === 'streaming'}><button class="btn secondary" onClick={() => void api.interrupt(props.id)}>{t('composer.stop')}</button></Show>
      <Show when={error()}><p class="err">{error()}</p></Show>
    </div>
  </Show>;
}

export function PlanName(props: { u: VendorUsage }) {
  const [plan, setPlan] = createSignal('');
  const [error, setError] = createSignal<string>();
  return <>
    <p>{t('usage.plan', { plan: props.u.plan ?? t('usage.planUnknown') })}</p>
    <Show when={props.u.askForPlan}>
      <form class="plan-form" onSubmit={(e) => {
        e.preventDefault();
        void api.answerPlan(props.u.vendor, plan()).catch((e: unknown) => setError(fromEngine(String(e))));
      }}>
        <label>{t('usage.planAsk')}<input value={plan()} maxLength={40} onInput={(e) => setPlan(e.currentTarget.value)} /></label>
        <button class="btn" disabled={!plan().trim()}>{t('usage.planSave')}</button>
      </form>
    </Show>
    <Show when={error()}><p class="err">{error()}</p></Show>
  </>;
}
