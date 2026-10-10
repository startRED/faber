import type { Choice, Vendor } from './api.ts';

export interface Option {
  label: string;
  choice: Choice;
}

const VENDOR_NAME: Record<Vendor, string> = { claude: 'Claude', codex: 'ChatGPT' };

// ponytail: fixed list per vendor; read each CLI's model list once the capability probe lands.
export function modelOptions(vendors: Vendor[]): Option[] {
  const options: Option[] = [{ label: 'Auto', choice: { vendor: 'auto' } }];
  if (vendors.includes('claude')) {
    for (const [label, model] of [['Opus', 'opus'], ['Sonnet', 'sonnet'], ['Haiku', 'haiku']] as const) {
      options.push({ label: `Claude ${label}`, choice: { vendor: 'claude', model } });
    }
  }
  if (vendors.includes('codex')) options.push({ label: 'ChatGPT (your default)', choice: { vendor: 'codex' } });
  return options;
}

export const vendorName = (vendor: Vendor) => VENDOR_NAME[vendor];

// "claude-sonnet-5-5" -> "Sonnet 5.5", "gpt-6-luna" -> "GPT-6 Luna", "sonnet" -> "Sonnet".
export function prettyModel(model: string | undefined): string {
  if (!model) return 'Default model';
  const claude = /^claude-([a-z]+)-(\d+)-(\d+)/.exec(model);
  if (claude) return `${cap(claude[1]!)} ${claude[2]}.${claude[3]}`;
  const gpt = /^gpt-([\d.]+)(?:-(.+))?$/.exec(model);
  if (gpt) return `GPT-${gpt[1]}${gpt[2] ? ` ${gpt[2].split('-').map(cap).join(' ')}` : ''}`;
  return cap(model);
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
