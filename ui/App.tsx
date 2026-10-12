import { createEffect, createMemo, createSignal, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { Ambient } from './Ambient.tsx';
import {
  api,
  type Appearance,
  type AppState,
  type Choice,
  type AppEvent,
  type Conversation,
  type Experience,
  type RunningItem,
  type Settings as Saved,
  type Summary,
  type Vendor,
} from './api.ts';
import { inOrder, refreshed, saveAndApply, watchSystem } from './appearance.ts';
import { Composer, type Draft } from './Composer.tsx';
import { LookStep } from './FirstRun.tsx';
import { keepFocusIn } from './focus.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { failureText, lang, t, templates, useLanguage } from './i18n.ts';
import { createWidth } from './layout.ts';
import { Lockup, Peek } from './Mark.tsx';
import { modelOptions, prettyModel } from './models.ts';
import { createOpening, upsert } from './opening.ts';
import { Running } from './Running.tsx';
import { createAutoOpen, openPlace, type Place, placeOfRunning, store, waitingKeys } from './running.ts';
import { type Keep, type Section, Settings } from './Settings.tsx';
import { Setup } from './Setup.tsx';
import { continuePatch, type FirstStep, firstRunStep, lookDue, setupPatch } from './setup.ts';
import { Sidebar } from './Sidebar.tsx';
import { seedQuotas, withQuota } from './sidebar.ts';
import { Thread } from './Thread.tsx';
import { tutorialDue } from './tour.ts';
import { Tutorial } from './Tutorial.tsx';
import { Welcome } from './Welcome.tsx';
import { mac, WindowControls } from './WindowControls.tsx';

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
export function App(props: { initial: AppState; inbox: { take: (listener: ((event: AppEvent) => void) | undefined) => void } }) {
  const saved = recall<Place | undefined>('faber.place', undefined);
  useLanguage(props.initial.settings.language);
  const [state, setState] = createSignal<AppState>(props.initial);
  // The area, the project and the conversation the user is in, changed together so no screen ever shows a conversation
  // with another conversation's project.
  const [place, setPlace] = createSignal<Place>({ area: saved?.area ?? 'chat', projectId: saved?.projectId });
  const [current, setCurrent] = createSignal<Conversation>();
  const [choice, setChoice] = createSignal<Choice>(recall<Choice>('faber.choice', { vendor: 'auto' }));
  const [quotas, setQuotas] = createSignal(seedQuotas(props.initial.quotas));
  const [clock, setClock] = createSignal(Date.now());
  const [error, setError] = createSignal<string>();
  // The text of the last refused or failed save of settings; the page stays as it was.
  const [saveError, setSaveError] = createSignal<string>();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  // The section a link into Settings names (the quota in the sidebar names the usage limits), and what Settings kept while
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
  // The choices of step 2 while the person is back at step 1, so going back and forward loses nothing.
  const [kept, setKept] = createSignal<{ appearance: Appearance; experience?: Experience }>();
  const [from, setFrom] = createSignal(0);
  // True from the moment the user picks a conversation until its snapshot is in: the screen still shows the old
  // conversation then, so nothing may be sent or warmed for either.
  const [loading, setLoading] = createSignal(false);
  const opening = createOpening();
  const width = createWidth();
  let thread: HTMLDivElement | undefined;
  let work: HTMLElement | undefined;
  let fill: ((text: string) => void) | undefined;
  let stick = true;
  // Where focus was, until the tutorial took it. Weak, so a control that left the page (Settings closed with the focus in
  // it) is not kept alive, with everything it hangs from, until focus moves again.
  let lastFocus: WeakRef<HTMLElement> | undefined;

  const area = () => place().area;
  const projectId = () => place().projectId;
  const refresh = () =>
    api.state().then((s) => {
      store.offer(s.running);
      setState((current) => refreshed(current, s));
    });
  const fail = (err: unknown) => setError(failureText(err));
  const openSettings = (at?: Section) => {
    setSettingsAt(at);
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
  const items = () => store.list().items;
  const needing = () => items().filter((i) => i.state === 'needsYou').length;

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
  const trapped = () => columnOpen() && byUser() && !docked();
  let column: HTMLElement | undefined;
  let runButton: HTMLButtonElement | undefined;
  const autoOpen = createAutoOpen();
  let retry: ReturnType<typeof setTimeout> | undefined;
  // An input method is composing text: the column waits for it to end.
  let composing = false;
  const openColumn = (user: boolean) => {
    if (columnOpen()) return;
    // Every question waiting now is on screen, so none of them opens the column again.
    autoOpen.seen(waitingKeys(items()));
    // Two modal dialogs never stack: one the user opens closes the sidebar overlay.
    if (user) closeSide(false);
    setByUser(user);
    setColumnOpen(true);
    if (user && !docked()) queueMicrotask(() => column?.querySelector<HTMLElement>('button')?.focus());
  };
  const closeColumn = () => {
    if (!columnOpen()) return;
    // Closed while it was waiting for the typing to stop, or after it: the questions seen stay seen.
    autoOpen.seen(waitingKeys(items()));
    clearTimeout(retry);
    setColumnOpen(false);
    setByUser(false);
    runButton?.focus();
  };
  const toggleColumn = () => (columnOpen() ? closeColumn() : openColumn(true));
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

  // The conversation the user is in: opening it asks the engine for a snapshot and the answer takes a while.
  const go = async (next: Place) => {
    setPlace(next);
    remember('faber.place', next);
    const token = opening.begin(next.id);
    setLoading(next.id !== undefined);
    setError(undefined);
    stick = true;
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
    setFrom(Math.max(0, (done.conversation?.messages.length ?? 0) - PAGE));
    setCurrent(done.conversation);
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
    if (saved?.id && props.initial.conversations.some((c) => c.id === saved.id)) void go({ ...saved, area: saved.area ?? 'chat' });
    const stopFollowing = watchSystem();
    // The restored conversation is already opening (`go` above), so what the inbox held since the page started is kept for
    // its snapshot and nothing is lost between the two.
    props.inbox.take((e) => {
      if (e.type === 'list') void refresh();
      if (e.type === 'quota') setQuotas((q) => withQuota(q, e));
      if (e.type === 'message' && opening.hear(e.conversationId, e.message) === 'apply') {
        setCurrent((c) => (c?.id === e.conversationId ? upsert(c, e.message) : c));
      }
    });
    const minute = setInterval(() => setClock(Date.now()), 60_000);
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
        if (ctrl && ['n', ',', '.'].includes(e.key.toLowerCase())) e.preventDefault();
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
      if (ctrl && e.key === '.') {
        e.preventDefault();
        // Settings and the account cards replace the shell: the column is not on screen to toggle.
        if (inShell) toggleColumn();
      }
      if (e.key === 'Escape') {
        if (sideOverlay() && inShell) return closeSide();
        // A column the user opened over the stage is modal; one that opened itself answers Esc only while focus is in it.
        if (inShell && columnOpen() && (trapped() || column?.contains(document.activeElement))) return closeColumn();
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
      clearInterval(minute);
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
    stick = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
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
    remember('faber.choice', c);
  };

  const target = () => ({ area: area(), projectId: area() === 'project' ? projectId() : undefined });

  const send = async (draft: Draft) => {
    // The place is taken once: the message, and the conversation it opens, belong to the place it was written in.
    const at = place();
    setError(undefined);
    stick = true;
    try {
      const id = await api.send({
        conversationId: current()?.id,
        area: at.area,
        projectId: at.area === 'project' ? at.projectId : undefined,
        text: draft.text,
        images: draft.images,
        files: draft.files,
        choice: picked(),
      });
      if (place() === at && id !== opening.id) await go({ ...at, id });
    } catch (err) {
      fail(err);
    }
  };

  const branch = async (messageId: string, text: string, picked: Choice) => {
    const c = current();
    if (!c) return;
    setError(undefined);
    try {
      await go({ ...place(), id: await api.branch({ conversationId: c.id, messageId, text, choice: picked }) });
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
  const remove = async (id: string) => {
    await api.remove(id);
    if (opening.id === id) void fresh(place());
  };

  const options = createMemo(() => (lang(), modelOptions(state().vendors, state().models)));
  const project = () => state().projects.find((p) => p.id === projectId());
  const busy = () => current()?.messages.at(-1)?.status === 'streaming';
  const needsProject = () => area() === 'project' && !project();
  // The new conversation of a chat with a connected AI is the one place the effects of a look appear.
  const newChat = () => area() === 'chat' && state().vendors.length > 0 && !current()?.messages.length;
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
  const composerKey = () => place().id ?? `new-${area()}-${projectId() ?? ''}`;
  const title = () => state().conversations.find((c) => c.id === place().id)?.title ?? current()?.title;
  // The model the open conversation is answering with now, else the one the next message will go to.
  const answering = () => current()?.messages.findLast((m) => m.role === 'assistant' && m.vendor);

  const composer = () => (
    <Show when={composerKey()} keyed>
      <Composer
        vendors={state().vendors}
        options={options()}
        choice={picked()}
        onChoice={pickChoice}
        busy={busy() && !loading()}
        locked={loading()}
        placeholder={area() === 'project' ? t('composer.projectPlaceholder') : newChat() ? t('composer.chatPlaceholder') : t('composer.replyPlaceholder')}
        onSend={(d) => void send(d)}
        onStop={() => current() && void api.interrupt(current()!.id)}
        onWarm={() => void api.warm({ conversationId: current()?.id, ...target(), choice: picked() })}
        onPause={(text) => {
          if (!loading() && picked().vendor === 'auto') void api.routeDraft({ conversationId: current()?.id, ...target(), choice: picked() }, text);
        }}
        onReady={(f) => (fill = f)}
      />
    </Show>
  );

  const runningColumn = () => (
    <Running
      items={items()}
      projects={state().projects}
      modal={trapped()}
      onOpen={openRunning}
      onClose={closeColumn}
      ref={(el) => (column = el)}
    />
  );

  return (
    <div class="app">
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
            ×
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
              onBack={(appearance, experience) => {
                setKept({ appearance, experience });
                setStep('accounts');
              }}
              onError={(err) => setSaveError(failureText(err))}
              onContinue={async (appearance, experience) => {
                const saved = await saveSettings(continuePatch(appearance, experience, state().settings.tutorialDone));
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
              now={clock()}
              start={back()?.section ?? settingsAt()}
              kept={back()?.draft}
              onSave={saveSettings}
              onClose={closeSettings}
              onOpenSetup={(keep) => {
                setBack(keep);
                // The cards remember the focused control before Settings is taken off the screen with it.
                openSetup();
                setSettingsOpen(false);
              }}
            />
          }
        >
          <div class="shell" classList={{ docked: columnOpen() && docked() }} inert={tourOpen()}>
            <aside
              class="side"
              classList={{ shown: sideOverlay() }}
              ref={(el) => (sideEl = el)}
              aria-label={t('side.label')}
              role={sideOverlay() ? 'dialog' : undefined}
              aria-modal={sideOverlay() ? 'true' : undefined}
              inert={trapped()}
            >
              <Sidebar
                conversations={state().conversations}
                projects={state().projects}
                vendors={state().vendors}
                version={state().version}
                place={place()}
                running={items()}
                quotas={quotas()}
                now={clock()}
                onNew={() => (closeSide(false), void fresh(place()))}
                onArea={switchArea}
                onProject={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onNewIn={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onAddProject={() => void addProject()}
                onOpen={openSummary}
                onRemove={(id) => void remove(id)}
                onSettings={(at) => (closeSide(false), openSettings(at))}
              />
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
                <div class="crumb">
                  <Switch>
                    <Match when={place().id}>
                      {area() === 'chat' ? t('area.chat') : t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        ›
                      </span>
                      <b>{title() ?? ''}</b>
                    </Match>
                    <Match when={area() === 'project' && project()}>
                      {t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        ›
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
                  <Show when={state().vendors.length > 0 && !newChat()}>
                    <span class="chip" title={t('topbar.model')}>
                      <Show when={answering()} fallback={<>{options().find((o) => o.choice.vendor === picked().vendor && o.choice.model === picked().model)?.label}</>}>
                        {(m) => (
                          <>
                            <VendorMark vendor={m().vendor} />
                            {prettyModel(m().model)}
                          </>
                        )}
                      </Show>
                    </span>
                  </Show>
                  <button
                    class="run-btn"
                    data-tour="run"
                    classList={{ need: needing() > 0 }}
                    ref={(el) => (runButton = el)}
                    aria-expanded={columnOpen()}
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
              <div class="work" classList={{ under: columnOpen() && !docked() }} ref={(el) => (work = el)}>
                <Show when={newChat()}>
                  {/* The effects of the look sit behind the greeting; dust keeps clear of the text and the composer. */}
                  <Ambient vendors={state().vendors} avoid={() => [...work!.querySelectorAll('.hello, .lead, .composer, .try, .resume')]} />
                </Show>
                <div class="thread" classList={{ welcome: newChat() }} ref={(el) => (thread = el)} onScroll={(e) => onScroll(e.currentTarget)} inert={trapped()}>
                  <Show
                    when={newChat()}
                    fallback={
                      <div class="col">
                        <Show
                          when={current()?.messages.length ? current() : undefined}
                          fallback={<Empty vendors={state().vendors.length} area={area()} project={project()} onConnect={openSetup} />}
                        >
                          {(c) => (
                            <Thread
                              messages={c().messages}
                              from={from()}
                              conversationId={c().id}
                              options={options()}
                              onBranch={(messageId, text, chosen) => void branch(messageId, text, chosen ?? picked())}
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
                      recent={state().conversations.slice(0, 12)}
                      running={items()}
                      projects={state().projects}
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
                <Show when={!newChat() && !needsProject()}>
                  <div class="dock" inert={trapped()}>
                    {composer()}
                  </div>
                </Show>
                <Show when={columnOpen() && !docked()}>{runningColumn()}</Show>
              </div>
            </main>
            <Show when={columnOpen() && docked()}>{runningColumn()}</Show>
          </div>
        </Show>
      </Show>
      <Show when={tourOpen()}>
        <Tutorial experience={state().settings.experience} onEnd={endTour} />
      </Show>
    </div>
  );
}

function Empty(props: { vendors: number; area: Place['area']; project?: { name: string; path: string }; onConnect: () => void }) {
  return (
    <div class="empty">
      <Switch>
        <Match when={props.vendors === 0}>
          <Peek />
          <h1 class="display">{t('empty.noAiTitle')}</h1>
          <p>{t('empty.noAiBody')}</p>
          <button class="btn primary" data-restore="connect" onClick={() => props.onConnect()}>
            {t('empty.noAiButton')}
          </button>
        </Match>
        <Match when={props.area === 'project' && !props.project}>
          <Peek />
          <h1 class="display">{t('empty.openProjectTitle')}</h1>
          <p>{t('empty.openProjectBody')}</p>
        </Match>
        <Match when={props.area === 'project'}>
          <Peek />
          <h1 class="display">{props.project?.name}</h1>
          <p>{t('empty.projectBody', { path: props.project?.path ?? '' })}</p>
        </Match>
      </Switch>
    </div>
  );
}
