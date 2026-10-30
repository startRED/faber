# Open a conversation with its newest messages and fetch older ones by request

Area: Chat conversation
Files: engine/src/engine.rs, engine/src/model.rs, engine/tests/big_conversation.rs
Depends: none
Size: about 300 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1 and "What T3 Code adds" (T3 serves history in pages of
10 user turns or 75 items, at most 1 MiB). `Engine::open_for_screen` (`engine/src/engine.rs` near line 980) returns the
whole conversation, with every message and tool record, and the shell sends it to the screen in one reply
(`src-tauri/src/main.rs` near line 222). The screen shows only 40 messages at a time (`PAGE = 40` in `ui/App.tsx`),
so opening the owner's 102 MB conversation moves about 100 MB through the bridge to draw 40 messages.

What to build, engine side only: `Engine::open_for_screen_window(id, newest: usize)` returns the same `Conversation`
(`for_screen()` applied) but with only the newest `newest` messages, plus how many messages came before them
(`Conversation.earlier: usize`, absent when 0), and `Engine::messages_before(id, message_id, n)` returns the `n`
messages ahead of that one, also through `Message::for_screen`. Both read the loaded conversation (`Core::opened`,
which already holds it in memory) and clone only the slice. Everything else on `Conversation` (plan, title, queue,
settings of the reply) stays whole; only `messages` is cut. Keep `open_for_screen` as it is for the callers that need
every message (export, search, rewind), and list them in the commit body.

Leave out: the shell command and the screen (task `older-messages-on-scroll`), the pushed `AppEvent::Message` events
(they already carry one message), search over history.

Done: a test on the 100 MB fixture (`big-conversation-fixture`, or a local loop of the same shape) opens with 40
messages in less than a tenth of the whole-conversation time and returns `earlier`, and `messages_before` walks back to
the first message without a gap or a repeat. Extends: product-map "Chat conversation". The commit states the memory and
processor cost (R125): a window of 40 messages against the whole record.
