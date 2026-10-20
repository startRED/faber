import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, errorText, type Vendor } from './api.ts';
import { Stage, Title } from './FirstRun.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { animateList } from './motion.ts';
import {
  OFFICIAL_PAGES,
  setupCard,
  toggleSubscription,
  type Check,
  type SetupMode,
  type SetupProgress,
  type Subscription,
} from './setup.ts';

export interface SetupProps {
  /** `firstRun` is step 1 of 3. `manage` is only the account cards with a Close button. */
  mode: SetupMode;
  /** Saves the setup decision. Resolving does not close anything: the screen decides what comes next. */
  onFinish: (result: 'done' | 'skipped') => void;
}

// An install outlives this screen: the shell keeps running it after Close. What it reached lives here, for the whole app,
// so the cards opened again show it still installing (first-user review, 2026-10-19) instead of "Not installed".
const [progresses, setProgresses] = createSignal<Partial<Record<Vendor, SetupProgress>>>({});
let listening = false;

/** Keeps the step an install reached; the shell's setup events land here. */
export function noteSetupProgress(e: SetupProgress) {
  setProgresses((prev) => ({ ...prev, [e.vendor]: e }));
}

const underway = (v: Vendor) => {
  const step = progresses()[v]?.step;
  return step !== undefined && step !== 'done' && step !== 'failed';
};

