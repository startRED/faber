# Faber's own work method, for every project

Status: proposal, waiting for the owner's yes. Nothing is built from it until then.

- **The yes.** The approval is the owner's own answer on the question card, as for every decision in this repo. The
  decision row records it.
- **The slices.** The four slices enter `docs/tasks/` in the commit that records the yes.
- **What exactly is approved.** The row names the commit of this file that was approved. Each slice task cites that
  commit, and any later change to the design needs a new yes.
- **The map.** The yes also settles the one split this spec leaves open: the product map (see "Faber's own
  development").

Asked by the owner on 2026-10-22 (task 990, direction); the details are the AI's (owner delegated). Sources: AGENTS.md,
docs/product/ (building.md "Coherence of a project's work", following.md "Learning and memory"), docs/dev/,
docs/product-rules.md, docs/decisions.md, the 2026-10-16 own-lessons spec, Felix Rieseberg's "How to be more ambitious",
and how BMAD, Spec Kit, Kiro, Agent OS and the AGENTS.md convention keep their documents (ideas only, nothing copied).
Three rounds of review by fresh Codex gpt-6.1-sol sessions (22, 25, then 3 findings) are answered in this version; the third was the last (R147).

## The problem

Faber's own development runs on a method that works:

- a short rules file;
- decisions with authors;
- taste cards;
- a map of what exists;
- a task queue;
- outside checks, and a review by the other vendor.

A person's project gets almost none of it. Today Faber's agent writes only one file in a project's `.faber/`:
`decisions.md`. Memory can also read five other names (`taste`, `plans`, `recipes`, `learning`, `memory`), but nothing
writes them. The brief in five parts and the coherence contract are planned, not built. So there are two ways of
working, and the person's way is the weaker one.

What others found, used here as hints, not proofs. The design is gated on Faber's own measurement (see Cost).

- **More context is not free, and may not help.** Chroma ("Context rot", 2025) saw quality drop as input grew. On the
  coding tasks of a 2026 study (arXiv 2602.11988), generated context files raised cost by about 20% on average without
  raising success. Codex's default cap on instruction files is 32 KiB. So Faber loads a small card, leaves the rest to
  search, and lets its own measurement decide.
- **Prose that restates code goes stale.** BMAD warns about this. Faber's own product map is 270 KB and is read whole.
  So facts the code can prove are extracted on demand by the `map` tool, never written down.
- **Ceremony should match the change.** BMAD's tracks and Kiro's quick spec both scale it.
- **No model grades its own work** (Rieseberg; R146).

## The method in one paragraph

Faber works toward the goal behind the request. Before building, it states what the change extends and how it will be
proved. A check Faber runs, never the writer, decides that the work is done, and a second AI reviews anything risky.
What was learned is written down only when it will change later work. The person writes nothing: Faber sets up
`.faber/`, keeps it small and current, and asks only for decisions, as options on a card. Permissions, approvals and
budgets are enforced by the engine, never by text the model reads.

## The loop, sized by risk

| Size | Steps |
|---|---|
| **Small** | Do it, run the checks, close. Nothing is written to `.faber/`. |
| **Normal** | 1. **Contract** in the plan (`plan` tool): `Extends:` the closest existing thing, or `New:` and why nothing fits; the check that will prove it; a measurable target when the work is about speed, size or cost. 2. Build. 3. Faber's checks decide done (R204, built). 4. **Close** (`wrap_up`): the receipt, look around (R369), and the `.faber/` proposals below. |
| **Risky** | Normal, plus the review gate below. |

**Choosing the size.** The size is set when the plan is written, before any action, from the request and the paths the
plan names. When a request is unknown, it starts at normal.

- **Raising it.** The engine raises the size, and never lowers it, before a call runs: the permission layer already
  sees each call first.
- **What counts as a boundary.** Deleting files, git history, credentials or secret files, payment or network-publish
  commands, anything the permission layer holds back, and any tool or command it cannot classify.
- **What raising does.** The call waits until the contract is written.
- **Words in the request** are only a first hint. The boundary checks are what count.

**The review gate.** A risky turn is done only after the review passes:

- **Who reviews.** A new CLI session with no earlier context, started read-only by the engine. It is the other vendor
  when that vendor's quota allows, else the same vendor, and the receipt says which.
- **Security.** Security takes two independent reviews.
- **What blocks.** Findings classed as person, data, security or written rule block until each one is fixed (with a
  failing test or a repro), or until an answer is accepted by the reviewer's next round or by the person on a card. A
  note never blocks (R147). The two security reviews are recorded separately.
- **No reviewer.** If no reviewer can start, the turn ends "not reviewed" in the closing box and never as done.
- **The receipt.** The engine writes it from the recorded check and review results, never from the agent's words.
- **Results go stale.** Each result is tied to the hashes of the files it saw. A later change to those files voids it,
  so the receipt never cites a check or review of an older version.

**Rieseberg's ideas**, each carried by a part that already exists:

- **The goal behind the task:** the bigger-ask offer (R419).
- **Loops without the person:** standing goals (R422) and Faber's checks.
- **Decisions with options:** the closing box's one question.
- **An outside judge:** the checks and the review gate.
- **Hard targets:** the contract's target line.
- **Other fields:** a red-team review the other vendor actually runs, never a persona.

## What lives in `.faber/`

| File | Holds | Written by | Cap |
|---|---|---|---|
| `rules.md` | The brief in five parts (building.md): what the project is for, where the work happens, how work is checked, what to do when something is missing, and what needs the person's yes. | The engine, deterministically (see "Building `rules.md`"). | 3 KB |
| `decisions.md` | Lasting choices, one list item each, ending `[by person]` or `[by AI]` (built), with an optional `because ...` half-line. | The agent, only for a choice that shapes later work (built). | 8 KB, then items a later one overrides move to `decisions-old.md`, which is never loaded |
| `taste.md` | Rules from the person's corrections. | The agent proposes; the engine writes after the person's yes. | 1 KB |
| `lessons.md` | What went wrong and why, when it changes later work. | The agent proposes at close; the engine writes after the yes. | 2 KB |
| `next.md` | Follow-ups found and not done, one line each (at most 200 characters) with why. | The agent at close; a line goes when its work lands. | 12 lines |

Other state:

- **Kept elsewhere.** The project map is never a file: the `map` tool extracts it on call. Plans stay in the
  conversation and standing goals in `projects.json` (both built).
- **Legacy names.** `plans`, `recipes`, `learning` and `memory` stay readable and searchable for folders that already
  have them, so nothing is lost. Faber just never writes them.

### Building `rules.md`

The engine builds `rules.md` from fields of a fixed shape, never from free prose:

| Source | Fields |
|---|---|
| Manifests (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`) | Languages, script names and the one-line description |
| `.github/workflows/*` | Names of the commands each job runs |
| The instruction files | The commands under a "Proof commands" heading |
| The project's own instruction files | Their names only |

Each field is length-limited. The description is at most 160 characters and is kept out of the card (see "Trust").

Two of the five parts cannot come from code:

- **What the project is for.** It is the description line, or the person's own words from the project page when there
  is no description.
- **What needs the person's yes.** It is Faber's fixed list, the boundaries above, never read from the repository.

When `rules.md` and an instruction file disagree, the instruction file wins, because `rules.md` only adds.

**Items.** An item's identity is the hash of its first line, normalised. An item longer than 500 characters loads as its
first 200 characters and "…".

### Trust: what the agent can and cannot change

An agent can write anything inside the project. So the folder holds text, and authority lives in Faber's data folder
(as `decisions-kept.json` does today, `memory.rs`).

- **Provenance.** Every item the person said, chose, kept or approved has its item ID and content hash recorded in the
  data folder. A `[by person]` mark, a taste line or a lesson that the record does not match counts as an unreviewed AI
  proposal, whatever the file says.
- **`rules.md`.** Its hash is recorded when the engine writes it. An edit by anyone else is overwritten at the next
  rebuild.
- **Repository text never reaches the system prompt.** The card in the system prompt holds only engine-fixed sentences,
  tokens of a checked shape (language names, file names, commands matching the proof-command pattern), and items the
  person kept or approved (each with a matching record).
  - The project's description and every unreviewed AI item reach the agent only as results of Faber's own tools
    (`memory_search`, and `status` at the session's first call). Tool results rank below the system prompt.
  - The card grants no permission and overrides nothing. The sandbox, the permission cards and the review gate do the
    enforcing.
- **Protecting the data folder.** Slice 1 denies writes to Faber's data folder in every sandboxed mode of both CLIs, and
  tests an attempt from each. In full access the person has handed the agent their own user account, and Settings
  must say so where full access is chosen.
- **Malicious repositories.** Slice 1 adds checks for a hostile repository: injected text in the files `rules.md` is
  built from, a forged `[by person]` mark, and a later session reading a planted file.

### The start card

At each CLI session start and resume, a project's own agent gets one fenced block after the persona. It is capped at
6 KB by bytes, with fixed shares:

| Share | Contents |
|---|---|
| 3 KB | `rules.md`, without the repository's description line |
| 2 KB | Newest first, the first lines of decisions with a matching record |
| 1 KB | Kept taste lines |

What does not fit is counted, not hidden: "and 14 older decisions; search them with `memory_search`". Lessons and
follow-ups are never loaded. Lessons are searched, and follow-ups show on the done card.

A change during a session reaches the next session; a session never rebuilds its own card. Technical details shows the
card as sent, and the CLI still reads `CLAUDE.md` and `AGENTS.md` by itself.

### Staying small and current

- **Two kinds of limit.** The card's shares are hard limits on what is loaded. A file's cap is a soft target for what is
  kept.
  - A file over its target is marked "over its limit" in Memory, and the next close asks the agent to fold it: merge
    duplicates and drop what a later line overrides.
  - When folding cannot reach the target without losing a live item, nothing is dropped. The file stays over its
    target, and Memory shows it.
  - Storage is bounded only by the person's own deletions. `decisions-old.md` grows but is never loaded.
- **Stale references.** These are checked only for backticked, project-relative file paths. A path that no longer
  exists gets `(stale?)` and leaves the card. Prose, commands and settings are never judged.
- **`rules.md` follows its sources.** It is rebuilt when a turn starts and a source file's hash has changed (manifests,
  CI files, instruction files). There is no timer and no watcher.
- **Deleting.** One click in Memory deletes any stored item, and its record goes with it. `rules.md` is built, so Memory
  shows it for reading only.

### Adopt before create

When a project already keeps one of these its own way, Faber uses that place and never makes a second copy in
`.faber/`. Examples are a decisions log, a style guide or a task list.

- **Approval.** The pointer and the person's approval of it live in the data folder, never in the repository. A
  repository cannot redirect anything.
- **Allowed targets.** A target is an existing file inside the project, or a folder for a task list.
  - Containment is checked at every open, not only at approval. The file is opened first, refusing a link.
  - The final path of the opened handle (resolving links, `..` and junctions, as `real_path` does today) must lie inside
    the project before any byte is written.
  - A new task file is created through a handle to its parent folder, checked in the same way.
  - A target outside the project is refused.
- **Adapters.** Each adopted format gets a small adapter that keeps its own IDs, dates and structure. Each adapter does
  only its format's own operations:
  - Faber's R-numbered table: append a row.
  - Its taste-card table: append a card.
  - Its `docs/tasks/` folder: add a task file, and delete it when the work lands.

  Adopted sources load by the same byte shares. Folding and one-click delete never touch them.
- **Before its adapter exists, a kind is read-only.** Until slice 4 builds the adapters, an adopted kind is loaded but
  never written. The agent's proposals for it show only on the closing box.
- **Existing content.** When adoption starts and `.faber/` already holds items of that kind, they are listed once for the
  person, to move or keep, before any write.

### Tasks and tool kits

- **One list per project.** For a person's project, `next.md` is the task list: a follow-up becomes work when the
  person picks it on the done card or a standing goal takes it. A project that has its own queue adopts it instead.
  Claims and lanes stay a Faber-repo practice and are not imposed on other projects.
- **Tool kits.** These are already assembled per task (product map, "Session kits"): code picks skills and servers from
  task intent and stack fingerprints at session boundaries. The method only feeds that matcher with `rules.md`'s stack
  facts, and slice 1 checks that a project's kit is unchanged or better with them. Nothing is hand-written.

## Faber's own development, on the same method

The shared part is the engine's loop: size, contract, checks, review gate, receipt. Shared storage alone is not one
method, so the change is staged:

| Part | Today | After slice 4 |
|---|---|---|
| Rules | AGENTS.md | `.faber/rules.md` adds only what AGENTS.md lacks |
| Decisions, taste, tasks | `docs/decisions.md`, the taste cards, `docs/tasks/` | Adopted through adapters, formats unchanged |
| Done | AGENTS.md "Done means" plus R146 by hand | The engine's loop enforces it. AGENTS.md keeps only the repo-specific lines (landing, lanes, what's-new) and points at this spec. |
| Map | `docs/product-map.md`, read whole | Unchanged for now. A later proposal moves the facts the code proves to the `map` tool, step by step, with the owner's yes for each. |

Until the map step lands, the two ways have not fully converged, and this spec does not claim they have. Whether to
plan that step now or later is part of the owner's yes.

## What it replaces

- The open parts of building.md's "Coherence of a project's work" and its brief in five parts. They become the loop and
  `rules.md`.
- The 2026-10-16 own-lessons spec, row by row:

| Row | What happens to it |
|---|---|
| 1. One writer per feature | Already practice (R119). The contract is per feature. |
| 2. Re-review on the other vendor | The review gate. |
| 3. A red base stops the line | Built for Faber's repo. A person's project has no landing queue, so it is dropped there. |
| 4. Number placeholders | Built for Faber's repo (`docs/next/`), and kept by its adapter. |
| 5. Idle priority | Built in the verifier's capped jobs (#257). |
| 6. Short check output | Built (R155). |
| 7. Balanced CI shards | A Faber-repo practice. Dropped for a person's projects. |
| 8. Reviews that end | The review gate's blocking classes. |
| 9. Quota pacing | Auto (built). |

## Cost

These are acceptance limits that each slice measures. They are not estimates.

**Tokens.** The card is at most 6 KB, a byte limit and not a token promise. Slice 1 compares identical Faber
configurations with and without the card:

- the five R410 requests, three paired runs each, on both CLIs;
- cold, warm and resumed sessions;
- input, cached input, output and extra model calls each reported apart, with their spread;
- success judged as in R410.

**The gate**, declared before any run:

- **What is compared.** The sums over the five requests, for each CLI and each session condition apart.
- **What passes.** No fewer successes, and at most 5% more total input-plus-output tokens with the card.
- **Too close to call.** When the difference is within the spread of the paired runs, three more pairs run. If it is
  still within the spread, the result counts as failing.
- **What it proves.** A pass covers this suite only.
- **No card passes.** The owner chooses between a smaller card and none.

Slice 4 also reports the whole method's added cost: reviews, folding and the other extra calls, on the same scenarios.
It reports these with the same separation of input, cached input and output.

**Memory and processor.** At most 2 MB extra peak memory and 20 ms per session start, on the maintainer's PC.

- The 20 ms cover discovery, hashing, parsing and a `rules.md` rebuild.
- The fixtures are a typical project and the largest supported adopted source, Faber's own `docs/decisions.md`
  (262 KB).
- Storage: the soft targets above.
- There is no new process, timer or watcher.

## Extends

- `Extends: Project memory (X3, R328)`: the five files, the records, and one-click delete.
- `Extends: Faber's project map (R146)`: the facts behind `rules.md`.
- `Extends: Agent's plan and closing box`: the contract, the receipt and the proposals.
- `Extends: Faber's checks (R203)`: unchanged.
- `Extends: Technical details`: shows the card.
- `Extends: Project instruction and learning files`: `rules.md` and adoption.
- `Extends: Session kits`: stack facts as input.
- `Extends: Offer the bigger ask` and `Extends: Standing goals`: unchanged, named as parts of the loop.

New: none.

## Build slices

Each slice depends on the one before it. Each uses AGENTS.md "Done means" as its checklist: the product map, the design
page, a what's-new entry in every language, a check that fails without it, and the measured cost.

1. **Rules, card, trust and adoption targets.**
   - The engine builds `rules.md` from fixed fields and writes the provenance records.
   - It adds the card for both CLIs, shown in Technical details.
   - It denies the data folder in sandboxed modes.
   - It adds adoption pointers in the data folder (read-only until slice 4), with containment at every open and the
     one-time reconciliation.
   - It adds the hostile-repository checks, the stack facts for the kit matcher, and the token benchmark.
2. **Caps and staleness.** Byte caps, overflow counts, folding at close, `decisions-old.md`, and path-only stale flags,
   with tests that fail without each.
3. **Taste, lessons and next.** Proposals at close, written by the engine after the yes, follow-ups on the done card, and
   delete-with-record in Memory.
4. **The sized loop.**
   - The size is set at plan time, and raised before a call runs.
   - It adds the contract and the engine-written receipt, tied to file hashes.
   - It adds the review gate with its fallback and accepted answers.
   - It builds the adapters, proven on Faber's own repo, and measures the whole method's cost.
   - AGENTS.md's "Done means" then shrinks to its repo-specific lines.
