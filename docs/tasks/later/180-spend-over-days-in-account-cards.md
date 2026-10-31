# Show Faber's own spend over the last weeks in each account card

Area: Settings
Files: engine/src/engine/usage_view.rs, engine/src/ledger.rs, ui/Accounts.tsx, ui/api.ts, ui/locales/, tests/ui/settings.check.ts
Depends: none
Size: about 250 changed lines

Why: `docs/product/parity.md`, row C3: usage over days and weeks (Claude Code's `/usage` stats and `/insights`, Codex's
equivalent) has no answer in Faber, whose account cards show the quota windows and only "Faber today: $1.25 in API value"
(`usage.ownSpend`, `ui/Accounts.tsx`). The data is already kept: `VendorLedger::spend` in `engine/src/ledger.rs` adds each
reply's cost to `own_spend`, a map of day to dollars, and keeps the last 31 days (and `unmetered`, the count of replies
whose cost was unknown). Only today's value reaches the screen (`own_spend_usd` in `engine/src/engine/usage_view.rs`).
Per-model and per-area splits are not stored and are left out of this task.

Build:

- `VendorUsage` also carries the last 30 days as a short list of day and dollars (zero days included, `unmetered` counted
  apart), computed from the existing map; no new storage, no new read of any CLI.
- In the account card's existing "Details" fold, one quiet row "Last 30 days: $X in API value" that opens a small bar
  list of the days (reuse the card's bar style; no chart library) and says in plain words that replies of unknown cost
  are not in the sum. Same noun as today's line ("in API value"); no new section and no new control on a work screen.
- Every text in `ui/locales/`, by hand, in every language.

Leave out: per-model or per-area numbers, exporting, and any figure of the subscription's quota (the cards already show it).

Done: an engine test that fails today returns 30 days with gaps as zero; a screen check in `tests/ui/settings.check.ts`
shows the row and its list; walk it at 720 px and in the longest language. Extends: the product-map entry "Accounts and
limits" in the Settings section. What's-new entry in every language. The commit body states the cost (a map copy per
usage event). One review by the other vendor (R146), plus the coherence review of a change a person sees.
