import { createEffect, createSignal, For, on, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { api, errorText, type Aside, type Conversation } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { Markdown, openLinks } from './markdown.tsx';
import type { Option } from './models.ts';
import { animateList } from './motion.ts';
import { useDraft } from './unsent.ts';

/** The quick question open in the running column (R250): the conversation it asks about, and a question `/btw` brought. */
export interface OpenAside {
  conversationId?: string;
  question?: string;
}

const [opened, setOpened] = createSignal<OpenAside>();
export const openedAside = opened;
/** Opens the quick question in the running column, or goes back to the running list. */
export const showAside = (open?: OpenAside) => setOpened(open);

// The engine's list per conversation, all of it each time one changes; the answers go when the turn after them ends. Each
// question keeps its row (by id) while its answer streams, so focus and a selection in it stay.
const [lists, setLists] = createStore<Record<string, Aside[]>>({});
// How many lists the engine sent per conversation: a snapshot asked before the latest one is older than it.
const heard = new Map<string, number>();
export const heardAsides = (conversationId: string, asides: Aside[]) => {
  heard.set(conversationId, (heard.get(conversationId) ?? 0) + 1);
  setLists(conversationId, reconcile(asides, { key: 'id' }));
};

/** Whether the conversation's agent has answered once: before that a quick question has nothing to ask about. */
export const answered = (c?: Conversation) => !!c?.messages.some((m) => m.role === 'assistant' && m.vendor);

/** `/btw <question>` in the conversation's box: the question, or undefined when the text is a message. */
export function btw(text: string): string | undefined {
  const m = /^\/btw(?:\s+([\s\S]*))?$/i.exec(text.trim());
  return m ? (m[1] ?? '').trim() : undefined;
}

/** A quick question on the side of the conversation on screen: it asks a read-only copy of the conversation, and nothing
 * goes back to it unless the person sends an answer to the agent's box (`onTell`). */
export function QuickQuestion(props: {
  conversation?: Conversation;
  /** The models of the person's AIs; the picker offers those of the conversation's own AI. */
  options: Option[];
  onClose: () => void;
  onTell: (text: string) => void;
}) {
  const id = () => props.conversation?.id;
  const vendor = () => props.conversation?.messages.findLast((m) => m.role === 'assistant' && m.vendor)?.vendor;
  const asides = () => lists[id() ?? ''] ?? [];
  const asking = () => asides().some((a) => a.state === 'running');
  const models = () => props.options.filter((o) => o.choice.vendor === vendor());
  const [text, setText] = createSignal('');
  const [model, setModel] = createSignal('');
  const [error, setError] = createSignal('');
  useDraft(() => text().trim().length > 0);
  createEffect(on(id, (conversation) => {
    setError('');
    if (!conversation) return;
    const seen = heard.get(conversation) ?? 0;
    void api.asides(conversation).then((list) => {
      if ((heard.get(conversation) ?? 0) === seen) heardAsides(conversation, list);
    }).catch(() => {});
  }));
  const ask = async (question: string) => {
    const conversation = id();
    if (!conversation || !question.trim()) return;
    setError('');
    // The recap's fixed question is not the person's: their draft in the box stays as it was.
    const own = question !== t('aside.recapQuestion');
    try {
      await api.askAside(conversation, question, model() || undefined);
      if (own) setText('');
    } catch (err) {
      if (own) setText(question);
      setError(fromEngine(errorText(err)));
    }
  };
  // A question `/btw` brought is asked at once, once.
  createEffect(on(opened, (open) => {
    if (!open?.question) return;
    setOpened({ conversationId: open.conversationId });
    if (vendor()) void ask(open.question);
    else setText(open.question);
  }));
  return (
    <>
      <div class="rc-head side-head">
        <h2 id="running-title" class="display">
          {t('aside.title')}
        </h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div class="rc-body side-body" data-surface="aside" ref={animateList}>
        <p class="side-meta">{vendor() ? t('aside.note') : t('aside.none')}</p>
        <For each={asides()}>
          {(a) => (
            <section class="aside" data-state={a.state} ref={animateList}>
              <div class="side-brief">
                <div class="you">{a.question}</div>
              </div>
              <Show when={a.answer}>
                <div class="aside-answer" onClick={openLinks}>
                  <Markdown text={a.answer} />
                </div>
              </Show>
              <Show when={a.state === 'running' && !a.answer}>
                <p class="side-meta" role="status">
                  {t('aside.looking')}
                </p>
              </Show>
              <Show when={a.error}>{(key) => <p class="bad" role="alert">{fromEngine(key())}</p>}</Show>
              <Show when={a.state === 'stopped'}>
                <p class="side-meta">{t('aside.stopped')}</p>
              </Show>
              <Show when={a.state === 'running'}>
                <button class="btn ghost" onClick={() => void api.stopAside(a.id).catch(() => {})}>
                  {t('aside.stop')}
                </button>
              </Show>
              <Show when={a.state === 'done' && a.answer}>
                <button class="btn ghost" onClick={() => props.onTell(a.answer)}>
                  {t('aside.send')}
                </button>
              </Show>
            </section>
          )}
        </For>
        <Show when={vendor()}>
          <form
            class="side-write"
            onSubmit={(e) => {
              e.preventDefault();
              void ask(text());
            }}
          >
            <label for="aside-box">{t('aside.label')}</label>
            <textarea
              id="aside-box"
              class="field"
              rows={2}
              value={text()}
              placeholder={t('aside.placeholder')}
              onInput={(e) => setText(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key !== 'Enter' || e.shiftKey || e.isComposing) return;
                e.preventDefault();
                void ask(text());
              }}
            />
            <div class="aside-row">
              <button type="button" class="btn ghost aside-recap" disabled={asking()} onClick={() => void ask(t('aside.recapQuestion'))}>
                {t('aside.recap')}
              </button>
              <Show when={models().length > 1}>
                <select class="field" aria-label={t('aside.model')} value={model()} onChange={(e) => setModel(e.currentTarget.value)}>
                  <option value="">{t('aside.sameModel')}</option>
                  <For each={models()}>{(o) => <option value={o.choice.model}>{o.label}</option>}</For>
                </select>
              </Show>
              <button type="submit" class="btn" disabled={!text().trim() || asking()}>
                {t('aside.ask')}
              </button>
            </div>
            <Show when={error()}>
              <p class="bad" role="alert">
                {error()}
              </p>
            </Show>
          </form>
        </Show>
      </div>
    </>
  );
}
