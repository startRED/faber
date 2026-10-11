import { createEffect, createMemo, createSignal, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { Ambient } from './Ambient.tsx';
import { api, type Appearance, type AppState, type Choice, type Conversation, type RunningItem, type Settings as Saved, type Summary } from './api.ts';
import { inOrder, saveAndApply, watchSystem } from './appearance.ts';
import { Composer, type Draft } from './Composer.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { failureText, lang, t, templates, useLanguage } from './i18n.ts';
import { createWidth } from './layout.ts';
import { Lockup, Peek } from './Mark.tsx';
import { modelOptions, prettyModel } from './models.ts';
import { createOpening, upsert } from './opening.ts';
import { Running } from './Running.tsx';
import { createAutoOpen, openPlace, type Place, placeOfRunning, store, waitingKeys } from './running.ts';
import { Settings } from './Settings.tsx';
import { Setup } from './Setup.tsx';
import { shouldOpenSetup } from './setup.ts';
import { Sidebar } from './Sidebar.tsx';
import { seedQuotas, withQuota } from './sidebar.ts';
import { Thread } from './Thread.tsx';
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
const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/** Tab and Shift+Tab stay inside an open modal dialog: past its last stop they go back to its first, and the other way. */
function keepFocusIn(e: KeyboardEvent, dialog: HTMLElement | undefined) {
  if (!dialog) return;
  const stops = [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => !(el as HTMLButtonElement).disabled && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden',
  );
  const first = stops[0];
  const last = stops.at(-1);
  if (!first || !last) return;
  const at = document.activeElement;
  const inside = !!at && dialog.contains(at);
  if (e.shiftKey ? !inside || at === first : !inside || at === last) {
    e.preventDefault();
    (e.shiftKey ? last : first).focus();
  }
}

/** `initial` is the state the page started from; `main.tsx` has already applied its look and handed the running list to
 * the store. */
export function App(props: { initial: AppState }) {
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
  const [setupOpen, setSetupOpen] = createSignal(shouldOpenSetup(props.initial.settings, props.initial.conversations));
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

  const area = () => place().area;
  const projectId = () => place().projectId;
  const refresh = () =>
    api.state().then((s) => {
      store.offer(s.running);
      setState(s);
    });
  const fail = (err: unknown) => setError(failureText(err));
  const items = () => store.list().items;
  const needing = () => items().filter((i) => i.state === 'needsYou').length;

  // ---- The sidebar: beside the stage from 900 px, behind a button below that. Opened, it is modal.
  const [sideOpen, setSideOpen] = createSignal(false);
  const narrow = () => width() === 'narrow';
  const sideOverlay = () => narrow() && sideOpen();
  let sideEl: HTMLElement | undefined;
  let menuButton: HTMLButtonElement | undefined;
  const openSide = () => {
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
    setSettingsOpen(false);
    closeSide(false);
    void go(openPlace(place(), c));
  };
  // A row of the running column: the area, the project and the conversation change together, so the crumb, the sidebar,
  // the composer and the next send all point at the same place. A conversation that is gone does nothing.
  const openRunning = (item: RunningItem) => {
    if (!state().conversations.some((c) => c.id === item.conversationId)) return;
    setSettingsOpen(false);
    if (!docked()) closeColumn();
    void go(placeOfRunning(place(), item));
  };

  onMount(() => {
    if (saved?.id && props.initial.conversations.some((c) => c.id === saved.id)) void go({ ...saved, area: saved.area ?? 'chat' });
    const stopFollowing = watchSystem();
    const unlisten = api.on((e) => {
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
      const inShell = !settingsOpen() && !setupOpen();
      if (e.key === 'Tab' && inShell && !ctrl && !e.altKey) {
        if (sideOverlay()) keepFocusIn(e, sideEl);
        else if (trapped()) keepFocusIn(e, column);
      }
      if (ctrl && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setSettingsOpen(false);
        setSetupOpen(false);
        closeSide(false);
        // A modal column would leave the new conversation's composer out of reach.
        if (trapped()) {
          setColumnOpen(false);
          setByUser(false);
        }
        void fresh(place()).then(focusComposer);
      }
      if (ctrl && e.key === ',') setSettingsOpen(true);
      if (ctrl && e.key === '.') {
        e.preventDefault();
        toggleColumn();
      }
      if (e.key === 'Escape') {
        if (sideOverlay() && inShell) return closeSide();
        // A column the user opened over the stage is modal; one that opened itself answers Esc only while focus is in it.
        if (inShell && columnOpen() && (trapped() || column?.contains(document.activeElement))) return closeColumn();
        setSettingsOpen(false);
        setSetupOpen(false);
        setSaveError(undefined);
      }
    };
    const typingEvents = ['keydown', 'input', 'compositionstart', 'compositionupdate', 'compositionend'];
    for (const type of typingEvents) window.addEventListener(type, typing, true);
    window.addEventListener('keydown', onKey);
    onCleanup(() => {
      stopFollowing();
      void unlisten.then((stop) => stop());
      clearInterval(minute);
      clearTimeout(retry);
      for (const type of typingEvents) window.removeEventListener(type, typing, true);
      window.removeEventListener('keydown', onKey);
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
  const changeAppearance = (next: (current: Appearance | undefined) => Appearance) =>
    inTurn(() => persist({ appearance: next(state().settings.appearance) }));

  const switchArea = (next: Place['area']) => {
    setSettingsOpen(false);
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
        placeholder={area() === 'chat' ? t('composer.chatPlaceholder') : t('composer.projectPlaceholder')}
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
        when={!setupOpen()}
        fallback={
          <Setup
            onClose={() => setSetupOpen(false)}
            onFinish={(result) =>
              void saveSettings({ setup: result }).then((saved) => {
                if (!saved) return;
                setSetupOpen(false);
                void refresh();
              })
            }
          />
        }
      >
        <Show
          when={!settingsOpen()}
          fallback={
            <Settings
              look={state().look}
              saved={state().settings}
              onAppearance={changeAppearance}
              onSave={saveSettings}
              onClose={() => setSettingsOpen(false)}
              onOpenSetup={() => {
                setSettingsOpen(false);
                setSetupOpen(true);
              }}
            />
          }
        >
          <div class="shell" classList={{ docked: columnOpen() && docked() }}>
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
                onSettings={() => (closeSide(false), setSettingsOpen(true))}
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
                    <Match when={true}>
                      <b>{t('crumb.new')}</b>
                    </Match>
                  </Switch>
                </div>
                <div class="right">
                  <Show when={state().vendors.length > 0}>
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
                          fallback={<Empty vendors={state().vendors.length} area={area()} project={project()} onConnect={() => setSetupOpen(true)} />}
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
                      recent={state().conversations.slice(0, 3)}
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
          <button class="btn primary" onClick={() => props.onConnect()}>
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
