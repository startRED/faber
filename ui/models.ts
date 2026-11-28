import { DEFAULT_EFFORT, type Choice, type Effort, type ModelInfo, type Vendor } from './api.ts';
import { t } from './i18n.ts';

export interface Option {
  label: string;
  choice: Choice;
  current?: boolean;
  efforts?: Effort[];
}

const VENDOR_NAME = { claude: 'Claude', codex: 'Codex' };
const ANTIGRAVITY_MODELS = ['gemini-4-neon', 'gemini-4-argon', 'gemini-4-hydrogen', 'gemini-3.8-flash'] as const;
const EFFORTS: Effort[] = ['low', 'medium', 'high', 'xhigh'];
// shortcut: Auto profile ids are mirrored here; share metadata when the account model list exposes profiles.
const AUTO_MODELS = new Set(['gpt-6-luna', 'gpt-6-sol', 'gpt-6-astra', 'gpt-6.1-sol', 'gpt-6.1-terra', 'gpt-6.2-sol', 'gpt-6.2-astra']);

// Claude Code has no model list, so its aliases are fixed; ChatGPT's list comes from the account.
export function modelOptions(vendors: Vendor[], codexModels: ModelInfo[] = [], antigravityModels?: ModelInfo[]): Option[] {
  const options: Option[] = [{ label: t('model.autoChoose'), choice: { vendor: 'auto' } }];
  if (vendors.includes('claude')) {
    for (const [label, model] of [['Fable', 'fable'], ['Opus', 'opus'], ['Sonnet', 'sonnet'], ['Haiku', 'haiku']] as const) {
      options.push({ label: `Claude ${label}`, choice: { vendor: 'claude', model }, current: true });
    }
  }
  if (vendors.includes('codex')) {
    // Each model by its own name, once the account lists them; Auto covers the moment before.
    for (const m of codexModels) options.push({ label: prettyModel(m.id), choice: { vendor: 'codex', model: m.id }, current: m.isDefault || AUTO_MODELS.has(m.id) });
  }
  if (vendors.includes('gemini')) {
    if (antigravityModels === undefined) {
      for (const model of ANTIGRAVITY_MODELS) options.push({ label: prettyModel(model), choice: { vendor: 'gemini', model }, current: true });
    } else {
      const families = new Map<string, { label: string; efforts: Set<Effort> }>();
      for (const variant of antigravityModels) {
        const effort = EFFORTS.find((level) => variant.id.endsWith(`-${level}`));
        const id = effort ? variant.id.slice(0, -effort.length - 1) : variant.id;
        const label = (effort ? variant.label.replace(/\s*\((?:low|medium|high|xhigh|extra high)\)$/i, '') : variant.label).trim() || prettyModel(id);
        const family = families.get(id) ?? { label, efforts: new Set<Effort>() };
        if (effort) family.efforts.add(effort);
        families.set(id, family);
      }
      for (const [model, family] of families) {
        const efforts = EFFORTS.filter((effort) => family.efforts.has(effort));
        const effort = efforts.includes(DEFAULT_EFFORT) ? DEFAULT_EFFORT : efforts[0];
        options.push({ label: family.label, choice: { vendor: 'gemini', model, ...(effort && { effort }) }, efforts, current: true });
      }
    }
  }
  return options;
}

export const vendorName = (vendor: Vendor) => vendor === 'gemini' ? t('setup.antigravity') : VENDOR_NAME[vendor];

export function availableChoice(options: Option[], choice?: Choice): Choice {
  const option = choice && options.find((o) => o.choice.vendor === choice.vendor && o.choice.model === choice.model);
  if (!choice || !option) return { vendor: 'auto' };
  if (option.efforts && (!choice.effort || !option.efforts.includes(choice.effort))) return option.choice;
  return choice;
}

// "claude-sonnet-5-5" -> "Sonnet 5.5", "gpt-6-luna" -> "GPT-6 Luna", "sonnet" -> "Sonnet".
export function prettyModel(model: string | undefined): string {
  // "codex" is what the engine reports when the CLI named no model: that is no name to show.
  if (!model || model === 'codex') return t('model.default');
  if (model === 'gemini-pro-orchestrator-neon') return 'Orchestrator (Neon)';
  if (model === 'gemini-pro-orchestrator-g1-pro') return 'Orchestrator (Argon)';
  if (model === 'gpt-oss-120b' || model === 'gpt-oss-120b-medium') return 'GPT-OSS 120B';
  const claude = /^claude-([a-z]+)-(\d+)-(\d+)/.exec(model);
  if (claude) return `${cap(claude[1]!)} ${claude[2]}.${claude[3]}`;
  const gpt = /^gpt-([\d.]+)(?:-(.+))?$/.exec(model);
  if (gpt) return `GPT-${gpt[1]}${gpt[2] ? ` ${gpt[2].split('-').map(cap).join(' ')}` : ''}`;
  const gemini = /^gemini-([\d.]+)-(.+)$/.exec(model);
  if (gemini) return `Gemini ${gemini[1]} ${gemini[2]!.split('-').map(cap).join(' ')}`;
  return cap(model);
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
