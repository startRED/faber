# Faber Method 2: one way of working, for Faber and for every project

Status: proposal waiting for the owner's yes. It replaces the approved own work method (R435) and the Faber Flow
proposal (`2026-10-22-faber-framework/`, never adopted) with one design. Everything R435 approved that is built or queued
is kept; what changes is how work is split, run in parallel, shown and measured.

Asked by the owner on 2026-10-30: rework the whole framework so that a request ends in a better result, faster and
cheaper than in a plain Claude Code, Codex, Cursor, OpenCode or Antigravity session, mostly by doing more in parallel;
progress a person reads on GitHub and in the app; AIs that know what to do next without spending time on Markdown; size
limits on everything an AI reads; better search. Sources: R435, Faber Flow and its BMAD and GSD audits, the study of
GSD, BMAD, Backlog.md, Spec Kit, Task Master, CCPM and Kiro, and `docs/research/2026-10-30-context-and-progress-tools.md`.

## Why a plain session loses

A plain CLI session does one thing at a time, in one context that grows until it is compacted, grades its own work,
and leaves progress in a chat nobody rereads. GSD and BMAD fix the order of work with documents, and pay for it: a
state file written by hand drifts and conflicts (GSD needs a lock, BMAD a never-regress rule), and front-loaded paper
is where BMAD's cost and slowness reports come from. Faber can do what neither can, because it sits beside the CLIs:
run several sessions at once, on two vendors, give each one only what it needs, and check the result outside the model.

## Seven rules

1. **Nobody writes progress.** Status is derived from what happened: plan steps, checks, receipts, git, claims. No
   status file is edited by a person or a model. (GSD's and BMAD's main failure, avoided.)
2. **Work is a graph, run in waves.** A plan's steps name the files they expect to touch and the steps they wait on.
   The engine groups steps with no overlap into a wave and runs them at once, then checks what each one actually
   changed: the declared files only schedule, the real changes decide.
3. **Each worker gets a packet, not a history.** The original request word for word, the step, the start card and
   pointers. It reports back in a few lines; anything long stays in a file it points to.
4. **A check decides done, never the writer.** Checks the engine runs, a receipt tied to file hashes, and for risky work
   a fresh review by the other vendor (two for security). "Not reviewed" is an honest end, never "done".
5. **The right model for each step.** Collecting and searching go to light models, building to frontier ones, review to
   the other vendor; the number of parallel workers follows quota, memory and processor room.
6. **Everything an AI reads has a size.** What is loaded into an AI has a hard byte limit checked by code. What is
   stored has a soft target (R435): past it, the next close folds duplicates and overridden lines, and a live decision
   is never dropped to reach it. Adopted sources are never folded.
7. **The method proves itself.** Every part can be switched off on the bench. A part stays only while the bench shows
   it helps, and the whole method is held to the product target against the plain CLI.

## The loop of one request

| Step | What happens | Who decides |
| :-- | :-- | :-- |
| 1. Size | Small, normal or risky, from the request and the paths it touches; the engine raises it before a risky call runs (R435). | Engine |
| 2. Plan | Small: no plan. Otherwise steps, each with `files`, `waits`, the check that proves it and, for normal and risky, the contract (`Extends:` or `New:`). | Agent writes, engine validates |
| 3. Waves | Steps with no unmet wait and no declared overlap start together. Each writing worker is a team conversation in its own copy, started from the leader's current commit (as a branched conversation's copy starts from its source, R201); read-only work (search, collect, review) goes to helpers. A step that changes interfaces others use is a `wait` of theirs, not a neighbour in the same wave. | Engine schedules |
| 4. Join | The leader's copy is the integration copy. After each wave the leader brings each worker's commits in, as R201 already has the agent do (Faber itself never merges), and the engine runs the project's checks on the combined result. Before that, the engine compares each worker's real changes (added, deleted and renamed files included) with what it declared: an overlap nobody declared sends the later step back to run again on top of the joined result instead of merging both. A red check on the combined result goes to the worker whose change it names, on that combined version, once; a failure with no clear owner, or a second failure, goes to the leader. | Engine checks, leader merges and fixes |
| 5. Prove | Receipt from the recorded checks; the review gate for risky work. | Engine and the other vendor |
| 6. Close | The done card: what changed, in plain words, what was checked, one question at most, follow-ups offered as next tasks. | Agent proposes, engine records |

Small requests skip steps 2 to 4 and cost nothing extra. Parallelism starts at two steps; a plan whose steps all touch
the same files runs in order with no team at all.

## The packet

