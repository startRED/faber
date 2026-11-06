# Lean Faber: cut the overengineering, close the weak points, beat the super-agent

Status: In progress. Proposed by AI (2026-11-05) at the owner's request. Done the same day with his yes: D1 fast lane (R556, `1950ced1`, its first full run green) and D2 paperwork (R557; product map 416 KB to 103 KB). The other steps marked **Owner** still wait for Erick's yes.
It builds on the W9 audit of 2026-10-31 (`reports/reforma/workflow-audit.md`, outside the repository), measures again
what changed since, and turns the comparison with DeepSeek's super-agent prototype
([research](../../research/2026-11-02-super-agent/faber-fit.md)) into concrete steps.

## 1. What the numbers say

Measured on 2026-11-05 from `gh`, `git log origin/phase-1-base` and the stored conversations.

| Measure | 2026-10-31 (W9) | 2026-11-05 | Reading |
| :--- | :--- | :--- | :--- |
| Pull request opened to landed (last 150) | median 1.0 h, p90 18.3 h | **median 6.7 h**, p75 16.1 h, p90 23.9 h | The trains made landing slower, not faster |
| Queue runs (last 100) | 28% red, 43% cancelled | 30% red, 11% cancelled, median 11.9 min, p90 24.5 min | Cancellations fell; red stayed; each run still waits |
| `phase-1-base` full run | — | median 25.8 min | |
| Commits about the process itself (7 days) | 38% | ~33% (98 of 297) | Still a third of the output |
| Decision rows (`docs/decisions.md`) | 484 | **555** (71 in 5 days) | Decisions are written for small choices |
| `docs/product-map.md` | 321 KB | 870 lines, rows of up to 900 characters listing edge cases | Too long to read, so agents search and miss |
| Markdown pages under `docs/` | — | 301 | |
| `scripts/` files | 23 (10-18) | 74 | |
| `.github/workflows/ci.yml` | 549 lines (10-18) | 867 lines, 18 jobs, plus `queue.yml` | |
| `testkit/` (own test runner) | — | 22,594 lines of Rust | |
| Watchers run beside the work | — | train-kick, claims sweep, renew-claims, release-watch, window guard, quota-watch | Machinery to keep machinery moving |
| Project messages Faber sent by itself, per message from the person | 23 | **8.6** (577 vs 67 since 11-01) | Better, still most of the traffic |
| Coordination tool calls vs file edits (Projects, since 11-01) | 6,440 vs 5,100 | **731 vs 580** | Still more talking about work than work |
| Automatic messages by kind (since 11-01) | — | keep going 268, another agent asked 202, helper done 54, closing turn 23, stuck 15 | |
| 1.0 checklist | — | 0 of 19 ticked | |

What works and stays: `main` only moves to checked commits, so the owner's installed Faber never got a broken update;
cheap local checks catch lint and type errors before CI; reviews by the other vendor catch real defects in state and
security changes.

## 2. The filter every rule and feature passes

1. **A rule stays only if it caught a real defect in the last 30 days or protects the owner's installed app, data or
   sign-ins.** Otherwise it goes, or moves to release time.
2. **One way to do each thing.** A second mechanism for the same job is deleted, not kept "just in case".
3. **Simplest thing first.** One agent and the CLI's own tools; a layer (helper, review, keep going, team) only where
   the benchmark shows it helps, and the screen says when it is added.
4. **Done means the person can see it.** A session ends when its change is on the base and shown working in the app,
   never when it is queued.

## 3. Development process: cuts

**D1. Fast lane: land directly, check after (Owner).** Replace the queue, trains and their scripts with one path:

- `node scripts/ship.mjs` runs the quick proofs on the PC (fmt, lint, typecheck, docs, build, the unit tests of the
  changed crates and files; target under 5 minutes), rebases on `phase-1-base` and pushes there directly. No squash
  rounds, no numbering, no `queue/*` branch, no PC-side `faber/verify` status.
- Every push to `phase-1-base` runs a quick gate (the same proofs, under 5 minutes); full suites (screen, macOS,
  memory limit of R125, Clippy) run after it, one run at a time on the newest commit: pushes made meanwhile are covered
  by the next run, so nothing is cancelled and thrown away.
