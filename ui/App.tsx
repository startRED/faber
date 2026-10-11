import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { Ambient } from './Ambient.tsx';
import { api, type Appearance, type AppState, type Choice, type Conversation, type Settings as Saved, type Vendor } from './api.ts';
import { inOrder, saveAndApply, watchSystem } from './appearance.ts';
import { Composer, type Draft } from './Composer.tsx';
import { failureText, lang, t, useLanguage } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { Lockup, Peek } from './Mark.tsx';
import { modelOptions, vendorName } from './models.ts';
import { createOpening, upsert } from './opening.ts';
import { Settings } from './Settings.tsx';
import { Setup } from './Setup.tsx';
import { shouldOpenSetup } from './setup.ts';
import { Thread } from './Thread.tsx';
import { mac, WindowControls } from './WindowControls.tsx';

type Area = 'chat' | 'project';
type Quota = { vendor: Vendor; window: string; used: number; resetsAt: number };

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

// Where the user was, so the screen rebuilt after closing to the tray opens in the same place.
type Place = { area: Area; projectId?: string; id?: string };

// A long conversation draws only its latest messages, and more as the user scrolls up, so memory stays flat.
const PAGE = 40;

function untilReset(resetsAt: number): string {
  const hours = (resetsAt * 1000 - Date.now()) / 3_600_000;
  if (hours <= 0) return t('quota.now');
  return hours < 24 ? t('quota.hours', { n: Math.ceil(hours) }) : t('quota.days', { n: Math.ceil(hours / 24) });
}

function windowName(window: string): string {
  const key = `quota.${window}`;
  return key in en ? t(key as Key) : window.replace('_', ' ');
}

