# Records, files and context

Status: Proposed formats and ownership, not installed files or adopted production schemas.

## File layout for a project

Create only the files a project needs, progressively. A question in Chat creates none of this tree.
Existing project documents stay where they are and are referenced, not rewritten into a framework template.

```text
AGENTS.md                         existing short entry point and rules
docs/                             existing product, design and development knowledge
.faber/
  README.md                       generated: now, next, decisions needed, links; safe to regenerate
  decisions.md                    existing durable choices with their original authors
  taste.md                        optional approved preferences; no automatic rule promotion
  plan/
    roadmap.json                  proposed milestone IDs, order and item membership
    items/
      <id>.md                     canonical desired outcome and acceptance criteria
    requests/
      <id>.md                     exact request text; immutable after capture
  missions/<id>/                  EXISTING execution journal; extend, never replace
    mission.json                  public mission metadata and mirrored state
    plan.json                     accepted task graph, pinned item revisions
    contracts/<id>.json            boundary and compatibility requirements
    briefs/<task>.json             complete bounded worker brief
    decisions/<id>.json            runtime decision request and answer
    artifacts/<id>.json            hand-back, output paths, hashes and receipts
    receipts/<id>.json             effect intent and reconciled result
  recipes/<id>.md                  optional reusable steps linked from a delivery
```

Code, images, videos and documents stay in the project's normal output locations. Artifact records point to them;
`.faber/` does not become a second copy of the delivered product. Raw logs, process IDs, live leases, grants,
CLI session identifiers, prompts containing private data and full conversations stay in Faber's private data folder.
Existing source requests are referenced in place when possible; export never silently copies private conversations.
Creating a portable request requires recording its exact selected text under the project's existing permissions.

The tree is a proposed contract, not a mandate to migrate Faber's own existing docs in one change.

## Exactly one owner per fact

| Fact | Canonical source | Other representations |
| :--- | :--- | :--- |
| What was asked | Immutable request text or pinned permitted source | Brief includes its exact required passages |
| Desired outcome, criteria and dependencies | One item definition | Accepted plan pins a revision; no second editable copy |
| Milestone membership and presentation order | Roadmap definition | UI and Markdown render it |
| Accepted attempt graph and contracts | Existing mission `plan.json` and contracts | Runtime references their revisions/digests |
| Current attempt state and permission to act | Existing private run, lease and grant records | Public journal is a recoverable mirror |
| What passed, failed or was not run | Engine-verified receipts for the candidate | Screen summarizes with links |
| Review and acceptance | Independent review plus engine acceptance record | Worker completion remains an untrusted claim |
| Human or AI choice | Existing decision record with original author | A cross-reference may appear in the readable ledger |
| Current summary, counts and next actions | Projection of the sources above | `.faber/README.md` and UI are disposable views |

An item definition does not contain an editable execution status, percentage or spend counter.
Roadmap order is not execution order: dependencies and resource admission govern execution.
The root roadmap does not copy task bodies or mission states.

## Minimal item definition

Proposed Markdown with a small machine-readable header, parsed strictly. This is an illustrative contract;
choose a parser only after inspecting existing repository parsing utilities during implementation.

```text
---
schema: 1
id: work-verifiable-progress
request: request-framework
depends_on: [work-evidence-projection]
kind: software
risk: normal
---
# Show verifiable project progress
## Outcome
A person can distinguish finished work from work still being checked.
## Acceptance
- AC-1: Every completed outcome links to accepted evidence for its current revision.
- AC-2: A change after review makes that acceptance stale until rechecked.
## Scope and non-goals
Extend project progress; no new model selector, board or settings section.
## Sources
References to existing product-map entries, decisions and relevant code.
```

Acceptance IDs are local to the stable item ID. Each criterion becomes a typed runtime requirement referring to its
exact source bytes and digest. The engine assigns revision digests; humans need not maintain numeric revisions.
Output paths, permitted effects, proof IDs, budget and stop rules belong in the compiled brief and contracts.
The human definition describes intent; the accepted run snapshot freezes what a particular attempt may do.

## Context compilation

Build a worker brief deterministically from the accepted definition, relevant rules, code map and evidence.
Use existing kit needs, selection receipts and worker-envelope limits.

1. Include the original requirement text, relevant owner decisions, acceptance criteria and non-goals verbatim.
2. Include scope, base candidate, dependency artifacts, contract digests, allowed effects, proof IDs and stop rules.
3. Add bounded source excerpts and references, then optional short notes. Never truncate required text silently.

If required context does not fit, split the task or surface the conflict; never replace the person's requirement
with a guessed summary. A digest detects drift, not trustworthiness. Untrusted web pages and tool output remain data.

Record why each source was selected and the bytes/tokens it contributed. Reuse unchanged packets by content digest;
invalidate them when relevant instructions, definitions, contracts, dependencies or candidate inputs change.
Start with path and symbol references plus the existing search tools; no new vector database or always-on index.

Resume from the accepted brief, last durable attempt state, remaining criteria and latest evidence. Read changed
inputs before acting. A fresh session must not reread every conversation, historical plan or research folder.
Keep context targets measured and model-aware; no universal hard-coded token budget that cuts required material.

## Concurrent edits and interruption

Keep existing atomic file replacement, schema validation, unknown-field preservation and refusal of newer schemas.
Add expected-revision checks to planning edits. Two sessions editing one item cannot silently overwrite each other.
The engine is the only writer of runtime transitions; planning files cannot grant permissions or authorize effects.

The journal uses several files, not a cross-file transaction. Extend the existing intent/receipt and recovery protocol:
write the transition intent, persist effects, then publish a new projection generation. A failed write leaves the old
generation visible or explicitly marks the view incomplete. Startup reconciles private authoritative state before
emitting a current view. Evidence from a partial transition cannot imply acceptance.

Each projected view carries a schema, generation, source revisions, capture time and freshness state. A generated
README carries the same generation and is replaced atomically. Offline, it says when it was captured and that live
activity is unknown. A stale view never keeps saying an agent is currently working as a verified live fact.

An external edit to a definition creates a new desired revision. Current execution remains pinned until a validated
amendment or a later attempt accepts it. Cycles, missing IDs, path escapes, duplicated ownership and conflicting
writers are refused using existing plan and path validation. Case aliases, symlinks and worktree identity matter.

## Privacy, retention and portability

Do not read login stores. Redact known secret-looking values from output before exposing receipts, using existing
filters; filtering is not permission to ingest credentials. Restrict request capture to authorized project content.
Export a previewed selection of intent, choices, outputs and evidence links, excluding private runtime authority.

Large logs and assets have bounded retention; preserve the small acceptance record and mark pruned evidence as
unavailable. Do not present a pruned artifact as reproducibly verified. Recipes pin tool versions and external
inputs where available, recording unavailable or licensed inputs without copying them.

## Faber's own repository: migration without competing backlogs

Initially `docs/tasks/<order>-<name>.md` remains the sole editable source of development tasks. An adapter assigns a
stable framework ID and indexes its existing path; it does not create another editable item file. Existing task
claims and `.faber-lane` remain active. Normal conversations without mission records show their available evidence
and honest partial status; they are not silently converted into mission-controlled executions.

Later, a separately accepted migration may move one task at a time into the proposed item format, preserving its
ID and leaving a redirect. Flip its source ownership before allowing writes; refuse two active definitions.
Use one task-selection backend at a time. Keep old readers during the compatibility window and prove rollback.
Do not retire the existing task queue until claim, resume and current landing workflows pass through the adapter.
