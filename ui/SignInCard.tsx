import { createResource, createSignal, Show } from 'solid-js';
import { api, errorText, type Message } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import { vendorName } from './models.ts';

/**
 * Under a reply Auto refused because the account it picked never confirmed a subscription (\`error.billingUnknown\`). On a
 * fresh install that is an AI the person installed but never signed in to, and the refusal alone never said so
 * (2026-11-27). The card checks that account once and, when it is signed out, offers its own sign-in right here; signed in,
 * it says to send again. Any other answer leaves the refusal as it was.
 */
export function SignInCard(props: { m: Message }) {
  const vendor = () => (props.m.error === 'error.billingUnknown' ? props.m.vendor : undefined);
  const [check, { refetch }] = createResource(vendor, (v) => api.setupCheck(v).catch(() => undefined));
  const [asked, setAsked] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const signIn = async (v: NonNullable<ReturnType<typeof vendor>>) => {
    setError(undefined);
    try {
      await api.setupSignIn(v);
      setAsked(true);
    } catch (e) {
      setError(`${t('error.setup.signIn')} ${fromEngine(errorText(e))}`);
    }
  };
  const state = () => (check()?.installed ? check()!.account : undefined);
  return (
    <Show when={vendor() && (state() === 'signedOut' || (asked() && state() === 'signedIn'))}>
      <div class="quota-card sign-in-card" role="status">
        <Show
          when={state() === 'signedOut'}
          fallback={<p>{t('signIn.done', { vendor: vendorName(vendor()!) })}</p>}
        >
          <p>{t(asked() ? 'signIn.waiting' : 'signIn.needed', { vendor: vendorName(vendor()!) })}</p>
          <Show
            when={asked()}
            fallback={<button type="button" class="btn primary" onClick={() => void signIn(vendor()!)}>{t('setup.signIn')}</button>}
          >
            <button type="button" class="btn" onClick={() => void refetch()}>{t('setup.checkAgain')}</button>
          </Show>
        </Show>
        <Show when={error()}><p class="err">{error()}</p></Show>
      </div>
    </Show>
  );
}
