# Keep going: the person hears when it is done, and never sees the stop line

Asked by Erick on 2026-10-19, after a long Keep going run in a project conversation, in his words: "Vc n avisou que
terminou coloque isso no plano pra arrumar e tbm esse keep going: stop". Two faults, both seen in that conversation:

1. **No word that it finished.** The run ended with nothing that reached him: he had to come back and ask "E ai
   terminou?" twice. The design says the tray notifies once when Keep going stops by itself ("{title}: kept going until
   nothing was left", the `halted` event; docs/product-map.md, Tray), unless notifications are off. Find why it did not
   fire or did not reach him. Candidates to check, not conclusions:
   - the stop line came in a later reply (a Stop hook added context and the agent answered again with the line);
   - the run was started from the person's own message and not from Keep going's prompt;
   - the person's later questions ("E ai terminou?") got an answer ending with the line while Keep going was already off.

   Done: every way a run ends with the line gives exactly one notice that it is done, plus an in-app sign on the
   conversation that does not depend on the tray.
2. **The stop line shows.** `[keep going: stop]` (engine/src/engine/keep_going.rs, `STOP`, `stop_at`) stayed in the
   text of several replies on screen: the one that ended the run, the answer to a Stop hook's context after it, and
   answers to the person's later questions. It is a signal for Faber, never words for the person. Done: no reply on
   screen or in a copy, export or the conversation's JSON shown to the person carries it, in any case or markup the
   model wraps it in (`stop_at` already reads those), whether Keep going is on or off when the reply ends.

Each fix gets a check that fails without it (a recorded fixture for each case above in tests/fixtures/, replayed by
engine/tests/ and tests/ui/). The product map's Tray and Keep going rows say what changed, and a what's-new line
says it in plain words.
