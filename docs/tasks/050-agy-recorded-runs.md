# Record how the Antigravity CLI (Gemini) behaves, so a driver can be built from facts

Area: none
Files: tests/fixtures/agy/, docs/dev/antigravity-cli.md, docs/README.md, testkit/src/fake.rs
Depends: none
Size: about 250 changed lines (mostly recorded fixtures)

Why: the owner's request spreads the work "between Claude, Codex and Gemini (Antigravity)". Faber knows two vendors only:
`Vendor { Claude, Codex }` in `engine/src/model.rs`, a driver each (`engine/src/claude.rs` speaks the CLI's stream-json control
protocol, `engine/src/codex.rs` the app-server), and `docs/product/README.md` lists "Vendors other than Claude Code and Codex: no
Gemini ..." as a non-goal that the owner's decision (`R-next`, in this pull request's `docs/next/leader-mode-tasks.json`) now
amends. Nothing about the Antigravity CLI is recorded in the repo. Checked on this PC (`agy`, in
`%LOCALAPPDATA%\agy\bin`, already signed in): `agy --help` lists `-p/--print`, `--output-format text|json|stream-json`,
`--input-format stream-json` (one NDJSON message per line on stdin, a turn each), `--model`, `--effort low|medium|high|xhigh|max`,
`--mode accept-edits|auto|plan`, `--dangerously-skip-permissions`, `--conversation <id>` / `--continue` to resume, `--add-dir`,
`--settings <json or file>` "for this session only, never saved", `--sandbox`, and subcommands `models`, `mcp`, `plugin`, `update`.
`agy models` lists Gemini 4 Argon, Gemini 3.8 Flash, Claude Opus 5.5 / Sonnet 5.5 and GPT-OSS 120B, each at three efforts. A one-word
turn with `--output-format stream-json` printed `{"event":"init", ...}` (conversation_id, model, cwd, the tool names, permission_mode
`request-review`), `{"event":"step_update", ...}` lines (`step_type` `user_input` / `agent_response`, `state` ACTIVE / DONE, a
`text_delta`, `usage` on DONE) and a final `{"event":"result", ...}` (status, response, duration, num_turns, usage). The CLI loaded
the owner's own global memory and skills into the answer, so Faber's isolation of it is unknown. No sign-in, usage or quota
subcommand exists.

Build, with the real CLI on the owner's subscription (tests under US$25 in total need no question; use the cheapest
`gemini-3.8-flash-low` model, a scratch folder under `%TEMP%`, no window, no build there): record under `tests/fixtures/agy/`
one file per scenario, trimmed of the owner's paths and of anything personal, and write `docs/dev/antigravity-cli.md` (listed in
`docs/README.md`) with what each shows, in short facts:
1. A plain turn, and a turn that reads a file, edits one and runs a command (the `step_type` names, how a tool call and its result
   and its arguments appear, what a changed file looks like).
2. A turn that needs approval, under `--mode accept-edits` and under the default `request-review`: how the request shows in print
   mode, whether stdin can answer it, what happens when nobody does.
3. Two turns in one process with `--input-format stream-json`, how a turn ends, and how to stop one (close stdin, a signal, the
   process tree on Windows; which child processes remain).
4. A resume with `--conversation`, and a conversation id that does not exist.
5. Errors: a wrong model, signed out (use a temporary empty home if the CLI allows; never touch the real sign-in files), and the
   words and exit code of a spent quota if one can be reached without spending it (otherwise say not verified).
6. Isolation: which `--settings` keys (or flags, or an empty config folder) keep the owner's memory, skills, plugins, hooks and MCP
   servers out of a Faber session, the way `engine/src/account.rs` does for the Claude CLI.
7. MCP: whether a session can be given a tool server for that session only (a stdio command or an address in `--settings`), since
   the Faber tools (`conversation`, `plan`, `helper`) reach Claude Code as an in-host server and Codex as dynamic tools.
8. Quota: any zero-token way to read the account's limits (a local file, a subcommand, a slash command that answers without a
   model), with the evidence. If there is none, say so plainly.
The `testkit/src/fake.rs` fake CLI gets a mode that replays one of these files as `agy -p` output, so the next tasks test without
the real CLI.

Leave out: any engine or screen change, running more than the scenarios above, committing anything from the owner's own CLI folder
(AGENTS.md: never read, copy or store CLI credentials or session tokens). Note what the CLI would store in its own folders.

Done: a test that reads every file in `tests/fixtures/agy/` as lines of JSON and checks the `event` kinds found (fails today: no
files), the fake CLI replays `agy-plain` for a command named `agy`, and `docs/dev/antigravity-cli.md` answers all eight questions
(or says "not verified" with what was tried). New: a developer page, no product-map entry, no what's-new entry. Cost: states the
dollars and tokens of the recording runs. One review by the other vendor, on privacy of the fixtures first.
