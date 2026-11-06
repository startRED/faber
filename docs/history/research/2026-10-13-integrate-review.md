# Integration review repair, 13 October 2026

PR #18 remains a draft. All eight findings are real. The unsafe approval and reporting behavior is repaired;
the separately granted miniature real-CLI canary remains incomplete and cannot return Pass.

## Reproductions and regression checks

| Finding | Evidence before repair | Regression test |
| --- | --- | --- |
| C1 | The seven-byte `payload` file returned Pass without starting a process. The corrected assertion failed. | `production_canary_rejects_a_non_executable_payload`; `canary_handshake_rejects_replay_hash_schema_and_fabricated_pass` |
| H1 | Staging with a saved Running mission succeeded. The assertion requiring refusal failed. Production had no call to the staging runner. | `landed_build_waits_for_running_missions_before_any_build_effect`; `landed_checkpoint_drives_exact_tree_staging_at_quiescence`; `local_build_reserves_chat_admission_and_releases_on_unwind` |
| H2 | A handcrafted repository receipt approved its commit/tree without a private checkpoint. The refusal assertion failed. | `production_approval_rejects_forged_repository_receipts`; `production_approval_requires_private_proof_review_commit_and_forward` |
| H3 | Missing check receipts, including a damaged editable mirror, produced `pass`. Assertions requiring `unknown` failed. | `landing_writes_mission_report_with_tokens_checks_receipts_and_timings`; `report_never_trusts_forged_repository_checks` |
| H4 | The first landing in a two-task mission reported `done`. The Running assertion failed. | `report_stays_running_until_all_tasks_retire_and_keeps_each_landing` |
| H5 | The startup recovery path retained Running and reacquired power. The Paused assertion failed. State changes inside the engine bypassed the button handlers. | `restart_pauses_a_saved_running_mission_without_reacquiring_power`; `private_checkpoints_release_mission_power_on_done_disk_pause_and_blocked`; `mission_guard_releases_awake_on_drop_or_panic` |
| M1 | With a damaged budget ledger, Stop saved Paused, returned an error and retained power. The release assertion failed. | `stop_releases_power_when_budget_fails_after_paused_checkpoint` |
| M2 | Swapping head/tree in the original implementation still passed the original report test. The repaired fixture uses different commit/tree OIDs and kills the swap. | `landing_writes_mission_report_with_tokens_checks_receipts_and_timings` |

No finding was rejected. Tests use fixture executors and temporary private ledgers; unit tests launch no process or real git.
Final review also reproduced a missing power-release notification after a private proof receipt write failed.
`proof_receipt_write_failure_releases_mission_power` failed before repair, then passed. All four halted-runner paths
now share the same notification, including decision and plan write failures.
The hidden screen scenario starts the actual candidate executable through the production canary adapter and verifies its
challenge, payload hash, schema, private receipt and NotRun verdict. It starts no GUI runtime or provider session.
The second hidden scenario exercises the actual shell checkpoint observer through Running, Done, Paused and Blocked,
then restarts its own sandbox app and verifies Paused without an awake hold.

## Resulting behavior

- Private runner checkpoints retain accepted candidate proof/review and the authoritative commit/forward intent IDs after retirement.
  Approval validates the private effect ledger and the saved queue acceptance evidence. Repository receipt copies grant no authority.
- Reports derive every task receipt from that ledger, accumulate task entries, and follow the persisted mission state.
  Missing checks remain unknown; damaged private records return errors. Unmeasured tokens remain unknown.
- Private checkpoint notifications update the counted power owner on engine-side Done, Paused, Blocked and IO failure.
  Startup pauses interrupted work before restoring notices. Stop releases power before returning a budget error.
  The admitted production runner owns a RAII power guard through build/canary work; error and panic release it.
  Quiescence waits occur before that owner is created.
- Both GUI and headless development paths consume durable landing requests. The existing coordinator blocks mission admission;
  a scoped engine guard reserves Chat admission throughout staging. Builds use the immutable landed tree, native hidden commands,
  matched installed dependencies, the owned next slot and existing compatibility/retention gates.
- The candidate really starts hidden and acknowledges a fresh challenge with its own executable hash and schema.
  The receipt is saved privately. Startup alone returns NotRun, leaving next waiting and current/last-good intact.

## Mutations

