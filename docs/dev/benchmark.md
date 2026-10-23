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
| `--vendors claude[,codex]` | same as `--plain` | The CLIs Faber may use (Auto picks among them) |
| `--cap-usd`, `--cap-tokens`, `--per-run-usd` | 8, 3000000, 1.5 | The run's cap in API-equivalent dollars and first-time tokens, and what one side of one request may spend |
| `--plain-model`, `--plain-effort` | claude-sonnet-5-5 medium | The plain CLI's model, the everyday default Faber itself starts on |
| `--judge-model`, `--judge-effort` | strongest measured profile of the selected vendor | Optional overrides for the fresh judge; default model and effort come from `route::PROFILES`, ordered by intelligence then agent score |
| `--account-data` | `FABER_DATA_DIR`, otherwise unknown | Read-only Faber data folder supplying `budget.json`; no default installed-data guess |
| `--dir`, `--run-id` | the system temp folder's `faber-bench`, the time | Where the run folder `<dir>/<run-id>/` goes |

## What a run does

For each request in `bench/requests/` (a header and the prompt exactly as a person would type it):

1. Builds the starting folder (`bench/fixtures/<name>.mjs`, or a README) under the run folder, never in the repository or in the person's projects.
2. Runs the plain CLI in its own copy (`claude -p` or `codex exec`, the person's own sign-in and binary, no shell, no window, Full access like Faber's), and Faber through the engine (`Engine::send` with Auto, in Chat or Projects, a separate copy for a project) with the same settings the app gives it.
3. Runs the request's proof (`bench/proofs/<name>.mjs`, a Node script that prints one JSON line) on each result.
4. Selects the installed CLI of the other vendor than plain; if only the plain vendor is installed, its strongest measured profile judges in a fresh session and the report labels the weaker coverage. Has the judge read both results blind: the final reply, the files and the proof's result as "A" and "B", the sides alternating from one request to the next and the first chosen by the run id, with the app's name scrubbed from the text. The answer is revealed after the verdict.
5. Writes `results.json` (everything, raw) and `tables.md` into the run folder, and each Faber CLI start's arguments to `<request>/faber/spawns.log`.

