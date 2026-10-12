# Task 2 second review disposition

Baseline: `1dddf27`, branch `selfdev-t2`, 2026-10-12. Reviewed the Opus 5.5 report against code,
controlled fake Proc tests, hidden fake CLI scenarios and installed help. No model session or additional
subscription probe was started. The six previously authorized slots remain consumed.

## Findings

| # | Disposition, reproduction and change |
|---|---|
| 1 | Real. Paused-clock tests reproduce a manual long tool being interrupted after silence. Only automatic roles now arm silence; explicit manual stop remains bounded. A failed Codex `turn/start` clears its watch/identity. The automatic role-loop test proves it does not later emit a second timeout error. |
| 2 | Real provider mismatch, other escapes latent/unconfirmed. The argv test failed because production omitted the probe's provider pin. Put `model_provider="openai"` in the shared role argv and remove the probe-only addition. Direct patch/config planting, hostile provider keys/notify and account verification remain unlock requirements; no fake test proves the official sandbox prevents them. Keep the spec's writer sandbox and deny admission. |
| 3 | Real missing containment evidence, not a current credential leak. Negative role-entry tests still deny every vendor before spawn. Read/network/native-code capability remains unproved; credential-path tests are forbidden. Document these as hard unlock requirements. |
| 4 | Real. A completed turn followed by queued interrupt killed the warm session in the red fake test. Ignore interrupt when no turn is active; the warm session accepts its next send. |
| 5 | Real synchronous guard wait and missing warm-memory measurement. A fake guard cannot acknowledge until the spawner writes its return marker: the previous spawner deadlocks until timeout. Move the same five-second handshake into async I/O, buffer input until acknowledgement, and kill the shared owned job on failure. The engine lock no longer waits for guard acknowledgement. A hidden warm-app scenario measures the guard and whole tree. Native process creation itself is still synchronous. |
| 6 | Real. Launch the cache example in guard mode with a fake echo CLI; its old entry printed help instead of acknowledging. Dispatch guard mode before constructing Tokio's runtime, then run the measurement tool normally. Testkit builds this example as a trusted guard host; tests never run its model scenarios. |
| 7 | False as a violation of this task: the owner explicitly required **every** spawned CLI to use kill-on-close. The fake child/grandchild tests prove this required behavior. User programs inherited into manual CLI jobs share that lifetime; do not promise detached servers/editors survive stop. No real browser/editor was launched. |
| 8 | Real qualification/coverage gaps, not a PID-kill bug. There is no PID-based stop API, so the stale-record case is a simulated identity collision plus repeated-stop safety proof. New private role-loop tests deliver all nine fixture requests through both drivers, not just the helper. Approval denial alone never proves denial of unprompted tools. Qualify Windows termination as inherited-job ownership; WMI/task/COM delegation remains a Restrictions prerequisite. |
| 9 | False as a present role-admission bypass. The public manual APIs are required for existing Chat/manual Projects; they have no role parameter. Actual role entrypoints still deny before spawning. Future Task 5 dispatch must use them and prove it cannot fall back to manual APIs; adding a speculative admission-token API would not prove that caller behavior. |
| 10 | Real invalid role-resume acceptance. The regression accepted `--add-dir`, `--help`, empty and non-UUID values. Validate Claude role resume IDs with the existing UUID dependency before argv or spawn. Installed help confirms `--resume [value]`; normal manual/Chat launch defaults remain unchanged. |

Initial red observations: manual sends returned an interrupt instead of the next user packet; the idle interrupt
emitted a fatal error; provider-pin and invalid-resume assertions failed; the synchronous guard and cache host
failed their hidden roundtrips. Tests were added before those changes.

## Six unconfirmed items

| Item | What can be settled without a model |
|---|---|
| U1: long Claude tools/thinking | Help advertises partial messages but makes no heartbeat/progress guarantee. Actual emission remains unknown. Manual turns no longer depend on it; fake-role silence/stop bounds are tested. |
| U2: Codex response ordering | Reordering the fake recording reproduces the lost completion, including a stale completion first. Hold at most two early completions until the RPC response confirms their ID, then process only the match. Excess unidentified completions fail the affected thread closed. Installed native ordering remains unknown; the driver no longer relies on it. |
| U3: trusted linked repo config | Help supports `-c key=value`; the provider mismatch is fixed. Config loading/trust, notification execution, merge precedence and protected paths cannot be proved by help or a fake CLI. Keep restrictions denied pending a separate authorized hostile-config probe. |
| U4: residual native code, reads/network | Historical `mcpToolCall` metadata does not identify the tool. No model session, credential read or network escape probe is authorized here. Unknown read/network capabilities remain blockers for writers and read-only roles alike. |
| U5: Claude `@path` expansion | Installed help gives no restricted mention-expansion guarantee. Fake user packets contain ordinary text but cannot prove the official CLI's expansion rules. Keep it an explicit unlock prerequisite; never probe credential paths. |
| U6: linked git/resume/non-Windows | Both fake role loops now decline the linked `.git` and common-directory fixture attempts. That proves wire denial, not filesystem isolation or resumed native policy. Non-Windows retains direct Tokio child ownership and its tree blocker; macOS CI checks the shared code, not real-CLI containment. |

