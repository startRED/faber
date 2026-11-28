import { createSignal } from 'solid-js';
import { api, type PieceData, type PieceValue, type VisualPiece } from './api.ts';
import { keep, kept, setDirty } from './unsent.ts';

// Only unsaved edits live here. A controller outlives its view, so navigation cannot cancel a write or lose a retry.
const pending = new Map<string, ReturnType<typeof controller>>();

function accepts(data: PieceData, value: unknown): value is PieceValue {
  if (data.type === 'checklist') return Array.isArray(value) && value.length === data.items.length && value.every((v) => typeof v === 'boolean');
  if (typeof value !== 'number' || !Number.isFinite(value)) return false;
  if (data.type === 'calculation') {
    const steps = (value - data.min) / data.step;
    return value >= data.min && value <= data.max && Math.abs(steps - Math.round(steps)) <= 1e-7;
  }
  const count = data.type === 'form' ? data.options.length : data.type === 'tabs' || data.type === 'steps' ? data.items.length : 0;
  return Number.isInteger(value) && value >= 0 && value < count;
}

function controller(key: string, conversationId: string, piece: VisualPiece) {
  let recovered: PieceValue | undefined;
  const saved = kept(key);
  if (saved && saved.length <= 512) {
    try {
      const parsed: unknown = JSON.parse(saved);
      if (accepts(piece.data, parsed)) recovered = parsed;
    } catch { /* Invalid local data never becomes a control value. */ }
  }
  const [value, setValue] = createSignal<PieceValue>(recovered ?? piece.value);
  const [saving, setSaving] = createSignal(false);
  const [failed, setFailed] = createSignal(recovered !== undefined);
  let confirmed = piece.value;
  let revision = 0;
  const owner = {};
  const retain = () => {
    pending.set(key, edit);
    keep(key, JSON.stringify(value()));
    setDirty(owner, true);
  };
  const persist = async () => {
    if (saving()) return;
    setSaving(true);
    setFailed(false);
    try {
      let sent: number;
      do {
        sent = revision;
        const attempted = value();
        // These identifiers belong to the edit, never to whichever conversation is on screen after an await.
        await api.savePieceValue(conversationId, piece.messageId, piece.block, attempted);
        confirmed = attempted;
      } while (sent !== revision);
      keep(key, '');
      pending.delete(key);
      setDirty(owner, false);
    } catch {
      setFailed(true);
      retain();
    } finally { setSaving(false); }
  };
  const edit = {
    value, saving, failed, persist,
    reconcile(next: PieceValue) {
      if (!accepts(piece.data, next)) return;
      confirmed = next;
      if (!saving() && !failed() && !pending.has(key)) setValue(next);
    },
    discard() {
      if (saving()) return;
      keep(key, '');
      pending.delete(key);
      setValue(confirmed);
      setFailed(false);
      setDirty(owner, false);
    },
    change(next: PieceValue) {
      if (!accepts(piece.data, next)) return;
      setValue(next);
      revision++;
      retain();
      void persist();
    },
  };
  if (recovered !== undefined) retain();
  return edit;
}

function editKey(conversationId: string, piece: VisualPiece) {
  // The bounded validated definition keeps an old source's failed choice separate from a replacement's defaults.
  return `piece-edit.${JSON.stringify([conversationId, piece.messageId, piece.block, piece.data])}`;
}

/** A restored snapshot also updates retries whose controls are currently offscreen. */
export function reconcilePieceEdit(conversationId: string, piece: VisualPiece) {
  const edit = pending.get(editKey(conversationId, piece));
  edit?.reconcile(piece.value);
  return edit?.value();
}

export function pieceEdit(conversationId: string, piece: VisualPiece) {
  const key = editKey(conversationId, piece);
  return pending.get(key) ?? controller(key, conversationId, piece);
}
