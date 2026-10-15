// Loaded by ui/main.tsx only in a test build (`vite build --mode test`); a production build never contains it.
//
// - `window.__faberChecks()` runs every check in this folder and returns `{ name, ok, message, ms }` for each.
// - `window.__faberText(key, vars, language)` is the screen's own translation, so screen scenarios find elements by role
//   and translated name and never write a sentence of their own.
// - `window.__faberMojibake(text)` is the UX sentinel's T1: why a text is garbled, or nothing (tests/ux/mojibake.ts).
// - When the shell opened the page as `index.html?checks` (it does that when it was started with `FABER_CHECKS_OUT`),
//   the checks run at load and their result goes to the shell's `report_checks` command, which writes it to that file and
//   ends the app. No DevTools port is involved.

import { LANGUAGES, t } from '../../ui/i18n.ts';
import type { Key } from '../../ui/locales/en.ts';
import { runAll, type Result } from './check.ts';
import { mojibake } from '../ux/mojibake.ts';
import { crash, report } from './crash.ts';
import './appearance.check.ts';
import './check.check.ts';
import './composer.check.ts';
import './hole.check.ts';
import './effort.check.ts';
import './i18n.check.ts';
import './inbox.check.ts';
import './interactions.check.ts';
import './models.check.ts';
import './mojibake.check.ts';
import './motion.check.ts';
import './nudge.check.ts';
import './opening.check.ts';
import './review-answer.check.ts';
import './review-motion.check.ts';
import './running.check.ts';
import './settings.check.ts';
import './pace.check.ts';
import './setup.check.ts';
import './styles.check.ts';
import './steer.check.ts';
import './thread.check.ts';
import './tutorial.check.ts';

type Vars = Record<string, string | number>;

declare global {
  interface Window {
    __faberChecks: () => Promise<Result[]>;
    __faberText: (key: string, vars?: Vars, language?: string | null) => string;
    __faberMojibake: (text: string) => string | undefined;
  }
}

const fill = (text: string, vars: Vars) => text.replace(/\{(\w+)\}/g, (all, name: string) => (name in vars ? String(vars[name]) : all));

/** The text for `key`: in the language on screen, or in another one when a scenario asks for it. */
function textFor(key: string, vars: Vars = {}, language?: string | null): string {
  if (!language) return t(key as Key, vars);
  const messages = LANGUAGES.find((l) => l.code === language)?.messages;
  if (!messages) throw new Error(`no language ${language}`);
  const entry = messages[key as Key];
  if (entry === undefined) throw new Error(`no text ${key} in ${language}`);
  if (typeof entry === 'string') return fill(entry, vars);
  return fill(new Intl.PluralRules(language).select(Number(vars.n ?? 0)) === 'one' ? entry.one : entry.other, vars);
}

window.__faberChecks = runAll;
window.__faberText = textFor;
window.__faberMojibake = mojibake;

if (new URLSearchParams(location.search).has('checks')) {
  runAll().then(report, (error: unknown) => crash(String(error)));
}
