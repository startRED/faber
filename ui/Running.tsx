import { createSignal, For, Show } from 'solid-js';
import type { Project, RunningItem } from './api.ts';
import { Asks, Parkeds } from './Ask.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { everyWhileShown } from './idle.ts';
import { t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { untilReset } from './Sidebar.tsx';
import { blocksOf, elapsed } from './running.ts';

/** The running column: what runs now, one block per project and one for Chat, in the order the engine sent. A question
 * that waits is answered here, in place; a click on a row opens its conversation. */
export function Running(props: {
  items: RunningItem[];
  projects: Project[];
  onOpen: (item: RunningItem) => void;
  onClose: () => void;
  /** Opened by the user over the stage: it is a modal dialog. Docked, or opened by itself, it is not. */
  modal: boolean;
  ref: (el: HTMLElement) => void;
}) {
  // The times move while the column is open and stop with it.
  const [now, setNow] = createSignal(Date.now());
  everyWhileShown(15_000, () => setNow(Date.now()));
  const blocks = () => blocksOf(props.items);
  const nameOf = (key: string) => (key === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === key)?.name ?? t('area.projects')));
  const words = (item: RunningItem) => (item.waitUntil ? t('run.waiting', { when: untilReset(item.waitUntil) }) : item.state === 'needsYou' ? t('run.needsYou') : item.vendor ? t('run.working') : t('run.routing'));
  const since = (item: RunningItem) => {
    const e = elapsed(item.since, now());
    return t(e.key, { n: e.n });
  };
  return (
    <aside id="running" class="runcol" data-surface="running" aria-labelledby="running-title" role={props.modal ? 'dialog' : undefined} aria-modal={props.modal ? 'true' : undefined} ref={props.ref}>
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
    </aside>
  );
}
