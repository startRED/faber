import { createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, type Project, type StorageUsage } from './api.ts';
import { bytes, failureText, t } from './i18n.ts';

// How much disk Faber's data folder takes, inside General > Storage. It measures when its fold opens (this component is
// only in the page while the fold is open) and when "Measure again" is pressed; there is no timer. A size the engine
// could not read is null and reads "Unknown", never 0. Nothing here removes anything: Faber already removes copies it no
// longer needs by itself, and no clean-up command is offered to the screen yet.

type Kind = keyof StorageUsage['byKind'];
const KINDS: Kind[] = ['copies', 'reports', 'tools', 'rewind', 'helperTranscripts', 'conversations', 'rest'];

/** Biggest first, unknown sizes after the known ones; `last` stays at the end whatever its size. */
const order = <T,>(rows: [T, number | null][], last?: T) =>
  [...rows].sort(([a, x], [b, y]) => Number(a === last) - Number(b === last) || (x === null ? 1 : 0) - (y === null ? 1 : 0) || (y ?? 0) - (x ?? 0));

function Row(props: { name: string; size: number | null; of: number | null }) {
  const share = () => (props.size !== null && props.of ? Math.min(1, props.size / props.of) : 0);
  return (
    <li class="store-row">
      <span class="store-name" title={props.name}>{props.name}</span>
      <span class="num" classList={{ unknown: props.size === null }}>{props.size === null ? t('set.storage.unknown') : bytes(props.size)}</span>
      <span class="meter" aria-hidden="true"><i style={{ 'clip-path': `inset(0 ${100 - share() * 100}% 0 0)` }} /></span>
    </li>
  );
}

export function StorageUse(props: { projects: Project[] }) {
  const [usage, setUsage] = createSignal<StorageUsage>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  let request = 0;
  const measure = () => {
    const mine = ++request;
    setBusy(true);
    setError(undefined);
    api.storageUsage().then(
      (found) => { if (mine === request) setUsage(found); },
      (failure) => { if (mine === request) setError(failure); },
    ).finally(() => { if (mine === request) setBusy(false); });
  };
  onMount(measure);
  // A reading that arrives after the fold closed is dropped.
  onCleanup(() => { request++; });

  const kinds = createMemo(() => {
    const found = usage();
    return found ? order(KINDS.map((kind): [Kind, number | null] => [kind, found.byKind[kind]]), 'rest') : [];
  });
  const projectName = (id: string) => props.projects.find((p) => p.id === id)?.name ?? t('set.storage.removedProject');
  const copies = createMemo(() => {
    const found = usage();
    if (!found) return [];
    const rows: [string, number | null][] = Object.entries(found.copiesByProject).map(([id, size]) => [projectName(id), size]);
    const loose = found.unassignedCopyBytes;
    if (loose !== 0) rows.push(['', loose]);
    return order(rows, '');
  });
  const line = () => {
    if (busy()) return t('set.storage.measuring');
    const free = usage()?.freeBytes;
    if (free === undefined) return '';
    return free === null ? t('set.storage.freeUnknown') : t('set.storage.free', { size: bytes(free) });
  };

  return (
    <div class="store" aria-busy={busy()}>
      <div class="st-opt">
        <span class="st-text">
          <span class="st-name">{t('set.storage.total')}</span>
          <span class="st-hint" role="status">{line()}</span>
        </span>
        <span class="st-ctl">
          <Show when={usage()}>
            {(found) => (
              <span class="num store-total" classList={{ unknown: found().totalBytes === null }}>
                {found().totalBytes === null ? t('set.storage.unknown') : bytes(found().totalBytes!)}
              </span>
            )}
          </Show>
          {/* aria-disabled, not disabled, so a keyboard press keeps its focus on the button while it measures. */}
          <button type="button" class="btn" aria-disabled={busy()} onClick={() => { if (!busy()) measure(); }}>
            {t(error() ? 'set.storage.retry' : 'set.storage.again')}
          </button>
        </span>
      </div>
      <Show when={error()}>
        {(failure) => <p class="note err store-note" role="alert">{failureText(failure())}</p>}
      </Show>
      <Show when={usage()}>
        {(found) => (
          <>
            <Show when={found().partial}>
              <p class="hint store-note">{t('set.storage.partial')}</p>
            </Show>
            <ul class="store-list" aria-label={t('set.storage.byKind')}>
              <For each={kinds()}>{([kind, size]) => <Row name={t(`set.storage.kind.${kind}`)} size={size} of={found().totalBytes} />}</For>
            </ul>
            <section class="store-copies" aria-labelledby="set-storage-copies">
              <h4 class="store-head" id="set-storage-copies">{t('set.storage.byProject')}</h4>
              <Show when={copies().length} fallback={<p class="hint store-note">{t('set.storage.noCopies')}</p>}>
                <ul class="store-list" aria-labelledby="set-storage-copies">
                  <For each={copies()}>{([name, size]) => <Row name={name || t('set.storage.noProject')} size={size} of={found().byKind.copies} />}</For>
                </ul>
              </Show>
              <p class="hint store-note">{t('set.storage.sweep')}</p>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}
