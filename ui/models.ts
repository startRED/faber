import type { Choice, ModelInfo, Vendor } from './api.ts';
import { t } from './i18n.ts';

export interface Option {
  label: string;
  choice: Choice;
}

const VENDOR_NAME: Record<Vendor, string> = { claude: 'Claude', codex: 'ChatGPT' };

// Claude Code has no model list, so its aliases are fixed; ChatGPT's list comes from the account.
export function modelOptions(vendors: Vendor[], codexModels: ModelInfo[] = []): Option[] {
  const options: Option[] = [{ label: t('model.auto'), choice: { vendor: 'auto' } }];
  if (vendors.includes('claude')) {
    for (const [label, model] of [['Opus', 'opus'], ['Sonnet', 'sonnet'], ['Haiku', 'haiku']] as const) {
      options.push({ label: `Claude ${label}`, choice: { vendor: 'claude', model } });
    }
  }
  if (vendors.includes('codex')) {
    options.push({ label: t('model.chatgptDefault'), choice: { vendor: 'codex' } });
    for (const m of codexModels) options.push({ label: prettyModel(m.id), choice: { vendor: 'codex', model: m.id } });
  }
  return options;
}

export const vendorName = (vendor: Vendor) => VENDOR_NAME[vendor];

// "claude-sonnet-5-5" -> "Sonnet 5.5", "gpt-6-luna" -> "GPT-6 Luna", "sonnet" -> "Sonnet".
export function prettyModel(model: string | undefined): string {
  if (!model) return t('model.default');
  const claude = /^claude-([a-z]+)-(\d+)-(\d+)/.exec(model);
  if (claude) return `${cap(claude[1]!)} ${claude[2]}.${claude[3]}`;
  const gpt = /^gpt-([\d.]+)(?:-(.+))?$/.exec(model);
  if (gpt) return `GPT-${gpt[1]}${gpt[2] ? ` ${gpt[2].split('-').map(cap).join(' ')}` : ''}`;
  return cap(model);
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
