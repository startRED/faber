import { For, Show } from 'solid-js';
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
    </section>
  );
}

/** The closing box of a task: the recap, and one question whose buttons reply for the person, the recommendation first. A
 * question Faber answered for them while Keep going ran keeps no buttons: the reply's answers card says what it chose. */
export function WrapUpBox(props: { box: WrapUp; open: boolean; onReply?: (text: string) => void }) {
  return (
    <section class="wrap-up" aria-label={t('wrap.label')}>
      <p class="recap">{props.box.recap}</p>
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
