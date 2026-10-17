# Building software

How Projects builds software: one voice with a team behind it, many workers, self-development and coherence.

## Building software: one voice, a team behind it

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
  an estimate overrun only warns. R88/R90/R91 govern admission, not the estimate.
- A stuck attempt is dropped while what it produced stays. The lead tries another path (another approach, a stronger
  model or one from the other vendor, a clean session) and only then asks the user about that one point, while the rest
  goes on.
- When a quota runs out, the work moves to the other subscription, or waits and resumes by itself at the renewal.

**Decision list.** Every project keeps a decision list with authors. The AI's decisions show as "not reviewed".

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

## Self-development before Full Chat (R96)

A minimal Projects slice comes after the new base and before Phase 2. Target: 2026-10-20 to 22.
It includes a lead, contracts and ready dispatch, Seguro by default, worker worktrees and leases,
a serial proof/review/fast-forward queue, Studio v0 read-only fan-out, daily own-cost grants,
a minimal board and decisions, needs-you notifications (R92), and local next-build promotion/rollback (R93).
Equilibrado adds only own-branch push and draft PR. Parallel writers need disjoint areas and measured qualification.
Catalog, the remaining powers, Livre, and public release automation are deferred.
The local development instance never touches the installed release in E:\Apps\Faber (R95).

Approved, not built (R190): the serial proof queue, `cargo t`'s plan and R146's "Faber decides done" become one
verifier in the engine. It checks each change in seconds on the PC with no extra process while idle, posts the result
to GitHub so CI only confirms, and later serves every project
([spec](../superpowers/specs/2026-10-17-built-in-verifier.md)).

## Coherence of a project's work (R146)

A mission keeps a project as coherent as the agents that build Faber keep Faber (R145). Planned, not built:

- **A project map built from the code.** Extractors list what the project has (settings, routes, commands, texts,
  exports) and stay tied to the files they came from; no AI-written overview. Roles get the index and look details up.
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
