import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import type { Area, Project, Summary } from './api.ts';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { keepForExit } from './motion.ts';
import { type Entry, rank } from './palette.ts';
import type { Section } from './Settings.tsx';

type Item = Entry<Parameters<typeof Icon>[0]['name']>;

// The sections of Settings, in its own order (ui/Settings.tsx).
const SECTIONS: ReadonlyArray<readonly [Section, Key, Item['icon']]> = [
  ['general', 'set.general', 'user'],
  ['projects', 'set.projects', 'folder'],
  ['appearance', 'set.appearance', 'palette'],
  ['accounts', 'set.subscriptions', 'plug'],
  ['privacy', 'set.privacy', 'shield'],
];

/** How many conversations the empty box lists under "Recent". */
const RECENT = 5;

interface Props {
  /** False while something covers the shell and must be answered first (the first run, the tutorial, an update). */
  enabled: boolean;
  conversations: Summary[];
  projects: Project[];
  onOpen: (conversation: Summary) => void;
  onProject: (id: string) => void;
  onNew: () => void;
  onArea: (area: Area) => void;
  onSettings: (section?: Section) => void;
  onRunning: () => void;
  onAddProject: () => void;
}

/** Ctrl+K: one box that opens a conversation or a project and runs the actions the sidebar and the topbar already have. It is
 * built from the lists on screen, asks the engine for nothing, and its elements exist only while it is open. */
