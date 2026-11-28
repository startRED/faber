import { createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type ContextFill } from './api.ts';
import { near, partsOf, percentOf } from './context.ts';
import { amount, catalogName, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { keepForExit } from './motion.ts';

/** A thin ring and a percent: how full the open conversation's own agent's context is (the caller shows it once the CLI has
 * said both numbers). A click opens what fills it, as far as the CLI counts it, and "Compact now", which hands the
 * conversation to the CLI's own compaction (task 190). Under them, the catalog tools the agent has now, each opening its Catalog
 * detail. The panel is in the page only while open, and reads the tools once each time it opens: no timer, no new process. */
export function ContextMeter(props: { id: string; fill: ContextFill; busy: boolean; onFail: (err: unknown) => void; onCatalog?: (id: string) => void }) {
  const [open, setOpen] = createSignal(false);
  // Unread until the engine answers, so the empty line never flashes before a list.
  const [tools, setTools] = createSignal<[string, string][]>();
  const shown = keepForExit(open);
  let root: HTMLElement | undefined;
  let button: HTMLButtonElement | undefined;
  const pct = () => percentOf(props.fill);
  const tip = () => t('ctx.tip', { used: amount(props.fill.used, 0), window: amount(props.fill.window, 0) });
  const close = (back: boolean) => {
    setOpen(false);
    if (back) button?.focus({ preventScroll: true });
  };
  const toggle = () => {
    if (open()) return close(false);
    setOpen(true);
    // Only Claude Code counts the parts; they arrive as an event and fill the list in place.
    void api.contextParts(props.id).catch(() => {});
    const id = props.id;
    setTools();
    void api.toolsInUse(id).then((list) => props.id === id && setTools(list), () => setTools([]));
  };
  // The job group the item was picked for, in the Catalog's own words; a part the catalog does not group says nothing.
  const why = (job: string) => {
    const key = `catalog.job.${job}`;
    return key in en ? t('ctx.toolsWhy', { job: t(key as Key) }) : '';
  };
  const show = (id: string) => {
    close(false);
    props.onCatalog?.(id);
  };
  const compact = () => {
    close(true);
    void api.compactNow(props.id).catch(props.onFail);
  };
  const outside = (e: PointerEvent) => open() && e.target instanceof Node && !root?.contains(e.target) && close(false);
  // The top layer answers Esc wherever focus is, ahead of the bars and columns under it (Find yields to it).
  const key = (e: KeyboardEvent) => {
    if (open() && e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault();
      e.stopImmediatePropagation();
      close(root?.contains(document.activeElement) ?? false);
    }
  };
  window.addEventListener('pointerdown', outside, true);
  window.addEventListener('keydown', key, true);
  onCleanup(() => {
    window.removeEventListener('pointerdown', outside, true);
    window.removeEventListener('keydown', key, true);
  });
  return (
    <span class="ctx-wrap" ref={(el) => (root = el)}>
      <button
        type="button"
        class="ctx"
        classList={{ near: near(props.fill) }}
        aria-label={tip()}
        title={tip()}
        aria-expanded={open()}
        aria-controls="ctx-panel"
        data-testid="context"
        ref={(el) => (button = el)}
        onClick={toggle}
      >
        <svg class="ctx-ring" viewBox="0 0 16 16" aria-hidden="true">
          <circle class="trk" cx="8" cy="8" r="6" />
          <circle class="arc" cx="8" cy="8" r="6" pathLength="100" stroke-dasharray={`${pct()} 100`} transform="rotate(-90 8 8)" />
        </svg>
        <span class="ctx-pct">{t('ctx.share', { pct: pct() })}</span>
      </button>
      <Show when={shown()}>
        <div class="ctx-panel" id="ctx-panel" classList={{ leaving: !open() }} inert={!open()} role="dialog" aria-label={t('ctx.title')}>
          <p class="ctx-sum">{tip()}</p>
          <Show when={partsOf(props.id)?.length}>
            <dl class="ctx-parts">
              <For each={partsOf(props.id)}>
                {(part) => (
                  <div>
                    <dt>{t(part.key as Key)}</dt>
                    <dd>{amount(part.tokens, 0)}</dd>
                  </div>
                )}
              </For>
            </dl>
          </Show>
          <Show when={tools()}>
            {(list) => (
              <section class="ctx-tools" aria-labelledby="ctx-tools-head">
                <h3 class="ctx-head" id="ctx-tools-head">{t('ctx.tools')}</h3>
                <Show when={list().length} fallback={<p class="ctx-none">{t('ctx.toolsNone')}</p>}>
                  <ul class="ctx-kit">
                    <For each={list()}>
                      {([id, job]) => (
                        <li>
                          <button type="button" class="link ctx-tool" title={t('ctx.toolsOpen', { name: catalogName(id) })} onClick={() => show(id)}>
                            {catalogName(id)}
                          </button>
                          <Show when={why(job)}>{(text) => <span class="ctx-why">{text()}</span>}</Show>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </section>
            )}
          </Show>
          <div class="ctx-act">
            <button type="button" class="btn" disabled={props.busy} onClick={compact}>
              {t('ctx.compact')}
            </button>
            <span class="ctx-hint">{t(props.busy ? 'ctx.compactBusy' : 'ctx.compactHint')}</span>
          </div>
        </div>
      </Show>
    </span>
  );
}
