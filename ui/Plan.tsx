import { createSignal, For, Show } from 'solid-js';
import type { GuardFact, Plan, PlanCount, Proof, WrapUp } from './api.ts';
import { Collapse } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

/** "3 of 7": how far a plan has come, for the running column and the sidebar. */
export const planWords = (n: PlanCount) => t('plan.count', { done: n.done, total: n.total });

/** The agent's plan under the newest reply (task 017): done steps checked and dimmed, the current one in ink with what the agent
 * says it is doing now beside it, pending ones plain. A step another conversation does names it under the step's text, as a
 * quiet link that opens it, with where it stands in a subagent row's words (task 040). It starts closed as one line, the count
 * and the step under way, so a long plan never fills the thread; a click on that line shows the steps and what each left. */
export function PlanList(props: { plan: Plan; now?: string; names?: Record<string, string>; onOpen?: (id: string) => void }) {
  const count = () => ({ done: props.plan.steps.filter((s) => s.state === 'done').length, total: props.plan.steps.length });
  const [open, setOpen] = createSignal(false);
  const current = () => props.plan.steps.find((s) => s.state === 'active') ?? props.plan.steps.find((s) => s.state === 'pending');
  return (
    <section class="plan" aria-label={t('plan.label')}>
      <Show when={props.plan.steps.length}>
        <button type="button" class="plan-head" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <Icon name={open() ? 'down' : 'right'} small />
          <span>{t('plan.label')}</span>
          <span class="num">{planWords(count())}</span>
          <Show when={!open() && current()}>{(step) => <span class="plan-now">{step().text}</span>}</Show>
        </button>
      </Show>
      <Show when={props.plan.raisedBecause}>{(reason) => <p class="note" role="status">{t('plan.raised', { size: t(`plan.size.${props.plan.size ?? 'small'}`), reason: t(`plan.boundary.${reason()}`) })}</p>}</Show>
      <Collapse open={open() || !props.plan.steps.length}>
      <Show when={props.plan.contract}>
        {(contract) => (
          <div class="plan-contract">
            <Show when={contract().extends || contract().newBecause}>
              <p class="note">{contract().extends ? t('plan.extends') : t('plan.new')}: {contract().extends || contract().newBecause}</p>
            </Show>
            <Show when={contract().proof || contract().target}>
              <p class="note"><Show when={contract().proof}>{t('plan.proof')}: {contract().proof}</Show><Show when={contract().target}>{contract().proof && ' · '}{t('plan.target')}: {contract().target}</Show></p>
            </Show>
          </div>
        )}
      </Show>
      <ol>
        <For each={props.plan.steps}>
          {(step) => (
            <li
              class={step.state}
              aria-current={step.state === 'active' ? 'step' : undefined}
            >
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
              <Show when={step.state === 'done' && step.evidence}>{(evidence) => <span class="now"> · {evidence()}</span>}</Show>
              <Show when={step.conversation}>
                {(id) => (
                  <span class="member">
                    <button type="button" class="link" onClick={() => props.onOpen?.(id())}>
                      {props.names?.[id()] || t('agent.untitled')}
                    </button>
                    <Show when={step.member}>{(state) => <span class="member-state" data-state={state()}> · {t(`agent.${state()}`)}</span>}</Show>
                  </span>
                )}
              </Show>
            </li>
          )}
        </For>
      </ol>
      <Show when={props.plan.estimate?.economy}>
        <p class="note">{t('plan.economy')}</p>
      </Show>
      </Collapse>
    </section>
  );
}

/** What Faber saw behind a done conclusion: how many things asked a receipt confirms, what is not confirmed, and what it
 * noticed in the edits. Quiet lines; a conclusion let through without proof says so first. */
function ProofLines(props: { proof: Proof }) {
  const fact = (f: GuardFact) =>
    f.kind === 'outside' ? t('wrap.fact.outside', { n: f.count, files: f.files.join(', ') })
      : f.kind === 'weakened' ? t('wrap.fact.weakened', { file: f.file, n: f.removed })
      : t(`wrap.fact.${f.kind}`, { file: f.file });
  return (
    <div class="wrap-proof" data-labelled={props.proof.labelled ? '' : undefined}>
      <Show when={props.proof.labelled}><p>{t('wrap.proof.labelled')}</p></Show>
      <Show when={props.proof.total > 0}><p>{t('wrap.proof.count', { confirmed: props.proof.confirmed, total: props.proof.total })}</p></Show>
      <Show when={props.proof.missing?.length}><p>{t('wrap.proof.missing', { list: (props.proof.missing ?? []).join('; ') })}</p></Show>
      <Show when={props.proof.failed}><p>{t('wrap.proof.failed')}</p></Show>
      <Show when={props.proof.unchecked && props.proof.labelled && !props.proof.total}><p>{t('wrap.proof.unchecked')}</p></Show>
      <For each={props.proof.facts ?? []}>{(f) => <p>{fact(f)}</p>}</For>
    </div>
  );
}

/** The closing box of a task: the recap, and one question whose buttons reply for the person, the recommendation first. A
 * question Faber answered for them while Keep going ran keeps no buttons: the reply's answers card says what it chose. */
export function WrapUpBox(props: { box: WrapUp; open: boolean; written?: boolean; onReply?: (text: string) => void }) {
  // A conclusion with its summary is the AI's own last message in the thread: the box then holds only its question, if any.
  // An AI that concluded and wrote nothing after it leaves the recap here, so the result is never lost.
  const own = () => !!props.box.summary && props.written !== false;
  return (
    <Show when={!own() || props.box.ask || props.box.proof}>
      <section class="wrap-up" aria-label={t('wrap.label')}>
        <Show when={!own()}>
          <p class="recap">{props.box.recap}</p>
        </Show>
        <Show when={props.box.proof}>{(proof) => <ProofLines proof={proof()} />}</Show>
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
    </Show>
  );
}
