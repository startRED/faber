import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, type CheckResult, type Vendor, type VendorUsage } from './api.ts';
import { Fold } from './controls.tsx';
import { VendorMark } from './Glyphs.tsx';
import { fromEngine, lang, money, percent, t } from './i18n.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { accountLine, freshness, type Quotas, usageRows } from './sidebar.ts';
import { ageWords, untilReset, windowName } from './Sidebar.tsx';

// Settings, Accounts and limits: one card per AI with its state, its plan, one bar per quota window and Faber's own spend.
// Every note about how the numbers are read sits behind one "Details" fold; nothing here asks the person to act, except the
// plan the CLI could not name.

const VENDORS: Vendor[] = ['claude', 'codex'];
const WORDS = { checking: 'set.checking', connected: 'set.connected', notConnected: 'set.notConnected', unsure: 'set.unsure' } as const;
/** Amber only near a real limit (C002): running low or out. Fast use is said calmly. */
const WARN: ReadonlySet<VendorUsage['state']> = new Set(['low', 'out']);

/** A plan as the CLI names it ("max") with its first letter capital, in the person's language. */
export const planName = (plan: string) => plan.charAt(0).toLocaleUpperCase(lang()) + plan.slice(1);

/** The question a card asks when the CLI cannot tell the plan. */
function PlanAsk(props: { vendor: Vendor }) {
  const [plan, setPlan] = createSignal('');
  const [error, setError] = createSignal<string>();
  return (
    <>
      <form
        class="plan-form"
        onSubmit={(e) => {
          e.preventDefault();
          void api.answerPlan(props.vendor, plan()).catch((err: unknown) => setError(fromEngine(String(err))));
        }}
      >
        <label>
          {t('usage.planAsk')}
          <input class="field" value={plan()} maxLength={40} autocomplete="off" onInput={(e) => setPlan(e.currentTarget.value)} />
        </label>
        <button class="btn" disabled={!plan().trim()}>
          {t('usage.planSave')}
        </button>
      </form>
      <Show when={error()}>
        <p class="note err">{error()}</p>
      </Show>
    </>
  );
}

/** The notes about how the figures are read, in the order a person would want them. */
function notes(u: VendorUsage): string[] {
  const out: string[] = [];
  if (u.counting) out.push(t('quota.countingNote'));
  out.push(t('usage.shared'));
  if (u.unmetered) out.push(t('usage.meterIncomplete', { n: u.unmetered }));
  if (u.relearning) out.push(t('usage.relearning'));
  else if (u.learning) out.push(t('usage.learning'));
  if (u.foreignUse) out.push(t('usage.foreign'));
  if (u.outsideShare != null) out.push(t('usage.outside', { share: percent(u.outsideShare) }));
  if (u.overageOn) out.push(t('usage.overageOn'));
  if (u.billedPerUse) out.push(t('usage.billedPerUse'));
  if (u.billingUnknown) out.push(t('usage.authNeeded'));
  return out;
}

