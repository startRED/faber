# Rewrite the Settings words a person who does not program cannot read

Area: Settings screen
Files: ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, ui/Accounts.tsx, tests/ui/settings.check.ts, tests/ui/money-words.check.ts
Depends: none
Size: about 250 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 4, and the live look of 2026-10-29: Settings shows words and
numbers that need a programmer to read. Checked in the locale files on 2026-10-29 (pt-BR line numbers; each key exists
in the other four languages too):

- "Os projetos rodam os meus próprios hooks da CLI" (`set.hooksBody`, pt-BR line 1391; en "Projects run my own CLI hooks")
  with its hint and more-text (`set.hooksHint`, `set.hooksLocked`, `set.hooksMore`): "hooks" and "CLI" are never
  explained.
- "Em cache" for web search (`set.webSearch.cached`, line 1319) and its `set.webSearch.more`: what "cached" means is in
  a long hint only.
- "Sem cartões: o computador inteiro." (`set.permissions.dontAsk.line`, line 1310): "cartões" are the permission cards,
  a Faber word the person has not met in Settings.
- "Faber hoje: US$ 9,46 em valor de API" (`usage.ownSpend`, line 1614) reads as a bill; the same words are on every
  answer (`answer.cost`, "{cost} em valor de API", line 1055, owned by the next task).
- "janela de 5 horas ≥ 2%": `quota.window` ("janela de {window}"), `quota.five_hour` and `quota.atLeast` ("≥ {share}")
  in `ui/Accounts.tsx` line 149 and the sidebar; the "≥" and the tick on the bar are not explained.
- The same actor is called "IA", "assistente", "agente", "Claude" and "Faber" on one screen: pick the product map's
  words ("IA" for the model, "ajudante" for a helper, "Faber" for the app) and use only them in Settings.

What to build: rewrite these texts in plain words in English (`en.ts`, the source) and each other language, by hand,
never machine-translated. Short sentences under 25 words; a technical word stays only when the person needs it to act
(the name of a setting in a CLI), with one plain line next to it. Search all five locale files for the same words
first and keep one word per idea across the app. The audit's full list is kept with the diagnosis archive
(`E:\Dev\re-corpus\faber-diagnosis\`); words in it that are no longer in `ui/locales` (the audit named "papel", "grafts,
clone raso", "AppContainer", "integridade baixa", none of which the locale files hold on 2026-10-29) are skipped, and
the commit says so.

Leave out: the security texts of Permissions (task `plain-security-text`), the words on the conversation screen (task
`plain-words-on-the-conversation-screen`), any new setting or layout change.

Done: a check in `tests/ui/settings.check.ts` fails today because the old words are still on screen (it looks for
"hooks da CLI", "Em cache" and "valor de API" in the Settings text of pt-BR and the English equivalents); it passes after.
The owner reads the pt-BR diff in a short card before landing (he is a native speaker). Extends: product-map "Settings
screen" and its Words table where a word changes. What's-new entry in every language. Walk it at 720 px and in the
longest language. No memory or processor cost. Review by the other vendor (R146) and the coherence review.
