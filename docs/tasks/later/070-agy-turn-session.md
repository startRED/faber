# An Antigravity conversation runs a turn, stops, and resumes

Area: Projects
Files: engine/src/agy.rs, engine/src/engine.rs, engine/src/proc.rs, engine/src/events.rs, testkit/src/fake.rs, engine/tests/agy_session.rs, docs/product-map.md
Depends: antigravity-driver, agy-sign-in-and-setup, agy-reply-parser
Size: re-estimate after the authenticated ACP recording; no driver implementation is authorized by this brief alone.

Blocked: ACP's own sign-in and authenticated protocol evidence. Do not implement or enable a provider mode until the
prerequisites below are proved. The sign-in dependency is deliberate: the CLI's existing login did not authenticate
ACP. The sign-in task must cover the chosen ACP component's own official flow, not just CLI discovery.

Read R160, R470, R472 and R482 in `docs/decisions.md`, `010-antigravity-driver.md`, and the dated follow-up in
`docs/research/2026-10-29-t3code-study.md`. `docs/dev/antigravity-cli.md` records the direct CLI format; it is not an ACP
contract. Direct-print fixtures cannot prove ACP questions, permissions, cancellation or resume.

Evidence on 2026-10-30: direct `agy` 1.4.11 print mode skipped an explicit `ask_question` without an answerable payload.
ACP 1.4.1 initialized as the truthful client `faber` with protocol 2, then `session/new` returned `-32000`,
`Authentication required`. Advertised resume capabilities are not proof of an authenticated resumed turn. Selected
metadata is in `.faber/verify/agy-acp-probe/receipt.log` in the study copy; repeat the probe if unavailable.
Never read, copy or transfer credentials from the CLI to ACP. The person completes the component's own sign-in.

Before implementation, record authenticated ACP exchanges using Faber's truthful identity: a plain reply, a question
answerable in every enabled mode, command approval before execution, cancellation during a turn, a later turn resuming
that conversation, and unexpected process exit. Record rejection and denial too. Discover models from the authenticated
component; do not assume the CLI list carries over or impersonate another client. Prove credential-folder protection
before execution with harmless canaries, including relevant tool, shell, MCP and path-alias bypasses. Native read/write
denials alone do not prove the whole boundary. Keep unsupported modes disabled.

Then implement the authenticated transport behind the existing `events::Session` boundary and its question, permission,
tool, usage and reply events. Reuse child ownership and shutdown in `engine/src/proc.rs` and `winproc/`: no shell, no
visible console, and Stop leaves no process tree. Define Stop and resume from recordings, keeping the interrupted
prompt in Faber even if the provider drops it. Never silently replace Ask with Accept edits, skip questions in Full
access, or enable direct print as a fallback. Preserve the environment scrub and credential protection.

Scope here is the Projects session lifecycle; R472 also requires Chat and Auto, covered by the driver and app tasks.
Use Antigravity as the provider label; Gemini remains a model name where supplied by the provider. Do not add setup,
quota routing or screen controls here, or promise Faber tools before authenticated capability tests prove them.

Done: replay recordings in `engine/tests/agy_session.rs` through the fake agent: text and tool turns, answerable questions,
approval and denial, Stop with no surviving child, resume with the recorded identity, and early-exit errors. Match the
chosen ACP transport, not direct CLI flags. A new check fails without the implementation. Update the Antigravity
product-map entry when built; no what's-new entry until the person can pick it. Before queueing, get the independent
review and second security review required for a process launcher. Measure child memory and processor cost through the
existing registry; R125 remains unchanged. Until authenticated evidence exists, leave this task blocked and work on
independent tasks.
