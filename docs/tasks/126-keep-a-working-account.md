# Never drop an AI that is working because one check missed it

Area: Account cards
Files: engine/src/engine.rs, engine/tests/account.rs
Depends: none
Size: about 180 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"). Settings showed
Claude as "Não ligada" while Claude was answering and its quota had been read, and minutes later the sidebar dropped
Claude's quota bar. Two causes are possible and only one is fixed so far. The first, a sign-in check that gave up after
5 s, is PR #438 (waits 20 s): check `gh pr view 438` and what it landed first. The second is a path the code allows:
the sidebar's list of AIs is `Core::vendors()` = the keys of `opts.clis` (`engine/src/engine.rs` line 1701), and
`Core::replace_clis` (line 1705) replaces that whole map with what `detect_clis()` found at that moment
(`src-tauri/src/setup.rs` lines 93, 98 and 209). A detection that misses the CLI for one moment (a file being replaced by
the CLI's own update, a changed PATH) removes the vendor, and `changed(v)` then retires its live sessions (the same
function, `stale`) and drops its quota bar. Do not assume this happened: reproduce it with a test first.

What to build: a new `Engine::refresh_clis(found)` (the existing `replace_clis` stays exactly as it is, because
`engine/tests/account.rs` lines 302 and 446 and `approve_plan_review.rs` line 421 use it to remove a CLI on purpose).
It keeps a vendor that `found` lacks when its previous `Cli.command` still names a file (or a vendor with a live session
or a reply in the last minutes), and drops it when that file is gone in two refreshes in a row. It logs one line to the
engine's stderr when it keeps one. Use it where Faber re-detects while running: the three calls of `setup.rs` change in
a one-line follow-up after PR #442 (the host move) and PR #438 have landed; this task does the engine side and its test
only, so it does not touch `src-tauri`.

Leave out: the account status wording (`AccountStatus`), the 20 s wait (PR #438), the quota reading itself.

Done: a test in `engine/tests/account.rs` fails today (a vendor with a live session disappears after one empty
detection and its session is retired) and passes with `refresh_clis`: kept after one miss, dropped after two with the
file gone, kept while the file exists. Extends: product-map "Account cards". The commit states there is no memory or
processor cost. Review by the other vendor (R146).
