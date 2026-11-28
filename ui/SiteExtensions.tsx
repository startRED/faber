import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api } from './api.ts';
import { failureText, t } from './i18n.ts';
import { pressedAny } from './keys.ts';

export function SiteExtensions(props: { conversationId: string; onClose: () => void }) {
  const [extensions, setExtensions] = createSignal<Awaited<ReturnType<typeof api.siteExtensions>>>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  let dialog: HTMLDialogElement | undefined;
  let add: HTMLButtonElement | undefined;
  const manage = async (action: 'list' | 'pick' | 'reload' | 'remove', folder?: string) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try {
      setExtensions(await api.siteExtensions(props.conversationId, action, folder));
      if (action === 'remove') queueMicrotask(() => add?.focus());
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };
  onMount(() => {
    const focused = document.activeElement;
    dialog?.showModal();
    const key = (event: KeyboardEvent) => {
      if (pressedAny(event)) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    window.addEventListener('keydown', key, true);
    void manage('list');
    onCleanup(() => {
      window.removeEventListener('keydown', key, true);
      dialog?.close();
      if (focused instanceof HTMLElement && focused.isConnected) focused.focus({ preventScroll: true });
    });
  });
  return <Portal>
    <dialog ref={(el) => (dialog = el)} class="cookies site-extensions" aria-labelledby="site-extensions-title" aria-modal="true"
      onCancel={(event) => { event.preventDefault(); props.onClose(); }}>
      <h2 id="site-extensions-title" class="display">{t('site.extensions')}</h2>
      <p class="side-meta">{t('site.extensionsRisk')}</p>
      <Show when={busy()}><p role="status">{t('site.extensionsWorking')}</p></Show>
      <Show when={error()}>{(failure) => <p role="alert">{failureText(failure())}</p>}</Show>
      <Show when={extensions()?.length === 0}><p>{t('site.extensionsEmpty')}</p></Show>
      <Show when={extensions()?.length}>
        <ul class="site-extensions-list" aria-label={t('site.extensions')}>
          <For each={extensions()}>
            {(item) => <li>
              <bdi>{item.name} {item.version}</bdi>
              <p class="side-meta"><bdi>{item.folder}</bdi></p>
              <Show when={item.failed}><p role="status">{t('error.browser.extensionsFailed')}</p></Show>
              <div class="cookies-actions">
                <button class="btn" type="button" disabled={busy()} aria-label={t('site.extensionsReloadName', { name: item.name })} onClick={() => void manage('reload', item.folder)}>{t('site.extensionsReload')}</button>
                <button class="btn" type="button" disabled={busy()} aria-label={t('site.extensionsRemoveName', { name: item.name })} onClick={() => void manage('remove', item.folder)}>{t('site.extensionsRemove')}</button>
              </div>
            </li>}
          </For>
        </ul>
      </Show>
      <div class="cookies-actions">
        <button class="btn" type="button" onClick={props.onClose}>{t('cookies.done')}</button>
        <button ref={(el) => (add = el)} class="btn primary" type="button" disabled={busy()} onClick={() => void manage('pick')}>{t('site.extensionsLoad')}</button>
      </div>
    </dialog>
  </Portal>;
}
