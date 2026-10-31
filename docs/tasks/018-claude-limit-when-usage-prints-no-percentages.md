# Claude's limit is read even when `/usage` prints no percentages, if that is still needed

Area: Account limits
Files: engine/src/account.rs, engine/src/cadence.rs, engine/src/engine/quota_watch.rs, engine/tests/quota_live.rs, docs/product/quota-pacing.md
Depends: none
Size: about 190 changed lines

Why: commit 3c4e136f on `queue/claude-limit-reading` ("read Claude's limit with one tiny request when /usage prints no
percentages") never landed: its gate was green and its landing was refused because the base moved (2026-10-28, run
40777869619). The three commits under it are on phase-1-base already; this one is not, and it cherry-picks cleanly.

Build: first check on the installed Claude Code whether `/usage` prints percentages today, and whether phase-1-base
already reads Claude's limit another way when it does not (`engine/src/account.rs`, `quota_watch.rs`). If it does, delete
this task and the queue branch. Otherwise cherry-pick 3c4e136f onto a branch from the base, give its decision row and
what's-new entry their current form (`docs/next/<branch>.json`), and keep the request as small and rare as the commit
says. Faber never reads the CLI's sign-in files (AGENTS.md).

Done: with `/usage` printing no percentages (the recorded fixture), the account card shows Claude's share from the one
small request, and `cargo t --filter quota_live` passes; or the task is deleted with the reason in its commit. Then delete
`queue/claude-limit-reading`.
