import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { api, type Project, type StorageChoice, type StorageMove, type StorageUsage } from './api.ts';
import { Opt } from './controls.tsx';
import { bytes, catalogName, failureText, t } from './i18n.ts';
import type { Key } from './locales/en.ts';

// How much disk Faber's data folder takes, inside General > Storage. It measures when its fold opens (this component is
// only in the page while the fold is open) and when "Measure again" is pressed; there is no timer. A size the engine
// could not read is null and reads "Unknown", never 0. Nothing here removes anything: Faber already removes copies it no
// longer needs by itself, and no clean-up command is offered to the screen yet.

type Kind = keyof StorageUsage['byKind'];
const KINDS: Kind[] = ['copies', 'reports', 'tools', 'rewind', 'helperTranscripts', 'conversations', 'rest'];

export function StorageLocation() {
  const [choice, setChoice] = createSignal<StorageChoice>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  onMount(() => void api.storageChoice().then(setChoice, setError));
  const change = async (reset: boolean) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try {
      const saved = await api.storageChoose(reset);
      if (saved) setChoice(saved);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="storage-location" aria-busy={busy()}>
      <Opt id="set-storage-location" label={t('set.storage.location')} hint={choice()?.folder} more={t('set.storage.locationMore')}>
        <button type="button" class="btn" aria-disabled={busy() || !choice() || choice()?.locked} onClick={() => {
          if (choice() && !choice()?.locked) void change(false);
        }}>{t('set.storage.change')}</button>
        <button type="button" class="btn" aria-disabled={busy() || !choice()?.custom || choice()?.locked} onClick={() => {
          if (choice()?.custom && !choice()?.locked) void change(true);
        }}>{t('set.storage.reset')}</button>
      </Opt>
      <Show when={choice()?.notice}>{(key) => <p class="hint store-note" role="status">{t(key())}</p>}</Show>
      <Show when={error()}>{(failure) => <p class="note err store-note" role="alert">{failureText(failure())}</p>}</Show>
      <ToolMove choice={choice} />
    </div>
  );
}

// Moving the tools already installed in the main data folder to the chosen one. The move itself runs when Faber starts
// again, before anything can use a tool (host/src/storage_choice/storage_move.rs); here the person asks for it, cancels a
// move that waits, reads how the last one ended and removes the originals it kept. Nothing here deletes without a second
// click.
function ToolMove(props: { choice: () => StorageChoice | undefined }) {
  const [move, setMove] = createSignal<StorageMove>();
  const [asking, setAsking] = createSignal<'move' | 'remove'>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<unknown>();
  let request = 0;
  const load = () => {
    const mine = ++request;
    api.storageMove().then((found) => { if (mine === request) setMove(found); }, (failure) => { if (mine === request) setError(failure); });
  };
  // Read when the fold opens, and again when the chosen folder changes: what can move depends on it, and a change cancels
  // a waiting move.
  onMount(load);
  createEffect(on(() => props.choice()?.folder, (folder, previous) => { if (previous !== undefined && folder !== previous) load(); }, { defer: true }));
  onCleanup(() => { request++; });
  const act = async (work: () => Promise<unknown>) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try {
      await work();
      setAsking(undefined);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
      load();
    }
  };
  const last = () => move()?.last;
  const canMove = () => !!props.choice()?.custom && !props.choice()?.locked && !move()?.pending && (move()?.movable ?? 0) > 0;
  return (
    <div class="store-move" aria-busy={busy()}>
      <Show when={move()?.pending}>
        <div class="st-opt">
          <span class="st-text"><span class="st-hint" role="status">{t('set.storage.movePending')}</span></span>
          <span class="st-ctl">
            <button type="button" class="btn" aria-disabled={busy()} onClick={() => void act(api.storageMoveCancel)}>{t('set.storage.moveCancel')}</button>
          </span>
        </div>
      </Show>
      <Show when={canMove() && move()}>
        {(found) => (
          <Opt id="set-storage-move" label={t('set.storage.move')} hint={t('set.storage.moveHint', { n: found().movable, size: bytes(found().movableBytes) })}>
            <button type="button" class="btn" aria-disabled={busy()} aria-expanded={asking() === 'move'} onClick={() => { if (!busy()) setAsking('move'); }}>
              {t('set.storage.moveHere')}
            </button>
          </Opt>
        )}
      </Show>
      <Show when={asking() === 'move' && canMove()}>
        <div class="store-ask" role="group" aria-label={t('set.storage.move')}>
          <p class="hint">{t('set.storage.moveConfirm')}</p>
          <span class="store-ask-actions">
            <button type="button" class="btn primary" aria-disabled={busy()} onClick={() => void act(api.storageMoveRequest)}>{t('set.storage.moveNow')}</button>
            <button type="button" class="btn" onClick={() => setAsking(undefined)}>{t('set.storage.moveBack')}</button>
          </span>
        </div>
      </Show>
      <Show when={last()}>
        {(outcome) => (
          <div class="store-note" role="status">
            <Show when={outcome().moved.length > 0}>
              <p class="hint">{t('set.storage.moved', { n: outcome().moved.length, size: bytes(outcome().bytes) })}</p>
            </Show>
            <For each={outcome().left}>{(left) => <p class="hint">{t('set.storage.moveLeft', { name: catalogName(left.id), why: t(left.reason) })}</p>}</For>
            <Show when={outcome().stopped}>{(key) => <p class="note err">{t(key() as Key)}</p>}</Show>
          </div>
        )}
      </Show>
      <Show when={(move()?.oldBytes ?? 0) > 0}>
        <Opt id="set-storage-old" label={t('set.storage.old')} hint={t('set.storage.oldHint', { size: bytes(move()?.oldBytes ?? 0) })}>
          <button type="button" class="btn" aria-disabled={busy()} aria-expanded={asking() === 'remove'} onClick={() => { if (!busy()) setAsking('remove'); }}>
            {t('set.storage.oldRemove')}
          </button>
        </Opt>
        <Show when={asking() === 'remove'}>
          <div class="store-ask" role="group" aria-label={t('set.storage.old')}>
            <p class="hint">{t('set.storage.oldConfirm')}</p>
            <span class="store-ask-actions">
              <button type="button" class="btn danger" aria-disabled={busy()} onClick={() => void act(api.storageMoveRemove)}>{t('set.storage.oldRemove')}</button>
              <button type="button" class="btn" onClick={() => setAsking(undefined)}>{t('set.storage.moveBack')}</button>
            </span>
          </div>
        </Show>
      </Show>
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
