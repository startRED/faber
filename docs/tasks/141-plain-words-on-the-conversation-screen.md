# Rewrite the conversation screen's technical words in plain words

Area: Chat conversation
Files: ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, docs/product-map.md, tests/ui/thread.check.ts, tests/ui/i18n.check.ts
Depends: none
Size: about 250 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 4: the locale audit of 2026-10-29 found words a person who
does not program cannot read. The ones on the conversation screen and its errors, checked in `ui/locales/pt-BR.ts` (the
other four languages hold the same keys):

- "Criar galho daqui" (`answer.branchHere`, line 1047), "Enviar como galho" (`mine.sendBranch`, line 1187): "galho" is
  the literal word for a git branch. In English "Branch from here"; the product map's "Branch" row (Words table)
  defines it as a new conversation from that point, so say that.
- "cache quente até {time}" (`answer.cacheWarm`, line 1058): the Words table has a "Cache warm" row; write what it
  means for the person (the next message is cheaper until that time).
- "{cost} em valor de API" (`answer.cost`, line 1055): reads as a bill. The cost at the vendor's list price is a fact the
  person may want, but with the plain meaning ("o que isto custaria pela API"), never as an invoice line.
- "A sessão da CLI parou inesperadamente" and "A CLI não tem mais a sessão anterior desta conversa ..."
  (`error.driverStopped`, `error.sessionLost`, lines 1248 and 1249), "Ferramentas da sessão" (`kit.details`, `kit.line`),
  "Continuar esta conversa no terminal da própria CLI" (`answer.terminalHint`, line 1043): "CLI" and "sessão" are
  engineering words. Say "o programa do Claude ou do Codex" / "a conversa com a IA" in plain terms, and keep the
  recovery step each error gives.
- The same actor named "IA", "assistente" and "agente" in one answer area: use the product map's words and one per idea.

What to build: rewrite these keys in English (`en.ts`, the source) and every language, by hand, never machine-translated,
keeping every instruction an error gives. Sentences under 25 words. Search all locale files for the same words first
(`sessão` appears in 9 pt-BR texts, `CLI` in 8; only the ones on this screen belong here, the rest in Settings are task
`plain-words-in-settings`). Update the product-map Words rows ("Branch", "Cache warm") in the same commit when a name
changes.

Leave out: Settings words and the Permissions security text (the two sibling tasks), helper titles and the persona
(task 130 and PR #447), a new setting.

Done: a check in `tests/ui/thread.check.ts` fails today because the answer's menu and tooltip hold "galho" and "cache
quente" (and their English equivalents are the old words); it passes after, and `tests/ui/i18n.check.ts` still finds
the same keys in all five languages. The owner reads the pt-BR diff in a short card before landing. Extends: product-map
"Branch" and "Cache warm". What's-new entry in every language. Walk it at 720 px and in the longest language. No
memory or processor cost. Review by the other vendor (R146) and the coherence review.
