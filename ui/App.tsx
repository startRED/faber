import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { api, errorText, type AppState, type Choice, type Conversation, type Message, type Settings as Saved, type Vendor } from './api.ts';
import { applyAppearance, followSystem, toAppearance, type Appearance } from './appearance.ts';
import { Composer, type Draft } from './Composer.tsx';
import { fromEngine, lang, t, useLanguage } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { modelOptions, vendorName } from './models.ts';
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

function upsert(c: Conversation, m: Message): Conversation {
  const i = c.messages.findIndex((x) => x.id === m.id);
  const messages = i < 0 ? [...c.messages, m] : c.messages.map((x, j) => (j === i ? m : x));
  return { ...c, messages };
}

export function App() {
  const [state, setState] = createSignal<AppState>({ conversations: [], projects: [], vendors: [], settings: {}, models: [], version: '' });
  const [loaded, setLoaded] = createSignal(false);
  const [area, setArea] = createSignal<Area>('chat');
  const [projectId, setProjectId] = createSignal<string>();
  const [current, setCurrent] = createSignal<Conversation>();
  const [choice, setChoice] = createSignal<Choice>(recall<Choice>('faber.choice', { vendor: 'auto' }));
  const [quotas, setQuotas] = createSignal<Record<string, Quota>>({});
  const [error, setError] = createSignal<string>();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [setupOpen, setSetupOpen] = createSignal(false);
  const [appearance, setAppearance] = createSignal<Appearance>(toAppearance(undefined));
  const [confirming, setConfirming] = createSignal<string>();
  const [from, setFrom] = createSignal(0);
  let currentId: string | undefined;
  let thread: HTMLDivElement | undefined;
  let stick = true;

  const refresh = () => api.state().then((s) => (setState(s), setLoaded(true)));
  const fail = (err: unknown) => setError(fromEngine(errorText(err)));

  onMount(() => {
    void api.state().then((s) => {
      const a = toAppearance(s.settings.appearance);
      setAppearance(a);
      applyAppearance(a);
      useLanguage(s.settings.language);
      setState(s);
      setLoaded(true);
      if (shouldOpenSetup(s.settings, s.conversations)) {
        setSetupOpen(true);
      }
      const place = recall<Place | undefined>('faber.place', undefined);
      if (place) {
        setArea(place.area);
        setProjectId(place.projectId);
        if (place.id && s.conversations.some((c) => c.id === place.id)) void open(place.id);
      }
    });
    const stopFollowing = followSystem(() => appearance());
    const unlisten = api.on((e) => {
      if (e.type === 'list') void refresh();
      if (e.type === 'quota') setQuotas((q) => ({ ...q, [`${e.vendor}:${e.window}`]: e }));
      if (e.type === 'message' && e.conversationId === currentId) setCurrent((c) => c && upsert(c, e.message));
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
    currentId = id;
    remember('faber.place', { area: area(), projectId: projectId(), id } satisfies Place);
    setError(undefined);
    stick = true;
    const c = (id ? await api.open(id) : undefined) ?? undefined;
    setFrom(Math.max(0, (c?.messages.length ?? 0) - PAGE));
    setCurrent(c);
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
      if (id !== currentId) await open(id);
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

  const changeAppearance = (a: Appearance) => {
    setAppearance(a);
    applyAppearance(a);
    void api.saveSettings({ appearance: { ...a } });
  };
  // The switch moves at once; the file is written in the background.
  const saveSettings = (patch: Saved) => {
    setState((s) => ({ ...s, settings: { ...s.settings, ...patch } }));
    if (patch.language !== undefined) useLanguage(patch.language);
    void api.saveSettings(patch);
  };

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
    if (currentId === id) void open(undefined);
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
  const composerKey = () => current()?.id ?? `new-${area()}-${projectId() ?? ''}`;

  return (
    <div class="app">
      {/* The top bar is the window's title bar: empty space drags the window, double click maximizes. */}
      <header class="top" classList={{ mac }} data-tauri-drag-region>
        <strong data-tauri-drag-region>Faber</strong>
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
      <Show
        when={!setupOpen()}
        fallback={
          <Setup
            onClose={() => setSetupOpen(false)}
            onFinish={(result) => {
              saveSettings({ setup: result });
              setSetupOpen(false);
              void refresh();
            }}
          />
        }
      >
        <Show
          when={!settingsOpen()}
          fallback={
            <Settings
              appearance={appearance()}
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
                  <button class="item" onClick={() => void open(undefined)}>
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
          <main class="main">
            <div class="thread" ref={(el) => (thread = el)} onScroll={(e) => onScroll(e.currentTarget)}>
              <div class="col">
                <Show when={current()?.messages.length ? current() : undefined} fallback={<Empty loaded={loaded()} vendors={state().vendors.length} area={area()} project={project()} />}>
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

function Empty(props: { loaded: boolean; vendors: number; area: Area; project?: { name: string; path: string } }) {
  return (
    <div class="empty">
      <Show when={props.loaded}>
        <Switch>
          <Match when={props.vendors === 0}>
            <h1>{t('empty.noAiTitle')}</h1>
            <p>{t('empty.noAiBody')}</p>
          </Match>
          <Match when={props.area === 'project' && !props.project}>
            <h1>{t('empty.openProjectTitle')}</h1>
            <p>{t('empty.openProjectBody')}</p>
          </Match>
          <Match when={props.area === 'project'}>
            <h1>{props.project?.name}</h1>
            <p>{t('empty.projectBody', { path: props.project?.path ?? '' })}</p>
          </Match>
          <Match when={true}>
            <h1>{t('empty.chatTitle')}</h1>
            <p>{t('empty.chatBody')}</p>
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
