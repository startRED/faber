import type { AppState, ChatGroup, Summary } from './api.ts';

/** A change to the lists the sidebar draws, shown before the engine has answered. */
export type Patch = (s: AppState) => AppState;

/** Shows each change at once and puts the lists back when the engine refuses. The patches still waiting are laid over every
 * state read in the meantime (`reapply`), so a refresh that started before the change committed cannot bring a deleted row
 * back or show the old title; a patch is dropped when a read that began after its answer has been applied. */
export function optimistic(get: () => AppState, set: (next: (s: AppState) => AppState) => void, refresh: () => Promise<unknown>) {
  const waiting = new Set<Patch>();
  const reapply: Patch = (s) => [...waiting].reduce((next, patch) => patch(next), s);
  /** `until: 'answer'` for a change the engine's answer replaces whole (a new row with its real id): its patch ends with the call. */
  const run = async <T>(patch: Patch, call: () => Promise<T>, until: 'read' | 'answer' = 'read'): Promise<T> => {
    const { conversations, projects, groups } = get();
    waiting.add(patch);
    set(patch);
    try {
      const result = await call();
      if (until === 'answer') waiting.delete(patch);
      await refresh().catch(() => {});
      return result;
    } catch (err) {
      waiting.delete(patch);
      set((s) => reapply({ ...s, conversations, projects, groups }));
      void refresh().catch(() => {});
      throw err;
    } finally {
      waiting.delete(patch);
    }
  };
  return { run, reapply };
}

const edit = (id: string, fields: Partial<Summary>): Patch => (s) => ({ ...s, conversations: s.conversations.map((c) => c.id === id ? { ...c, ...fields } : c) });
export const pinned = (id: string, on: boolean) => edit(id, { pinnedAt: on ? Date.now() : undefined });
export const archived = (id: string, on: boolean) => edit(id, { archived: on });
export const renamed = (id: string, title: string) => edit(id, { title: title.trim() });
export const movedToProject = (id: string, projectId: string) => edit(id, { area: 'project', projectId });
export const withoutConversation = (id: string): Patch => (s) => ({ ...s, conversations: s.conversations.filter((c) => c.id !== id) });
export const withoutProject = (id: string): Patch => (s) => ({
  ...s, projects: s.projects.filter((p) => p.id !== id), conversations: s.conversations.filter((c) => c.projectId !== id),
});
const groups = (change: (list: ChatGroup[]) => ChatGroup[]): Patch => (s) => ({ ...s, groups: change(s.groups ?? []) });
export const movedToGroup = (id: string, groupId?: string) => groups((list) => list.map((g) => {
  const rest = g.conversationIds.filter((c) => c !== id);
  return { ...g, conversationIds: g.id === groupId ? [...rest, id] : rest };
}));
export const withoutGroup = (id: string) => groups((list) => list.filter((g) => g.id !== id));
export const groupNamed = (id: string, name: string) => groups((list) => list.map((g) => g.id === id ? { ...g, name: name.trim() } : g));
export const withGroup = (group: ChatGroup) => groups((list) => [...list, group]);
