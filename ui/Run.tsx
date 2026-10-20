import { createEffect, createSignal, For, Index, on, Show, type JSX } from 'solid-js';
import type { Message, Plan } from './api.ts';
import { openMenu } from './contextmenu.ts';
import { Icon } from './Glyphs.tsx';
import { elapsed } from './running.ts';
import { fromEngine, t } from './i18n.ts';
import { planWords } from './Plan.tsx';

/** A Keep going run (task 018): turns Faber started by itself, one after another with no message of the person between. */
export interface Run {
  /** Index in the conversation of each reply of the run, oldest first. */
  replies: number[];
}

/** The reply a message Faber sent by itself was answered with. */
const byFaber = (messages: Message[], at: number) => messages[at]?.role === 'assistant' && !!messages[at - 1]?.wake;

/** The runs of a conversation by the index of their newest reply. Only two turns or more make a run: one turn Faber sent
 * reads as a reply like any other. */
export function runsOf(messages: Message[]): Map<number, Run> {
  const runs = new Map<number, Run>();
  let replies: number[] = [];
  const close = () => {
    if (replies.length >= 2) runs.set(replies.at(-1)!, { replies });
    replies = [];
  };
  for (let at = 0; at < messages.length; at++) {
    if (messages[at]!.role === 'user') {
      if (!messages[at]!.wake) close();
      continue;
    }
    if (byFaber(messages, at)) replies.push(at);
    else close();
  }
  close();
  return runs;
}

/** The reply indexes a run folds away (all but its newest). */
export function folded(runs: Map<number, Run>): Set<number> {
  return new Set([...runs.values()].flatMap((run) => run.replies.slice(0, -1)));
}

/** A question Faber answered for the person, with the choices they may change it to. */
export interface Answered {
  question: string;
  answer: string;
  others: string[];
}

/** The questions Faber answered in these replies: the agent's questions (`autoAnswers`) and its closing boxes. */
export function answeredIn(replies: Message[]): Answered[] {
  return replies.flatMap((m) => [
    ...(m.autoAnswers ?? []).map((a) => ({ question: a.question, answer: a.answer, others: a.others ?? [] })),
    ...(m.wrapUp?.ask && m.wrapUp.answered
      ? [{ question: m.wrapUp.ask, answer: m.wrapUp.answered, others: (m.wrapUp.actions ?? []).filter((a) => a !== m.wrapUp!.answered) }]
      : []),
  ]);
}

/** "Faber answered 3 questions for you": one quiet card, a row per question with the answer chosen. "Change" sends another
 * choice as the person's message (with more than one, a menu picks it). */
export function AnswersCard(props: { answers: Answered[]; onReply?: (text: string) => void }) {
  const change = (e: MouseEvent, a: Answered) => {
    const send = (answer: string) => props.onReply?.(t('answers.changeTo', { question: a.question, answer }));
    if (a.others.length === 1) return send(a.others[0]!);
    openMenu(e, a.others.map((other, i) => ({ id: `other-${i}`, label: other, run: () => send(other) })));
  };
  return (
    <section class="answers" aria-label={t('answers.title', { n: props.answers.length })}>
      <p class="woke">
        <Icon name="loop" small />
        {t('answers.title', { n: props.answers.length })}
      </p>
      <ul>
        {/* By place, not by object: the list is made again on every tick of a stream, and a row (and a focused Change) stays. */}
        <Index each={props.answers}>
          {(a) => (
            <li>
              <span class="q" title={a().question}>{a().question}</span>
              <span class="a">{a().answer}</span>
              <Show when={props.onReply && a().others.length}>
                <button type="button" class="btn ghost change" onClick={(e) => change(e, a())}>
                  {t('answers.change')}
                </button>
              </Show>
            </li>
          )}
        </Index>
      </ul>
    </section>
  );
}

/** What Faber asked in a run, once per reason, for the header's tooltip. */
function reasons(messages: Message[], run: Run): string {
  const count = new Map<string, number>();
  for (const at of run.replies) {
    const why = messages[at - 1]?.wake?.why;
    if (why) count.set(why, (count.get(why) ?? 0) + 1);
  }
  return [...count].map(([why, n]) => (n > 1 ? `${fromEngine(why)} ×${n}` : fromEngine(why))).join(' · ');
}

/** One header over a run: the loop icon, how long it worked on its own and how many turns, and the plan's progress on the
 * newest run, in the work log's disclosure. Opened, it shows the earlier turns, each with its work log folded; only the
 * newest turn stays open below it. The earlier turns are drawn only while shown, so a long run keeps a small screen. */
export function RunBlock(props: { messages: Message[]; run: Run; plan?: Plan; earlier: (at: number) => JSX.Element; reveal?: number; from?: number }) {
  const [open, setOpen] = createSignal(false);
  let details!: HTMLDetailsElement;
  createEffect(on(() => props.reveal, (at) => {
    if (at !== undefined && props.run.replies.slice(0, -1).includes(at)) {
      details.open = true;
      setOpen(true);
    }
  }));
  const first = () => props.messages[props.run.replies[0]! - 1];
  const newest = () => props.messages[props.run.replies.at(-1)!];
  const time = () => {
    const end = newest();
    const e = elapsed(first()?.at ?? 0, end?.finishedAt ?? end?.movedAt ?? end?.at ?? 0);
    return t(e.key, { n: e.n });
  };
  const plan = () => {
    const p = props.plan;
    if (!p?.steps.length) return '';
    return t('run.block.plan', { count: planWords({ done: p.steps.filter((s) => s.state === 'done').length, total: p.steps.length }) });
  };
  const title = () =>
    [t('run.block.title', { time: time(), turns: t('run.block.turns', { n: props.run.replies.length }) }), plan()].filter(Boolean).join(' · ');
  return (
    <details class="run-block disclosure" ref={(el) => (details = el)} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary title={reasons(props.messages, props.run) || undefined}>
        <Icon name="loop" small />
        <span class="run-title">{title()}</span>
      </summary>
      <Show when={open()}>
        <div class="run-earlier">
          <For each={props.run.replies.slice(0, -1).filter((at) => at >= (props.from ?? 0))}>{(at) => props.earlier(at)}</For>
        </div>
      </Show>
    </details>
  );
}
