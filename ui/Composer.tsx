import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { DEFAULT_EFFORT, type Choice, type Effort, PERMISSION_MODES, type PermissionMode, type Vendor } from './api.ts';
import { Ghost } from './Ghost.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Option } from './models.ts';
import { resizeComposer } from './composer-size.ts';
import { keep, kept, release, useDraft } from './unsent.ts';

export interface Draft {
  text: string;
  images: { mediaType: string; base64: string }[];
  files: { name: string; base64: string }[];
}

const EMPTY: Draft = { text: '', images: [], files: [] };

interface Props {
  vendors: Vendor[];
  options: Option[];
  choice: Choice;
  onChoice: (choice: Choice) => void;
  /** The permission mode of Chat, and what picks another. Chat only: a project composer passes neither and shows no chip. */
  mode?: PermissionMode;
  onMode?: (mode: PermissionMode) => void;
  busy: boolean;
  /** Subagents and helpers at work in the reply under way: Stop asks first, since it ends them too. */
  helpers?: number;
  /** "Keep going" in a project conversation under way: on, off, or undefined where it is not offered. */
  keepGoing?: boolean;
  onKeepGoing?: (on: boolean) => void;
  /** A conversation is still opening: nothing can be sent or warmed until it is in. */
  locked?: boolean;
  placeholder: string;
  /** Hints that take the place of the placeholder while the empty field idles (ui/Ghost.tsx); the placeholder stays for
   * reduced motion and for readers of the screen. */
  hints?: string[];
  onSend: (draft: Draft) => void;
  onStop: () => void;
  onWarm: () => void;
  /** The user stopped typing for a moment: Auto routes the draft before it is sent. */
  onPause: (text: string) => void;
  /** Hands the screen a function that puts text in the box and focuses it, without sending (the suggestions of the new
   * conversation use it). */
  onRestore?: (restore: (draft: Draft) => boolean) => void;
  onClearRestored?: (clear: (draft: Draft) => boolean) => void;
  onReady?: (fill: (text: string) => void) => void;
  /** Names this box's unsent text in storage; the text comes back after a restart. */
  draftKey?: string;
}

// Small text files go inside the message (no tool call needed); anything else is attached as a file.
const INLINE_TEXT = 50_000;
const TEXT = /\.(txt|md|csv|json|ya?ml|toml|xml|html?|css|[cm]?[jt]sx?|py|rb|go|rs|java|kt|c|h|cpp|cs|php|sh|ps1|sql|log|ini)$/i;

function readAs(file: File, how: 'text' | 'dataUrl'): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    if (how === 'text') reader.readAsText(file);
    else reader.readAsDataURL(file);
  });
}

// Images go to the model as images, small text files inside the message, and anything else as a file the model opens.
async function attach(list: Iterable<File>, draft: Draft): Promise<{ draft: Draft; names: string[] }> {
  const names: string[] = [];
  let { text, images, files } = draft;
  for (const file of list) {
    if (file.type.startsWith('image/')) {
      const url = await readAs(file, 'dataUrl');
      images = [...images, { mediaType: file.type, base64: url.slice(url.indexOf(',') + 1) }];
    } else if ((file.type.startsWith('text/') || TEXT.test(file.name)) && file.size <= INLINE_TEXT) {
      text = `${text}${text ? '\n\n' : ''}${t('composer.attachedFile', { name: file.name })}\n\`\`\`\n${await readAs(file, 'text')}\n\`\`\``;
    } else {
      const url = await readAs(file, 'dataUrl');
      files = [...files, { name: file.name, base64: url.slice(url.indexOf(',') + 1) }];
    }
    names.push(file.name);
  }
  return { draft: { text, images, files }, names };
}

/** What Stop asks while helpers are at work: how many it ends too, "Stop all", and "Keep working", focused, which Escape
 * picks too. The composer's Stop and the plan-limit card's ask the same way. When it goes, however it goes, focus that was in
 * it lands on `back` instead of the page. */
export function StopAsk(props: { helpers: number; onStop: () => void; onKeep: () => void; back?: () => HTMLElement | undefined }) {
  let root: HTMLDivElement | undefined;
  onCleanup(() => {
    const had = !!root?.contains(document.activeElement);
    queueMicrotask(() => {
      if (had || !document.activeElement || document.activeElement === document.body) props.back?.()?.focus();
    });
  });
  return (
    <div ref={(el) => (root = el)} class="stop-ask" role="group" aria-label={t('composer.stop')} onKeyDown={(e) => e.key === 'Escape' && props.onKeep()}>
      <span role="status">{t('composer.stopHelpers', { n: props.helpers })}</span>
      <button type="button" class="send stop" onClick={() => props.onStop()}>{t('composer.stopAll')}</button>
      <button type="button" class="send" ref={(el) => queueMicrotask(() => el.focus())} onClick={() => props.onKeep()}>{t('composer.keepWorking')}</button>
    </div>
  );
}

