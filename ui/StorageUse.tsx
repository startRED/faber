import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, type Project, type StorageChoice, type StorageMove, type StorageUsage } from './api.ts';
import { Opt } from './controls.tsx';
import { bytes, failureText, t } from './i18n.ts';

// How much disk Faber's data folder takes, inside General > Storage. It measures when its fold opens (this component is
// only in the page while the fold is open) and when "Measure again" is pressed; there is no timer. A size the engine
// could not read is null and reads "Unknown", never 0.

type Kind = keyof StorageUsage['byKind'];
const KINDS: Kind[] = ['copies', 'reports', 'tools', 'rewind', 'helperTranscripts', 'conversations', 'rest'];

export function StorageLocation(props: { onRemoved: () => void }) {
  const [choice, setChoice] = createSignal<StorageChoice>();
  const [move, setMove] = createSignal<StorageMove>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  const [confirm, setConfirm] = createSignal(false);
  const [kept, setKept] = createSignal<{ id: string; error: 'reason.storage.inUse' }[]>([]);
  let request = 0;
  const refresh = async () => {
    const mine = ++request;
    const status = await api.storageMoveStatus();
    if (mine === request) setMove(status);
  };
  onCleanup(() => { request++; });
  const moving = createMemo(() => move()?.running === true);
  createEffect(() => {
    if (!moving()) return;
    let reading = false;
    const timer = setInterval(() => {
      if (busy() || reading) return;
      reading = true;
      void refresh().catch(setError).finally(() => { reading = false; });
    }, 250);
    onCleanup(() => clearInterval(timer));
  });
  const run = async (action: () => Promise<void>) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try {
      await action();
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };
  onMount(() => void run(async () => { setChoice(await api.storageChoice()); }));
  const change = (reset: boolean) => run(async () => {
    const saved = await api.storageChoose(reset);
    if (saved) {
      setChoice(saved);
      setConfirm(false);
    }
  });
  const remove = () => run(async () => {
    setKept(await api.storageMoveRemove());
    setConfirm(false);
    props.onRemoved();
  });
  const locked = () => busy() || !choice() || choice()?.locked;
  const progress = () => {
    const item = move()?.item;
    if (!item) return t('set.storage.movePreparing');
    const position = { name: item.id, number: item.number, total: item.total };
    return item.size === null
      ? t('set.storage.moveItem', position)
      : t('set.storage.moveProgress', { ...position, copied: bytes(item.copied), size: bytes(item.size) });
  };
  return (
    <div class="storage-location" aria-busy={busy()}>
      <Opt id="set-storage-location" label={t('set.storage.location')} hint={choice()?.folder} more={t('set.storage.locationMore')}>
        <button type="button" class="btn" aria-disabled={!!locked()} onClick={() => {
          if (!locked()) void change(false);
        }}>{t('set.storage.change')}</button>
        <button type="button" class="btn" aria-disabled={!!locked() || !choice()?.custom} onClick={() => {
          if (!locked() && choice()?.custom) void change(true);
        }}>{t('set.storage.reset')}</button>
      </Opt>
      <Show when={choice()?.notice}>{(key) => <p class="hint store-note" role="status">{t(key())}</p>}</Show>
      <Show when={move()?.running || move()?.available}>
        <Opt id="set-storage-move" label={t('set.storage.move')} hint={move()?.running ? move()?.target ?? undefined : t('set.storage.moveHint')}>
          <button type="button" class="btn" disabled={busy() || move()?.stopping}
            onClick={() => void run(move()?.running ? api.storageMoveStop : api.storageMoveStart)}>
            {t(move()?.stopping ? 'set.storage.moveStopping' : move()?.running ? 'set.storage.moveStop' : 'set.storage.moveAction')}
          </button>
        </Opt>
      </Show>
      <Show when={move()?.running}>
        <p class="hint store-note" role="status">{progress()}</p>
      </Show>
      <Show when={move()?.stopped}><p class="hint store-note" role="status">{t('set.storage.moveStopped')}</p></Show>
      <Show when={move()?.superseded}><p class="hint store-note" role="status">{t('set.storage.moveSuperseded')}</p></Show>
      <Show when={move()?.moved || move()?.failures.length}>
        <p class="hint store-note" role="status">{t('set.storage.moveResult', { moved: move()!.moved, failed: move()!.failures.length })}</p>
      </Show>
      <For each={move()?.failures}>{(item, index) => (
        <Opt id={`set-storage-failed-${index()}`} label={item.id} hint={t(item.error)} />
      )}</For>
      <Show when={move()?.oldCopies && !move()?.running}>
        <Opt id="set-storage-left" label={t('set.storage.left')} hint={confirm() ? t('set.storage.removeConfirm') : undefined}>
          <Show when={confirm()}>
            <button type="button" class="btn" disabled={busy()} onClick={() => {
              document.getElementById('set-storage-remove')?.focus();
              setConfirm(false);
            }}>{t('set.storage.moveCancel')}</button>
          </Show>
          <button type="button" id="set-storage-remove" class="btn" classList={{ danger: confirm() }}
            disabled={busy()} aria-describedby={confirm() ? 'set-storage-left-hint' : 'set-storage-left-name'}
            onClick={() => confirm() ? void remove() : setConfirm(true)}>
            {t('set.storage.removeAction')}
          </button>
        </Opt>
      </Show>
      <For each={move()?.oldCopies ? kept() : []}>{(item) => (
        <p class="hint store-note" role="status">{t(item.error, { name: item.id })}</p>
      )}</For>
      <Show when={move()?.error}>{(key) => <p class="note err store-note" role="alert">{t(key())}</p>}</Show>
      <Show when={error()}>{(failure) => <p class="note err store-note" role="alert">{failureText(failure())}</p>}</Show>
    </div>
  );
}

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

export function StorageUse(props: { projects: Project[]; revision?: number }) {
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
  createEffect(() => {
    void props.revision;
    measure();
  });
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