- **`main`, which updates the owner's installed Faber, still moves only to a base commit whose full run is green.**
  This keeps the one guarantee that measurably helped.
- A red full run finds the culprit among the commits since the last green one (usually one to three, checked by
  reverting each in turn) and pushes a revert commit with the failing log; the author's session is woken by the
  comment. Nobody fixes forward on a red base while other work waits.
- Delete: trains, `--split`, `--finish`, `train-then`, `queue.yml`, the verify status, the board branch, number
  placeholders and renumbering, lane claims and their sweep and renewal, the watchers listed in section 1.
- Expected: a change is on the base minutes after its proofs instead of a median 6.7 h; `ci.yml` falls from 867 lines
  to roughly 300; `scripts/land.mjs`, `tasks.mjs`, `numbers.mjs` and their tests shrink to `ship.mjs`. Proof: median
  push-to-base time and red-base hours, measured over the first week, beside this table.
- Needs the process freeze (R489, until 2026-11-14) lifted for this one change, since it deletes machinery.

**D2. Paperwork moves out of each change (Owner).**

- What's-new: written once per release from the landed titles, by hand in every language by the release session
  (still never machine-translated), instead of a `docs/next/<branch>.json` per pull request.
- Decision rows: only for a direction choice the owner makes or a rule that changes what a person gets. A choice
  inside one feature lives in its code comment or commit body. Target: under 5 decisions a week.
- Product map: one or two lines per row (what it is, where it lives, its check). Edge cases go to code comments and
  tests. `engine/tests/product_map.rs` checks only settings, stored values and shell commands, not every feature word.
- Commit body: the memory and processor line only when the change touches processes, long-lived state or the screen's
  list sizes; `Extends:`/`New:` goes (the map search already answers it).

**D3. Documentation diet.**

- Dated specs, plans and research move to `docs/history/`, which agents never read; `docs/README.md` lists only living
  pages.
- `docs/decisions.md` splits into the decisions in force (expected under 80 rows) and `docs/history/decisions.md`.
- `AGENTS.md` under 60 lines: the hard rules (credentials, no shell, no bundling, vendor names, telemetry, memory
  limit, languages), the proof command and the fast lane. Everything a test enforces leaves it.

**D4. Fewer lanes, sessions that finish.**

- At most three lanes at a time, one per area; no manager of leads, no team conversations sending each other work.
- A lane takes the biggest coherent slice (one feature with its tests), ships it with D1, checks it in the app built
  from the base, and only then takes the next task.
- The task list keeps its cap of 15, written by the owner or the session he names.

**D5. Checks that fail by chance leave the gate.** A check that failed and then passed on the same commit moves to the
nightly run automatically, with its name in the morning summary, until fixed. Reviews stay by risk: the other vendor
for engine state, stored data, permissions, processes and security (two for security); words, docs, tests and
screen-only changes ship on their proofs.

**D6. Code that two implementations carry.** See section 6 (measured by a read-only helper on the same tree).

## 4. Product: cuts

**P1. Direct by default.** A Projects request runs as the CLI's own turn with the person's words. Keep going stays a
chip the person turns on, and stops after two turns that changed no file and passed no new check.

**P2. Agents talk to helpers they started, nothing else.** The `Conversation` tool (505 calls since 11-01, the largest
coordination cost) and the 202 "another agent asked" wakes come from conversations sending each other work. Projects
keep subagents and the other vendor's helper (R161); conversation-to-conversation messaging goes.

**P3. No automatic closing turn.** The 23 `wake.conclusion` turns each re-send the context to ask the AI to sum up.
The engine writes the closing box from what the turn did (files changed, checks run, plan state); the AI's own
`wrap_up` stays when it calls it.

**P4. Fewer Faber tools.** Plan, WrapUp, Status, Verify, Quota, Review and Helper become at most three: `plan` (steps
and closing box), `check` (Faber's proofs and the other vendor's review) and `helper`. Each tool that stays has a
measured use per conversation and a benchmark gain; one that has neither goes.

