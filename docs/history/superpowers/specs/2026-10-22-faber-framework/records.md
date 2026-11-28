# Records, files and focused context

Status: Proposed extensions, constrained by the approved R435 work method.

## Intuitive files without another source of truth

Keep the approved structure. Create only what the current work needs; a short Chat answer creates none of it.
The engine and approved adapters maintain these files. People can inspect them in a normal editor.

```text
AGENTS.md                  existing project instructions
README.md                  what the project is and how to use it
docs/                      existing product/design/development knowledge, adopted in place
.faber/
  rules.md                 generated project orientation; approved 3 KB startup share
  decisions.md             lasting choices and original authors; approved 8 KB soft target
  taste.md                 person-approved preferences; approved 1 KB target
  lessons.md               person-approved lessons that change later work; approved 2 KB target
  next.md                  follow-ups, at most 12 lines of 200 characters
<normal output folders>/   the actual app, images, documents and other deliveries
```

The five files describe the approved target, not proof that every writer has shipped. Runtime support files such
as verifier logs/local configuration are internal facilities, not new human-maintained planning documents.
Legacy plans/recipes/learning/memory stay readable; this framework never creates new ones under those names.
Use an existing decision/taste/task source through its approved adapter instead of adding a second file.

Plans stay in conversations. Standing goals stay in `projects.json`. The map is extracted on demand by `map`.
The person's words remain in their original messages/files, with permitted references and exact source digests.
Do not export private conversation text to a repository just to make a worker brief convenient.

## One owner per fact

| Fact | Canonical owner | Derived view |
| :--- | :--- | :--- |
| Original request | Original conversation message or authorized source file | Exact required passages in a worker brief |
| Accepted task intent and steps | Existing saved conversation plan | Native plan and roadmap detail |
| Long-running objective | Existing project goal record | Goal progress and stop condition |
| Follow-ups | Approved next.md or the adopted task list | Done card and candidate next work |
| Milestone order and membership | Proposed versioned metadata under existing project extra | Outcome route, no copied task bodies |
| Current execution | Existing engine/conversation/helper state | Running column and current plan details |
| Check results | Verifier journal and message check result | Evidence attached to the appropriate outcome |
| Human approval and authority | Private provenance and permission records | Project text is descriptive, never authoritative |
| Output | Actual delivered file/version | Current artifact cards and receipts |

An agent setting a plan step to done establishes reported plan progress only. Verified completion additionally
needs applicable evidence and review. The native checklist and verified outcome count must not imply they are equal.
The root roadmap metadata never becomes another task list or a store of mutable execution status.

## Minimal proposed record extension

Extend existing structs/commands rather than publish a second tool protocol. Keep old conversations readable.
Implementation must pin the exact schema after inspecting current model/store patterns.

```text
Conversation.plan:
  existing title/steps/cost fields
  optional schema, revision digest
  steps gain stable id, outcome reference, dependency ids, acceptance source refs
Project.extra.roadmap:
  schema and expected revision
  milestones: stable id, title, ordered references to existing outcomes
Outcome evidence link:
  project/conversation/step ids, plan revision, candidate/content key
  check-result references, independent-review reference, delivery reference
```

No copies of task prose, usage counters, review verdicts or live status in roadmap metadata. The existing plan owns
its step definitions; the verifier owns checks; the engine computes their relationship. A hierarchy references
subplans in other conversations rather than removing the current 12-step bound or loading one enormous plan.

Once exposed to workers, new stable IDs do not change when a label changes or an item is reordered. A material
requirement change creates a new revision and invalidates affected evidence. Preserve exact requirement bytes
through refs plus checked excerpts. Separate ideas from authorized scope; an inferred task is not an owner decision.

## Focused worker context

Keep the approved 6 KB startup card: 3 KB rules, 2 KB kept decisions, 1 KB kept taste. It remains fixed for the
session; changes apply at the next session. Keep approved trust/provenance verification and overflow counts.
Descriptions and unreviewed repository text remain tool-result data, never new system instructions.

