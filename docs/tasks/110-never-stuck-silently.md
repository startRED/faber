# Never wait or loop without telling the person

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 2 (code audit re-checked on `phase-1-base`, and the
owner's saved conversations: two answers alternating 19 times each under keep-going, 255 automatic turns with no text or
tools, a main agent repeating one failing command 18 and 19 times).

Fix each, with a check that fails before the fix:

1. `engine/src/engine/silence.rs` `waits`: an open tool resets the silence clock for ever. Track how long each open
   tool has gone without output; after a limit (longer while a command runs) show a notice with Stop, never kill on
   silence alone. Permission cards keep pausing the clock.
2. `engine/src/engine/compact.rs`: give compaction its own deadline and a notice; keep the queue and the context.
3. `engine/src/engine/stuck_watch.rs`: a pending question pauses only the helper it belongs to, not its siblings.
   Background helpers get the same silence and repeat signs.
4. `engine/src/engine/keep_going.rs`: cap idle rounds as well as working ones; stop on the same non-empty answer
   repeated (normalized) twice in a row or alternating; give the two-empty-answers stop its own reason instead of
   `reason.keepGoing.done` (it proves nothing is done).
5. The main agent: the same command failing three times with no change between, or the same call ten times, shows a
   sign like a helper's, without stopping it.

Words (locales, every language, plain): "Essa etapa está sem dar notícias há 12 min. Você pode esperar ou parar." / "A IA
repetiu a mesma resposta. Parei para não gastar sua cota." / "Parei porque a IA não respondeu duas vezes; não sei se o
trabalho terminou." Keep everything the audit lists as working (silence stop with PC-sleep handling, bounded failure
retries, save-failure retries). Extends: product-map entries for keep-going and the running list. Security-neutral, one
review by the other vendor.