Measured per request and side: the verdict and its reason, the proof, the reply's own time (the plain run, or Faber's reply `took`; wall time is in `results.json`), and reported tokens and API-equivalent cost. Claude supplies `total_cost_usd`. Plain Codex tokens use the existing model pricing when one completed turn has every token counter (including explicit cache writes) and its total input fits the short-context tier; unknown models, missing counters/turns and larger totals have unknown cost because the CLI events do not identify each request's context tier. Faber's cost is the difference between its private engine ledger before setup and after shutdown, including routing, helpers and other auxiliary work. Any new unmetered ledger entry makes that cost unknown. Missing costs stay unknown in rows and totals and suppress cost ratios. The current upstream [exec event schema](https://github.com/openai/codex/blob/main/codex-rs/exec/src/exec_events.rs) includes `cache_write_input_tokens`; older events missing it remain incomplete, as in the engine adapter.

Tokens count input, output and cache writes as first-time tokens; cache reads are listed apart. Faber's token counts are explicitly partial: the ledger exposes auxiliary costs but not their token counts. Its token totals and the whole-run token count are lower bounds, so the token cap cannot be guaranteed for auxiliary work.

The dollar and token budgets are checked before each request, side and judge started by the harness. Unknown cost closes admission to further paid work; it never means free work or unlimited dollars. Faber also checks the ledger after setup before sending, then every 500 ms and interrupts its conversation when the allowance is spent or cost becomes unknown. Plain Claude and a Claude judge additionally receive `--max-budget-usd`. A Codex judge is accounted through the same complete-counter/model pricing as plain Codex and has a ten minute invocation backstop. Plain Codex is read only after its current invocation ends, with the 20 minute run limit as a backstop. These are admission limits, not absolute spend ceilings: current invocations, internal requests and auxiliary calls already in flight may overshoot, with no guaranteed dollar bound. No new harness side or judge starts after the cap or an unknown cost is observed.

Claude judges disable tools and settings sources. Codex judges use an ephemeral read-only session in the empty judge folder and ignore user configuration while retaining the CLI sign-in. Shell, web search, hooks and app/browser integrations are disabled. An empty MCP override alone does not clear inherited tables; ignoring user configuration excludes personal MCP servers, but system or managed servers are not independently excluded or tested. Read-only is not a guarantee of no built-in read tools; the prompt asks only for a verdict from the supplied blind text. The selected vendor, model, effort and weaker fallback coverage are saved in `results.json` and `tables.md`.

Before plain, Faber and judging, the benchmark waits for positive quiet-account evidence from the app's saved `budget.json`. Point `--account-data` (or `FABER_DATA_DIR`) at the running Faber app's data folder. It reads at most 1 MiB every 30 seconds, never opens that folder as an engine Store, never writes/quarantines it and launches no quota checker. The installed app's existing watcher supplies updates. The selected source is recorded in the results. No source, bad JSON, missing windows/no-model samples, expired resets, future timestamps or stale observations mean Unknown and block new paid work.

For every requested vendor/window, recovery requires two distinct no-model readings of identical use and reset, separated by the existing vendor lag (Claude 10 minutes, Codex 5). Both must be no older than the existing 15 minute watcher cadence plus that lag. Any movement or reset starts the pair over; an unchanged old snapshot and elapsed time alone never release it. Active requests in the existing mission budget block admission, as does an unreadable budget record. Samples and both sides of a pair must belong to the current plan/learning epoch and postdate its start plus lag, and the end plus lag of saved own spans, pending work and folded work. Previously observed timestamps/epochs are retained as high-water marks across invalid/stale files, so rolling back a file cannot erase known movement. The retained map is capped at 128 vendor/window keys; extra keys leave admission unknown. Each completed benchmark invocation invalidates old pairs and requires new post-completion evidence before another paid stage, conservatively even across vendors. The aggregate Outside analysis is not used. These saved snapshots cannot prove that another session has not started since their last reading, or expose work from an app version that does not persist active requests; this is a conservative admission pause, not account locking.

Waiting stays at the current stage in the same process: a completed side is saved before waiting for the next and is not run again when evidence recovers. Create an empty `cancel` file inside the run folder to abort the wait; completed results and the unstarted stage remain in the report and no later call starts. Ctrl+C ends the process. There is no restart/resume system: starting the command again starts a new run, so do not reuse a run id expecting a checkpoint replay. Running paid work finishes under the existing budget rules; the pause gates only subsequent stages. Keep going is unchanged. No distributed-app memory or CPU cost is added; benchmark waiting keeps one bounded ledger and a small sample map, and performs one bounded file read per 30 seconds (not measured on a live account).

## What it does not do yet

- Codex: the harness runs plain Codex (`--plain codex`) and lets Faber use it (`--vendors claude,codex`), but its event reader was written from the documented format and has not met a live account.
- Judge availability means an installed CLI on PATH, not a verified sign-in or spare quota. A failed judge is reported without retrying another vendor or starting an unbudgeted call. Independence is from the plain vendor; Faber Auto can itself use both vendors.
- One judging pass per request, without the swapped second pass that removes the leaning toward the first answer; the alternating deal spreads it.
- Routing and auxiliary tokens are unavailable in the public ledger; reports mark Faber token counts as partial. Their ledger costs are counted.
- The plain side is unguarded: it runs with bypassPermissions (or danger-full-access) and without the secret-path deny list Faber passes in its settings, and it loads this PC's hooks, instructions and MCP servers. The five prompts are benign.
- `spawns.log` writes each argument, except the values of `--settings`, `--mcp-config` and the system-prompt flags, which are logged as their length.
- The proofs run the agent's code on this PC, as the agent itself already did.
- The full ~40 requests, creative media (thumbnail, video, 3D, mod) and the goals that are not quality (speed to first token, precision, memory) are measured elsewhere or not yet.

A dated report of a run lives in [research/](../research/) (the first: [2026-10-22/benchmark-light.md](../research/2026-10-22/benchmark-light.md)). Its pure parts (request files, the deal, the verdict, what a CLI reported, the cap, the tables) are `bench/pure.rs`, tested by `engine/tests/all.rs`; the runner is `engine/examples/bench.rs`.
