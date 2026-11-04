# Fix the data-loss findings of the 1.0 review

Area: none
Files: engine/src/engine/resume_cut.rs, engine/src/engine/install_gate.rs, engine/src/engine/plan.rs, engine/src/engine.rs, host/src/table.rs, electron/main.ts, electron/sidecar.ts, electron/wayback.ts, ui/unsent.ts, src-tauri/src/main.rs
Depends: none
Size: two sequential pull requests, about 400 changed lines each

Why: the 1.0 gate's fresh other-vendor correctness review (reports/reforma/helpers/rv2.result.md, gpt-6.1-sol on v0.3.17..2c19f1da, source reading only, nothing run) found, besides the two known ones (draft after a forced close, task 455; update restart race, task 460):
High:
1. `resume_cut.rs:71` and `install_gate.rs:97` empty the queue before `send_by_itself`, whose synchronous save (`engine.rs:4207`) writes the conversation with the queue empty: a crash then loses accepted queued messages. Keep the queue in every saved snapshot.
2. `host/src/table.rs:707` copies a workspace file straight onto the chosen destination: a failed copy leaves the old file destroyed. Reuse the atomic replace of `host/src/look.rs:617`.
3. Tauri to Electron migration strands drafts and attachments (new profile and origin `faber://app`, `electron/main.ts:31`; localStorage and IndexedDB, `ui/unsent.ts:87`, `:185`). Carry them over or store them with the engine.
4. `electron/sidecar.ts:132`: normal Quit gives the engine five seconds from the stdin close, including saves; slow storage kills it before the last save. Wait for the engine's own end, with a longer bound.
Medium: `plan.rs:104` deletes the follow-up file before the capacity check fails; recovering a moved project leaves saved `WorkCopy.root` and linked worktrees on the old path (`engine.rs:5733`, `copies.rs:337`); the rollback backup (`electron/wayback.ts:56`) omits child transcripts, project attachments and Chat working files; an engine crash leaves the Electron screen stale with no restart (`electron/sidecar.ts:75`); and `engine/src/review.rs:208` merges unrelated findings into one "Agreed" row by category and file only. Also low: Tauri release honours `FABER_CDP_PORT` (`src-tauri/src/main.rs:796`), gate it behind `test-hooks`.

Build: first reproduce each in a test (current-thread and crash-point tests for 1 and 4; a failing-copy test for 2). PR one: the four highs. PR two: the mediums and the low. A check per fix fails without it.

Leave out: the two known issues (tasks 455 and 460).

Done: each finding has a check that fails without its fix; a second fresh review of the diff finds none of them open.
