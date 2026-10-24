# Building software

How Projects builds software: one voice with a team behind it, many workers, self-development and coherence.

Phase 4's [nightly CLI candidate checks](../dev/cli-qualification.md) detect launch-option and request-schema drift
without sign-in. Their metadata receipts always remain unqualified; authenticated turns, cancellation and cleanup
evidence are still needed before a candidate can authorize installation.

## Building software: one voice, a team behind it

For frontend work, the existing guide delivery provides Faber's original `frontend-craft` and `frontend-proof`
skills ([contract](frontend.md)): project-aware composition and interaction states, followed by task, accessibility,
layout and resource evidence. They complement installed design skills and never impose Faber's own desktop theme
on another project. Delivery checks do not establish a measured gain in generated interface quality.

Normal mode feels like Claude Code: the user talks only to Faber. Behind it, Claude and Codex sessions work in
parallel, each on what it does best, and the result must read as if one careful team made it.

- **The lead keeps the thread.** One live session plans, splits the work and answers the user. Workers get a short
  brief and return a short report; a worker that stays on a part keeps its session while the prompt cache is warm, and
  each step loads only what it needs.
- **One map of the product, read before and updated after every change.** The design system (tokens, components,
  type, spacing, voice) and an inventory of every screen, button and flow with its purpose. A worker building UI starts
  from it; nothing ships that the map does not account for.
- **No duplicates, no strangers.** Before a new screen, button or setting, the agent searches the inventory for one
  with the same purpose and extends it instead. Code checks enforce the design system: only its tokens and components.
  A coherence pass looks at the whole app, every screen side by side at each size, not just the diff, and flags
  anything that does not look or behave like the rest.
- **Design starts from references.** Three directions grounded in real references come first; in autonomous mode the
  two judges from different vendors pick, with a written rubric. The final screens are checked in a real browser at
  1920, 1440 and phone width.
- **Done means it works.** The proof is written before the work and run after it: unit tests, end-to-end runs in a
  real browser or the real app, and a ghost user from another vendor that uses the result like a person, recorded.
  A bug hunter tries edge cases and broken paths on purpose. Every bug found gets its root cause fixed and a test that
  fails without the fix.
- **Everything persists.** The mode, the plan, each worker's progress and session, the product map, open question
  cards, parked actions and the report are saved as they change. Closing Faber, a crash or a reboot resumes exactly
  where it was, autonomous runs included; Faber keeps working from the tray when its window is closed.
- **Measured against the TL-ADE.** The same requests run on both: design scores from blind judges, bugs found after
  delivery, coherence findings, time, tokens and how often the user had to step in.

**Big work always reaches a conclusion.**

- "Stop" pauses, and the work resumes from where it stopped.
- Going 50% over the estimate only warns and switches to economy mode: cheaper models for support work, extras put
  off. Faber preserves unfinished work. Projects narrows or pauses before its live worker, mission and daily cost caps;
  an estimate overrun only warns. R88/R90/R91 govern admission, not the estimate. Built for a project agent that gives an estimate in its plan: past half again it, the screen says so once, helpers run one rung cheaper and no conversation starts (`engine/src/engine/plan.rs`, R414).
- A stuck attempt is dropped while what it produced stays. The lead tries another path (another approach, a stronger
  model or one from the other vendor, a clean session) and only then asks the user about that one point, while the rest
  goes on.
- When a quota runs out, the work moves to the other subscription, or waits and resumes by itself at the renewal.

**Decision list.** Every project keeps a decision list with authors. The AI's decisions show as "not reviewed".
The list is `.faber/decisions.md`: each entry ends with a short mark, `[by AI]` or `[by person]`, so the file reads
the same in any editor and either side can write it by hand. The project's agent is told to add a lasting choice, and only then, ending it with
`[by person]` when the person made it and `[by AI]` when it did; an entry with no mark has an unknown author and is never shown as the AI's. The Memory view of the project
shows the author, and "Keep" on an AI decision stores, outside the project, that the person kept those words, so a review is never read from the project, where an agent writes, and reworded words read "not reviewed" again (R415).

## Running many workers

Taken from Claude Code Projects and pushed further (AI proposal, not reviewed; research in
[projects-orchestration.md](../research/2026-10-14/projects-orchestration.md)).

- **The lead routes every message.** A quick question is answered in place; new work goes to a new worker or to the
  worker already in that area; several tasks in one message become several workers. It can list suggested workers
  with "start all".
- **The lead never reads the whole history.** It works at low effort from worker reports, recent turns and the
  `.faber/` memory, so a project can run for weeks.
- **A brief in five parts** goes to every worker, written by Faber, not by the user: what the project is for, where
  the work happens, how a worker checks its work, what to do when something is missing, and what needs approval.
- **A new project starts with proposals.** Faber reads the recent work and proposes the first tasks; the first worker
  only reads and proposes next steps.
- **Missing-item gate.** A worker finishes through a report with required "assumed" and "could not reach" fields. A
  missing login, tool, file or secret, or a stand-in (a mock, a fake key, an invented path), keeps the step waiting
  with the missing item named, and "done" is refused. The user's answer goes into the brief, and every new brief is
  checked against past gaps first. The reviewer from the other vendor checks "assumed" against the diff.
- **A canary before a batch.** When the lead fans out similar workers (one per repository, file, chapter or
  thumbnail), one runs first and passes its proof and a review from the other vendor; its mistakes become rules in
  the brief, then the rest start in parallel. Batches of three or fewer, or with a very cheap proof, skip it.
