import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { basename, join } from 'node:path';
import type { Area } from './claude/args.ts';
import { startClaude } from './claude/session.ts';
import { startCodex } from './codex/session.ts';
import type { EngineEvent, Session } from './events.ts';
import { realSpawn, type Spawner } from './proc.ts';
import type { Cli } from './resolve-cli.ts';
import { pickModel, type Choice, type Pick } from './router.ts';
import { createStore, type Conversation, type Message, type Project, type Summary, type Vendor } from './store.ts';
import { insideFolder, newFiles } from './workspace.ts';

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
  effort: string;
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
  // Each chat conversation gets a private workspace folder: code runs and files are created there, never elsewhere.
  const workspaceOf = (id: string) => join(opts.dataDir, 'chat', id);
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
    if (c.area === 'chat') {
      mkdirSync(workspaceOf(c.id), { recursive: true });
      return workspaceOf(c.id);
    }
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
        if (c.messages.length) store.save(c); // a draft is saved only once something is sent
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
        if (c.allowAlways?.includes(e.tool)) return entry.session.answer(e.requestId, true);
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
    if (m && c.area === 'chat') {
      const files = newFiles(workspaceOf(c.id), m.at);
      if (files.length) m.files = files;
    }
    c.updatedAt = now();
    store.save(c);
    push(c.id, m, true);
    emit({ type: 'list' });
  }

  function launch(c: Conversation, { vendor, model, effort }: Pick): Live {
    const current = live.get(c.id);
    if (current && current.vendor === vendor && current.model === model && current.effort === effort) {
      live.delete(c.id);
      live.set(c.id, current); // most recently used goes last
      return current;
    }
    current?.session.close();
    const cli = opts.clis[vendor]!;
    const spawn: Spawner = (_bin, args, cwd) => (opts.spawn ?? realSpawn)(cli.command, [...cli.prefix, ...args], cwd);
    const entry = { vendor, model, effort, conversation: c } as Live;
    const onEvent = (e: EngineEvent) => handle(entry, e);
    const common = { bin: cli.command, cwd: cwdOf(c), spawn };
    entry.session =
      vendor === 'claude'
        ? startClaude({ ...common, launch: { area: c.area, model, effort, resume: c.sessions.claude } }, onEvent)
        : startCodex({ ...common, platform: opts.platform, launch: { area: c.area, model, effort, resume: c.sessions.codex } }, onEvent);
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
  const blank = (area: Area, projectId: string | undefined, at: number): Conversation => ({ id: randomUUID(), title: '', area, projectId, sessions: {}, createdAt: at, updatedAt: at, messages: [] });
  // A new conversation still being typed: its CLI starts before the first message is sent.
  let draft: Conversation | undefined;

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
      const reuseDraft = !existing && draft?.area === req.area && draft.projectId === req.projectId;
      const c = existing ?? (reuseDraft ? draft! : blank(req.area, req.projectId, at));
      if (reuseDraft) draft = undefined;
      if (!c.title) c.title = req.text.trim().replace(/\s+/g, ' ').slice(0, 60);
      const joiningLate = !c.sessions[pick.vendor] && c.messages.length > 0;
      const history = c.messages.slice();
      const reply: Message = { id: randomUUID(), role: 'assistant', text: '', at, vendor: pick.vendor, model: pick.model, reason: pick.reason, tools: [], status: 'streaming' };
      c.messages.push({ id: randomUUID(), role: 'user', text: req.text, at, images: req.images }, reply);
      c.updatedAt = at;
      store.save(c);
      emit({ type: 'list' });
      const entry = launch(c, pick);
      entry.turn = reply;
      entry.session.send({ text: joiningLate ? withHistory(history, req.text) : req.text, images: req.images });
      push(c.id, reply, true);
      return c.id;
    },
    // Starts the CLI before the user finishes typing, so the first word arrives sooner.
    warm(req: { conversationId?: string; area: Area; projectId?: string; choice: Choice }): void {
      const pick = pickModel(req.choice, vendors);
      if (!pick) return;
      let c = req.conversationId ? load(req.conversationId) : undefined;
      if (!c) {
        if (!draft || draft.area !== req.area || draft.projectId !== req.projectId) {
          if (draft) {
            live.get(draft.id)?.session.close();
            live.delete(draft.id);
            rmSync(workspaceOf(draft.id), { recursive: true, force: true });
          }
          draft = blank(req.area, req.projectId, now());
        }
        c = draft;
      }
      if (!live.get(c.id)?.turn) launch(c, pick);
    },
    interrupt(conversationId: string): void {
      live.get(conversationId)?.session.interrupt();
    },
    answer(conversationId: string, requestId: string, allow: boolean, always = false): void {
      const entry = live.get(conversationId);
      if (!entry) return;
      const tool = entry.turn?.permission?.tool;
      if (allow && always && tool) {
        const c = entry.conversation;
        c.allowAlways = [...new Set([...(c.allowAlways ?? []), tool])];
        store.save(c);
      }
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
      rmSync(workspaceOf(id), { recursive: true, force: true });
      emit({ type: 'list' });
    },
    // The absolute path of a file in a chat workspace, or undefined if the path points outside it.
    workspaceFile(conversationId: string, path: string): string | undefined {
      const full = insideFolder(workspaceOf(conversationId), path);
      return full && existsSync(full) ? full : undefined;
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
