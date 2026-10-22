import { createMemo, createSignal, For, Show, type Accessor, type ComponentProps } from 'solid-js';
import type { BackgroundCommand, Conversation, FileChange, Project, RunningItem, Summary } from './api.ts';
import { AgentChat } from './AgentChat.tsx';
import { DetailsPane } from './Details.tsx';
import { Changes } from './Changes.tsx';
import { FilePane } from './FileView.tsx';
import { Commands } from './Commands.tsx';
import { Asks, Parkeds } from './Ask.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { Grip } from './Grip.tsx';
import type { Limits } from './layout.ts';
import { everyWhileShown } from './idle.ts';
import { t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { planWords } from './Plan.tsx';
import { QuickQuestion } from './QuickQuestion.tsx';
import type { Option } from './models.ts';
import { SitePane, type OpenSite } from './Site.tsx';
import { VisualPane, type OpenVisual } from './VisualAnswer.tsx';
import type { VisualAnswer } from './api.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { untilReset } from './Sidebar.tsx';
import { blocksOf, type Block, elapsed } from './running.ts';
import type { AgentStep } from './worklog.ts';

/** The running column: what runs now, one block per project and one for Chat, in the order the engine sent. A question
 * that waits is answered here, in place; a click on a row opens its conversation. */
export function Running(props: {
  items: RunningItem[];
  /** Runs that ended while nobody looked (`doneCards`): a calm card each, until opened or dismissed. */
  done?: Summary[];
  projects: Project[];
  onOpen: (item: RunningItem) => void;
  onDetails?: (item: RunningItem) => void;
  /** A done card's click opens its conversation, which clears it; its × only clears it. */
  onOpenDone?: (c: Summary) => void;
  onDismiss?: (c: Summary) => void;
  onClose: () => void;
  /** Opened by the user over the stage: it is a modal dialog. Docked, or opened by itself, it is not. */
  modal: boolean;
  /** A subagent the person opened from its reply: the column shows it instead of the list until "back". */
  agent?: { step: AgentStep; streaming: boolean; helpable: boolean };
  /** A page of Faber's browser the person opened from a reply, shown the same way. */
  site?: OpenSite;
  visual?: { answer: VisualAnswer; open: OpenVisual };
  /** The quick question on the side (R250), opened from the topbar, Ctrl+; or `/btw`, shown the same way. */
  aside?: { conversation?: Conversation; options: Option[] };
  /** A file a reply named (parity P2), shown the same way. */
  file?: { conversationId: string; at: string };
  /** The conversation's changes, opened from the topbar. */
  changes?: { id: string; files: FileChange[]; loading: boolean; incomplete?: boolean; loadingMore?: boolean; onMore?: () => void; onReview?: (security: boolean) => void };
  details?: Omit<ComponentProps<typeof DetailsPane>, 'onBack' | 'onClose'>;
  background?: BackgroundCommand[];
  conversations?: Summary[];
  commands?: boolean;
  onCommands?: () => void;
  /** Back from an opened subagent or the commands to the list. */
  onBack?: () => void;
  /** The open subagent or browser page fills the stage (the column sits over the work area), and the control in its head that swaps the two. */
  wide?: boolean;
  onWide?: () => void;
  /** "Tell it…" from the open subagent: the conversation's box, reached through the column. */
  onTell?: (text: string) => void;
  /** The column's width and the limits of its left edge, which the person drags: omitted, the column has no grip. */
  resize?: { value: number; limits: Limits; fallback: number; onChange: (px: number) => void };
  ref: (el: HTMLElement) => void;
}) {
  // The times move while the column is open and stop with it.
  const [now, setNow] = createSignal(Date.now());
  everyWhileShown(15_000, () => setNow(Date.now()));
  // The engine sends the whole list anew at every change, so its items are new objects each time. A row keyed by its object
  // would be built again at every change and lose what it holds (an answered question card folding away): rows are kept by
  // conversation and turn, and each reads its latest item.
  const rowSignals = new Map<string, [Accessor<RunningItem>, (item: RunningItem) => void]>();
  const rowKey = (item: RunningItem) => `${item.conversationId}:${item.turnId}`;
  const rowOf = (item: RunningItem) => rowSignals.get(rowKey(item))![0];
  const blocks = createMemo(() => {
    const kept = new Set<string>();
    for (const item of props.items) {
      const key = rowKey(item);
      kept.add(key);
      const row = rowSignals.get(key);
      if (row) row[1](item);
      else {
        const [get, set] = createSignal(item);
        rowSignals.set(key, [get, set]);
      }
    }
    for (const key of rowSignals.keys()) if (!kept.has(key)) rowSignals.delete(key);
    return blocksOf(props.items, props.done);
  });
  // The blocks are kept by their key for the same reason.
  const blockKeys = createMemo(() => blocks().map((b) => b.key), [], { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) });
  const noBlock: Block = { key: '', items: [], done: [], needs: 0 };
  const rows = () => props.items.length + (props.done?.length ?? 0);
  const commandCount = () => props.background?.filter((item) => item.state === 'running').length ?? 0;
  const ended = (c: Summary) => c.ended!;
  const endWords = (c: Summary) => (ended(c).kind === 'done' ? t('run.done') : ended(c).kind === 'failed' ? t('run.doneFailed') : t('run.needsYou'));
  const worked = (c: Summary) => {
    const e = elapsed(0, ended(c).workMs);
    return t('run.worked', { time: t(e.key, { n: e.n }) });
  };
  const nameOf = (key: string) => (key === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === key)?.name ?? t('area.projects')));
  const words = (item: RunningItem) =>
    item.waitUntil
      ? t('run.waiting', { when: untilReset(item.waitUntil) })
      : item.state === 'needsYou'
        ? t('run.needsYou')
        : item.keepWait
          ? t(`run.kg.wait.${item.keepWait.why}`)
          : item.keepGoing
            ? t('run.keepGoing')
            : item.vendor
              ? t('run.working')
              : t('run.routing');
  const since = (item: RunningItem) => {
    const e = elapsed(item.since, now());
    return t(e.key, { n: e.n });
  };
  // A conversation that goes on by itself: how far it has come, so a person who comes back sees it is still getting somewhere.
  const progress = (p: NonNullable<RunningItem['keepProgress']>) => {
    const turns = t('run.kg.turns', { n: p.turns });
    if (!p.since) return turns;
    const e = elapsed(p.since, now());
    return `${turns} · ${t('run.kg.since', { time: t(e.key, { n: e.n }) })}`;
  };
  return (
    <aside id="running" class="runcol" classList={{ wide: props.wide }} data-surface="running" aria-labelledby="running-title" role={props.modal ? 'dialog' : undefined} aria-modal={props.modal ? 'true' : undefined} ref={props.ref}>
      <Show when={props.details} fallback={<>
      <Show
        when={props.agent}
        fallback={
          <Show when={props.visual} fallback={<Show when={props.site} fallback={<Show when={props.aside} fallback={<Show when={props.file} fallback={<Show when={props.changes} fallback={<Show when={props.commands} fallback={<>
            <div class="rc-head">
              <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
                <Icon name="close" />
              </button>
              <h2 id="running-title" class="display">
                {t('run.title')}
              </h2>
              <span class="n num">{rows()}</span>
              <Show when={props.onCommands}>
                <button class="btn ghost rc-commands" onClick={props.onCommands}>
                  {t('commands.title')}<Show when={commandCount()}>{' '}<span class="num">{commandCount()}</span></Show>
                </button>
              </Show>
            </div>
            <div class="rc-body" ref={animateList}>
              <Show
                when={rows() > 0}
                fallback={
                  <div class="rc-empty">
                    <Peek />
                    <p>{t('run.none')}</p>
                  </div>
                }
              >
                <For each={blockKeys()}>
                  {(key) => { const block = () => blocks().find((b) => b.key === key) ?? noBlock; return (
                    <section class="proj-g">
                      <h3>
                        <span class="t">{nameOf(key)}</span>
                        <Show when={block().needs > 0}>
                          <span class="amber-tag">
                            <Icon name="ask" small />
                            {t('run.needs', { n: block().needs })}
                          </span>
                        </Show>
                      </h3>
                      <ul class="ritems" ref={animateList}>
                        <For each={block().items.map(rowOf)}>
                          {(item) => (
                            <li class="ri" classList={{ need: item().state === 'needsYou' }} ref={animateList}>
                              <div class="rhead">
                                <Show when={item().state === 'needsYou'}>
                                  <span class="ask-label">
                                    <Icon name="ask" small />
                                    {words(item())}
                                  </span>
                                </Show>
                                <span class="vm" classList={{ claude: item().vendor === 'claude', codex: item().vendor === 'codex' }}>
                                  <VendorMark vendor={item().vendor} />
                                  <Show when={item().vendor}>{(v) => <b>{vendorName(v())}</b>}</Show>
                                  <Show when={item().state !== 'needsYou'}>
                                    <span class="verb">{words(item())}</span>
                                  </Show>
                                </span>
                                <span class="time num">{since(item())}</span>
                              </div>
                              <button class="name" onClick={() => props.onOpen(item())}>
                                <Show when={item().plan}>{(n) => <span class="plan-n num">{planWords(n())}</span>}</Show>
                                {item().title || t('run.untitled')}
                              </button>
                              <Show when={item().area === 'project' && props.onDetails}>
                                <button class="btn ghost" onClick={() => props.onDetails?.(item())}>{t('details.title')}</button>
                              </Show>
                              <Show when={!item().keepProgress && item().now}>{(now) => <p class="run-kg"><span class="last" title={now()}>{now()}</span></p>}</Show>
                              <Show when={item().keepProgress}>
                                {(p) => (
                                  <p class="run-kg">
                                    <span class="num">{progress(p())}</span>
                                    <Show when={p().last}>{(last) => <span class="last" title={last()}>{last()}</span>}</Show>
                                  </p>
                                )}
                              </Show>
                              {/* Always mounted: an answered question card keeps itself for its fold-away (Asks), as in the thread. */}
                              <Asks conversationId={item().conversationId} turnId={item().turnId} permissions={item().permissions} />
                              <Show when={item().parked?.length || item().parkedMore}>
                                <Parkeds parked={item().parked ?? []} more={item().parkedMore} />
                              </Show>
                              <Show when={item().held}>{(held) => <p class="run-held">{t('run.held', { pr: `#${held()[0]}` })}</p>}</Show>
                            </li>
                          )}
                        </For>
                        <For each={block().done}>
                          {(c) => (
                            <li class="ri done" classList={{ need: ended(c).kind !== 'done' }} ref={animateList}>
                              <div class="rhead">
                                <span class="vm end">
                                  <Icon name={ended(c).kind === 'done' ? 'check' : 'ask'} small />
                                  <span class="verb">{endWords(c)}</span>
                                </span>
                                <span class="time num">{worked(c)}</span>
                                <button class="iconbtn dismiss" aria-label={t('run.dismiss', { title: c.title || t('run.untitled') })} title={t('run.dismiss', { title: c.title || t('run.untitled') })} onClick={() => props.onDismiss?.(c)}>
                                  <Icon name="close" small />
                                </button>
                              </div>
                              <button class="name" onClick={() => props.onOpenDone?.(c)}>
                                {c.title || t('run.untitled')}
                              </button>
                              <Show when={ended(c).line}>{(line) => <p class="run-kg"><span class="last" title={line()}>{line()}</span></p>}</Show>
                            </li>
                          )}
                        </For>
                      </ul>
                    </section>
                  ); }}
                </For>
              </Show>
            </div>
          </>}>
            <Commands items={props.background ?? []} conversations={props.conversations ?? []} onBack={() => props.onBack?.()} onClose={props.onClose} />
          </Show>}>
            {(changes) => <Changes id={changes().id} files={changes().files} loading={changes().loading} incomplete={changes().incomplete} loadingMore={changes().loadingMore} onMore={changes().onMore} onReview={changes().onReview} onClose={props.onClose} />}
          </Show>}>
            {(file) => <FilePane conversationId={file().conversationId} at={file().at} onBack={() => props.onBack?.()} onClose={props.onClose} />}
          </Show>}>
            {(aside) => <QuickQuestion conversation={aside().conversation} options={aside().options} onClose={props.onClose} onTell={(text) => props.onTell?.(text)} />}
          </Show>}>
            {(site) => <SitePane site={site()} onClose={props.onClose} wide={props.wide} onWide={props.onWide} />}
          </Show>
          }>{(visual) => <VisualPane answer={visual().answer} open={visual().open} onBack={() => props.onBack?.()} onClose={props.onClose} />}</Show>
        }
      >
        {(agent) => <AgentChat step={agent().step} streaming={agent().streaming} helpable={agent().helpable} onBack={() => props.onBack?.()} onTell={(text) => props.onTell?.(text)} onClose={props.onClose} wide={props.wide} onWide={props.onWide} />}
      </Show>
      </>}>{(details) => <DetailsPane {...details()} onBack={() => props.onBack?.()} onClose={props.onClose} />}</Show>
      <Show when={props.resize}>{(r) => <Grip edge="run" label={t('run.resize')} value={r().value} limits={r().limits} fallback={r().fallback} onChange={r().onChange} />}</Show>
    </aside>
  );
}
