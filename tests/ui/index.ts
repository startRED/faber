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
import { crash, progress, report } from './crash.ts';
import './appearance.check.ts';
import './check.check.ts';
import './composer.check.ts';
import './prompt-history.check.ts';
import './money-words.check.ts';
import './connect-more.check.ts';
import './contextmenu.check.ts';
import './rewind.check.ts';
import './pin.check.ts';
import './chat-groups.check.ts';
import './hole.check.ts';
import './effort.check.ts';
import './first-frame.check.ts';
import './file-at-line.check.ts';
import './follow.check.ts';
import './plumbing.check.ts';
import './donecard.check.ts';
import './foot.check.ts';
import './checks.check.ts';
import './changes.check.ts';
import './project-memory.check.ts';
import './visual-answers.check.ts';
import './commands.check.ts';
import './idle.check.ts';
import './hints.check.ts';
import './i18n.check.ts';
import './inbox.check.ts';
import './keepgoing.check.ts';
import './interactions.check.ts';
import './models.check.ts';
import './mojibake.check.ts';
import './motion.check.ts';
import './appear.check.ts';
import './nudge.check.ts';
import './notes.check.ts';
import './opening.check.ts';
import './palette.check.ts';
import './find.check.ts';
import './search-messages.check.ts';
import './review-answer.check.ts';
import './run.check.ts';
import './shortcuts.check.ts';
import './terminal-sessions.check.ts';
import './review-motion.check.ts';
import './running.check.ts';
import './settings.check.ts';
import './shell.check.ts';
import './projects.check.ts';
import './pace.check.ts';
import './plan.check.ts';
import './question.check.ts';
import './quiet.check.ts';
import './agents.check.ts';
import './site.check.ts';
import './skills.check.ts';
import './mcp-servers.check.ts';
import './catalog.check.ts';
import './worklog.check.ts';
import './quota.check.ts';
import './recents.check.ts';
import './setup.check.ts';
import './styles.check.ts';
import './grammar.check.ts';
import './lighter.check.ts';
import './continue.check.ts';
import './mention.check.ts';
import './slash.check.ts';
import './quick-question.check.ts';
import './steer.check.ts';
import './edit-queued.check.ts';
import './stop-helpers.check.ts';
import './thread.check.ts';
import './viewer.check.ts';
import './tutorial.check.ts';
import './unsent.check.ts';
import './context.check.ts';
import './approve-plan.check.ts';
import './research.check.ts';
import './calm.check.ts';
import './homes.check.ts';
import './update.check.ts';

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
  runAll(progress).then(report, (error: unknown) => crash(String(error)));
}
