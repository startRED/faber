import { createMemo, createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type MemoryEntry, type MemoryFile, type MemoryList } from './api.ts';
import { Collapse } from './controls.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { setDirty, useDraft } from './unsent.ts';

// Match ContextMenu: two activations in one double-click interval cannot confirm a deletion.
const SAME_GESTURE = 500;

/** The project's own instruction files, read only on opening and refresh; no process or watcher. */
export function ProjectMemory(props: { projectId: string }) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="project-memory">
      <button type="button" class="btn ghost" aria-describedby="memory-summary" aria-expanded={open()} onClick={() => setOpen(!open())}>{t('memory.title')}</button>
      <p id="memory-summary" class="project-summary">{t('memory.summary')}</p>
      <Collapse open={open()}>
        <Show when={open() && props.projectId} keyed>{(id) => <MemoryList projectId={id} />}</Show>
      </Collapse>
    </section>
  );
}

function MemoryList(props: { projectId: string }) {
  let alive = true;
  let body!: HTMLDivElement;
  onCleanup(() => { alive = false; });
  const [notice, setNotice] = createSignal('');
  const [editing, setEditing] = createSignal<string>();
  const [draft, setDraft] = createSignal('');
  const [confirm, setConfirm] = createSignal<string>();
  let armedAt = 0;
  let keyDown = false;
  let needsRelease = false;
  const [busy, setBusy] = createSignal(false);
  useDraft(() => editing() !== undefined || busy());
  const [error, setError] = createSignal('');
  // The list is held here, not in a resource: a change shows on it at once and a re-read replaces it quietly, so it never
  // goes back to "Loading". Only the first read says so; every read still holds the buttons, so an editor never opens on
  // a revision the read in flight is about to replace.
  const [files, setFiles] = createSignal<MemoryList>();
  const [loading, setLoading] = createSignal(true);
  const [reading, setReading] = createSignal(0);
  const refetch = async () => {
    setReading((n) => n + 1);
    try { setFiles(await api.projectMemory(props.projectId)); }
    catch (err) { setError(message(err)); }
    finally { setLoading(false); setReading((n) => n - 1); }
  };
  void refetch();
  const [shown, setShown] = createSignal(50);
  const rows = createMemo(() => files()?.files.flatMap((file) => file.entries.length
    ? file.entries.map((entry, index) => ({ file, entry: entry as MemoryEntry | undefined, first: index === 0 }))
    : [{ file, entry: undefined, first: true }]) ?? []);
  const key = (file: MemoryFile, entry: MemoryEntry) => `${file.path}:${entry.start}`;
  const message = (err: unknown) => {
    const key = String(err);
    return t((key.startsWith('error.memory.') ? key : 'error.memory.read') as Key);
  };
  const refresh = async () => {
    setEditing(undefined); setConfirm(undefined); setError(''); setNotice(''); setShown(50);
    keyDown = needsRelease = false;
    await refetch();
  };
  const save = async (file: MemoryFile, entry: MemoryEntry, text: string | null, keep = false) => {
    // The write can outlive the editor when the fold closes or the person changes projects.
    const pending = {};
    let kept = false;
    setDirty(pending, true);
    setBusy(true); setError(''); setNotice('');
    const before = files();
    // What the person asked for is on the list before the engine answers; a refusal puts the old list back.
    const change = (e: MemoryEntry): MemoryEntry | undefined => e.start !== entry.start ? e : text === null ? (keep ? { ...e, reviewed: true } : undefined) : { ...e, text, body: text };
    setFiles((list) => list && { ...list, files: list.files.map((f) => f.path !== file.path ? f : { ...f, entries: f.entries.flatMap((e) => change(e) ?? []) }) });
    try {
      await api.editProjectMemory(props.projectId, { path: file.path, revision: file.revision, start: entry.start, end: entry.end, text, keep });
      if (alive && keep) {
        // Keep is the repeated action of the list: the list keeps its length, an open editor and the person's place.
        setNotice(t('memory.kept'));
        await refetch();
        kept = true;
      } else if (alive) await refresh();
    } catch (err) { if (alive) { setFiles(before); setError(message(err)); } }
    finally { if (alive) setBusy(false); setDirty(pending, false); }
    // The buttons are enabled again only now, and a disabled one cannot take focus.
    if (kept && alive) [...body.querySelectorAll<HTMLElement>('.memory-entry')].find((row) => row.dataset.entry === key(file, entry))?.querySelector<HTMLElement>('[data-edit]')?.focus();
  };
  const remove = (file: MemoryFile, entry: MemoryEntry) => {
    if (confirm() !== key(file, entry)) {
      armedAt = performance.now();
      needsRelease = keyDown;
      return setConfirm(key(file, entry));
    }
    if (needsRelease || performance.now() - armedAt < SAME_GESTURE) return;
    void save(file, entry, null);
  };
  return (
    <div class="memory-body" aria-busy={busy() || reading() > 0} ref={body}>
      <p class="sr-only" role="status">{notice()}</p>
      <div class="memory-head"><p class="hint">{t('memory.scope')}</p><button type="button" class="btn ghost" disabled={busy() || reading() > 0} onClick={() => void refresh()}>{t('memory.refresh')}</button></div>
      <p class="hint">{t('memory.nextLaunch')}</p>
      <Show when={loading()}><p role="status">{t('memory.loading')}</p></Show>
      <Show when={error()}><p class="note err" role="alert">{error()}</p></Show>
      <Show when={files() && !loading() && !rows().length}><p class="hint">{t('memory.empty')}</p></Show>
      <Show when={files()?.incomplete}><p class="hint" role="status">{t('memory.incomplete')}</p></Show>
      <ul class="memory-entries">
        <For each={rows().slice(0, shown())}>{({ file, entry, first }) => (
          <li class="memory-entry" data-entry={entry ? key(file, entry) : undefined}>
            <div class="memory-from">{t(`memory.source.${file.source}`)} · <span>{file.path}{entry ? `:${entry.line}` : ''}</span>{entry && ['.faber/taste.md', '.faber/lessons.md'].includes(file.path) && !entry.reviewed && <> · <span class="memory-flag ask">{t('memory.notReviewed')}</span></>}{entry?.author && <> · <span class="memory-flag" classList={{ ask: entry.author === 'ai' && !entry.reviewed }}>{t(entry.author === 'person' ? 'memory.author.person' : entry.reviewed ? 'memory.author.aiKept' : 'memory.author.ai')}</span></>}</div>
            <Show when={first && file.overLimit}><p class="hint">{t('memory.overLimit', { size: file.overLimit![0], target: file.overLimit![1] })}</p></Show>
            <Show when={entry} fallback={<p class="hint">{t(file.tooLarge ? 'memory.tooLarge' : 'memory.noEntries')}</p>}>
              {(entry) => <Show when={editing() === key(file, entry())} fallback={<>
                <p class="memory-text">{(entry().body || entry().text).replace(/ \(stale\?\)$/u, '')}</p>
                <Show when={entry().stale}><p class="hint memory-stale">{t('memory.stale')}</p></Show>
                <Show when={file.path !== ".faber/rules.md"}><div class="memory-actions">
                  <Show when={entry().author === 'ai' && !entry().reviewed}><button type="button" class="btn ghost" disabled={busy() || reading() > 0} onClick={() => void save(file, entry(), null, true)}>{t('memory.keep')}</button></Show>
                  <button type="button" class="btn ghost" data-edit disabled={busy() || reading() > 0} onClick={() => { setConfirm(undefined); setEditing(key(file, entry())); setDraft(entry().text); }}>{t('memory.edit')}</button>
                  <button type="button" class="btn ghost danger" disabled={busy() || reading() > 0}
                    onKeyDown={(event) => {
                      if (event.key !== 'Enter' && event.key !== ' ') return;
                      if (event.repeat) event.preventDefault();
                      else keyDown = true;
                    }}
                    onKeyUp={(event) => { if (event.key === 'Enter' || event.key === ' ') keyDown = needsRelease = false; }}
                    onFocusOut={() => { keyDown = needsRelease = false; if (confirm() === key(file, entry())) setConfirm(undefined); }}
                    onClick={() => remove(file, entry())}>{t(confirm() === key(file, entry()) ? 'memory.confirmDelete' : 'memory.delete')}</button>
                  <Show when={confirm() === key(file, entry())}><button type="button" class="btn ghost" disabled={busy() || reading() > 0} onClick={() => setConfirm(undefined)}>{t('memory.cancel')}</button></Show>
                </div></Show>
              </>}>
                <form onSubmit={(event) => { event.preventDefault(); if (!busy() && !reading()) void save(file, entry(), draft()); }} onKeyDown={(event) => { if (event.key === 'Escape' && !event.isComposing && !busy()) { event.preventDefault(); event.stopPropagation(); setEditing(undefined); } }}>
                  <textarea class="field" aria-label={t('memory.editText')} value={draft()} disabled={busy() || reading() > 0} onInput={(event) => setDraft(event.currentTarget.value)} ref={(el) => queueMicrotask(() => el.focus())} />
                  <div class="memory-actions">
                    <button type="submit" class="btn ghost" disabled={busy() || reading() > 0 || !draft().trim()}>{t('memory.save')}</button>
                    <button type="button" class="btn ghost" disabled={busy() || reading() > 0} onClick={() => setEditing(undefined)}>{t('memory.cancel')}</button>
                  </div>
                </form>
              </Show>}
            </Show>
          </li>
        )}</For>
      </ul>
      <Show when={rows().length > shown()}><button type="button" class="btn ghost" onClick={() => setShown(shown() + 50)}>{t('memory.more')}</button></Show>
    </div>
  );
}
