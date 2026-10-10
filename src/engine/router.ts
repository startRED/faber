import type { Vendor } from './store.ts';

export interface Choice {
  vendor: Vendor | 'auto';
  model?: string;
}

export interface Pick {
  vendor: Vendor;
  model?: string;
  reason: string;
}

// ponytail: fixed default until the measured router (phase F5) replaces it; the reason line is already shown.
export function pickModel(choice: Choice, available: Vendor[]): Pick | undefined {
  if (choice.vendor !== 'auto') {
    return available.includes(choice.vendor) ? { vendor: choice.vendor, model: choice.model, reason: 'chosen by you' } : undefined;
  }
  if (available.includes('claude')) return { vendor: 'claude', model: 'sonnet', reason: 'Auto: everyday default' };
  if (available.includes('codex')) return { vendor: 'codex', reason: 'Auto: the only connected subscription' };
  return undefined;
}
