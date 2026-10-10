import type { Vendor } from './store.ts';

export interface Choice {
  vendor: Vendor | 'auto';
  model?: string;
}

export interface Pick {
  vendor: Vendor;
  model?: string;
  effort: string;
  reason: string;
}

// Medium effort by default: high quality for most work without spending quota on easy turns.
const EFFORT = 'medium';

// ponytail: fixed default until the measured router (phase F5) replaces it; the reason line is already shown.
export function pickModel(choice: Choice, available: Vendor[]): Pick | undefined {
  if (choice.vendor !== 'auto') {
    return available.includes(choice.vendor) ? { vendor: choice.vendor, model: choice.model, effort: EFFORT, reason: 'chosen by you' } : undefined;
  }
  if (available.includes('claude')) return { vendor: 'claude', model: 'sonnet', effort: EFFORT, reason: 'Auto: everyday default' };
  if (available.includes('codex')) return { vendor: 'codex', effort: EFFORT, reason: 'Auto: the only connected subscription' };
  return undefined;
}
