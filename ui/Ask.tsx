import { createMemo, createSignal, For, Show } from 'solid-js';
import { api, type Parked, type Permission } from './api.ts';
import { fromEngine, t } from './i18n.ts';
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
  options: { label: string; description?: string }[];
}

/** The questions the assistant put to the person (the `AskUserQuestion` tool), read defensively: anything malformed is dropped. */
export function askedQuestions(input: unknown): AskedQuestion[] {
  const list = (input && typeof input === 'object' ? (input as { questions?: unknown }).questions : undefined) as unknown;
  if (!Array.isArray(list)) return [];
  return list.flatMap((q) => {
    if (!q || typeof q !== 'object' || typeof q.question !== 'string') return [];
    const options = (Array.isArray(q.options) ? q.options : []).flatMap((o: unknown) => {
      const option = o as { label?: unknown; description?: unknown };
      return typeof option?.label === 'string' ? [{ label: option.label, description: typeof option.description === 'string' ? option.description : undefined }] : [];
    });
    return [{ question: q.question, header: typeof q.header === 'string' ? q.header : undefined, multiSelect: q.multiSelect === true, options }];
  });
}

/** The assistant's own questions as a card in its message: tap an answer for each, or write another, then send. */
function QuestionCard(props: { conversationId: string; turnId: string; permission: Permission; surface?: string }) {
  const questions = () => askedQuestions(props.permission.input);
  // Kept by the question's place in the list, never by its text: the text is the model's, and could be `__proto__`.
  const [picked, setPicked] = createSignal<string[][]>([]);
  const [other, setOther] = createSignal<string[]>([]);
  const at = <T,>(list: T[], i: number, value: T) => {
    const next = [...list];
    next[i] = value;
    return next;
  };
  const [dropped, setDropped] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  const chosen = (i: number) => picked()[i] ?? [];
  // A single choice is one answer: tapping an option clears what was written, and writing clears the tapped option.
  const toggle = (q: AskedQuestion, i: number, label: string) => {
    const had = chosen(i).includes(label);
    const rest = chosen(i).filter((l) => l !== label);
    setPicked((now) => at(now, i, q.multiSelect ? (had ? rest : [...rest, label]) : had ? [] : [label]));
    if (!q.multiSelect && !had) setOther((now) => at(now, i, ''));
  };
  const written = (i: number) => (other()[i] ?? '').trim();
  const answerOf = (i: number) => [...chosen(i), ...(written(i) ? [written(i)] : [])].join(', ');
  const ready = () => questions().length > 0 && questions().every((_, i) => answerOf(i) !== '');
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
      <div class="ask question" data-surface={props.surface} data-testid="question-card" role="group" aria-label={t('ask.question.label')}>
        <For each={questions()}>
          {(q, i) => (
            <fieldset class="q" disabled={sending()}>
              <legend>
                <Show when={q.header}>
                  <span class="q-head">{q.header}</span>
                </Show>
                {q.question}
              </legend>
              <Show when={q.multiSelect}>
                <span class="note">{t('ask.question.many')}</span>
              </Show>
              <div class="q-options">
                <For each={q.options}>
                  {(o) => (
                    <button
                      type="button"
                      class="q-option"
                      classList={{ on: chosen(i()).includes(o.label) }}
                      role={q.multiSelect ? 'checkbox' : 'radio'}
                      aria-checked={chosen(i()).includes(o.label)}
                      onClick={() => toggle(q, i(), o.label)}
                    >
                      <span>{o.label}</span>
                      <Show when={o.description}>
                        <small>{o.description}</small>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
              <input
                class="field q-other"
                type="text"
                aria-label={t('ask.question.other')}
                placeholder={t('ask.question.otherPlaceholder')}
                value={other()[i()] ?? ''}
                onInput={(e) => {
                  const text = e.currentTarget.value;
                  setOther((now) => at(now, i(), text));
                  if (text && !q.multiSelect) setPicked((now) => at(now, i(), []));
                }}
              />
            </fieldset>
          )}
        </For>
        <div>
          <button class="btn primary" disabled={sending() || !ready()} onClick={() => void submit()}>
            {t('ask.question.send')}
          </button>
          <button class="btn" disabled={sending()} onClick={() => void skip()}>
            {t('ask.question.skip')}
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
  return (
    <>
      <Show when={first()} keyed>
        {(p) =>
          p.tool === 'AskUserQuestion' ? (
            <QuestionCard conversationId={props.conversationId} turnId={props.turnId} permission={p} surface={props.surface} />
          ) : (
            <Card conversationId={props.conversationId} turnId={props.turnId} permission={p} surface={props.surface} />
          )
        }
      </Show>
      <Show when={props.permissions.length > 1}>
        <div class="note">{t('ask.moreWaiting', { n: props.permissions.length - 1 })}</div>
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
