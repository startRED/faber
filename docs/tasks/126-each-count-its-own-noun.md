# Give each running count its own noun, so four numbers never disagree

Area: Running column
Files: ui/App.tsx, ui/Running.tsx, ui/Details.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/running.check.ts, tests/ui/details.check.ts, docs/design/running.md
Depends: none
Size: about 220 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"): one working
conversation showed "1 rodando", "Rodando agora 1", "Comandos 2" and "2 coisas ainda rodando" at once, each counting
something else, all with the same word. Verified in the code:

- the header pill (`ui/App.tsx` line 1843, `run.running`) counts the running list (`items()`, conversations at work);
- the column heading "Rodando agora" plus a number (`ui/Running.tsx` line 140 and `rows()` at line 96) counts those
  plus the done cards;
- the "Comandos" button (`ui/Running.tsx` line 145, `commandCount()` at line 97) counts running commands of every
  conversation, not of the open one;
- the line under an answer (`DetailsLine` in `ui/Details.tsx`, key `background.count`: "{n} things still running")
  counts the background work of this conversation only.

What to build: a noun per number, in all five languages by hand: the pill says what it counts ("1 conversa trabalhando"),
the heading drops its number or counts only what runs (the done cards are a separate group that already has its own
heading), the Commands button says "{n} comandos" and what scope it has (all conversations), and the line under the
answer says "{n} ajudantes ou comandos desta conversa ainda rodando" in the plain words of the product map (helper,
command). If the single live state (task 122) removes the line under the answer, drop that key instead of rewording it.
The tray text `tray.running` (`ui/locales/en.ts` line 327) uses the pill's noun.

Leave out: the layout of the column, the count of needs-you (it has its own word and amber).

Done: a screen check in `tests/ui/running.check.ts` fails today because the pill, the heading, the button and the line
use "running" for four different counts; it passes with one distinct noun each, and a conversation with one helper and
one command shows numbers that add up to what the person can see. Extends: product-map "Running column" and
"Technical details". Update `docs/design/running.md`. What's-new entry in every language. Walk it at 720 px and in the
longest language. No memory or processor cost. Review by the other vendor (R146) and the coherence review.
