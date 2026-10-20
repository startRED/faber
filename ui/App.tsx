import { createEffect, createMemo, createResource, createSignal, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { Ambient } from './Ambient.tsx';
import { Hole } from './Hole.tsx';
import {
  api,
  errorText,
  type Appearance,
  type AppState,
  type Choice,
  type AppEvent,
  type Conversation,
  type Experience,
  type Plan,
  type Project,
  type RunningItem,
  type Settings as Saved,
  type Summary,
  type UpdateStatus,
  type Vendor,
  STOPS,
} from './api.ts';
import { inOrder, refreshed, saveAndApply, watchSystem, windowHidden } from './appearance.ts';
import { useClock } from './clock.ts';
import { Composer, type Draft } from './Composer.tsx';
import { queuedDrafts, recoverQueuedDraft } from './unsent.ts';
import { ContextMeter } from './Context.tsx';
import { Empty } from './Empty.tsx';
import { Find } from './Find.tsx';
import { LookStep } from './FirstRun.tsx';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { failureText, lang, t, templates, useLanguage } from './i18n.ts';
import { homeHints } from './idle.ts';
import { createFileDrop } from './drop.ts';
import { Grip } from './Grip.tsx';
import { autoRun, createWindowWidth, type Edge, fit, limitsOf, savedWidths, SIDE, widthClass } from './layout.ts';
import { Lockup } from './Mark.tsx';
import { isFirstInstall, newestId, NOTES, type Note, whatsNew } from './notes.ts';
import { modelOptions, vendorName } from './models.ts';
import { animateDetails, animateList, keepForExit, watchMotion } from './motion.ts';
import { Palette } from './Palette.tsx';
import { Projects } from './Projects.tsx';
import { createOpening, upsert } from './opening.ts';
import { Running } from './Running.tsx';
import { agentWide, openedAgent, showAgent, startMessage, toggleAgentWide } from './AgentChat.tsx';
import { openedSite, showSite } from './Site.tsx';
import { answered, btw, heardAsides, openedAside, showAside } from './QuickQuestion.tsx';
import { agentSteps, helpersAtWork } from './worklog.ts';
import { createAutoOpen, doneCards, openPlace, type Place, placeOfRunning, shownRunning, stopOffered, store, waitingKeys } from './running.ts';
import { type Keep, type Section, Settings } from './Settings.tsx';
import { Setup } from './Setup.tsx';
import { continuePatch, type FirstStep, firstRunStep, lookDue, setupPatch } from './setup.ts';
import { Sidebar } from './Sidebar.tsx';
import { keepRows, seedQuotas, withQuota } from './sidebar.ts';
import { Thread } from './Thread.tsx';
import { tutorialDue } from './tour.ts';
import { Tutorial } from './Tutorial.tsx';
import { Welcome } from './Welcome.tsx';
import { UpdateStage } from './UpdateStage.tsx';
import { WhatsNew } from './WhatsNew.tsx';
import { mac, WindowControls } from './WindowControls.tsx';
import { keepGoingWarning } from './keep-going.ts';
import { KeepGoingNote } from './KeepGoingNote.tsx';

// Small per-window conveniences; storage can be missing, and the app works without it.
function recall<T>(key: string, fallback: T): T {
  try {
    return (JSON.parse(localStorage.getItem(key) ?? 'null') as T | null) ?? fallback;
  } catch {
    return fallback;
  }
}
function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // per-viewer convenience only
  }
}

// A long conversation draws only its latest messages, and more as the user scrolls up, so memory stays flat.
const PAGE = 40;
const TYPING = 'textarea, input:not([type=checkbox], [type=radio], [type=range], [type=color], [type=file]), [contenteditable]';

/** `initial` is the state the page started from; `main.tsx` has already applied its look and handed the running list to
 * the store. */