/** `initial` is the state the page started from; `main.tsx` has already applied its look. */
export function App(props: { initial: AppState }) {
  const place = recall<Place | undefined>('faber.place', undefined);
  useLanguage(props.initial.settings.language);
  const [state, setState] = createSignal<AppState>(props.initial);
  const [area, setArea] = createSignal<Area>(place?.area ?? 'chat');
  const [projectId, setProjectId] = createSignal<string | undefined>(place?.projectId);
  const [current, setCurrent] = createSignal<Conversation>();
  const [choice, setChoice] = createSignal<Choice>(recall<Choice>('faber.choice', { vendor: 'auto' }));
  const [quotas, setQuotas] = createSignal<Record<string, Quota>>({});
  const [error, setError] = createSignal<string>();
  // The text of the last refused or failed save of settings; the page stays as it was.
  const [saveError, setSaveError] = createSignal<string>();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [setupOpen, setSetupOpen] = createSignal(shouldOpenSetup(props.initial.settings, props.initial.conversations));
  const [confirming, setConfirming] = createSignal<string>();
  const [from, setFrom] = createSignal(0);
  const opening = createOpening();
  let thread: HTMLDivElement | undefined;
  let main: HTMLElement | undefined;
  let stick = true;

  const refresh = () => api.state().then(setState);
  const fail = (err: unknown) => setError(failureText(err));

  onMount(() => {
    if (place?.id && props.initial.conversations.some((c) => c.id === place.id)) void open(place.id);
    const stopFollowing = watchSystem();
    const unlisten = api.on((e) => {
      if (e.type === 'list') void refresh();
      if (e.type === 'quota') setQuotas((q) => ({ ...q, [`${e.vendor}:${e.window}`]: e }));
      if (e.type === 'message' && opening.hear(e.conversationId, e.message) === 'apply') {
        setCurrent((c) => (c?.id === e.conversationId ? upsert(c, e.message) : c));
      }
    });
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setSettingsOpen(false);
        setSetupOpen(false);
        void open(undefined);
      }
      if ((e.ctrlKey || e.metaKey) && e.key === ',') setSettingsOpen(true);
      if (e.key === 'Escape') {
        setSettingsOpen(false);
        setSetupOpen(false);
        setSaveError(undefined);
      }
    };
    window.addEventListener('keydown', onKey);
    onCleanup(() => {
      stopFollowing();
      void unlisten.then((stop) => stop());
      window.removeEventListener('keydown', onKey);
    });
  });

  // The tray menu lives outside the page, so it gets its words from here.
  createEffect(on(lang, () => void api.setTray(t('tray.open'), t('tray.quit')).catch(() => undefined)));

  // Follow the answer while it streams, unless the user scrolled up to read.
  createEffect(
    on(current, () => {
      if (thread && stick) thread.scrollTop = thread.scrollHeight;
    }),
  );

  const open = async (id: string | undefined) => {
    const token = opening.begin(id);
    remember('faber.place', { area: area(), projectId: projectId(), id } satisfies Place);
    setError(undefined);
    stick = true;
    const snapshot = (id ? await api.open(id) : undefined) ?? undefined;
    // Messages that came while the snapshot was on its way are already in it, merged by `opening`.
    const done = opening.finish(token, snapshot);
    if (done.stale) return;
    setFrom(Math.max(0, (done.conversation?.messages.length ?? 0) - PAGE));
    setCurrent(done.conversation);
  };

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
    setError(undefined);
    stick = true;
    try {
      const id = await api.send({ conversationId: current()?.id, ...target(), text: draft.text, images: draft.images, files: draft.files, choice: picked() });
      if (id !== opening.id) await open(id);
    } catch (err) {
      fail(err);
    }
  };

  const branch = async (messageId: string, text: string, picked: Choice) => {
    const c = current();
    if (!c) return;
    setError(undefined);
    try {
      await open(await api.branch({ conversationId: c.id, messageId, text, choice: picked }));
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

  const switchArea = (next: Area) => {
    setSettingsOpen(false);
    setArea(next);
    void open(undefined);
  };
  const addProject = async () => {
    const project = await api.addProject();
    if (project) {
      setProjectId(project.id);
      void open(undefined);
    }
  };
  // The first click asks, the second deletes.
  const remove = async (id: string) => {
    if (confirming() !== id) return setConfirming(id);
    setConfirming(undefined);
    await api.remove(id);
    if (opening.id === id) void open(undefined);
  };

  const options = createMemo(() => (lang(), modelOptions(state().vendors, state().models)));
  const project = () => state().projects.find((p) => p.id === projectId());
  const list = () => state().conversations.filter((c) => c.area === area() && (area() === 'chat' || c.projectId === projectId()));
  const busy = () => current()?.messages.at(-1)?.status === 'streaming';
  const byVendor = () => {
    const best = new Map<Vendor, Quota>();
    for (const q of Object.values(quotas())) if ((best.get(q.vendor)?.used ?? -1) < q.used) best.set(q.vendor, q);
    return [...best.values()];
  };
  const needsProject = () => area() === 'project' && !project();
  // The new conversation of a chat with a connected AI is the one place the effects of a look appear.
  const newChat = () => area() === 'chat' && state().vendors.length > 0 && !current()?.messages.length;
  const composerKey = () => current()?.id ?? `new-${area()}-${projectId() ?? ''}`;

  return (
    <div class="app">
      {/* The top bar is the window's title bar: empty space drags the window, double click maximizes. */}
      <header class="top" classList={{ mac }} data-tauri-drag-region>
        <span class="brand" data-tauri-drag-region>
          <Lockup height={20} />
        </span>
        <div class="toggle" role="group" aria-label={t('area.label')}>
          <button aria-pressed={area() === 'chat'} onClick={() => switchArea('chat')}>
            {t('area.chat')}
          </button>
          <button aria-pressed={area() === 'project'} onClick={() => switchArea('project')}>
            {t('area.projects')}
          </button>
        </div>
        <div class="quota" aria-label={t('quota.label')} data-tauri-drag-region>
          <For each={byVendor()}>
            {(q) => (
              <span class="q" title={t('quota.window', { window: windowName(q.window) })}>
                {vendorName(q.vendor)} <i style={{ '--v': `${Math.round(q.used * 100)}%` }} /> {Math.round(q.used * 100)}% · {untilReset(q.resetsAt)}
              </span>
            )}
          </For>
        </div>
        <button class="gear" aria-pressed={settingsOpen()} onClick={() => setSettingsOpen((o) => !o)} title={t('settings.shortcut')}>
          {t('settings.open')}
        </button>
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
        <div class="body">
          <aside class="side">
            <nav class="nav">
              <Show
                when={area() === 'project'}
                fallback={
                  <button class="item new" onClick={() => void open(undefined)}>
                    <span>{t('side.newChat')}</span>
                    <kbd>Ctrl N</kbd>
                  </button>
                }
              >
                <button class="item" onClick={() => void addProject()}>
                  <span>{t('side.addProject')}</span>
                </button>
                <For each={state().projects}>
                  {(p) => (
                    <button class="item" aria-current={p.id === projectId()} title={p.path} onClick={() => (setProjectId(p.id), void open(undefined))}>
                      <span>{p.name}</span>
                    </button>
                  )}
                </For>
                <Show when={project()}>
                  <button class="item" onClick={() => void open(undefined)}>
                    <span>{t('side.newConversation')}</span>
                  </button>
                </Show>
              </Show>
            </nav>
            <div class="list" onMouseLeave={() => setConfirming(undefined)}>
              <Show when={list().length > 0}>
                <small>{area() === 'chat' ? t('side.chats') : project()?.name}</small>
              </Show>
              <For each={list()}>
                {(c) => (
                  <div class="item" aria-current={c.id === current()?.id} role="button" tabIndex={0} onClick={() => void open(c.id)} onKeyDown={(e) => e.key === 'Enter' && void open(c.id)}>
                    <span>{c.title}</span>
                    <button class="x" classList={{ confirm: confirming() === c.id }} aria-label={t('side.delete', { title: c.title })} onClick={(e) => (e.stopPropagation(), void remove(c.id))}>
                      {confirming() === c.id ? t('side.confirmDelete') : '×'}
                    </button>
                  </div>
                )}
              </For>
            </div>
            <div class="foot">
              {state().vendors.length ? state().vendors.map(vendorName).join(' · ') : t('side.noAi')} · v{state().version}
            </div>
          </aside>
          <main class="main" ref={(el) => (main = el)}>
            <Show when={newChat()}>
              {/* The effects of the look sit behind the greeting; dust keeps clear of the text and the composer. */}
              <Ambient vendors={state().vendors} avoid={() => [...main!.querySelectorAll('.empty h1, .empty p, .composer')]} />
            </Show>
            <div class="thread" classList={{ welcome: !current()?.messages.length }} ref={(el) => (thread = el)} onScroll={(e) => onScroll(e.currentTarget)}>
              <div class="col">
                <Show when={current()?.messages.length ? current() : undefined} fallback={<Empty vendors={state().vendors.length} area={area()} project={project()} />}>
                  {(c) => (
                    <Thread
                      messages={c().messages}
                      from={from()}
                      conversationId={c().id}
                      options={options()}
                      onAnswer={(requestId, allow, always) => void api.answer(c().id, requestId, allow, always)}
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
            </div>
            <Show when={!needsProject()}>
              <Show when={composerKey()} keyed>
                <Composer
                  vendors={state().vendors}
                  options={options()}
                  choice={picked()}
                  onChoice={pickChoice}
                  busy={busy()}
                  placeholder={area() === 'chat' ? t('composer.chatPlaceholder') : t('composer.projectPlaceholder')}
                  onSend={(d) => void send(d)}
                  onStop={() => current() && void api.interrupt(current()!.id)}
                  onWarm={() => void api.warm({ conversationId: current()?.id, ...target(), choice: picked() })}
                  onPause={(text) => {
                    if (picked().vendor === 'auto') void api.routeDraft({ conversationId: current()?.id, ...target(), choice: picked() }, text);
                  }}
                />
              </Show>
            </Show>
          </main>
        </div>
      </Show>
    </Show>
  </div>
  );
}

function Empty(props: { vendors: number; area: Area; project?: { name: string; path: string } }) {
  return (
    <div class="empty">
      <Switch>
        <Match when={props.vendors === 0}>
          <h1 class="display">{t('empty.noAiTitle')}</h1>
          <p>{t('empty.noAiBody')}</p>
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
        <Match when={true}>
          <h1 class="display">{t('empty.chatTitle')}</h1>
          <p>{t('empty.chatBody')}</p>
        </Match>
      </Switch>
    </div>
  );
}
