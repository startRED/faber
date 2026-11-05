# Plan boundary admission qualification

Task 500 gates the permission requests the engine actually receives. It does not require universal pre-execution
coverage of tools that the vendor runs without asking. The recordings below describe that coverage limit; they are
not an authenticated probe of installed binaries or proof of an escape.

## What the recordings establish

| Recording | Observation | Qualification |
| :--- | :--- | :--- |
| `tests/fixtures/codex/tools.jsonl`, lines 43-55 | Two command approval requests have request ids. Each matching answer precedes that command's completion; one allows and one declines. The recorded thread uses `workspace-write` and `untrusted`. | Positive evidence for these two approvals only. |
| `tests/fixtures/claude/tools.jsonl` | Two `can_use_tool` requests likewise receive matching answers before their Write results. | Positive evidence for these two approvals only. |
| `tests/fixtures/codex/helper-run.jsonl`, lines 42-57 | Native command notifications carry opaque `processId` handles and successful completions without a command approval request. Thread start sends only `cwd`, so this is not evidence of a particular permission policy. These commands read local files. | An observation is not a request the engine can hold. Reads do not demonstrate a boundary escape. |
| `tests/fixtures/claude/queued-midturn.jsonl` | The init reports `auto`. Bash and its successful result arrive without `can_use_tool`. The command is a local wait. | No universal callback established, even with a guarded mode. This is not a dangerous execution probe. |

`engine/tests/plan_boundary_qualification.rs` contains the three `size_boundary_qualification_` checks. They assert request/answer/result
ordering for both positive recordings, successful ungated observations for both negative recordings, and the production
parsers' handling of synthetic boundary substitutions. The last check substitutes data-folder and credential reads,
an outside-project read, publishing, deletion outside a copy, unsupported shell syntax and a plain local read control
into the recorded notification shapes. Every substitution remains an observed Tool event, never a Permission request.
**These substituted commands are never executed and are not new vendor recordings.**

The same check preserves the current full-access launch contract: Codex sends `approvalPolicy: never`, no dynamic host
command tool, and `danger-full-access`; Claude uses `bypassPermissions`. An implementation attached only to approvals
would miss calls in those modes. Isolated's host command tool is not an all-mode solution. Interrupting after a Tool
event cannot establish that a write, delete or publish waited before taking effect.

## Shipped admission layer

Boundary fixtures resolve temporary paths before classification so Windows short-name aliases do not change the
expected demand. Ordinary project controls use folders outside the Store; text-ordering and repeated-request-id
tests substitute synthetic single-file additions for the recording's opaque shell approvals. The running-project-row
screen test also replaces the recorder's absolute Write paths with local file names.

The link probe rejects network and non-disk device names before filesystem I/O, including a supplied working folder.
It accepts at most 64 candidates and 64 components per path and makes at most 64 prefix probes in total; exceeding a
limit requires a risky contract. Existing symlinks and junctions require the same contract.
Shell and file-tool deletions protect both the copy root and its `.git`. Filesystem roots remain absolute, and `--`
ends shell option parsing in both classification and probing. Directory searches and directory deletes always require
a risky contract for now: unclassifiable, reads many files (the existing plan.boundary.unknown reason). The engine
checks only literal path prefixes and whether a search/delete target is one file; it never enumerates contents.
Grep/LS on a directory or without a single file target, Glob, rg directory searches, grep -r/-R, find, ls -R/--recursive,
and PowerShell Get-ChildItem -Recurse cannot pass a normal contract. A literal one-file search can remain small.
Directory deletes, including file-change delete paths, cannot pass even inside a separate copy.
Each admission and answer rechecks literal paths. Credential components are checked below the project root;
explicit relocated credential roots still apply. Task 952 records ignore rules versus grep traversal, caps, skipped
credentials, nested .git/.env deletes, hard links, Glob below **, lock cost and downstream secret() worktree ancestors.

Project plans carry small, normal or risky size, default small. Size never decreases when the plan is rewritten,
advanced or cleared. A contract records the size it covered when supplied and needs an extension/new reason and proof.
The pure classifier in engine/src/permissions.rs classifies literal paths, supported read/delete argv, publishing
and shared-history commands. Local reads, writes and ordinary file deletes inside a separate copy keep the current size;
deleting the copy root or `.git` demands risky.
Codex file-change arrays use the same path rules, including delete kinds. Unknown shapes, tools, scripts, wrappers,
flags, escapes and compound/expanded shell syntax demand risky. Windows shell wrappers (including powershell.exe
-Command strings with backslashes) are deliberately unknown, even when the wrapped command looks harmless; parsing
another shell language is outside the literal argv subset. Direct command strings and native file paths still classify.
It performs no I/O, reads no credential contents and never executes a command. The engine probes existing path components before admission and passes an existing-link fact to the pure classifier.
Symlinks and Windows reparse points demand risky, including missing children and parent traversal. Hard-link alias
inspection is deferred; no special hard-link classification ships. The answer path
rechecks admission. This is a point-in-time permission check, not an execution sandbox or protection against a link
changed after admission.
Git reads stay unknown/risky because this layer does not force executable configuration off. Writes under .git also
demand risky. Ambiguous Windows aliases and PowerShell provider paths fail closed. Glob is always unclassifiable
until an authoritative resolver can prove its reach.
An active separate copy is project work even when stored under Faber's data folder; other data paths demand risky.

Admission runs at Core's EngineEvent::Permission handling, before remembered grants or full-access answers. It sees
Claude can_use_tool requests and Codex requestApproval requests forwarded by the adapters, including child approvals
owned by that conversation. Existing adapter denials (credential guards, data-write exclusions, isolated policy)
remain denials and can finish before Core receives a request. Native tools that do not request approval, Tool
notifications, helper execution and isolated run_command execution are not intercepted by this layer. Permission
modes and their coverage remain unchanged. A differing Codex request cwd is retained for classification.

A boundary raises the plan before approval. A contract below that demand, absent or missing its extension/proof holds
the original request on a question card, whose default is to state the contract. Supplying the contract with plan while
the request waits re-enters the ordinary permission decision: Ask still asks, remembered grants still apply, credential
refusals still refuse silently before any plan hold. Requests without a current turn are denied, including when a
contract already covers them. The question cannot approve execution. Choosing its default declines this attempt and tells the
agent to state the contract and retry, so a CLI waiting on its own serial approval can progress. Claude receives the
instruction in its denial; Codex receives a durable next-turn wake through the existing queue. A retry still passes admission and ordinary
permissions. Stop and stale/session checks remain in the existing answer path.

## Proof and cost

engine/tests/plan.rs covers each boundary, unsupported syntax, plain reads, default/legacy plans, monotonic size,
contract coverage, the actual held permission wire, restored ordinary cards, the default denial and Stop/stale answers.
Unit checks inject a panic-on-I/O probe to prove network names and excessive requests are rejected without disk access;
integration checks cover risky directory searches, held normal contracts and small single-file reads/searches.
Its size_boundary_classifier_is_pure_conservative_and_measured check prints measured ns/call over 100,000 mixed
approvals. A unit check in engine/src/engine/plan.rs also measures 1,000 engine classifications including existing-parent
metadata probes. The commit body records these timings. No process, background service or dependency is added; bounded plan metadata and the existing question card
are retained. Contract questions reuse the existing Keep going answer timer. Independent security reviews remain required before landing.
