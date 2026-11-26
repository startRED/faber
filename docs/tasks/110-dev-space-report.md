# A report lists what could be removed from Faber's development folders, and removes nothing

Area: none
Files: scripts/space.mjs, scripts/space.test.mjs, docs/dev/build-folders.md
Depends: none
Size: about 350 changed lines

Why: plan item A3 (the development side) and the owner's answer 11 of 2026-11-25: clean-up is approved in principle,
and nothing is removed before he sees the list. Measured on 2026-11-25 on the owner's PC: the Faber folder takes 62 GB,
`target` 46.4 GB against a written cap of 24 GB (R566), and git lists 412 worktrees. Read in the code:
`scripts/cargo.mjs` calls `scripts/storage-sweep.rs` at most once an hour; it sums the main `target` and the tagged
targets of registered worktrees, keeps recent, in-use, reserved and main-profile folders, and only reports the protected
excess, so 46 GB is possible. No script says which worktrees are merged, clean and old.

Build:

- `node scripts/space.mjs` (read-only): the build folders with size, last use and why each is protected or would go;
  every registered worktree with its path, branch, whether its work is on `origin/phase-1-base` (ancestor or same
  content), clean or dirty, age, and a verdict (would go, keep, unknown); a read error gives "unknown", never "would
  go". Totals per verdict. `--json` for a machine.
- Why the cap was passed, in a sentence the report prints (which protection held which folder).
- No `--delete`: removing is a later step, after the owner's yes on the printed list.

Leave out: changing the sweep's rules; anything in the app's data folder (task copy-sweep-while-others-work).

Done: `scripts/space.test.mjs` with fake git output and a temp folder: a merged clean old worktree is "would go", a
dirty one "keep", an unreadable one "unknown" (they fail without the code); `npm run test:scripts` runs it. The report
of the owner's PC, totals only, in the commit body. A read-only script and words: ships on its proofs.