Help-only checks: Claude Code `2.1.296`; Codex `0.162.1`. Native executables used
`UseShellExecute=false`, `CreateNoWindow=true`, redirected output and a ten-second help deadline.
No global config or credentials were read or changed. The original live evidence is not rewritten to claim
these later changes were tested against a real subscription.

## Scope and safety

`Restrictions` and `Exposure` still deny all automated roles on every platform. Windows job ownership is proved
for inherited descendants, not services outside the job. Non-Windows tree termination remains unproved.
The async guard adds two bounded 8 KiB buffers; owned job closure on stop/drop/crash still kills all inherited
members, including when startup has not acknowledged. CLI input cannot precede the one-byte acknowledgement.
No retry or longer timeout substitutes for a fix. The hidden guard test releases startup only after spawn returns,
so its pass does not depend on a faster machine or a longer deadline.

Local build issue: the cache host linker stalled in `mspdbcore.dll`; after ending only that verified owned linker,
Cargo reported `LNK1318`, PDB `LIMIT (12)` for `cache_probe.pdb`. The hidden helper build now uses
`cargo rustc ... -- -C debuginfo=0 -C link-arg=/DEBUG:NONE` for this example only. This removes PDB generation,
not a test or deadline. Installed MSVC help advertises `DEBUG:{FASTLINK|FULL|NONE}` and
[Microsoft documents `/DEBUG:NONE`](https://learn.microsoft.com/en-us/cpp/build/reference/debug-generate-debug-info?view=msvc-170).
Production build flags are unchanged; no failed CI run is retried.

The first full run found a bridge EOF regression: eleven tutorial checks timed out and the warm-memory
scenario had no live fake CLI. A new fake `--eof` child reproduces missing stdin closure after dropping
the caller's writer. Tokio's Windows pipe shutdown flushes but does not drop its handle; borrowed handles
inside the joined futures remained alive waiting for the other pipe. Move each handle into its forwarding
future so completion releases it independently. Keep both the EOF and acknowledgement roundtrip tests.
The first warm-memory attempt also used a plain recording, which intentionally logs no command records;
use the existing signed-in setup recording to identify the warm fake CLI. The corrected focused hidden run
passes all four scenarios: acknowledgement/EOF 815 ms, cache dispatch 618 ms, warm guard **2.0 MB** and whole
app tree **120.0 MB**, stop/drop/crash each remove four inherited processes in 2.3 seconds for the complete scenario.

Shared files touched this round: `engine/src/claude.rs`, `engine/src/codex.rs`, `engine/src/proc.rs`,
`engine/src/project.rs`, `engine/examples/cache_probe.rs`, `engine/tests/claude_session.rs`, `engine/tests/codex_session.rs`,
`testkit/src/bin/fake-cli.rs`, `testkit/src/gate.rs`, `testkit/src/testbuild.rs`,
`testkit/src/screen/mod.rs`, `docs/superpowers/plans/2026-10-12-self-development-mvp.md`.
Task-owned files: `engine/src/project/policy.rs`, `engine/tests/project_policy.rs`,
`testkit/src/screen/project_drivers.rs` (renamed from `projects.rs`), `testkit/examples/project-policy-probes.rs`, and this report.
No edits to the git broker/lease implementation or other worktrees/branches.

## Final local proofs

All required commands exit 0 before commit: `npm run lint`, `npm run typecheck`, `npm run build`,
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo clippy -p faber --features test-hooks -- -D warnings`, `cargo t --all`.
The complete final run reports **602 passed, 0 failed, 0 not run, 0 skipped**; all previously failing tutorial
checks pass after the tested EOF fix. The three opt-in runner unit tests also pass without calling its main.
Final hidden measurements: guard **2.0 MB**, whole warm tree **117.2 MB**; acknowledgement/EOF 856 ms,
cache entry 729 ms; stop/drop/crash four inherited processes each, complete tree scenario 2.6 seconds.
PR #3 CI must separately pass Windows and macOS for the pushed head; no reruns substitute for repairs.

The base advanced during review. PR metadata initially retained the older base; an idempotent base refresh
revealed Task 4's shared registrations and its separately added `screen/projects.rs`. Task 2 driver scenarios
now use `project_drivers.rs`, with separate registration. Move policy registration and the fake guard hook
away from Task 4's insertion points. A staged three-way preview has no conflict markers; no branch/index merge
or rebase was performed, and git/lease implementation files were never edited. Preserve the accepted base plan
text together with Task 2 evidence. These integration repairs require the same full proof set before commit.
