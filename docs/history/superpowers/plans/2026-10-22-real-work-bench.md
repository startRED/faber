# Real-work benchmark (Task 7 of the test rework)

Status: in progress, 2026-10-22. Steps 1 to 4 are done: Faber passes its 7 tasks and TL-ADE's driver its 5, each run
once. Steps 5 and 6 (the measured runs and the report) are left.

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
- [x] **2. Record the sessions.** One short answer and one 50-turn thread, in Faber's fixture format and in TL-ADE's
  scenario format, with the same text and the same delays.
- [x] **3. Faber's scenarios.** Tasks 1 to 7 as screen scenarios run alone, each one a single meter run, with times to
  the first event and to the end read from the page.
  - The tasks are `testkit/src/screen/real_work.rs` ("real work N: …"). They join the suite only when `--filter`
    names "real work", since task 1 sits idle for a minute, and they always run alone. They replay the recordings the
    suite has (`chat.jsonl`, `resume.jsonl`, `interrupt.jsonl`), so step 2 needs only the 50-turn thread, which task 4
    writes as a saved conversation the way the thousand-message scenario does.
  - Each notes its times and the app's memory (`open 1.20 s, app 180 MB`), and `run.mjs` reads them back from the
    runner's log into `<date>.notes.jsonl`.
  - Task 6 needs a turn on Faber's side: the CLI reports its limits with each answer, while TL-ADE reads a file. So
    the two are different work under different names (`turn_and_limit_shown_s` against `limit_shown_s`), and the
    report says so beside the numbers.
  - Task 7 replays `real-work-proof.jsonl`, written by hand: a project's answer in full access writes a file, so Faber's
    checks (R204) run on the sample project. The fake CLI writes nothing, so the recording holds after its Write step
    while the scenario writes the file into the work copy, and the checks show in the details beside the thread. Its
    proof is `git diff --check`, since the sandbox's PATH holds git and no
    Node; TL-ADE's mission runs the same command.
- [x] **4. TL-ADE's driver.** One script per task (Node, DevTools over the debugging port), started by the meter with
  `serve` and the headless browser inside the same job.
  - It is `scripts/bench/tlade.mjs --task N`. It makes a temporary repo (one commit), home and PATH. The PATH holds the
    stand-in `faber-standin` copied as `claude.exe`, with `reply.txt` beside it, plus Node and git. It refuses to run
    if either folder holds a program named after an AI's CLI.
  - It starts `serve` and headless Chrome, and types into the panel's chat box (`Pergunta ao chat`). It counts the
    answers (`.chat-turn.assistant`), polling every 50 ms.
  - Task 3 stops both and starts them again on the same repo and home. Task 4 seeds `<repo>/.ade/chat/repo.json` with
    the same 50 turns. Task 6 writes the quota receipt and opens Opções, then Modelos.
  - The panel's triage call and its quota poll run the stand-in too. That is its own cost, and they stay in the
    numbers.
- [ ] **5. `run.mjs --only real-work`.** It adds the 7 pairs with a warm-up and three runs each, written with the suites
  to `docs/research/bench/`.
- [ ] **6. Report.** The table goes into `docs/research/bench/README.md` against the floor of R113, with the tasks one
  side cannot do listed.
