import { createSignal, createUniqueId, For, Show } from 'solid-js';
import type { Plan, PlanCount, WrapUp } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

/** "3 of 7": how far a plan has come, for the running column and the sidebar. */
export const planWords = (n: PlanCount) => t('plan.count', { done: n.done, total: n.total });

/** The agent's plan under the newest reply (task 017): done steps checked and dimmed, the current one in ink with what the agent
 * says it is doing now beside it, pending ones plain. It is a list of facts: nothing in it moves or takes input. */
export function PlanList(props: { plan: Plan; now?: string }) {
  const count = () => ({ done: props.plan.steps.filter((s) => s.state === 'done').length, total: props.plan.steps.length });
  return (
    <section class="plan" aria-label={t('plan.label')}>
      <p class="plan-head">
        <span>{t('plan.label')}</span>
        <span class="num">{planWords(count())}</span>
      </p>
      <ol>
        <For each={props.plan.steps}>
          {(step) => (
            <li class={step.state} aria-current={step.state === 'active' ? 'step' : undefined}>
              <span class="mark" aria-hidden="true">
                <Show when={step.state === 'done'}>
                  <Icon name="check" small />
                </Show>
              </span>
              <span class="text">{step.text}</span>
              <Show when={step.state === 'active' && props.now}>
                <span class="now"> · {props.now}</span>
              </Show>
              <span class="sr">{t(`plan.state.${step.state}`)}</span>
            </li>
          )}
        </For>
      </ol>
      <Show when={props.plan.estimate?.economy}>
        <p class="note">{t('plan.economy')}</p>
      </Show>
    </section>
  );
}

/** The closing box of a task: the recap, and one question whose buttons reply for the person, the recommendation first. A
 * question Faber answered for them while Keep going ran keeps no buttons: the reply's answers card says what it chose. */
export function WrapUpBox(props: { box: WrapUp; open: boolean; onReply?: (text: string) => void; onKeep?: (kind: 'taste' | 'lesson') => Promise<void> }) {
  const id = createUniqueId();
  const [busy, setBusy] = createSignal(false);
  const [kept, setKept] = createSignal<string[]>([]);
  const [error, setError] = createSignal(false);
  const keep = async (kind: 'taste' | 'lesson') => {
    if (busy() || !props.onKeep) return;
    const key = `${kind}:${props.box[kind]}`;
    setBusy(true); setError(false);
    try { await props.onKeep(kind); setKept([...kept(), key]); }
    catch { setError(true); }
    finally { setBusy(false); }
  };
  return (
    <section class="wrap-up" aria-label={t('wrap.label')}>
      <p class="recap">{props.box.recap}</p>
      <For each={['taste', 'lesson'] as const}>{(kind) => <Show when={props.box[kind]}>{(text) => (
        <div class="wrap-note">
          <p id={`${id}-${kind}`}>{t(kind === 'taste' ? 'wrap.taste' : 'wrap.lesson')} · {text()}</p>
          <Show when={props.box[kind === 'taste' ? 'tasteKept' : 'lessonKept'] || kept().includes(`${kind}:${text()}`)} fallback={
            <Show when={props.onKeep}><button type="button" class="btn ghost" aria-describedby={`${id}-${kind}`} disabled={busy()} onClick={() => void keep(kind)}>{t('memory.keep')}</button></Show>
          }><span role="status">{t('wrap.kept')}</span></Show>
        </div>
      )}</Show>}</For>
      <Show when={error()}><p class="note err" role="alert">{t('error.memory.write')}</p></Show>
      <Show when={props.box.ask}>
        {(ask) => (
          <div class="wrap-ask">
            <p class="q">{ask()}</p>
            <Show when={!props.box.answered}>
              <Show when={props.open && props.onReply}>
                <div class="wrap-actions">
                  <For each={props.box.actions ?? []}>
                    {(action, i) => (
                      <button type="button" class={i() === 0 ? 'btn primary' : 'btn secondary'} onClick={() => props.onReply?.(action)}>
                        {action}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </div>
        )}
      </Show>
    </section>
  );
}
