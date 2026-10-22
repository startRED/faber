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
