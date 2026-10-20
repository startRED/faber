import { createEffect, createMemo, createSignal, createUniqueId, For, onCleanup, Show, untrack } from 'solid-js';
import { api, PERMISSION_MODES, type Parked, type Permission, type PermissionMode } from './api.ts';
import { PermissionChip } from './Composer.tsx';
import { Markdown, openLinks } from './markdown.tsx';
import { motion } from './appearance.ts';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { millis } from './motion.ts';
import { answerOutcome } from './running.ts';

// What a permission request means, in plain words.
function describe(tool: string, input: unknown): { title: string; note?: string; body: string } {
  const i = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
  const raw = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
  if (tool === 'Bash' || tool === 'command') {
    return { title: t('ask.command'), note: typeof i.description === 'string' ? i.description : undefined, body: typeof input === 'string' ? input : String(i.command ?? raw) };
  }
  if (['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool)) return { title: t('ask.file'), body: String(i.file_path ?? i.path ?? raw) };
  if (tool === 'WebFetch') return { title: t('ask.page'), body: String(i.url ?? raw) };
  return { title: t('ask.other', { tool }), body: raw };
}

interface AskedQuestion {
  question: string;
  header?: string;
  multiSelect?: boolean;
  options: { label: string; description?: string; preview?: string }[];
}

/** The questions the assistant put to the person (the `AskUserQuestion` tool), read defensively: anything malformed is dropped. */
export function askedQuestions(input: unknown): AskedQuestion[] {
  const list = (input && typeof input === 'object' ? (input as { questions?: unknown }).questions : undefined) as unknown;
  if (!Array.isArray(list)) return [];
  return list.flatMap((q) => {
    if (!q || typeof q !== 'object' || typeof q.question !== 'string') return [];
    const options = (Array.isArray(q.options) ? q.options : []).flatMap((o: unknown) => {
      const option = o as { label?: unknown; description?: unknown; preview?: unknown };
      if (typeof option?.label !== 'string') return [];
      const text = (v: unknown) => (typeof v === 'string' && v.trim() ? v : undefined);
      return [{ label: option.label, description: text(option.description), preview: text(option.preview) }];
    });
    return [{ question: q.question, header: typeof q.header === 'string' ? q.header : undefined, multiSelect: q.multiSelect === true, options }];
  });
}

// The assistant marks the option it suggests by ending its label with "(Recommended)". The tile shows it as a badge; the label that
// goes back to the assistant stays exactly as it wrote it.
const RECOMMENDED = /\s*\(recommended\)\s*$/i;
const titleOf = (label: string) => label.replace(RECOMMENDED, '') || label;
// A mockup may come fenced as markdown; the panel shows the drawing itself, as plain text.
const previewText = (text: string) => text.replace(/^\s*```[^\n]*\n/, '').replace(/\n```\s*$/, '');

/** The assistant's own questions as a card in its message: one step per question, a tile for each answer (with a preview beside it
 * when the assistant drew one), a quiet way to write another answer, then send. `gone` folds the card away once it is answered. */
function QuestionCard(props: { conversationId: string; turnId: string; permission: Permission; surface?: string; gone?: boolean }) {
  const uid = createUniqueId();
  const questions = () => askedQuestions(props.permission.input);
  // Kept by the question's place in the list, never by its text: the text is the model's, and could be `__proto__`.
  const [picked, setPicked] = createSignal<string[][]>([]);
  const [other, setOther] = createSignal<string[]>([]);
  const [writing, setWriting] = createSignal<boolean[]>([]);
  const [peek, setPeek] = createSignal<(number | undefined)[]>([]);
  const [active, setActive] = createSignal(0);
  const at = <T,>(list: T[], i: number, value: T) => {
    const next = [...list];
    next[i] = value;
    return next;
  };
  const [dropped, setDropped] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  const chosen = (i: number) => picked()[i] ?? [];
  const written = (i: number) => (other()[i] ?? '').trim();
  const answerOf = (i: number) => [...chosen(i), ...(written(i) ? [written(i)] : [])].join(', ');
  const answered = (i: number) => answerOf(i) !== '';
  const total = () => questions().length;
  const done = () => questions().filter((_, i) => answered(i)).length;
  const ready = () => total() > 0 && done() === total();
  const panels: HTMLElement[] = [];
  const steps: HTMLElement[] = [];
  const fields: HTMLInputElement[] = [];
  let advancing: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(advancing));
  // A single choice is one answer: tapping an option clears what was written, and writing clears the tapped option.
  const toggle = (q: AskedQuestion, i: number, label: string) => {
    const had = chosen(i).includes(label);
    const rest = chosen(i).filter((l) => l !== label);
    setPicked((now) => at(now, i, q.multiSelect ? (had ? rest : [...rest, label]) : had ? [] : [label]));
    if (!q.multiSelect && !had) setOther((now) => at(now, i, ''));
  };
  const choose = (i: number, label: string) => {
    setPicked((now) => at(now, i, [label]));
    setOther((now) => at(now, i, ''));
  };
  const nextOpen = (from: number) => [...questions().keys()].map((k) => (from + 1 + k) % total()).find((j) => j !== from && !answered(j));
  const stay = () => clearTimeout(advancing);
  const show = (j: number) => {
    stay();
    setActive(j);
  };
  const goTo = (j: number) => {
    setActive(j);
    (panels[j]?.querySelector('[tabindex="0"], .q-opt, .q-more') as HTMLElement | null)?.focus({ preventScroll: true });
  };
  const advance = (i: number) => {
    const j = nextOpen(i);
    if (j === undefined || total() < 2) return;
    clearTimeout(advancing);
    // Long enough to see the tile take, short enough not to feel slow.
    advancing = setTimeout(() => active() === i && answered(i) && !writing()[i] && goTo(j), 260);
  };
  // Arrows walk the tiles: a single choice takes the one it lands on (like a radio group), several only move the focus.
  const walk = (e: KeyboardEvent, q: AskedQuestion, i: number, n: number) => {
    stay();
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    const to = (n + step + q.options.length) % q.options.length;
    (e.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLElement>('.q-opt')[to]?.focus();
    if (!q.multiSelect) choose(i, q.options[to].label);
    setPeek((now) => at(now, i, to));
  };
  const onStepKey = (e: KeyboardEvent, i: number) => {
    const to = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: total() - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const j = (to + total()) % total();
    show(j);
    steps[j]?.focus();
  };
  const tabbable = (q: AskedQuestion, i: number, n: number) =>
    q.multiSelect || (chosen(i).length ? chosen(i).includes(q.options[n].label) : n === 0) ? 0 : -1;
  // The tile the preview panel draws: the one under the pointer or the focus, else the one chosen, else the first that has a drawing.
  const shown = (q: AskedQuestion, i: number) => {
    const by = peek()[i] ?? q.options.findIndex((o) => chosen(i).includes(o.label));
    return q.options[by >= 0 ? by : Math.max(0, q.options.findIndex((o) => o.preview))];
  };
  const openOther = (i: number) => {
    stay();
    const open = !(writing()[i] ?? false);
    setWriting((now) => at(now, i, open));
    if (open) fields[i]?.focus({ preventScroll: true });
  };
  const finish = async (send: () => Promise<void>) => {
    setProblem(undefined);
    setSending(true);
    try {
      await send();
    } catch (err) {
      const outcome = answerOutcome(err);
      if (outcome === 'stale') setDropped(true);
      else setProblem(fromEngine(outcome.problem));
    } finally {
      setSending(false);
    }
  };
  const submit = () =>
    finish(() => api.answerQuestion(props.conversationId, props.turnId, props.permission.requestId, Object.fromEntries(questions().map((q, i) => [q.question, answerOf(i)]))));
  const skip = () => finish(() => api.answer(props.conversationId, props.turnId, props.permission.requestId, false));
  return (
    <Show when={!dropped()}>
      <div class="ask question" classList={{ gone: props.gone }} data-surface={props.surface} data-testid="question-card" role="group" aria-label={t('ask.question.label')}>
        <div class="q-inner" inert={sending() || props.gone} aria-busy={sending()}>
          <div class="q-body">
            <Show when={total() > 1}>
              <div class="q-steps" role="tablist" aria-label={t('ask.question.steps')}>
                <For each={questions()}>
                  {(q, i) => (
                    <button
                      type="button"
                      ref={(el) => (steps[i()] = el)}
                      class="q-step"
                      role="tab"
                      id={`${uid}-s${i()}`}
                      aria-selected={active() === i()}
                      aria-controls={`${uid}-q${i()}`}
                      tabindex={active() === i() ? 0 : -1}
                      classList={{ done: answered(i()) }}
                      onClick={() => show(i())}
                      onKeyDown={(e) => onStepKey(e, i())}
                    >
                      <span class="q-n" aria-hidden="true">
                        <Show when={answered(i())} fallback={i() + 1}>
                          <Icon name="check" small />
                        </Show>
                      </span>
                      <span class="q-t">{q.header || t('ask.question.step', { n: i() + 1 })}</span>
                      <Show when={answered(i())}>
                        <span class="sr">{t('ask.question.done')}</span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            </Show>
            <div class="q-stage">
              <For each={questions()}>
                {(q, i) => (
                  <div
                    class="q"
                    ref={(el) => (panels[i()] = el)}
                    id={`${uid}-q${i()}`}
                    role={total() > 1 ? 'tabpanel' : undefined}
                    aria-labelledby={total() > 1 ? `${uid}-s${i()}` : undefined}
                    data-pos={i() === active() ? 'now' : i() < active() ? 'before' : 'after'}
                    inert={i() !== active()}
                  >
                    <p class="q-text" id={`${uid}-t${i()}`}>
                      <Show when={q.header && total() < 2}>
                        <span class="q-head">{q.header}</span>
                      </Show>
                      {q.question}
                    </p>
                    <Show when={q.multiSelect}>
                      <span class="note">{t('ask.question.many')}</span>
                    </Show>
                    <div class="q-main" classList={{ 'with-preview': q.options.some((o) => o.preview) }}>
                      <div
                        class="q-options"
                        role={q.multiSelect ? 'group' : 'radiogroup'}
                        aria-labelledby={`${uid}-t${i()}`}
                        onMouseLeave={() => setPeek((now) => at(now, i(), undefined))}
                      >
                        <For each={q.options}>
                          {(o, n) => (
                            <button
                              type="button"
                              class="q-opt"
                              classList={{ on: chosen(i()).includes(o.label) }}
                              role={q.multiSelect ? 'checkbox' : 'radio'}
                              aria-checked={chosen(i()).includes(o.label)}
                              tabindex={tabbable(q, i(), n())}
                              onClick={() => {
                                const had = chosen(i()).includes(o.label);
                                toggle(q, i(), o.label);
                                if (!q.multiSelect && !had) advance(i());
                              }}
                              onKeyDown={(e) => walk(e, q, i(), n())}
                              onMouseEnter={() => setPeek((now) => at(now, i(), n()))}
                              onFocus={() => setPeek((now) => at(now, i(), n()))}
                            >
                              <span class="q-mark" aria-hidden="true">
                                <Icon name="check" small />
                              </span>
                              <span class="q-txt">
                                <span class="q-label">
                                  {titleOf(o.label)}
                                  <Show when={RECOMMENDED.test(o.label)}>
                                    <em class="q-rec">{t('ask.question.recommended')}</em>
                                  </Show>
                                </span>
                                <Show when={o.description}>
                                  <small title={o.description}>{o.description}</small>
                                </Show>
                              </span>
                            </button>
                          )}
                        </For>
                      </div>
                      <Show when={q.options.some((o) => o.preview) && shown(q, i())}>
                        {(o) => (
                          <div class="q-preview" tabindex="0" role="region" aria-label={t('ask.question.preview', { name: titleOf(o().label) })}>
                            <Show when={o().preview} keyed fallback={<p class="note">{t('ask.question.noPreview')}</p>}>
                              {(text) => <pre>{previewText(text)}</pre>}
                            </Show>
                          </div>
                        )}
                      </Show>
                    </div>
                    <div class="q-other">
                      <button
                        type="button"
                        class="q-more"
                        classList={{ has: written(i()) !== '' }}
                        aria-expanded={writing()[i()] ?? false}
                        aria-controls={`${uid}-o${i()}`}
                        onClick={() => openOther(i())}
                      >
                        <Icon name={written(i()) ? 'check' : 'plus'} small />
                        <span>{written(i()) && !writing()[i()] ? written(i()) : t('ask.question.write')}</span>
                      </button>
                      <div class="fold" classList={{ open: writing()[i()] }} inert={!writing()[i()]} id={`${uid}-o${i()}`}>
                        <div class="fold-inner">
                          <input
                            ref={(el) => (fields[i()] = el)}
                            class="field q-field"
                            type="text"
                            aria-label={t('ask.question.otherPlaceholder')}
                            placeholder={t('ask.question.otherPlaceholder')}
                            value={other()[i()] ?? ''}
                            onInput={(e) => {
                              stay();
                              const text = e.currentTarget.value;
                              setOther((now) => at(now, i(), text));
                              if (text && !q.multiSelect) setPicked((now) => at(now, i(), []));
                            }}
                            onKeyDown={(e) => {
                              if (e.key !== 'Enter') return;
                              e.preventDefault();
                              if (ready()) void submit();
                              else if (written(i())) {
                                const j = nextOpen(i());
                                if (j !== undefined) goTo(j);
                              }
                            }}
                          />
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </For>
            </div>
            <div class="q-foot">
              <Show when={total() > 1}>
                <span class="q-sum" role="status">
                  {t('ask.question.progress', { done: done(), total: total() })}
                </span>
              </Show>
              <button class="btn" onClick={() => void skip()}>
                {t('ask.question.skip')}
              </button>
              <button class="btn primary" disabled={!ready()} onClick={() => void submit()}>
                {t('ask.question.send')}
              </button>
            </div>
            <Show when={problem()}>
              <div class="note err" role="alert">
                {problem()}
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}

/** The same plan decision in the thread and running column; the engine owns both the mode and the run. */
export function PlanCard(props: { conversationId: string; turnId: string; permission: Permission; surface?: string }) {
  const input = () => props.permission.input as { mode?: PermissionMode; plan?: unknown };
  const plan = () => typeof input()?.plan === 'string' ? input().plan as string : '';
  const modes = PERMISSION_MODES.filter((m) => m !== 'plan');
  const [mode, setMode] = createSignal<PermissionMode>(modes.find((m) => m === input()?.mode) ?? 'ask');
  const [sending, setSending] = createSignal(false);
  const [changed, setChanged] = createSignal(false);
  const [dropped, setDropped] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  const answer = async (approve: boolean) => {
    setSending(true);
    setProblem(undefined);
    try {
      await api.approvePlan(props.conversationId, props.turnId, props.permission.requestId, approve ? mode() : undefined);
    } catch (err) {
      const outcome = answerOutcome(err);
      if (outcome === 'stale') setDropped(true);
      else {
        if (outcome.problem === 'error.permission.planChanged') setChanged(true);
        setProblem(fromEngine(outcome.problem));
      }
    } finally {
      setSending(false);
    }
  };
  return (
    <Show when={!dropped()}>
      <div class="ask plan-approval" data-surface={props.surface} data-testid="plan-approval" role="group" aria-label={t('ask.plan.title')} aria-busy={sending()}>
        <b role={props.surface === 'ask' ? 'status' : undefined} aria-live={props.surface === 'ask' ? 'polite' : undefined}>{t('ask.plan.title')}</b>
        <Show when={plan()}>{(text) => <div class="md" onClick={openLinks}><Markdown text={text()} /></div>}</Show>
        <div>
          <PermissionChip mode={mode()} onMode={setMode} modes={modes} caption={t('ask.plan.mode')} disabled={sending() || changed()} />
          <p class="note">{t('ask.plan.shared')}</p>
        </div>
        <div>
          <button class="btn primary" ref={(el) => {
            // The running column can show the same card; only its thread copy receives focus.
            if (props.surface === 'ask') queueMicrotask(() => {
              const active = document.activeElement;
              if (active instanceof HTMLElement && (active.matches('input, textarea') || active.isContentEditable)) return;
              if (el.isConnected) el.focus({ preventScroll: true });
            });
          }} disabled={sending() || changed()} onClick={() => void answer(true)}>{t('ask.plan.approve')}</button>
          <button class="btn" disabled={sending()} onClick={() => void answer(false)}>{t('ask.plan.keep')}</button>
        </div>
        <Show when={problem()}><div class="note err" role="alert">{problem()}</div></Show>
      </div>
    </Show>
  );
}

/** One question the CLI waits on. The conversation, the turn and the request name it, so an answer can only reach the
 * question it was given for. The same question can be on screen twice (in the thread and in the running column). */
function Card(props: { conversationId: string; turnId: string; permission: Permission; surface?: string }) {
  const [dropped, setDropped] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  const d = () => describe(props.permission.tool, props.permission.input);
  const answer = async (allow: boolean, always = false) => {
    setProblem(undefined);
    setSending(true);
    try {
      await api.answer(props.conversationId, props.turnId, props.permission.requestId, allow, always);
    } catch (err) {
      // A question already answered (here, in the other copy, or elsewhere) is dropped without an alarm; an answer the
      // CLI did not get keeps the card, with the reason, so the user can try again.
      const outcome = answerOutcome(err);
      if (outcome === 'stale') setDropped(true);
      else setProblem(fromEngine(outcome.problem));
    } finally {
      setSending(false);
    }
  };
  return (
    <Show when={!dropped()}>
      <div class="ask" data-surface={props.surface} role="alertdialog" aria-label={t('ask.label')}>
        <b>{d().title}</b>
        <Show when={d().note}>
          <span class="note">{d().note}</span>
        </Show>
        <pre>{d().body}</pre>
        <div>
          <button class="btn primary" disabled={sending()} onClick={() => void answer(true)}>
            {t('ask.allow')}
          </button>
          <button class="btn" disabled={sending()} onClick={() => void answer(true, true)}>
            {t('ask.always')}
          </button>
          <button class="btn" disabled={sending()} onClick={() => void answer(false)}>
            {t('ask.deny')}
          </button>
        </div>
        <Show when={problem()}>
          <div class="note err" role="alert">
            {problem()}
          </div>
        </Show>
      </div>
    </Show>
  );
}

/** The first waiting question as a card, and a note for the others. `turnId` is the id of the message that holds the
 * question (in the thread) or of the running item's turn (in the column): the same id. */
export function Asks(props: { conversationId: string; turnId: string; permissions: Permission[]; surface?: string }) {
  // The card is kept while the same question stays first, however often the list is rebuilt around it.
  const first = createMemo(() => props.permissions[0], undefined, { equals: (a, b) => a?.requestId === b?.requestId });
  // An answered question card stays until it has folded away, so the thread under it glides up instead of jumping.
  const [shown, setShown] = createSignal<Permission | undefined>(first());
  const [gone, setGone] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const p = first();
    clearTimeout(timer);
    if (p) {
      setShown(p);
      setGone(false);
    } else if (untrack(shown)?.tool === 'AskUserQuestion' && motion() === 'full') {
      setGone(true);
      timer = setTimeout(() => setShown(undefined), millis(document.documentElement, '--dur-exit', 180) + 40);
    } else setShown(undefined);
  });
  onCleanup(() => clearTimeout(timer));
  return (
    <>
      <Show when={shown()} keyed>
        {(p) =>
          p.tool === 'ExitPlanMode' ? (
            <PlanCard conversationId={props.conversationId} turnId={props.turnId} permission={p} surface={props.surface} />
          ) : p.tool === 'AskUserQuestion' ? (
            <QuestionCard conversationId={props.conversationId} turnId={props.turnId} permission={p} surface={props.surface} gone={gone()} />
          ) : (
            <Card conversationId={props.conversationId} turnId={props.turnId} permission={p} surface={props.surface} />
          )
        }
      </Show>
      <Show when={props.permissions.length > 1}>
        <div class="note ask-more">{t('ask.moreWaiting', { n: props.permissions.length - 1 })}</div>
      </Show>
    </>
  );
}

/** Requests the CLI denied because nobody could answer them. No buttons: the CLI is past them, and what was held back was not
 * done. The words say so and what the person can do. */
export function Parkeds(props: { parked: Parked[]; more?: number; surface?: string }) {
  return (
    <Show when={props.parked.length > 0 || (props.more ?? 0) > 0}>
      <div class="ask parked" data-surface={props.surface} role="status">
        <b>{t('ask.parked')}</b>
        <For each={props.parked}>
          {(p) => (
            <span class="note">
              {t(`parked.${p.kind}`, { tool: p.tool })}
              {p.label ? ` ${p.label}` : ''}
            </span>
          )}
        </For>
        <Show when={props.more}>{(n) => <span class="note">{t('parked.more', { n: n() })}</span>}</Show>
        <span class="note">{t('ask.parkedHint')}</span>
      </div>
    </Show>
  );
}
