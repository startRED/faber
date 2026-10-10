import type { AppEvent, SendRequest } from '../engine/engine.ts';
import type { Choice } from '../engine/router.ts';
import type { Conversation, Project, Summary, Vendor } from '../engine/store.ts';

export type { AppEvent, Choice, Conversation, Project, Summary, Vendor };
export type { Message, ToolUse } from '../engine/store.ts';

export interface AppState {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
}

interface Bridge {
  call(method: string, arg?: unknown): Promise<unknown>;
  on(listener: (event: AppEvent) => void): () => void;
}

const bridge = (window as unknown as { faber: Bridge }).faber;

// Typed calls into the engine (see the allowlist in src/main/main.ts).
export const api = {
  state: () => bridge.call('state') as Promise<AppState>,
  open: (id: string) => bridge.call('open', id) as Promise<Conversation | undefined>,
  send: (req: SendRequest) => bridge.call('send', req) as Promise<string>,
  warm: (req: { conversationId?: string; area: 'chat' | 'project'; projectId?: string; choice: Choice }) => bridge.call('warm', req),
  interrupt: (id: string) => bridge.call('interrupt', id),
  answer: (conversationId: string, requestId: string, allow: boolean, always = false) =>
    bridge.call('answer', { conversationId, requestId, allow, always }),
  rename: (id: string, title: string) => bridge.call('rename', { id, title }),
  remove: (id: string) => bridge.call('remove', id),
  saveFile: (conversationId: string, path: string) => bridge.call('saveFile', { conversationId, path }) as Promise<boolean>,
  showFile: (conversationId: string, path: string) => bridge.call('showFile', { conversationId, path }),
  readImage: (conversationId: string, path: string) => bridge.call('readImage', { conversationId, path }) as Promise<string | undefined>,
  addProject: () => bridge.call('addProject') as Promise<Project | undefined>,
  removeProject: (id: string) => bridge.call('removeProject', id),
  on: (listener: (event: AppEvent) => void) => bridge.on(listener),
};
