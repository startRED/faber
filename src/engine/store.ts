import { mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Area } from './claude/args.ts';

export type Vendor = 'claude' | 'codex';

export interface ToolUse {
  id: string;
  name: string;
  input: unknown;
  ok?: boolean;
  output?: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  at: number;
  images?: { mediaType: string; base64: string }[];
  vendor?: Vendor;
  model?: string;
  reason?: string;
  thinking?: string;
  tools?: ToolUse[];
  permission?: { requestId: string; tool: string; input: unknown };
  status?: 'streaming' | 'done' | 'stopped' | 'error';
  error?: string;
  costUsd?: number;
  inputTokens?: number;
  outputTokens?: number;
}

export interface Conversation {
  id: string;
  title: string;
  area: Area;
  projectId?: string;
  // One CLI session per vendor, so switching models keeps each vendor's own history and cache.
  sessions: Partial<Record<Vendor, string>>;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}

export interface Project {
  id: string;
  name: string;
  path: string;
}

export type Summary = Pick<Conversation, 'id' | 'title' | 'area' | 'projectId' | 'updatedAt'>;

// Plain JSON files in the app's data folder: readable by people and by any future tool.
// Writes go to a temp file first, so a crash never leaves a half-written conversation.
export function createStore(dataDir: string) {
  const convDir = join(dataDir, 'conversations');
  mkdirSync(convDir, { recursive: true });
  const write = (path: string, value: unknown) => {
    writeFileSync(`${path}.tmp`, JSON.stringify(value, null, 1));
    renameSync(`${path}.tmp`, path);
  };
  const read = <T>(path: string, fallback: T): T => {
    try {
      return JSON.parse(readFileSync(path, 'utf8')) as T;
    } catch {
      return fallback;
    }
  };
  const projectsFile = join(dataDir, 'projects.json');

  return {
    list(): Summary[] {
      return readdirSync(convDir)
        .filter((f) => f.endsWith('.json'))
        .map((f) => read<Conversation | null>(join(convDir, f), null))
        .filter((c): c is Conversation => c !== null)
        .map(({ id, title, area, projectId, updatedAt }) => ({ id, title, area, projectId, updatedAt }))
        .sort((a, b) => b.updatedAt - a.updatedAt);
    },
    get(id: string): Conversation | undefined {
      return read<Conversation | undefined>(join(convDir, `${id}.json`), undefined);
    },
    save(conversation: Conversation): void {
      write(join(convDir, `${conversation.id}.json`), conversation);
    },
    remove(id: string): void {
      rmSync(join(convDir, `${id}.json`), { force: true });
    },
    projects(): Project[] {
      return read<Project[]>(projectsFile, []);
    },
    saveProjects(projects: Project[]): void {
      write(projectsFile, projects);
    },
  };
}

export type Store = ReturnType<typeof createStore>;
