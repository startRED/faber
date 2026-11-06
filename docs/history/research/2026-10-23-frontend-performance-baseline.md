# Frontend performance scenario: functional evidence, 2026-10-23

Status: the new native scenario passed its functional contract. No quiet performance baseline or improvement has been established. This is a dated observation, not a product rule.

## Scope and environment

This change adds test instrumentation only, with no runtime optimization, dependency, telemetry or service. The original functional observations below use the worktree based on `94ad1137`, without the later reading change. They cannot be compared directly with reading measurements from a different base. A separately identified sequential common-base check is recorded below.

Observed environment: Windows 11 Pro 10.0.26200, Intel Core i7-10700K (8 physical cores, 16 logical processors), 35.85 GiB visible RAM. Installed WebView2 executable metadata reports 155.0.4283.62; the version actually used by the running app was not captured. No device serials, user identifiers or credentials were collected.

Other proof/build workloads were active, including the send-persistence work; Windows and other projects were not excluded. The scenario is SOLO within its own runner, which does not isolate it from other runners or the rest of the PC. Every timing and CPU observation below is invalid as a quiet performance baseline.

## Reproducible contract

Run the existing hidden native harness with `cargo t --suite screen --filter 'performance baseline:'`. It reuses the existing thousand-message generator. The original scenario requests the tool-free variant; the new variant deserializes through the real engine Conversation/ToolUse contract and requires 1,000 messages, 500 tools, and exactly 40 output lines per tool, each at most 2,048 bytes.

The scenario checks bounded initial answers, loads one earlier page, checks the mounted answer/tool-group bounds, and requires the groups to remain closed. It collects 120 finite, nonnegative samples per operation and reports nearest-rank p50/p95/max. Native CDP `Input.insertText` must produce 120 trusted input events and exactly 120 letters. Synthetic scrollTop must actually move the thread; CDP viewport emulation must reach both 720 and 1,028 px. The fixed active window also verifies the native edit count and exact final text. Whole-process-tree sampled memory remains subject to the unchanged 400 MiB gate. Cleanup and the existing sandbox audit must pass.

The fake CLI configuration is reused. No assertion establishes that no CLI process exists: detection, warmup or routing may use fake CLI processes, and resource sampling includes the whole app tree.

## Functional observations under concurrent load

Native run 91458 exited 0: one scenario passed, no failures. The following observations describe that functional run only.

| Observation | Samples | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Trusted CDP insertText input event to two rAF | 120 | 3.80 ms | 7.00 ms | 8.40 ms |
| Synthetic scrollTop round trip plus two rAF | 120 | 11.19 ms | 14.59 ms | 70.96 ms |
| CDP viewport resize round trip plus two rAF | 120 | 58.59 ms | 99.92 ms | 146.40 ms |

Whole-tree private working set: 212.0 MiB loaded, 301.6 MiB sampled peak. The resource gate passed. The nominal five-second windows observed 31 ms CPU over 5,010 ms with no scenario action, and 7,922 ms CPU over 5,046 ms during 60 input/scroll/resize cycles. Those are 0.6% and 157.0% of one core; they are observations under concurrent system load, not isolated idle cost or a performance improvement.

## Failures and correction

The interrupted first attempt left no native result. A later pre-launch claim collision was coordination, not an app failure; only this lane's app.rs claim changed from own to share after disjoint scope was confirmed.

The first completed native scenario failed at its first input sample. A read-only diagnostic confirmed a focused, connected textarea, the same listener target, empty text, zero trusted input events, and zero samples. An isolated two-rAF probe completed. The existing shortcut helper deliberately omits text for letter keys, so `press("a")` could not type; it also has no Backspace mapping. The scenario now reuses the existing native insertText path and sends Backspace through the existing CDP call API. No app behavior, global key mapping, timing limit or synthetic JavaScript input event changed. The corrected scenario passed.

The original tool-free thousand-message scenario was also run (20274) and exited 1: opening took 4,257 ms against its existing 3,000 ms limit. Startup took 57.6 seconds. Its fixture branch and limit remain unchanged. This is a real failed timing check under concurrent load; neither a product regression nor its cause has been established. That first failure remains recorded alongside the single exact-source control and one predeclared confirmation below.

## Single exact-source control and verification

After the seven local checks passed, one control used app.rs and mod.rs exactly from 94ad1137, with the same native executable, hardware and existing 3,000 ms timer. Both current source files were backed up and restored byte-for-byte in finally. The native executable SHA-256 was identical before/after (DBC24696A0D5EAB627702AEE4C74954827654AD8AAA724CE830F7C9A873ECDDA). No other runner was started in this worktree during the control; rootguardRED68996 and other project/system load remained concurrent.

