import { readAs } from './attachments.ts';
import { batch, createEffect, createMemo, createSignal, createUniqueId, For, on, onCleanup, onMount, Show } from 'solid-js';
import { DEFAULT_EFFORT, type Choice, type Command, type Effort, type Mention, PERMISSION_MODES, type PermissionMode, type Vendor } from './api.ts';
import { Ghost } from './Ghost.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { type Option, vendorName } from './models.ts';
import { caretOnFirstLine, resizeComposer } from './composer-size.ts';
import { animateList, keepForExit } from './motion.ts';
import { Thumb } from './Thumbs.tsx';
import { appendAttachments, attachmentDiscardVersion, attachmentSaveFailed, hasKeptAttachments, keep, keepAttachments, kept, keptAttachments, release, retainAttachments, setDirty, useDraft, watchAttachments, type DraftAttachments } from './unsent.ts';
import { pressed } from './keys.ts';

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
  /** The helper the message will be about (task 130), named above the empty field; the screen adds its words on send. */
  about?: string;
  onDropAbout?: () => void;
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
  /** `@` lists the working folder's entries that match what follows it (parity T4); absent where there is no folder yet. */
  onMention?: (query: string) => Promise<Mention[]>;
  /** `/` lists this conversation's agent commands, or the chosen agents' last reported lists for a new conversation. */
  onCommands?: () => Promise<Command[]>;
  /** Earlier messages in this area (and this project), read only during this box's focus. */
  onHistory?: (signal: AbortSignal) => Promise<string[]>;
  /** An announcement from the engine invalidates an open or pending `/` query. */
  commandsVersion?: number;
  /** The open conversation offers Faber's `/btw` and `/recap` before the agent's commands. */
  canAside?: boolean;
  onRecap?: () => void;
  /** Web search is enabled; other capability limits are known only to the CLI. */
  canResearch?: boolean;
}

type Completion = { path: string; dir: boolean; command?: Command; aside?: 'btw' | 'recap'; research?: boolean; history?: boolean };
type Trigger = { at: number; query: string; kind: 'mention' | 'command' | 'history' };

/** The `@` being typed at the caret: where it starts and what follows it, or undefined. A path with a space is written
 * `@"My folder/…"`, so a quote opens one that spaces do not end. */