export function App(props: { initial: AppState; update?: UpdateStatus; inbox: { take: (listener: ((event: AppEvent) => void) | undefined) => void } }) {
  watchMotion();
  const saved = recall<Place | undefined>('faber.place', undefined);
  useLanguage(props.initial.settings.language);
  const [state, setState] = createSignal<AppState>(props.initial);
  // The area, the project and the conversation the user is in, changed together so no screen ever shows a conversation
  // with another conversation's project.
  const [place, setPlace] = createSignal<Place>({ area: saved?.area ?? 'chat', projectId: saved?.projectId });
  const [current, setCurrent] = createSignal<Conversation>();
  const [searchIndex, setSearchIndex] = createSignal<number>();
  let searchFrame: number | undefined;
  let searchMark: HTMLElement | undefined;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const clearSearch = () => {
    if (searchFrame !== undefined) cancelAnimationFrame(searchFrame);
    clearTimeout(searchTimer);
    searchMark?.classList.remove('search-focus');
    searchMark = undefined;
    setSearchIndex(undefined);
  };
  onCleanup(clearSearch);
  const [changesOpen, setChangesOpen] = createSignal<string>();
  // A stable key avoids reading git on every streamed token; a turn ending reads the list again.
  const changesKey = createMemo(() => {
    const c = current();
    if (c?.area !== 'project') return undefined;
    const reply = c.messages.findLast((m) => m.role === 'assistant');
    return `${c.id}\0${reply?.id ?? ''}\0${reply?.status ?? ''}`;
  });
  const [changes, { mutate: setChanges }] = createResource(changesKey, (key) => api.changes(key.split('\0')[0]!).catch(() => ({ files: [], incomplete: true, next: '' })).then((list) => ({ key, ...list })));
  const [loadingMoreChanges, setLoadingMoreChanges] = createSignal(false);
  const changeList = () => {
    const list = changes();
    return list && list.key === changesKey() ? list : undefined;
  };
  const moreChanges = async () => {
    const list = changeList();
    if (!list || loadingMoreChanges()) return;
    setLoadingMoreChanges(true);
    try {
      const page = await api.changes(list.key.split('\0')[0]!, list.next);
      if (list.key !== changesKey()) return;
      const files = [...new Map([...list.files, ...page.files].map((file) => [file.path, file])).values()];
      setChanges({ ...page, files, key: list.key });
    } catch { /* Keep the partial list and its continuation available for another try. */ }
    finally { setLoadingMoreChanges(false); }
  };
  const [choice, setChoice] = createSignal<Choice>({ vendor: 'auto' });
  const [commandsVersion, setCommandsVersion] = createSignal(0);
  const commandVendor = () => {
    const conversation = current();
    const chosen = picked();
    return chosen.vendor !== 'auto' ? chosen.vendor
      : conversation?.messages.findLast((m) => m.role === 'assistant' && m.vendor)?.vendor
        ?? (conversation?.choice?.vendor !== 'auto' ? conversation?.choice?.vendor : undefined);
  };
  const [quotas, setQuotas] = createSignal(seedQuotas(props.initial.quotas));
  const clock = useClock();
  const [error, setError] = createSignal<string>();
  // The text of the last refused or failed save of settings; the page stays as it was.
  const [saveError, setSaveError] = createSignal<string>();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  // The section a link into Settings names (the quota in the sidebar names the accounts and limits), and what Settings kept while
  // the account cards were open from it: closing them brings Settings back as it was.
  const [settingsAt, setSettingsAt] = createSignal<Section>();
  const [back, setBack] = createSignal<Keep>();
  // The first run: step 1 (the accounts) or step 2 (the look) while one is due, else none. `setupOpen` is the same account
  // cards opened later, from the no-AI state or from Settings: they never reopen the first run.
  const [step, setStep] = createSignal<FirstStep | undefined>(firstRunStep(props.initial.settings, props.initial.conversations));
  const [setupOpen, setSetupOpen] = createSignal(false);
  // The control that opened the account cards, by its `data-restore`: the shell is drawn again when they close, so focus
  // goes to the new copy of that control.
  let opener: string | undefined;
  const openSetup = () => {
    const from = document.activeElement;
    opener = from instanceof HTMLElement ? from.closest<HTMLElement>('[data-restore]')?.dataset.restore : undefined;
    setSetupOpen(true);
  };
  createEffect(
    on(
      setupOpen,
      (open) => {
        const key = opener;
        if (open || !key) return;
        opener = undefined;
        setTimeout(() => document.querySelector<HTMLElement>(`[data-restore="${key}"]`)?.focus());
      },
      { defer: true },
    ),
  );
  // The tutorial ended in this window, whether or not the save of `tutorialDone` worked.
  const [tourEnded, setTourEnded] = createSignal(false);
  // "What's new": the entries newer than the last one seen, read once at start. A first install stores the newest and shows none.
  const [notes, setNotes] = createSignal<Note[]>([]);
  // The shell's view of the update: a download at open shows a calm line, a kept update a control that restarts at once.
  const [update, setUpdate] = createSignal<UpdateStatus>(props.update ?? { phase: 'none' });
  // The opening screen: a window that opens while the install at open is looking, downloading or installing shows it until
  // that ends. A window opened from the tray in the middle of a download shows the sidebar's line instead.
  const [startScreen, setStartScreen] = createSignal(props.update?.phase === 'checking');
  const heard = (status: UpdateStatus) => {
    setUpdate(status);
    if (!['checking', 'downloading', 'installing'].includes(status.phase)) setStartScreen(false);
  };
  // The choices of step 2 while the person is back at step 1, so going back and forward loses nothing.
  const [kept, setKept] = createSignal<{ appearance: Appearance; experience?: Experience; name: string }>();
  const [from, setFrom] = createSignal(0);
  const [until, setUntil] = createSignal<number>();
  // True from the moment the user picks a conversation until its snapshot is in: the screen still shows the old
  // conversation then, so nothing may be sent or warmed for either.
  const [loading, setLoading] = createSignal(false);
  const opening = createOpening();
  const win = createWindowWidth();
  const width = createMemo(() => widthClass(win()));
  let thread: HTMLDivElement | undefined;
  let work: HTMLElement | undefined;
  let restore: ((draft: Draft) => boolean) | undefined;
  let clearRestored: ((draft: Draft) => boolean) | undefined;
  const held = new Map<string, { draft: Draft; reply?: string; restored?: boolean }>();
  let sendTicket = 0;
  const restoreRejected = (id: string, m: import("./api.ts").Message | undefined) => {
    const saved = held.get(id);
    if (!saved || !m || (saved.reply && saved.reply !== m.id)) return;
    if (m.status === "streaming") {
      saved.reply = m.id;
      if (saved.restored && place().id === id) clearRestored?.(saved.draft);
      saved.restored = false;
      return;
    }
    if (["error.quotaWait", "error.moveFailed", "error.quotaOut", "error.copy.failed"].includes(m.error ?? "")) {
      if (place().id !== id) return;
      saved.restored = restore?.(saved.draft) || saved.restored;
      if (m.error !== 'error.moveFailed') return;
    }
    held.delete(id);
  };
  let fill: ((text: string) => void) | undefined;
  const [queuedNotice, setQueuedNotice] = createSignal<string>();
  const showRecoveredQueue = (id: string, text: string) => {
    setQueuedNotice(id);
    if (!loading() && place().id === id) fill?.(text);
  };
  // Queue edits remain recoverable after Settings, a thread change or a restart, without keeping their rows alive.
  const draftQueue = createMemo(() => current()?.queued);
  const draftConversation = createMemo(() => current()?.id);
  createEffect(on([draftQueue, draftConversation, loading], () => {
    const c = current();
    if (!c || loading() || place().id !== c.id) return;
    const ids = new Set(c.queued?.map((q) => q.id));
    for (const id of queuedDrafts(c.id)) {
      if (ids.has(id)) continue;
      const text = recoverQueuedDraft(c.id, id);
      if (text !== undefined) showRecoveredQueue(c.id, text);
    }
  }));
  let stick = true;
  // One observer keeps the tail visible when the thread box or the column inside it changes size.
  let tail: ResizeObserver | undefined;
  const tailWatch = () => (tail ??= new ResizeObserver(() => { if (thread && stick) thread.scrollTop = thread.scrollHeight; }));
  let lastScrollTop = 0;
  // Where focus was, until the tutorial took it. Weak, so a control that left the page (Settings closed with the focus in
  // it) is not kept alive, with everything it hangs from, until focus moves again.
  let lastFocus: WeakRef<HTMLElement> | undefined;

  const area = () => place().area;
  const projectId = () => place().projectId;
  const refresh = () =>
    api.state().then((s) => {
      store.offer(s.running);
      setState((current) => {
        const next = refreshed(current, s);
        // A row that did not change stays the same object, so the sidebar keeps its node.
        return { ...next, conversations: keepRows(current.conversations, next.conversations), projects: keepRows(current.projects, next.projects) };
      });
    });
  const fail = (err: unknown) => setError(failureText(err));
  // The sidebar's quota line still names the old usage section: accounts and their limits are one section now.
  const openSettings = (at?: Section | 'usage') => {
    setSettingsAt(at === 'usage' ? 'accounts' : at);
    setSettingsOpen(true);
  };
  const closeSettings = () => {
    setSettingsOpen(false);
    setBack(undefined);
  };
  // The account cards close to where they were opened from: Settings when it sent them, else the shell.
  const closeSetup = () => {
    setSetupOpen(false);
    if (back()) setSettingsOpen(true);
  };
  // A reply whose agent finished while the CLI still closes its turn is done for the person: its row is left out of what the screen
  // shows as running (the engine keeps it, so an update or the awake lock still waits for the process).
  const every = () => store.list().items;
  const items = createMemo(() => shownRunning(every()));
  const needing = () => items().filter((i) => i.state === 'needsYou').length;
  // A run that ended while nobody looked stays a done card in the column and a dot on its sidebar row (task 016) until the person
  // sees the conversation: its thread loaded on the stage (not under Settings, the setup or the first run) of a window they can
  // see (not hidden, no game in front). The engine keeps the mark, so a restart keeps both.
  const onScreen = () => {
    const id = place().id;
    const covered = windowHidden() || settingsOpen() || setupOpen() || !!step() || loading();
    return !id || covered || current()?.id !== id ? undefined : id;
  };
  const done = createMemo(() => doneCards(state().conversations, every(), onScreen()));
  createEffect(() => {
    const id = onScreen();
    if (id && state().conversations.some((c) => c.id === id && c.ended)) void api.seen(id).catch(() => {});
  });

  // ---- The widths the person gave the sidebar and the running column by dragging their edges (ui/Grip.tsx), kept per window like the
  // other small conveniences, and focus mode (Ctrl+B): the sidebar and the column step aside to give the stage the whole window.
  const [widths, setWidths] = createSignal(savedWidths(recall('faber.layout', null)));
  const setWidth = (edge: Edge, px: number | undefined) => {
    setWidths((w) => ({ ...w, [edge]: px }));
    remember('faber.layout', widths());
  };
  const [focus, setFocus] = createSignal(false);
  const dropping = createFileDrop();
  const dropShown = keepForExit(dropping);

  // ---- The sidebar: beside the stage from 900 px, behind a button below that. Opened, it is modal.
  const [sideOpen, setSideOpen] = createSignal(false);
  const narrow = () => width() === 'narrow';
  const sideOverlay = () => narrow() && sideOpen();
  let sideEl: HTMLElement | undefined;
  let menuButton: HTMLButtonElement | undefined;
  const openSide = () => {
    // The column over the stage, if open, makes way: the sidebar overlay is a dialog of its own.
    if (columnOpen() && !docked()) closeColumn();
    setSideOpen(true);
    queueMicrotask(() => sideEl?.querySelector<HTMLElement>('button')?.focus());
  };
  const closeSide = (focusBack = true) => {
    if (!sideOpen()) return;
    setSideOpen(false);
    if (focusBack) menuButton?.focus();
  };
  createEffect(() => {
    if (!narrow()) setSideOpen(false);
  });

  // ---- The running column: closed when the window opens. Beside the stage from 1,100 px, over it below that. Opened by
  // the user over the stage it is modal; opened by itself it is never modal and never moves focus.
  const [columnOpen, setColumnOpen] = createSignal(false);
  const [byUser, setByUser] = createSignal(false);
  const docked = () => width() === 'wide';
  // The column is on screen when it is open and focus mode is off.
  const colShown = () => columnOpen() && !focus();
  const runLimits = createMemo(() => limitsOf('run', win(), widths().side, docked()));
  // What the column really measures: the width the person dragged it to, or until then the room the window leaves beside the
  // reading column, inside what the window leaves it. A drag that ends on the usual width goes back to following the window.
  const autoWidth = () => fit(autoRun(win(), widths().side), runLimits());
  const runWidth = () => fit(widths().run ?? autoRun(win(), widths().side), runLimits());
  // An opened subagent can fill the stage as a full thread (R161 step 3): the column then sits over the work area, not beside it.
  const wide = () => !!sideAgent() && agentWide();
  const trapped = () => colShown() && byUser() && !docked() && !wide();
  let column: HTMLElement | undefined;
  let runButton: HTMLButtonElement | undefined;
  const autoOpen = createAutoOpen();
  let retry: ReturnType<typeof setTimeout> | undefined;
  // An input method is composing text: the column waits for it to end.
  let composing = false;
  const openColumn = (user: boolean) => {
    // Something that opens the column shows it: focus mode steps aside.
    const hidden = focus();
    setFocus(false);
    if (columnOpen()) {
      if (hidden) autoOpen.seen(waitingKeys(items()));
      return;
    }
    // Every question waiting now is on screen, so none of them opens the column again.
    autoOpen.seen(waitingKeys(items()));
    // Two modal dialogs never stack: one the user opens closes the sidebar overlay.
    if (user) closeSide(false);
    setByUser(user);
    setColumnOpen(true);
    if (user && !docked()) queueMicrotask(() => column?.querySelector<HTMLElement>('button')?.focus());
  };
  const closeColumn = () => {
    showAgent(undefined);
    showSite(undefined);
    showAside(undefined);
    setChangesOpen(undefined);
    if (!columnOpen()) return;
    // Closed while it was waiting for the typing to stop, or after it: the questions seen stay seen.
    autoOpen.seen(waitingKeys(items()));
    clearTimeout(retry);
    setColumnOpen(false);
    setByUser(false);
    runButton?.focus();
  };
  const toggleColumn = () => (colShown() ? closeColumn() : openColumn(true));
  const toggleFocus = () => {
    if (focus()) return setFocus(false);
    // Focus inside what is about to leave the screen goes to the box that stays (asked before they go: removing the column drops focus).
    const at = document.activeElement;
    const leaving = !!at && !!(sideEl?.contains(at) || column?.contains(at));
    closeSide(false);
    setFocus(true);
    if (leaving) queueMicrotask(() => (document.querySelector<HTMLTextAreaElement>('.composer textarea') ?? runButton)?.focus());
  };
  // A subagent the person opened (R161 step 3) is the column's content until "back"; it belongs to the conversation on
  // screen, so opening another one goes back to the list.
  const sideAgent = createMemo(() => {
    const open = openedAgent();
    const conversation = current();
    if (!open || conversation?.id !== open.conversationId) return undefined;
    const m = conversation.messages.find((message) => message.id === open.messageId);
    const helperKind = m?.vendor ? vendorName(m.vendor === 'claude' ? 'codex' : 'claude') : '';
    const step = agentSteps(m?.tools, helperKind).find((s) => s.call.id === open.callId);
    // A helper works only in a project whose agent can reach the other AI (both connected), as the engine offers it.
    const helpable = conversation.area === 'project' && state().vendors.length > 1;
    return step && { step, streaming: m?.status === 'streaming', helpable };
  });
  // A page of Faber's browser (R161 step 7) opens in the column the same way, and one of the two shows at a time.
  const sideSite = () => {
    const open = openedSite();
    return open && current()?.id === open.conversationId ? open : undefined;
  };
  // The quick question on the side (R250), of the conversation on screen, or of none yet (it says to ask once it has started).
  const sideAside = () => {
    const open = openedAside();
    return open && current()?.id === open.conversationId ? open : undefined;
  };
  const sideChanges = () => changesOpen() && changesOpen() === current()?.id ? changesOpen() : undefined;
  createEffect(on(openedAgent, (open) => open && (showSite(undefined), showAside(undefined), setChangesOpen(undefined)), { defer: true }));
  createEffect(on(openedSite, (open) => open && (showAgent(undefined), showAside(undefined), setChangesOpen(undefined)), { defer: true }));
  createEffect(on(openedAside, (open) => open && (showAgent(undefined), showSite(undefined), setChangesOpen(undefined)), { defer: true }));
  createEffect(on(() => openedAgent() ?? openedSite() ?? openedAside() ?? (changesOpen() ? { conversationId: changesOpen() } : undefined), (open) => {
    if (!open) return;
    // One of another conversation is never shown (`sideAgent`); it goes when the conversation on screen changes.
    if (current()?.id !== open.conversationId) return;
    if (!columnOpen() || focus()) return openColumn(true);
    // A column that opened by itself becomes the person's: over the stage it is now a modal dialog, like one they opened.
    if (!byUser()) {
      closeSide(false);
      setByUser(true);
    }
    queueMicrotask(() => column?.querySelector<HTMLElement>('button')?.focus());
  }, { defer: true }));
  createEffect(on(() => current()?.id, (id) => {
    if (openedAgent() && openedAgent()!.conversationId !== id) showAgent(undefined);
    if (openedSite() && openedSite()!.conversationId !== id) showSite(undefined);
    if (openedAside() && openedAside()!.conversationId !== id) showAside(undefined);
    if (changesOpen() !== id) setChangesOpen(undefined);
  }, { defer: true }));
  // The window changes width while the column is open: it becomes a modal dialog or stops being one, and its element is
  // drawn again, so focus goes where the new state wants it.
  createEffect(
    on(
      trapped,
      (modal) =>
        queueMicrotask(() => {
          if (modal && !column?.contains(document.activeElement)) column?.querySelector<HTMLElement>('button')?.focus();
          else if (!modal && columnOpen() && (!document.activeElement || document.activeElement === document.body)) runButton?.focus();
        }),
      { defer: true },
    ),
  );

  const consider = () => {
    clearTimeout(retry);
    if (composing) autoOpen.typed(Date.now());
    const verdict = autoOpen.check(waitingKeys(items()), Date.now());
    if (verdict.open) openColumn(false);
    else if (verdict.retryIn !== undefined) retry = setTimeout(consider, verdict.retryIn + 20);
  };
  // Every list that arrives, and the window's first one, may hold a question the column has not shown yet.
  createEffect(() => {
    items();
    consider();
  });

  // A page whose messages fold into a run (task 018) may not fill the window, and a list that cannot scroll never asks for
  // the page above it: it takes pages until it scrolls or nothing is left.
  const fillPage = (): void => {
    requestAnimationFrame(() => {
      if (!thread || from() === 0 || thread.scrollHeight > thread.clientHeight) return;
      setFrom((f) => Math.max(0, f - PAGE));
      fillPage();
    });
  };

  // A plan that changed while its conversation was opening: the snapshot may be older (task 017).
  const planHeard: { plan?: { id: string; plan?: Plan } } = {};
  // The conversation the user is in: opening it asks the engine for a snapshot and the answer takes a while.
  const go = async (next: Place, messageIndex?: number) => {
    clearSearch();
    setUntil(undefined);
    setFrom(Math.max(0, (current()?.messages.length ?? 0) - PAGE));
    planHeard.plan = undefined;
    setPlace(next);
    remember('faber.place', next);
    const token = opening.begin(next.id);
    setLoading(next.id !== undefined);
    setError(undefined);
    stick = true;
    lastScrollTop = 0;
    let snapshot: Conversation | undefined;
    try {
      snapshot = (next.id ? await api.open(next.id) : undefined) ?? undefined;
    } catch (err) {
      fail(err);
    }
    // Messages that came while the snapshot was on its way are already in it, merged by `opening`.
    const done = opening.finish(token, snapshot);
    if (done.stale) return;
    setLoading(false);
    const target = messageIndex !== undefined && messageIndex >= 0 && messageIndex < (done.conversation?.messages.length ?? 0) ? messageIndex : undefined;
    stick = target === undefined;
    setUntil(target === undefined ? undefined : Math.min(done.conversation!.messages.length, target + PAGE));
    setFrom(Math.min(target ?? Infinity, Math.max(0, (done.conversation?.messages.length ?? 0) - PAGE)));
    fillPage();
    const heard = planHeard.plan as { id: string; plan?: Plan } | undefined;
    const plan = heard?.id === done.conversation?.id ? heard : undefined;
    setCurrent(done.conversation && plan ? { ...done.conversation, plan: plan.plan } : done.conversation);
    setSearchIndex(target);
    if (target !== undefined) searchFrame = requestAnimationFrame(() => {
      searchMark = thread?.querySelector<HTMLElement>(`[data-message-index="${target}"]`) ?? undefined;
      if (!searchMark) return;
      searchMark.classList.add('search-focus');
      searchMark.focus({ preventScroll: true });
      const smooth = searchMark.closest<HTMLElement>('[data-motion]')?.dataset.motion === 'full' && !matchMedia('(prefers-reduced-motion: reduce)').matches;
      searchMark.scrollIntoView({ block: 'center', behavior: smooth ? 'smooth' : 'instant' });
      lastScrollTop = thread?.scrollTop ?? 0;
      searchTimer = setTimeout(() => searchMark?.classList.remove('search-focus'), 1800);
      // Consume the reveal in App: the next automatic turn can recreate its RunBlock.
      setSearchIndex(undefined);
    });
    setChoice(next.id ? recall<Choice>(`faber.choice.${next.id}`, done.conversation?.choice ?? { vendor: 'auto' }) : { vendor: 'auto' });
    if (done.conversation) queueMicrotask(() => restoreRejected(done.conversation!.id, done.conversation!.messages.at(-1)));
  };
  const fresh = (next: Pick<Place, 'area' | 'projectId'>) => go({ ...next, id: undefined });
  const openSummary = (c: Summary) => {
    closeSettings();
    closeSide(false);
    void go(openPlace(place(), c));
  };
  // A row of the running column: the area, the project and the conversation change together, so the crumb, the sidebar,
  // the composer and the next send all point at the same place. A conversation that is gone does nothing.
  const openRunning = (item: RunningItem) => {
    if (!state().conversations.some((c) => c.id === item.conversationId)) return;
    closeSettings();
    if (!docked()) closeColumn();
    void go(placeOfRunning(place(), item));
  };

  onMount(() => {
    const firstInstall = isFirstInstall(props.initial.settings, props.initial.conversations);
    void api.notesSeen().then(
      (seen) => {
        const due = whatsNew(NOTES, seen, firstInstall);
        if (due.remember !== undefined) void api.saveNotesSeen(due.remember).catch(() => {});
        setNotes(due.show);
      },
      () => {},
    );
    void api.updateStatus().then(heard, () => {});
    const unlistenUpdate = api.onUpdate(heard).catch(() => () => {});
    onCleanup(() => void unlistenUpdate.then((stop) => stop()));
    // The first typing, paste or drop of the session tells the shell a draft may exist only here, so an update at open waits.
    const told = () => {
      for (const type of ['input', 'paste', 'drop']) window.removeEventListener(type, told, true);
      void api.inputSeen().catch(() => {});
    };
    for (const type of ['input', 'paste', 'drop']) window.addEventListener(type, told, true);
    onCleanup(() => {
      for (const type of ['input', 'paste', 'drop']) window.removeEventListener(type, told, true);
    });
    if (saved?.id && props.initial.conversations.some((c) => c.id === saved.id)) void go({ ...saved, area: saved.area ?? 'chat' });
    const stopFollowing = watchSystem();
    // The restored conversation is already opening (`go` above), so what the inbox held since the page started is kept for
    // its snapshot and nothing is lost between the two.
    props.inbox.take((e) => {
      if (e.type === 'commands' && (e.conversationId === current()?.id || !commandVendor() || e.vendor === commandVendor())) {
        setCommandsVersion((n) => n + 1);
      }
      if (e.type === 'list') void refresh();
      if (e.type === 'usage') setState((s) => ({ ...s, usage: e.vendors }));
      if (e.type === 'message') restoreRejected(e.conversationId, e.message.role === 'assistant' ? e.message : undefined);
      if (e.type === 'quota') setQuotas((q) => withQuota(q, e));
      if (e.type === 'message' && opening.hear(e.conversationId, e.message) === 'apply') {
        setCurrent((c) => (c?.id === e.conversationId ? upsert(c, e.message) : c));
      }
      if (e.type === 'asides') heardAsides(e.conversationId, e.asides);
      if (e.type === 'queue') {
        setCurrent((c) => (c?.id === e.conversationId ? { ...c, queued: e.queued } : c));
      }
      if (e.type === 'context') {
        setCurrent((c) => (c?.id === e.conversationId ? { ...c, context: e.context ?? undefined } : c));
      }
      if (e.type === 'plan') {
        if (opening.id === e.conversationId) planHeard.plan = { id: e.conversationId, plan: e.plan ?? undefined };
        setCurrent((c) => (c?.id === e.conversationId ? { ...c, plan: e.plan ?? undefined } : c));
      }
    });
    // Any key in a text field holds the running column back for a moment, whether or not the text changed (the arrows
    // move the caret), and so does an input method that is composing.
    const typing = (e: Event) => {
      if (!(e.target instanceof Element && e.target.matches(TYPING))) return;
      if (e.type === 'compositionstart') composing = true;
      if (e.type === 'compositionend') composing = false;
      autoOpen.typed(Date.now());
    };
    const focusComposer = () => document.querySelector<HTMLTextAreaElement>('.composer textarea')?.focus();
    const onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      // A first-run step and a tutorial card are modal: their own buttons are the only way on, so Esc, Ctrl N, Ctrl , and
      // Ctrl . do nothing (the card handles its own keys, ui/Tutorial.tsx).
      if (step() || tourOpen()) {
        if (ctrl && ['n', ',', '.', 'b'].includes(e.key.toLowerCase())) e.preventDefault();
        if (step() && e.key === 'Tab' && !ctrl && !e.altKey) keepFocusIn(e, document.querySelector<HTMLElement>('.fr-stage'));
        return;
      }
      const inShell = !settingsOpen() && !setupOpen();
      if (e.key === 'Tab' && !ctrl && !e.altKey) {
        if (!inShell) {
          if (setupOpen()) keepFocusIn(e, document.querySelector<HTMLElement>('.fr-stage'));
        } else if (sideOverlay()) keepFocusIn(e, sideEl);
        else if (trapped()) keepFocusIn(e, column);
      }
      if (ctrl && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        closeSettings();
        opener = undefined; // the new conversation's composer takes focus, not the control that opened the cards
        setSetupOpen(false);
        closeSide(false);
        // A modal column would leave the new conversation's composer out of reach.
        if (trapped()) {
          setColumnOpen(false);
          setByUser(false);
        }
        void fresh(place()).then(focusComposer);
      }
      if (ctrl && e.key === ',') openSettings();
      if (ctrl && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        if (inShell) toggleFocus();
      }
      if (ctrl && e.key === '.') {
        e.preventDefault();
        // Settings and the account cards replace the shell: the column is not on screen to toggle.
        if (inShell) toggleColumn();
      }
      if (e.key === 'Escape') {
        if (sideOverlay() && inShell) return closeSide();
        // A column the user opened over the stage is modal; one that opened itself answers Esc only while focus is in it.
        if (inShell && colShown() && (trapped() || column?.contains(document.activeElement))) return closeColumn();
        if (setupOpen()) closeSetup();
        else closeSettings();
        setSaveError(undefined);
      }
    };
    const watchFocus = (e: FocusEvent) => {
      if (!tourOpen() && e.target instanceof HTMLElement) lastFocus = new WeakRef(e.target);
    };
    document.addEventListener('focusin', watchFocus);
    const typingEvents = ['keydown', 'input', 'compositionstart', 'compositionupdate', 'compositionend'];
    for (const type of typingEvents) window.addEventListener(type, typing, true);
    window.addEventListener('keydown', onKey);
    onCleanup(() => {
      stopFollowing();
      props.inbox.take(undefined);
      clearTimeout(retry);
      for (const type of typingEvents) window.removeEventListener(type, typing, true);
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('focusin', watchFocus);
    });
  });

  // The tray (menu, tooltip, notification) lives outside the page, so the shell gets its words from here. It keeps
  // them while the window is closed.
  createEffect(
    on(
      lang,
      () =>
        void api
          .setShellTexts({
            open: t('tray.open'),
            quit: t('tray.quit'),
            running: templates('tray.running'),
            needsYou: templates('tray.needsYou'),
            notify: templates('tray.notify').other,
            notifyMany: templates('tray.notifyMany').other,
            stopped: Object.fromEntries(STOPS.map((why) => [why, templates(`tray.stopped.${why}`).other])),
          })
          .catch(() => undefined),
    ),
  );

  // Follow the answer while it streams, unless the user scrolled up to read.
  createEffect(
    on(current, () => {
      if (thread && stick) thread.scrollTop = thread.scrollHeight;
    }),
  );

  const onScroll = (el: HTMLDivElement) => {
    const below = el.scrollHeight - el.scrollTop - el.clientHeight;
    const end = until();
    if (below < 40 && (end === undefined || end >= (current()?.messages.length ?? 0))) stick = true;
    else if (el.scrollTop < lastScrollTop - 1) stick = false;
    lastScrollTop = el.scrollTop;
    if (below < 400 && end !== undefined) {
      const next = end + PAGE;
      setUntil(next < (current()?.messages.length ?? 0) ? next : undefined);
    }
    if (el.scrollTop < 400 && from() > 0) {
      // The earlier messages appear above; keep what the user is reading in place.
      const before = el.scrollHeight;
      setFrom((f) => Math.max(0, f - PAGE));
      el.scrollTop += el.scrollHeight - before;
    }
  };

  // A saved choice that is no longer in the list (a model the account dropped, or the old generic ChatGPT entry)
  // falls back to Auto without forgetting it, so it comes back if the model does.
  const picked = (): Choice => {
    const c = choice();
    return options().some((o) => o.choice.vendor === c.vendor && o.choice.model === c.model) ? c : { vendor: 'auto' };
  };

  const pickChoice = (c: Choice) => {
    setChoice(c);
    if (current()) remember(`faber.choice.${current()!.id}`, c);
  };

  const target = () => ({ area: area(), projectId: area() === 'project' ? projectId() : undefined });

  const send = async (draft: Draft) => {
    setQueuedNotice(undefined);
    // `/btw <question>` asks on the side (R250): it opens the quick question with it, and the conversation never sees it.
    const side = draft.images.length || draft.files.length ? undefined : btw(draft.text);
    if (side !== undefined) {
      showAside({ conversationId: current()?.id, question: answered(current()) ? side : undefined });
      // Before the agent has answered there is nothing to ask about: the words go back in the box, and the tab says when.
      // (after the box has emptied itself for the send).
      if (!answered(current())) queueMicrotask(() => restore?.(draft));
      return;
    }
    // The place is taken once: the message, and the conversation it opens, belong to the place it was written in.
    const at = place();
    const chosen = picked();
    const ticket = ++sendTicket;
    // Written while an answer runs, the message is queued or handed to that answer: it is the engine's to keep, so a
    // rejected answer must not put it back in the box.
    const alongside = engineBusy();
    let accepted = false;
    setError(undefined);
    clearSearch();
    setUntil(undefined);
    setFrom(Math.max(0, (current()?.messages.length ?? 0) - PAGE));
    stick = true;
    try {
      const id = await api.send({
        conversationId: current()?.id,
        area: at.area,
        projectId: at.area === 'project' ? at.projectId : undefined,
        text: draft.text,
        images: draft.images,
        files: draft.files,
        choice: chosen,
      });
      const key = `faber.choice.${id}`;
      remember(key, place() === at ? picked() : recall<Choice>(key, chosen));
      accepted = true;
      if (ticket === sendTicket && !alongside) held.set(id, { draft });
      const moved = place() === at && id !== opening.id;
      if (moved) await go({ ...at, id });
      const sent = moved && current()?.id === id ? current() : await api.open(id);
      if (ticket === sendTicket && !alongside) {
        const reply = sent?.messages.at(-1);
        const tracked = held.get(id) ?? { draft };
        tracked.reply = reply?.id;
        held.set(id, tracked);
        restoreRejected(id, reply);
      }
    } catch (err) {
      fail(err);
      if (!accepted && ticket === sendTicket && place() === at) restore?.(draft);
    }
  };

  const branch = async (messageId: string, text: string, picked: Choice) => {
    const c = current();
    if (!c) return;
    const at = place();
    setError(undefined);
    try {
      const id = await api.branch({ conversationId: c.id, messageId, text, choice: picked });
      remember(`faber.choice.${id}`, picked);
      await go({ ...at, id });
    } catch (err) {
      fail(err);
    }
  };

  // Saves run one at a time, so each is built from what the one before stored and answers in order.
  const inTurn = inOrder();
  // The page changes only after the save succeeded. A refusal or a failed write keeps it as it was and says why.
  const persist = async (patch: Saved): Promise<boolean> => {
    setSaveError(undefined);
    try {
      const saved = await saveAndApply(patch, api.saveSettings);
      setState((s) => ({ ...s, settings: saved.settings, look: saved.look }));
      if (patch.language !== undefined) useLanguage(saved.settings.language);
      return true;
    } catch (err) {
      setSaveError(failureText(err));
      return false;
    }
  };
  const saveSettings = (patch: Saved) => inTurn(() => persist(patch));

  const switchArea = (next: Place['area']) => {
    closeSettings();
    closeSide(false);
    void fresh({ area: next, projectId: projectId() });
  };
  const addProject = async () => {
    const project = await api.addProject();
    if (project) {
      closeSide(false);
      void fresh({ area: 'project', projectId: project.id });
    }
  };
  const openProjects = (projects: Project[]) => {
    if (!projects.length) return;
    closeSide(false);
    void fresh({ area: 'project', projectId: projects[0].id });
  };
  /** Deletes a conversation; `'unsaved'` when its copy holds work found nowhere else and the person must say so first. */
  const remove = async (id: string, discard = false): Promise<'unsaved' | undefined> => {
    try {
      await api.remove(id, discard);
    } catch (err) {
      if (errorText(err) === 'error.copy.unsaved') return 'unsaved';
      setSaveError(failureText(err));
      return;
    }
    try { localStorage.removeItem(`faber.choice.${id}`); } catch { /* Storage can be unavailable. */ }
    if (opening.id === id) void fresh(place());
  };

  const options = createMemo(() => (lang(), modelOptions(state().vendors, state().models)));
  const project = () => state().projects.find((p) => p.id === projectId());
  // Faber's checks of the last reply (R146), and a compaction, keep the conversation at work: what is written waits for them, and Stop ends them.
  const engineBusy = () => {
    const last = current()?.messages.at(-1);
    // A working row in the running list that waits for no quota is the engine saying it is at work: an answer, Faber's checks, or a compaction.
    return last?.status === 'streaming' || last?.checks?.state === 'running' || items().some((i) => i.conversationId === current()?.id && i.state === 'working' && !i.waitUntil);
  };
  // What the composer shows: Stop ends the work; once the agent finished (only the CLI's turn is closing) it reads Send, and what is
  // written then waits in the queue like any message written while the engine is busy.
  const busy = () => stopOffered(engineBusy(), every(), current()?.id);
  // As the running column's head button (R250): any open conversation; before its first answer the tab says to ask once the
  // agent has answered. A boolean memo, so streaming updates never touch the lists that read it.
  const canAside = createMemo(() => !!current());
  // "Send now" works while an answer streams (the engine hands the message to its turn, or puts it first); Faber's own checks
  // keep it hidden, as Stop is what ends them.
  const checking = () => engineBusy() && current()?.messages.at(-1)?.status !== 'streaming';
  // "Keep going" (R199): a project conversation under way can go on by itself. The engine keeps the switch; the list says it.
  const keepGoing = () => {
    const c = current();
    if (area() !== 'project' || !c?.messages.length) return undefined;
    return state().conversations.some((s) => s.id === c.id && s.keepGoing);
  };
  // Poor quota is asked about before keep going turns on, and said on screen while it is on (ui/keep-going.ts).
  const keepGoingWarn = createMemo(() => {
    const c = current();
    if (keepGoing() === undefined || !c) return undefined;
    return keepGoingWarning(state().usage ?? [], items().find((r) => r.conversationId === c.id));
  });
  const [askKeepGoing, setAskKeepGoing] = createSignal(false);
  createEffect(on(() => current()?.id, () => setAskKeepGoing(false)));
  const setKeepGoing = (value: boolean) => {
    setAskKeepGoing(false);
    const id = current()?.id;
    if (id) void api.keepGoing(id, value).then(refresh, (err) => setSaveError(failureText(err)));
  };
  // An archived conversation, by the person or its own agent: once its reply ends, one line in place of the composer brings it back.
  const archived = () => !!place().id && state().conversations.some((s) => s.id === place().id && s.archived);
  const unarchive = () => {
    const id = place().id;
    if (id) void api.archive(id, false).then(refresh, (err) => setSaveError(failureText(err)));
  };
  // With no AI, Projects says what Chat says ("No AI connected") instead of offering a folder nothing could work in.
  const needsProject = () => area() === 'project' && !project() && state().vendors.length > 0;
  // The new conversation of a chat with a connected AI is the one place the effects of a look appear.
  const newChat = () => (area() === 'chat' || !!project()) && state().vendors.length > 0 && !current()?.messages.length;
  // A new conversation the user is in, not a remembered one that is still on its way: the tutorial would open over it and
  // vanish when its messages arrive.
  const startsFresh = () => newChat() && !loading() && !place().id;
  // An installed CLI is not a connected account: the tutorial waits for the CLI itself to say it is signed in. It asks
  // again when the installed ones change and when the account cards or a first-run step close.
  const [signedIn, setSignedIn] = createSignal(0);
  const wantsTour = createMemo(() => state().settings.tutorialDone === false && !tourEnded());
  const installed = createMemo(() => state().vendors.join());
  let asked = 0;
  createEffect(() => {
    const vendors = (installed() ? installed().split(',') : []) as Vendor[];
    if (!wantsTour() || step() || setupOpen() || settingsOpen()) return;
    const mine = ++asked;
    void Promise.all(vendors.map((v) => api.setupCheck(v).then((c) => c.account === 'signedIn', () => false))).then((answers) => {
      if (mine === asked) setSignedIn(answers.filter(Boolean).length);
    });
  });
  // The tutorial starts at the new conversation, with an AI connected, when the first run asked for it.
  const tourOpen = createMemo(() =>
    tutorialDue({
      tutorialDone: state().settings.tutorialDone,
      vendors: state().vendors.length > 0 ? signedIn() : 0,
      onNewChat: startsFresh(),
      covered: !!step() || setupOpen() || settingsOpen(),
      endedHere: tourEnded(),
    }),
  );
  // Finishing or skipping saves `tutorialDone`. If that save fails the tutorial still ends for this window, and may show
  // once more at the next launch. Focus goes back to where it was before the tutorial began, or to the composer.
  const endTour = () => {
    setTourEnded(true);
    void saveSettings({ tutorialDone: true });
    const was = lastFocus?.deref();
    const back = was?.isConnected ? was : document.querySelector<HTMLElement>('.composer textarea');
    back?.focus();
  };
  // The screen of what changed comes before anything else, once the first run and the tutorial are not in the way.
  const notesOpen = () => notes().length > 0 && !step() && !tourOpen() && !startScreen();
  const closeNotes = () => {
    void api.saveNotesSeen(newestId(notes())).catch(() => {});
    setNotes([]);
  };
  const restartForUpdate = () => void api.updateRestart().catch((err) => setSaveError(failureText(err)));
  const composerKey = () => place().id ?? `new-${area()}-${projectId() ?? ''}`;
  const title = () => state().conversations.find((c) => c.id === place().id)?.title ?? current()?.title;
  // The same conversation's draft survives moving between the greeting and the dock.
  const composer = createMemo(() => (
    <Show when={composerKey()} keyed>
      <Composer
        draftKey={composerKey()}
        vendors={state().vendors}
        options={options()}
        choice={picked()}
        onChoice={pickChoice}
        mode={state().settings.permissions ?? 'dontAsk'}
        onMode={(permissions) => void saveSettings({ permissions })}
        busy={busy() && !loading()}
        helpers={helpersAtWork(current()?.messages)}
        locked={loading()}
        placeholder={area() === 'project' ? t('composer.projectPlaceholder') : newChat() ? t('composer.chatPlaceholder') : t('composer.replyPlaceholder')}
        hints={newChat() ? homeHints(area() === 'project' ? 'project' : 'chat') : undefined}
        onSend={(d) => void send(d)}
        onStop={() => current() && void api.interrupt(current()!.id)}
        keepGoing={keepGoing()}
        onKeepGoing={(on) => {
          // Turning it on while the quota is poor asks first (the chip again closes the question); off never asks.
          if (on && keepGoingWarn()) return setAskKeepGoing(!askKeepGoing());
          setKeepGoing(on);
        }}
        onWarm={() => void api.warm({ conversationId: current()?.id, ...target(), choice: picked() })}
        onPause={(text) => {
          if (!loading() && picked().vendor === 'auto') void api.routeDraft({ conversationId: current()?.id, ...target(), choice: picked() }, text);
        }}
        onReady={(f) => (fill = f)}
        onHistory={(signal) => api.promptHistory(current()?.id, area(), area() === 'project' ? projectId() : undefined, signal)}
        onMention={current() || (area() === 'project' && projectId()) ? (query) => api.mentions(current()?.id, area() === 'project' ? projectId() : undefined, query) : undefined}
        onCommands={() => {
          const vendor = commandVendor();
          return api.commands(current()?.id, vendor).then((list) => vendor ? list.filter((c) => c.vendor === vendor) : list);
        }}
        commandsVersion={commandsVersion()}
        canAside={canAside()}
        onRestore={(f) => {
          restore = f;
          queueMicrotask(() => current() && restoreRejected(current()!.id, current()!.messages.at(-1)));
        }}
        onClearRestored={(f) => (clearRestored = f)}
      />
    </Show>
  ));
  createEffect(on(newChat, () => {
    const focused = lastFocus?.deref();
    queueMicrotask(() => {
      if (focused?.matches('.composer textarea') && focused.isConnected && document.activeElement === document.body) focused.focus();
    });
  }, { defer: true }));

  const runningColumn = () => (
    <Running
      items={items()}
      done={done()}
      projects={state().projects}
      modal={trapped()}
      onOpen={openRunning}
      onOpenDone={(c) => {
        if (!docked()) closeColumn();
        openSummary(c);
      }}
      onDismiss={(c) => void api.seen(c.id).catch(() => {})}
      onClose={closeColumn}
      agent={sideAgent()}
      site={sideSite()}
      aside={sideAside() && { conversation: current(), options: options() }}
      changes={sideChanges() ? { id: sideChanges()!, files: changeList()?.files ?? [], incomplete: changeList()?.incomplete ?? false, loading: changes.loading, loadingMore: loadingMoreChanges(), onMore: () => void moreChanges() } : undefined}
      onChanges={changeList()?.files.length || changeList()?.incomplete ? () => {
        showAgent(undefined);
        showSite(undefined);
        showAside(undefined);
        setChangesOpen(current()?.id);
      } : undefined}
      onAside={canAside() ? () => showAside({ conversationId: current()?.id }) : undefined}
      onBack={() => {
        showAgent(undefined);
        showSite(undefined);
        showAside(undefined);
        setChangesOpen(undefined);
        // The back button is gone with the subagent or the page: focus stays in the column, on the list's first control.
        queueMicrotask(() => column?.querySelector<HTMLElement>('button')?.focus());
      }}
      onTell={(text) => {
        // The box sits behind a modal column: the column makes way first, so the box can take focus.
        if (trapped()) closeColumn();
        queueMicrotask(() => startMessage(text));
      }}
      wide={wide()}
      onWide={() => {
        toggleAgentWide();
        // The column is drawn again where it now sits: focus goes back to the button that was pressed.
        queueMicrotask(() => column?.querySelector<HTMLElement>('.agent-wide')?.focus());
      }}
      resize={wide() ? undefined : { value: runWidth(), limits: runLimits(), fallback: autoWidth(), onChange: (px) => setWidth('run', px === autoWidth() ? undefined : px) }}
      ref={(el) => (column = el)}
    />
  );

  return (
    <div class="app" ref={(el) => { animateList(el); animateDetails(el); }}>
      {/* The title bar is the window's title bar: empty space drags the window, double click maximizes. */}
      <header class="top" classList={{ mac }} data-tauri-drag-region>
        <span class="brand" data-tauri-drag-region>
          <Lockup height={20} />
        </span>
        <WindowControls />
      </header>
      <Show when={saveError()}>
        <div class="toast" role="alert">
          <span>{saveError()}</span>
          <button type="button" aria-label={t('toast.dismiss')} onClick={() => setSaveError(undefined)}>
            <Icon name="close" small />
          </button>
        </div>
      </Show>
      <Show
        when={!step() && !setupOpen()}
        fallback={
          <Show
            when={step() === 'look'}
            fallback={
              <Setup
                mode={step() ? 'firstRun' : 'manage'}
                onFinish={(result) =>
                  void saveSettings(setupPatch(result)).then((saved) => {
                    if (!saved) return;
                    // Step 1 hands over to step 2 once its choice is stored; the cards opened later only close.
                    if (step()) setStep(lookDue(state().settings) ? 'look' : undefined);
                    closeSetup();
                    void refresh();
                  })
                }
              />
            }
          >
            <LookStep
              initial={kept()?.appearance ?? state().settings.appearance}
              experience={kept()?.experience ?? state().settings.experience}
              userName={kept()?.name ?? state().settings.userName}
              onBack={(appearance, experience, name) => {
                setKept({ appearance, experience, name });
                setStep('accounts');
              }}
              onError={(err) => setSaveError(failureText(err))}
              onContinue={async (appearance, experience, name) => {
                const saved = await saveSettings(continuePatch(appearance, experience, state().settings.tutorialDone, name));
                if (saved) {
                  // The first conversation is a new one, whatever place was remembered from an older version.
                  setStep(undefined);
                  void fresh({ area: 'chat' });
                }
                return saved;
              }}
            />
          </Show>
        }
      >
        <Show
          when={!settingsOpen()}
          fallback={
            <Settings
              look={state().look}
              saved={state().settings}
              vendors={state().vendors}
              quotas={quotas()}
              usage={state().usage ?? []}
              projects={state().projects}
              now={clock()}
              version={state().version}
              start={back()?.section ?? settingsAt()}
              kept={back()?.draft}
              onSave={saveSettings}
              chatFolder={current()?.workingFolder}
              chatFolderLocked={!!current()?.messages.length || loading()}
              onChatFolder={area() === 'chat' ? async (scratch, path) => {
                try {
                  const conversation = await api.chatFolder(current()?.id, scratch, path);
                  if (conversation) {
                    await refresh();
                    await go({ area: 'chat', id: conversation.id });
                  }
                } catch (err) { setSaveError(failureText(err)); }
              } : undefined}
              onError={(err) => setSaveError(failureText(err))}
              onClose={closeSettings}
              onTour={() => {
                // The tutorial plays again where it starts, a new conversation, once Settings has gone.
                void saveSettings({ tutorialDone: false }).then((saved) => {
                  if (!saved) return;
                  setTourEnded(false);
                  closeSettings();
                  closeSide(false);
                  void fresh(place());
                });
              }}
              onOpenSetup={(keep) => {
                setBack(keep);
                // The cards remember the focused control before Settings is taken off the screen with it.
                openSetup();
                setSettingsOpen(false);
              }}
            />
          }
        >
          <div
            class="shell"
            data-screen={`${area() === 'chat' ? 'chat' : 'project'}.${newChat() ? 'new' : 'thread'}`}
            classList={{ docked: colShown() && docked() && !wide(), focus: focus() }}
            style={{ '--side-w': `${widths().side}px`, '--run-w': `${runWidth()}px` }}
            inert={tourOpen() || notesOpen() || startScreen()}
            ref={animateList}
          >
            <aside
              class="side"
              data-surface="sidebar"
              classList={{ shown: sideOverlay() }}
              ref={(el) => (sideEl = el)}
              aria-label={t('side.label')}
              role={sideOverlay() ? 'dialog' : undefined}
              aria-modal={sideOverlay() ? 'true' : undefined}
              inert={trapped() || (narrow() && !sideOverlay()) || (focus() && !narrow())}
              aria-hidden={(narrow() && !sideOverlay()) || (focus() && !narrow()) ? 'true' : undefined}
            >
              <Sidebar
                conversations={state().conversations}
                projects={state().projects}
                vendors={state().vendors}
                version={state().version}
                update={update()}
                updateBlocked={every().length > 0}
                onRestart={restartForUpdate}
                userName={state().settings.userName}
                // A Skip on the account cards means "later": with one AI connected, the sidebar offers the other until the cards
                // are opened and closed (which saves `done`) or both are in. With none, the No AI screen already says it.
                remindConnect={state().settings.setup === 'skipped' && state().vendors.length === 1}
                onConnect={openSetup}
                place={place()}
                running={items()}
                quotas={quotas()}
                usage={state().usage ?? []}
                now={clock()}
                onNew={() => (closeSide(false), void fresh(place()))}
                onArea={switchArea}
                onProject={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onNewIn={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onAddProject={() => void addProject().catch(fail)}
                onOpen={openSummary}
                onMessage={(hit) => {
                  const c = state().conversations.find((c) => c.id === hit.conversationId);
                  if (!c) return;
                  closeSettings();
                  closeSide(false);
                  void go(openPlace(place(), c), hit.messageIndex);
                }}
                onRemove={remove}
                onSettings={(at) => (closeSide(false), openSettings(at))}
                onError={(err) => setSaveError(failureText(err))}
              />
              <Show when={!narrow()}>
                <Grip edge="side" label={t('side.resize')} value={widths().side} limits={SIDE} fallback={SIDE.fallback} onChange={(px) => setWidth('side', px)} />
              </Show>
            </aside>
            <Show when={sideOverlay()}>
              <button class="scrim" tabindex="-1" aria-label={t('side.hide')} onClick={() => closeSide()} />
            </Show>
            <main class="main" inert={sideOverlay()}>
              <div class="topbar" classList={{ bare: newChat() }} inert={trapped()}>
                <Show when={narrow()}>
                  <button class="iconbtn menu" ref={(el) => (menuButton = el)} aria-label={t('side.show')} aria-expanded={sideOverlay()} onClick={openSide}>
                    <Icon name="menu" />
                  </button>
                </Show>
                {/* Focus mode hid the sidebar: this brings it back (and the column). It is always here from 900 px, shut, so it grows in
                    instead of appearing; below that the menu button is the way to the sidebar. */}
                <button
                  class="iconbtn focus-exit"
                  classList={{ off: !focus() || narrow() }}
                  aria-label={t('focus.exit')}
                  title={t('focus.exit')}
                  inert={!focus() || narrow()}
                  onClick={() => setFocus(false)}
                >
                  <Icon name="menu" />
                </button>
                <div class="crumb">
                  <Switch>
                    <Match when={place().id}>
                      {area() === 'chat' ? t('area.chat') : t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        {t('crumb.separator')}
                      </span>
                      <b>{title() ?? ''}</b>
                      {/* A project conversation in a copy of its own says which branch, quietly; the path is the tooltip. */}
                      <Show when={current()?.id === place().id && current()?.copy}>
                        {(copy) => (
                          <>
                            <span class="sep" aria-hidden="true">
                              {t('crumb.separator')}
                            </span>
                            <span class="copy" data-testid="crumb-copy" title={`${t('crumb.copy')}
${copy().path}`}>
                              {copy().branch}
                            </span>
                          </>
                        )}
                      </Show>
                    </Match>
                    <Match when={area() === 'project' && project()}>
                      {t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        {t('crumb.separator')}
                      </span>
                      <b>{project()?.name}</b>
                    </Match>
                    <Match when={area() === 'project'}>
                      <b>{t('crumb.project')}</b>
                    </Match>
                    <Match when={true}>{t('crumb.new')}</Match>
                  </Switch>
                </div>
                <div class="right">
                  <Show when={current()?.id === place().id ? current()?.context : undefined}>{(fill) => <ContextMeter fill={fill()} />}</Show>
                  <button
                    class="run-btn"
                    data-tour="run"
                    classList={{ need: needing() > 0 }}
                    ref={(el) => (runButton = el)}
                    aria-expanded={colShown()}
                    aria-controls="running"
                    title={t('run.toggle')}
                    onClick={toggleColumn}
                  >
                    <Show when={items().length > 0} fallback={<span class="seg">{t('run.none')}</span>}>
                      <span class="seg">
                        <span class="live" aria-hidden="true" />
                        {t('run.running', { n: items().length })}
                      </span>
                      <Show when={needing() > 0}>
                        <span class="seg ask">
                          <Icon name="ask" small />
                          {t('run.needs', { n: needing() })}
                        </span>
                      </Show>
                    </Show>
                  </button>
                </div>
              </div>
              <div class="work" classList={{ under: colShown() && !docked() }} ref={(el) => { work = el; animateList(el); }}>
                <Show
                  when={needsProject()}
                  fallback={
                    <>
                      <Show when={newChat()}>
                        {/* The effects of the look sit behind the greeting; dust keeps clear of the text and the composer. */}
                        <Ambient variant={area() === 'project' ? 'project' : 'chat'} vendors={state().vendors} avoid={() => [...work!.querySelectorAll('.hello, .lead, .composer, .try, .resume')]} />
                        {/* A project's new conversation is a work screen: no beam, and in Dark Room the black hole sits behind its column. */}
                        <Show when={area() === 'project'}>
                          <Hole avoid={() => [...work!.querySelectorAll('.welcome-grid .peek, .welcome-grid .w-main')]} />
                        </Show>
                      </Show>
                      <div class="thread" data-gallery classList={{ welcome: newChat() }} ref={(el) => {
                        thread = el;
                        onMount(() => {
                          // Keep the tail visible as the composer grows, without pulling a reader back down.
                          tailWatch().observe(el);
                          onCleanup(() => tailWatch().unobserve(el));
                        });
                      }} onScroll={(e) => onScroll(e.currentTarget)} inert={trapped() || (colShown() && wide())}>
                        <Show
                          when={newChat()}
                          fallback={
                            <div class="col" ref={(el) => {
                              animateList(el);
                              onMount(() => {
                                tailWatch().observe(el);
                                onCleanup(() => tailWatch().unobserve(el));
                              });
                            }}>
                              <Show
                                when={current()?.messages.length ? current() : undefined}
                                fallback={<NoAiOrProject vendors={state().vendors.length} area={area()} project={project()} onConnect={openSetup} />}
                              >
                                {(c) => (
                                  <Thread
                                    messages={c().messages}
                                    queued={c().queued}
                                    busy={checking()}
                                    weighing={items().some((i) => i.conversationId === c().id && i.weighing)}
                                    onError={fail}
                                    onQueuedRecovered={showRecoveredQueue}
                                    usage={state().usage ?? []}
                                    from={from()}
                                    until={until()}
                                    searchIndex={searchIndex()}
                                    conversationId={c().id}
                                    options={options()}
                                    choice={picked()}
                                    onBranch={(messageId, text, chosen) => void branch(messageId, text, chosen ?? picked())}
                                    plan={c().plan}
                                    onReply={(text) => void send({ text, images: [], files: [] })}
                                  />
                                )}
                              </Show>
                              <Show when={error()}>
                                <div class="note err" role="alert">
                                  {error()}
                                </div>
                              </Show>
                            </div>
                          }
                        >
                          <Welcome
                            recent={state().conversations}
                            name={state().settings.userName}
                            running={items()}
                            projects={state().projects}
                            project={area() === 'project' ? project() : undefined}
                            onSuggest={(text) => fill?.(text)}
                            onOpen={openSummary}
                          >
                            {composer()}
                          </Welcome>
                          <Show when={error()}>
                            <div class="note err" role="alert">
                              {error()}
                            </div>
                          </Show>
                        </Show>
                      </div>
                      <Show when={!newChat() && !needsProject() && state().vendors.length > 0}>
                        <div class="dock" inert={trapped()} onInput={() => setQueuedNotice(undefined)}>
                          <Show when={queuedNotice() && queuedNotice() === current()?.id}>
                            <p class="note" role="status">{t('queue.editRecovered')}</p>
                          </Show>
                          <KeepGoingNote
                            warning={keepGoingWarn()}
                            on={!!keepGoing()}
                            asking={askKeepGoing()}
                            onYes={() => setKeepGoing(true)}
                            onNo={() => setAskKeepGoing(false)}
                          />
                          <Show when={archived() && !busy()} fallback={composer()}>
                            <div class="archived-bar" role="status">
                              <span>{t('archive.line')}</span>
                              <button type="button" class="btn" onClick={unarchive}>
                                {t('menu.unarchive')}
                              </button>
                            </div>
                          </Show>
                        </div>
                      </Show>
                    </>
                  }
                >
                  <Projects
                    vendors={state().vendors}
                    projects={state().projects}
                    recent={state().conversations}
                    running={items()}
                    onOpen={openSummary}
                    onAdd={() => void addProject().catch(fail)}
                    onOpened={openProjects}
                    onError={fail}
                  />
                </Show>
                <Show when={colShown() && (!docked() || wide())}>{runningColumn()}</Show>
              </div>
              <Find enabled={!newChat() && !needsProject() && !loading() && !!current()?.messages.length && !(colShown() && wide())} root={() => thread} />
            </main>
            <Show when={colShown() && docked() && !wide()}>{runningColumn()}</Show>
          </div>
        </Show>
      </Show>
      <Show when={tourOpen()}>
        <Tutorial experience={state().settings.experience} onEnd={endTour} />
      </Show>
      <Show when={startScreen()}>
        <UpdateStage status={update()} />
      </Show>
      <Show when={dropShown()}>
        {/* A file over the window: where it will go, said calmly. It never takes a click; the drop is heard by the window. */}
        <div class="dropveil" classList={{ leaving: !dropping() }} aria-hidden="true">
          <div class="dv-card">
            <Icon name="clip" />
            <span>{t('drop.attach')}</span>
          </div>
        </div>
      </Show>
      <Show when={notesOpen()}>
        <WhatsNew version={state().version} notes={notes()} onClose={closeNotes} />
      </Show>
      <Palette
        enabled={!step() && !setupOpen() && !tourOpen() && !notesOpen() && !startScreen()}
        conversations={state().conversations}
        projects={state().projects}
        onOpen={openSummary}
        onProject={(id) => (closeSettings(), closeSide(false), void fresh({ area: 'project', projectId: id }))}
        onNew={() => (closeSettings(), closeSide(false), void fresh(place()).then(() => document.querySelector<HTMLElement>('.composer textarea')?.focus()))}
        onArea={switchArea}
        onSettings={(at) => (settingsOpen() ? (closeSettings(), queueMicrotask(() => openSettings(at))) : openSettings(at))}
        onRunning={() => (closeSettings(), toggleColumn())}
        onAddProject={() => (closeSettings(), void addProject().catch(fail))}
      />
    </div>
  );
}

function NoAiOrProject(props: {
  vendors: number;
  area: Place['area'];
  project?: { name: string; path: string };
  onConnect: () => void;
}) {
  return (
    <Switch>
      <Match when={props.vendors === 0}>
        <Empty
          title={t('empty.noAiTitle')}
          line={t(props.area === 'project' ? 'empty.noAiProjectsBody' : 'empty.noAiBody')}
          button={{ label: t('empty.noAiButton'), onClick: props.onConnect, restore: 'connect' }}
        />
      </Match>
      <Match when={props.area === 'project'}>
        <Empty title={props.project?.name ?? ''} line={t('empty.projectBody', { path: props.project?.path ?? '' })} />
      </Match>
    </Switch>
  );
}
