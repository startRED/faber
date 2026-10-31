# Recover provably orphaned nested build reservations without reclaiming unknown owners

Area: none
Files: scripts/cargo.mjs, scripts/cargo-recovery.test.mjs, testkit/src/sweep.rs, docs/dev/build-folders.md
Depends: none
Size: about 250 changed lines; re-estimate on top of #441 before implementation.

Start after PR #441 (`fix/build-slot-leases`, "free a build cache whose command was killed") lands: this extends its
launcher. Until then `node scripts/tasks.mjs release` it and take another. Do not amend that PR, duplicate its
receipt-publication/token work, or start another implementation in parallel with its author.

Why: today's code keeps an empty reservation because it has no owner receipt; #441 checks overlaps first, so an
ancestor reservation and slot reservations stop one another before reclamation. This does not prove a real empty
reservation is abandoned, nor explain every wait.

Reproduction (virtual file tree, no real cache, nothing compiled): a root reservation `target/.faber-lease` and slot
reservations `target/build-1..3/.faber-lease` (as seen on the PC on 2026-10-31: root and build-2/3 empty, build-1 with an
owner receipt), timestamps old enough to pass every age rule, an already-aborted signal so one pass ends the attempt.
Four cases, each acquired nothing and removed nothing:
- today's launcher, root override (`CARGO_TARGET_DIR=target`): stops at the missing receipt (1 probe, no overlap scan);
- today's launcher, slot pool: stops at the missing receipts (3 probes, no overlap scan);
- #441's launcher, root override: reaping the root finds a slot lease below it and refuses (overlap, 0 receipt probes);
- #441's launcher, slot pool: each slot finds the root lease above it and refuses (overlap, 0 receipt probes).
So in #441 `reap` calls `overlapsLease` before its receipt-less recovery, and root and slots block each other forever.
Recreate these as self-contained repository tests. Extra reference while it exists: `.faber/verify/` of
`C:/Users/Erick/AppData/Roaming/Faber/copies/185ae5db-79a7-4462-99b7-dce61ce4e9bb`.

Build: extend the existing reaper's decision, under its ownership/exclusion safeguards, so a hierarchy containing only
eligible, provably ended owners can be recovered without a permanent ancestor/descendant rejection cycle. Preserve
the current eligibility ages, native process/child evidence, uncertain-child protection, no-follow checks and token
ownership rules. A recovery decision is not permission to start an overlapping parent and child concurrently.

A second writer exists outside Node: the storage sweep's `Lease` in `testkit/src/sweep.rs` (`take` creates
`.faber-lease` before it checks overlaps and writes `owner.json`; `Drop` removes `owner.json` before the folder), and #441
does not change it. Either order, cut by a kill, leaves an empty reservation, so the recovery covers this writer and
its older builds too: no proof of quiescence may rest on Node, cargo or rustc processes alone. `.faber-reap` serializes
reapers only, not acquisitions, so an admission marker counts only when every participant honors it, including legacy
ones already past their check; a cached native helper keeps its old protocol when only the JavaScript launcher changes.

Unknown, missing, corrupt or ambiguous owner evidence remains occupied. Do not infer abandonment from age alone,
an empty directory, a dead launcher with an uncertain child, a process count or a snapshot with no compiler. Report
that particular blocker once through the existing waiting diagnostic and remain cancelable. Do not manufacture a
replacement owner receipt or automatically clean the legacy empty ancestor observed in the investigation.

Keep this narrow: no new command, setting, dependency, build cache, publication protocol or grace-period policy. If
safe exclusion needs a broader protocol change, keep the failing reproduction local and propose that prerequisite.

Done: deterministic virtual/disposable fixtures over the actual launcher prove all of these, with no real cache access:

- An eligible parent/child hierarchy with complete confirmed-ended owner and child records can be reclaimed and one
  claimant can acquire the correct target; parent and child never run concurrently.
- A live owner, live child, uncertain spawn/stop, unknown legacy owner, missing/corrupt receipt or unexpected link keeps
  its reservation. A partially eligible hierarchy never releases an unsafe member to make the test pass.
- Root override and pool entry paths both terminate the isolated assertion with the expected acquisition or controlled
  cancellation, rather than hanging; the unknown-blocker diagnostic is emitted once.
- The native writer cut before it writes its owner, and cut between removing its owner and its folder (isolated
  fixtures), leaves a reservation that stays occupied; Node/native and old/new interleavings never overwrite or remove
  another participant's newer reservation.
- An interleaving that pauses publication, recovery or release never lets a stale claimant overwrite or remove a later
  owner's reservation. Include the supported-version transition; do not assume token checks in an older launcher.

Tests must fail for the targeted unsupported recovery behavior before the change. No test spawns a compiler, runs real
Git, probes real PIDs or touches the shared pool. Independent review of the exclusion argument before queueing.
Document what stays unrecoverable without a coordinated maintenance interval. Cost: measure metadata work, no
persistent process/timer or unbounded cache walk. What's-new only if waiting or its diagnostic changes visibly.
