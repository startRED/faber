import { createEffect, createMemo, createResource, createSignal, For, on, Show } from 'solid-js';
import { api, type Project } from './api.ts';
import { McpServers } from './McpServers.tsx';
import { failureText, t } from './i18n.ts';

/** File discovery, not an invented live CLI report. External installs stay read-only. */
export function CatalogInventory(props: { revision: number; projectId?: string; projects: Project[]; query?: string; catalogMatches?: number; available?: (name: string) => boolean; onProject: (id: string) => void; onOpen: (id: string) => void }) {
  const [retry, setRetry] = createSignal(0);
  const [error, setError] = createSignal<string>();
  const [lists] = createResource(() => ({ project: props.projectId, revision: props.revision, retry: retry() }), async ({ project }) => { setError(undefined); try { return await api.toolsInventory(project); } catch (err) { setError(failureText(err)); return []; } });
  const [page, setPage] = createSignal(0);
  const [servers, setServers] = createSignal({ total: 0, matching: 0, ready: false });
  createEffect(on(() => [props.query, props.projectId, props.revision], () => setPage(0)));
  const filtered = createMemo(() => {
    const query = props.query?.trim().toLocaleLowerCase() ?? '';
    return (lists() ?? []).map((list) => ({ ...list, rows: list.rows.filter((row) => !query ||
      `${row.name} ${t(`catalog.kind.${row.kind}`)} ${t(`set.mcp.source.${row.source}`)} ${row.location === 'pluginRecord' ? t('catalog.pluginRecord') : row.location}`.toLocaleLowerCase().includes(query)) }));
  });
  const count = () => Math.max(0, ...filtered().map((list) => list.rows.length));
  const matches = () => (props.catalogMatches ?? 0) + filtered().reduce((n, list) => n + list.rows.length, 0) + (props.projectId ? servers().matching : 0);
  const unavailable = () => lists()?.some((list) => list.issues.some((issue) => issue.reason === 'unavailable' || issue.reason === 'unreadable'));
  const ready = () => !lists.loading && (!props.projectId || servers().ready);
  return <section class="catalog-inventory" data-catalog-inventory>
    <h2>{t('catalog.cliSees')}</h2>
    <p class="hint">{t('catalog.discovery')}</p>
    <label class="catalog-project">{t('catalog.project')}<select class="field" aria-label={t('catalog.project')} value={props.projectId ?? ''} onChange={(e) => { props.onProject(e.currentTarget.value); setPage(0); }}>
      <option value="">{t('catalog.personOnly')}</option><For each={props.projects}>{(project) => <option value={project.id}>{project.name}</option>}</For>
    </select></label>
    <Show when={lists.loading}><p role="status">{t('catalog.loading')}</p></Show>
    <Show when={error()}><p role="alert">{error()}</p><button class="btn" onClick={() => setRetry(retry() + 1)}>{t('catalog.retry')}</button></Show>
    <Show when={unavailable() && !error()}><button class="btn" onClick={() => setRetry(retry() + 1)}>{t('catalog.retry')}</button></Show>
    <Show when={ready() && !error() && !unavailable() && !matches()}><p class="catalog-empty" role="status">{t(props.query?.trim() ? 'catalog.noMatch' : 'catalog.emptyInstalled')}</p></Show>
    <For each={filtered()}>{(list) => <section data-inventory-vendor={list.vendor}>
      <h3>{t(list.vendor === 'claude' ? 'setup.claude' : 'setup.chatgpt')}</h3>
      <Show when={!list.rows.length && !props.query?.trim() && !list.issues.some((issue) => issue.reason === 'unavailable' || issue.reason === 'unreadable')}><p class="hint">{t('catalog.noneFound')}</p></Show>
      <ul class="catalog-source-list"><For each={list.rows.slice(page() * 48, (page() + 1) * 48)}>{(row) => <li data-inventory-source={row.source}>
        <div><Show when={row.catalogId} fallback={<span>{row.name}</span>}>{(id) => <button class="link" onClick={() => props.onOpen(id())}>{row.name}</button>}</Show><span class="hint">{t(`catalog.kind.${row.kind}`)} · {t(`set.mcp.source.${row.source}`)}</span></div>
        <span class="catalog-location">{row.location === 'pluginRecord' ? t('catalog.pluginRecord') : row.location}</span><Show when={row.linked}><span class="hint" data-inventory-linked>{t('catalog.linked')}</span></Show><Show when={row.enabled !== undefined}><span class="hint">{t(row.enabled ? 'catalog.on' : 'catalog.off')}</span></Show>
      </li>}</For></ul>
      <For each={list.issues}>{(issue) => <p class="hint" data-inventory-issue>{t(`set.mcp.source.${issue.source}`)} · {t(`catalog.kind.${issue.kind}`)}: {t(list.vendor === 'codex' && issue.kind === 'plugin' && issue.reason === 'unsupported' ? 'catalog.pluginUpdates' : `catalog.issue.${issue.reason}`)}</p>}</For>
    </section>}</For>
    <Show when={count() > 48}><nav class="catalog-pages" aria-label={t('catalog.pages')}><button class="btn" disabled={!page()} onClick={() => setPage(page() - 1)}>{t('catalog.previous')}</button><span>{t('catalog.page', { n: page() + 1, total: Math.ceil(count() / 48) })}</span><button class="btn" disabled={(page() + 1) * 48 >= count()} onClick={() => setPage(page() + 1)}>{t('catalog.next')}</button></nav></Show>
    <Show when={props.projectId}><h3>{t('catalog.reportedServers')}</h3><McpServers query={props.query} available={props.available} onCount={setServers} revision={props.revision + retry()} projectId={props.projectId} projects={props.projects} fixedProject onError={(err) => setError(failureText(err))} /></Show>
  </section>;
}
