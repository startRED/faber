# Faber reads Gemini's limits the way it reads the other two, or says it cannot

Area: Quota window, pace, plan
Files: engine/src/ledger.rs, engine/src/cadence.rs, engine/src/engine/quota_watch.rs, engine/src/budget.rs, engine/src/usage.rs, engine/src/toolbox.rs, ui/Settings.tsx, ui/locales/, docs/product/quota-pacing.md, docs/product-map.md
Depends: gemini-vendor-variant, agy-recorded-runs
Size: about 380 changed lines

Why: the owner wants work spread "by quota" over three AIs. Pacing is the product's strength (`docs/product/quota-pacing.md`):
Claude's and Codex's counters are read by commands that answer locally and cost no model tokens (`claude -p /usage`, Codex
`account/rateLimits/read`), on a debounced schedule per vendor (`engine/src/cadence.rs`), recorded in `ledger.rs`, turned into pace
by `budget.rs`, and exposed to the agent by the `quota` tool (`engine/src/toolbox.rs`). Auto "requires billing confirmed as a
subscription before any request" and denies unknown billing. For Antigravity: `agy --help` shows no usage command and a `/usage`
turn is a model turn (checked); the 2026-10-14 plan (`docs/superpowers/plans/2026-10-14-parallel-work-loop.md`) notes an Antigravity "Claude/GPT bucket", so Antigravity's Claude and GPT models seem to share a bucket apart from
Gemini's. Whether any zero-token source exists is a question of `docs/dev/antigravity-cli.md` (task `agy-recorded-runs`, item 8).

Build, depending on that answer. If a zero-token reading exists: a reader like the other two (short-lived, hidden, idle priority
on Windows, one at a time, the same debounce and the same "still counting" lag handling with a measured lag), windows named for
what the source reports (per model family where it does: Gemini models versus the Claude and GPT-OSS models in Antigravity),
the ledger entry from `gemini-vendor-variant`, the reading in Settings' whole-account bars and in the `quota` tool's answer, with
a page section in `quota-pacing.md` (what is read, how often, measured lag and cost). If none exists: Gemini has "no confirmed
reading" (an existing state), Auto never picks it by pace, an explicit choice and a leader's `ai: "gemini"` still work and count only
Faber's own spend, the `quota` tool says "no reading" for it, and the pull request says plainly what was tried; do not poll a model
to find out.

Leave out: routing table (`gemini-profiles-in-routing`), marks, paying for any usage; Faber never starts a turn only to measure.

Done: tests that fail today: a recorded reading becomes windows and pace for Gemini in the ledger and in the `quota` answer, a failed
or malformed reading leaves the old one (or none), a reading with model tokens is thrown away as the other two are; or, on the no-source
branch, the "no confirmed reading" state and the exclusion from pace. Extends: "Quota window, pace, plan" and "Faber's AI limits" in
`docs/product-map.md` and the measured-lag table of `docs/product/quota-pacing.md`. What's-new: none until Gemini can be chosen.
Review: the other vendor. Cost: the new reader's CPU seconds and peak memory, measured and written in the body.