**P5. Features keep their core.** Rows of the product map that list a dozen edge cases (visual answers, permissions,
queue and steer) are reviewed one by one: keep what a person uses, delete paths for states that never happen, and
keep the tests for those that do.

## 5. Closing every weak point against the super-agent

| Weak point today | Target | Smallest version that closes it | Proof |
| :--- | :--- | :--- | :--- |
| Work starts only when the person asks | Faber notices and proposes | "Faber noticed": an opt-in watch of the open project's own signals (a red check on the person's pushes, a new review comment, a failing local test) through tools already signed in (`gh`, read-only). It shows a card "Found X. Fix it?"; one click starts a normal conversation. It never acts alone, treats what it reads as data, and work it starts never gets full access by itself (**Owner** picks the sources). Routines ("every morning, do X") while the app is open come after | A planted red check shows the card within 5 minutes; no conversation starts without the click; idle cost one timer, no process |
| Big work, one piece at a time | Parallel where parts are independent | The agent may run up to three helpers at once, each in its own copy of the project (git worktree), when its plan names parts that touch different files; the rest queue under the RAM budget. The lead keeps answering the person while they work | A three-part task finishes faster than serial on the benchmark, at no worse score; RAM stays under budget |
| Chat's default has full access (R121, owner decision) | Full access with a net | Keep no approval cards; add what makes mistakes cheap: every file written outside the hidden workspace is covered by Rewind, irreversible actions (push, deleting outside the folder, sending) stay blocked unless the person asked in words, and work started by "Faber noticed" runs with the CLI's own sandbox (**Owner** confirms) | A planted delete outside the folder is refused; a write outside is undone in one click |
| Finding the right past task needs the whole history | Recall by summary and number | Task 044: each conversation and helper keeps a two-line summary and an id; the agent's recall tool lists summaries and reads one by id on demand | A follow-up naming an old task reaches it without loading other histories; tokens per resume measured before and after |
| No measured gain over using the CLI directly | A published number | The fair benchmark ([fair-bench](../../dev/fair-bench.md)): 8 real requests, Faber against plain Claude Code and plain Codex, judged blind by the other vendor, budget under US$25 (pre-approved). Repeated before each release on a rotating sample | The score and cost per request in `docs/dev/benchmark.md` and the release notes |
| Base still in construction | Faber 1.0 | Feature freeze until the 1.0 checklist (task 390, 0 of 19) is done: the Electron release switch, `src-tauri/` retired, the data-loss fixes of task 515 | The 19 boxes ticked with their proofs |

## 6. Code: what two implementations carry

Measured read-only on the tree at `d151ba31` (`wc -l` over tracked files, tests included). Each line is a candidate;
a deletion lands only with its proofs green and its last users moved.

