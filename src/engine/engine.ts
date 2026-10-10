import { randomUUID } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import type { Area } from './claude/args.ts';
import { startClaude } from './claude/session.ts';
import { startCodex } from './codex/session.ts';
import type { EngineEvent, Session } from './events.ts';
import { realSpawn, type Spawner } from './proc.ts';
import type { Cli } from './resolve-cli.ts';
import { pickModel, type Choice } from './router.ts';
import { createStore, type Conversation, type Message, type Project, type Summary, type Vendor } from './store.ts';

export type AppEvent =
  | { type: 'message'; conversationId: string; message: Message }
  | { type: 'quota'; vendor: Vendor; window: string; used: number; resetsAt: number }
  | { type: 'list' };

export interface EngineOptions {
  dataDir: string;
  clis: Partial<Record<Vendor, Cli>>;
  spawn?: Spawner;
  platform?: NodeJS.Platform;
  maxWarm?: number;
  now?: () => number;
}

export interface SendRequest {
  conversationId?: string;
  area: Area;
  projectId?: string;
  text: string;
  images?: { mediaType: string; base64: string }[];
  choice: Choice;
}

interface Live {
  vendor: Vendor;
  model?: string;
  conversation: Conversation;
  session: Session;
  turn?: Message;
}

const HISTORY_LIMIT = 20_000;

// A model that joins a conversation midway reads what was said before, as plain text.
export function withHistory(history: Message[], text: string): string {
  const lines = history.map((m) => `${m.role === 'user' ? 'User' : 'Assistant'}: ${m.text}`).join('\n\n');
  return `Earlier in this conversation, with another assistant:\n\n${lines.slice(-HISTORY_LIMIT)}\n\nNow the user says:\n${text}`;
}

