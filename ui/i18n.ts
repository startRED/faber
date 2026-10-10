import { createSignal } from 'solid-js';
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
  const form = new Intl.PluralRules(lang()).select(Number(vars.n ?? 0)) === 'one' ? entry.one : entry.other;
  return fill(form, vars);
}

/** Texts that arrive from the engine as keys (errors, routing reasons); anything else is shown as it came. */
export function fromEngine(text: string | undefined): string {
  if (!text) return '';
  return text in en ? t(text as Key) : text;
}
