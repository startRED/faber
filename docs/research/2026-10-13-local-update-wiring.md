# Local development lifecycle wiring

Scope: Task 11, spec section 11, rework-fit step 6, R103/R106 and review-sd-t11. This is a development adapter;
the installed release still uses `fetch_update`, the signed Tauri updater, `release.yml` and `latest.json`.
No downloader, signature system, installer, UI redesign or production deployment is added.

## Entry points and ownership

`src-tauri/src/local_update.rs` is shared by the shell and the independent `faber-local` binary. Development
requires explicit `FABER_CHANNEL=development`, `FABER_DEV_ROOT`, `FABER_INSTALLED_DATA` and
`FABER_INSTALLED_RELEASE`. Local children receive the separate development data folder. Protected release
paths, overlapping roots, aliases and links are refused before staging. Neither debug mode nor the test feature
selects development implicitly. Both Windows executables have the GUI subsystem even in debug builds.
The development data root is directory-anchored before engine startup; development WebViews explicitly use
that root's profile even on an ordinary user-requested GUI start, rather than a platform default profile.

One OS coordinator lock serializes staging and supervision. Each running slot holds a separate OS lock for its
entire lifetime. A swap is durable before the helper starts; the helper waits for the old slot lock without adopting
or terminating any PID. Build children and the probation child use the engine's guard handshake and an owned
kill-on-close Windows job. A healthy child is handed off only after five minutes. Rollback verifies the immutable
last-good payload before launching it once, with no window or installer. The independent transition checkpoint
also handles a corrupt live manifest, and is retired after rollback so corruption cannot replay it.

The review corrections are recorded in [local-update-review-fixes](2026-10-13-local-update-review-fixes.md).
The enrolled launcher is now immutable and carries `development.json`. Launch it with `--launch` for a headless
start or `--open` for an explicit user open request; never use a slot binary as the development entry point.
Healthy/pause commits supersede the transition checkpoint before the live write, and preparing another slot
checkpoints current before reuse. Cleanup failure cannot suppress rollback launch or healthy handoff. Orderly
exit pauses without quarantine; the hidden Windows session observer handles shutdown/logoff without activation.

R106 cleanup removes superseded inactive payloads and known build outputs automatically. Current and last-good
are retained. A build ownership marker prevents sweeping uncertain directories. Slot locks prevent cleanup of
a live executable. A minimum 4 GiB free-space check, or the larger requested estimate, precedes snapshot/build;
unknown free space blocks admission. Rejected identities remain in the pure manifest after their files are deleted.

## Runner and Projects seams

T5 supplies `Runner`: private exact-tree acceptance plus other-vendor review; contained build; reserved fake and
real canaries; and old/new lossless data round trips. `Stage` snapshots git objects without checking out or modifying
the owner's dirty worktree. Links, submodules, metadata and escaping paths are rejected. File inventory is frozen
before acceptance and checked after proof and build, so either source mutation or injected source blocks staging.
`native_build` copies already-installed dependencies pinned to the accepted lockfile and runs native Node/Vite and
Cargo directly, offline/locked, with the target inside owned staging. It never installs or bundles an installer.

Compatibility runs on copies of development data. The frozen miniature canary has a separate root and goal,
narrow source lease, named failing normalization test, mandatory independent review and spending reservation,
no remote effects and no recursive update. Missing canary availability keeps the built slot waiting; it cannot
quiesce or promote. A later accepted tree can coalesce that unpromoted candidate. A swap or probation cannot
be superseded. The preparing timestamp precedes the canary, so real canary duration does not invalidate evidence.

T5 calls `transition` only while its window/turn/tool/proof/git observations remain settled, after flushing durable
state and settling owned processes and costs. The existing pure machine rechecks current trust, repo and revision.
Health is signaled separately from the manifest; startup validates saved JSON without rewriting spend or grants.
Resume requires acknowledgment and the first bounded operation. A user open request cancels quiescence or
waits for health. A local headless start runs only the engine, deferring the entire Tauri runtime until an explicit
open request: Tao otherwise creates a visible event-target HWND even with no WebView. The helper generated with next only
becomes eligible for a future transition, after that slot has earned current-good status.

T9 owns presentation. The only new screen command is read-only `local_update_state`; no screen command accepts
proof, compatibility, quiescence or canary assertions. No T5 runner file is changed.

The review follow-up connects that read-only view to the existing Projects build card, including before a mission
exists, using its existing translated labels. The default `Runner::compatible` now runs bounded headless codec
probes in old and next on copied private data. `native_build` obtains candidate schema/hash identity from the built
executable. Unavailable canaries stay pending; an intact accepted payload can retry without rebuilding, after a
fresh compatibility check. T5 still supplies acceptance, canary execution/reservation and settled-operation gates.

## Evidence and remaining qualification

Test-first evidence includes initial missing-adapter/API failures and behavioral failures for clock reversal,
corrupt-manifest recovery, missing frozen canary, and post-acceptance source mutation. Eight individually removed
production guards compiled and failed their regressions (channel, space, source, compatibility, payload, health
deadline, clock, ownership); all mutants were restored before final proofs. The desktop probe caught Tao's visible
event-target before the headless-only engine fix. Shell tests cover every
adapter gate with synthetic executors and temporary files; they never spawn a process or use real git/CLIs.
The existing pure local-update target remains required. The Windows screen scenario uses only temporary slots,
the hidden test build and the independent helper. It proves forced early-crash recovery, a running restored engine,
one rollback, exact retained bytes, unchanged spend/grants/extensions and absence of visible windows.

Full G6/G8 remain unqualified until T5 connects its real acceptance, reservations, canary workflow and mission
resume. Projects now presents the local read-only status. No real-CLI canary, paid probe, full native slot build through the accepted runner,
public release, installed app mutation or cross-vendor implementation review is claimed by this adapter's fixtures.
Windows owns descendant containment; the Unix path retains the repository's existing direct-child limitation.
Final proof logs and CI results belong to the draft PR, not a claim that the full self-development loop is accepted.
