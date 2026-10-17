import { createEffect, createMemo, createSignal, For, on, Show } from 'solid-js';
import { UpdateLine } from './UpdateStage.tsx';
import { api, type Area, type Project, type RunningItem, type Summary, type UpdateStatus, type Vendor, type VendorUsage } from './api.ts';
import { openMenu } from './contextmenu.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { percent, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { Peek } from './Mark.tsx';
import { animateList, keepForExit } from './motion.ts';
import { vendorName } from './models.ts';
import { forgetProject } from './projects.ts';
import type { Place } from './running.ts';
import { mac } from './WindowControls.tsx';
import { ageOf, freshness, groupChats, matches, type Quotas, showsChats, tailOf, tightest } from './sidebar.ts';

/** The name of a conversation while it is being renamed: Enter keeps it, Esc and leaving the box too when nothing changed. */
function RenameBox(props: { title: string; onDone: (title?: string) => void }) {
  let done = false;
  let box: HTMLInputElement | undefined;
  // Enter and Esc give focus back to the row; leaving the box by a click leaves focus where the click put it.
  const finish = (title?: string, refocus = false) => {
    if (done) return;
    done = true;
    const row = box?.closest('.crow');
    props.onDone(title);
    if (refocus) queueMicrotask(() => row?.querySelector<HTMLElement>('.item')?.focus());
  };
  return (
    <input
      class="rename"
      value={props.title}
      maxLength={120}
      aria-label={t('menu.renameLabel')}
      ref={(el) => ((box = el), queueMicrotask(() => (el.focus(), el.select())))}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && !e.isComposing) finish(e.currentTarget.value, true);
        else if (e.key === 'Escape') finish(undefined, true);
        else return;
        e.preventDefault();
        e.stopPropagation();
      }}
      onBlur={(e) => finish(e.currentTarget.value)}
    />
  );
}

export function untilReset(resetsAt: number): string {
  const hours = (resetsAt * 1000 - Date.now()) / 3_600_000;
  if (hours <= 0) return t('quota.now');
  return hours < 24 ? t('quota.hours', { n: Math.ceil(hours) }) : t('quota.days', { n: Math.ceil(hours / 24) });
}

/** "now", "4 min ago": how long ago a reading was taken, in plain words. */
export function ageWords(readAt: number, nowMs: number): string {
  const { key, n } = ageOf(readAt, nowMs);
  return t(key, { n });
}

export function windowName(window: string): string {
  const key = `quota.${window}`;
  return key in en ? t(key as Key) : window.replace('_', ' ');
}

interface Props {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
  version: string;
  /** The shell's view of the update (`update.rs`): a download or an install, or one kept for a restart. */
  update?: UpdateStatus;
  /** An answer is running, so the restart waits. */
  updateBlocked?: boolean;
  onRestart?: () => void;
  /** The name the person gave, shown above the connected accounts. */
  userName?: string;
  place: Place;
  running: RunningItem[];
  quotas: Quotas;
  /** What the engine knows about how current each vendor's number is. */
  usage?: VendorUsage[];
  /** The time now in milliseconds, which the screen moves on every minute: the groups of chats and the quota readings age. */
  now: number;
  /** A new conversation in the area and project the user is in. */
  onNew: () => void;
  onArea: (area: Area) => void;
  /** The user picked a project: its row is selected and no conversation is open. */
  onProject: (id: string) => void;
  /** A new conversation in this project. */
  onNewIn: (id: string) => void;
  onAddProject: () => void;
  onOpen: (conversation: Summary) => void;
  onRemove: (id: string) => void;
  /** Opens Settings; the quota of a vendor opens it at the usage limits. */
  onSettings: (section?: 'usage') => void;
  /** Something a menu asked for was refused (a folder that is gone, a project still working). */
  onError?: (err: unknown) => void;
}

/** The sidebar: new conversation, the two areas, the projects with their conversations, the chats by day, and at the
 * bottom Settings and the quota of each connected vendor. */
