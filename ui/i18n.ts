import { createSignal } from 'solid-js';
import { failureOf } from './api.ts';
import { de } from './locales/de.ts';
import { en, type Key, type Messages } from './locales/en.ts';
import { es } from './locales/es.ts';
import { fr } from './locales/fr.ts';
import { ptBR } from './locales/pt-BR.ts';

// Each language is written by hand (see locales/en.ts). Names are shown in their own language.
export const LANGUAGES = [
  { code: 'en', name: 'English', messages: en as Messages },
  { code: 'pt-BR', name: 'Português (Brasil)', messages: ptBR },
  { code: 'es', name: 'Español', messages: es },
  { code: 'fr', name: 'Français', messages: fr },
  { code: 'de', name: 'Deutsch', messages: de },
] as const;

export type Lang = (typeof LANGUAGES)[number]['code'];

// The system language decides when the user has not chosen; anything Faber does not have falls back to English.
export function systemLanguage(preferred: readonly string[] = navigator.languages): Lang {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split('-')[0];
    if (base === 'pt') return 'pt-BR';
    const found = LANGUAGES.find((l) => l.code === base);
    if (found) return found.code;
  }
  return 'en';
}

const [lang, setLang] = createSignal<Lang>(systemLanguage());
export { lang };

export function useLanguage(choice: string | undefined): void {
  const chosen = LANGUAGES.find((l) => l.code === choice)?.code ?? systemLanguage();
  setLang(chosen);
  document.documentElement.lang = chosen;
}

const fill = (text: string, vars: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (all, k: string) => (k in vars ? String(vars[k]) : all));

/** The text for `key` in the current language; `n` picks the plural form where a text has one. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  const messages = LANGUAGES.find((l) => l.code === lang())?.messages ?? en;
  const entry = messages[key];
  if (typeof entry === 'string') return fill(entry, vars);
  return fill(one(Number(vars.n ?? 0)) ? entry.one : entry.other, vars);
}

/** Catalog cards, inventory and session reports share the same hand-written names. */
export function catalogName(id: string): string {
  const key = `catalog.name.${id}`;
  return key in en ? t(key as Key) : id.replaceAll('-', ' ');
}

/** Whether `n` takes the singular form. Zero counts as plural everywhere ("0 steps"), though Portuguese and French grammar put it
 * with the singular. */
const one = (n: number) => n !== 0 && new Intl.PluralRules(lang()).select(n) === 'one';

/** A number written the person's way ("0.12", "0,12"). */
export const amount = (value: number, digits: number) => new Intl.NumberFormat(lang(), { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);

/** A share from 0 to 1 as a whole percentage, written the person's way ("72%", "72 %"). */
export const percent = (share: number) => new Intl.NumberFormat(lang(), { style: 'percent', maximumFractionDigits: 0 }).format(share);

/** A large count in few characters ("22k", "22 mil"), in the person's language. */
export const compact = (n: number) => new Intl.NumberFormat(lang(), { notation: 'compact', maximumFractionDigits: 1 }).format(n);

/** US dollars, the currency of API prices, with the sign where the language puts it. */
export const money = (usd: number, digits = 2) => t('money.usd', { amount: amount(usd, digits) });

const unit = (value: number, name: string, digits = 0) =>
  new Intl.NumberFormat(lang(), { style: 'unit', unit: name, unitDisplay: 'short', maximumFractionDigits: digits }).format(value);

/** A file size in the person's language ("1.5 MB", "1,5 MB"). */
export const bytes = (n: number) =>
  n < 1024 ? unit(n, 'byte') : n < 1024 * 1024 ? unit(n / 1024, 'kilobyte', 1) : unit(n / 1024 / 1024, 'megabyte', 1);

/** A short span of time in the person's language ("45 sec", "3 min 20 sec"). */
export const duration = (seconds: number) =>
  seconds < 60 ? unit(seconds, 'second') : `${unit(Math.floor(seconds / 60), 'minute')} ${unit(seconds % 60, 'second')}`;

/** A text with its placeholders still in it, as `{one, other}`. The tray is outside the page, so the shell fills them. */
export function templates(key: Key, code: Lang = lang()): { one: string; other: string } {
  const entry = (LANGUAGES.find((l) => l.code === code)?.messages ?? en)[key];
  return typeof entry === 'string' ? { one: entry, other: entry } : entry;
}

/** Texts that arrive from the engine as keys (errors, routing reasons); anything else is shown as it came. */
export function fromEngine(text: string | undefined, vars: Record<string, string | number> = {}): string {
  if (!text) return '';
  // Auto's reason, "reason.route.<kind>.<level>": the kind of task and how hard it is.
  const route = /^reason\.route\.(\w+)\.(\w+)$/.exec(text);
  if (route) return t('reason.route', { kind: t(`kind.${route[1]}` as Key), level: t(`level.${route[2]}` as Key) });
  if (text in en) return t(text as Key, vars);
  // A key with no sentence yet never shows raw: a budget wait says so, anything else says it is in the log.
  if (/^reason\.budget\.[\w.]+$/.test(text)) return t('reason.budget.other');
  if (/^(reason|error)\.[\w.]+$/.test(text)) return t('reason.other');
  return text;
}

/** The sentence for a refusal: the shell's key, with the field or the colour pairs it is about as `{detail}`. */
export function failureText(err: unknown): string {
  const { key, detail } = failureOf(err);
  return fromEngine(key, detail === undefined ? {} : { detail });
}
