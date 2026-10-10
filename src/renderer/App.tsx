import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { api, type AppState, type Choice, type Conversation, type Message, type Vendor } from './api.ts';
import { Composer, type Draft } from './Composer.tsx';
import { vendorName } from './models.ts';
import { Thread } from './Thread.tsx';

type Area = 'chat' | 'project';
type Quota = { vendor: Vendor; window: string; used: number; resetsAt: number };

function loadChoice(): Choice {
  try {
    return JSON.parse(localStorage.getItem('faber.choice') ?? '') as Choice;
  } catch {
    return { vendor: 'auto' };
  }
}

function untilReset(resetsAt: number): string {
  const hours = (resetsAt * 1000 - Date.now()) / 3_600_000;
  if (hours <= 0) return 'now';
  return hours < 24 ? `${Math.ceil(hours)}h` : `${Math.ceil(hours / 24)}d`;
}

function upsert(c: Conversation, m: Message): Conversation {
  const i = c.messages.findIndex((x) => x.id === m.id);
  const messages = i < 0 ? [...c.messages, m] : c.messages.map((x, j) => (j === i ? m : x));
  return { ...c, messages };
}

export function App({ version }: { version: string }) {
  const [state, setState] = useState<AppState>({ conversations: [], projects: [], vendors: [] });
  const [loaded, setLoaded] = useState(false);
  const [area, setArea] = useState<Area>('chat');
  const [projectId, setProjectId] = useState<string>();
  const [current, setCurrent] = useState<Conversation>();
  const [choice, setChoice] = useState<Choice>(loadChoice);
  const [quotas, setQuotas] = useState<Record<string, Quota>>({});
  const [error, setError] = useState<string>();
  const currentId = useRef<string | undefined>(undefined);
  const thread = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  const refresh = useCallback(() => api.state().then((s) => (setState(s), setLoaded(true))), []);
  useEffect(() => {
    void refresh();
    return api.on((e) => {
      if (e.type === 'list') void refresh();
      if (e.type === 'quota') setQuotas((q) => ({ ...q, [`${e.vendor}:${e.window}`]: e }));
      if (e.type === 'message' && e.conversationId === currentId.current) setCurrent((c) => c && upsert(c, e.message));
    });
  }, [refresh]);

  // Follow the answer while it streams, unless the user scrolled up to read.
  useLayoutEffect(() => {
    const el = thread.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [current]);

  const open = useCallback(async (id: string | undefined) => {
    currentId.current = id;
    setError(undefined);
    stick.current = true;
    setCurrent(id ? await api.open(id) : undefined);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        void open(undefined);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);

  const pickChoice = (c: Choice) => {
    setChoice(c);
    try {
      localStorage.setItem('faber.choice', JSON.stringify(c));
    } catch {
      // per-viewer convenience only
    }
  };

  const send = async (draft: Draft) => {
    setError(undefined);
    stick.current = true;
    try {
      const id = await api.send({ conversationId: current?.id, area, projectId: area === 'project' ? projectId : undefined, text: draft.text, images: draft.images, files: draft.files, choice });
      if (id !== currentId.current) await open(id);
    } catch (err) {
      setError(err instanceof Error ? err.message.replace(/^Error invoking remote method 'faber': (Error: )?/, '') : String(err));
    }
  };

  const switchArea = (next: Area) => {
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
  const remove = async (id: string) => {
    if (!window.confirm('Delete this conversation?')) return;
    await api.remove(id);
    if (currentId.current === id) void open(undefined);
  };

  const project = state.projects.find((p) => p.id === projectId);
  const list = state.conversations.filter((c) => c.area === area && (area === 'chat' || c.projectId === projectId));
  const busy = current?.messages.at(-1)?.status === 'streaming';
  const byVendor = new Map<Vendor, Quota>();
  for (const q of Object.values(quotas)) if ((byVendor.get(q.vendor)?.used ?? -1) < q.used) byVendor.set(q.vendor, q);
  const needsProject = area === 'project' && !project;

  return (
    <div className="app">
      <header className="top">
        <strong>Faber</strong>
        <div className="toggle" role="group" aria-label="Area">
          <button aria-pressed={area === 'chat'} onClick={() => switchArea('chat')}>
            Chat
          </button>
          <button aria-pressed={area === 'project'} onClick={() => switchArea('project')}>
            Projects
          </button>
        </div>
        <div className="quota" aria-label="Quota">
          {[...byVendor.values()].map((q) => (
            <span key={q.vendor} className="q" title={`${q.window.replace('_', ' ')} window`}>
              {vendorName(q.vendor)} <i style={{ ['--v' as string]: `${Math.round(q.used * 100)}%` }} /> {Math.round(q.used * 100)}% · {untilReset(q.resetsAt)}
            </span>
          ))}
        </div>
      </header>
      <div className="body">
        <aside className="side">
          <nav className="nav">
            {area === 'chat' ? (
              <button className="item" onClick={() => void open(undefined)}>
                <span>+ New chat</span>
                <kbd>Ctrl N</kbd>
              </button>
            ) : (
              <>
                <button className="item" onClick={() => void addProject()}>
                  <span>+ Add project folder</span>
                </button>
                {state.projects.map((p) => (
                  <button key={p.id} className="item" aria-current={p.id === projectId} title={p.path} onClick={() => (setProjectId(p.id), void open(undefined))}>
                    <span>{p.name}</span>
                  </button>
                ))}
                {project && (
                  <button className="item" onClick={() => void open(undefined)}>
                    <span>+ New conversation</span>
                  </button>
                )}
              </>
            )}
          </nav>
          <div className="list">
            {list.length > 0 && <small>{area === 'chat' ? 'Chats' : project?.name}</small>}
            {list.map((c) => (
              <div key={c.id} className="item" aria-current={c.id === current?.id} role="button" tabIndex={0} onClick={() => void open(c.id)} onKeyDown={(e) => e.key === 'Enter' && void open(c.id)}>
                <span>{c.title}</span>
                <button className="x" aria-label={`Delete ${c.title}`} onClick={(e) => (e.stopPropagation(), void remove(c.id))}>
                  ×
                </button>
              </div>
            ))}
          </div>
          <div className="foot">
            {state.vendors.length ? state.vendors.map(vendorName).join(' · ') : 'No AI connected'} · v{version}
          </div>
        </aside>
        <main className="main">
          <div className="thread" ref={thread} onScroll={(e) => (stick.current = e.currentTarget.scrollHeight - e.currentTarget.scrollTop - e.currentTarget.clientHeight < 40)}>
            <div className="col">
              {current?.messages.length ? (
                <Thread messages={current.messages} conversationId={current.id} onAnswer={(requestId, allow, always) => void api.answer(current.id, requestId, allow, always)} />
              ) : (
                <div className="empty">
                  {!loaded ? null : state.vendors.length === 0 ? (
                    <>
                      <h1>No AI found</h1>
                      <p>Install Claude Code or Codex and sign in with your subscription, then reopen Faber.</p>
                    </>
                  ) : needsProject ? (
                    <>
                      <h1>Open a project</h1>
                      <p>Add a folder and Faber works inside it, like a coding agent.</p>
                    </>
                  ) : area === 'project' ? (
                    <>
                      <h1>{project?.name}</h1>
                      <p>Faber works inside {project?.path}</p>
                    </>
                  ) : (
                    <>
                      <h1>What can I help with?</h1>
                      <p>Chat never changes files on your computer.</p>
                    </>
                  )}
                </div>
              )}
              {error && <div className="note err" role="alert">{error}</div>}
            </div>
          </div>
          {!needsProject && (
            <Composer
              key={current?.id ?? `new-${area}-${projectId}`}
              vendors={state.vendors}
              choice={choice}
              onChoice={pickChoice}
              busy={busy}
              placeholder={area === 'chat' ? 'Ask anything' : 'Ask Faber to do something in this folder'}
              onSend={(d) => void send(d)}
              onStop={() => current && void api.interrupt(current.id)}
              onWarm={() => void api.warm({ conversationId: current?.id, area, projectId: area === 'project' ? projectId : undefined, choice })}
            />
          )}
        </main>
      </div>
    </div>
  );
}
