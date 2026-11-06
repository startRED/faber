# T3 Code study: what to take, and Antigravity

Dated research, history after its day. T3 Code (pingdotgg/t3code, MIT) was read as ideas only; Faber writes its own
code (AGENTS.md). Four read-only passes wrote full notes with file and line references outside the repo, in
`E:\Dev\re-corpus\t3code\notes\` (`01-output-rendering`, `02-agent-progress`, `03-providers-gemini`, `04-general-ux`).
Nothing was run or built. Line numbers in the notes marked `~` are approximate.

## What T3 does that Faber does not yet

| Idea | What the person gets | Verdict |
| :--- | :--- | :--- |
| Turn fold | A finished turn collapses to "Worked for 2m 3s"; approvals, questions and live helper cards are never folded | Take |
| Group sentence | "Ran 3 commands", "Changed 4 files" plus one live row for the work in progress | Take |
| Lazy detail | Tool output is fetched only when a row is opened; the panel listens only while open; terminal tail capped at 64 KiB | Take (memory, R125) |
| Needs-you-first list | One status per thread: needs you beats running beats done; unread dot; child threads under their parent; collapsed project shows its loudest status | Take |
| Anti-flicker | A thread keeps reading "working" 2 s after it stops; "Done" waits 2 s; background work blocks "Done" | Take |
| Docked questions | Approvals and questions sit in the composer, never fold away, answered with number keys, auto-advance | Take |
| Child approvals bubble | A helper's approval shows as one banner in the parent | Take |
| Helper record | Status, title, one live progress line, final report, model; lineage to the parent | Take (we have steps in the reply, R161) |
| Plan card | One card, one smart button ("Refine" or "Implement") | Maybe |
| Edit from here | Rewind the chat, choose: cancel, revert files too, or keep changes | Maybe |
| Streaming code | Parse cached up to the last closed fence; math, diagrams and "Run" wait for the fence to close | Take |
| Notifications | T3 deduplicates by run/status and suppresses first sightings, archived and stopped runs | Adapt: only needs-you notices in Faber (R74/R92), deduplicated per question |
| Command palette | Also searches threads, message text and settings | Take |
| Queue | Messages typed while it works show as dashed bubbles, "Send now", "Resume at reset" | Take |
| Settle + undo | Archive with an undo toast and `mod+z` | Maybe |
| Chip context | File, thread, terminal excerpt, quoted answer, image as chips in the composer | Maybe |
| Threads without a project | Dated folder, "turn into project" later | Maybe |
| Reconnect replay | Replay missed events only up to 128 events and 1 MiB, else one snapshot | Maybe (engine to screen) |

Not taken: remote access over SSH or Tailscale and the mobile app (need a relay and a second app), 25 settings pages,
an Electron-sized memory use, importing history from the CLIs' folders (reads credential folders' neighbours; needs the
owner), the quota feature that reads the agent's token file (breaks the no-credentials rule), and posing as another
client to unlock models (a provider-terms question for the owner).

Owner said yes (2026-10-29) to running one request on several models at once, each in its own worktree and thread, and
to switching vendor in the middle of a conversation with a handover of what happened. Both spend quota, so Auto's
quota pacing must show and bound them. Tasks 060 and 070.

## Antigravity (the owner means this one, not the plain Gemini CLI)

T3 supports it as a provider: Google's Antigravity agent run as `agy_acp_server` (a separate download, version 1.4.1)
over ACP, newline-delimited JSON-RPC on stdin and stdout. Its models are Gemini, Claude and GPT-OSS on the person's
Google plan.

Checked on this PC, 2026-10-29: `agy` 1.4.7 is installed. `agy models` lists Gemini 4 Argon, Gemini 3.8 Flash, Claude
Opus 5.5, Claude Sonnet 5.5 and GPT-OSS 120B, each at low, medium and high. It has no ACP mode of its own. One tiny
run (`agy --print ... --output-format stream-json --model gemini-3.8-flash-low`, 4 s, 19,093 tokens) printed JSON lines:
`init` (conversation id, model, working folder, tool list including `ask_permission`, `ask_question`,
`invoke_subagent`, `permission_mode: request-review`), `step_update` (step index, state ACTIVE or DONE, `step_type`,
`text_delta`, usage) and `result` (status, response, duration, usage). The reply carried a stray `[mimi]` / `[sig: ...]`
prefix, so the answer text needs a check before it is shown. `--input-format stream-json` takes one JSON line per turn;
`--conversation <id>` resumes; `--mode accept-edits|auto|plan`, `--effort`, `--dangerously-skip-permissions`.
Probed the same day with `gemini-3.8-flash-low`, a scratch folder, nothing kept:

- **Resume works.** `--conversation <id>` keeps the id, continues the step numbers and remembers the earlier turns.
- **Several turns in one process work.** `--input-format stream-json` with `--print=""` (the flag needs a value; a
  prompt on the command line is refused) reads one line per turn,
  `{"event":"user","message":{"role":"user","content":"..."}}`, and prints one `result` per turn.
- **Cancel is killing the process.** There is no cancel message that I found. The killed turn is not kept: resuming the
  same conversation afterwards did not know the interrupted request. Faber must keep that prompt itself.
- **Print mode never asks.** Edits inside the working folder and file reads ran without a question. A shell command was
  auto-denied with the text "headless mode cannot prompt" and listed in `result.denied_actions`
  (`{"action":"command"}`); `--dangerously-skip-permissions` or an allow rule in the CLI's `settings.json` lifts that.
  So "Ask before commands" cannot be shown as a card on path A. Not found: how `ask_question` would appear.
- Tool steps carry `tool_name`, `tool_info.parameters` and, when done, a short `output`; usage is on every finished step.
- Every answer starts with `[mimi]:` and a `[sig: ...]` line, in all runs: a house style of the agent, to strip or keep.

Two ways to drive it, decided by a recording, not by taste:

- **A. `agy` itself, stream-json.** Same shape as how Faber drives Claude Code: the official binary, unmodified,
  nothing extra to download. Limit: no approval cards (probed above).
- **B. T3's way, the ACP server.** Structured permission requests, plans, tool kinds, cancel and resume are known to
  work, but it is a second binary from Google's download site, and T3 poses as another client (`zed`) so the agent
  offers the Claude and GPT models. Posing is a provider-terms question: Faber would send its truthful name and see
  which models appear.

Path A remains experimental and disabled: R160 requires answerable questions in every mode, and the follow-up
probe below shows print mode skips them. Prefer B once its own sign-in is available and its questions, approvals,
cancel and resume have been proved with Faber's truthful client name. Both paths must preserve credential-folder protection where the
CLI can enforce it (native deny rules or host-mediated requests before execution; observing tool steps afterwards is
insufficient; to prove in task 010). Do not reintroduce R97's superseded general push, rewrite or outside-project deletion
blocks. If credential blocks or answerable questions cannot be enforced, keep that mode disabled. Path B is what "Ask before
commands" and questions need. Sign-in stays in the CLI's own flow. Faber
scrubs inherited credential variables from the child's environment and never reads a token file; T3's quota call
reads such a file, so Faber shows context use from the stream's usage numbers and the reset time in error text only.

Plan: `Vendor` gets a third value (about 30 files; 154 uses of `Vendor::Codex`) and the screen says "Antigravity",
never "Gemini" in a feature name (AGENTS.md). The driver maps `step_update` and `result` to Faber's events (message,
thinking, tool, plan, usage, permission, question), keeps the conversation id for resume, and treats unexpected
lines leniently (log, do not die). Auto's table, quota reading, setup, the picker and the "other vendor reviews" rule
get an Antigravity row; models come from `agy models`, not a hard-coded list. Tests: one recorded run per path
(plain answer, tool call with permission, question, cancel, resume) and a replay agent in the testkit.

## Where it went

### Follow-up probes, 2026-10-30

The installed CLI is now `agy` 1.4.11; the 1.4.7 observations above remain dated observations, not current guarantees.
An explicit `ask_question` request in print mode ended with an `unknown` step and a successful result saying the choice
was skipped. No answerable question payload reached stdout. The official
[headless reference](https://antigravity.google/docs/cli/headless/) also rejects control-request and control-response
input events. Direct streaming therefore cannot currently satisfy Faber's question-card requirement.

A separate harmless-canary probe used session-only `--settings`, `toolPermission: always-proceed`, and
`permissions.deny: ["read_file(blocked/)"]`. An allowed file read completed; reading and writing the blocked canary
both produced native permission-check errors matching the configured deny rule. The file stayed unchanged and its
marker was absent from the answer. `result.denied_actions` was absent for these explicit denials, so tool errors must
also be mapped. This proves these two native file operations only, not shell, MCP, symlink or credential protection.
No actual credential file or sign-in folder was inspected. Local logs are in `.faber/verify/agy-policy-probe/` and
`.faber/verify/agy-question-probe/`; rerun the probes before treating them as release evidence.
See the official [permission reference](https://antigravity.google/docs/permissions?tab=cli) for rule precedence and
Windows path normalization. Full access follows R121, not the superseded general restrictions of R97.

The official Windows ACP 1.4.1 archive was downloaded for study outside the repository. Its SHA-256 matched
`5f5b7d0328df17c128b2be338d4567b0d24e56ace8196e88794fec37d1372554`. No provider driver is implemented yet.
With the truthful client identity `faber`, ACP initialization negotiated protocol 2 and reported version 1.4.1.
Creating a session then returned `-32000`, `Authentication required`. The existing CLI sign-in did not carry over.
The local receipt is `.faber/verify/agy-acp-probe/receipt.log`; it contains selected protocol metadata only. The
probe process and its children were stopped. ACP model access, questions, approvals and credential protection remain
unverified until the person completes that component's own sign-in; no token may be copied from the existing CLI.

`docs/tasks/` holds the build order (Antigravity first, then the screen ideas). The scope change is the `R-next` row in
`docs/next/t3code-study.json`. Living pages (`product/README.md`, `models.md`, `product-map.md`) change in the commits
that build it, not before.
