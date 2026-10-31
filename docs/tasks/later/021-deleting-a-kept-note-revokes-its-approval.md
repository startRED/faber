# Deleting the last copy of a kept project note revokes its approval

Area: Project memory
Files: engine/src/engine.rs, engine/src/store.rs, engine/src/project/memory.rs, engine/src/engine/memory_search.rs, engine/tests/engine.rs
Depends: none
Size: about 600 changed lines; split it into two pull requests in sequence if it passes 1000 with its tests

Why: commit c48b3c94 on `queue/approval-identity-recovery` (2026-10-24, 587 lines) makes an approval of a kept project note
die with the note's last copy, so a note written again later with the same words is not approved by itself. It failed
only Clippy (`cloned_ref_to_slice_refs` at `store.rs:759`), and the base has since moved the model to `KeptDecision`, so
it conflicts in four Rust files.

Build: rebase its idea onto `KeptDecision`. First check on the base whether an approval still outlives its note's last
copy, with a test in `engine/tests/engine.rs` that fails there. It is a security change: two reviews from fresh sessions,
the other vendor's first (AGENTS.md).

Done: the new test fails on the base and passes with the change, Clippy passes, and both reviews are answered. Then delete
`queue/approval-identity-recovery`.
