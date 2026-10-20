# Preserve pending work and close retired servers after settings changes

The existing settings-change path for permissions and reply style has two lifecycle defects. Fix that shared path,
not the reply-style feature alone. Start in `engine/src/engine.rs` (`Core::save_settings`, the "Idle CLIs restart"
block, `end_turn`, `close_when_idle`), `engine/src/engine/compact.rs` (`retire`), `engine/src/engine/keep_going.rs`
(`halt`), and `engine/src/codex.rs` (`CodexServer::retire`).

A project agent requests `compact(next=...)`, its reply ends, and Faber's checks start. Change permissions or reply
style while those checks run: the settings path considers the session idle, and `retire` deletes its scheduled
compaction and continuation. Once checks finish, the remaining work silently stops.

A retired Codex server can also keep its idle sessions and process alive indefinitely. Start two Codex conversations,
leave one answering, change permissions or reply style, and resume the other on the new server. Let the old reply
finish without sending another message to it: the old server stays open. Cover the same cleanup when Keep going
halts by itself, not only when the person presses Stop.

Done when: fake-CLI engine tests drive the real settings save during running project checks for both CLIs and prove
the scheduled compaction and exact continuation survive; tests in Chat and Projects prove a retired Codex process
closes after its last work, including an automatic Keep going halt, before any later message or engine shutdown.
Exercise both permission and reply-style changes; these regression checks must fail without the fix. Other
conversations' running turns and the existing pull-request watch wakes during Keep going rests must keep working.
Update the product map and docs/design for any changed behavior, and add a what's-new entry in five languages.
