# Local update review regressions

The Opus and Gemini reviews examined 813e0c0. Each reported failure was reproduced before the responsible
production change. The regressions use temporary directories; native lifecycle scenarios use only the hidden
test build, private slots and owned jobs. Neither the installed app nor an existing Faber process is modified.

| Review finding | Verdict and regression | Root correction |
| --- | --- | --- |
| Opus H1 | Real: `refused_transition_does_not_rotate_or_leak_a_helper`, `failed_atomic_replacement_removes_its_temporary_payload`; hidden second-transition scenario | The enrolled launcher is immutable. A verified healthy slot's companion supervises its future transition. A slot's detached guard runs from the slot, so it cannot pin the previous helper image. Failed atomic writes remove their temporary payload. Incomplete slot publication cannot take the canary-only retry path. |
| Opus H2 | Real: `healthy_checkpoint_never_replays_an_older_transition`, `promotion_and_clean_exit_checkpoints_survive_an_interrupted_live_commit` | Promotion commits a Healthy checkpoint before the live write. Preparing another slot commits the current healthy tuple before reusing its previous slot. Corrupt-manifest recovery verifies the selected payload before changing state; it never replays an older transition over Healthy. |
| Opus H3 | Real: `candidate_reported_schema_cannot_bypass_rollback_compatibility` | Native builds obtain schema/hash identity from the actual executable. Stage binds that identity to its payload and refuses newer schemas before publication. The old/new executables also run record codecs on a private data copy. |
| Opus M1 | Real: `rollback_relaunch_decision_survives_cleanup_failures`, `interrupted_empty_build_cleanup_does_not_wedge_staging` | Cleanup cannot suppress a verified rollback launch. Startup retries obsolete owned output. A validated empty build directory can be removed after an interrupted final cleanup; nonempty unmarked output remains protected. |
| Opus M2 | Real: `clean_quit_pauses_probation_without_quarantine_or_relaunch`, `orderly_session_end_before_health_retries_full_probation_without_rejection`; hidden session/clean-quit scenarios | Settled clean exits pause without rejecting next or relaunching last good. A hidden Windows session observer persists orderly shutdown/logoff intent, including before health. Cancelled shutdown does nothing. Restart requires a fresh acknowledgment, operation and full 300 seconds. Session callbacks and monitor writes are serialized and joined before recovery. |
| Opus M3 | Real: hidden `trusted launcher recovers before a broken candidate can enter main`; `enrolled_launcher_has_explicit_configuration_without_a_slot_or_environment` | The enrolled `faber-local --launch` entry reconciles before executing any slot. Its explicit development descriptor works without environment variables. The test poisons next with an invalid executable and proves last-good recovery without entering next's main. |
| Gemini H1 | Real: `healthy_handoff_survives_cleanup_failure`; hidden five-minute handoff with an exclusively held output | Owned cleanup failure cannot turn an already Healthy handoff into an error that kills the child. The held output is removed automatically on the following startup. |
| Gemini M2 | Real failure path, duplicate of Opus M1: `rollback_relaunch_decision_survives_cleanup_failures` | A busy candidate lease cannot prevent last-good relaunch. Cleanup retries after lease release. The natural OS scheduling frequency of the reported termination race was not measured. |
| Gemini M3 | Real: `startup_accepts_windows_case_and_verbatim_aliases` | Both startup checks use the same validated, case/prefix-normalized comparison. |
| Gemini L4 | Real, same helper-rotation defect: `refused_transition_does_not_rotate_or_leak_a_helper` | Refused quiescence cannot change the enrolled helper or its identity. No helper rotation writes precede eligibility checks. |

Additional independent items: promotion retains proof/canary evidence in the immutable good record;
`waiting_canary_retries_the_same_payload_without_rebuilding` keeps the built payload pending, repeats compatibility
on current saved data and retries availability without rebuilding. `incomplete_waiting_payload_requires_a_new_accepted_build`
prevents that shortcut after interrupted artifact publication. `user_open_really_cancels_quiescence_and_requires_a_fresh_canary`
reaches Quiescent itself. Projects consumes the read-only BuildView and reuses existing manually written labels in
all five locales; no machine translation was added. Its regression is
`projects: local build status uses the read-only channel view before a mission exists`.

The native compatibility probe exercises real Grant, Runtime, Mission, Budget and Ledger codecs where present,
preserves unknown fields, compares both receipts with the input, and never rewrites the input. Schema upgrades
remain conservatively blocked in this MVP. This is not permission for destructive migration.

T5 still supplies the private acceptance/other-vendor receipt, accepted-landing event, settled-operation observations,
daily reservation and separately granted real canary, miniature goal/writer/proof/review/landing workflow, project
reconciliation and mission resume. There is no screen command that can supply those assertions. Automatic
stage/transition registration, the real canary and the headless-next-fixes-the-tiny-test acceptance remain partial
until that runner is merged. A separate imitation runner would not prove those contracts.

