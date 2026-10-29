# One calm status while work runs

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3. Under one running answer the owner saw ten small grey
lines; six answer "is it working?" (`ui/Run.tsx`, the work log and away line in `ui/Thread.tsx`, `ui/foot.ts`,
`ui/Details.tsx`), two count turns over different periods, two show the plan count, two the same provider switch, four
unlabelled clocks, and one a raw shell line. He compared it with the Claude desktop app and Grok: bigger text, nothing
repeated, nothing to hunt for.

Build one derived state for the conversation's live work, shown once, near the answer, at reading size:

- Priority: needs the person (the question or approval itself, with its buttons) > failed or stopped (scoped to its own
  answer, never to a newer turn) > working > done.
- Working: one live sentence, "Trabalhando há 15 min: <what, in the person's words>", then helpers as one short list
  (name, state, own timer), then "Ver detalhes". If no plain description is known: "A IA está trabalhando", never a
  shell line.
- Done: the list folds to one line ("Trabalhou por 15 min ›", like Grok and Claude); a provider switch or a summarized
  history is said once.
- Counts of steps and commands, raw commands, the full plan, model and effort, the away log and the cheaper-helper note
  move behind "Ver detalhes"; nothing that needs the person is hidden there.
- Status text waits about 400 ms before it shows and stays at least 400 ms (no flicker); the live region announces
  changes, never clock ticks.

If the readable-turns task (fold and group sentences) landed first, extend it; this task owns the live state. Walk it at
720 px, the longest language, reduced motion, light and dark. Visual critique before and after (`visual-critique`
skill). Extends: product-map entries for the work log, the running column and keep-going. What's-new entry.

Seen live on 2026-10-29 (task 125 lists the faults found in code): T3 Code shows one line, "Working for 26m 50s",
whose time ticks every second, and under it one live line in plain words ("Patching the file", "Thinking"); finished
work folds to "Worked for 10m 22s ›" and opens into one headline per step with commands grouped ("Ran 7 commands"). A
helper is a card in the answer (title, state, its own timer) that opens its own page with a link back to the parent,
and a line says "2 of 3 delegated tasks finished". When a request waits over 120 s it says so in a notice. Two photos
of Faber 18 s apart showed nothing changed: something the person can see must move while work goes on.
