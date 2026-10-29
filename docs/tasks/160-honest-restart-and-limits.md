# Honest restart and limits

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, section "What T3 Code adds". T3 marks every run that was live
when its server restarts as cancelled and offers "continue where you left off" (capped at three in five minutes), types
failures (usage limit, provider, transport, permission, validation, unknown) with a reset time when known, offers
"Resume at reset" (off by default), and retries an overloaded Codex five times with 2 to 30 s backoff from the saved
history, never resending the message.

Compare each with Faber first (`engine/src/engine.rs` load path settles streaming messages; quota recovery and
keep-going waits already exist) and build only the gaps: after a restart, a stopped turn says "O Faber foi fechado
enquanto isto rodava. Continuar?" with one button and a note that files already changed stay changed; a typed failure
reason per answer, with the reset time only when known; a bounded overload retry that shows "Tentando de novo (2 de
5)". Extends: product-map entries for errors and keep-going. Review by the other vendor.