Twenty-one unit guard removals and four native guard removals compiled and failed their regressions, including
the original helper rotation before refused quiescence, stale Healthy/reuse checkpoints, forged candidate schema,
cleanup aborts, clean exit, path aliases, temporary-file leaks, receipt retention, incomplete publication and fresh
compatibility. Native removals caught a missing launcher, ignored session intent, visible observer and the mapped
previous helper image after a full 300-second probation. Every mutation was restored before final proofs.

Proof and mutation logs are archived under `target/sd-t11w/proofs`; final command results, memory measurement and
exact-head Windows/macOS CI belong to draft PR #10. The session event handler is exercised synthetically on the
hidden desktop; the maintainer's PC is never shut down or logged off. Unix containment retains the existing
direct-child limitation. Full G6/G8 and updated independent review are not claimed by these wiring regressions.

Local pre-commit validation: all eight required commands exited 0. Both `cargo t` and `cargo t --all` reported
1,094 passed, 0 failed, 0 not run and 0 skipped, including the hidden Windows screen suite. The maximum reported
screen memory was 231.9 MB, below the inherited 233 MB reference; the 250 MB gate is unchanged.

## Phase 1 integration

The merge from `origin/selfdev` preserves the local startup/probe gates, shutdown checkpoint and release-updater
exclusion alongside counted keep-awake, opt-in tray autostart, the UX sentinel and Chat send-while-busy.
The three textual conflicts were the shell's state fields, early argument handling and state initialization.

Development autostart registers the immutable enrolled helper with `--launch`, rather than a reusable slot path.
Registration verifies the helper and persisted enrollment, so login works without development environment variables.
A direct slot `--autostart` also discovers that enrollment and delegates before any slot lease or GUI runtime;
corrupt enrollment fails closed. Installed release autostart retains `faber --autostart` and its tray behavior.

`development_autostart_uses_the_trusted_launcher_instead_of_a_slot` failed on the original slot command and now
checks both slots, changed-helper rejection and the installed release command. The hidden regression
`local_update: autostart uses the trusted launcher to recover a broken candidate without environment variables`
failed attempting to execute the poisoned candidate; it now checks immutable last-good rollback, saved data,
headless launch and direct slot delegation. `legacy_slot_autostart_discovers_enrollment_and_refuses_an_untrusted_entry`
covers slot aliases, corrupt enrollment and unknown entry points without processes or registry writes.
The existing full five-minute handoff now resumes a clean-exit probation through the same autostart command.
All registry interactions in screen tests remain fake; only owned temporary slots and hidden binaries are used.

## Task 5 quiescence integration

The next merge retains `disk` and public `files`, the runner's shared async mission lock alongside local lifecycle
state, and both runner and local-update screen scenarios. The local exact-tree git invocation also carries T5's
new private-index field without borrowing the owner's index.

Build transition, probation promotion, recovery and helper startup check authoritative private runner records,
pending effect receipts, nonretired leases and worker termination/cost reservations. A caller's idle claim cannot
override those records. Running or planning missions must pause first. Unknown effects, held/releasing/unsafe
quarantined leases, unfinished heavy proofs and unsettled workers keep both build payloads intact.

Mission start, resume and worker dispatch hold the same OS coordinator as the lifecycle helper. The guard rejects
admission during quiescence, swap and the whole probation interval. Both GUI and headless engines install it.
An update pause request uses T5's existing stop path under the mission lock: pause admission, checkpoint the runner,
interrupt and close owned sessions. It neither releases a lease nor declares unknown cost settled. The old slot
exits only after durable quiescence and a committed swap; the helper waits for its OS lease before starting next.
Completed missions also close warm owned sessions without reopening the mission or settling unknown spending.

`running_mission_blocks_transition_despite_the_callers_idle_claim`, `running_mission_blocks_promotion_until_paused`
and `paused_mission_with_an_active_lease_blocks_rollback_without_losing_last_good` failed before the guard and pass
after it. `pending_effect_and_heavy_proof_block_rollback_without_mutating_builds` protects unfinished effects.
Engine tests check coordinated admission through probation and owned worker stop without erasing spend or leases.
The hidden handoff now starts with a running mission, verifies refusal, lets the actual old engine pause it, then
retries transition and checks admission in the actual candidate before its full five-minute probation.

The quiescence seam is wired. The separately granted real canary, automatic accepted-build registration and complete
tiny-goal repair acceptance still require their production receipts and qualified driver path; this integration
does not substitute an imitation runner. Uncertain worker authority intentionally requires reconciliation before
a swap. No recorded PID is adopted or killed, and no lease or spending record is deleted to make an update proceed.
