import { createEffect, createSignal, For, on, Show } from 'solid-js';
import type { Area, Project, RunningItem, Summary, Vendor, VendorUsage } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { Peek } from './Mark.tsx';
import { animateList, keepForExit } from './motion.ts';
import { vendorName } from './models.ts';
import type { Place } from './running.ts';
import { ageOf, freshness, groupChats, matches, type Quotas, showsChats, tailOf, tightest } from './sidebar.ts';

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
}

/** The sidebar: new conversation, the two areas, the projects with their conversations, the chats by day, and at the
 * bottom Settings and the quota of each connected vendor. */
export function Sidebar(props: Props) {
  const [confirming, setConfirming] = createSignal<string>();
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
  const chats = () => groupChats(props.conversations.filter((c) => c.area === 'chat' && matches(c.title, query())), props.now);
  // The one rule for a project showing its conversations: the user opened it, or the search found something in it. The
  // list and aria-expanded both read it, so they never disagree.
  const expanded = (p: Project) => showsChats(open().has(p.id), searching(), inProject(p).length);
  const nothing = () => searching() && projects().length === 0 && chats().length === 0;
  // The first click asks, the second deletes.
  const remove = (id: string) => {
    if (confirming() !== id) return setConfirming(id);
    setConfirming(undefined);
    props.onRemove(id);
  };

  const Row = (row: { c: Summary }) => (
    <div class="crow">
      <button class="item" aria-current={row.c.id === props.place.id} onClick={() => props.onOpen(row.c)}>
        <span>{row.c.title}</span>
        <Show when={props.running.find((r) => r.conversationId === row.c.id)?.waitUntil}>{(at) => <span class="waiting-label" title={t('run.waiting', { when: untilReset(at()) })}>{t('run.waiting', { when: untilReset(at()) })}</span>}</Show>
      </button>
      <button class="x" classList={{ confirm: confirming() === row.c.id }} aria-label={t('side.delete', { title: row.c.title })} onClick={() => remove(row.c.id)}>
        {confirming() === row.c.id ? t('side.confirmDelete') : '×'}
      </button>
    </div>
  );

  return (
    <>
      <button class="newchat" onClick={() => props.onNew()}>
        <Icon name="plus" />
        <span>{t('side.new')}</span>
        <kbd>Ctrl N</kbd>
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
                    <div class="prow">
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
                        {/* Amber is a bubble and the words; in a row this narrow the words are the label and the tooltip. */}
                        <Show when={tail().needs > 0}>
                          <span class="tail amber-tag" role="img" aria-label={t('run.needs', { n: tail().needs })} title={t('run.needs', { n: tail().needs })}>
                            <Icon name="ask" small />
                            <span class="num" aria-hidden="true">
                              {tail().needs}
                            </span>
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
        <Show when={nothing()}>
          <div class="side-empty">{t('side.noMatch')}</div>
        </Show>
        <div class="list" ref={(el) => animateList(el, true)}>
          <For each={chats()}>
            {(g) => (
              <section ref={animateList}>
                <div class="group-h">{t(`side.${g.group}` as Key)}</div>
                <For each={g.chats}>{(c) => <Row c={c} />}</For>
              </section>
            )}
          </For>
        </div>
      </div>
      <div class="side-foot">
        <button class="area" onClick={() => props.onSettings()} title={t('settings.shortcut')}>
          <Icon name="sliders" />
          <span>{t('settings.open')}</span>
        </button>
        <Show when={props.userName}>
          <div class="who">{props.userName}</div>
        </Show>
        <div class="usage" role="group" aria-label={t('quota.label')}>
          <For each={props.vendors}>
            {(v) => {
              const q = () => tightest(props.quotas, v, props.now);
              const share = () => (q() ? `${Math.round(q()!.used * 100)}%` : t('quota.unknown'));
              const fresh = () => freshness(props.usage, v, q()?.window);
              return (
                <button
                  class="vendor"
                  onClick={() => props.onSettings('usage')}
                  title={q() ? `${t('quota.window', { window: windowName(q()!.window) })} · ${untilReset(q()!.resetsAt)}` : t('side.usage')}
                >
                  <VendorMark vendor={v} />
                  <span>{vendorName(v)}</span>
                  <span class="share num">{share()}</span>
                  <Show when={q() && fresh().uncountedPoints}>{(n) => <span class="extra num">{t('quota.uncountedShort', { share: `${n()}%` })}</span>}</Show>
                  <Show when={q() && fresh().readAt !== undefined}><span class="age">{ageWords(fresh().readAt!, props.now)}</span></Show>
                  <Show when={q() && fresh().counting}><span class="counting">{t('quota.counting')}</span></Show>
                </button>
              );
            }}
          </For>
          <Show when={props.vendors.length === 0}>
            <span class="note">{t('side.noAi')}</span>
          </Show>
        </div>
        <div class="version num" lang={lang()}>
          v{props.version}
        </div>
      </div>
    </>
  );
}