export function createEngine(opts: EngineOptions, emit: (event: AppEvent) => void) {
  const store = createStore(opts.dataDir);
  const now = opts.now ?? Date.now;
  const chatHome = join(opts.dataDir, 'chat-home');
  mkdirSync(chatHome, { recursive: true });
  const live = new Map<string, Live>();
  const timers = new Map<string, NodeJS.Timeout>();
  const vendors = Object.keys(opts.clis) as Vendor[];

  // Streaming text is sent to the screen at most ~30 times a second; anything else goes at once.
  const push = (conversationId: string, message: Message | undefined, immediate = false) => {
    if (!message) return;
    const send = () => {
      timers.delete(conversationId);
      emit({ type: 'message', conversationId, message });
    };
    if (immediate) {
      clearTimeout(timers.get(conversationId));
      send();
    } else if (!timers.has(conversationId)) {
      timers.set(conversationId, setTimeout(send, 33));
    }
  };

  const cwdOf = (c: Conversation) => {
    if (c.area === 'chat') return chatHome;
    const project = store.projects().find((p) => p.id === c.projectId);
    if (!project) throw new Error('This project is no longer in Faber.');
    return project.path;
  };

  function handle(entry: Live, e: EngineEvent): void {
    const c = entry.conversation;
    const m = entry.turn;
    switch (e.type) {
      case 'started':
        c.sessions[entry.vendor] = e.sessionId;
        if (m) m.model = e.model;
        store.save(c);
        break;
      case 'text':
        if (m) m.text += e.delta;
        break;
      case 'thinking':
        if (m) m.thinking = (m.thinking ?? '') + e.delta;
        break;
      case 'tool':
        m?.tools?.push({ id: e.id, name: e.name, input: e.input });
        break;
      case 'tool-result': {
        const tool = m?.tools?.find((t) => t.id === e.id);
        if (tool) Object.assign(tool, { ok: e.ok, output: e.text.slice(0, 4000) });
        break;
      }
      case 'permission':
        if (m) m.permission = { requestId: e.requestId, tool: e.tool, input: e.input };
        return push(c.id, m, true);
      case 'quota':
        return emit({ type: 'quota', vendor: entry.vendor, window: e.window, used: e.used, resetsAt: e.resetsAt });
      case 'done':
        if (m) Object.assign(m, { status: e.ok ? 'done' : 'stopped', costUsd: e.costUsd, inputTokens: e.inputTokens, outputTokens: e.outputTokens, permission: undefined });
        return finish(entry);
      case 'error':
        if (!e.fatal) {
          if (m) m.error = e.message;
          break;
        }
        if (m) Object.assign(m, { status: 'error', error: e.message, permission: undefined });
        live.delete(c.id);
        return finish(entry);
    }
    push(c.id, m);
  }

  function finish(entry: Live): void {
    const c = entry.conversation;
    const m = entry.turn;
    entry.turn = undefined;
    c.updatedAt = now();
    store.save(c);
    push(c.id, m, true);
    emit({ type: 'list' });
  }

  function launch(c: Conversation, vendor: Vendor, model: string | undefined): Live {
    const current = live.get(c.id);
    if (current && current.vendor === vendor && current.model === model) {
      live.delete(c.id);
      live.set(c.id, current); // most recently used goes last
      return current;
    }
    current?.session.close();
    const cli = opts.clis[vendor]!;
    const spawn: Spawner = (_bin, args, cwd) => (opts.spawn ?? realSpawn)(cli.command, [...cli.prefix, ...args], cwd);
    const entry = { vendor, model, conversation: c } as Live;
    const onEvent = (e: EngineEvent) => handle(entry, e);
    const common = { bin: cli.command, cwd: cwdOf(c), spawn };
    entry.session =
      vendor === 'claude'
        ? startClaude({ ...common, launch: { area: c.area, model, resume: c.sessions.claude } }, onEvent)
        : startCodex({ ...common, platform: opts.platform, launch: { area: c.area, model, resume: c.sessions.codex } }, onEvent);
    live.delete(c.id);
    live.set(c.id, entry);
    // ponytail: count-based warm limit; switch to the visible RAM budget once memory is measured.
    for (const [id, l] of live) {
      if (live.size <= (opts.maxWarm ?? 3)) break;
      if (!l.turn) {
        l.session.close();
        live.delete(id);
      }
    }
    return entry;
  }

  const load = (id: string) => live.get(id)?.conversation ?? store.get(id);

  return {
    vendors,
    state(): { conversations: Summary[]; projects: Project[]; vendors: Vendor[] } {
      return { conversations: store.list().filter((c) => c.title), projects: store.projects(), vendors };
    },
    open(id: string): Conversation | undefined {
      return load(id);
    },
    send(req: SendRequest): string {
      const pick = pickModel(req.choice, vendors);
      if (!pick) throw new Error('No connected subscription can answer this. Connect one in Settings.');
      const existing = req.conversationId ? load(req.conversationId) : undefined;
      if (existing && live.get(existing.id)?.turn) throw new Error('Wait for the current answer or stop it first.');
      const at = now();
      const c: Conversation = existing ?? {
        id: randomUUID(),
        title: req.text.trim().replace(/\s+/g, ' ').slice(0, 60),
        area: req.area,
        projectId: req.projectId,
        sessions: {},
        createdAt: at,
        updatedAt: at,
        messages: [],
      };
      const joiningLate = !c.sessions[pick.vendor] && c.messages.length > 0;
      const history = c.messages.slice();
      const reply: Message = { id: randomUUID(), role: 'assistant', text: '', at, vendor: pick.vendor, model: pick.model, reason: pick.reason, tools: [], status: 'streaming' };
      c.messages.push({ id: randomUUID(), role: 'user', text: req.text, at, images: req.images }, reply);
      c.updatedAt = at;
      store.save(c);
      emit({ type: 'list' });
      const entry = launch(c, pick.vendor, pick.model);
      entry.turn = reply;
      entry.session.send({ text: joiningLate ? withHistory(history, req.text) : req.text, images: req.images });
      push(c.id, reply, true);
      return c.id;
    },
    // Starts the CLI before the user finishes typing, so the first word arrives sooner.
    warm(conversationId: string, choice: Choice): void {
      const c = load(conversationId);
      const pick = pickModel(choice, vendors);
      if (c && pick && !live.get(c.id)?.turn) launch(c, pick.vendor, pick.model);
    },
    interrupt(conversationId: string): void {
      live.get(conversationId)?.session.interrupt();
    },
    answer(conversationId: string, requestId: string, allow: boolean): void {
      const entry = live.get(conversationId);
      if (!entry) return;
      entry.session.answer(requestId, allow);
      if (entry.turn?.permission?.requestId === requestId) entry.turn.permission = undefined;
      push(conversationId, entry.turn, true);
    },
    rename(id: string, title: string): void {
      const c = load(id);
      if (!c) return;
      c.title = title.trim().slice(0, 120) || c.title;
      store.save(c);
      emit({ type: 'list' });
    },
    remove(id: string): void {
      live.get(id)?.session.close();
      live.delete(id);
      store.remove(id);
      emit({ type: 'list' });
    },
    addProject(path: string): Project {
      const projects = store.projects();
      const found = projects.find((p) => p.path === path);
      if (found) return found;
      const project = { id: randomUUID(), name: basename(path) || path, path };
      store.saveProjects([...projects, project]);
      emit({ type: 'list' });
      return project;
    },
    removeProject(id: string): void {
      store.saveProjects(store.projects().filter((p) => p.id !== id));
      emit({ type: 'list' });
    },
    shutdown(): void {
      for (const entry of live.values()) entry.session.close();
      live.clear();
    },
  };
}

export type Engine = ReturnType<typeof createEngine>;
