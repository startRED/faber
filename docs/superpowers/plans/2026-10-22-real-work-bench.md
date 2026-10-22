# Real-work benchmark (Task 7 of the test rework)

Status: in progress, 2026-10-22. Step 1 is done (how TL-ADE is fed); nothing is built yet.

This is the plan for the real-work tier of the fair benchmark in R113: the spec's section 3.1
([spec](../specs/2026-10-14-test-system-rework-design.md)) and Task 7 of
[the test rework plan](2026-10-14-test-system-rework.md). Each app does the same 7 tasks through its own screen. The
CLI answers are recorded, so no quota is spent and both sides get the same answers at the same speed. Every run happens
inside the meter (`faber-bench`, docs/dev/testing.md item 6). What is measured is the app's own cost around the CLI:
wall time to the first shown event and to the end, plus processes, CPU, peak commit and writes.

TL-ADE's code is read only to learn how to drive it; none of it is copied.

## How each app is driven

- **Faber.** The screen suite already starts the hidden test build with a fake CLI replaying `tests/fixtures/` and
  drives it over DevTools (`testkit/src/screen/`). The real-work tasks become scenarios of their own. They run inside
  the meter instead of the suite's own process meter, one task per run, with a fresh data folder.
- **TL-ADE.**
  - Start it with `node bin/ade.js serve --repo <temp copy> --port <n> --no-open`. Its panel is a plain web page on
    127.0.0.1, so headless Chromium on the hidden desktop loads it with `--remote-debugging-port`. The page's session
    cookie comes with the first load.
  - Set `ADE_HOME` to a temporary folder, so the owner's `~/.ade` is never touched. Point `--repo` at a temporary copy
    of the sample project.
  - Its fake CLI (`src/adapters/fake/cli.ts`, `ADE_FAKE_CLI=1` with `ADE_FAKE_SCENARIO`) is honoured only by `ade run`,
    which runs missions.
  - The panel's chat and quota reads call the real CLIs, so step 1 below finds the hook before any number is taken.

## What each app can do

| # | Task | Faber | TL-ADE |
|---|---|---|---|
| 1 | Open the app and sit idle on a new conversation for 60 s | yes | yes, a project opened in the panel (its quota polling runs meanwhile) |
| 2 | One chat turn that streams an answer | yes | partly: a turn-based chat that answers through its state and WebSocket, with no token stream |
| 3 | Close and reopen, then continue the same session | yes | yes, with the same `--repo` and `ADE_HOME` |
| 4 | A 50-turn conversation, then scroll its whole thread | yes | yes, seeded; one chat per project |
| 5 | Interrupt a running turn and send the next one | yes | no for chat (no cancel); a mission can be paused and stopped, which is a different task |
| 6 | Show the quota of each vendor | yes | yes, its Models page (from a quota receipt written in `ADE_HOME`) |
| 7 | Run the proof of a small sample project and report it | yes (the checks of R203) | partly: proof runs only inside a mission (`ade run`) |

A task one side cannot do is listed, not measured (spec, section 3.1): 5 for TL-ADE. Tasks 2 and 7 are measured with
their difference written beside the numbers.

## Steps

- [x] **1. Find TL-ADE's chat and quota hooks** (2026-10-22).
  - Its chat runs `claude -p --output-format text` in a worktree copy, prompt on stdin, with no session kept; codex and
    agy take its place by family.
  - The binary is a plain PATH lookup (`.exe`, `.cmd` shim or bare name), and `serve` from the command line takes no
    fake: only its in-process `deps.chatAgent` does, which its own tests use.
  - So the run puts a stand-in `claude.exe` first on a temporary PATH. It reads stdin to the end, prints the recorded
    answer as plain text and exits 0, and TL-ADE is never changed.
  - A text answer is one block, so task 2 compares Faber's stream with a whole reply, and the table says so.
  - Quota: `serve` always polls every 3 minutes and spawns the real CLIs. With the stand-in on PATH the poll fails
    quietly (logged only). Task 6 reads a `quota-<family>.json` written in the temporary `ADE_HOME` (`source:
    "official"`, its family, `observed_at` under 24 h old, `weekly_reset_at` ahead, `used_percent`).
- [ ] **2. Record the sessions.** One short answer and one 50-turn thread, in Faber's fixture format and in TL-ADE's
  scenario format, with the same text and the same delays.
- [ ] **3. Faber's scenarios.** Tasks 1 to 7 as screen scenarios run alone (`cargo t --suite screen --filter real-work`),
  each one a single meter run, with times to the first event and to the end read from the page.
- [ ] **4. TL-ADE's driver.** One script per task (Node, DevTools over the debugging port), started by the meter with
  `serve` and the headless browser inside the same job.
- [ ] **5. `run.mjs --only real-work`.** It adds the 7 pairs with a warm-up and three runs each, written with the suites
  to `docs/research/bench/`.
- [ ] **6. Report.** The table goes into `docs/research/bench/README.md` against the floor of R113, with the tasks one
  side cannot do listed.
