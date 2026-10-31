# Board

Built by `node scripts/tasks.mjs board` from the task list and git after every claim, release and landing; nobody
edits it. Updated 2026-10-31 22:01 UTC.

**27 doing · 23 landing · 64 next · 31 waiting**

## Doing

- **A base that moved only by words lands a green queued commit without checking it again** (`a-base-moved-by-words-only-lands-without-a-new-check`), team-2, for 4 h
- **Browser pane checks that follow a window resize stop acting before the page has settled** (`browser-pane-checks-wait-for-the-resize-to-settle`), equipe-7, for 4 h
- **Recover provably orphaned nested build reservations without reclaiming unknown owners** (`recover-orphaned-nested-build-reservations`), codex-t3-recovery, for 4 h
- **A visual answer's frame sometimes stays on the first page after a version switch** (`visual-answers-frame-stays-on-the-first-page`), equipe-7, for 4 h
- **A green landing is never dropped while it waits for another** (`a-green-landing-is-never-dropped`), Erick 6720, for 21 h
- **Antigravity driver: a third vendor** (`antigravity-driver`), Erick 31676, for 21 h
- **Antigravity in the app: setup, picker, Auto, quota, docs** (`antigravity-in-the-app`), Erick 29488, for 21 h
- **Codex starts the team it plans for a big request** (`codex-starts-the-team-it-plans`), Erick 32664, for 21 h
- **Thread list: needs you first, calm status** (`needs-you-first`), Erick 16920, for 21 h
- **Readable turns: fold, group sentences, docked questions** (`readable-turns`), Erick 41436, for 20 h
- and 17 more

## Landing

- chore(scripts): a words-only base move lands a green batch, and the queue batches the ones a landing sends back ([#482](https://github.com/startRED/faber/pull/482)), queued 24 h ago
- fix: Faber opens in 8 s instead of 40 s with a long undo history, and no longer crashes at start (R481), queued 24 h ago
- perf(engine): conversation files are written as compact JSON ([#508](https://github.com/startRED/faber/pull/508)), queued 25 h ago
- fix(engine): read Claude's limit with one tiny request when /usage prints no percentages ([#514](https://github.com/startRED/faber/pull/514)), queued 26 h ago
- feat: the plan shows which conversation does each step, and opens it ([#467](https://github.com/startRED/faber/pull/467)), queued 26 h ago
- test(engine): the other engine test files wait real time, not counted sleeps ([#505](https://github.com/startRED/faber/pull/505)), queued 26 h ago
- fix(engine): a team leader cut off by closing Faber goes on by itself when no member will wake it ([#499](https://github.com/startRED/faber/pull/499)), queued 26 h ago
- perf(engine): the engine counts an edit's lines and keeps only the first 40 ([#506](https://github.com/startRED/faber/pull/506)), queued 27 h ago
- fix(tooling): free a build cache whose command was killed, instead of holding it for a day ([#441](https://github.com/startRED/faber/pull/441)), queued 27 h ago
- fix(engine): words deleted or rewritten in Memory lose their review ([#496](https://github.com/startRED/faber/pull/496)), queued 27 h ago
- and 13 more

## Next

- **Screen checks that decide on real elapsed time judge the page's own timestamps instead** (`screen-checks-that-read-the-real-clock`), none
- **A landing that must finish on the PC is a wait with the right next step, not a red round** (`landing-a-workflow-change-is-a-wait`), none
- **The PC runs Clippy and the format check before it queues, so a lint error never costs a queue round** (`land-runs-the-cheap-lints-before-queueing`), none
- **Engine test polls wait on the shared deadline: engine and keep going** (`engine-test-polls-in-engine-and-keep-going`), none, claim by team-5 gone stale
- **Engine test polls wait on the shared deadline: copies, goals and verify** (`engine-test-polls-in-copies-goals-and-verify`), none, claim by team-5 gone stale
- **Engine test polls wait on the shared deadline: the other files** (`engine-test-polls-in-the-other-files`), none, claim by team-3 gone stale
- **Claude's limit is read even when `/usage` prints no percentages, if that is still needed** (`claude-limit-when-usage-prints-no-percentages`), Account limits, claim by team-6 gone stale
- **An answer being typed in a question card survives a refresh of the running list** (`typed-answer-survives-a-list-refresh`), Running, claim by team-5 gone stale
- **Deleting the last copy of a kept project note revokes its approval** (`deleting-a-kept-note-revokes-its-approval`), Project memory, claim by team-5 gone stale
- **A CLI is created inside its job, so no end of Faber leaves one behind** (`a-cli-is-created-inside-its-job`), none, claim by team-6 gone stale
- and 54 more

## Waiting on other tasks

- **A commit landed on a base moved by words counts as green for the next run's scope** (`a-words-only-landing-counts-as-green`) waits on `a-base-moved-by-words-only-lands-without-a-new-check`
- **A stuck CI test job ends after a time limit instead of holding a runner** (`ci-jobs-end-when-stuck`) waits on `landing-a-workflow-change-is-a-wait`
- **A person can publish their project's progress board to their own GitHub** (`project-board-on-github`) waits on `project-progress-line`, `follow-ups-as-task-files`
- **A helper or reviewer comes from whichever other AI has the most room** (`helper-picks-among-others`) waits on `gemini-vendor-variant`
- **Read an Antigravity run's output into Faber's reply events** (`agy-reply-parser`) waits on `gemini-vendor-variant`
- **An Antigravity conversation runs a turn, stops, and resumes** (`agy-turn-session`) waits on `antigravity-driver`, `agy-sign-in-and-setup`, `agy-reply-parser`
- **Connecting Gemini: Faber finds Antigravity's CLI and shows whether it is signed in** (`agy-sign-in-and-setup`) waits on `gemini-vendor-variant`
- **Faber installs Antigravity's CLI for Gemini, the way it installs the other two** (`agy-install`) waits on `agy-sign-in-and-setup`
- **Faber reads Gemini's limits the way it reads the other two, or says it cannot** (`agy-quota-reading`) waits on `gemini-vendor-variant`
- **Auto and the team can pick Gemini models for project work** (`gemini-profiles-in-routing`) waits on `agy-turn-session`, `agy-quota-reading`
- and 21 more

## Landed

- 2026-10-31: docs(decisions): record the rework choices on voice, done and the Faber CLI
- 2026-10-31: feat(engine): ready plan steps that share no file start together, each from the leader's latest work (R487) ([#523](https://github.com/startRED/faber/pull/523))
- 2026-10-31: docs(tasks): drop task 009, done by #471 ([#531](https://github.com/startRED/faber/pull/531))
- 2026-10-31: chore(scripts): a base moved by words only lands a green batch without a new run ([#480](https://github.com/startRED/faber/pull/480))
- 2026-10-31: docs(tasks): eight stability and recovery tasks for the queue ([#530](https://github.com/startRED/faber/pull/530))
- 2026-10-31: docs(tasks): the build reservation recovery comes first in the queue (task 004)
- 2026-10-31: docs(tasks): task 220 weighs PR 441 instead of waiting for it to land
- 2026-10-31: docs(tasks): task 221 makes the storage sweep run at most once an hour on Windows
- 2026-10-31: docs(tasks): task 220 keeps cache identity, reservation and maintenance apart
- 2026-10-31: docs: the README says how to install Faber, and two living pages stop claiming what is not true ([#529](https://github.com/startRED/faber/pull/529))
