# Antigravity driver: a third vendor

Owner asked for Antigravity (`agy`) support (R482 in `docs/next/t3code-study.json`, replaces the "only Claude and
Codex" part of R4). Read `docs/research/2026-10-29-t3code-study.md` (Antigravity) and `03-providers-gemini.md` in
`E:\Dev\re-corpus\t3code\notes\`.

Already probed (details in the research page): resume by `--conversation` works; several turns in one process work with
`--input-format stream-json --print=""` and lines `{"event":"user","message":{"role":"user","content":"..."}}`; cancel
is killing the process and the killed turn is lost, so Faber keeps that prompt; print mode never asks, it runs edits in
the working folder and auto-denies commands (`result.denied_actions`) unless run with `--dangerously-skip-permissions`
or an allow rule. Still to prove, on recordings: credential-folder protection where the CLI can enforce it in
Full access (R121 supersedes R97's general push, rewrite and outside-project deletion blocks), through enforcement
before execution (native deny rules or host-mediated requests; observing tool steps afterwards is insufficient), how `ask_question` appears,
and whether the reply prefix `[mimi]` / `[sig: ...]` is stripped or shown.
R160 requires answerable questions in every mode. Direct print mode remains experimental and disabled because it
skips questions; do not ship it as Full access. Prefer the ACP path (truthful client name), once authenticated and
proved. "Ask before commands" also requires its pre-execution approval flow.

2026-10-30 follow-up: agy 1.4.11 skips `ask_question` in print mode without an answerable payload. A native
session-only deny rule blocked both read and write of a harmless canary (not a credential); shell and integration
bypasses remain unproven. ACP 1.4.1 initializes with client name `faber`, but `session/new` returns `-32000`,
`Authentication required`. The person must complete ACP's own sign-in; never copy the CLI's token. See the dated
research follow-up and local receipts. While sign-in waits, tasks 030 onward are independent and can proceed.

Engine: `Vendor::Antigravity`, a driver for the chosen path (no shell; `engine/src/cli.rs` resolves the binary), mapping
to the existing events, permission and question cards reused, environment scrub for inherited credentials, no token
file ever read, lenient on lines it does not know. Testkit: a replay agent and one recorded run per path.
Done: questions can be answered in every enabled mode and supported credential-folder blocks demonstrably prevent forbidden actions before execution; if the direct mode cannot enforce both,
use ACP with proven enforcement or leave that mode disabled. An Antigravity chat and a Projects turn run against the replay agent, cancel and resume work, the touched engine
tests pass in CI, memory limit R125 unchanged. Words on screen only in `ui/locales/` (every language), never "Gemini" in
a feature name.
