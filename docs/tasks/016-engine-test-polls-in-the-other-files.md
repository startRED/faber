# Engine test polls wait on the shared deadline: the other files

Area: none
Files: engine/tests/*.rs except engine, keep_going, toolbox, compact, conversation_copy, goals and verify
Depends: engine-test-waits-hold-on-a-loaded-runner
Size: about 180 changed lines

Why: `engine/tests` has 125 polls that count sleeps (`for _ in 0..200 { ...; sleep(10 ms) }`) in 31 files, so each one's
budget is a number of timer ticks (2 to 5 s on Windows) and fails on a loaded runner while the code is right.
`conversation_copy::move_chat_to_project_paid_consent_waits_for_its_copy_and_cancel_revokes_it` (`conversation_copy.rs:832`)
failed that way in `queue/pr-470` run 41054660246 (2026-10-30 12:40), on a change to other files, and
`goals::a_fix_stays_held_...` in run 41052178816 through `next_pipe` (task 012). These files hold 36 of them.

Build: replace each poll that waits for something to happen with the helper task 012 adds to `support/mod.rs`, keeping
its condition and giving it a message that says what did not happen. A poll that checks something must *not* happen within
a time (the loop runs to the end on purpose) stays as it is. A test on paused tokio time (`start_paused`) keeps its loop:
its sleeps cost no real time.

Done: `node scripts/cargo.mjs t --filter <each file's tests>` passes, no counted sleep poll that waits for something is left
in these files (`grep -nE "for _ in 0\.\.[0-9]+ \{" <files>` shows only the kept ones, each with a one-line reason), and the Rust
suite's time does not grow.
