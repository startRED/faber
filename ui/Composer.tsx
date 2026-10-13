import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import type { Choice, Effort, Vendor } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Option } from './models.ts';
import { resizeComposer } from './composer-size.ts';

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
  busy: boolean;
  nextMessage?: boolean;
  /** A conversation is still opening: nothing can be sent or warmed until it is in. */
  locked?: boolean;
  placeholder: string;
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
      text = `${text}${text ? '\n\n' : ''}Attached file ${file.name}:\n\`\`\`\n${await readAs(file, 'text')}\n\`\`\``;
    } else {
      const url = await readAs(file, 'dataUrl');
      files = [...files, { name: file.name, base64: url.slice(url.indexOf(',') + 1) }];
    }
    names.push(file.name);
  }
  return { draft: { text, images, files }, names };
}

export function Composer(props: Props) {
  const [draft, setDraft] = createSignal<Draft>(EMPTY);
  const [names, setNames] = createSignal<string[]>([]);
  let warmed = false;
  let pause: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(pause));
  let fileInput: HTMLInputElement | undefined;
  let field: HTMLTextAreaElement | undefined;
  const resize = () => resizeComposer(field, draft().text);
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
  const canSend = () => !props.busy && !props.locked && hasContent() && props.vendors.length > 0;

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
            <button type="button" onClick={() => (setDraft((d) => ({ ...d, images: [], files: [] })), setNames([]))} aria-label={t('composer.removeAttachments')}>
              {t('composer.clear')}
            </button>
          </span>
        </div>
      </Show>
      <textarea
        aria-label={t('composer.message')}
        rows={1}
        placeholder={props.placeholder}
        value={draft().text}
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
              props.onChoice(choice.vendor === 'auto' ? choice : { ...choice, effort: props.choice.effort ?? 'medium' });
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
            <select aria-label={t('effort.label')} value={props.choice.effort ?? 'medium'} disabled={props.locked} onChange={(e) => {
              props.onChoice({ ...props.choice, effort: e.currentTarget.value as Effort });
              warmed = false;
            }}>
              <For each={['low', 'medium', 'high'] as const}>{(effort) => <option value={effort}>{t(`effort.${effort}`)}</option>}</For>
            </select>
            <Icon name="down" small />
          </label>
        </Show>
        <Show
          when={props.busy}
          fallback={
            <button type="submit" class="send" disabled={!canSend()} aria-label={t('composer.send')}>
              <span aria-hidden="true">{t('composer.send')}</span>
              <Icon name="up" small />
            </button>
          }
        >
          <button type="button" class="send stop" onClick={() => props.onStop()} aria-label={t('composer.stop')}>
            <svg class="i sm" viewBox="0 0 24 24" aria-hidden="true" style={{ fill: 'currentColor', stroke: 'none' }}>
              <rect width="12" height="12" x="6" y="6" rx="2" />
            </svg>
            <span aria-hidden="true">{t('composer.stop')}</span>
          </button>
        </Show>
      </div>
      <Show when={props.nextMessage && props.choice.vendor !== 'auto' && props.choice.model}>
        <p class="effort-hint">{t('effort.nextMessage')}</p>
      </Show>
    </form>
  );
}
