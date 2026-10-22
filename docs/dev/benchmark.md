# Benchmark

The light benchmark: how to run it, what it measures and what it leaves out. The plan it follows is "Goals" in [product/README.md](../product/README.md#goals).

## Run it

One command, from the repository root, with cargo on the PATH (see AGENTS.md for this PC). It spends real quota on the person's own subscription, so it is never part of `cargo t` or CI:

```bash
cargo run -p faber-engine --example bench
```

Start it by hand at below-normal priority so a game in front keeps the processor (the runner does not lower its own; the programs it starts inherit the class it has, and the proofs start below normal). `--dry-run` prints the plan and starts nothing.

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--only id,id` | all | The requests to run (the `id` of a file in `bench/requests/`) |
| `--plain claude\|codex` | claude | The plain CLI Faber is measured against; run twice for both |
| `--vendors claude[,codex]` | claude | The CLIs Faber may use (Auto picks among them) |
| `--cap-usd`, `--cap-tokens`, `--per-run-usd` | 8, 3000000, 1.5 | The run's cap in API-equivalent dollars and first-time tokens, and what one side of one request may spend |
| `--plain-model`, `--plain-effort` | claude-sonnet-5-5 medium | The plain CLI's model, the everyday default Faber itself starts on |
| `--judge-model`, `--judge-effort` | claude-opus-5-5 high | The judge: always a fresh Claude Code session with no tools |
| `--dir`, `--run-id` | the system temp folder's `faber-bench`, the time | Where the run folder `<dir>/<run-id>/` goes |

## What a run does

For each request in `bench/requests/` (a header and the prompt exactly as a person would type it):

1. Builds the starting folder (`bench/fixtures/<name>.mjs`, or a README) under the run folder, never in the repository or in the person's projects.
2. Runs the plain CLI in its own copy (`claude -p` or `codex exec`, the person's own sign-in and binary, no shell, no window, Full access like Faber's), and Faber through the engine (`Engine::send` with Auto, in Chat or Projects, a separate copy for a project) with the same settings the app gives it.
3. Runs the request's proof (`bench/proofs/<name>.mjs`, a Node script that prints one JSON line) on each result.
4. Has the judge read both results blind: the final reply, the files and the proof's result as "A" and "B", the sides alternating from one request to the next and the first chosen by the run id, with the app's name scrubbed from the text. The answer is revealed after the verdict.
5. Writes `results.json` (everything, raw) and `tables.md` into the run folder, and each Faber CLI start's arguments to `<request>/faber/spawns.log`.

Measured per request and side: the verdict and its reason, the proof, the reply's own time (the plain run, or Faber's reply `took`; the wall time with the engine's start is in `results.json`), and the tokens and the API-equivalent cost the CLI itself reports (Claude's `total_cost_usd` and `usage`; Faber's replies carry the same numbers). Tokens count input, output and cache writes as first-time tokens; cache reads are listed apart. The run stops starting work when the cap is reached. Inside a side the cap is weaker: plain Claude is stopped by `--max-budget-usd`, but Faber's cost is written only when a turn ends, so one long reply is never interrupted (a side may overshoot by one turn, and a project's fix loop runs until the 20 minute run limit, the backstop), and Codex reports no cost, so `--cap-usd` and `--per-run-usd` do nothing for it; Codex counts only by tokens, read after each side.

## What it does not do yet

- Codex: the harness runs plain Codex (`--plain codex`) and lets Faber use it (`--vendors claude,codex`), but its event reader was written from the documented format and has not met a live account.
- With one vendor available the judge is that vendor's strongest model in a fresh session (models.md allows it) and the report says the review is weaker; the other vendor judges once it can be used.
- One judging pass per request, without the swapped second pass that removes the leaning toward the first answer; the alternating deal spreads it.
- The router's own small model (Haiku) is not counted in Faber's tokens; it costs a few cents a run.
- The plain side is unguarded: it runs with bypassPermissions (or danger-full-access) and without the secret-path deny list Faber passes in its settings, and it loads this PC's hooks, instructions and MCP servers. The five prompts are benign.
- `spawns.log` writes each argument, except the values of `--settings`, `--mcp-config` and the system-prompt flags, which are logged as their length.
- The proofs run the agent's code on this PC, as the agent itself already did.
- The full ~40 requests, creative media (thumbnail, video, 3D, mod) and the goals that are not quality (speed to first token, precision, memory) are measured elsewhere or not yet.

A dated report of a run lives in [research/](../research/) (the first: [2026-10-22/benchmark-light.md](../research/2026-10-22/benchmark-light.md)). Its pure parts (request files, the deal, the verdict, what a CLI reported, the cap, the tables) are `bench/pure.rs`, tested by `engine/tests/all.rs`; the runner is `engine/examples/bench.rs`.

## Card pairs: the token gate of the start card

The approved own-work-method spec ("Cost") declares a gate for the start card, the fenced block of a project's rules that Faber appends to a project agent's system prompt at session start (`rules::card`, `Engine::start_card`). `--card-pairs` runs it. It spends real quota, so it is never part of `cargo t` or CI.

```bash
# Claude Code (pinned to claude-sonnet-5-5 at medium effort unless --model/--effort say otherwise)
cargo run -p faber-engine --example bench -- --card-pairs --vendor claude --run-id card-claude
# Codex (pinned to gpt-6.1-sol at medium)
cargo run -p faber-engine --example bench -- --card-pairs --vendor codex --run-id card-codex
# the plan only, nothing started
cargo run -p faber-engine --example bench -- --card-pairs --vendor claude --dry-run
# three more pairs when the gate says "too close", then the gate over both files
cargo run -p faber-engine --example bench -- --card-pairs --vendor claude --pair-start 3 --run-id card-claude-more
cargo run -p faber-engine --example bench -- --gate <dir>/card-claude/results.json,<dir>/card-claude-more/results.json
```

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `--card-pairs` | off | The mode. Only Faber runs: no plain CLI side, no Auto |
| `--vendor claude\|codex`, `--model`, `--effort low\|medium\|high` | claude, its everyday model, medium | The one CLI of the run and what it is pinned to (Codex: gpt-6.1-sol medium). One CLI per invocation |
| `--conditions cold,warm,resumed` | all three | The session conditions to run and report |
| `--pairs`, `--pair-start` | 3, 0 | How many pairs, and the index of the first (the three more pairs are `--pair-start 3`) |
| `--only`, `--cap-usd`, `--cap-tokens`, `--per-run-usd`, `--judge-model`, `--judge-effort`, `--dir`, `--run-id`, `--dry-run` | as above | As in the first table. The judge is Claude Code for both CLIs |
| `--gate a.json,b.json` | none | Reads the gate from the `runs` of earlier results files and starts nothing |

### What a run does

For each pair (all five requests of pair 1, then pair 2, so a run the cap stops holds whole pairs), each request runs twice, with the card on and with it off (`Engine::set_start_cards`). The arm that goes first alternates by pair (the card first in pairs 1, 3, 5, the other first in 2, 4, 6), so the prompt cache the first arm warms helps each arm equally often.

Every request runs as a **Project** conversation in a new folder prepared as for a project request (its fixture, or a README), the chat requests included, because the card exists only in projects. Each arm of each pair gets a project, a data folder and an engine of its own. Engine, vendor, model and effort are the same in both arms; the only difference is the card (the length of the system-prompt argument in the arm's `spawns.log` shows it).

| Condition | What it does |
| :--- | :--- |
| `cold` | The first conversation of a new engine, data folder and project: there is no native session to resume. The CLI's own prompt cache cannot be emptied and may still hold the previous arm, which is what the alternating order spreads |
| `warm` | Right after `cold`, in the same engine and project, a second new conversation with the same request (its own copy of the project): the prompt cache is warm |
| `resumed` | After `cold` (and `warm`), the engine is closed, which ends the CLI processes, and a new engine starts on the same data folder. The first conversation then gets "Continue: check your work once more and say what you checked.", so the CLI resumes its native session. Only this follow-up turn is counted; its proof reads the folder after it and the reply of the first turn |

The `cold` turn always runs, because the other two follow it, and is recorded only when `cold` is a wanted condition. The usage of a condition is only the replies of its own turn (with the fix loops Faber's checks add).

**Success** is judged as in R410. A request with a proof (`bench/proofs/`) succeeds when the reply ended well and the proof passed. The one without (plan-an-app) goes to a blind acceptance judge, a fresh Claude Code session with no tools that reads that one answer, with no sign of the arm, and says whether the person got what they asked for: a pass or a fail for each run (the A/B judge of the first mode cannot count successes).

**Recorded per run**: input not read from the cache, cache writes, cache reads (cached input), output, agent replies and tool uses, wall time, the cost where the CLI reports one (not Codex), the proof, the judge's answer and the reasons for a failure. The CLIs do not report their model calls through the engine, so "extra model calls" are shown as replies (more than one in a session means Faber's checks sent the agent back) and tool uses. The cap works as in the first mode (`--cap-usd`, `--cap-tokens`, `--per-run-usd`): it ends the run between sessions, and a session in progress is only held to the per-run allowance. Start it at below-normal priority like the first mode; the CLIs and proofs it starts have no window.

### The gate arithmetic

Declared before any run, in the spec: what is compared are the sums over the five requests, for each CLI and each condition apart. It passes with no fewer successes and at most 5% more input-plus-output tokens with the card. When the difference is within the spread of the paired runs, three more pairs run; if it is still within the spread, the result counts as failing.

- **Tokens** are the first-time tokens of a run: input not read from the cache, plus cache writes (new input the model read and the CLI stored), plus output. **Cached input (cache reads) is reported apart and is not in the gate total.** Cache writes count as input because on Claude a card in the system prompt shows up there in a cold session; leaving them out would hide exactly the card's cost. The consequence to remember: in a warm session the card's tokens sit in cache reads, which the gate does not count, so read the cache-read row too. Codex has no cache writes; its input is what it did not read from the cache.
- A pair's sum is the tokens of all requests of one arm. `D_p` is the card's sum minus the other's for pair `p`. The **spread** is the sample standard deviation of the `D_p`. Only pairs that are complete in every request count.
- `excess = mean(D_p) - 0.05 * mean(sum without the card)`: how far the card is beyond the allowed 5%, per pair.
- **Fail**: fewer successes with the card (no spread is allowed on that), or `excess > spread`. **Pass**: no fewer successes and `excess <= -spread`. **Too close**: in between; a single pair has no spread and is too close. Three more pairs (`--pair-start 3`) are then run, the gate is read over all six (`--gate`), and with more than three pairs a result still in between is **fail**.
- "Within the spread" is read against the 5% line, not against zero: a card that changes nothing has an excess of minus 5% and passes unless the runs are noisier than that. This is the one reading of the spec the code had to choose; its words also allow a stricter one (any difference inside the spread, even a harmless one, fails after the extra pairs). The owner can change it in `card_gate` (`bench/pure.rs`).
- A pass covers this suite only (spec).

`results.json` holds every run and the gate rows; `tables.md` the gate table, the measures apart (each as card, no card, and the difference with its spread) and every run. The pure parts (the arms, the sums, the verdict) are in `bench/pure.rs`, tested by `engine/tests/all.rs`.

### Size

Per CLI, with the defaults: 5 requests x 3 pairs x 2 arms = 30 engine sessions, each with a cold, a warm and a resumed turn, so at least 90 agent turns (more with fix loops) and 18 judge calls (3 pairs x 2 arms x 3 conditions, for the one request with no proof). The three more pairs double it. For both CLIs, run the command once for each.

### Not covered

- Codex has not met a live account in this mode either; its event reader is the one of the first mode.
- The runner does not check that the resumed turn resumed the native session; the arm's `spawns.log` shows the `--resume` (Claude) or `resume` (Codex) argument of the second start.
- The judge is Claude Code even for the Codex runs.
