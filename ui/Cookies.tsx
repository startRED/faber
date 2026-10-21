import { createEffect, createMemo, createResource, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api, type CookieProfile, type CookieSite } from './api.ts';
import { keepFocusIn } from './focus.ts';
import { failureText, t } from './i18n.ts';
import { pressedAny } from './keys.ts';

/** The person's Firefox profiles, read each time the browser pane or the dialog opens (one small file): the pane offers to
 * bring sign-ins only where one exists, and a profile made meanwhile shows. */
export const cookieProfiles = () => api.cookieProfiles().catch(() => [] as CookieProfile[]);

const profileName = (p: CookieProfile) => (p.name === 'default' ? t('cookies.defaultProfile') : p.name);

/** Brings the person's sign-ins from their own Firefox into one conversation's pages in Faber's browser: they pick the
 * profile, then the sites. Only names and counts reach the screen; the cookies stay in the shell, in memory, until Faber
 * quits or the person forgets them here. */
export function Cookies(props: { conversationId: string; onClose: () => void }) {
  const [found] = createResource(cookieProfiles);
  const [held, { refetch: reheld }] = createResource(() => props.conversationId, (id) => api.cookieHeld(id).catch(() => []));
  const [profile, setProfile] = createSignal<string>();
  const chosen = () => profile() ?? found()?.find((p) => p.lastUsed)?.id ?? found()?.[0]?.id;
  const [listed, setListed] = createSignal<{ ticket: string; sites: CookieSite[] }>();
  const [picked, setPicked] = createSignal<Set<string>>(new Set());
  const [filter, setFilter] = createSignal('');
  const [working, setWorking] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  const [brought, setBrought] = createSignal<number>();
  let dialog: HTMLDialogElement | undefined;
  let first: HTMLElement | undefined;
  const finish = () => !working() && props.onClose();
  onMount(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    dialog?.showModal();
    queueMicrotask(() => first?.focus());
    // The shell's shortcuts must not open another surface over a native modal dialog.
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        finish();
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
      queueMicrotask(() => before?.isConnected && before.focus({ preventScroll: true }));
    });
  });
  const run = async (step: () => Promise<void>) => {
    if (working()) return;
    setWorking(true);
    setError(undefined);
    try {
      await step();
    } catch (err) {
      setError(err);
    } finally {
      setWorking(false);
    }
  };
  // Every site starts picked but banks, email and sign-in services, listed first: the agent may act on what comes over, so
  // the person ticks those themselves.
  const list = () =>
    run(async () => {
      const id = chosen();
      if (!id) return;
      const next = await api.cookieSites(id);
      setPicked(new Set(next.sites.filter((s) => !s.kind).map((s) => s.name)));
      setFilter('');
      setListed(next);
      queueMicrotask(() => first?.focus());
    });
  const bring = () =>
    run(async () => {
      const at = listed();
      if (!at || !picked().size) return;
      setBrought(await api.cookieImport(props.conversationId, at.ticket, [...picked()]));
      setListed(undefined);
      queueMicrotask(() => first?.focus());
    });
  const forget = () =>
    run(async () => {
      await api.cookieForget(props.conversationId);
      await reheld();
    });
  const shown = createMemo(() => {
    const words = filter().trim().toLowerCase();
    return (listed()?.sites ?? []).filter((s) => !words || s.name.includes(words));
  });
  const allShown = () => shown().length > 0 && shown().every((s) => picked().has(s.name));
  const someShown = () => !allShown() && shown().some((s) => picked().has(s.name));
  const toggle = (name: string, on: boolean) =>
    setPicked((old) => {
      const next = new Set(old);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  const toggleShown = (on: boolean) =>
    setPicked((old) => {
      const next = new Set(old);
      for (const s of shown()) {
        if (on) next.add(s.name);
        else next.delete(s.name);
      }
      return next;
    });
  const name = () => {
    const p = found()?.find((p) => p.id === chosen());
    return p ? profileName(p) : '';
  };
  return (
    <Portal>
      <dialog ref={(el) => (dialog = el)} class="cookies" aria-labelledby="cookies-title" aria-modal="true" onCancel={(e) => (e.preventDefault(), finish())}>
        <Show
          when={brought() === undefined}
          fallback={
            <>
              <h2 id="cookies-title" class="display">{t('cookies.title')}</h2>
              <p role="status">{t('cookies.brought', { n: brought() ?? 0 })}</p>
              <p class="side-meta">{t('cookies.where')}</p>
              <div class="cookies-actions">
                <button type="button" class="btn primary" ref={(el) => (first = el)} onClick={() => props.onClose()}>{t('cookies.done')}</button>
              </div>
            </>
          }
        >
          <Show
            when={listed()}
            fallback={
              <>
                <h2 id="cookies-title" class="display">{t('cookies.title')}</h2>
                <Show when={(found()?.length ?? 0) > 1} fallback={<p>{t('cookies.from', { profile: name() })}</p>}>
                  <label class="cookies-from">
                    <span>{t('cookies.fromLabel')}</span>
                    <select class="field" value={chosen()} disabled={working()} onChange={(e) => setProfile(e.currentTarget.value)}>
                      <For each={found()}>{(p) => <option value={p.id}>{profileName(p)}</option>}</For>
                    </select>
                  </label>
                </Show>
                <ul class="cookies-points">
                  <li>{t('cookies.onlyCookies')}</li>
                  <li>{t('cookies.youPick')}</li>
                  <li>{t('cookies.where')}</li>
                </ul>
                <p class="side-meta">{t('cookies.whyFirefox')}</p>
                <Show when={held()?.length}>
                  {(n) => (
                    <p class="cookies-held">
                      <span>{t('cookies.held', { n: n() })}</span>
                      <button type="button" class="btn" disabled={working()} onClick={() => void forget()}>{t('cookies.forget')}</button>
                    </p>
                  )}
                </Show>
                <div class="cookies-state" role="status" aria-live="polite">
                  <Show when={working()}><p>{t('cookies.reading')}</p></Show>
                </div>
                <Show when={error()}>{(err) => <p class="note err" role="alert">{failureText(err())}</p>}</Show>
                <div class="cookies-actions">
                  <button type="button" class="btn primary" ref={(el) => (first = el)} disabled={working() || !chosen()} onClick={() => void list()}>{t('cookies.continue')}</button>
                  <button type="button" class="btn" disabled={working()} onClick={finish}>{t('cookies.cancel')}</button>
                </div>
              </>
            }
          >
            {(at) => (
              <>
                <h2 id="cookies-title" class="display">{t('cookies.pickTitle')}</h2>
                <p class="side-meta">{t('cookies.pickFrom', { profile: name() })}</p>
                <Show when={at().sites.length} fallback={<p>{t('cookies.noSites')}</p>}>
                  <input class="field" type="search" ref={(el) => (first = el)} placeholder={t('cookies.filter')} aria-label={t('cookies.filter')} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
                  <label class="cookies-all">
                    <input type="checkbox" checked={allShown()} ref={(el) => createEffect(() => (el.indeterminate = someShown()))} disabled={!shown().length} onChange={(e) => toggleShown(e.currentTarget.checked)} />
                    {t('cookies.all')}
                  </label>
                  <ul class="cookies-sites" aria-label={t('cookies.pickTitle')}>
                    <For each={shown()} fallback={<li class="side-meta">{t('cookies.noMatch')}</li>}>
                      {(site) => (
                        <li>
                          <label>
                            <input type="checkbox" checked={picked().has(site.name)} onChange={(e) => toggle(site.name, e.currentTarget.checked)} />
                            <span class="cookies-site">{site.name}</span>
                            <Show when={site.kind}>{(kind) => <span class="cookies-kind">{t(`cookies.kind.${kind()}`)}</span>}</Show>
                          </label>
                        </li>
                      )}
                    </For>
                  </ul>
                  <p class="side-meta" aria-live="polite">{t('cookies.count', { n: picked().size, total: at().sites.length })}</p>
                </Show>
                <Show when={error()}>{(err) => <p class="note err" role="alert">{failureText(err())}</p>}</Show>
                <div class="cookies-actions">
                  <button type="button" class="btn primary" disabled={working() || !picked().size} onClick={() => void bring()}>{t(working() ? 'cookies.bringing' : 'cookies.bring')}</button>
                  <button type="button" class="btn" disabled={working()} onClick={() => (setListed(undefined), setError(undefined))}>{t('cookies.back')}</button>
                </div>
              </>
            )}
          </Show>
        </Show>
      </dialog>
    </Portal>
  );
}