All mutation changes were restored after their focused run. Each killed mutant compiled and failed an assertion.

| Mutant | Result |
| --- | --- |
| Accept a fabricated canary Pass | Killed by `canary_handshake_rejects_replay_hash_schema_and_fabricated_pass` |
| Disable the candidate's native canary entry point | Killed by the hidden `candidate starts hidden` scenario |
| Restore the original repository-based approval | Killed by `production_approval_rejects_forged_repository_receipts` |
| Ignore private queue review failure | Killed by `production_approval_requires_private_proof_review_commit_and_forward` |
| Suppress terminal checkpoint notifications | Killed by `private_checkpoints_release_mission_power_on_done_disk_pause_and_blocked` |
| Disable the shell's actual mission power observer | Killed by the hidden `private checkpoints release mission power` scenario |
| Suppress the halted-runner notification after a proof write failure | Killed by `proof_receipt_write_failure_releases_mission_power` |
| Leak the production runner's guard | Killed by `production_runner_releases_power_after_error_or_panic` |
| Return a Stop error before releasing power | Killed by `stop_releases_power_when_budget_fails_after_paused_checkpoint` |
| Invent Pass for missing checks | Killed by `landing_writes_mission_report_with_tokens_checks_receipts_and_timings` |
| Swap commit head and tree | Killed by `landing_writes_mission_report_with_tokens_checks_receipts_and_timings`; survived the original test |

## Proofs before the upstream refresh

All eight commands exited 0 on the frozen repair before `origin/selfdev` advanced to `6cc42b3`:

- `npm run lint`
- `npm run typecheck`
- `npm run build`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy -p faber --features test-hooks -- -D warnings`
- `cargo t`: 1,272 passed, 0 failed, 0 not run, 0 skipped.
- `cargo t --all`: 1,272 passed, 0 failed, 0 not run, 0 skipped.

Each final test run includes 1,028 Rust tests, 142 UI checks and 102 hidden screen scenarios, including the full
300-second probation. The final all-suite run measured idle full/reduced motion at 115/114 MB and the large-thread
toggle peak at 217.4 MB, below the 250 MB test limit.

An earlier run hit the existing narrow-column typing timing failure. Its unchanged focused rerun and both final
full runs passed. No assertion, timeout, desktop or containment guard was weakened.

The staging regression also reproduced coalescing an older mission into the wrong checkout mirror. It now uses
the older mission's own authoritative project path; `landed_checkpoint_drives_exact_tree_staging_at_quiescence`
is green with two checkouts sharing a repository identity.

## Final proofs after merging selfdev

`origin/selfdev` advanced during validation. The repair merges `6cc42b3` (PR #15) before its commit, with no changes
to `engine/src/project/remote.rs` relative to that base. All eight proof commands above were repeated and exited 0.
Both `cargo t` and `cargo t --all` finished with 1,363 passed, 0 failed, 0 not run and 0 skipped:
1,118 Rust tests, 142 UI checks and 103 hidden screen scenarios. Full probation passed in both runs.

## Portable fixture roots

CI on `b253d6b` exposed a macOS fixture failure: mission observer registration returned `Store(Read)` for the
system temporary directory's symlinked ancestor. Both `private_checkpoints_release_mission_power_on_done_disk_pause_and_blocked`
and `proof_receipt_write_failure_releases_mission_power` reproduced the same failure locally with `TEMP` and `TMP`
pointing through an isolated Windows junction. Both are green after creating their temporary directories beneath the
canonical temporary root, following the existing local-update fixture pattern. Production path validation is unchanged.

All eight required proof commands were repeated after this fixture repair and exited 0. Both `cargo t` and `cargo t --all`
finished with 1,363 passed, 0 failed, 0 not run and 0 skipped, including 103 hidden screen scenarios and full probation.

## Limits

The production writer dispatch seam still refuses execution without the T2/T4 containment qualification.
This repair does not implement or bypass that driver. No real-CLI spending grant has been supplied for the separate
miniature mission. The canary cannot manufacture writer/proof/other-vendor acceptance or compatible schema receipts,
so live promotion remains blocked. The fake-CLI miniature mission and Task 13 live acceptance are not claimed complete.

No installed release, credentials, tokens, updater keys or unrelated running Faber process were inspected or modified.
No locale text changed in this repair; the PR's existing translated build-card keys still need native review.