export function mentionAt(text: string, caret: number): { at: number; query: string } | undefined {
  const m = /(^|\s)@(?:"([^"]*)|([^\s@"]*))$/.exec(text.slice(0, caret));
  return m ? { at: m.index + m[1]!.length, query: m[2] ?? m[3]! } : undefined;
}

/** How an entry is written into the message, so `mentionAt` reads it back: quoted when it has a space or an `@` (the engine
 * never offers a name with a double quote); a folder stays open so its entries show next. */
export function mentionText(path: string, dir: boolean): string {
  if (!/[\s@]/.test(path)) return `@${path}${dir ? '' : ' '}`;
  return dir ? `@"${path}` : `@"${path}" `;
}

// Small text files go inside the message (no tool call needed); anything else is attached as a file.
const INLINE_TEXT = 50_000;
const TEXT = /\.(txt|md|csv|json|ya?ml|toml|xml|html?|css|[cm]?[jt]sx?|py|rb|go|rs|java|kt|c|h|cpp|cs|php|sh|ps1|sql|log|ini)$/i;

// A small text file written into the message: its name, and the exact words it added, so taking it away takes those words out.
interface Inlined {
  name: string;
  block: string;
}
// The name a picture came with. A draft keeps only the media type and the data, so the name waits here, by the picture itself,
// and is still found when a rejected message puts its draft back.
const called = new WeakMap<object, string>();
// The small text files a draft had when it was sent, kept by the draft itself: a message the engine rejects comes back with them.
const sentWith = new WeakMap<object, Inlined[]>();

// Images go to the model as images, small text files inside the message, and anything else as a file the model opens.
async function attach(list: Iterable<File>, draft: Draft): Promise<{ draft: Draft; inlined: Inlined[] }> {
  const inlined: Inlined[] = [];
  let { text, images, files } = draft;
  for (const file of list) {
    if (file.type.startsWith('image/')) {
      const url = await readAs(file, 'dataUrl');
      const image = { mediaType: file.type, base64: url.slice(url.indexOf(',') + 1) };
      called.set(image, file.name);
      images = [...images, image];
    } else if ((file.type.startsWith('text/') || TEXT.test(file.name)) && file.size <= INLINE_TEXT) {
      const block = `${text ? '\n\n' : ''}${t('composer.attachedFile', { name: file.name })}\n\`\`\`\n${await readAs(file, 'text')}\n\`\`\``;
      text += block;
      inlined.push({ name: file.name, block });
    } else {
      const url = await readAs(file, 'dataUrl');
      files = [...files, { name: file.name, base64: url.slice(url.indexOf(',') + 1) }];
    }
  }
  return { draft: { text, images, files }, inlined };
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

/** The conversation's mode picker: every surface uses the same choices and their one-line meanings. */
export function PermissionChip(props: { mode: PermissionMode; onMode: (mode: PermissionMode) => void; modes?: readonly PermissionMode[]; caption?: string; disabled?: boolean }) {
  return (
    <label class="chip choose mode" title={t(`set.permissions.${props.mode}.line`)}>
      <Show when={props.caption}><span>{props.caption}</span></Show>
      <select aria-label={props.caption ?? t('composer.mode')} aria-description={t(`set.permissions.${props.mode}.line`)} value={props.mode} disabled={props.disabled} onChange={(e) => props.onMode(e.currentTarget.value as PermissionMode)}>
        <For each={props.modes ?? PERMISSION_MODES}>{(m) => <option value={m} selected={m === props.mode} title={t(`set.permissions.${m}.line`)}>{t(`set.permissions.${m}`)}</option>}</For>
      </select>
      <Icon name="down" small />
    </label>
  );
}

export function Composer(props: Props) {
  const [draft, setDraft] = createSignal<Draft>({ ...EMPTY, text: props.draftKey ? kept(props.draftKey) : '' });
  const [inlined, setInlined] = createSignal<Inlined[]>([]);
  const [attachmentsReady, setAttachmentsReady] = createSignal(true);
  const [attachmentLoadFailed, setAttachmentLoadFailed] = createSignal(false);
  const draftNoticeId = createUniqueId();
  const attachmentError = () => attachmentLoadFailed() || !!(props.draftKey && attachmentSaveFailed(props.draftKey));
  let alive = true;
  const attachmentSource = {};
  let activeKey = props.draftKey;
  let attachmentTurn = 0;
  let additions = 0;
  let hydration: Promise<boolean> = Promise.resolve(true);
  let savedImages = draft().images;
  let savedFiles = draft().files;
  let savedInline = inlined();
  const attachmentSnapshot = (d: Draft, inline: Inlined[]): DraftAttachments => ({
    images: d.images.map((image) => ({ ...image, name: called.get(image) })), files: d.files, inlined: inline,
  });
  const applyAttachments = (saved: DraftAttachments, text?: string) => {
    const images = saved.images.map(({ name, mediaType, base64 }) => {
      const image = { mediaType, base64 };
      if (name) called.set(image, name);
      return image;
    });
    const words = text ?? (historyMode() ? beforeHistory.text : draft().text);
    const inline = saved.inlined.filter((item) => words.includes(item.block));
    savedImages = images; savedFiles = saved.files; savedInline = inline;
    batch(() => {
      if (text !== undefined && historyMode()) leaveHistory(false);
      setDraft((current) => ({ ...current, text: text ?? current.text, images, files: saved.files }));
      setInlined(inline);
      setAttachmentsReady(true);
      setAttachmentLoadFailed(false);
    });
  };
  createEffect(on(() => props.draftKey, (key, previous) => {
    const turn = ++attachmentTurn;
    activeKey = key;
    setAttachmentsReady(false);
    if (previous !== undefined) {
      additions++;
      setDraft({ ...EMPTY, text: key ? kept(key) : '' });
      setInlined([]);
    }
    savedImages = draft().images; savedFiles = draft().files; savedInline = inlined();
    setAttachmentLoadFailed(false);
    if (key) onCleanup(watchAttachments(key, (value, change) => {
      if (change.source === attachmentSource || !alive) return;
      attachmentTurn++;
      applyAttachments(value, change.text);
      hydration = Promise.resolve(true);
    }));
    const needed = !!key && hasKeptAttachments(key);
    setAttachmentsReady(!needed);
    hydration = needed ? keptAttachments(key!).then((saved) => {
      if (!alive || turn !== attachmentTurn) return true;
      if (saved) {
        applyAttachments(saved);
        if (saved.inlined.length !== inlined().length) {
          void keepAttachments(key!, attachmentSnapshot(draft(), inlined()), { source: attachmentSource, preserveReads: true });
        }
      }
      savedImages = draft().images; savedFiles = draft().files; savedInline = inlined();
      setAttachmentsReady(true);
      return true;
    }).catch(() => {
      if (alive && turn === attachmentTurn) setAttachmentLoadFailed(true);
      return false;
    }) : Promise.resolve(true);
  }));
  // Array identity changes only on attachment edits: typing never serializes bytes or writes IndexedDB.
  createEffect(() => {
    const d = draft(); const inline = inlined();
    if (!attachmentsReady() || !activeKey) return;
    if (d.images === savedImages && d.files === savedFiles && inline === savedInline) return;
    savedImages = d.images; savedFiles = d.files; savedInline = inline;
    void keepAttachments(activeKey, attachmentSnapshot(d, inline), { source: attachmentSource });
  });
  onCleanup(() => { alive = false; field = undefined; fileInput = undefined; });
  const [focused, setFocused] = createSignal(false);
  const [historyMode, setHistoryMode] = createSignal<'browse' | 'search'>();
  const [historyError, setHistoryError] = createSignal(false);
  const [recalling, setRecalling] = createSignal(false);
  let history: Promise<string[]> | undefined;
  let historyRead: AbortController | undefined;
  let historyIndex = -1;
  let historyTurn = 0;
  let historyFocus = 0;
  let beforeHistory = { text: '', start: 0, end: 0 };
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
  const canSend = () => attachmentsReady() && !props.locked && !recalling() && historyMode() !== 'search' && hasContent() && props.vendors.length > 0;
  useDraft(hasContent);
  createEffect(() => props.draftKey && keep(props.draftKey, historyMode() ? beforeHistory.text : draft().text));
  onCleanup(() => props.draftKey && release(props.draftKey));

  // The first keystroke starts the CLI, not focus: the box is focused as soon as Faber opens, and an unused Claude
  // process costs about 120 MB.
  const warm = () => {
    if (!warmed && !props.locked) {
      warmed = true;
      props.onWarm();
    }
  };
  // `@` and `/` share the list over the box and every interaction. The engine is asked once typing
  // pauses briefly; until its answer for the current query comes, the list on screen is an older one and cannot be picked.
  const [mentions, setMentions] = createSignal<Completion[]>([]);
  const mixedCommands = createMemo(() => new Set(mentions().flatMap((m) => m.command ? [m.command.vendor] : [])).size > 1);
  const [active, setActive] = createSignal(0);
  const [pending, setPending] = createSignal(false);
  const [kind, setKind] = createSignal<Trigger['kind']>('mention');
  // An answer that takes long shows a waiting line, so a held Enter is never a box that seems not to answer.
  const [slow, setSlow] = createSignal(false);
  let slowing: ReturnType<typeof setTimeout> | undefined;
  let mention: Trigger | undefined;
  const triggerAt = (el: HTMLTextAreaElement): Trigger | undefined => {
    if (historyMode() === 'search') return { at: 0, query: el.value, kind: 'history' };
    const caret = el.selectionStart ?? el.value.length;
    if (el.selectionEnd !== caret) return;
    if ((props.onCommands || props.canAside) && caret === el.value.length && /^\/\S*$/.test(el.value)) {
      return { at: 0, query: el.value.slice(1), kind: 'command' };
    }
    const found = props.onMention ? mentionAt(el.value, caret) : undefined;
    return found ? { ...found, kind: 'mention' } : undefined;
  };
  // The entry the arrows reach stays in view in a long list.
  createEffect(on(active, (i) => document.getElementById(`mention-${i}`)?.scrollIntoView({ block: 'nearest' }), { defer: true }));
  let asked = 0;
  let asking: ReturnType<typeof setTimeout> | undefined;
  const closeMentions = () => {
    asked++;
    clearTimeout(asking);
    clearTimeout(slowing);
    mention = undefined;
    setPending(false);
    setSlow(false);
    setMentions([]);
  };
  onCleanup(closeMentions);
  const loadHistory = () => {
    if (history) return history;
    const focus = historyFocus;
    historyRead = new AbortController();
    return history = props.onHistory!(historyRead.signal).catch(() => {
      if (focus === historyFocus && focused()) setHistoryError(true);
      return [];
    });
  };
  const cancelHistoryRead = () => {
    historyFocus++;
    historyRead?.abort();
    historyRead = undefined;
    history = undefined;
  };
  const putHistoryText = (text: string, start = text.length, end = start) => {
    setDraft((d) => ({ ...d, text }));
    field?.setSelectionRange(start, end);
  };
  const rememberDraft = () => {
    if (historyMode()) return;
    beforeHistory = { text: draft().text, start: field?.selectionStart ?? 0, end: field?.selectionEnd ?? 0 };
    historyIndex = -1;
    clearTimeout(pause);
  };
  const leaveHistory = (restore: boolean) => {
    historyTurn++;
    if (recalling() || (historyMode() === 'search' && pending())) cancelHistoryRead();
    if (restore && historyMode()) putHistoryText(beforeHistory.text, beforeHistory.start, beforeHistory.end);
    setHistoryMode(undefined);
    setRecalling(false);
    historyIndex = -1;
    closeMentions();
  };
  // Content edits commit the visible message; only navigation may restore the snapshot from before history.
  const editDraft = (change: (current: Draft) => Draft) => {
    if (historyMode()) leaveHistory(historyMode() === 'search');
    setDraft(change);
  };
  const fillPiece = (event: Event) => {
    const detail = (event as CustomEvent<{ conversationId: string; text: string }>).detail;
    if (!detail || detail.conversationId !== props.draftKey || typeof detail.text !== 'string') return;
    editDraft((current) => ({ ...current, text: current.text ? `${current.text}\n\n${detail.text}` : detail.text }));
    field?.focus();
  };
  document.addEventListener('faber:piece-fill', fillPiece);
  onCleanup(() => document.removeEventListener('faber:piece-fill', fillPiece));
  const keepEditingFocus = (event: MouseEvent) => {
    if (event.button === 0 && historyMode()) event.preventDefault();
  };
  onCleanup(() => { historyTurn++; cancelHistoryRead(); });
  const browseHistory = (move: number) => {
    rememberDraft();
    setHistoryMode('browse');
    setRecalling(true);
    historyIndex += move;
    const ticket = ++historyTurn;
    void loadHistory().then((list) => {
      if (ticket !== historyTurn || !focused() || historyMode() !== 'browse') return;
      setRecalling(false);
      historyIndex = Math.min(list.length - 1, Math.max(-1, historyIndex));
      if (historyIndex < 0) leaveHistory(true);
      else putHistoryText(list[historyIndex]!);
    });
  };
  const lookForMention = (el: HTMLTextAreaElement) => {
    const found = triggerAt(el);
    if (!found) return closeMentions();
    if (found.at === mention?.at && found.query === mention.query && found.kind === mention.kind) return;
    if (found.kind !== mention?.kind) setMentions([]);
    mention = found;
    setKind(found.kind);
    const ticket = ++asked;
    const ask = props.onMention;
    const commands = props.onCommands;
    const canAside = props.canAside;
    setPending(true);
    clearTimeout(asking);
    clearTimeout(slowing);
    slowing = setTimeout(() => ticket === asked && setSlow(true), 300);
    const answered = (list: Completion[]) => {
      if (ticket !== asked) return;
      clearTimeout(slowing);
      setPending(false);
      setSlow(false);
      setMentions(list);
      setActive(0);
    };
    asking = setTimeout(() => {
      const query = found.query.toLowerCase();
      const request = found.kind === 'history'
        ? loadHistory().then((list) => list.filter((text) => text.toLowerCase().includes(query)).map((path) => ({ path, dir: false, history: true })))
        : found.kind === 'mention' ? ask!(found.query) : (commands?.() ?? Promise.resolve([])).catch(() => []).then((commands) => {
        const list: Completion[] = commands
          .filter((c) => props.vendors.includes(c.vendor) && c.name.toLowerCase().includes(query)
            && !(canAside && ['btw', 'recap'].includes(c.name.toLowerCase())))
          .sort((a, b) => Number(b.name.toLowerCase().startsWith(query)) - Number(a.name.toLowerCase().startsWith(query)))
          .map((command) => ({ path: `${command.vendor === 'claude' ? '/' : '$'}${command.name}`, dir: false, command }));
        if (props.canResearch && 'research'.includes(query)) list.unshift({ path: '/research', dir: false, research: true });
        if (canAside) {
          if ('recap'.includes(query)) list.unshift({ path: '/recap', dir: false, aside: 'recap' });
          if ('btw'.includes(query)) list.unshift({ path: '/btw', dir: false, aside: 'btw' });
        }
        return list.slice(0, 50);
      });
      void request.then(answered, () => answered([]));
    }, 60);
  };
  // Refresh only when these values change, not when their callers reread a streaming conversation or a model choice.
  const researchAvailable = createMemo(() => !!props.canResearch);
  const asideAvailable = createMemo(() => !!props.canAside);
  const commandVendor = createMemo(() => props.choice.vendor);
  const commandsVersion = createMemo(() => props.commandsVersion);
  createEffect(on(() => [asideAvailable(), researchAvailable(), commandVendor(), commandsVersion()], () => {
    if (mention?.kind !== 'command' || !field) return;
    closeMentions();
    lookForMention(field);
  }, { defer: true }));
  // The path takes the place of the `@` and what followed it, as a typed edit (undo takes it back). A file ends with a space;
  // a folder stays open, so its own entries show next. Nothing is put in unless the caret still ends that same `@`.
  const pickMention = (m: Completion) => {
    if (!field || !mention || pending()) return;
    if (m.history) {
      const ticket = ++historyTurn;
      setHistoryMode('browse');
      void loadHistory().then((list) => { if (ticket === historyTurn) historyIndex = list.indexOf(m.path); });
      putHistoryText(m.path);
      closeMentions();
      return;
    }
    if (m.aside && !props.canAside) return closeMentions();
    const now = triggerAt(field);
    if (now?.at !== mention.at || now.query !== mention.query || now.kind !== mention.kind) return closeMentions();
    if (m.aside === 'recap') {
      const text = field.value.slice(0, mention.at) + field.value.slice(field.selectionStart);
      closeMentions();
      editDraft((d) => ({ ...d, text }));
      props.onRecap?.();
      return;
    }
    const at = mention.at;
    if (m.command && props.choice.vendor === 'auto') {
      const choice = props.options.find((o) => o.choice.vendor === m.command!.vendor)?.choice;
      props.onChoice({ ...(choice ?? { vendor: m.command.vendor }), effort: DEFAULT_EFFORT });
      warmed = false;
    }
    field.focus();
    field.setSelectionRange(at, field.selectionStart ?? field.value.length);
    const words = m.command || m.aside || m.research ? `${m.path} ` : mentionText(m.path, m.dir);
    if (!document.execCommand('insertText', false, words)) {
      const text = field.value;
      field.value = text.slice(0, field.selectionStart) + words + text.slice(field.selectionEnd);
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    }
  };
  const send = () => {
    if (!canSend() || historyMode() === 'search') return;
    leaveHistory(false);
    cancelHistoryRead();
    closeMentions();
    clearTimeout(pause);
    additions++;
    const sending = draft();
    sentWith.set(sending, inlined());
    props.onSend(sending);
    batch(() => { setDraft(EMPTY); setInlined([]); });
    warmed = false;
  };
  props.onReady?.((text) => {
    editDraft((d) => ({ ...d, text }));
    warm();
    field?.focus();
    field?.setSelectionRange(text.length, text.length);
  });
  props.onRestore?.((saved) => {
    if (!attachmentsReady() || hasContent() || props.locked) return false;
    batch(() => { editDraft(() => saved); setInlined(sentWith.get(saved) ?? []); }); warmed = false;
    field?.focus(); return true;
  });
  props.onClearRestored?.((saved) => {
    if (draft() !== saved) return false;
    batch(() => { editDraft(() => EMPTY); setInlined([]); }); warmed = false; return true;
  });
  const add = async (list: Iterable<File>) => {
    const files = [...list];
    if (!files.length) return;
    // Escape or a late recall must not undo this edit while FileReader is still at work.
    if (historyMode()) leaveHistory(historyMode() === 'search');
    const key = activeKey;
    const generation = additions;
    const discarded = key ? attachmentDiscardVersion(key) : 0;
    const reading = {};
    const releaseRead = key ? retainAttachments(key) : () => {};
    setDirty(reading, true);
    try {
      if (!await hydration) return;
      const result = await attach(files, EMPTY);
      if (generation !== additions || (key && attachmentDiscardVersion(key) !== discarded)) return;
      if (!alive || key !== activeKey) {
        if (!key) return;
        await appendAttachments(key, { ...attachmentSnapshot(result.draft, result.inlined), text: result.draft.text }, discarded);
        return;
      }
      let added = result.inlined;
      batch(() => {
        editDraft((d) => {
          const gap = d.text ? '\n\n' : '';
          // The first small text file starts the added words, so the gap before them belongs to it: taking it away takes the gap too.
          added = result.inlined.map((item, i) => (i === 0 ? { ...item, block: gap + item.block } : item));
          return {
            text: result.draft.text ? `${d.text}${gap}${result.draft.text}` : d.text,
            images: [...d.images, ...result.draft.images],
            files: [...d.files, ...result.draft.files],
          };
        });
        setInlined((all) => [...all, ...added]);
      });
    } finally { releaseRead(); setDirty(reading, false); }
  };
  // The attachments, each its own. A memo per kind, so typing (which makes a new draft) never builds the thumbnails again.
  const images = createMemo(() => draft().images);
  const files = createMemo(() => draft().files);
  const count = () => images().length + files().length + inlined().length;
  const shown = keepForExit(() => count() > 0);
  const dropImage = (image: Draft['images'][number]) => editDraft((d) => ({ ...d, images: d.images.filter((i) => i !== image) }));
  const dropFile = (file: Draft['files'][number]) => editDraft((d) => ({ ...d, files: d.files.filter((f) => f !== file) }));
  const dropText = (item: Inlined) => batch(() => {
    setInlined((all) => all.filter((i) => i !== item));
    editDraft((d) => ({ ...d, text: d.text.replace(item.block, '') }));
  });
  const dropAll = () => batch(() => {
    additions++;
    editDraft((d) => ({ ...d, images: [], files: [], text: inlined().reduce((text, i) => text.replace(i.block, ''), d.text) }));
    setInlined([]);
  });

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
      aria-busy={!attachmentsReady() && !attachmentLoadFailed()}
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
      <Show when={attachmentError()}>
        <p class="hint" id={draftNoticeId} role="status">{t(attachmentLoadFailed() ? 'composer.draftLoadFailed' : 'composer.draftSaveFailed')}
          <Show when={attachmentLoadFailed()}>
            <button type="button" class="more" onClick={() => {
              additions++;
              setAttachmentLoadFailed(false);
              setAttachmentsReady(true);
              if (activeKey) void keepAttachments(activeKey, { images: [], files: [], inlined: [] }, { source: attachmentSource });
            }}>{t('composer.removeAttachments')}</button>
          </Show>
          <Show when={!attachmentLoadFailed()}>
            <button type="button" class="more" onClick={() => activeKey && void keepAttachments(activeKey, attachmentSnapshot(draft(), inlined()), { source: attachmentSource })}>{t('changes.retry')}</button>
          </Show>
        </p>
      </Show>
      {/* The row opens and closes with its height, so the box never jumps when the first file lands or the last one goes. */}
      <Show when={shown()}>
        <div class="fold attachments" classList={{ open: count() > 0 }} inert={count() === 0}>
          <div class="fold-inner">
            <div class="attach-row" ref={animateList} onMouseDown={keepEditingFocus}>
              <For each={images()}>{(image) => <Thumb name={called.get(image) ?? t('composer.image')} src={`data:${image.mediaType};base64,${image.base64}`} onRemove={() => dropImage(image)} />}</For>
              <For each={files()}>{(file) => <Thumb name={file.name} onRemove={() => dropFile(file)} />}</For>
              <For each={inlined()}>{(item) => <Thumb name={item.name} onRemove={() => dropText(item)} />}</For>
              <Show when={count() > 1}>
                <button type="button" class="more attach-all" onClick={dropAll}>
                  {t('composer.removeAttachments')}
                </button>
              </Show>
            </div>
          </div>
        </div>
      </Show>
      <Show when={props.about}>
        {(name) => (
          <p class="composer-about" id="composer-about">
            <span>{t('composer.about', { name: name() })}</span>
            <button type="button" class="iconbtn" onMouseDown={keepEditingFocus} aria-label={t('composer.aboutDrop')} title={t('composer.aboutDrop')} onClick={() => props.onDropAbout?.()}>
              <Icon name="close" small />
            </button>
          </p>
        )}
      </Show>
      <textarea
        aria-label={t('composer.message')}
        aria-describedby={[attachmentError() && draftNoticeId, props.about && 'composer-about'].filter(Boolean).join(' ') || undefined}
        rows={1}
        placeholder={historyMode() === 'search' ? t('composer.history') : props.placeholder}
        value={draft().text}
        onFocus={(e) => {
          setFocused(true);
          lookForMention(e.currentTarget);
        }}
        onBlur={() => {
          setFocused(false);
          leaveHistory(true);
          cancelHistoryRead();
          setHistoryError(false);
        }}
        aria-controls={mentions().length ? 'composer-mentions' : undefined}
        aria-activedescendant={mentions().length ? `mention-${active()}` : undefined}
        onInput={(e) => {
          const text = e.currentTarget.value;
          if (historyMode() === 'search') setDraft((d) => ({ ...d, text }));
          else editDraft((d) => ({ ...d, text }));
          lookForMention(e.currentTarget);
          if (historyMode() === 'search') return;
          warm();
          clearTimeout(pause);
          if (text.trim()) pause = setTimeout(() => props.onPause(text), 300);
        }}
        onKeyDown={(e) => {
          if (e.isComposing) return;
          if (props.onHistory && pressed(e, 'search')) {
            e.preventDefault();
            e.stopPropagation();
            rememberDraft();
            historyTurn++;
            closeMentions();
            setHistoryMode('search');
            setRecalling(false);
            putHistoryText('');
            lookForMention(e.currentTarget);
            return;
          }
          if (historyMode() && e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            leaveHistory(true);
            return;
          }
          const list = mentions();
          if (mention && (list.length || pending() || mention.kind === 'history' || e.key === 'Escape')) {
            const moves: Record<string, number> = { ArrowDown: 1, ArrowUp: -1 };
            if (e.key in moves && list.length) {
              e.preventDefault();
              setActive((i) => (i + moves[e.key]! + list.length) % list.length);
              return;
            }
            // While the answer for what was just typed is on its way, Enter neither sends nor puts in an older entry; with
            // no list on screen yet, only Enter waits.
            if ((e.key === 'Enter' || (e.key === 'Tab' && list.length)) && !e.shiftKey) {
              e.preventDefault();
              if (list.length) pickMention(list[active()]!);
              return;
            }
            if (e.key === 'Escape') {
              // The list closes; the window's own Escape (the running column, a dialog) does not hear it.
              e.preventDefault();
              e.stopPropagation();
              closeMentions();
              return;
            }
          }
          const el = e.currentTarget;
          if (props.onHistory && (!mention || (!pending() && !list.length && mention.kind !== 'history')) && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey &&
            el.selectionStart === el.selectionEnd &&
            ((e.key === 'ArrowUp' && caretOnFirstLine(el)) ||
              (e.key === 'ArrowDown' && historyMode() === 'browse'))) {
            e.preventDefault();
            closeMentions();
            browseHistory(e.key === 'ArrowUp' ? 1 : -1);
            return;
          }
          if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
            e.preventDefault();
            send();
          }
        }}
        // The caret moved away from the token (or to another one): the shared list follows it.
        onKeyUp={(e) => {
          if (/^(Arrow(Left|Right)|Home|End|Page(Up|Down))$/.test(e.key)) lookForMention(e.currentTarget);
        }}
        onClick={(e) => lookForMention(e.currentTarget)}
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
      <Show when={slow() && !mentions().length}>
        <p class="mentions waiting" role="status">{t('composer.mentionsWaiting')}</p>
      </Show>
      <Show when={historyError() || (historyMode() === 'search' && !pending() && !mentions().length)}>
        <p class="mentions waiting" role="status">{t(historyError() ? 'composer.historyFailed' : 'composer.historyEmpty')}</p>
      </Show>
      <Show when={mentions().length}>
        <ul class="mentions" id="composer-mentions" role="listbox" aria-label={t(kind() === 'history' ? 'composer.history' : kind() === 'command' ? 'composer.commands' : 'composer.mentions')} aria-busy={pending()}>
          <For each={mentions()}>
            {(m, i) => (
              <li
                id={`mention-${i()}`}
                role="option"
                aria-selected={i() === active()}
                classList={{ on: i() === active() }}
                onMouseDown={(e) => {
                  // The box keeps focus, so the list is still there when the click lands; only the main button picks.
                  if (e.button !== 0) return;
                  e.preventDefault();
                  pickMention(m);
                }}
                onMouseMove={() => setActive(i())}
              >
                <Icon name={m.command || m.aside || m.research ? 'terminal' : m.dir ? 'folder' : 'sheet'} small />
                <span>{m.path}</span>
                <Show when={m.aside || m.research || m.command?.note}><span class="mention-note">{m.aside ? t(m.aside === 'recap' ? 'aside.recap' : 'aside.title') : m.research ? t('composer.research') : m.command!.note}</span></Show>
                <Show when={m.command && mixedCommands()}>
                  <span class="mention-vendor">{t('composer.commandAgent', { name: vendorName(m.command!.vendor) })}</span>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <Show when={props.hints && !hasContent()}>
        <Ghost hints={props.hints!} focused={focused()} />
      </Show>
      <div class="cbar">
        <button type="button" class="iconbtn" onMouseDown={keepEditingFocus} aria-label={t('composer.attach')} title={t('composer.attach')} onClick={() => fileInput?.click()}>
          <Icon name="clip" />
        </button>
        <input ref={(el) => (fileInput = el)} type="file" multiple hidden onChange={(e) => {
          if (e.currentTarget.files) void add(e.currentTarget.files);
          e.currentTarget.value = '';
        }} />
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
          <PermissionChip mode={props.mode!} onMode={props.onMode!} />
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
          <button type="submit" class="send" disabled={!canSend()} aria-label={t('composer.send')}
            onMouseDown={(e) => { if (e.button === 0 && historyMode() === 'browse') e.preventDefault(); }}>
            <span aria-hidden="true">{t('composer.send')}</span>
            <Icon name="up" small />
          </button>
        </Show>
      </div>
    </form>
  );
}