Rust programs compiled from the original and extracted generator bodies produced byte-identical tool-free JSON (SHA-256 8A459B12BD40ADDEB113B36D298FFA4F7EAA9FC835498D8BB1419D0866242D68). The original launch, opening timer, click, waits, notes and budget checks after fixture preparation were also textually identical. These comparisons locate no changed application input or opening-timer mechanism.

| Original-scenario observation | Opening | Result |
| --- | ---: | --- |
| Extracted generator, run 20274 | 4,257 ms | Failed the unchanged 3,000 ms gate |
| Exact pre-extraction source, single control 42971 | 1,163 ms | Passed; 200 MiB observed |
| Run 7386 after source restoration, stale control runner | 2,163 ms | Executed successfully, but does not verify the candidate |
| Rebuilt candidate, single artifact-verified confirmation | 1,085 ms | Passed; 208 MiB observed |

The first extracted run failed and the exact-source control passed. Run 7386 cannot approve the candidate: a later artifact audit found that Copy-Item had restored old source modification times, so Cargo reused the control runner compiled at 03:59:18 UTC. The saved pre-refresh runner lacks the new scenario name and has SHA-256 01a856789d5a927a5e1127ddb072bf7c6863ccaa230802830b64ba887813424a. Its unchanged app executable hash proves only the app, not the runner that contains the generator and scenario registry. The 2,163 ms observation is retained as a stale-control-artifact run. The earlier claim that it verified the candidate was incorrect.

The corrected confirmation rebuilt the runner through Cargo after refreshing only the restored source modification times. Cargo reported both runner artifacts as not fresh; the runner SHA-256 changed to 9B3BE1D0257A0CFE899B5DB3189877D7AFB0CCB7BDA483EEEE6806C7D4EDF961 and its new scenario name was present. That exact executable ran the original scenario once, retained its hash before/after, and exited 0: opening took 1,085 ms, with 208 MiB observed. Concurrent work remained active. This verifies the restored candidate can satisfy the unchanged gate; the differing uncontrolled timings establish neither a causal diagnosis of the first failure nor an optimization gain.

Before the artifact audit, the seven local proof commands exited 0: lint (existing warnings), typecheck, docs, production build, cargo fmt, scripts (101 passed, one skipped), and lane claims. Source restoration was hash-checked. All seven commands were repeated on the final restored sources and report. The last claim check initially failed because a newly active lane reserved the same fixture file and all docs; after that lane narrowed and shared its disjoint scope, the unchanged local check passed. The first failed receipt is retained. Raw evidence stays in ignored local receipts, not in this commit.

## Sequential common-base check: declared protocol

The benchmark-only delta was replayed onto 06de6a5fff14843f8d1c1a8aed192543b2f57a3a, the reading change's base. One native workload ran on that source, followed sequentially in the same worktree and target directory by the same workload with only reading change edb6a274f66b0b3666bd098e463e53b45826a15d added. Other project and Windows activity remain uncontrolled; no same-lane runners overlap. The existing scenario, dimensions, 120 samples per operation, five-second resource windows and 400 MiB bound remain unchanged; active-cycle counts are observed and asserted, not fixed across versions. A test-only three-line addition records the SHA-256 of the actual generated conversation bytes before the app launches, using the existing sha2 dependency. Each run retains source, runner and application hashes. These two functional resource observations can reveal a failed budget or workload contract; they cannot establish a quiet idle baseline or a causal latency improvement. Both native executions exited 0, with one scenario passed and no failures each. The first after-reading preflight rejected a lane area name before launching the app; only that local reservation line was removed and its failed receipt was retained.


The before source was 26eea54ddbbc415b89a5dd29ea2024df5ab2f403 plus the three-line fixture-hash instrumentation; the after source was c6653e968711c2d79ca6abbfcaf986186acb3360 with the same instrumentation. The fixture generator and scenario source hashes were identical. Both actual conversation files hashed to 76db123390fdf49dddbceb7a3e2d24ba9e2db05e06f4a6b6f5649a197606c1df. A separate whole-ui comparison against the authored reading delta edb6a274, before landing-time release-note numbering, exited 0. The temporary reading commit is not part of this benchmark branch.