export function Palette(props: Props) {
  const [open, setOpen] = createSignal(false);
  const shown = keepForExit(open);
  const [query, setQuery] = createSignal('');
  const [active, setActive] = createSignal(0);
  let opener: HTMLElement | undefined;
  let dialog: HTMLElement | undefined;
  let list: HTMLElement | undefined;

  const actions = (): Item[] => [
    { id: 'new', kind: 'action', label: t('side.new'), icon: 'plus', run: props.onNew },
    { id: 'chat', kind: 'action', label: t('palette.goArea', { area: t('area.chat') }), icon: 'chat', run: () => props.onArea('chat') },
    { id: 'projects', kind: 'action', label: t('palette.goArea', { area: t('area.projects') }), icon: 'folder', run: () => props.onArea('project') },
    { id: 'settings', kind: 'action', label: t('settings.open'), icon: 'sliders', run: () => props.onSettings() },
    ...SECTIONS.map(([id, key, icon]): Item => ({ id: `settings-${id}`, kind: 'action', label: t('palette.settings', { section: t(key) }), icon, run: () => props.onSettings(id) })),
    { id: 'running', kind: 'action', label: t('run.toggle'), icon: 'panel', run: props.onRunning },
    { id: 'add', kind: 'action', label: t('side.addProject'), icon: 'plus', run: props.onAddProject },
  ];
  // The newest conversation first, whatever order the list came in.
  const conversations = (): Item[] => {
    const names = new Map(props.projects.map((p) => [p.id, p.name]));
    return [...props.conversations]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((c): Item => ({
        id: c.id,
        kind: 'conversation',
        label: c.title,
        hint: c.area === 'chat' ? t('area.chat') : (names.get(c.projectId ?? '') ?? t('area.projects')),
        icon: 'chat',
        run: () => props.onOpen(c),
      }));
  };
  // Built only while the box is on screen. An empty box lists the latest conversations, then the actions; typing ranks
  // the actions, the projects and every conversation together.
  const rows = createMemo<Array<{ entry: Item; heading?: Key }>>(() => {
    if (!shown()) return [];
    const all = conversations();
    if (!query().trim()) {
      return [
        ...all.slice(0, RECENT).map((entry, i) => ({ entry, heading: i === 0 ? ('palette.recent' as Key) : undefined })),
        ...actions().map((entry, i) => ({ entry, heading: i === 0 ? ('palette.actions' as Key) : undefined })),
      ];
    }
    const projects = props.projects.map((p): Item => ({ id: p.id, kind: 'project', label: p.name, hint: t('area.projects'), icon: 'folder', run: () => props.onProject(p.id) }));
    return rank([...actions(), ...projects, ...all], query()).map((entry) => ({ entry }));
  });

  const hide = (back: boolean) => {
    setOpen(false);
    if (back && opener?.isConnected) opener.focus({ preventScroll: true });
  };
  const pick = (entry: Item) => {
    // Focus goes back first, so an action that moves it (a new conversation takes the composer) has the last word.
    hide(true);
    entry.run();
  };
  const show = () => {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    setQuery('');
    setActive(0);
    setOpen(true);
  };
  createEffect(() => {
    if (!props.enabled) hide(false);
  });
  createEffect(
    on(active, () => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }), { defer: true }),
  );

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      const swallow = () => (e.preventDefault(), e.stopImmediatePropagation());
      if (ctrl && !e.altKey && !e.shiftKey && key === 'k') {
        e.preventDefault();
        if (!props.enabled) return;
        e.stopImmediatePropagation();
        return open() ? hide(true) : show();
      }
      // A right-click menu over the box (Cut, Copy, Paste) owns the keys until it closes.
      if (!open() || document.querySelector('.cmenu:not(.leaving)')) return;
      // The shortcuts of the screen underneath wait until the box is closed.
      if (ctrl && ['n', ',', '.', 'f'].includes(key)) return swallow();
      const count = rows().length;
      if (e.key === 'Escape') {
        swallow();
        hide(true);
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        swallow();
        if (count) setActive((at) => (at + (e.key === 'ArrowDown' ? 1 : count - 1)) % count);
      } else if (e.key === 'Enter' && !e.isComposing) {
        swallow();
        const row = rows()[active()];
        if (row) pick(row.entry);
      } else if (e.key === 'Tab' && !ctrl && !e.altKey) {
        // The shell's own trap (the sidebar overlay, the running column) must not move focus behind the box.
        keepFocusIn(e, dialog);
        e.stopImmediatePropagation();
      }
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => window.removeEventListener('keydown', onKey, true));
  });

  return (
    <Show when={shown()}>
      <div
        class="pal-scrim"
        classList={{ leaving: !open() }}
        inert={!open()}
        onPointerDown={(e) => e.target === e.currentTarget && hide(true)}
      >
        <div class="pal" role="dialog" aria-modal="true" aria-label={t('palette.label')} ref={(el) => (dialog = el)}>
          <label class="field search">
            <Icon name="search" />
            <input
              type="text"
              role="combobox"
              aria-expanded="true"
              aria-controls="pal-list"
              aria-activedescendant={rows().length ? `pal-${active()}` : undefined}
              autocomplete="off"
              spellcheck={false}
              placeholder={t('palette.placeholder')}
              aria-label={t('palette.label')}
              value={query()}
              ref={(el) => queueMicrotask(() => el.focus())}
              onInput={(e) => {
                setQuery(e.currentTarget.value);
                setActive(0);
              }}
            />
          </label>
          <div class="pal-list" id="pal-list" role="listbox" ref={(el) => (list = el)} onMouseDown={(e) => e.preventDefault()}>
            <For each={rows()} fallback={<div class="pal-none">{t('palette.none')}</div>}>
              {(row, i) => (
                <>
                  <Show when={row.heading}>{(heading) => <div class="pal-head" aria-hidden="true">{t(heading())}</div>}</Show>
                  <div
                    class="pal-row"
                    id={`pal-${i()}`}
                    role="option"
                    aria-selected={i() === active()}
                    onMouseMove={() => active() !== i() && setActive(i())}
                    onClick={() => pick(row.entry)}
                  >
                    <Icon name={row.entry.icon} small />
                    <span class="pal-t">{row.entry.label}</span>
                    <Show when={row.entry.hint}>{(hint) => <span class="pal-hint">{hint()}</span>}</Show>
                  </div>
                </>
              )}
            </For>
          </div>
          <div class="sr" role="status">{query().trim() ? t('palette.count', { n: rows().length }) : ''}</div>
        </div>
      </div>
    </Show>
  );
}
