# Projects: self-development missions

The published contract of the mission engine: screens, records, worker envelope, kits, proof isolation and commands.

## Projects: self-development missions

The minimal Projects slice of R96 ([spec](../superpowers/specs/2026-10-12-self-development-mvp-design.md)). The engine types
live in `engine/src/project.rs`, `project/store.rs` and `project/wire.rs`; this section is their published contract, and
screen work may start against it with mock data before the engine runs missions.

### Screens

A mission has no screen: a project is worked in by talking to its agent (R160), and the start card, the checks sheet, the
mission board and the local build card left the bundle with their 30 s poll (R171). The engine still keeps the mission
machinery, which no command reaches (task 060). The Projects area shows one of two things:

- With no project open, the small "Open a project" screen (`ui/Projects.tsx`): the empty state with its one button, the
  recent folders and saved environments, and at most three project conversations to pick up.
- With a project open, the new-conversation screen of Chat (`welcome-grid project`) scoped to the folder, in one column:
  Curioso, the project and its folder in short form as one quiet line in the interface size (the last two folder names,
  the full path on hover; the sidebar and the crumb already name it), the greeting "What should we tackle?", the shared
  composer, and under it at most three of this project's own conversations to pick up, each saying only when. No
  suggestions (the composer's hints show what to ask) and no second column. No beam of light: a project screen is a place
  to work; in Dark Room the black hole keeps to the room on the right, so no words sit on its bright part. The sidebar
  lists the projects, and every setting of a project (the
  separate-copies switch and the checks) is in Settings, Projects (C011), one closed entry per project with one row per option with one short line and
  the rest behind the "?" of the row.

### Separate copies

A project conversation works in a separate copy of the repository (R201, `engine/src/project/copy.rs`): a git
worktree in `<data>/copies/<conversation id>` on the branch `faber/<a few words>-<4 characters of the id>`, made with the
mission broker's git runner (fixed argv, no shell, hooks and filters off) from the head the person has checked out, before
the first message reaches the CLI. The agent starts in the same place of the copy as the project folder is in the
repository. The copy is made outside the engine's lock and in a blocking task of the shell, so a slow checkout holds
neither the screen nor the other conversations. Nothing polls: a copy is removed when its conversation is deleted, and
once at start (`Engine::sweep_copies`) when it is clean and every file its branch changed is the same in the person's
branch or its upstream, which a merge, a fast-forward and a squash all satisfy. Uncommitted changes, or commits no other
branch holds, refuse the delete with `error.copy.unsaved` until the person discards them: the row's × then asks once more,
"Discard changes?", with the reason as its label and tooltip, and only that click discards. The topbar crumb adds
"› faber/…", the branch, in the extra-small data font and the crumb's quiet ink, cut before the title is; the copy's
folder is its tooltip.

### Records

A mission keeps plain records in the repository's `.faber/missions/<id>/` folder and private state in Faber's data
folder. Every record has a `schema` version. A reader keeps the fields it does not know and writes them back unchanged,
and refuses a record whose schema is newer than its own (`error.journal.newer`). Ids are 1 to 64 letters, digits, `-` or
`_`.

| File | Holds |
| :--- | :--- |
| `mission.json` | goal, project, repository (its common git directory), base revision, `faber/<id>` branch, grant revision, run mode, state |
| `plan.json` | the task graph and its revision |
| `contracts/<id>.json` | owner, version, digest, base, boundary, examples, proof ids, dependencies |
| `briefs/<task>.json` | what a fresh worker session starts from: goal, non-goals, scope, base, contract, sources, proofs, grant, stop |
| `decisions/<id>.json` | question, kind, who asked, default, the answer and who gave it |
| `artifacts/<id>.json` | a hand-back: changed paths, revisions, command receipts, checks not run, cost completeness, summary |
| `receipts/<intent>.json` | an intent written before an effect, and its receipt after; an intent with no receipt is reconciled, never replayed |
| `missions/<id>/runtime.json` (data folder) | CLI sessions per task and the leases with their processes; never in the repository |

In `projects.json` a project gains `repo` (its common git directory) and `grant` (mode, revision, author, remote,
self-update trust, revocation). A project saved before missions has neither and allows no effect. Seguro allows
worktrees, commits with a known author and fast-forwards of the mission's branch. Equilibrado adds the push of that
branch and one draft pull request to a named remote. Self-update needs its own trust. Each check names the grant
revision the work was admitted under, so a new revision or a revocation stops the next effect (`reason.grant.*`).

Writes replace the file atomically and flush it; a failure leaves the old file and returns `error.store.full` or
`error.store.write`. A damaged mission record, runtime sidecar, project list (read for Projects) or quota ledger is
`error.store.corrupt`, never empty data. A damaged `budget.json` is copied to `budget.json.quarantine`, and Projects stays
closed while that copy exists, even after Chat has saved a fresh ledger over the damaged one.

The engine refuses a plan with a cycle, a missing or repeated task, a deliverable claimed twice, a write path outside
the repository, writes on a read-only task, or two writers that nothing orders sharing an area, parent folders and case
aliases included (`error.plan.*`). A contract's digest is SHA-256 over its content with sorted keys. A revision sends the
tasks built on the old digest, and everything that depends on them, back to Blocked with `reason.contract.revised`;
nothing else moves.

### Worker envelope

Every worker turn ends with one JSON object, version 2, with no unknown fields. Durable journal records remain version 1;
old records stay readable and retain their extension data. Wire version 1 is not accepted for a new dispatch.
The engine refuses an envelope over
512 KB, a stale task, attempt, base, contract digest or lease generation (`error.wire.stale`), an action outside the role
(`error.wire.role`), and a path that leaves the worktree (`error.wire.path`). A file write carries the SHA-256 of the
content it replaces, or none for a new file.

```json
{"schema": 2, "task": "fix", "attempt": 1, "base": "3d3d502", "contract": "<sha-256>", "lease": 4,
 "action": {"patch": {"ops": [
   {"write": {"path": "engine/src/x.rs", "expect": "<sha-256 of the file as read>", "content": "..."}},
   {"rename": {"from": "a.rs", "to": "b.rs", "expect": "<sha-256>"}}
 ]}}}
```

| Action | Who | Carries |
| :--- | :--- | :--- |
| `read` | every role | up to 64 paths, and symbols whose callers to find |
| `plan` | lead | summary, acceptance, tasks (id, title, kind, requires, decisions, contracts, deliverables, writes, proofs) |
| `amend` | lead | tasks to add or replace, ids to remove, reason |
| `patch` | writer; test proposal, kept as an artifact | up to 64 write, rename and delete operations, each with its expected hash |
| `proof` | lead, writer | named proof ids, never command text |
| `decision` | lead | question, kind (technical, direction, taste, money or legal), options, default |
| `complete` | every role | summary, changed paths, assumptions, risks, checks not run, a reviewer's verdict and findings, next |

The AI answers only technical decisions; direction, taste, money and legal ones wait for the person
(`error.decision.person`), in autonomous mode too.

Wire v2 adds optional fields without changing role authority. `ping` is `Order`, `Found`, `Blocked`, `NeedEyes`,
`DoneReady` or `Disagree`; `note` is at most 60 whitespace-delimited words. Envelope `requires`, `checks` and `quotes`
are verbatim-text ids; task `requires` continues to mean task dependencies. Briefs retain each id's exact UTF-8 text.
Task drafts add `labels` (verb, at most eight normalized needs, risk, freshness and facets), scoped `paths`, check ids
and a `kit` receipt/digest pin. Plans add the deliverable `sheet`, grounded `bar` and `compileReceipt` reference.
Completions add a check `receipt` id, bounded check outcomes, artifact paths/SHA-256 hashes, claims tied to checks,
assumptions with provenance, `unconfirmed`, `kitUsed` and newly requested needs. Unknown nested fields remain errors.

`wire::CheckReceipt` is distinct from the journal's effect `Receipt`; it binds checks and evidence to one candidate,
model, effort and known or unknown cost, with links to effect receipts. Check results are pass, fail or not-run:
pass requires evidence; failure and not-run require a reason. `CompileReceipt` records phase timestamps and cost tags.
These types publish the shared T5K/T5B/T5 contract. A worker's claimed receipt is data; only engine-verified proofs and
the independent exact-tree review can authorize acceptance. Missing optional fields do not imply a passing bar or kit.

### Automatic project kits

`project/kit.rs` and its submodules implement the pure T5K contracts. Cards are written from reported inventory,
scoped project sections and named engine proofs; kits are assembled from needs, never saved as hand-written loadouts.
`Inventory::new` validates, hashes and indexes an immutable snapshot once per inventory change. Assembly uses that
snapshot, origin priors, job ownership, conflicts, role budgets and safety filters. Versioned Beta outcomes only break
ties. A blind, shuffled shortlist supports shadow comparisons; the code selection remains authoritative.

Persist the selection with `Journal::record_kit` before dispatch. Compose approved, hash-checked inline bodies into
the existing safe role prompt with `compose_role_prompt`; the role prompt remains its byte-for-byte prefix. This
adapter changes no driver flags or policy. Record init with `attest_kit` before work, then add the outcome with
`complete_kit`. Completion cannot replace the selection or its attestation. Receipts live in
`.faber/missions/<id>/kits/<task>-<attempt>.json` and preserve additive root and nested metadata. Card caches and cited
field briefs do the same; a newer durable schema is refused as newer. Dispatch and card-writer replies retain their
strict schemas. A failed write prevents dispatch.
The runner must enforce T2 admission and handle mismatched/unreported init; these receipts never grant containment.

G0 is empty. G1 returns at most three cited suggestions with downsides, with installation parked. G2 stores bounded,
cited primary-source briefs in `.faber/briefs/`; G3 retains verified role controls. Only a missing required capability
parks its dependent node. The pure start-card adapter supplies two locale keys; T9 owns their screen and translations.
The Auto labels adapter shares the existing classification turn and preserves its model-choice table. T5 integrates
classification, fresh pass sessions, dispatch, proofs and two-order comparisons; T5B resolves native bar proof IDs.

See [the T5K proof report](../research/2026-10-12-project-kits.md) for measured synthetic performance and the live G9
evidence still required by T13. Fixture attestation is not proof of native isolation.

### Proof isolation (R138)

A mission proof, and the canary's build and test of the code its Writer changed, run code a role wrote before any review.
The engine starts them through `proc::isolated_spawner`: the recorded environment only, created suspended, joined to a
kill-on-close job, and on Windows lowered to Low integrity before the first instruction. The mission clone, the proof's
build folder, its own cargo home and npm cache, and its temporary folder are labelled Low (inheritable), so the proof can
write there and nowhere else: Windows refuses its writes to the installed app, the engine's records, the owner checkout
and the user profile (except folders that are Low by design, such as `AppData\LocalLow`). Where the user may only Modify
a folder (a second drive), the folder's owner first grants itself the right to label it. On macOS `sandbox-exec` denies
writes outside the same folders.

Residual in v0, disclosed on screen (`projects.proofs.isolationTech`): a proof can still read what the user can read and
use the network, and on Linux nothing limits its writes. R139 accepts this for v0: chat roles already run with full access
(R121), which exposes strictly more, and a proof starts from a cleared environment at Low integrity. The upgrade path is an
AppContainer (Windows), which closes reads and the network as well.

The real canary is opt-in (`real-canary.json`). It runs the candidate build, untrusted until it passes, with the real
Claude Code and Codex CLIs, signed in through the user's own profile and environment, with copied qualification receipts;
only its proof runs isolated.

Two more launch checks: every qualification receipt binds the launch-policy revision (a SHA-256 of every role's launch
parameters), so a changed flag or setting asks for a new qualification run; and a role CLI is created suspended, the file
its image came from is pinned (opened with a share mode that denies write and delete) and hashed through that handle before
`--version` runs and again before resume; the handle stays open until the process has started, so the file cannot be
swapped in between, and a hash other than the receipt's kills it before it runs. Waiver markers and their ledger records
live next to the canary data folder, not inside it, so deleting that folder does not revive a spent waiver, and a waiver
is recorded as spent before the mission that uses it runs.

Follow-up: a crash after a proof ran but before its progress mark was written replays the proof on resume. Proofs are
idempotent tests, so v0 accepts the replay.
