# One notice when a leading agent finishes its job or needs a decision

Area: Tray
Files: engine/src/engine/keep_going.rs, engine/src/engine/plan.rs, engine/src/events.rs, src-tauri/src/tray.rs, src-tauri/src/tray/decide.rs, ui/locales/, docs/product-map.md
Depends: none
Size: about 220 changed lines

Why: the owner wants to be interrupted only for decisions, blocks and when an important stage ends. The tray notifies once per
question, held-back request, followed pull request that failed and standing goal that needs the person, and once when Keep going
stops by itself with its reason (`src-tauri/src/tray/decide.rs` `stopped_text`, `on_halted` in `src-tauri/src/tray.rs`; the product
map's "Tray" row). A project agent that led a team and ends its job with `wrap_up` `done: true`, or with a closing question for the
person (`engine/src/engine/plan.rs`, the `WRAP_UP_TOOL` arm), notifies nobody unless Keep going was on: the person comes back to a
finished job nobody told them about, or to a question that has waited all night. A team member finishing must stay silent (the
leader reports once).

Build: when a conversation that started other conversations ends its turn with a closing box that is `done` or asks a question,
and every member it started has stopped working, the engine sends the same halted event Keep going uses with a new reason
(`reason.keepGoing.stage` for done, `reason.keepGoing.decision` for a closing question; read how `reason.keepGoing.*` reaches
`on_halted` first and reuse that path, the AppEvent and the shell's `ShellTexts.stopped` map; if it needs a new host command,
stop and report, because the host command table is moving in the Electron change, #442). Texts in every language by hand, plain
("{title}: done, come and look", "{title}: needs a decision from you"), no pull request or commit words. Once per closing box
(the reply id), not again after a restart, never for a conversation the person has in front of them (`on_halted` already skips
a hidden app; the delivery rules of R92 for a focused window or a game in front stay as they are). The existing Keep going stop
keeps its own notice and is not doubled when both happen on the same reply.

Leave out: a setting (Notifications on/off already covers it), a sound of its own, notices for members, a summary notification
for the whole team.

Done: a test in `src-tauri/src/tray/decide.rs` and an engine test that fail today: a leader whose members have stopped and whose
box is `done` produces one halted event with the stage reason, a second identical close produces none, a member produces none,
a Keep going stop on the same reply produces one notice in total. Extends: the "Tray" row in `docs/product-map.md`. What's-new
entry in every language ("Faber tells you once when a big job is finished or needs you"). Review: the other vendor.
Cost: none at rest; one event per finished job.