| Part | Limit | Source |
| :-- | :-- | :-- |
| The request, word for word, with later corrections; past 2 KB the part this step serves plus a pointer to the whole | 2 KB | The conversation |
| The step: goal, files, check, contract | 1 KB | The plan |
| The start card | 6 KB | Built (#464): rules, kept decisions, taste |
| Pointers: which pages, tasks or files to open, and when | 0.5 KB | The plan and the index |

The whole request, every later correction and the attachments stay in the conversation record, and a worker reads
them through Faber's tools in bounded pieces whenever the packet's part is not enough: nothing the person said is ever
out of reach. Everything else comes by search, at the moment it is needed: `memory_search` for notes and decisions,
`map` for what exists, the code graph for code. A worker's report is a summary of at most 1 KB that always says whether
anything failed and names, by its full path in the worker's copy, the file that keeps every failure line, the logs,
diffs and long output whole; the leader reads that file in bounded pieces only when needed. So the leader's context grows by a few lines per step, not by every worker's transcript.

## What a person sees

- **In the app.** One line on the project, always derived: "3 of 5 steps checked, 2 running, 1 needs you". The plan
  shows each step's state (waiting, running, checking, done, needs you) and opens its conversation (task 040). Words
  follow the plain-words rule: no branch, merge or CI talk.
- **On GitHub.** BOARD.md, generated: doing, checking, next, waiting, landed, each list at most ten rows. Built for
  Faber's own repository (docs/dev/workflow.md#board); a person's project gets the same page on a branch of its own
  when they turn it on in the project's settings, since it publishes to their repository.
- **Notices.** One when a long run finishes or needs a decision (task 045), never one per step.

## Files and their limits

| File | Holds | Limit | Written by |
| :-- | :-- | :-- | :-- |
| `AGENTS.md` / `CLAUDE.md` | Rules the code cannot show, and where to look | 14 KB (Faber's own: checked) | The person or the owner's sessions |
| `.faber/rules.md` | Facts from manifests, CI and proof commands | 3 KB | The engine (built) |
| `.faber/decisions.md` | Lasting choices, `[by person]` or `[by AI]` | 8 KB, then folded; old ones move to `decisions-old.md`, never loaded | The agent; the person's are recorded |
| `.faber/taste.md`, `lessons.md` | Corrections and what went wrong | 1 KB, 2 KB | The engine after the person's yes |
| Task file (`.faber/tasks/` or the project's own, `docs/tasks/` here) | One task: title, `Area`, `Files`, `Depends`, `Size`, what done looks like | 6 KB (here: checked) | The agent at close, or the person |
| BOARD.md | Generated progress | Ten rows a list | Code only |
| Living pages | One kind of question each, listed in an index that says when to read it | 240 lines, 20 KB (checked) | Whoever changes what they describe |

`.faber/tasks/` replaces R435's `next.md`: the same one-file-per-task format as Faber's own queue, so a long project
keeps a real backlog, two workers never edit the same line, and "ready" is computed from `Depends`. A project that
already keeps tasks its own way is adopted instead (R435, task 906).

## Faber's own repository

The same method, with the parts a shared repository needs and a person's project does not: lane claims, the landing
queue, decision numbers and the task claims of `scripts/tasks.mjs`. Its board is the first one built. AGENTS.md shrinks
to the repository's own lines once the engine enforces the loop (task 935).

## Measuring it

- **The bench** (`cargo run -p faber-engine --example bench`) gets a switch for each part: card, packets, waves and join,
  review gate. The same requests run with the part on and off, in paired runs (the gate of task 905 stays as written).
- **The bar** is the product's (docs/product/README.md): win or tie 9 of 10 requests against the plain CLI, which keeps
  its own subagents; at most 70% of its cost per accepted task, priced at each model's API rates with cache, counting
  failed attempts, joins, repairs and reviews (a run whose cost is not fully known cannot pass); and for waves, less
  time from the request to the accepted result on requests with independent parts. A part that does not help on the
  bench is switched off, not argued for. Runs stay inside the US$25 test allowance.
- **The judge.** Every run records each vendor that contributed. A run of one vendor is judged blind by the other with
  an executable proof, as today. A run where both contributed has no outside vendor left, so it counts only when the
  executable proof passes and both vendors, judging blind in fresh sessions, agree on the winner; the owner checks a
  sample of three such verdicts by eye before the bar is claimed.
- **Real use** keeps local counts only, never sent: how often the person corrects a result after "done", how often work
  reopens, time to done.

## What is cut

From Faber Flow: the twelve-feature catalog, the composition vocabulary, effect state machines, confidence-interval
qualification and the pinned BMAD and GSD comparisons. Each costs more than it can return on one shared subscription;
the plain CLI is the competitor that matters. From R435: `next.md` (now task files) and slice order as a rule (slices
whose tasks do not depend on each other run in parallel).

## Build order

Each slice is a few tasks the size of one pull request; tasks already queued keep their names.

1. **Visible progress.** The project line and step states in the app, derived (with task 040); the board for a person's
   project as an option. Faber's own board: built.
2. **Plan graph and waves.** `files` and `waits` on plan steps, wave grouping by file overlap, parallel start of helpers
   and team conversations for both CLIs (with task 021), the join step.
3. **Packets and short reports.** The packet built by the engine, reports capped at 1 KB with long output in a file,
   the leader's context measured on a five-step request.
4. **Proof.** Contract, receipt from hashes, review gate: tasks 930 to 933, unchanged.
5. **Memory and tasks.** Caps, folding, taste and lessons, delete: tasks 910 to 923; `.faber/tasks/` in place of
   `next.md` (task 922 changes).
6. **Measurement.** Bench switches per part, the local correction count, then the method's whole cost (task 935).

Slices 1, 2, 4 and 5 can start at once; 3 needs 2's plan fields; 6 needs the parts it switches.
