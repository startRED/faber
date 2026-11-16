export type AppRequest = { app: 'faber-live' | 'faber-test'; action: string; ref?: number; text?: string; up?: boolean; size?: string };
const SHARED_EVENTS = new Set(['list', 'catalog', 'quota', 'usage', 'settings', 'lagged']);
const CONVERSATION_EVENTS = new Set(['commands', 'message', 'asides', 'queue', 'context', 'contextParts', 'plan', 'halted']);
export function appEventAllowed(payload: unknown, conversations: ReadonlySet<string>): boolean {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return false;
  const p = payload as Record<string, unknown>;
  if (typeof p.type !== 'string') return false;
  if (CONVERSATION_EVENTS.has(p.type)) return typeof p.conversationId === 'string' && conversations.has(p.conversationId);
  return SHARED_EVENTS.has(p.type) && p.conversationId === undefined;
}
export function appRequest(input: unknown, testing: boolean): AppRequest {
  const p = input as Record<string, unknown>;
  if (!p || typeof p !== 'object' || p.app !== (testing ? 'faber-test' : 'faber-live')) throw new Error('explicit app target does not match this backend');
  if (Object.keys(p).some(k => !['app','action','ref','text','up','size'].includes(k))) throw new Error('unknown app control argument');
  const action = p.action ?? 'read';
  if (!['read','click','type','scroll','screenshot','close'].includes(action as string)) throw new Error('unsupported app action');
  if (['click','type'].includes(action as string) && (!Number.isInteger(p.ref) || (p.ref as number) < 1 || (p.ref as number) > 4294967295)) throw new Error('read first and use an element ref');
  if (action === 'type' && (typeof p.text !== 'string' || p.text.length > 2000)) throw new Error('text must be at most 2000 characters');
  if (p.up !== undefined && typeof p.up !== 'boolean') throw new Error('up must be boolean');
  if (p.size !== undefined && (typeof p.size !== 'string' || !/^(?:[3-9]\d{2}|1\d{3}|2[0-4]\d{2}|25[0-5]\d|2560)x(?:[3-9]\d{2}|1\d{3}|2[0-4]\d{2}|25[0-5]\d|2560)$/.test(p.size))) throw new Error('size must be 300..2560 CSS pixels per side');
  return {...p, action} as AppRequest;
}
