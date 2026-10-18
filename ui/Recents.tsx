import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api, type FolderView, type Project } from './api.ts';
import { openMenu } from './contextmenu.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { animateList } from './motion.ts';

/** A long folder path with its middle cut out: the drive (or root) and the last two folders stay, the full path goes in the tooltip. */
export function shortPath(path: string): string {
  const parts = path.split(/[\\/]/);
  if (parts.length <= 3) return path;
  const sep = path.includes('\\') ? '\\' : '/';
  return [parts[0], '…', ...parts.slice(-2)].join(sep);
}

type Section = 'rec' | 'env';

/** Reads the list again without remaking the rows that did not change, so forgetting one entry moves only that row. */
function steady<T>(key: (item: T) => string) {
  let kept = new Map<string, T>();
  return (list: T[] | undefined): T[] => {
    const next = new Map<string, T>();
    const out = (list ?? []).map((item) => {
      const old = kept.get(key(item));
      const same = old !== undefined && JSON.stringify(old) === JSON.stringify(item);
      next.set(key(item), same ? old : item);
      return same ? old : item;
    });
    kept = next;
    return out;
  };
}

/** The last five folders (one click each) and, in the project area, the saved environments and the tray that builds a new one.
 * The engine judges each folder when the list is read: one that is gone or refused arrives with a reason and cannot be picked. */
export function Recents(props: {
  /** `chat` picks one working folder; `project` opens folders as projects and also keeps environments. */
  mode: 'chat' | 'project';
  /** Opens the folder the person picked (the caller decides what it means in its area). */
  onPick: (path: string) => void;
  onOpened?: (projects: Project[]) => void;
  onError: (error: unknown) => void;
  disabled?: boolean;
}) {
  const [data, { refetch }] = createResource(() => api.recents());
  const sameFolders = steady<FolderView>((f) => f.path);
  const sameEnvironments = steady<NonNullable<ReturnType<typeof data>>['environments'][number]>((e) => e.id);
  const folders = createMemo(() => sameFolders(data()?.folders));
  const environments = createMemo(() => sameEnvironments(data()?.environments));
  const [selected, setSelected] = createSignal<FolderView[]>([]);
  const [name, setName] = createSignal('');
  let box: HTMLDivElement | undefined;
  const guard = async (run: () => Promise<unknown>) => {
    try {
      await run();
    } catch (error) {
      props.onError(error);
    }
    await refetch();
  };
  /** Removes an entry and keeps focus in the list: on the entry that took its place, or on the block itself when none is left. */
  const remove = async (section: Section, index: number, run: () => Promise<unknown>) => {
    await guard(run);
    setTimeout(() => {
      const buttons = box?.querySelectorAll<HTMLElement>(`[data-section="${section}"] .rec-x`);
      (buttons?.[Math.min(index, buttons.length - 1)] ?? box)?.focus();
    }, 0);
  };
  /** Faber's own menu (C009): the one destructive item, armed first and confirmed by a second choice. */
  const menuAt = (e: MouseEvent, label: Key, run: () => Promise<unknown>, section: Section, index: number) => {
    const row = e.currentTarget as HTMLElement;
    const active = document.activeElement;
    const focus = active instanceof HTMLElement && row.contains(active) ? active : row.querySelector<HTMLElement>('.rec-x');
    openMenu(e, [{ id: 'remove', label: t(label), danger: true, confirm: t('menu.removeConfirm'), run: () => void remove(section, index, run) }], focus);
  };
  const chosen = (f: FolderView) => selected().some((s) => s.path === f.path);
  const toggle = (f: FolderView) => setSelected((all) => (chosen(f) ? all.filter((s) => s.path !== f.path) : [...all, f]));
  const paths = () => selected().map((f) => f.path);
  const save = () =>
    guard(async () => {
      await api.saveEnvironment(name(), paths());
      setName('');
      setSelected([]);
    });
  return (
    <div class="recents" ref={(el) => { box = el; animateList(el); }} tabindex="-1">
      <Show when={(data()?.folders.length ?? 0) > 0}>
        <section aria-labelledby="rec-title" data-section="rec">
          <h2 id="rec-title">{t('recents.title')}</h2>
          <ul ref={animateList}>
            <For each={folders()}>
              {(f, i) => (
                <li class="rec" classList={{ gone: !!f.reason, on: chosen(f) }} onContextMenu={(e) => menuAt(e, 'recents.forget', () => api.forgetRecent(f.path), 'rec', i())}>
                  <button type="button" class="rec-main" disabled={!!f.reason || props.disabled} title={f.path} onClick={() => props.onPick(f.path)}>
                    <span class="n">{f.name}</span>
                    <span class="d" title={f.reason ? undefined : f.parent}>{f.reason ? t(f.reason as Key) : shortPath(f.parent)}</span>
                  </button>
                  <Show when={props.mode === 'project' && !f.reason}>
                    <button type="button" class="rec-sel" aria-pressed={chosen(f)} aria-label={t('recents.select', { name: f.name })} title={t('recents.select', { name: f.name })} onClick={() => toggle(f)}>
                      <Icon name={chosen(f) ? 'check' : 'plus'} small />
                    </button>
                  </Show>
                  <button type="button" class="rec-x" aria-label={t('recents.removeNamed', { name: f.name })} title={t('recents.forget')} onClick={() => void remove('rec', i(), () => api.forgetRecent(f.path))}>
                    <Icon name="close" small />
                  </button>
                </li>
              )}
            </For>
          </ul>
        </section>
      </Show>
      <Show when={props.mode === 'project' && selected().length > 0}>
        <form
          class="env-tray"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <p class="hint">{t('env.selected', { names: selected().map((f) => f.name).join(', ') })}</p>
          <input class="field" type="text" value={name()} maxLength={60} placeholder={t('env.name')} aria-label={t('env.name')} onInput={(e) => setName(e.currentTarget.value)} />
          <button type="submit" class="btn" disabled={!name().trim()}>
            {t('env.save')}
          </button>
          <button type="button" class="btn primary" onClick={() => void guard(async () => props.onOpened?.(await api.openFolders(paths())))}>
            {t('env.start')}
          </button>
        </form>
      </Show>
      <Show when={props.mode === 'project' && (data()?.environments.length ?? 0) > 0}>
        <section aria-labelledby="env-title" data-section="env">
          <h2 id="env-title">{t('env.title')}</h2>
          <ul ref={animateList}>
            <For each={environments()}>
              {(env, index) => {
                const none = () => env.folders.every((f) => f.reason);
                return (
                  <li class="rec" classList={{ gone: none() }} onContextMenu={(e) => menuAt(e, 'env.forget', () => api.forgetEnvironment(env.id), 'env', index())}>
                    <button type="button" class="rec-main" disabled={none() || props.disabled} onClick={() => void guard(async () => props.onOpened?.(await api.startEnvironment(env.id)))}>
                      <span class="n">{env.name}</span>
                      <span class="d">
                        <For each={env.folders}>
                          {(f, i) => (
                            <>
                              {i() > 0 ? ' · ' : ''}
                              <span classList={{ off: !!f.reason }} title={f.reason ? t(f.reason as Key) : f.path}>
                                {f.name}
                              </span>
                            </>
                          )}
                        </For>
                      </span>
                    </button>
                    <button type="button" class="rec-x" aria-label={t('env.removeNamed', { name: env.name })} title={t('env.forget')} onClick={() => void remove('env', index(), () => api.forgetEnvironment(env.id))}>
                      <Icon name="close" small />
                    </button>
                  </li>
                );
              }}
            </For>
          </ul>
        </section>
      </Show>
    </div>
  );
}
