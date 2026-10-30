# After a restart, say the answer was cut and offer one button to continue

Area: Chat conversation
Files: engine/src/engine.rs, engine/src/model.rs, engine/src/engine/by_itself.rs, ui/api.ts, ui/Thread.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, engine/tests/engine.rs, tests/ui/thread.check.ts
Depends: none
Size: about 380 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, "What T3 Code adds" (T3 marks every run that was live when its
server restarted as cancelled and offers "continue where you left off", capped at three in five minutes) and part 3
(the owner's screenshot of a running project turn starts with "Interrompida."). Compared with Faber on 2026-10-29: when
a conversation loads from disk, `Core::load` calls `settle` on each message (`engine/src/engine.rs` lines 402 and 1798),
which turns a `Streaming` reply into `Stopped` and, for helpers, `interrupted`. The screen then shows the same word as
for the person's own Stop: `answer.stopped` ("Interrompida."), and `Answer` draws it only when the stop was not Faber's
own (`ui/Thread.tsx` near line 593, `props.prompt && !props.prompt.wake`). Nothing says Faber was closed, nothing
offers to go on, and nothing warns that files the agent already changed stay changed.

What to build: `settle` records on the message that it was cut by a restart (`Message.closed: bool`,
`serde(default, skip_serializing_if)`, set only on the load path when the status was `Streaming`; the other callers of
`settle` at lines 2026, 4591 and 4847 keep their meaning, check each). The screen shows, under that reply, "O Faber foi
fechado enquanto isto rodava. Continuar?" with one button and, for a project, a short second line "Arquivos que ela já
mudou continuam mudados." The button sends one fixed turn as Faber's own message (`Message.wake` with a new key, the way
`engine/src/engine/by_itself.rs` sends Faber's other messages, so the reply says who asked), telling the agent where it
stopped and to look at the files before it goes on. At most three such continues in five minutes per conversation
(an in-memory counter; the fourth shows the same line without the button).

Leave out: continuing by itself without a click, resuming at quota reset (Faber has the quota wait already:
`reason.waiting`, `QuotaCard`), anything about the helper that was running (`interrupted` already shows it as stopped).

Done: an engine test in `engine/tests/engine.rs` fails today (a reply saved as `streaming` loads as a plain stop): it
loads with `closed` set, a person's own Stop does not set it, and the fourth continue in five minutes is refused; a screen
check in `tests/ui/thread.check.ts` shows the line and the button, and no line after the person pressed Stop. Extends:
product-map "Chat conversation" and "Why a reply stopped". What's-new entry in every language. Walk it at 720 px and in the
longest language. No memory or processor cost beyond one flag and one counter. Review by the other vendor (R146).
