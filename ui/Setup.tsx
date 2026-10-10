import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, errorText, type Vendor } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import {
  OFFICIAL_PAGES,
  setupCard,
  toggleSubscription,
  type Check,
  type SetupProgress,
  type Subscription,
} from './setup.ts';

export interface SetupProps {
  onClose?: () => void;
  onFinish: (result: 'done' | 'skipped') => void;
}

export function Setup(props: SetupProps) {
  const [selected, setSelected] = createSignal<Subscription[]>([]);
  const [checks, setChecks] = createSignal<Partial<Record<Vendor, Check>>>({});
  const [progresses, setProgresses] = createSignal<Partial<Record<Vendor, SetupProgress>>>({});
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
    setProgresses((prev) => ({ ...prev, [v]: { type: 'setup', vendor: v, step: 'prerequisite' } }));
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
    const unlisten = api.on((e) => {
      if (e.type === 'setup') {
        setProgresses((prev) => ({ ...prev, [e.vendor]: e }));
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

  return (
    <div class="setup-container" role="dialog" aria-modal="true" aria-labelledby="setup-heading">
      <div class="setup-card-panel">
        <header class="setup-header">
          <h1 id="setup-heading">{t('setup.title')}</h1>
          <p class="setup-body-text">{t('setup.body')}</p>
        </header>

        <div class="setup-choices" role="group" aria-label={t('setup.title')}>
          <button
            type="button"
            class="opt"
            aria-pressed={selected().includes('claude')}
            onClick={() => onToggle('claude')}
          >
            {t('setup.claude')}
          </button>
          <button
            type="button"
            class="opt"
            aria-pressed={selected().includes('codex')}
            onClick={() => onToggle('codex')}
          >
            {t('setup.chatgpt')}
          </button>
          <button
            type="button"
            class="opt"
            aria-pressed={selected().includes('none')}
            onClick={() => onToggle('none')}
          >
            {t('setup.none')}
          </button>
        </div>

        <div class="setup-cards">
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
                <div class="setup-card" data-vendor={vendor}>
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
                    <div class="setup-card-actions">
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
                          class="btn secondary"
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
                          class="btn secondary"
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

        <footer class="setup-footer">
          <button
            type="button"
            class="btn secondary"
            onClick={() => props.onFinish('skipped')}
          >
            {t('setup.skip')}
          </button>
          <button
            type="button"
            class="btn primary"
            onClick={() => props.onFinish('done')}
          >
            {t('setup.done')}
          </button>
        </footer>
      </div>
    </div>
  );
}
