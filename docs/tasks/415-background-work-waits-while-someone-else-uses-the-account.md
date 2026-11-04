# Background work waits while someone else uses the account

Area: engine
Files: engine/src/budget.rs, engine/src/ledger.rs, the engine files that start background work, docs/product-map.md
Depends: none
Size: about 200 changed lines (split if the starters are many: one pull request for the rule, then one per kind of work)

Why: docs/product/README.md promises a shared-account mode: "Faber watches the quota readout: if it drops faster than
Faber's own usage explains, someone else is using the account, so background work (benchmarks, harvesting expiring
quota, re-runs) pauses until the account is quiet again", and "Runs wait while someone else is using the account". The
reading exists (`Snap::jumped` in `engine/src/budget.rs`, from `ledger.rs` `outside(now).jumped()`; the usage view shows
`foreign_use`), but today it only stops discounts (`Snap::discount_ok`): nothing that Faber starts on its own waits for it.

Build: one rule in the budget, "the account is in use by someone else now" (the same `jumped` reading, per vendor), and
every kind of work Faber starts without a person's message asks it before it starts: benchmark runs, spending quota that
is about to expire, re-runs and checks by itself. Find them first (search for where such work is started and gated by
quota today) and list them in the pull request. A person's own message is never held. Waiting work starts by itself
when the reading is quiet again, and the screen's existing quota line says why it waits, with keys in `ui/locales/` in
every language (hand-written; English is the source).

Done: an engine test that fails today: with `jumped` set for a vendor, a background start for that vendor waits and a
person's message does not; when it clears, the waiting work starts. The product map names the rule where the quota
readout is described. One review by the other vendor.
