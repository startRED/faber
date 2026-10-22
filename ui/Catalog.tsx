import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { api, type Project, type ToolRow, type Vendor } from './api.ts';
import { Toggle } from './controls.tsx';
import { Icon, type GlyphName } from './Glyphs.tsx';
import { catalogName, failureText, fromEngine, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { CatalogInventory } from './CatalogInventory.tsx';
import { McpServers } from './McpServers.tsx';
import { keysOf, pressed } from './keys.ts';
import { Peek } from './Mark.tsx';

const PAGE = 48;
// Preserve the legacy catalog's jobs while presenting a few broad groups.
const jobs: Record<string, string> = {
  debugging: 'code', testing: 'code', lean: 'code', architecture: 'code', mcp: 'code', skills: 'code', performance: 'code',
  payments: 'services', platform: 'services', auth: 'services', measure: 'services',
};
const job = (row: ToolRow) => jobs[row.job] ?? row.job;
// One drawing per job, so a card shows at a glance what kind of help it is; the words beside it carry the meaning.
const glyphs: Record<string, GlyphName> = {
  debugging: 'bug', testing: 'flask', lean: 'feather', architecture: 'layers', mcp: 'plug', skills: 'wand', performance: 'gauge',
  design: 'palette', plan: 'list', database: 'database', payments: 'card', platform: 'cloud', auth: 'lock', measure: 'gauge',
  mobile: 'phone', documents: 'sheet', marketing: 'megaphone', games: 'gamepad', 'game-design': 'dice', code: 'terminal', services: 'cloud',
};
type Filter = 'all' | 'skills' | 'plugins' | 'servers' | 'installed';
const words = (key: string, fallback: string) => key in en ? t(key as Key) : fallback;
const plugin = (row: ToolRow) => !!row.pluginParts?.length;
const needsNode = (row: ToolRow) => (row.server || row.requiresNode) && row.nodeAvailable === false;
const glyph = (row: ToolRow): GlyphName => plugin(row) ? 'puzzle' : row.server ? 'globe' : glyphs[row.job] ?? 'wand';
const tone = (row: ToolRow) => row.status || needsNode(row) ? 'warn' : !row.installed ? 'none' : row.enabled === false ? 'off' : 'on';
type Batch = 'install' | 'on' | 'off' | 'remove';
const fits = (row: ToolRow, action: Batch) => action === 'install' ? !needsNode(row) && (!row.installed || !!row.status)
  : action === 'remove' ? row.installed : row.installed && !row.status && (row.enabled === false) === (action === 'on');
const partsSummary =(row: ToolRow) => (['skill', 'command', 'agent', 'server'] as const).flatMap((kind) => {
  const n = row.pluginParts?.filter((part) => part.kind === kind).length ?? 0;
  return n ? [t(`catalog.parts.${kind}`, { n })] : [];
}).join(' · ');

function PluginLimits(props: { row: ToolRow; claude: boolean }) {
  const limits = () => [
    props.row.pluginParts?.some((part) => part.kind === 'command') && t('catalog.pluginCodexCommands'),
    props.row.pluginParts?.some((part) => part.kind === 'command' || part.kind === 'agent') && t('catalog.pluginProjectsOnly'),
    props.claude && props.row.claudePluginsAvailable === false && props.row.pluginParts?.some((part) => part.kind === 'command' || part.kind === 'agent') && t('catalog.pluginClaudeUnavailable'),
  ].filter(Boolean).join(' ');
  return <Show when={plugin(props.row) && limits()}>{(text) => <span class="catalog-description">{text()}</span>}</Show>;
}

// What was filtered, searched, paged and selected, held while Faber runs: leaving the Catalog and coming back finds it as it was.
const held: { query: string; filter: Filter; page: number; selected?: string } = { query: '', filter: 'all', page: 0 };

/** Back to an unfiltered list, for the screen checks that each start from a fresh Catalog. */
export const forgetCatalogView = (): void => { Object.assign(held, { query: '', filter: 'all', page: 0, selected: undefined }); };

/** Browse only while open: no CLI launch, polling or hidden card tree. */
export function Catalog(props: { projects: Project[]; projectId?: string; vendors: Vendor[] }) {
  const [rows, setRows] = createStore<ToolRow[]>([]);
  const [error, setError] = createSignal<string>();
  const [retry, setRetry] = createSignal<() => void>();
  const [loading, setLoading] = createSignal(true);
  const [query, setQuery] = createSignal(held.query);
  const [filter, setFilter] = createSignal<Filter>(held.filter);
  const [page, setPage] = createSignal(held.page);
  const [selected, setSelected] = createSignal<string | undefined>(held.selected);
  createEffect(() => { held.query = query(); held.filter = filter(); held.page = page(); held.selected = selected(); });
  const [busy, setBusy] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);
  const [removed, setRemoved] = createSignal<{ id: string; enabled: boolean }>();
  const [revision, setRevision] = createSignal(0);
  const [picked, setPicked] = createSignal<string[]>([]);
  const [progress, setProgress] = createSignal<{ done: number; total: number }>();
  const [confirmingBatch, setConfirmingBatch] = createSignal(false);
  const [batchError, setBatchError] = createSignal<{ text: string; action: Batch }>();
  const [projectChoice, setProjectChoice] = createSignal(props.projectId ?? '');
  const projectId = () => props.projects.some((p) => p.id === projectChoice()) ? projectChoice() : undefined;
  let gone = false;
  let stop: (() => void) | undefined;
  let readStarted = false;
  let read = 0;
  let refreshAfterAction = false;
  let search: HTMLInputElement | undefined;
  let surface: HTMLElement | undefined;
  let detail: HTMLHeadingElement | undefined;
  let undo: HTMLButtonElement | undefined;
  let cancelBatch: HTMLButtonElement | undefined;
  let returnCard: HTMLElement | undefined;
  const load = async () => {
    setLoading(true);
    setError(undefined);
    try {
      await api.toolsClearRemoved();
      readStarted = true;
      const current = ++read;
      const list = await api.toolsList();
      if (!gone && current === read) setRows(reconcile(list));
    } catch (err) {
      if (!gone) { setError(failureText(err)); setRetry(() => () => void load()); }
    } finally { if (!gone) setLoading(false); }
  };
  // The screen's optimistic rows and the engine's replies differ in shape (absent or null status, the flag of a removed item): compare what the screen shows.
  const same = (list: readonly ToolRow[]) => JSON.stringify(list.map((r) => [r.id, r.installed, r.installed ? r.enabled !== false : true, r.status ?? null, r.version ?? null, r.nodeAvailable ?? null]));
  const refresh = async () => {
    if (gone) return;
    if (busy()) { refreshAfterAction = true; return; }
    const current = ++read;
    try {
      const list = await api.toolsList();
      if (!gone && current === read) {
        // Rereading the inventory remounts the server switches: only when the catalog really changed, never for an identical catch-up read.
        const changed = same(list) !== same(rows);
        setRows(reconcile(list));
        if (changed) setRevision((v) => v + 1);
      }
    } catch (err) {
      if (!gone && current === read) { setError(failureText(err)); setRetry(() => () => void refresh()); }
    }
  };
  onMount(() => {
    void api.on((event) => { if (event.type === 'catalog') void refresh(); }).then((unlisten) => {
      if (gone) unlisten();
      else { stop = unlisten; if (readStarted) void refresh(); }
    }).catch(() => {});
    void load();
  });
  const key = (event: KeyboardEvent) => {
    if (surface?.closest('[inert]') || document.querySelector('.pal-scrim, .keys-sheet, .cmenu')) return;
    if (pressed(event, 'find')) {
      event.preventDefault();
      setSelected(undefined);
      queueMicrotask(() => search?.focus());
    }
    if (!event.defaultPrevented && event.key === 'Escape' && selected()) { event.preventDefault(); if (confirming()) setConfirming(false); else back(); }
    // The search field keeps its own Escape, which clears what was typed.
    else if (!event.defaultPrevented && event.key === 'Escape' && event.target !== search && chosen().length && !busy()) { event.preventDefault(); if (confirmingBatch()) setConfirmingBatch(false); else setPicked([]); }
  };
  window.addEventListener('keydown', key);
  onCleanup(() => { gone = true; stop?.(); window.removeEventListener('keydown', key); });
  const starters = createMemo(() => {
    const recommended = rows.filter((row) => row.tier === 'recommended');
    const seen = new Set<string>();
    const first = recommended.filter((row) => {
      const group = job(row);
      if (seen.has(group)) return false;
      seen.add(group); return true;
    });
    return [...first, ...recommended.filter((row) => !first.includes(row))].slice(0, 12);
  });
  const matching = createMemo(() => {
    const text = query().trim().toLocaleLowerCase();
    const list = filter() === 'all' && !text ? [...starters(), ...rows.filter((row) => !starters().includes(row))] : rows;
    return list.filter((row) => (filter() !== 'skills' || (!row.server && !plugin(row))) && (filter() !== 'servers' || row.server)
      && (filter() !== 'plugins' || plugin(row))
      && (filter() !== 'installed' || row.installed || row.status)
      && (!text || `${row.id} ${catalogName(row.id)} ${row.repo} ${words(`tools.${row.id}`, row.id)} ${words(`catalog.job.${row.job}`, row.job)} ${words(`catalog.job.${job(row)}`, job(row))}`.toLocaleLowerCase().includes(text)));
  });
  createEffect(() => {
    if (loading()) return;
    const last = Math.max(0, Math.ceil(matching().length / PAGE) - 1);
    if (page() > last) setPage(last);
  });
  const visible = () => matching().slice(page() * PAGE, (page() + 1) * PAGE);
  const groups = createMemo(() => {
    const list = visible();
    const good = filter() === 'all' && !query().trim() && page() === 0 ? list.filter((r) => starters().includes(r)) : [];
    return [
      ...(good.length ? [{ name: t('catalog.goodStart'), job: '', rows: good }] : []),
      ...[...new Set(list.filter((r) => !good.includes(r)).map(job))].map((group) => ({
        name: words(`catalog.job.${group}`, t('catalog.other')), job: group, rows: list.filter((r) => job(r) === group && !good.includes(r)),
      })),
    ];
  });
  const item = () => rows.find((r) => r.id === selected());
  const status = (id: string) => rows.find((row) => row.id === id)?.status ?? undefined;
  const hasServer = (row: ToolRow, name: string) => (row.server && `faber-${row.id}` === name) || row.pluginParts?.some((part) => part.kind === 'server' && `faber-${row.id}--${part.id}` === name);
  const serverStatus = (name: string) => rows.find((row) => hasServer(row, name))?.status ?? undefined;
  const available = (name: string) => rows.some((r) => hasServer(r, name) && r.installed && r.enabled !== false && !r.status);
  const state = (row: ToolRow) => row.status ? fromEngine(row.status) : needsNode(row) ? t('catalog.needsNode') : t(row.installed ? row.enabled === false ? 'catalog.off' : 'catalog.on' : 'catalog.notInstalled');
  const site = (row: ToolRow) => void api.openLink(row.home).catch((e) => setError(failureText(e)));
  // Rows the engine no longer lists drop out of the selection by themselves.
  const chosen = createMemo(() => rows.filter((row) => picked().includes(row.id)));
  const pick = (id: string, on: boolean) => { setConfirmingBatch(false); setBatchError(undefined); setPicked((list) => on ? [...list, id] : list.filter((x) => x !== id)); };
  const allShown = () => visible().length > 0 && visible().every((row) => picked().includes(row.id));
  const pickShown = () => {
    const ids = visible().map((row) => row.id), all = allShown();
    setConfirmingBatch(false); setBatchError(undefined);
    setPicked((list) => all ? list.filter((id) => !ids.includes(id)) : [...new Set([...list, ...ids])]);
  };
  const focusCard = (id: string) => queueMicrotask(() => [...document.querySelectorAll<HTMLElement>('[data-catalog-card]')].find((el) => el.dataset.catalogCard === id)?.focus());
  // One item after another through the same store calls as the detail. Done items leave the selection; failed ones and
  // the ones the action did not fit stay, and the failure shows in the bar beside them, where the person is looking.
  const runBatch = async (action: Batch) => {
    const ids = chosen().filter((row) => fits(row, action)).map((row) => row.id);
    if (busy() || !ids.length) return;
    ++read;
    setBusy(true); setError(undefined); setRetry(undefined); setBatchError(undefined); setConfirmingBatch(false);
    // Undo keeps only the latest single removal: a group removal says so in its question, and any group action on the
    // item waiting for Undo makes that Undo stale.
    if (action === 'remove' || ids.includes(removed()?.id ?? '')) setRemoved(undefined);
    const done: string[] = [];
    let reason = '';
    for (const [index, id] of ids.entries()) {
      if (gone) return;
      setProgress({ done: index + 1, total: ids.length });
      try {
        if (action === 'install') await api.toolsInstall(id);
        else if (action === 'remove') await api.toolsRemove(id);
        else await api.toolsSetEnabled(id, action === 'on');
        if (gone) return;
        done.push(id);
        const at = rows.findIndex((r) => r.id === id);
        if (at >= 0) setRows(at, { installed: action !== 'remove', enabled: action !== 'off', status: undefined });
      } catch (err) { reason = failureText(err); }
    }
    if (gone) return;
    const failed = ids.length - done.length;
    setProgress(undefined);
    setPicked((list) => list.filter((id) => !done.includes(id)));
    if (failed) setBatchError({ text: `${t('catalog.batchFailed', { n: failed })} ${reason}`, action });
    else focusCard(ids[0]);
    setRevision((v) => v + 1);
    setBusy(false);
    if (refreshAfterAction) { refreshAfterAction = false; void refresh(); }
  };
  const open = (row: ToolRow, button: HTMLElement) => {
    returnCard = button;
    setSelected(row.id);
    setConfirming(false);
    setError(undefined);
    queueMicrotask(() => detail?.focus());
  };
  const back = () => {
    const id = selected();
    setSelected(undefined); setConfirming(false); setError(undefined);
    queueMicrotask(() => { const card = [...document.querySelectorAll<HTMLElement>('[data-catalog-card]')].find((el) => el.dataset.catalogCard === id); (card ?? (returnCard?.isConnected ? returnCard : search))?.focus(); });
  };
  const change = async (id: string, action: 'install' | 'remove' | 'toggle' | 'restore', enabled = true): Promise<boolean> => {
    if (busy()) return false;
    ++read;
    const wasEnabled = rows.find((r) => r.id === id)?.enabled !== false;
    const restoring = removed();
    setBusy(true); setError(undefined); setRetry(undefined);
    try {
      if (action === 'install') await api.toolsInstall(id);
      else if (action === 'remove') await api.toolsRemove(id);
      else if (action === 'restore') await api.toolsRestore(id);
      else await api.toolsSetEnabled(id, enabled);
      if (gone) return true;
      // Update fields in place so a successful toggle keeps its native input and focus.
      const index = rows.findIndex((r) => r.id === id);
      if (index >= 0) setRows(index, { installed: action !== 'remove', enabled: action === 'toggle' ? enabled : action === 'restore' ? restoring?.enabled !== false : true, status: undefined });
      if (action === 'remove') { setRemoved({ id, enabled: wasEnabled }); setConfirming(false); queueMicrotask(() => undo?.focus()); }
      else if (removed()?.id === id) setRemoved(undefined);
      if (action === 'install' || action === 'restore') queueMicrotask(() => detail?.focus());
      setRevision((v) => v + 1);
      return true;
    } catch (err) {
      if (!gone) { setError(failureText(err)); setRetry(() => () => void change(id, action, enabled)); }
      return false;
    } finally { if (!gone) { setBusy(false); if (refreshAfterAction) { refreshAfterAction = false; void refresh(); } } }
  };
  const projectServers = (row: ToolRow, names: { serverName?: string; serverNames?: string[] }) => <>
    <h2>{t('catalog.projectServers')}</h2><p class="hint">{t('catalog.projectChoice')}</p>
    <McpServers status={serverStatus} globallyEnabled={row.enabled !== false && !row.status} revision={revision()} projectId={projectId()} projects={props.projects} {...names} onError={(e) => { setError(failureText(e)); setRetry(() => () => { setError(undefined); setRevision((v) => v + 1); }); }} />
  </>;
  const count = (action: Batch) => chosen().filter((row) => fits(row, action)).length;
  return (
    <section class="catalog" ref={(el) => (surface = el)} data-catalog data-picking={chosen().length ? '' : undefined} aria-label={t('area.catalog')}>
      <div class="catalog-content">
        <Show when={props.vendors.length === 0}><p class="hint">{t('catalog.noAi')}</p></Show>
        <Show when={error()}><div class="catalog-notice" role="alert"><span>{error()}</span><Show when={retry()}><button class="btn" disabled={busy()} onClick={() => retry()?.()}>{t('catalog.retry')}</button></Show></div></Show>
        <Show when={removed()}><div class="catalog-notice" role="status"><span>{t('catalog.removed', { name: catalogName(removed()!.id) })}</span><button class="btn" ref={(el) => (undo = el)} disabled={busy()} onClick={() => void change(removed()!.id, 'restore')}>{t('catalog.undo')}</button></div></Show>
        <Show when={item()} fallback={<>
          <header class="catalog-top">
            <h1>{t('area.catalog')}</h1>
            <label class="field catalog-search"><Icon name="search" /><input ref={(el) => (search = el)} type="search" aria-label={t('catalog.search')} placeholder={t('catalog.search')} value={query()} onInput={(e) => { setQuery(e.currentTarget.value); setPage(0); }} /><kbd>{keysOf('find')}</kbd></label>
          </header>
          <div class="catalog-bar">
            <div class="catalog-filters" role="group" aria-label={t('catalog.filter')}>
              <For each={['all', 'skills', ...(rows.some(plugin) ? ['plugins' as const] : []), 'servers', 'installed'] as const}>{(kind) => <button class="pill" aria-pressed={filter() === kind} onClick={() => { setFilter(kind); setPage(0); }}>{t(`catalog.${kind}`)}</button>}</For>
            </div>
            <Show when={filter() !== 'installed' && visible().length}><button class="btn ghost" data-catalog-pick-shown onClick={pickShown}><Icon name="check" small />{t(allShown() ? 'catalog.unpickShown' : 'catalog.pickShown')}</button></Show>
          </div>
          <Show when={loading()}><div class="catalog-empty" role="status"><Peek /><p>{t('catalog.loading')}</p></div></Show>
          <Show when={filter() !== 'installed' && !loading() && !matching().length && !error()}><div class="catalog-empty" role="status"><Peek /><p>{t(query().trim() ? 'catalog.noMatch' : 'catalog.empty')}</p></div></Show>
          <For each={groups()}>{(group) => <section class="catalog-group"><h2><Show when={glyphs[group.job]}>{(name) => <Icon name={name()} small />}</Show>{group.name}</h2><div class="catalog-grid"><For each={group.rows}>{(row) => <div class="catalog-tile" data-tone={tone(row)} classList={{ picked: picked().includes(row.id) }}>
            <button class="catalog-card" data-catalog-card={row.id} onClick={(e) => open(row, e.currentTarget)}>
              <span class="catalog-icon"><Icon name={glyph(row)} /></span>
              <span class="catalog-name">{catalogName(row.id)}</span><span class="catalog-description">{words(`tools.${row.id}`, row.id)}</span>
              <Show when={plugin(row)}><span class="catalog-description">{partsSummary(row)}</span><PluginLimits row={row} claude={props.vendors.includes('claude')} /></Show>
              <span class="catalog-origin">{row.repo} · {row.license}</span><span class="catalog-state">{state(row)}</span>
            </button>
            <input type="checkbox" class="catalog-pick" aria-label={t('catalog.pick', { name: catalogName(row.id) })} checked={picked().includes(row.id)} onChange={(e) => pick(row.id, e.currentTarget.checked)} />
            <button class="iconbtn catalog-site" aria-label={t('catalog.openSite', { repo: row.repo })} title={t('catalog.openSite', { repo: row.repo })} onClick={() => site(row)}><Icon name="external" small /></button>
            <Show when={fits(row, 'install')}><button class="btn secondary catalog-quick" aria-label={t('catalog.installName', { name: catalogName(row.id) })} onClick={() => void change(row.id, 'install').then((done) => done && focusCard(row.id))}><Icon name="download" small />{t('catalog.install')}</button></Show>
            <Show when={row.installed && !row.status}><input type="checkbox" role="switch" class="sw2 catalog-quick" aria-label={t('catalog.switchName', { name: catalogName(row.id) })} checked={row.enabled !== false} onChange={async (e) => { const box = e.currentTarget; if (!(await change(row.id, 'toggle', box.checked))) box.checked = row.enabled !== false; }} /></Show>
          </div>}</For></div></section>}</For>
          <Show when={matching().length > PAGE}><nav class="catalog-pages" aria-label={t('catalog.pages')}><button class="btn" disabled={!page()} onClick={() => setPage(page() - 1)}>{t('catalog.previous')}</button><span aria-live="polite">{t('catalog.page', { n: page() + 1, total: Math.ceil(matching().length / PAGE) })}</span><button class="btn" disabled={(page() + 1) * PAGE >= matching().length} onClick={() => setPage(page() + 1)}>{t('catalog.next')}</button></nav></Show>
          <Show when={filter() === 'installed'}><CatalogInventory query={query()} catalogMatches={matching().length} status={status} serverStatus={serverStatus} available={available} revision={revision()} projectId={projectId()} projects={props.projects} onProject={setProjectChoice} onOpen={(id) => { const row = rows.find((r) => r.id === id); if (row) open(row, search!); }} /></Show>
          <Show when={chosen().length}><div class="catalog-batch" role="region" aria-label={t('catalog.selection')} data-catalog-batch>
            <Show when={batchError()}>{(failure) => <span class="catalog-batch-error" role="alert">{failure().text}<button class="btn" onClick={() => void runBatch(failure().action)}>{t('catalog.retry')}</button></span>}</Show>
            <span class="catalog-batch-count" aria-live="polite">{progress() ? t('catalog.batchWorking', { n: progress()!.done, total: progress()!.total }) : t('catalog.picked', { n: chosen().length })}</span>
            <Show when={confirmingBatch()} fallback={<div class="catalog-batch-actions">
              {/* Only the actions that apply to something selected. */}
              <For each={(['install', 'on', 'off'] as const).filter((action) => count(action))}>{(action) => <button class="btn" classList={{ primary: action === 'install' }} data-batch={action} onClick={() => void runBatch(action)}>{t(`catalog.batch.${action}`, { n: count(action) })}</button>}</For>
              <Show when={count('remove')}><button class="btn" data-batch="remove" onClick={() => { if (!busy()) { setConfirmingBatch(true); queueMicrotask(() => cancelBatch?.focus()); } }}>{t('catalog.batch.remove', { n: count('remove') })}</button></Show>
              <button class="iconbtn" aria-label={t('catalog.unpick')} title={t('catalog.unpick')} onClick={() => setPicked([])}><Icon name="close" small /></button>
            </div>}>
              <div class="catalog-batch-actions"><span>{t('catalog.confirmRemoveMany', { n: count('remove') })}</span><button class="btn" ref={(el) => (cancelBatch = el)} onClick={() => setConfirmingBatch(false)}>{t('catalog.cancel')}</button><button class="btn danger" data-batch="confirm-remove" onClick={() => void runBatch('remove')}>{t('catalog.remove')}</button></div>
            </Show>
          </div></Show>
        </>}>
          {(row) => <article class="catalog-detail" aria-busy={busy()} data-tone={tone(row())}>
            <button class="btn" onClick={back}><Icon name="left" small />{t('catalog.back')}</button>
            <header class="catalog-hero">
              <span class="catalog-icon"><Icon name={glyph(row())} /></span>
              <div class="catalog-hero-text">
                <h1 ref={(el) => (detail = el)} tabindex="-1">{catalogName(row().id)}</h1>
                <p class="catalog-meta"><span>{words(`catalog.job.${row().job}`, words(`catalog.job.${job(row())}`, t('catalog.other')))}</span><span class="catalog-state">{state(row())}</span></p>
              </div>
              <button class="btn secondary" onClick={() => site(row())}><Icon name="external" small />{t('catalog.openSource')}</button>
            </header>
            <Show when={row().cliName && row().cliName !== row().id}><p>{t('catalog.cliName', { name: row().cliName! })}</p></Show>
            <section class="catalog-gain"><h2>{t('catalog.gain')}</h2><p class="catalog-description">{words(`tools.${row().id}`, row().id)}</p></section>
            <Show when={needsNode(row())}><p>{t(plugin(row()) ? 'catalog.pluginNodeHelp' : 'catalog.nodeHelp')}</p><button class="link" onClick={() => void api.openLink('https://nodejs.org/en/download').catch((e) => setError(failureText(e)))}>{t('catalog.getNode')}</button></Show>
            <div class="catalog-actions">
              <Show when={row().installed} fallback={<button class="btn primary big" disabled={busy() || !!needsNode(row())} onClick={() => void change(row().id, 'install')}>{t(busy() ? 'catalog.working' : 'catalog.install')}</button>}>
                <Show when={row().status}><button class="btn primary big" disabled={busy() || !!needsNode(row())} onClick={() => void change(row().id, 'install')}>{t(busy() ? 'catalog.working' : 'catalog.install')}</button></Show>
                <Toggle id="catalog-enabled" label={t('catalog.enabled')} hint={t('catalog.nextConversation')} checked={!row().status && row().enabled !== false} disabled={!!row().status} onChange={(next) => change(row().id, 'toggle', next)} />
                <Show when={confirming()} fallback={<button class="btn danger" disabled={busy()} onClick={() => setConfirming(true)}>{t('catalog.remove')}</button>}>
                  <div class="catalog-notice"><span>{t('catalog.confirmRemove', { name: catalogName(row().id) })}</span><button class="btn" disabled={busy()} onClick={() => setConfirming(false)}>{t('catalog.cancel')}</button><button class="btn danger" disabled={busy()} onClick={() => void change(row().id, 'remove')}>{t('catalog.remove')}</button></div>
                </Show>
              </Show>
            </div>
            <h2>{t('catalog.how')}</h2>
            <ul class="catalog-how">
              <li><span class="catalog-icon"><Icon name="download" small /></span><div><h3>{t('catalog.howInstall')}</h3><p>{t('catalog.safeInstall')}</p></div></li>
              <li><span class="catalog-icon"><Icon name="chat" small /></span><div><h3>{t('catalog.when')}</h3><p>{t(plugin(row()) ? 'catalog.whenPlugin' : row().server ? 'catalog.whenServer' : 'catalog.whenSkill')}</p>
                <PluginLimits row={row()} claude={props.vendors.includes('claude')} />
                <Show when={words(`tools.${row().id}.needs`, '')}>{(needs) => <p>{needs()}</p>}</Show></div></li>
              <li><span class="catalog-icon"><Icon name="shield" small /></span><div><h3>{t('catalog.access')}</h3>
                <Show when={plugin(row())} fallback={<p>{t(row().server ? 'catalog.accessServer' : 'catalog.accessSkill')}</p>}>
                  <p>{t('catalog.pluginParts')}</p>
                  <ul class="catalog-source-list"><For each={row().pluginParts}>{(part) => <li data-plugin-part={part.id}><div><span>{part.id} · {t(`catalog.part.${part.kind}`)}</span><p class="hint">{t(part.kind === 'skill' ? 'catalog.accessSkill' : part.kind === 'server' ? 'catalog.accessPluginServer' : part.kind === 'command' ? 'catalog.accessCommand' : 'catalog.accessAgent')}</p></div></li>}</For></ul>
                </Show></div></li>
            </ul>
            <dl><dt>{t('catalog.source')}</dt><dd><button class="link" onClick={() => site(row())}>{row().repo}<Icon name="external" small /></button></dd><dt>{t('catalog.license')}</dt><dd>{row().license}</dd><dt>{t('catalog.version')}</dt><dd class="catalog-version">{row().version}</dd></dl>
            <Show when={row().server && row().installed}>{projectServers(row(), { serverName: `faber-${row().id}` })}</Show>
            <Show when={plugin(row()) && row().installed && row().pluginParts?.some((part) => part.kind === 'server')}>{projectServers(row(), { serverNames: row().pluginParts?.filter((part) => part.kind === 'server').map((part) => `faber-${row().id}--${part.id}`) })}</Show>
          </article>}
        </Show>
      </div>
    </section>
  );
}
