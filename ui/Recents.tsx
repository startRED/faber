import { createResource, createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type FolderView, type Project } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';

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
  const [menu, setMenu] = createSignal<{ x: number; y: number; label: Key; run: () => Promise<unknown> }>();
  const closeMenu = () => setMenu(undefined);
  const key = (e: KeyboardEvent) => e.key === 'Escape' && closeMenu();
  addEventListener('pointerdown', closeMenu);
  addEventListener('keydown', key);
  onCleanup(() => {
    removeEventListener('pointerdown', closeMenu);
    removeEventListener('keydown', key);
  });
  const guard = async (run: () => Promise<unknown>) => {
    try {
      await run();
    } catch (error) {
      props.onError(error);
    }
    void refetch();
  };
  const menuAt = (e: MouseEvent, label: Key, run: () => Promise<unknown>) => {
    e.preventDefault();
    const at = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: e.clientX || at.left + 24, y: e.clientY || at.bottom, label, run });
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
    <div class="recents">
      <Show when={(data()?.folders.length ?? 0) > 0}>
        <section aria-labelledby="rec-title">
          <h2 id="rec-title">{t('recents.title')}</h2>
          <ul>
            <For each={data()?.folders}>
              {(f) => (
                <li class="rec" classList={{ gone: !!f.reason, on: chosen(f) }} onContextMenu={(e) => menuAt(e, 'recents.forget', () => api.forgetRecent(f.path))}>
                  <button type="button" class="rec-main" disabled={!!f.reason || props.disabled} title={f.path} onClick={() => props.onPick(f.path)}>
                    <span class="n">{f.name}</span>
                    <span class="d">{f.reason ? t(f.reason as Key) : f.parent}</span>
                  </button>
                  <Show when={props.mode === 'project' && !f.reason}>
                    <button type="button" class="rec-sel" aria-pressed={chosen(f)} aria-label={t('recents.select', { name: f.name })} title={t('recents.select', { name: f.name })} onClick={() => toggle(f)}>
                      <Icon name={chosen(f) ? 'check' : 'plus'} small />
                    </button>
                  </Show>
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
        <section aria-labelledby="env-title">
          <h2 id="env-title">{t('env.title')}</h2>
          <ul>
            <For each={data()?.environments}>
              {(env) => {
                const none = () => env.folders.every((f) => f.reason);
                return (
                  <li class="rec" classList={{ gone: none() }} onContextMenu={(e) => menuAt(e, 'env.forget', () => api.forgetEnvironment(env.id))}>
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
                const run = m().run;
                closeMenu();
                void guard(run);
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
