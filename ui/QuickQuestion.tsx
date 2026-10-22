import { createEffect, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { api, errorText, type Aside, type Conversation } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { Markdown, openLinks } from './markdown.tsx';
import { prettyModel, vendorName, type Option } from './models.ts';
import { animateList } from './motion.ts';
import { keep, kept, release, useDraft } from './unsent.ts';

/** The quick question open in the running column (R250): the conversation it asks about, and a question `/btw` brought. */
export interface OpenAside {
  conversationId?: string;
  question?: string;
}

const [opened, setOpened] = createSignal<OpenAside>();
export const openedAside = opened;
/** Opens the quick question in the running column, or goes back to the running list. */
export const showAside = (open?: OpenAside) => setOpened(open);

// The engine's side thread per conversation, all of it each time one changes; it keeps the newest turns with the
// conversation. Each question keeps its row (by id) while its answer streams, so focus and a selection in it stay.
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

/** The suggestions of an empty side chat: Recap (R265) and two plain questions, each asked at once. */
const SUGGESTIONS = [
  { label: 'aside.recap', question: 'aside.recapQuestion' },
  { label: 'aside.left', question: 'aside.leftQuestion' },
  { label: 'aside.lastStep', question: 'aside.lastStepQuestion' },
] as const;

/** A side chat beside the conversation on screen: each question asks a read-only copy of the conversation, with the side
 * thread's last turns, and nothing goes back to it unless the person sends an answer to the agent's box (`onTell`). */
export function QuickQuestion(props: {
  conversation?: Conversation;
  /** The models of the person's AIs; the chip offers those of the conversation's own AI. */
  options: Option[];
  onClose: () => void;
  onTell: (text: string) => void;
}) {
  const id = () => props.conversation?.id;
  const vendor = () => props.conversation?.messages.findLast((m) => m.role === 'assistant' && m.vendor)?.vendor;
  const asides = () => lists[id() ?? ''] ?? [];
  const running = () => asides().find((a) => a.state === 'running');
  const models = () => props.options.filter((o) => o.choice.vendor === vendor());
  const [text, setText] = createSignal('');
  const [model, setModel] = createSignal('');
  const [error, setError] = createSignal('');
  let body: HTMLDivElement | undefined;
  let box: HTMLTextAreaElement | undefined;
  useDraft(() => text().trim().length > 0);
  // The conversation on screen, kept here: after the view closes its props can no longer be read (an answer to an ask
  // can come back then, and the cleanup runs then).
  let shown: string | undefined;
  // A draft belongs to the conversation it was typed for, even when an answer to it comes back after a switch.
  const write = (value: string, conversation = id()) => {
    if (!conversation) return;
    keep(`aside.${conversation}`, value);
    if (conversation === shown) setText(value);
  };
  createEffect(on(id, (conversation, previous) => {
    setError('');
    if (previous) release(`aside.${previous}`);
    shown = conversation;
    // The draft of this conversation's side chat comes back, as the main box's does.
    setText(conversation ? kept(`aside.${conversation}`) : '');
    if (!conversation) return;
    const seen = heard.get(conversation) ?? 0;
    void api.asides(conversation).then((list) => {
      if ((heard.get(conversation) ?? 0) === seen) heardAsides(conversation, list);
    }).catch(() => {});
  }));
  onCleanup(() => {
    if (shown) release(`aside.${shown}`);
    shown = undefined;
  });
  // The newest turn stays in view while it streams, unless the person scrolled up to read an earlier one.
  let pinned = true;
  createEffect(on(() => asides().map((a) => a.answer.length + a.state).join(), () => {
    if (body && pinned) body.scrollTop = body.scrollHeight;
  }));
  // Where a question came from: the side box (it empties on success and gets the words back on failure), `/btw` in the
  // conversation's box (a failure leaves the words in an empty side box), or a suggestion. Only the box's own words
  // ever replace its draft.
  const ask = async (question: string, from: 'box' | 'btw' | 'suggestion') => {
    const conversation = id();
    if (!conversation || !question.trim()) return;
    setError('');
    pinned = true;
    if (from !== 'box') box?.focus();
    try {
      await api.askAside(conversation, question, model() || undefined);
      if (from === 'box') write('', conversation);
    } catch (err) {
      if (from === 'box' || (from === 'btw' && !kept(`aside.${conversation}`).trim())) write(question, conversation);
      if (conversation === shown) setError(fromEngine(errorText(err)));
    }
  };
  const stop = () => {
    const a = running();
    if (a) void api.stopAside(a.id).catch(() => {});
  };
  // Esc stops the answer under way from anywhere in the column, before it would close the column.
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || e.defaultPrevented || e.isComposing || !running()) return;
    const at = e.target instanceof Element ? e.target : null;
    if (at && at !== document.body && !at.closest('.runcol')) return;
    e.preventDefault();
    e.stopPropagation();
    stop();
  };
  window.addEventListener('keydown', onEscape, true);
  onCleanup(() => window.removeEventListener('keydown', onEscape, true));
  // A question `/btw` brought is asked at once, once; the recap's fixed question is not the person's either.
  createEffect(on(opened, (open) => {
    if (!open?.question) return;
    setOpened({ conversationId: open.conversationId });
    const from = open.question === t('aside.recapQuestion') ? 'suggestion' : 'btw';
    if (vendor()) void ask(open.question, from);
    else if (from === 'btw' && !text().trim()) write(open.question);
  }));
  return (
    <>
      <div class="rc-head side-head">
        <h2 id="running-title" class="display">
          {t('aside.title')}
        </h2>
        <Show when={asides().length > 0}>
          <button class="btn ghost side-clear" onClick={() => id() && void api.clearAsides(id()!).catch(() => {})}>
            {t('aside.clear')}
          </button>
        </Show>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div
        ref={body}
        class="rc-body side-body side-chat"
        data-surface="aside"
        onScroll={(e) => (pinned = e.currentTarget.scrollHeight - e.currentTarget.scrollTop - e.currentTarget.clientHeight < 48)}
      >
        <Show when={vendor()} fallback={<p class="side-meta side-empty">{t('aside.none')}</p>}>
          <Show when={asides().length === 0}>
            <div class="side-empty">
              <p>{t('aside.note')}</p>
              <div class="chips">
                <For each={SUGGESTIONS}>
                  {(s) => (
                    <button type="button" class="chip suggest" classList={{ 'aside-recap': s.label === 'aside.recap' }} disabled={!!running()} onClick={() => void ask(t(s.question), 'suggestion')}>
                      {t(s.label)}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>
          <ol class="side-thread" ref={animateList}>
            <For each={asides()}>
              {(a) => (
                <li class="aside" data-state={a.state}>
                  <div class="you">{a.question}</div>
                  <div class="aside-reply">
                    <div class="model">
                      <span class="vm" classList={{ claude: a.vendor === 'claude', codex: a.vendor === 'codex' }}>
                        <VendorMark vendor={a.vendor} />
                        <b>{a.model ? prettyModel(a.model) : vendorName(a.vendor)}</b>
                      </span>
                    </div>
                    <Show when={a.answer}>
                      <div class="aside-answer md-small" onClick={openLinks}>
                        <Markdown text={a.answer} />
                      </div>
                    </Show>
                    <Show when={a.state === 'running' && !a.answer}>
                      <p class="side-meta looking" role="status">
                        {t('aside.looking')}
                      </p>
                    </Show>
                    <Show when={a.error}>{(key) => <p class="bad" role="alert">{fromEngine(key())}</p>}</Show>
                    <Show when={a.state === 'stopped'}>
                      <p class="side-meta">{t('aside.stopped')}</p>
                    </Show>
                    <Show when={a.state === 'done' && a.answer}>
                      <div class="aside-actions">
                        <button type="button" class="btn ghost aside-tell" onClick={() => props.onTell(a.answer)}>
                          <Icon name="up" small />
                          {t('aside.send')}
                        </button>
                      </div>
                    </Show>
                  </div>
                </li>
              )}
            </For>
          </ol>
        </Show>
      </div>
      <Show when={vendor()}>
        <form
          class="side-compose"
          onSubmit={(e) => {
            e.preventDefault();
            if (!running()) void ask(text(), 'box');
          }}
        >
          <textarea
            ref={box}
            id="aside-box"
            rows={1}
            value={text()}
            aria-label={t('aside.label')}
            placeholder={t(asides().length ? 'aside.followUp' : 'aside.placeholder')}
            onInput={(e) => write(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.shiftKey || e.isComposing) return;
              e.preventDefault();
              if (!running()) void ask(text(), 'box');
            }}
          />
          <Show when={error()}>
            <p class="bad" role="alert">
              {error()}
            </p>
          </Show>
          <div class="cbar">
            <Show when={models().length > 1}>
              <label class="chip choose">
                <VendorMark vendor={vendor()} />
                <select aria-label={t('aside.model')} value={model()} onChange={(e) => setModel(e.currentTarget.value)}>
                  <option value="">{t('aside.sameModel')}</option>
                  <For each={models()}>{(o) => <option value={o.choice.model}>{o.label}</option>}</For>
                </select>
                <Icon name="down" small />
              </label>
            </Show>
            <Show
              when={running()}
              fallback={
                <button type="submit" class="send" disabled={!text().trim()} aria-label={t('aside.ask')}>
                  <span aria-hidden="true">{t('aside.ask')}</span>
                  <Icon name="up" small />
                </button>
              }
            >
              <button type="button" class="send stop" onClick={stop} aria-label={t('aside.stop')}>
                <Icon name="stop" small />
                <span aria-hidden="true">{t('aside.stop')}</span>
              </button>
            </Show>
          </div>
        </form>
      </Show>
    </>
  );
}
