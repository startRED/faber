import { createEffect, createResource, createSignal, For, onCleanup, Show } from 'solid-js';
import { createStore } from 'solid-js/store';
import { api, type McpServerList, type McpServerRow, type Project } from './api.ts';
import { Opt, Toggle } from './controls.tsx';
import { fromEngine, lang, t } from './i18n.ts';
import type { Key } from './locales/en.ts';

const statuses: Record<string, Key> = {
  connected: 'set.mcp.status.connected',
  failed: 'set.mcp.status.failed',
  pending: 'set.mcp.status.pending',
  starting: 'set.mcp.status.starting',
  'needs-auth': 'set.mcp.status.authenticationRequired',
  authenticationRequired: 'set.mcp.status.authenticationRequired',
  disabled: 'set.mcp.status.disabled',
  notStarted: 'set.mcp.status.notStarted',
  cancelled: 'set.mcp.status.cancelled',
};

/** Mounted only while the catalog is open: one snapshot, no timer or CLI launch. */
export function McpServers(props: { revision: number; projects?: Project[]; projectId?: string; serverName?: string; serverNames?: string[]; fixedProject?: boolean; globallyEnabled?: boolean; available?: (name: string) => boolean; status?: (name: string) => string | undefined; query?: string; onCount?: (value: { total: number; matching: number; ready: boolean }) => void; onError?: (error: unknown) => void }) {
  let gone = false;
  onCleanup(() => (gone = true));
  const [projects] = createResource(() => props.projects ? Promise.resolve(props.projects) : api.state().then((state) => state.projects).catch((error) => {
    if (!gone) props.onError?.(error);
    return [];
  }));
  const [selected, setSelected] = createSignal('');
  const projectId = () => selected() || props.projectId || projects()?.[0]?.id;
  const [lists] = createResource(
    () => projectId() ? { id: projectId()!, revision: props.revision } : undefined,
    ({ id }) => api.mcpServers(id).catch((error) => {
      if (!gone && projectId() === id) props.onError?.(error);
      return [];
    }),
  );
  const [groups, setGroups] = createStore<McpServerList[]>([]);
  createEffect(() => setGroups(lists() ?? []));
  const [busy, setBusy] = createSignal<string>();
  const flip = async (name: string, enabled: boolean): Promise<boolean> => {
    if (busy() !== undefined || props.globallyEnabled === false || props.available?.(name) === false) return false;
    const id = projectId()!;
    setBusy(name);
    try {
      await api.setMcpServer(id, name, enabled);
      if (!gone && projectId() === id) setGroups(() => true, 'rows', (row) => row.name === name && row.source === 'catalog', 'enabled', enabled);
      return true;
    } catch (error) {
      if (!gone) props.onError?.(error);
      return false;
    } finally {
      if (!gone) setBusy(undefined);
    }
  };
  const hint = (row: McpServerRow) => `${t(`set.mcp.source.${row.source}`)} · ${row.source === 'catalog' && props.status?.(row.name) ? fromEngine(props.status(row.name)!) : t(statuses[row.status ?? ''] ?? 'set.mcp.status.unknown')}`;
  const off = (row: McpServerRow) => row.source === 'catalog' && (props.globallyEnabled === false || props.available?.(row.name) === false);
  const matching = (row: McpServerRow) => (!props.serverName || row.name === props.serverName) && (!props.serverNames || props.serverNames.includes(row.name)) && (!props.query?.trim() || `${row.name} ${hint(row)}`.toLocaleLowerCase().includes(props.query.trim().toLocaleLowerCase()));
  createEffect(() => props.onCount?.({ total: groups.reduce((n, group) => n + group.rows.length, 0), matching: groups.reduce((n, group) => n + group.rows.filter(matching).length, 0), ready: lists() !== undefined && !lists.loading }));
  return (
    <div data-mcp-servers>
      <Show when={projects()?.length} fallback={<Show when={!projects.loading}><p class="hint st-note">{t('set.projects.none')}</p></Show>}>
        <Show when={!props.fixedProject}><Opt id="set-mcp-project" label={t('set.mcp.project')} htmlFor="set-mcp-project-select">
          <select id="set-mcp-project-select" class="pill" value={projectId()} onChange={(event) => setSelected(event.currentTarget.value)}>
            <For each={projects()}>{(project) => <option value={project.id}>{project.name}</option>}</For>
          </select>
        </Opt></Show>
        <Show when={lists.loading}><p class="hint st-note">{t('set.mcp.loading')}</p></Show>
        <Show when={!lists.loading}>
          <For each={groups}>{(group) => (
            <section data-mcp-vendor={group.vendor}>
              <h3 class="st-title">{t(group.vendor === 'claude' ? 'setup.claude' : 'setup.chatgpt')}</h3>
              <p class="hint st-note">{group.readAt === undefined ? t('set.mcp.unread') : t('set.mcp.readAt', { at: new Date(group.readAt * 1000).toLocaleString(lang()) })}</p>
              <For each={group.rows.filter(matching)}>{(row, index) => (
                <div data-mcp-row={row.name} data-global-off={off(row) || undefined} aria-busy={busy() === row.name}>
                  <Show when={row.source === 'catalog'} fallback={<Opt id={`set-mcp-${group.vendor}-${index()}`} label={row.name} hint={hint(row)} />}>
                    <Toggle id={`set-mcp-${group.vendor}-${index()}`} label={row.name} hint={off(row) ? `${hint(row)} · ${t('catalog.off')}` : hint(row)} checked={!off(row) && (row.enabled ?? true)} disabled={off(row) || (busy() !== undefined && busy() !== row.name)} onChange={(next) => flip(row.name, next)} />
                    <p class="hint st-note">{t('set.mcp.nextLaunch')}</p>
                  </Show>
                </div>
              )}</For>
            </section>
          )}</For>
        </Show>
      </Show>
    </div>
  );
}
