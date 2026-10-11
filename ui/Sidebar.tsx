import { createEffect, createSignal, For, on, Show } from 'solid-js';
import type { Area, Project, RunningItem, Summary, Vendor } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { Peek } from './Mark.tsx';
import { vendorName } from './models.ts';
import type { Place } from './running.ts';
import { groupChats, type Quotas, tailOf, tightest } from './sidebar.ts';

function untilReset(resetsAt: number): string {
  const hours = (resetsAt * 1000 - Date.now()) / 3_600_000;
  if (hours <= 0) return t('quota.now');
  return hours < 24 ? t('quota.hours', { n: Math.ceil(hours) }) : t('quota.days', { n: Math.ceil(hours / 24) });
}

function windowName(window: string): string {
  const key = `quota.${window}`;
  return key in en ? t(key as Key) : window.replace('_', ' ');
}

interface Props {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
  version: string;
  place: Place;
  running: RunningItem[];
  quotas: Quotas;
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
  onSettings: () => void;
}

/** The sidebar: new conversation, the two areas, the projects with their conversations, the chats by day, and at the
 * bottom Settings and the quota of each connected vendor. */
export function Sidebar(props: Props) {
  const [confirming, setConfirming] = createSignal<string>();
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
  const inProject = (p: Project) => props.conversations.filter((c) => c.area === 'project' && c.projectId === p.id);
  const chats = () => groupChats(props.conversations.filter((c) => c.area === 'chat'), props.now);
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
      <div class="side-scroll" onMouseLeave={() => setConfirming(undefined)}>
        <section class="projects" aria-labelledby="side-projects">
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
            <For each={props.projects}>
              {(p) => {
                const tail = () => tailOf(props.running, p.id);
                const here = () => props.place.area === 'project' && props.place.projectId === p.id;
                return (
                  <div class="proj">
                    <div class="prow">
                      <button
                        class="item"
                        aria-current={here() ? (props.place.id ? 'location' : 'true') : undefined}
                        aria-expanded={open().has(p.id)}
                        title={p.path}
                        onClick={() => {
                          // A second click on the project the user is in folds its conversations.
                          toggle(p.id, !(here() && open().has(p.id)));
                          props.onProject(p.id);
                        }}
                      >
                        <span class="t">{p.name}</span>
                        <Show when={tail().running && tail().needs === 0}>
                          <span class="run-dot" role="img" aria-label={t('side.running')} />
                        </Show>
                        <Show when={tail().needs > 0}>
                          <span class="amber-tag">
                            <Icon name="ask" small />
                            {t('run.needs', { n: tail().needs })}
                          </span>
                        </Show>
                      </button>
                      <button class="plus" aria-label={t('side.newIn', { name: p.name })} title={t('side.newIn', { name: p.name })} onClick={() => (toggle(p.id, true), props.onNewIn(p.id))}>
                        <Icon name="plus" small />
                      </button>
                    </div>
                    <Show when={open().has(p.id)}>
                      <div class="proj-chats">
                        <For each={inProject(p)}>{(c) => <Row c={c} />}</For>
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Show>
        </section>
        <div class="list">
          <For each={chats()}>
            {(g) => (
              <section>
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
        <div class="usage" role="group" aria-label={t('quota.label')}>
          <For each={props.vendors}>
            {(v) => {
              const q = () => tightest(props.quotas, v, props.now);
              const share = () => (q() ? `${Math.round(q()!.used * 100)}%` : t('quota.unknown'));
              return (
                <button
                  class="vendor"
                  onClick={() => props.onSettings()}
                  title={q() ? `${t('quota.window', { window: windowName(q()!.window) })} · ${untilReset(q()!.resetsAt)}` : t('side.usage')}
                >
                  <VendorMark vendor={v} />
                  <span>{vendorName(v)}</span>
                  <span class="share num">{share()}</span>
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
