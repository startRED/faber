import { createComputed, createSignal, For, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import type { Project, RunningItem } from './api.ts';
import { AgentChat } from './AgentChat.tsx';
import { Asks, Parkeds } from './Ask.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { everyWhileShown } from './idle.ts';
import { t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { SitePane, type OpenSite } from './Site.tsx';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { untilReset } from './Sidebar.tsx';
import { type Block, blocksOf, elapsed } from './running.ts';
import type { AgentStep } from './worklog.ts';

/** The running column: what runs now, one block per project and one for Chat, in the order the engine sent. A question
 * that waits is answered here, in place; a click on a row opens its conversation. */
export function Running(props: {
  items: RunningItem[];
  projects: Project[];
  onOpen: (item: RunningItem) => void;
  onClose: () => void;
  /** Opened by the user over the stage: it is a modal dialog. Docked, or opened by itself, it is not. */
  modal: boolean;
  /** A subagent the person opened from its reply: the column shows it instead of the list until "back". */
  agent?: { step: AgentStep; streaming: boolean; helpable: boolean };
  /** A page of Faber's browser the person opened from a reply, shown the same way. */
  site?: OpenSite;
  onBack?: () => void;
  /** "Tell it…" from the open subagent: the conversation's box, reached through the column. */
  onTell?: (text: string) => void;
  ref: (el: HTMLElement) => void;
}) {
  // The times move while the column is open and stop with it.
  const [now, setNow] = createSignal(Date.now());
  everyWhileShown(15_000, () => setNow(Date.now()));
  // The engine sends the whole list again with every change of any conversation, as new objects. Drawn from those, each
  // row (and the card the person is writing in) would be built again; reconciled by key, a row that is still there stays
  // the same element and only what changed in it is updated.
  const [view, setView] = createStore<{ blocks: Block[] }>({ blocks: [] });
  createComputed(() => {
    const next = blocksOf(props.items).map((block) => ({ ...block, items: block.items.map((item) => ({ ...item, key: `${item.conversationId}:${item.turnId}` })) }));
    setView('blocks', reconcile(next, { key: 'key' }));
  });
  const blocks = () => view.blocks;
  const nameOf = (key: string) => (key === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === key)?.name ?? t('area.projects')));
  const words = (item: RunningItem) => (item.waitUntil ? t('run.waiting', { when: untilReset(item.waitUntil) }) : item.state === 'needsYou' ? t('run.needsYou') : item.vendor ? t('run.working') : t('run.routing'));
  const since = (item: RunningItem) => {
    const e = elapsed(item.since, now());
    return t(e.key, { n: e.n });
  };
  return (
    <aside id="running" class="runcol" data-surface="running" aria-labelledby="running-title" role={props.modal ? 'dialog' : undefined} aria-modal={props.modal ? 'true' : undefined} ref={props.ref}>
      <Show
        when={props.agent}
        fallback={
          <Show when={props.site} fallback={<>
            <div class="rc-head">
              <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
                <Icon name="close" />
              </button>
              <h2 id="running-title" class="display">
                {t('run.title')}
              </h2>
              <span class="n num">{props.items.length}</span>
            </div>
            <div class="rc-body" ref={animateList}>
              <Show
                when={props.items.length > 0}
                fallback={
                  <div class="rc-empty">
                    <Peek />
                    <p>{t('run.none')}</p>
                  </div>
                }
              >
                <For each={blocks()}>
                  {(block) => (
                    <section class="proj-g">
                      <h3>
                        <span class="t">{nameOf(block.key)}</span>
                        <Show when={block.needs > 0}>
                          <span class="amber-tag">
                            <Icon name="ask" small />
                            {t('run.needs', { n: block.needs })}
                          </span>
                        </Show>
                      </h3>
                      <ul class="ritems" ref={animateList}>
                        <For each={block.items}>
                          {(item) => (
                            <li class="ri" classList={{ need: item.state === 'needsYou' }} ref={animateList}>
                              <div class="rhead">
                                <Show when={item.state === 'needsYou'}>
                                  <span class="ask-label">
                                    <Icon name="ask" small />
                                    {words(item)}
                                  </span>
                                </Show>
                                <span class="vm" classList={{ claude: item.vendor === 'claude', codex: item.vendor === 'codex' }}>
                                  <VendorMark vendor={item.vendor} />
                                  <Show when={item.vendor}>{(v) => <b>{vendorName(v())}</b>}</Show>
                                  <Show when={item.state !== 'needsYou'}>
                                    <span class="verb">{words(item)}</span>
                                  </Show>
                                </span>
                                <span class="time num">{since(item)}</span>
                              </div>
                              <button class="name" onClick={() => props.onOpen(item)}>
                                {item.title || t('run.untitled')}
                              </button>
                              <Show when={item.permissions.length > 0}>
                                <Asks conversationId={item.conversationId} turnId={item.turnId} permissions={item.permissions} />
                              </Show>
                              <Show when={item.stuck?.length}>
                                <div class="ask stuck" role="status">
                                  <b>{t('run.stuck', { n: item.stuck?.length ?? 0 })}</b>
                                  <span class="note">{t('run.stuckHint')}</span>
                                  <div>
                                    <button class="btn primary" onClick={() => props.onOpen(item)}>
                                      {t('run.stuckOpen')}
                                    </button>
                                  </div>
                                </div>
                              </Show>
                              <Show when={item.parked?.length || item.parkedMore}>
                                <Parkeds parked={item.parked ?? []} more={item.parkedMore} />
                              </Show>
                            </li>
                          )}
                        </For>
                      </ul>
                    </section>
                  )}
                </For>
              </Show>
            </div>
          </>}>
            {(site) => <SitePane site={site()} onBack={() => props.onBack?.()} onClose={props.onClose} />}
          </Show>
        }
      >
        {(agent) => <AgentChat step={agent().step} streaming={agent().streaming} helpable={agent().helpable} onBack={() => props.onBack?.()} onTell={(text) => props.onTell?.(text)} onClose={props.onClose} />}
      </Show>
    </aside>
  );
}