function Card(props: { vendor: Vendor; check: CheckResult | 'failed' | undefined; shown: boolean; quotas: Quotas; usage?: VendorUsage[]; now: number }) {
  const v = () => props.vendor;
  const line = () => accountLine(props.check);
  const u = () => props.usage?.find((x) => x.vendor === v());
  const rows = () => usageRows(props.quotas, v(), props.now);
  // A sign-in billed per use, or one the CLI has not confirmed, is refused by Auto and reads "out" in the engine, but it has no
  // quota to run out of: say what it is, calmly (C002).
  const billing = () => (u()?.billedPerUse ? 'billed' : u()?.billingUnknown ? 'unconfirmed' : undefined);
  const word = () => (line() === 'connected' && u() ? t(`usage.pill.${billing() ?? u()!.state}`) : t(WORDS[line()]));
  const warn = () => line() === 'connected' && !!u() && !billing() && WARN.has(u()!.state);
  return (
    <section class="st-card acct" data-vendor={v()} data-state={line()} aria-labelledby={`acct-${v()}-name`} ref={animateList}>
      <header class="acct-head" ref={animateList}>
        <h3 class="vm" id={`acct-${v()}-name`}>
          <VendorMark vendor={v()} />
          {vendorName(v())}
        </h3>
        <Show when={props.shown && u()}>{(x) => <span class="acct-plan">{x().plan ? t('usage.plan', { plan: planName(x().plan!) }) : t('usage.planUnknown')}</span>}</Show>
        <span class="acct-pill" classList={{ on: line() === 'connected', warn: warn() }} title={u() && line() === 'connected' ? (billing() ? t(billing() === 'billed' ? 'usage.billedPerUse' : 'usage.authNeeded') : t(`usage.state.${u()!.state}`)) : undefined}>
          {word()}
        </span>
      </header>
      <Show when={props.shown && u()}>
        {(x) => (
          <>
            <Show when={x().askForPlan}>
              <div class="acct-ask">
                <PlanAsk vendor={v()} />
              </div>
            </Show>
            <div class="acct-rows" ref={animateList}>
              <For each={rows()}>
                {(row) => {
                  const w = () => x().windows.find((win) => win.name === row.window);
                  // A share that may still rise reads "at least" (C010); the long explanation stays in Details.
                  const fresh = () => freshness(props.usage, v(), row.window);
                  const open = () => fresh().counting || !!fresh().uncountedPoints;
                  return (
                    <div class="urow">
                      <span class="uname">{row.window ? t('quota.window', { window: windowName(row.window) }) : t('usage.noReading')}</span>
                      <span class="share num">
                        {row.used === undefined ? t('quota.unknown') : open() ? t('quota.atLeast', { share: percent(row.used) }) : percent(row.used)}
                        <Show when={row.used !== undefined && fresh().counting}>
                          <span class="sr-only"> {t('quota.counting')}</span>
                        </Show>
                      </span>
                      <span class="meter" aria-hidden="true">
                        <Show when={w()}>{(win) => <b class="expected" style={{ left: `${Math.round(win().expected * 100)}%` }} />}</Show>
                        <i style={{ 'clip-path': `inset(0 ${100 - Math.round(Math.min(1, Math.max(0, row.used ?? 0)) * 100)}% 0 0)` }} />
                      </span>
                      <span class="reset">{row.used !== undefined && row.resetsAt ? t('usage.resets', { when: untilReset(row.resetsAt) }) : ''}</span>
                    </div>
                  );
                }}
              </For>
            </div>
            <p class="acct-foot">{t('usage.ownSpend', { cost: money(x().ownSpendUsd) })}</p>
            <Fold id={`acct-${v()}-details`} title={t('usage.details')}>
              <div class="acct-details">
                <For each={notes(x())}>{(text) => <p class="hint">{text}</p>}</For>
                <For each={rows().filter((row) => row.window)}>
                  {(row) => {
                    const w = () => x().windows.find((win) => win.name === row.window);
                    const fresh = () => freshness(props.usage, v(), row.window);
                    return (
                      <p class="hint wnote">
                        <b>{t('quota.window', { window: windowName(row.window!) })}</b>
                        <Show when={w()}>
                          {(win) => <span class="projection">{t('usage.expect', { share: percent((win().used + (win().uncounted ?? 0)) / Math.max(win().elapsed, 0.1)) })}</span>}
                        </Show>
                        <Show when={fresh().readAt !== undefined}>
                          <span class="age">{t('quota.read', { when: ageWords(fresh().readAt!, props.now) })}</span>
                        </Show>
                        <Show when={fresh().uncountedPoints}>{(n) => <span class="uncounted">{t('quota.uncounted', { share: `${n()}%` })}</span>}</Show>
                      </p>
                    );
                  }}
                </For>
              </div>
            </Fold>
          </>
        )}
      </Show>
    </section>
  );
}

export function AccountsSection(props: { vendors: Vendor[]; quotas: Quotas; usage?: VendorUsage[]; now: number; onManage: () => void }) {
  // Whether each CLI says someone is signed in. Asked every time the section opens, so it is fresh after the account cards.
  const [checks, setChecks] = createSignal<Partial<Record<Vendor, CheckResult | 'failed'>>>({});
  let gone = false;
  onCleanup(() => (gone = true));
  onMount(() => {
    for (const v of VENDORS) {
      void api.setupCheck(v).then(
        (res) => !gone && setChecks((prev) => ({ ...prev, [v]: res })),
        () => !gone && setChecks((prev) => ({ ...prev, [v]: 'failed' })),
      );
    }
  });
  return (
    <>
      <div class="acct-top">
        <p class="hint">{t('set.accountsHint')}</p>
        <button type="button" class="btn" data-restore="manage" onClick={() => props.onManage()}>
          {t('set.openSetup')}
        </button>
      </div>
      <For each={VENDORS}>
        {(v) => <Card vendor={v} check={checks()[v]} shown={props.vendors.includes(v)} quotas={props.quotas} usage={props.usage} now={props.now} />}
      </For>
    </>
  );
}
