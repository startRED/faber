# Codex CLI helpers: behavior spec from public source (0.162.0-alpha.17.2)

Source: public Codex repository snapshot, cited by path under `codex-rs/`. No binaries were run. Every point is tagged
"(observed: source)" meaning it was read in the public source text; runtime behavior is not verified.

## 1. Windows sandbox

- Sandbox modes the user picks: read-only, workspace-write, danger-full-access (protocol/src/config_types.rs). (observed: source)
- Windows isolation levels: off, a restricted-token mode, and an elevated mode. The config accepts elevated, unelevated and an opt-in
  MXC backend; the MXC backend overrides the others when selected (config/src/types.rs, protocol/src/sandbox.rs,
  core/src/windows_sandbox.rs, mxc-sandbox/README.md). (observed: source)
- Elevated setup asks for administrator approval once (a UAC prompt) and then provisions two local sandbox accounts (offline and
  online), a sandbox group, firewall block rules for the offline account, and access control rules on the project folder
  (windows-sandbox-rs/src/setup.rs, acl.rs, setup_provisioning/firewall.rs). Setup can run in full, provision-only, or interactive
  provisioning modes. (observed: source)
- Deny-read rules: some paths are blocked from reads for the sandboxed account, and that deny state persists across runs
  (deny_read_acl.rs, deny_read_state.rs). (observed: source)
- The command runs as the sandbox account inside a private desktop reused per account, with output streamed back as frames and
  either a pseudo-terminal or plain pipes (bin/command_runner/win.rs, desktop.rs). A job object owns the process tree.
  (observed: source)
- A separate privileged service can provision the sandbox over an authenticated named pipe, with a debug-only foreground mode and
  policy validation (windows-sandbox-service/src/lib.rs, service.rs, ipc.rs, machine_policy.rs). (observed: source)
- Denial detection: a non-zero exit is treated as a sandbox denial only when output contains keywords such as "operation not
  permitted", "permission denied", "read-only file system", "sandbox", "failed to write file". Exit codes 2, 126 and 127 are
  never treated as denials. A denial is reported as a sandbox-denied error, separate from a timeout (sandboxing/src/denial.rs,
  core/src/exec.rs). (observed: source)
- Escalation: a command that needs more access asks for approval first; an approved escalation keeps the denied-read rules in
  place (core/src/tools/sandboxing.rs). The exact approval wording was not extracted. (observed: source, partial)

Faber idea: show the active mode and why a command was blocked as a card, with a "try with more access" button that always
asks first.

## 2. Code mode

- A model writes JavaScript that calls the enabled tools through one global object. The script runs in a fresh isolate with no
  file system, network or console (code-mode-protocol/src/description.rs). (observed: source)
- Tool names are normalized to identifiers; each nested tool takes a string or object and returns an object or string.
  (observed: source)
- Helpers the script may call: text, image, audio, generated-image, store/load (per-session key-value), notify (injects an
  extra output mid-run), timers, a list of enabled tools, exit, and yield-control. (observed: source)
- First-line pragma `// @exec:` with JSON options: an early-yield time and an output token budget. Defaults: yield ~10 s, 10,000
  tokens. Unawaited promises are dropped when the script finishes. (observed: source)
- Two model-facing tools: `exec` starts a cell and returns "Script running with cell ID ..." if it yields; `wait` resumes
  that cell, returns only new output, can terminate it, and closes it once finished. (observed: source)
- Sessions are durable per thread. Limits per session: a maximum yield time and a heap size cap (code-mode-protocol/src/session.rs).
  (observed: source)
- The host runs over stdio or gRPC and can export traces (code-mode-host/src/main.rs). JIT mode is chosen at engine start
  (code-mode-runtime/src/v8_init.rs). (observed: source)

Faber idea: a "run a script" card that shows the cell's progress, yields early with partial output, and offers a stop button.

## 3. App-server protocol

- Transport: JSON-RPC style requests, responses and notifications, with a v1 and a v2 schema (app-server-protocol/schema/json).
  (observed: source)
- v2 request families (names as published in the schema): thread start/resume/fork/read/list/archive/delete/compact/rename,
  turn start/steer/interrupt, review start, command exec (run, write, resize, terminate), shell command in a thread, config
  read/write, models list, plugins and marketplace, filesystem read/write/watch, account login/read, goal set/get/clear,
  attachments, sections. (observed: source, schema names)
- Server notifications: thread started/closed/archived/renamed/status changed/token usage updated, turn started/completed,
  turn diff updated, turn plan updated, item started/completed, agent message delta, plan delta, reasoning text and summary
  deltas, command output delta, file change patch updated, hook started/completed, MCP tool progress, guardian review
  started/completed, model rerouted, context compacted, error, config warning, deprecation notice, account rate limits. (observed:
  source, schema names)
- Experimental user verification: status, enroll, delete, verify, cancel; gated behind an opt-in flag (app-server/README.md).
  The feature gate is noted; it was not enabled. (observed: source)
- Rollout compression and a managed model provider requirement are documented in the same README. (observed: source)

Faber idea: map turn plan, item and diff notifications to plan, progress and file-change cards in the chat, rather than raw text.

## Unknowns and bounds

- Approval wording and the exact on-screen denial message were not extracted; only the code paths were located.
- Server request names (approval asks, questions to the user) were not read from ServerRequest.json; only notifications were listed.
- Nothing was run; behavior is inferred from source only.