export function Sidebar(props: Props) {
  const [confirming, setConfirming] = createSignal<string>();
  const [renaming, setRenaming] = createSignal<string>();
  // The search box filters the titles on screen; it never leaves the window and nothing is asked of the engine.
  const [query, setQuery] = createSignal('');
  const searching = () => query().trim() !== '';
  // The projects whose conversations show. The project the user is in starts open.
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set(props.place.projectId ? [props.place.projectId] : []));
  const toggle = (id: string, on: boolean) =>
    setOpen((held) => {
      const next = new Set(held);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  // A conversation opened from elsewhere (a row of the running column) opens its project too, so the marked row is in view.
  createEffect(
    on(
      () => props.place.id,
      (id) => {
        if (id && props.place.area === 'project' && props.place.projectId) toggle(props.place.projectId, true);
      },
    ),
  );
  const inProject = (p: Project) => props.conversations.filter((c) => c.area === 'project' && c.projectId === p.id && matches(c.title, query()));
  // A project shows when its name or one of its conversations holds the search, and then opens on those conversations.
  const projects = () => props.projects.filter((p) => !searching() || matches(p.name, query()) || inProject(p).length > 0);
  const chats = createMemo(() => groupChats(props.conversations.filter((c) => c.area === 'chat' && matches(c.title, query())), props.now));
  // The one rule for a project showing its conversations: the user opened it, or the search found something in it. The
  // list and aria-expanded both read it, so they never disagree.
  const expanded = (p: Project) => showsChats(open().has(p.id), searching(), inProject(p).length);
  // Each area lists only its own: Chat its conversations, Projects its projects with theirs.
  const inChat = () => props.place.area === 'chat';
  const nothing = () => searching() && (inChat() ? chats().length === 0 : projects().length === 0);
  // The first click asks, the second deletes.
  const remove = (id: string) => {
    if (confirming() !== id) return setConfirming(id);
    setConfirming(undefined);
    props.onRemove(id);
  };

  const Row = (row: { c: Summary }) => (
    <div
      class="crow"
      onContextMenu={(e) =>
        openMenu(e, [
          { id: 'rename', label: t('menu.rename'), run: () => setRenaming(row.c.id) },
          { id: 'delete', label: t('menu.delete'), danger: true, confirm: t('side.confirmDelete'), run: () => props.onRemove(row.c.id) },
        ])
      }
    >
      <Show
        when={renaming() !== row.c.id}
        fallback={
          <RenameBox
            title={row.c.title}
            onDone={(title) => {
              setRenaming(undefined);
              if (title?.trim() && title.trim() !== row.c.title) void api.rename(row.c.id, title).catch((err) => props.onError?.(err));
            }}
          />
        }
      >
        <button class="item" aria-current={row.c.id === props.place.id} onClick={() => props.onOpen(row.c)}>
          <span>{row.c.title}</span>
          <Show when={props.running.find((r) => r.conversationId === row.c.id)?.waitUntil}>{(at) => <span class="waiting-label" title={t('run.waiting', { when: untilReset(at()) })}>{t('run.waiting', { when: untilReset(at()) })}</span>}</Show>
        </button>
      </Show>
      <button class="x" classList={{ confirm: confirming() === row.c.id }} aria-label={t('side.delete', { title: row.c.title })} onClick={() => remove(row.c.id)}>
        {confirming() === row.c.id ? t('side.confirmDelete') : <Icon name="close" small />}
      </button>
    </div>
  );

  return (
    <>
      <button class="newchat" onClick={() => props.onNew()}>
        <Icon name="plus" />
        <span>{t('side.new')}</span>
        <kbd>{t(mac ? 'key.newMac' : 'key.new')}</kbd>
      </button>
      <div class="areas" role="group" aria-label={t('area.label')}>
        <button class="area" aria-pressed={props.place.area === 'chat'} onClick={() => props.onArea('chat')}>
          <Icon name="chat" />
          <span>{t('area.chat')}</span>
        </button>
        <button class="area" data-tour="projects" aria-pressed={props.place.area === 'project'} onClick={() => props.onArea('project')}>
          <Icon name="folder" />
          <span>{t('area.projects')}</span>
          <span class="count num">{props.projects.length}</span>
        </button>
      </div>
      <label class="search">
        <Icon name="search" small />
        <input type="search" placeholder={t('side.search')} aria-label={t('side.search')} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      </label>
      <div class="side-scroll" onMouseLeave={() => setConfirming(undefined)} ref={(el) => animateList(el, true)}>
        <Show when={!inChat()}>
        <section class="projects" aria-labelledby="side-projects" ref={(el) => animateList(el, true)}>
          <div class="group-h">
            <span id="side-projects">{t('side.projects')}</span>
            <button class="plus" aria-label={t('side.addProject')} title={t('side.addProject')} onClick={() => props.onAddProject()}>
              <Icon name="plus" small />
            </button>
          </div>
          <Show
            when={props.projects.length > 0}
            fallback={
              <div class="side-empty">
                <Peek />
                <span>{t('side.noProjects')}</span>
              </div>
            }
          >
            <For each={projects()}>
              {(p) => {
                const tail = () => tailOf(props.running, p.id);
                const here = () => props.place.area === 'project' && props.place.projectId === p.id;
                const shown = keepForExit(() => expanded(p));
                return (
                  <div class="proj">
                    <div
                      class="prow"
                      onContextMenu={(e) =>
                        openMenu(e, [
                          { id: 'new', label: t('menu.newHere'), run: () => (toggle(p.id, true), props.onNewIn(p.id)) },
                          { id: 'open', label: t('menu.openFolder'), run: () => void api.showProject(p.id).catch((err) => props.onError?.(err)) },
                          { id: 'remove', label: t('menu.removeProject'), danger: true, confirm: t('menu.removeConfirm'), disabled: () => tail().running, run: () => void api.removeProject(p.id).then(() => forgetProject(p.id), (err) => props.onError?.(err)) },
                        ])
                      }
                    >
                      <button
                        class="item"
                        aria-current={here() ? (props.place.id ? 'location' : 'true') : undefined}
                        aria-expanded={expanded(p)}
                        title={p.path}
                        onClick={() => {
                          // A second click on the project the user is in folds its conversations.
                          toggle(p.id, !(here() && open().has(p.id)));
                          props.onProject(p.id);
                        }}
                      >
                        <span class="t">{p.name}</span>
                        <Show when={tail().running && tail().needs === 0}>
                          <span class="tail run-dot" role="img" aria-label={t('side.running')} />
                        </Show>
                        {/* Amber always comes with its words, as on the run button and in the running column. */}
                        <Show when={tail().needs > 0}>
                          <span class="tail amber-tag">
                            <Icon name="ask" small />
                            {t('run.needs', { n: tail().needs })}
                          </span>
                        </Show>
                      </button>
                      <button class="plus" aria-label={t('side.newIn', { name: p.name })} title={t('side.newIn', { name: p.name })} onClick={() => (toggle(p.id, true), props.onNewIn(p.id))}>
                        <Icon name="plus" small />
                      </button>
                    </div>
                    <div class="fold" classList={{ open: expanded(p) }} inert={!expanded(p)}>
                      <div class="fold-inner">
                        <Show when={shown()}>
                          <div class="proj-chats" ref={animateList}>
                            <For each={inProject(p)}>{(c) => <Row c={c} />}</For>
                          </div>
                        </Show>
                      </div>
                    </div>
                  </div>
                );
              }}
            </For>
          </Show>
        </section>
        </Show>
        <Show when={nothing()}>
          <div class="side-empty">{t('side.noMatch')}</div>
        </Show>
        <Show when={inChat()}>
          <div class="list" ref={(el) => animateList(el, true)}>
            {/* The groups are keyed by their name and the rows by their conversation, so a refresh builds again only what changed. */}
            <For each={chats().map((g) => g.group)}>
              {(group) => (
                <section ref={animateList}>
                  <div class="group-h">{t(`side.${group}` as Key)}</div>
                  <For each={chats().find((g) => g.group === group)?.chats ?? []}>{(c) => <Row c={c} />}</For>
                </section>
              )}
            </For>
          </div>
        </Show>
      </div>
      <div class="side-foot">
        {/* One row: the name, when the person gave one, and Settings. The version waits in Settings' tooltip until Settings shows it. */}
        <div class="foot-row">
          <Show when={props.userName}>
            <div class="who">{props.userName}</div>
          </Show>
          <button class="area" onClick={() => props.onSettings()} title={`${t('settings.shortcut')} · ${t('side.version', { version: props.version })}`}>
            <Icon name="sliders" />
            <span>{t('settings.open')}</span>
          </button>
        </div>
        {/* One thin line per connected AI: its mark, how much of its tightest window is used, the share. The window, the reset,
            how old the reading is and "still counting" (C010) are in the tooltip, so the line never wraps. */}
        <div class="usage" role="group" aria-label={t('quota.label')}>
          <For each={props.vendors}>
            {(v) => {
              const q = () => tightest(props.quotas, v, props.now);
              const share = () => (q() ? percent(q()!.used) : t('quota.unknown'));
              const fresh = () => freshness(props.usage, v, q()?.window);
              const low = () => props.usage?.find((u) => u.vendor === v)?.state === 'low';
              const details = () =>
                q()
                  ? [
                      `${t('quota.window', { window: windowName(q()!.window) })} · ${untilReset(q()!.resetsAt)}`,
                      fresh().uncountedPoints ? t('quota.uncounted', { share: `${fresh().uncountedPoints}%` }) : '',
                      fresh().readAt !== undefined ? t('quota.read', { when: ageWords(fresh().readAt!, props.now) }) : '',
                      fresh().counting ? t('quota.counting') : '',
                    ]
                  : [t('side.usage')];
              return (
                <button class="vendor" classList={{ low: low() }} onClick={() => props.onSettings('usage')} title={[vendorName(v), ...details()].filter(Boolean).join(' · ')}>
                  <VendorMark vendor={v} />
                  <span class="sr">{vendorName(v)}</span>
                  <span class="meter" aria-hidden="true">
                    <i style={{ width: `${Math.round(Math.min(1, Math.max(0, q()?.used ?? 0)) * 100)}%` }} />
                  </span>
                  <span class="share num">{share()}</span>
                  <Show when={q() && fresh().counting}>
                    <span class="counting" aria-hidden="true">…</span>
                  </Show>
                  <Show when={q() && fresh().counting}>
                    <span class="sr">{t('quota.counting')}</span>
                  </Show>
                </button>
              );
            }}
          </For>
          <Show when={props.vendors.length === 0}>
            <span class="note">{t('side.noAi')}</span>
          </Show>
        </div>
        {/* The update: a calm line while one downloads at open, a quiet control once one waits for a restart. */}
        <UpdateLine status={props.update ?? { phase: 'none' }} />
        <Show when={props.update?.phase === 'ready'}>
          <button
            type="button"
            class="upd act"
            disabled={props.updateBlocked}
            title={props.updateBlocked ? t('update.busyHint') : t('update.readyHint')}
            onClick={() => props.onRestart?.()}
          >
            {t('update.ready')}
          </button>
        </Show>
      </div>
    </>
  );
}
