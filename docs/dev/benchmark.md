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
| `--dir`, `--run-id` | the system temp folder's `faber-bench`, the time | Where the run folder `<dir>/<run-id>/` goes |

## What a run does

For each request in `bench/requests/` (a header and the prompt exactly as a person would type it):

1. Builds the starting folder (`bench/fixtures/<name>.mjs`, or a README) under the run folder, never in the repository or in the person's projects.
2. Runs the plain CLI in its own copy (`claude -p` or `codex exec`, the person's own sign-in and binary, no shell, no window, Full access like Faber's), and Faber through the engine (`Engine::send` with Auto, in Chat or Projects, a separate copy for a project) with the same settings the app gives it.
3. Runs the request's proof (`bench/proofs/<name>.mjs`, a Node script that prints one JSON line) on each result. A non-zero or missing process exit code fails the proof even if its output claims success.
4. After both answers, selects an installed judge vendor that did not author Faber’s recorded replies, breaking ties with independence from plain. Each row records its judge model/effort and independence from each side separately. Missing author metadata or untracked helper/conversation delegation makes Faber independence unknown (reported as false), never presumed. With both vendors contributing or only one installed, the report labels weaker evidence and excludes it from the cross-vendor quality totals. Explicit judge model or effort overrides pin the original default vendor (other than plain when installed), so an override is never sent to the wrong CLI. Has the judge read both results blind: the final reply, the files and the proof's result as "A" and "B", the sides alternating from one request to the next and the first chosen by the run id, with the app's name scrubbed from the text. The answer is revealed after the verdict.
5. Writes `results.json` (everything, raw) and `tables.md` into the run folder, and each Faber CLI start's arguments to `<request>/faber/spawns.log`.

The folder-organizing fixture includes an existing backup with the same content as another file. Its proof preserves the original count of each file's content: losing one of those originals fails, keeping both passes, and adding another copy fails. Older runs without that backup are not matched baselines for this updated fixture.