| Observation | Before reading | After reading |
| --- | ---: | ---: |
| Whole app tree loaded | 210.9 MiB | 214.2 MiB |
| Whole app tree sampled peak | 292.3 MiB | 306.6 MiB |
| No scenario action: CPU / wall | 94 / 5,005 ms | 125 / 5,006 ms |
| No scenario action: one-core CPU | 1.9% | 2.5% |
| Active: CPU / wall | 8,250 / 5,053 ms | 7,891 / 5,002 ms |
| Active: one-core CPU | 163.3% | 157.7% |
| Active cycles completed | 55 | 57 |
| insertText event to two rAF, p50 / p95 / max | 3.90 / 6.40 / 8.30 ms | 4.00 / 6.70 / 8.60 ms |
| Synthetic scroll round trip plus two rAF, p50 / p95 / max | 11.11 / 15.96 / 69.39 ms | 12.45 / 18.64 / 24.51 ms |
| CDP resize round trip plus two rAF, p50 / p95 / max | 61.46 / 79.23 / 96.18 ms | 70.67 / 97.22 / 106.17 ms |

Both passed the unchanged whole-tree memory gate and all operation/count assertions. The after observation had 14.3 MiB more sampled peak memory and more CPU without scenario action; those increases remain visible rather than being dismissed. The active windows completed different numbers of cycles, as their unchanged five-second loop requires. External load was not controlled, and there is only one window per version; neither attribution of the increases to reading restoration, absence of a regression, nor a general speed improvement is established. This is functional/budget evidence with explicitly limited cost observations.

Executed artifact identities (SHA-256):

- Before runner: 3DDF01B01FD681FC41ED957AE4310FD8D849B0AAC1384A89C86900980DF346BD; identical before and after that invocation.
- Before app: 4E909F14869462EDA2B5F42E1057ADDCC21C3C7E572BF76B8E69112F94B2DB66.
- After runner: 581415BDE6A7C2518FD46AECC99C05055A88F2A67E1A2313A88187500D4BAD45; identical before and after that invocation.
- After app: 052F54BEEE14BBC1D0D607CC6C1343A22BBE653B86D1CBCE1EE0B482EC6EC0E9.

Each runner was built through Cargo in this worktree before direct invocation, contained the requested scenario name, and used the normal app build path rather than prebuilt mode. Sources were switched with git, not timestamp-preserving file restoration. No second runner used this lane's target during either invocation. Full source/build receipts and both native outputs are retained locally. This branch was returned to the benchmark-only version after the pair; it adds no reading implementation or shipping behavior.

For delivery, the benchmark-only commit was subsequently replayed onto 406b0d452cbdb4e3e619d0c21abcd675a89d859b after reading had landed. The normal queue had rejected the single scenario-registry conflict; its resolution preserves both reading and performance registrations. The final delta still contains only four benchmark/research files, without a second reading implementation. Before the Clippy notation follow-up below, the three test sources matched the after-pair sources, including the registry hash. A full UI comparison with that historical pair exits 1 because five inherited locale files contain numbered release notes; excluding only those locale files gives exit 0. Therefore the final source inherits the delivered notes and must not be described as the exact historical pair executable. The historical measurements and their artifact identities remain unchanged; the final queued build has its own identity and gate.

## Queue follow-up: standard parity predicate

The queued package failed the Rust 1.99 Clippy stage on both operating systems at the scroll helper's manual parity check. Only that expression changed from `i % 2 == 0` to `i.is_multiple_of(2)`; both select 0.25 for even usize indices and 0.75 for odd indices, for the fixed nonzero divisor two. Both call sites, operation counts, viewport targets, timing labels and budgets remain unchanged. No lint suppression or additional test framework was added. Historical scenario and executable hashes above identify the measured versions and are not rewritten to identify this notation-only revision. The affected Clippy target and seven required local commands are checked separately on the revised source; there is no new native pair measurement or claim that the final executable is a historical measured artifact.

## Limits and provenance

Two rAF callbacks estimate scheduling/layout; they do not acknowledge paint or establish true INP. insertText does not measure the keydown handler path. Synthetic scrollTop does not cover a real wheel gesture, and viewport emulation does not cover an operating-system window drag. Driver round trips are included in the scroll/resize times. No p99 is reported. One synthetic fixture does not prove all workloads or identify an optimization hot path.

The hidden test desktop prevents a visible test window; it does not prove product occlusion behavior or document.visibilityState=hidden. The diagnostic actually observed visibilityState=visible. The memory peak is sampled rather than continuous.

The ignored local receipts contain source hashes, environment metadata, full functional logs, exit codes and the independent read-only review. They are not repository inputs or committed artifacts. The original-source review and a separate fresh read-only review of the native-input correction found no blocking issue. The latter matched all three source hashes to the preserved new-scenario native pass; it did not independently rerun the app or verify the later original-scenario/control/7386 observations. Restored source hashes alone do not identify the executed runner; the artifact audit and the corrected confirmation are separately recorded with the executed runner identity. Before/after observations require a shared base, identical fixture and recorded load conditions. The declared sequential check preserves those source and fixture controls while explicitly retaining external-load uncertainty.
