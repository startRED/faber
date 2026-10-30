# Count the lines an edit changed in the engine, not from its saved input

Area: Work log
Files: engine/src/change.rs, engine/src/model.rs, engine/src/engine.rs, ui/api.ts, ui/worklog.ts, tests/ui/worklog.check.ts, engine/tests/changes.rs
Depends: none
Size: about 380 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1 (92.6 MB of the 102 MB conversation is tool input and
output). The output is already cut at 4000 characters (`engine/src/engine.rs` near lines 2511 and 2535), but the
input is not: `Write` keeps the whole file it wrote and `Edit` both strings, and `changeOf` in `ui/worklog.ts`
(near line 454) reads them to show "+3 -1" and the first changed lines (`Delta`, `ChangedLines` in `ui/AgentChat.tsx`).
That is the one reason the big input must stay in the conversation. The next task (`big-tool-input-in-pages`) cannot
move it out until the screen no longer needs it, so this one first moves the counting to the engine.

What to build: when the engine sees a `Tool` event (`Core::handle`, `EngineEvent::Tool`) for `Edit`, `MultiEdit`,
`Write` or Codex's `fileChange`, compute the same result as `changeOf` (added, removed, and the first 40 changed lines
with their sign, nothing more) in a new `engine/src/change.rs`, and store it as an optional `ToolUse.change`
(`serde(default, skip_serializing_if)` so old files load). Port the line comparison of `replaced()` in `ui/worklog.ts`
(longest common run, compared only at the ends past 400 lines) without a new dependency (`engine/Cargo.toml` has no
diff crate). `changeOf` then returns `tool.change` when present and keeps its current code as the fallback for old
conversations. The input stays whole in this task.

Leave out: moving the input anywhere (next task), the Changes pane (it reads git, `ui/Changes.tsx`).

Done: Rust tests mirror the `changeOf` cases of `tests/ui/worklog.check.ts` (the check "an edit step counts the lines it added and removed": an edit, a
written file, a multi-edit, a Codex change) and give the same counts and lines; a screen check shows `changeOf` using `tool.change`
when the input is gone. Extends: product-map "Work log". The commit states the memory cost: at most 40 short lines per
edit, against the whole input. Review by a fresh session of the other vendor (R146).