Measured per request and side: the verdict and its reason, the proof, the reply's own time (the plain run, or Faber's reply `took`; wall time is in `results.json`), and reported tokens and API-equivalent cost. Claude supplies `total_cost_usd`. Plain Codex tokens use the existing model pricing when one completed turn has every token counter (including explicit cache writes) and its total input fits the short-context tier; unknown models, missing counters/turns and larger totals have unknown cost because the CLI events do not identify each request's context tier. Faber's cost is the difference between its private engine ledger before setup and after shutdown, including routing, helpers and other auxiliary work. Any new unmetered ledger entry makes that cost unknown. Missing costs stay unknown in rows and totals and suppress cost ratios. The current upstream [exec event schema](https://github.com/openai/codex/blob/main/codex-rs/exec/src/exec_events.rs) includes `cache_write_input_tokens`; older events missing it remain incomplete, as in the engine adapter.

Tokens count input, output and cache writes as first-time tokens; cache reads are listed apart. Faber's token counts are explicitly partial: the ledger exposes auxiliary costs but not their token counts. Its token totals and the whole-run token count are lower bounds, so the token cap cannot be guaranteed for auxiliary work.

The dollar and token budgets are checked before each request, side and judge started by the harness. Unknown cost closes admission to further paid work; it never means free work or unlimited dollars. Faber also checks the ledger after setup before sending, then every 500 ms and interrupts its conversation when the allowance is spent or cost becomes unknown. Plain Claude and a Claude judge additionally receive `--max-budget-usd`. A Codex judge is accounted through the same complete-counter/model pricing as plain Codex and has a ten minute invocation backstop. Plain Codex is read only after its current invocation ends, with the 20 minute run limit as a backstop. These are admission limits, not absolute spend ceilings: current invocations, internal requests and auxiliary calls already in flight may overshoot, with no guaranteed dollar bound. No new harness side or judge starts after the cap or an unknown cost is observed.

Claude judges disable tools and settings sources. Codex judges use an ephemeral read-only session in the empty judge folder and ignore user configuration while retaining the CLI sign-in. Shell, web search, hooks and app/browser integrations are disabled. An empty MCP override alone does not clear inherited tables; ignoring user configuration excludes personal MCP servers, but system or managed servers are not independently excluded or tested. Read-only is not a guarantee of no built-in read tools; the prompt asks only for a verdict from the supplied blind text. The selected vendor, model, effort and weaker fallback coverage are saved in `results.json` and `tables.md`.

## Reading quality totals

Cross-vendor verdict totals describe independent opinions, not completed qualification. The separate Projects total credits a Faber win or tie only with an independent verdict, a successful completed Faber side (no error or cap stop), and a passing executable proof. Every recorded Project row stays in its denominator, including failed and skipped work; Chat rows stay out. A failed plain-side proof does not erase a proven Faber result. The target is 9/10, but this count alone does not qualify a version: unrecorded requests, the full first-public set and Chat's source accuracy require their own evidence.

Actual spending remains visible for failed or skipped work. The cost percentage appears only when every request with a recorded side has both sides completed successfully, without errors or cap stops, and both costs are known. Requests with no recorded sides do not affect that comparison. This compares the cost of completed work; it does not qualify the quality or approved-task cost targets.

## Pausing for other account use

Before each plain side, Faber side and judge, the harness requires two new readings with the same vendor-wide windows, reset times and usage. They must be at least the existing status-reading gap apart (five minutes for Claude, one for Codex). Missing, invalid, stale or changed readings keep admission closed. After its own work, it first waits the engine's quota-reporting lag (ten minutes for Claude, five for Codex), then establishes a new baseline. It reads only the vendors that the next stage can use; all must have a fresh quiet interval together. Status commands use the existing local `/usage` and rate-limit reader, without asking a model. If `/usage` unexpectedly reports model work, spending becomes unknown and the run ends without retrying it.

This is conservative evidence of no **observed** account movement between stages, not attribution or a guarantee that another person is idle. Rounded or delayed counters can hide outside work; overlapping work cannot be separated reliably. The historical outside-use heuristic alone cannot establish fresh quiet, and a private Faber ledger does not include the plain side or judge. The harness therefore uses fresh run-local baselines instead of treating its own earlier calls as outside use. Long waits are intentional; nothing is interrupted or resumed inside a paid invocation. Keep going is unchanged.

Each run needs a new run folder: an existing `--run-id` is rejected before saving anything, so earlier results cannot be overwritten. Completed sides, proofs and known spending are saved before the next admission. To cancel during a pause, create an empty file named `CANCEL` in the run folder printed by the harness. Cancellation starts no new paid work and preserves those checkpoints; an in-flight status read may take up to 45 seconds to finish. It does not resume a cancelled run or recover an invocation interrupted before its result was collected. The quiet policy and cancellation/checkpoint paths have fake-clock or local checks; real-account behavior remains unverified until a manual qualification run.

## What it does not do yet

- Codex: the harness runs plain Codex (`--plain codex`) and lets Faber use it (`--vendors claude,codex`), but its event reader was written from the documented format and has not met a live account.
- Judge availability means an installed CLI on PATH, not a verified sign-in or spare quota. A failed judge is reported without retrying another vendor or starting an unbudgeted call. Independence describes recorded answer vendors, not external services invoked by arbitrary commands or user-installed MCP tools. Native subagents use their CLI vendor; Faber helper/conversation calls lack persisted contributor vendors, so such rows are conservatively unqualified. All verdicts remain visible, with separate qualified totals.
- One judging pass per request, without the swapped second pass that removes the leaning toward the first answer; the alternating deal spreads it.
- Routing and auxiliary tokens are unavailable in the public ledger; reports mark Faber token counts as partial. Their ledger costs are counted.
- The plain side is unguarded: it runs with bypassPermissions (or danger-full-access) and without the secret-path deny list Faber passes in its settings, and it loads this PC's hooks, instructions and MCP servers. The five prompts are benign.
- `spawns.log` writes each argument, except the values of `--settings`, `--mcp-config` and the system-prompt flags, which are logged as their length.
- The proofs run the agent's code on this PC, as the agent itself already did.
- The research proof checks at most 20 distinct cited links; extra links make coverage incomplete and the proof fails. Reachability (including 401, 403 and 429) does not verify the cited facts, official authorship or page contents. A passing link check alone does not qualify research accuracy.
- The full ~40 requests, creative media (thumbnail, video, 3D, mod) and the goals that are not quality (speed to first token, precision, memory) are measured elsewhere or not yet.

A dated report of a run lives in [research/](../research/) (the first: [2026-10-22/benchmark-light.md](../research/2026-10-22/benchmark-light.md)). Its pure parts (request files, the deal, the verdict, what a CLI reported, the cap, the tables) are `bench/pure.rs`, tested by `engine/tests/all.rs`; the runner is `engine/examples/bench.rs`.
