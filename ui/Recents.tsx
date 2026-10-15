import { createResource, createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type FolderView, type Project } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';

/** A long folder path with its middle cut out: the drive (or root) and the last two folders stay, the full path goes in the tooltip. */
export function shortPath(path: string): string {
  const parts = path.split(/[\\/]/);
  if (parts.length <= 3) return path;
  const sep = path.includes('\\') ? '\\' : '/';
  return [parts[0], '…', ...parts.slice(-2)].join(sep);
}

type MenuState = { x: number; y: number; label: Key; run: () => Promise<unknown>; opener?: HTMLElement; section: Section; index: number };
type Section = 'rec' | 'env';

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
  const [selected, setSelected] = createSignal<FolderView[]>([]);
  const [name, setName] = createSignal('');
  const [menu, setMenu] = createSignal<MenuState>();
  let box: HTMLDivElement | undefined;
  /** Closing with the keyboard puts focus back where the menu came from; a click elsewhere leaves it where the person clicked. */
  const closeMenu = (restore = false) => {
    const open = menu();
    setMenu(undefined);
    if (restore) open?.opener?.focus();
  };
  const key = (e: KeyboardEvent) => e.key === 'Escape' && menu() && closeMenu(true);
  const away = () => closeMenu();
  addEventListener('pointerdown', away);
  addEventListener('keydown', key);
  onCleanup(() => {
    removeEventListener('pointerdown', away);
    removeEventListener('keydown', key);
  });
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
    queueMicrotask(() => {
      const buttons = box?.querySelectorAll<HTMLElement>(`[data-section="${section}"] .rec-x`);
      (buttons?.[Math.min(index, buttons.length - 1)] ?? box)?.focus();
    });
  };
  const menuAt = (e: MouseEvent, label: Key, run: () => Promise<unknown>, section: Section, index: number) => {
    e.preventDefault();
    const row = e.currentTarget as HTMLElement;
    const at = row.getBoundingClientRect();
    const active = document.activeElement;
    const opener = active instanceof HTMLElement && row.contains(active) ? active : (row.querySelector<HTMLElement>('.rec-x') ?? undefined);
    setMenu({ x: e.clientX || at.left + 24, y: e.clientY || at.bottom, label, run, opener, section, index });
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
    <div class="recents" ref={(el) => (box = el)} tabindex="-1">
      <Show when={(data()?.folders.length ?? 0) > 0}>
        <section aria-labelledby="rec-title" data-section="rec">
          <h2 id="rec-title">{t('recents.title')}</h2>
          <ul>
            <For each={data()?.folders}>
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
          <input type="text" value={name()} maxLength={60} placeholder={t('env.name')} aria-label={t('env.name')} onInput={(e) => setName(e.currentTarget.value)} />
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
          <ul>
            <For each={data()?.environments}>
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
      <Show when={menu()}>
        {(m) => (
          <div class="rec-menu" role="menu" style={{ left: `${m().x}px`, top: `${m().y}px` }} onPointerDown={(e) => e.stopPropagation()}>
            <button
              type="button"
              role="menuitem"
              ref={(el) => queueMicrotask(() => el.focus())}
              onClick={() => {
                const { run, section, index } = m();
                closeMenu();
                void remove(section, index, run);
              }}
            >
              {t(m().label)}
            </button>
          </div>
        )}
      </Show>
    </div>
  );
}