| Weight | Size | Step |
| :--- | :--- | :--- |
| Two shells wired at once: `src-tauri/` drives the engine in-process, `electron/` + `sidecar/` + `host/` drive it through `faber_host`; both are Cargo members and both run in CI; `ui/shell.ts` switches between them | `src-tauri/` 7,160 Rust + 807 JS; duplicated tray (~350 vs ~1,800 in `host/src/tray/`), pane (~2,800 vs ~5,000 in `host/src/pane/` vs 400 in `electron/pane.ts`), visual, autostart and awake | Retire `src-tauri/` with the Electron release (already planned); then also its CI path and the `ui/shell.ts` fallback. The largest single cut |
| Four update paths: `src-tauri/src/update.rs` (1,364) + `update/handoff.rs`, the `update/` crate (3,017), `scripts/install-update-self.mjs` (346), `sidecar/src/updates.rs` | ~5,000 lines | One updater in Electron's shell plus the `update/` handoff; delete the rest with `src-tauri/` |
| Two "kits" modules: `engine/src/engine/kits.rs` (4,043) and `engine/src/tools/kits.rs` (3,097) | 7,140 lines | Read both; merge what overlaps, keep one module |
| Antigravity/Gemini support: `Vendor::Gemini` in `engine/src/model.rs` and ~30 match arms in the engine, five UI files, a line in each locale, `testkit/src/fake.rs` replay (lines 90 to 345), 17 recordings in `tests/fixtures/agy/`, tasks 010 to 092 in `docs/tasks/later/` | spread across the tree | **Owner**: the product says only Claude Code and Codex, while the later tasks plan agy to spare Claude quota. Either finish it as one driver after 1.0, or delete it all now |
| Engine size: `engine/` 147,360 lines (274 files); `engine/src/engine.rs` 6,705 and `engine/tests/engine.rs` 7,561 | | Not a deletion target by itself; P1 to P5 remove the paths for conversation messaging, closing turns and merged tools, and section 4's review of edge-case paths trims the rest |
| Own test runner `testkit/`: 22,594 lines (CDP driver 1,250, screen tester ~7,000, sandbox 564), plus `bench/` pulled in by `#[path]` as a second harness | 24,000 lines | Keep the parts that pay (affected-test selection, screen driver); with D1, drop the landing, verify-status and base-warning parts; fold `bench/` into one harness |
| Process scripts: 7,635 non-test lines in `scripts/` (+4,763 of their tests); `land.mjs` 1,181, `numbers.mjs` 300, `council.mjs` 382, `persona-eval.mjs` 447, `pace.mjs`, `owed.mjs`, `direction.mjs`, `tasks.mjs` | ~3,000 lines go with D1 and D2 | Delete with D1 and D2; keep `cargo.mjs`, `docs.mjs`, the release and channel scripts |
| Screens that look like experiments: `ui/Hole.tsx` (540), `ui/Site.tsx` (1,019), `ui/tour.ts`, `ui/draft.ts`, `ui/worklog.ts`, `ui/illustrations/scenes.tsx`, `engine/src/creative.rs` + `creative_judges.rs` | ~2,000+ lines | Check each against the product map and real use; delete what no person reaches |
| Reports kept in the repository: `reports/` | 1,128 lines | Move out of the repository with the D3 history move |

Expected total, before the edge-case review: roughly 25,000 to 30,000 lines fewer, most of it from retiring
`src-tauri/` and the landing machinery.

## 7. Clear advantages to keep and show

What Faber does that the super-agent does not, each visible on screen:

- Two vendors in one place, each checking the other (the reviewer's name and verdict on the result).
- The person's own subscriptions, with quota shown, paced and handed between vendors near the limit; work pauses when
  someone else uses a shared account.
- Any kind of work on a desktop, not only a GitHub repository: files, research, images, video, the person's own apps.
- Proof before "done", on screen as the check that passed.
- Undo for file changes (Rewind) and work in a copy for projects.
- Light: under 500 MB without a CLI, CLIs started on demand and closed when idle.
- A visible sign when the agent or a helper seems stuck, with what to do.
- After section 5: proposals from what Faber noticed, parallel helpers in separate copies, recall by summary, a
  published benchmark and a 1.0 release.

## 8. Order

1. **Week 1 (after the owner's yes):** D1 fast lane, D4 lanes, D5 flaky checks, D2 paperwork. These make every later
   step faster, so they go first. Ships with D1 itself, checked by its own first week of numbers.
2. **Week 2:** D3 documentation diet; P1 to P4; section 6 deletions; task 044 recall; the fair benchmark's first run.
3. **Week 3:** "Faber noticed" in observe mode, parallel helpers, the Chat safety net.
4. **Then:** the 1.0 checklist with the feature freeze; nothing new enters until it is done.

Each step reports its memory and processor cost and has a check that fails without it.

## 9. What waits on the owner

1. Lift the process freeze for D1 and the deletions of section 3.
2. Direct landing with checks after, `main` still moving only to fully green commits (D1).
3. What's-new once per release; decision rows only for direction (D2).
4. The Chat safety net under R121, and full access never given to work Faber starts by itself (section 5).
5. Which signals "Faber noticed" may watch.

## 10. Later: visual answers at the level of the best chat apps

Faber already has visual answers (R348) and ten built-in pieces. The gap is quality and interaction: a short design
guide the AI reads before drawing (one font scale, a small palette that works in light and dark, diagrams sized to the
column), diagram boxes a person clicks to ask about that part, and step-by-step diagrams for processes. Kept as
[task 960](../../tasks/later/960-visual-answers-guide-and-clicks.md), after 1.0.
