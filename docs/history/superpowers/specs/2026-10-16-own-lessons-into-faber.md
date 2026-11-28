# Faber's own lessons, made automatic

Status: proposal, nothing built. AI proposal, not reviewed by the owner.

## Why

The owner asked for two things: build Faber faster, and have Faber, once ready, build software without the same
bottlenecks. Every lesson below was learned by hand while agents built Faber, and today it lives in AGENTS.md, the
decision ledger and the maintainer's private notes, applied by whoever remembers it. Phase 1a (R96, target 2026-10-20 to
22) is the first point where Faber runs its own development, so it is where each lesson should become engine behaviour
instead of a note.

Each row names the feature that makes the lesson automatic, the roadmap section it extends, what already exists, and a
size: **S** is up to half a day for one strong agent, **M** about a day, **L** several days. "1a" marks what Faber
building Faber needs by the target date; the rest waits for Phase 3 or 4.

R160 moved a person's project work to an agent chat, and missions stay in the engine with no way to start one on screen.
So each feature lands where the work runs: the coordinator (`engine/src/project/`) for Faber's own missions, and the
project agent's brief and checks for a person's projects. Both read the same rules.

## Lessons and the features that carry them

| # | Lesson (where it is written today) | Feature in Faber | Extends | Exists | Size | When |
| :-- | :--- | :--- | :--- | :--- | :-- | :-- |
| 1 | One strong agent per feature, one-shot, tests first, one review batch at the end (R119) | The lead writes one contract per feature, not a task list; one writer owns it end to end, writes the proof first (`proof.define`) and repairs its own failures; review runs once on the composed candidate, never per commit. The width of the chain comes from risk (row R4 below) | Building software ("Done means it works"); MVP spec §6, §9 | Contracts, leases and the serial queue in `project/`; one review per candidate | S (brief and contract defaults) | 1a |
| 2 | Re-reviews on the other vendor; a small fix goes to a fresh lean agent, never a resumed 300k-token one (maintainer notes) | Review rounds after the first go to whichever vendor is behind its pace. A repair worker starts fresh from a self-contained brief (the findings with `file:line`, the contract, the receipt) unless the writer's cache is warm and its context is small; the MVP spec's "reuse the context for related repairs" gets that bound | Guardrail 5; Running many workers ("Workers leave a written state and exit"); MVP spec §7 | Written worker state; resume vs fresh is shown, not decided by size | S | 1a |
| 3 | A red base stops the line (maintainer notes) | When the integration head's own proof or CI is red, the queue lands nothing else and dispatches one repair owner for the base first; writers keep working in their worktrees, only landing waits | MVP spec §9 (serial queue); "After delivery the worker keeps watching" | Queue lands only proven trees; no base-red state | S | 1a |
| 4 | Decision and notes numbers are placeholders until merge (maintainer notes) | A project declares its sequence files (Faber: `docs/decisions.md` R-numbers, `notes.<id>` in `ui/locales/`). Writers write a placeholder; the queue's compose step assigns the next free numbers on the current head and rewrites only the candidate's own references | MVP spec §9 steps 2 and 6; R146 project map | Nothing; sessions renumber by hand | S | 1a |
| 5 | Builds at idle priority; agents stop their own background scripts; games must not stutter (maintainer notes) | Every worker, proof and build runs in the mission's job object at idle priority, and closing a lease closes the job, so nothing a worker started outlives it. The RAM policy also counts leftovers: an owned process alive after its lease is a defect in the report | Architecture (memory budget, CLIs idle); MVP spec §8 (RAM), §10 (owned trees) | Idle priority for background readers (`proc::spawn_background`); owned-tree kill planned in §10 | S to M | 1a |
| 6 | Short check output: a passing run went from about 34k tokens to about 50 (R154, R155) | Faber reads each check's output and hands roles only the failures with `file:line`, the tool's suggested fix, measurements and one summary line; the whole output stays in the mission log; an unknown format falls back to its last lines. Parsers first for what Faber itself runs (cargo, tsc, oxlint), then the common ones (pytest, jest/vitest, eslint, go test) | R146 ("Short check output"); MVP spec §7 (result packet) | `cargo t` does this for Faber's lanes (`testkit/`); missions do not | M | 1a for Faber's own checks; S more per format later |
| 7 | CI shards balanced by measured time (parallel rounds; `cargo t --shard`) | Faber keeps each check's measured duration and deals a project's proof set across parallel runners so they finish together; for a project with hosted CI, a worker proposes the sharded matrix once the suite is slow enough to matter | Faber's own test system (item 6); Phase 4 | `testkit` deals screen parts by CI time | M | Phase 4 |
| 8 | Reviews that end (R147); a weekly scout for new practice (R148) | The reviewer envelope carries a class per finding (person, data, security, written rule, or note); only the first four block, a fix needs a failing test or a repro, and each round receives the findings already decided. Today any finding blocks (`accepted = pass && findings.is_empty()` in `coordinator.rs`). The scout moves into the catalog scout | R146 ("The reviewer"); Curated catalog | Reviewer prompt and verdict; scout runs on the owner's PC | S (R147); M (scout) | 1a for R147; Phase 3 for the scout |
| 9 | Pace quota across vendors and spend what would expire near a reset; pacing never stalls the critical path (R119 notes, quota pacing) | Mission admission reads the same pace as Auto: the critical-path task goes to the vendor that does it best whatever the pace, and deferrable work (reviews, canaries, research, re-runs) goes to the vendor furthest behind or with quota about to reset | What makes it better ("Quota as a perishable resource"); MVP spec §8 | Auto quota pacing built; mission grants planned in §8 | S to M | 1a (reviews only), rest Phase 3 |