On demand, provide a task brief with original required wording, outcome, non-goals, source refs, allowed scope,
applicable checks, dependency outputs, current candidate, budget and stop rules. Use current kits, map and memory
search. Do not reread all history or load lessons/follow-ups on every turn. Do not let a brief grant permissions.

If required task context does not fit, split the task or expose the conflict; never silently truncate a requirement.
Reuse sources by content key, invalidating on relevant changes. Record source-selection and briefing overhead so
shorter prompts are not mistaken for cheaper accepted work. No new vector database or always-on index is required.

## Writes, recovery and stale evidence

Use current atomic storage and unknown-field compatibility. Add expected-revision checks for concurrent roadmap
and plan edits; two conversations cannot overwrite each other's accepted scope silently. Multi-file updates need
an explicit recoverable transition or reconciliation before publishing a new consistent projection generation.
Do not claim atomicity across the project record and conversation files merely because each replacement is atomic.

Every projected view carries source revisions, generation, capture time and freshness. Reconstruct runtime facts
from existing engine records. The current verifier content keys and journal are reusable, but step-level acceptance,
plan history and cross-plan invalidation are proposed extensions, not already-established guarantees.

A changed candidate or requirement makes affected evidence stale. A restarted session must re-read changed inputs
and reconcile unfinished effects before retrying. Never infer permission or a live writer's death from a timestamp.
Do not claim exactly-once arbitrary external actions: when the effect is ambiguous, stop that action for reconciliation.

## Human-readable export

Offer a generated roadmap document through the current export/artifact flow, in a location the person chooses or
an existing adopted report location. It is a snapshot with capture time and source revision, not a prescribed new
`.faber/README.md`, plan file or writable status source. No new mandatory file just to inspect progress.

Include ready/now/next, pending decisions, delivery links and evidence. Exclude credentials, private runtime IDs,
full conversations and unselected source text. Keep unavailable/pruned evidence explicit. Technical details remain
behind disclosure; a person should not need JSON or an editor to understand the project.

## Faber's own files

Adopt its R-numbered decisions, taste cards and `docs/tasks/` exactly through the R435 adapters. Claims, lanes,
number allocation and the landing queue remain Faber-repository conventions; do not impose them on user projects.
Do not migrate the product map as an incidental framework step: R435 reserves that later change for owner approval.

## Revision 2: adaptive work records

Add optional versioned fields to existing plan, artifact, check and Auto records, preserving older readers:

| Record extension | Needed fields | Owner |
| :--- | :--- | :--- |
| Derived task packet | Source/requirement refs, selected examples, current inputs, allowed scope, output shape, checker refs, stop conditions | Deterministic compilation from canonical sources; not another editable plan |
| Selected work path | Eligible capability snapshot, path/reason, selected model/effort, budget reservation and fallback condition | Existing Auto/admission services |
| Outcome evidence | Requirement/input/output/checker revisions, machine result, independent judgment and preference as distinct fields | Existing verifier/closing records |
| Review coverage | Candidate and source digests, rubric dimensions, independence/session provenance, decided findings | Independent reviewer plus engine receipt |
| Route observations | Task family, model/tool version, path, full cost completeness, validity and preference signals separated | Existing private Auto learning state |
| Recovery checkpoint | Last durable output, unresolved criteria, attributed effect intent/result, next permitted action and non-progress fingerprint | Existing execution/conversation state, extended explicitly |

These are proposed relationships, not five new stores or tools. Capability records never override permissions.
Generated evidence refs cannot make an imported artifact authoritative without identity/integrity checks and
applicable independent validation. A previous review name alone is insufficient to reuse its verdict.

Bound caches, observation windows and raw logs; keep small acceptance summaries and disclose unavailable pruned
evidence. Never auto-delete user-authored requests, accepted decisions or current artifacts as a retention shortcut.
Preserve R435's approved provenance and startup-card shares. No catalog dump, copied conversation history,
unreviewed lesson injection or new framework-written plan/recipe directory.
