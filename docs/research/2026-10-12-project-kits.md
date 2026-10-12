# T5K: automatic kits and fixture proof

Date: 2026-10-12. Scope: the pure T5K slice, with original synthetic fixture replays. No real model session,
paid credit, CLI credential read, installation or third-party bundling is used by these proofs.

The scope follows R105's fit order, the local `faber-notas/pesquisa/rework-fit.md` mapping and its "Delivery vs safety"
rule, [tool-selection section 17.1 and its status note](../superpowers/specs/2026-10-12-tool-selection-rework-design.md),
[master section 10.1](../superpowers/specs/2026-10-12-faber-rework-design.md), the
[MVP plan](../superpowers/plans/2026-10-12-self-development-mvp.md) and the generated-pipelines contract.

## Published boundary

The first implementation commit publishes wire schema **2** once, including kits, labels/facets, bar sheet,
check/compile receipts, pings, verbatim requirements/checks/quotes and assumptions/unconfirmed. Durable journal schema
stays **1**: old T1 records remain readable; unknown durable root fields survive; unknown wire/nested fields fail.
Role authority is unchanged. T5B and T5 consume these types rather than introducing competing wire revisions.

`engine/src/project/kit.rs` exports the assembler and the `inventory`, `request`, `delivery`, `picker`, `passes`,
`gaps` and `measure` modules. No handwritten kit database exists. Reported tools and bounded repository sections
produce card-writing questions, whose closed replies are validated and cached by content/version. Native init parsing
whitelists reported names/descriptions/version and never reads configuration or credential folders. An absent body
stays unavailable. Cached metadata cannot restore old source permissions or paths. Named engine proofs remain required.

The immutable inventory snapshot checks card shape, IDs, paths, origins and hashes before hashing/indexing. Selection
uses normalized weighted needs (including purpose priors), examples, origin priors, symmetric conflicts, project job
ownership, explicit pins, machine requirements, stages and role budgets. `process.*` jobs and unavailable native
capabilities never load. Pins can exceed the token budget with a receipt flag, but cannot override safety, conflicts or
the during-skill cap. Per-version Beta outcomes break ties only. Shortlists hide names/jobs behind opaque IDs, shuffle
by seed and bound candidates; invalid/late shadow replies never replace the code pick.

`compose_role_prompt` takes T2's safe role prompt as its unchanged prefix and adds JSON-encoded untrusted references.
It verifies selected version/path/hash and rejects credential-home paths, arbitrary bodies and oversized content.
It neither removes driver restrictions nor enables native tools/hooks/plugins/MCP. Worker environment data is
allow-listed. Init attestation compares against the inline-only native surface, records mismatches, missing fields,
reported model/effort/version and explicit best-effort limitations. `containment_proven` remains false on every path.
The composer is an adapter; it is not a replacement for T2 policy or its admission gate.

Selections persist atomically before dispatch; init persists before outcome completion; completion cannot rewrite
selected content or init. Corrupt/newer records block, disk-full writes remain errors, and future receipt root fields
survive completion. Outcomes derive idempotent per-version Beta evidence. Briefs are bounded to 3,000 tokens, cited,
hashed, versioned and invalidated after 30 days or a version change. G0 has no candidates; G1 only suggests up to three
cited candidates with downsides; G2 fetches supplied primary URLs through the existing fetch callback; G3 provides a
knowledge floor under verified role controls. Installs always remain parked. Required unavailable capabilities park
their node, not independent nodes. Presence supplies two start-card choices as locale keys.

The Auto adapter appends labels to the same classification prompt without changing the existing choice table.
Facet inheritance fills only unstated values. Pass kits request fresh sessions for a base pass and at most two UI
polish passes, or optional simplify; prototypes skip polish. The pure keep rule requires proofs, another vendor's
judge winning both orders, remaining budget and pass cost at most 30%; the previous result wins ties.

## Red and green evidence

The initial kit tests failed before the module/Journal APIs existed. After implementation, 29 of 30 passed and the
2,000-card performance target failed: p50 **157.9 ms**, p99 **216.2 ms**. Instrumentation found repeated whole-inventory
validation/serialization/hash work at every node. Moving that work to the immutable, validated inventory snapshot
removed the repeated cost without weakening validation or raising the limits. Temporary instrumentation was removed.

The rerun measured 100 assemblies over a synthetic 2,000-card snapshot: p50 **2.359 ms**, p99 **3.543 ms**, below the
**20/200 ms** limits. This is debug-profile local Windows assembly timing; snapshot preparation is outside this
per-node timing and is not asserted to be free or a measured covered-start delay. The test prints a machine-readable
report with `source: synthetic`, missing production measurements as null and `realGoalProven: false`.

Additional regressions first failed for completion without a persisted attestation, a persisted model alias, and
cached source permissions after availability changed. Their fixes preserve frozen receipts and validate both fresh
and persisted selections. `engine/tests/project_kit.rs` covers these boundaries, original init/gap fixture replays,
fingerprint scope across software/non-JS inputs, cache reload/versioning, gaps/passes, immutable approval pins,
unknown costs and G9 aggregation. The 28 T1 journal tests and six wire-v2 tests also pass in the focused run.

Full proof exposed an existing draft-loss race: the conversation title can arrive before its snapshot, and moving
from the greeting to the dock recreated the composer. A controlled 800 ms fixture delay reproduced the lost text and
caret. Keeping one composer per conversation and restoring its existing focus after the DOM move passed the regression;
the typing test finishes complete left/right key pairs. No timeout is raised and no click or command is retried.
The label fixture also first failed with the specified 22 old/new pairs, then passed after correcting the aggregator's
sample count from 44 pairs to 22 pairs (44 calls).

Reproduce the kit proof without a real CLI:

```text
cargo test -p faber-engine --test project_kit -- --nocapture
```

Use the maintainer's Rust paths, the isolated `sd-t5k` target and `line-tables-only` dev/test debug settings.
All seven AGENTS proofs plus `cargo t --all` are the commit gates. Hidden screen tests use testkit's fake CLI sandbox.

## Measurements and integration still required

The aggregator computes assembly p50/p99; 22 paired label samples (44 old/new calls), token/cost overhead; plan
tokens/node; own-cost tags `kit.label`, `kit.card`, `kit.gap`, `kit.pass`; selection share; gap latency/cost; covered
start delays; resident-token error; card-writing costs and pass costs. Fixture inputs prove arithmetic and missing
evidence handling. They do not establish live targets. No live label/token/cost data is invented.

T5 consumes the classification adapter and supplies scoped bodies, source truth, inventory snapshots, approved
pins, fresh sessions, dispatch timestamps, outcome usage, phase costs and proofs. It must gate dispatch on successful
selection persistence and T2 admission, then gate work on recorded init; mismatch retries can only remain inline.
T5B supplies the executable native software-bar proof IDs and check receipts. T9 renders the presence choices and
handwrites the locale texts. T3 grants/admission remain untouched by this slice.

The owner forbids real sessions/paid credits in this task. Therefore native delivery/escape probes, the live paired
classifier measurement, resident-token calibration, real G8/G9 receipts, first-inventory cost, live gap/selection/pass
costs and the held-out family picker gold set are **unproven**. T2's actual isolation gate and T13's real-goal acceptance
must pass before automated roles or a native delivery capability can ship. Probe-lane/transfer, catalog, installs,
tool servers and learned picker rollout remain outside this MVP slice.
