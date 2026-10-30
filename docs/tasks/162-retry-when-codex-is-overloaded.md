# Retry an overloaded Codex a few times, without sending the message again

Area: Why a reply stopped
Files: engine/src/engine.rs, engine/src/engine/by_itself.rs, engine/src/model.rs, ui/api.ts, ui/Thread.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, engine/tests/engine.rs, tests/ui/thread.check.ts
Depends: typed-failure-reasons
Size: about 350 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, "What T3 Code adds": T3 retries a Codex `serverOverloaded` five
times with 2 to 30 s backoff, from the saved history, and never resends the person's message. Faber has no such retry
(a search for "overload" in `engine/src` found nothing on 2026-10-29): an overloaded turn ends as a failure and the
person must send again. The typed failure `Overloaded` comes from task `typed-failure-reasons`.

What to build: when a turn of a Codex conversation ends with `Failure::Overloaded`, the engine does not finish it with an
error. It keeps the reply open, shows "Tentando de novo (2 de 5)" on it (`Message.retrying: Option<{ n, of }>`,
cleared when a retry starts producing words, the turn ends or the person presses Stop), and after 2, 4, 8, 16 and 30 s
sends one fixed internal turn on the same thread asking the agent to continue from where the history stands, the way
`engine/src/engine/by_itself.rs` sends Faber's own messages. The person's message is never sent again. After the fifth
failure it ends with the typed sentence and the usual "send a message to try again". The waits use tokio timers on the engine's runtime (no thread of their own), are cancelled when the conversation is stopped, closed or deleted, and keep the quota gate
(`Core::allows`) in front of every retry. Claude is out of scope.

Leave out: other failure kinds, a setting for the count, resending the message.

Done: an engine test with the fake app-server fails today (the turn ends at the first `serverOverloaded`): it retries
with the stated waits with tokio's paused time, shows `retrying`, stops at five, does not retry after Stop, and
never contains the person's message in a retry. A screen check shows the line and its number. Extends: product-map
"Why a reply stopped". What's-new entry in every language. The commit states the processor cost (one timer per retrying
conversation). Review by the other vendor (R146).
