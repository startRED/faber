# Say what kind of failure ended an answer, with the reset time when it is known

Area: Why a reply stopped
Files: engine/src/failure.rs, engine/src/events.rs, engine/src/codex.rs, engine/src/claude.rs, engine/src/engine.rs, engine/src/lib.rs, ui/Thread.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, engine/tests/failure.rs, engine/tests/all.rs, tests/ui/thread.check.ts
Depends: none
Size: about 380 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, "What T3 Code adds": T3 types failures (usage limit, provider,
transport, permission, validation, unknown) with a reset time when known. Compared with Faber on 2026-10-29: a turn that
did not end well gets one of five keys from `terminal_reason` and `failure_reason` (`engine/src/events.rs` lines 24 to
53): `reason.interrupted`, `reason.turnLimit`, `reason.budget`, `reason.failed` ("The AI stopped because something went
wrong. Send a message to try again.") and `reason.other`. A usage limit is told apart by phrase matching of the error text
(`recovery::plan_limit`, `engine/src/engine/recovery.rs` line 38) and handled by the quota recovery. Everything else, an
overloaded server, a lost connection, a rejected request, a revoked sign-in, a full context, reads the same generic
sentence. Codex says more: its `error` notification carries `error.codexErrorInfo` (values such as `usageLimitExceeded`,
`rateLimitExceeded`, `serverOverloaded`, `contextWindowExceeded`, `unauthorized`, `badRequest`, `internalServerError`,
`httpConnectionFailed`, `sandboxError`; checked on 2026-10-29 with `codex app-server generate-json-schema`, codex-cli
0.166.0, `v2/ErrorNotification.json`, which also has `willRetry`), and `engine/src/codex.rs` near line 721 reads only
`error.message`.

What to build: `engine/src/failure.rs` with `Failure { Limit, RateLimit, Overloaded, Connection, SignIn, Rejected,
ContextFull, Other }` and two classifiers: one from Codex's `codexErrorInfo` (carried on the new
`EngineEvent::Error` field or on `Reason`), one from Claude's `errors` and `result` text using the phrase-list style of
`plan_limit` (a fixed list, tested). The reply's error key becomes `reason.fail.<kind>` with one plain sentence each and
a next step ("Sem conexão com a OpenAI. Tente de novo em instantes."). Where the reset time is known (`wait_until`,
`untilReset` in the screen) the sentence includes it; where it is not, it says nothing about time. `reason.failed` and
`reason.other` stay for what is truly unknown.

Leave out: the bounded retry (task `retry-when-codex-is-overloaded`), "Resume at reset" (the quota wait exists), the
quota recovery path itself.

Done: engine tests in `engine/tests/failure.rs` fail today (every case gives `reason.failed`) and pass for each Codex
value and a table of Claude phrases, with a real Codex fixture line for `serverOverloaded` in `tests/fixtures/`; a screen
check shows the sentence and the reset time only when known. Extends: product-map "Why a reply stopped". What's-new entry
in every language. No memory or processor cost. Review by the other vendor (R146).
