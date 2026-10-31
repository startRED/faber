# The pace report names the checks that fail by chance

Area: none
Files: scripts/pace.mjs, scripts/pace.test.mjs, docs/dev/workflow.md
Depends: none
Size: about 120 changed lines

Why: on 2026-10-30 the queue had 31 green rounds and 36 red ones in 24 hours, and most reds were not the change's fault:
the visual answers screen check, the sales proof, the draft attachments audit and two engine background tests
(`background_claude_recording_reads_stops_without_a_message_and_ends_naturally`,
`background_codex_child_command_can_print_and_finish_after_its_parent_turn`, run 41110780029 on #503, which does not touch
them). Each such red costs a full queue round (30 to 40 minutes on a busy day) and a session's attention, yet nobody sees
which check does it most until someone counts by hand.

Build: in `scripts/pace.mjs`, for each window, read the failed queue runs' `FAIL` lines (the same reading as
`scripts/base.mjs` `failures`, reused, not copied) and count a check as flaky when the same pull request head later passed
it (a later queue round of the same `Pull-request-head` trailer, or a rerun of the same run). Print the three most frequent
as `flaky: <check> ×<n>` lines under the queue rounds. Logs are read only for failed runs, at most 40 per window, so the
report stays under a minute.

Done: a `scripts/pace.test.mjs` case with two recorded failed runs of one check and one later green round of the same
head shows `×2`, and a red that never passed again is not counted; the task queue section of docs/dev/workflow.md
says in one sentence what `node scripts/pace.mjs` prints, flaky checks included.
