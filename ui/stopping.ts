import { createSignal } from 'solid-js';
import { api } from './api.ts';

const [ids, setIds] = createSignal<ReadonlySet<string>>(new Set());

/** Stop was pressed for this conversation and the engine has not answered yet: every Stop button reads "Stopping…" at once. */
export const stopping = (id?: string) => !!id && ids().has(id);

/** Stops a conversation's run; the answer, or the refusal for the caller to show, ends the "Stopping…" state. */
export async function stopRun(id: string): Promise<void> {
  if (ids().has(id)) return;
  setIds((held) => new Set(held).add(id));
  try {
    await api.interrupt(id);
  } finally {
    setIds((held) => { const next = new Set(held); next.delete(id); return next; });
  }
}
