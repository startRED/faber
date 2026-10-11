import { createMemo, createSignal, Show } from 'solid-js';
import { api, type Permission } from './api.ts';
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

/** One question the CLI waits on. The conversation, the turn and the request name it, so an answer can only reach the
 * question it was given for. The same question can be on screen twice (in the thread and in the running column). */
function Card(props: { conversationId: string; turnId: string; permission: Permission }) {
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
      <div class="ask" role="alertdialog" aria-label={t('ask.label')}>
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
export function Asks(props: { conversationId: string; turnId: string; permissions: Permission[] }) {
  // The card is kept while the same question stays first, however often the list is rebuilt around it.
  const first = createMemo(() => props.permissions[0], undefined, { equals: (a, b) => a?.requestId === b?.requestId });
  return (
    <>
      <Show when={first()} keyed>
        {(p) => <Card conversationId={props.conversationId} turnId={props.turnId} permission={p} />}
      </Show>
      <Show when={props.permissions.length > 1}>
        <div class="note">{t('ask.moreWaiting', { n: props.permissions.length - 1 })}</div>
      </Show>
    </>
  );
}