export function Composer(props: Props) {
  const [draft, setDraft] = createSignal<Draft>({ ...EMPTY, text: props.draftKey ? kept(props.draftKey) : '' });
  const [names, setNames] = createSignal<string[]>([]);
  const [focused, setFocused] = createSignal(false);
  let warmed = false;
  let pause: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(pause));
  let fileInput: HTMLInputElement | undefined;
  let field: HTMLTextAreaElement | undefined;
  // The field is measured at most once per frame (16 ms): a change that finds the last measure older than that measures at
  // once (no frame is drawn at the old height), and the ones that follow within it (a paste, a key held down) share one
  // measure when the frame ends. A timer, not requestAnimationFrame: a window that is not shown draws no frames.
  const FRAME_MS = 16;
  let measuredAt = -FRAME_MS;
  let later: ReturnType<typeof setTimeout> | undefined;
  const resize = () => {
    if (later !== undefined) return;
    const wait = measuredAt + FRAME_MS - performance.now();
    if (wait > 0) {
      later = setTimeout(() => {
        later = undefined;
        resize();
      }, wait);
      return;
    }
    measuredAt = performance.now();
    resizeComposer(field, draft().text);
  };
  onCleanup(() => clearTimeout(later));
  createEffect(on(() => draft().text, () => queueMicrotask(resize), { defer: true }));
  onMount(() => {
    let width = 0;
    const observer = new ResizeObserver(([entry]) => {
      if (entry.contentRect.width === width) return;
      const first = width === 0;
      width = entry.contentRect.width;
      if (!first) resize();
    });
    observer.observe(field!);
    window.addEventListener('resize', resize);
    onCleanup(() => {
      observer.disconnect();
      window.removeEventListener('resize', resize);
    });
  });
  const selected = () => Math.max(0, props.options.findIndex((o) => o.choice.vendor === props.choice.vendor && o.choice.model === props.choice.model));
  const hasContent = () => draft().text.trim() !== '' || draft().images.length > 0 || draft().files.length > 0;
  const canSend = () => !props.locked && hasContent() && props.vendors.length > 0;
  useDraft(hasContent);
  createEffect(() => props.draftKey && keep(props.draftKey, draft().text));
  onCleanup(() => props.draftKey && release(props.draftKey));

  // The first keystroke starts the CLI, not focus: the box is focused as soon as Faber opens, and an unused Claude
  // process costs about 120 MB.
  const warm = () => {
    if (!warmed && !props.locked) {
      warmed = true;
      props.onWarm();
    }
  };
  const send = () => {
    if (!canSend()) return;
    clearTimeout(pause);
    props.onSend(draft());
    setDraft(EMPTY);
    setNames([]);
    warmed = false;
  };
  props.onReady?.((text) => {
    setDraft((d) => ({ ...d, text }));
    warm();
    field?.focus();
    field?.setSelectionRange(text.length, text.length);
  });
  props.onRestore?.((saved) => {
    if (hasContent() || props.locked) return false;
    setDraft(saved); setNames(saved.files.map((f) => f.name)); warmed = false;
    field?.focus(); return true;
  });
  props.onClearRestored?.((saved) => {
    if (draft() !== saved) return false;
    setDraft(EMPTY); setNames([]); warmed = false; return true;
  });
  const add = async (list: Iterable<File>) => {
    const result = await attach(list, draft());
    setDraft(result.draft);
    setNames((n) => [...n, ...result.names]);
  };

  // Stop with helpers at work says what it ends and offers to let them work; it closes by itself once they are done.
  const [stopping, setStopping] = createSignal(false);
  let stopButton: HTMLButtonElement | undefined;
  createEffect(() => {
    if (!props.busy || !props.helpers) setStopping(false);
  });

  return (
    <form
      class="composer"
      data-tour="composer"
      onSubmit={(e) => {
        e.preventDefault();
        send();
      }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        if (e.dataTransfer) void add(e.dataTransfer.files);
      }}
    >
      <Show when={names().length > 0}>
        <div class="chips">
          <For each={names()}>{(name) => <span class="chip">{name}</span>}</For>
          <span class="chip">
            <button type="button" onClick={() => (setDraft((d) => ({ ...d, images: [], files: [] })), setNames([]))} aria-label={t('composer.removeAttachments')} title={t('composer.removeAttachments')}>
              <Icon name="close" small />
            </button>
          </span>
        </div>
      </Show>
      <textarea
        aria-label={t('composer.message')}
        rows={1}
        placeholder={props.placeholder}
        value={draft().text}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onInput={(e) => {
          const text = e.currentTarget.value;
          setDraft((d) => ({ ...d, text }));
          warm();
          clearTimeout(pause);
          if (text.trim()) pause = setTimeout(() => props.onPause(text), 300);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
            e.preventDefault();
            send();
          }
        }}
        onPaste={(e) => {
          const images = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith('image/'));
          if (images.length) {
            e.preventDefault();
            void add(images);
          }
        }}
        autofocus
        ref={(el) => {
          field = el;
          queueMicrotask(() => el.focus());
        }}
      />
      <Show when={props.hints && !hasContent()}>
        <Ghost hints={props.hints!} focused={focused()} />
      </Show>
      <div class="cbar">
        <button type="button" class="iconbtn" aria-label={t('composer.attach')} title={t('composer.attach')} onClick={() => fileInput?.click()}>
          <Icon name="clip" />
        </button>
        <input ref={(el) => (fileInput = el)} type="file" multiple hidden onChange={(e) => e.currentTarget.files && void add(e.currentTarget.files)} />
        {/* The model: a chip with the rays of "let the AI choose" and a chevron, over the real select that does the work. */}
        <label class="chip choose" data-tour="auto">
          <Icon name="auto" small />
          <select
            aria-label={t('composer.model')}
            value={selected()}
            disabled={props.locked}
            onChange={(e) => {
              const choice = props.options[Number(e.currentTarget.value)]!.choice;
              // A level means different work on each model, so a new model starts again at the default.
              const same = choice.vendor === props.choice.vendor && choice.model === props.choice.model;
              props.onChoice(choice.vendor === 'auto' ? choice : { ...choice, effort: same ? (props.choice.effort ?? DEFAULT_EFFORT) : DEFAULT_EFFORT });
              warmed = false; // the next keystroke warms the newly chosen model
            }}
          >
            <For each={props.options}>
              {(o, i) => (
                <option value={i()} selected={i() === selected()}>
                  {o.label}
                </option>
              )}
            </For>
          </select>
          <Icon name="down" small />
        </label>
        <Show when={props.choice.vendor !== 'auto' && props.choice.model}>
          <label class="chip choose effort">
            <span>{t('effort.label')}</span>
            <select aria-label={t('effort.label')} value={props.choice.effort ?? DEFAULT_EFFORT} disabled={props.locked} onChange={(e) => {
              props.onChoice({ ...props.choice, effort: e.currentTarget.value as Effort });
              warmed = false;
            }}>
              <For each={['low', 'medium', 'high'] as const}>{(effort) => <option value={effort}>{t(`effort.${effort}`)}</option>}</For>
            </select>
            <Icon name="down" small />
          </label>
        </Show>
        <Show when={props.mode && props.onMode}>
          <label class="chip choose mode" title={t(`set.permissions.${props.mode!}.line`)}>
            <select aria-label={t('composer.mode')} value={props.mode} onChange={(e) => props.onMode!(e.currentTarget.value as PermissionMode)}>
              <For each={PERMISSION_MODES}>{(m) => <option value={m} selected={m === props.mode} title={t(`set.permissions.${m}.line`)}>{t(`set.permissions.${m}`)}</option>}</For>
            </select>
            <Icon name="down" small />
          </label>
        </Show>
        <Show when={props.keepGoing !== undefined && props.onKeepGoing}>
          <button
            type="button"
            class="chip choose keep"
            aria-pressed={props.keepGoing}
            title={t(props.keepGoing ? 'composer.keepGoing.on' : 'composer.keepGoing.line')}
            onClick={() => props.onKeepGoing!(!props.keepGoing)}
          >
            <Icon name="loop" small />
            <span>{t('composer.keepGoing')}</span>
          </button>
        </Show>
        <Show when={props.busy && stopping()}>
          <StopAsk helpers={props.helpers ?? 0} onStop={() => {
            setStopping(false);
            props.onStop();
          }} onKeep={() => setStopping(false)} back={() => (stopButton?.isConnected ? stopButton : field)} />
        </Show>
        <Show when={props.busy && !stopping()}>
          <button ref={(el) => (stopButton = el)} type="button" class="send stop" onClick={() => (props.helpers ? setStopping(true) : props.onStop())} aria-label={t('composer.stop')}>
            <svg class="i sm" viewBox="0 0 24 24" aria-hidden="true" style={{ fill: 'currentColor', stroke: 'none' }}>
              <rect width="12" height="12" x="6" y="6" rx="2" />
            </svg>
            <span aria-hidden="true">{t('composer.stop')}</span>
          </button>
        </Show>
        <Show when={!props.busy || hasContent()}>
          <button type="submit" class="send" disabled={!canSend()} aria-label={t('composer.send')}>
            <span aria-hidden="true">{t('composer.send')}</span>
            <Icon name="up" small />
          </button>
        </Show>
      </div>
    </form>
  );
}