export function Setup(props: SetupProps) {
  // An AI still installing from an earlier visit is picked already, so its card shows where the install is.
  const [selected, setSelected] = createSignal<Subscription[]>((['claude', 'codex'] as const).filter(underway));
  const [checks, setChecks] = createSignal<Partial<Record<Vendor, Check>>>({});
  const [waitings, setWaitings] = createSignal<Partial<Record<Vendor, boolean>>>({});
  const [errors, setErrors] = createSignal<Partial<Record<Vendor, { key: string; detail?: string }>>>({});

  const checkVendor = async (v: Vendor) => {
    try {
      const res = await api.setupCheck(v);
      setChecks((prev) => ({ ...prev, [v]: res }));
      setErrors((prev) => ({ ...prev, [v]: undefined }));
      if (res.account === 'signedIn') {
        setWaitings((prev) => ({ ...prev, [v]: false }));
      }
    } catch (err) {
      setErrors((prev) => ({ ...prev, [v]: { key: 'error.setup.check', detail: errorText(err) } }));
    }
  };

  const installVendor = async (v: Vendor) => {
    noteSetupProgress({ type: 'setup', vendor: v, step: 'prerequisite' });
    setErrors((prev) => ({ ...prev, [v]: undefined }));
    try {
      await api.setupInstall(v);
    } catch (err) {
      setErrors((prev) => ({ ...prev, [v]: { key: 'error.setup.install', detail: errorText(err) } }));
    }
  };

  const signInVendor = async (v: Vendor) => {
    setWaitings((prev) => ({ ...prev, [v]: true }));
    setErrors((prev) => ({ ...prev, [v]: undefined }));
    try {
      await api.setupSignIn(v);
    } catch (err) {
      setErrors((prev) => ({ ...prev, [v]: { key: 'error.setup.signIn', detail: errorText(err) } }));
      setWaitings((prev) => ({ ...prev, [v]: false }));
    }
  };

  const onToggle = (option: Subscription) => {
    const next = toggleSubscription(selected(), option);
    setSelected(next);
    if (option !== 'none' && next.includes(option)) {
      void checkVendor(option as Vendor);
    }
  };

  onMount(() => {
    const onFocus = () => {
      for (const v of ['claude', 'codex'] as const) {
        if (selected().includes(v) && waitings()[v]) {
          void checkVendor(v);
        }
      }
    };
    window.addEventListener('focus', onFocus);
    if (!listening) {
      listening = true;
      void api.on((e) => {
        if (e.type === 'setup') noteSetupProgress(e);
      });
    }
    for (const v of selected()) if (v !== 'none') void checkVendor(v);
    const unlisten = api.on((e) => {
      if (e.type === 'setup') {
        if (e.step === 'done') {
          void checkVendor(e.vendor);
        }
      }
    });
    onCleanup(() => {
      window.removeEventListener('focus', onFocus);
      void unlisten.then((u) => u());
    });
  });

  const first = () => props.mode === 'firstRun';
  const choices: [Subscription, string][] = [
    ['claude', t('setup.claude')],
    ['codex', t('setup.chatgpt')],
    ['none', t('setup.none')],
  ];

  return (
    <Stage step={first() ? 1 : undefined} labelledby="setup-heading">
      <div class="fr-col">
        <Show when={first()}>
          <Peek />
        </Show>
        <Title id="setup-heading" text={t('setup.title')} />
        <p class="sub">{t('setup.body')}</p>

        <div class="picks" role="group" aria-label={t('setup.title')}>
          <For each={choices}>
            {([option, label]) => (
              <button type="button" class="pick" aria-pressed={selected().includes(option)} onClick={() => onToggle(option)}>
                <Show when={option !== 'none'}>
                  <VendorMark vendor={option as Vendor} />
                </Show>
                {label}
                <Show when={selected().includes(option)}>
                  <span class="tick" aria-hidden="true">
                    <Icon name="check" small />
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>

        <div class="setup-cards" ref={animateList}>
          <For each={(['claude', 'codex'] as const).filter((v) => selected().includes(v))}>
            {(vendor) => {
              const card = () =>
                setupCard(
                  {
                    check: checks()[vendor],
                    progress: progresses()[vendor],
                    waiting: waitings()[vendor],
                    error: errors()[vendor],
                  },
                  fromEngine,
                );

              return (
                <div class="setup-card" data-vendor={vendor} ref={animateList}>
                  <div class="setup-card-header">
                    <h2>{vendor === 'claude' ? t('setup.claude') : t('setup.chatgpt')}</h2>
                    <span class={`setup-badge ${card().status}`}>
                      {fromEngine(`setup.${card().status}`)}
                    </span>
                  </div>

                  <Show when={card().status === 'installing' && progresses()[vendor]?.step}>
                    {(step) => (
                      <div class="setup-step">
                        <span class="setup-step-text">
                          {fromEngine(`setup.step.${step()}`)}
                        </span>
                      </div>
                    )}
                  </Show>

                  <Show when={card().error}>
                    {(err) => (
                      <div class="setup-error" role="alert">
                        <p class="setup-error-msg">{err().message}</p>
                        <Show when={err().detail}>
                          <details class="setup-details">
                            <summary>{t('setup.details')}</summary>
                            <pre>{err().detail}</pre>
                          </details>
                        </Show>
                      </div>
                    )}
                  </Show>

                  <Show when={card().actions.length > 0}>
                    <div class="setup-card-actions" ref={animateList}>
                      <Show when={card().actions.includes('install')}>
                        <button
                          type="button"
                          class="btn primary"
                          onClick={() => void installVendor(vendor)}
                        >
                          {t('setup.install')}
                        </button>
                      </Show>
                      <Show when={card().actions.includes('officialPage')}>
                        <button
                          type="button"
                          class="btn"
                          onClick={() => void api.openLink(OFFICIAL_PAGES[vendor])}
                        >
                          {t('setup.officialPage')}
                        </button>
                      </Show>
                      <Show when={card().actions.includes('signIn')}>
                        <button
                          type="button"
                          class="btn primary"
                          onClick={() => void signInVendor(vendor)}
                        >
                          {t('setup.signIn')}
                        </button>
                      </Show>
                      <Show when={card().actions.includes('checkAgain')}>
                        <button
                          type="button"
                          class="btn"
                          onClick={() => void checkVendor(vendor)}
                        >
                          {t('setup.checkAgain')}
                        </button>
                      </Show>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </div>
      <div class="first-foot">
        <Show
          when={first()}
          fallback={
            <button type="button" class="btn primary big" onClick={() => props.onFinish('done')}>
              {t('setup.close')}
            </button>
          }
        >
          <button type="button" class="btn ghost" onClick={() => props.onFinish('skipped')}>
            {t('setup.skip')}
          </button>
          <button type="button" class="btn primary big" onClick={() => props.onFinish('done')}>
            {t('first.continue')}
            <Icon name="right" small />
          </button>
        </Show>
      </div>
    </Stage>
  );
}
