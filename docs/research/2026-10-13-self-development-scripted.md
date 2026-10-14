# Task 13: scripted acceptance

Baseline: `dc46d45`, branch `selfdev-t13`. Measured on 2026-10-13.

The owner's R111 answers govern this run: private authority, cross-vendor review, Chat priority,
and isolated target/install paths. R105 keeps 22 October as a milestone without reducing the bar.

## Regression first

`cargo test -p faber-engine --test project_report` failed before the fix:
`left: Known("unknown")`, `right: Unknown("unknown")`. The same target then passed both tests.
The scalar JSON format is preserved. The literal `"unknown"` is reserved for missing evidence;
other string metrics and numeric metrics remain known. Every production caller uses this sentinel.

The new hidden T13 receipt check failed with `os error 2` before the scripted helper emitted
its evidence. Missing npm dependencies and relocated Tauri permission artifacts were build
environment faults, corrected only in this worktree and the authorized `t13` target.

## Scope of the seven checks

| Runbook case | Executable check | What it establishes |
| --- | --- | --- |
| 1. Red acceptance | `T13 1` in `testkit/src/screen/projects.rs` | Nonzero exit, private red receipt round-trip, and unverified red proof set. |
| 2. Local landing | `T13 2` in the same file | Temporary repository; green exact tree; fresh fixture other-vendor review; Seguro CAS landing; owner index/ref/dirty bytes unchanged. |
| 3. Persisted report | `T13 3` in the same file | Deserialize the saved report; exact commit/tree; genuine green check receipt; all unmeasured role tokens remain unknown. |
| 4. Promoted build | `T13 4` in `testkit/src/screen/local_update.rs` | Hidden native lifecycle with fixture canary evidence; full 300-second probation; active/current B and last-good A; exact candidate identity. |
| 5. Early crash | `T13 5` in the same file | Trusted helper detects the injected crash, restores A within the 30-second bounded wait, retains saved data and deletes bad B. |
| 6. Resume | `T13 6` in the same file | Actual public Resume after native rollback refuses unqualified roles twice and preserves the pre-rollback $2 fixture grant bytes. **Successful Resume consumption is not established.** |
| 7. Untouched owner/install | `T13 7` in `projects.rs` | SHA-256 equality for the temporary owner source, index and synthetic stable payload. Actual owner/install comparison is recorded separately in the final acceptance report. |

These are scripted checks, not live qualification. Cases 1–3 use the existing first-party fake helper
and native git broker on the hidden desktop. Cases 4–6 use the hidden test build and trusted local
launcher. No provider model turn is invoked. Fixture canary/review receipts do not authorize a
production promotion. Case 6 deliberately records the admission blocker instead of bypassing it.

The existing `opus_h2_settled_dispatch_closes_before_resume_and_cleanup`,
`duplicate_ids_cannot_replay_dispatch_or_move_spend_to_another_worker`, and
`session_resume_uses_the_saved_cumulative_base_instead_of_charging_it_twice` cover authorized
resume reconciliation and duplicate charging at their pure seams. They do not prove a live
post-rollback Resume. Task 13 and R96 remain incomplete.

## Validation

Before the phase commit, all seven required proof commands must exit 0. Their full output is retained
locally under `target/t13-evidence/`. The final acceptance report records the measured suite totals.
No roadmap completion claim is changed.