Lessons that later rounds teach take the same road on purpose: a mission's retrospective proposes the rule or card with
its evidence, and it becomes engine behaviour only after the owner's yes (R146, "Lessons need the owner's yes"). This
document is that retrospective, written by hand once.

## R146 coherence: what 1a needs first

| R146 item | Critical for 1a? | Smallest version | Size |
| :--- | :--- | :--- | :-- |
| **Faber decides "done"** | Yes. Without it a writer's "tests passed" lands work, the failure that guardrail 4 exists for | The queue already proves the composed tree (MVP spec §9). The smallest step is to confirm and test that no path lands a candidate without an engine receipt, and that Faber's map check runs inside it (it does: `engine/tests/product_map.rs` is part of `cargo t`) | S (a test, maybe no code) |
| **A contract before writing** | Yes. It is how a mission avoids a second way to do the same thing, the most common coherence break | Three fields added to the existing contract: the map areas it touches, the closest existing thing, and `Extends:` or `New:` with why. The writer cannot start without them and the reviewer gets them. For Faber the areas are the same ones `.faber-lane` claims, so the lease and the lane claim share one source | S |
| **Sized by risk** | Yes. The full chain on every change is what made T13 take two days (R119) | Two tiers set by the engine, not the model: one map area and no security path means writer, checks and one review; more than one area, or a path on a security list (spawn and shell, `winproc/`, permissions, credentials, updates) means the full chain with two reviews from different sessions | S |
| **A project map built from the code** | No, for Faber: its map is hand-kept in `docs/product-map.md` and a test already keeps it honest | For 1a, read Faber's existing map. Extractors for other projects (settings, routes, commands, texts, exports) come in Phase 3 with the lead agent; the first one is the cheapest that is useful, a list of routes and exported names tied to their files | L (Phase 3) |

Critical path for 1a, in order: sized by risk (it decides how much of the rest each change pays), then the contract
fields, then the "done" receipt test. Each is one small commit; together with rows 1 to 6 and 8 of the first table they
add up to about five agent-days. They fit the 2026-10-20 to 22 window only as two or three lanes with disjoint files
(the coordinator, the testkit output parsers, the process and job code), each one strong agent, started now.

## Out of scope

Building any of this; changing the MVP spec's acceptance gates; anything a person sees on screen.