- **Workers leave a written state and exit.** A worker's lease renews only with evidence of progress. When it lapses
  or the work ends, the worker writes its state (step, branch, next step, proof, files) to `.faber/` and its process
  closes. A follow-up resumes the session while the one-hour cache is warm and starts a fresh one from that state when
  it is cold, showing both token estimates. The same file serves vendor switches and crash recovery.
- **Workers claim their files (R155).** Before it writes, each worker states the files it will change; Faber refuses a
  change outside the claim or a claim that overlaps another worker's, and the reviewer sees the claim. It comes with
  missions of more than one task, as Faber's own lanes already do (R154).
- **Limits in plain words become real limits.** "At most two at a time" or "ask before starting" is stored as a
  setting the engine enforces, not a hint for the model. New workers have a daily cap.
- **After delivery the worker keeps watching.** A worker that opened a pull request fixes failing checks and review
  comments until it lands. Its row offers the next step as a plain button ("Fix the failing check", "Publish"), and
  publishing still follows guardrail 3.
- **The progress view adds "Waiting on you" and "Landing"** (approved, waiting to merge), and a worker idle for a week
  is resolved by itself.
- **A library per project** lists the files the user added and the files workers produced.
- **One question explains a stall:** for every worker, what it was asked, what it assumed, what it could not reach and
  what it waits on.

## Self-development (R96, through the project conversation)

R96 put a minimal Projects slice for self-development before Full Chat. It was first planned as missions (a lead,
roles, a board and a serial proof queue; [spec](../superpowers/specs/2026-10-12-self-development-mvp-design.md)), and
R202 replaces that shape: self-development is the project conversation of R160 and R161, and the mission machinery
stays in the engine only for what still uses it, with no way to start one. How to start it, step by step:
[dev/self-development.md](../dev/self-development.md). The local development instance never touches the installed
release in E:\Apps\Faber (R95).

Since R160 the person reaches this through the project conversation, not a mission screen: Faber builds Faber the way
the sessions that build it work. The project's agent starts conversations of its own for separate pieces (R198,
"Conversation started by the agent" in the product map), each conversation can work in its own copy of the folder, a
conversation can keep going while the person is away, and a red check on its pull request wakes it. A landed change
reaches the installed app through the public updater (R156). A live probe on the real Claude Code CLI
(`engine/examples/selfdev_probe.rs`, two small Haiku turns) checked it on 2026-10-17: the agent started a conversation,
read it back, and the new conversation wrote its file in 16 s. A second probe (`engine/examples/selfdev_accept.rs`, by
hand, never in CI) runs one whole task: the agent fixes a failing test in a throwaway crate in its own separate copy,
checks its work with the `verify` tool and commits on its `faber/…` branch while the person's checkout stays untouched,
and Faber's own checks under the reply end `passed`. A second scenario has the person confirm a check the agent cannot
see (`cargo fmt --check`): Faber's check fails on the reply, Faber sends the fix turn by itself and the next ends
`passed`. On 2026-10-17 both passed on Haiku (26 s and 44 s, about US$0.01 each), and with `--review` the helper from the
other AI sent its report back.

Partly built (R190): the engine's `verify` tool runs a project's declared checks on the PC, summarizes failures and
keeps the full output, with no extra process while idle. Faber's landing workflow also checks its exact queued tree
locally and posts that coverage to GitHub. The wider evidence-bound review and completion contract remains planned;
the existing checks tool does not enforce it ([current inventory](areas.md#toolbox),
[spec](../superpowers/specs/2026-10-17-built-in-verifier.md)).

## Coherence of a project's work (R146)

A mission keeps a project as coherent as the agents that build Faber keep Faber (R145). Planned, not built:

- **A project map built from the code (built for the project's agent).** Extractors list what the project has
  (settings, commands, texts, browser storage keys, screen files, package scripts, exports) and stay tied to the files
  they came from; no AI-written overview. The agent gets the index through Faber's `map` tool and looks details up.
  Routes and other languages come with the first project that needs them.
- **A contract before writing.** The Lead names what the change touches, the closest thing that already exists, and
  `Extends:` it or `New:` with why it does not fit; the Writer starts only once the contract exists.
- **Faber decides "done".** After the Writer's turn Faber runs the project's checks and the map check itself; a Writer
  cannot declare its own work done.
- **Short check output (R155; reading built for Cargo, tsc and oxlint, pytest, jest or vitest, eslint and go test
  next).** The roles read only what failed: each failure with its file and line, the tool's
  suggested fix when it has one, measurements and a one-line summary. The whole output stays in the mission's log, and
  a format Faber does not know falls back to its last lines. Faber's own runner cut a passing run from about 34k tokens
  to about 50 this way (R154).
- **Lessons need the owner's yes.** A mission's retrospective proposes rule or card changes with their evidence, and
  each becomes a rule only after the owner's yes.
- **Sized by risk.** A small change in one area gets the Writer and the checks; a change across areas gets the whole
  chain.
- **The reviewer** follows guardrail 5: the other vendor when it can, a fresh read-only session otherwise. It reviews
  by the rules of [product-rules.md](../product-rules.md) (R147): an empty review is a pass, only a break for a person, for
  data, for security or for a written rule blocks, a fix needs a failing test or a repro, and each round gets the
  findings already decided so the loop ends.
