# Quota you can trust

Area: Quota window, pace, plan; Accounts and limits
Files: engine/src/engine/quota_watch.rs, engine/src/budget.rs, engine/src/ledger.rs, engine/src/model.rs, engine/src/engine/recovery.rs, ui/Thread.tsx, ui/Accounts.tsx, ui/locales/, engine/tests/quota_live.rs, engine/tests/route.rs, tests/ui/quota.check.ts, docs/product-map.md
Depends: none
Size: two pull requests: first wake refresh and unreadable quota, then handoff wording

Why: After sleep a quota reading can look current for hours. An unreadable quota shows a dash with no explanation or next step (walkthrough shots `reports/reforma/w10/shots/b/b01-chat-home.png` and `reports/reforma/w10/shots/b/b10-settings-accounts.png`). A handoff can also say an AI ran out and continued on that same AI.

Build: Detect a wake in `quota_tick` from a wall-clock/monotonic gap or late tick; mark cached readings stale, refresh each connected AI serially at once, and exclude the first new sample from foreign-use comparison. For unreadable quota, explain why it cannot be read and the action available, in the sidebar and Accounts and limits, without presenting an unknown value as empty. Check what PR #514 already supplies. Store the model that ran out beside `moved_from` with a backward-compatible default. For different AIs name the one out of quota and the one taking over; for a move within one AI do not claim its quota ran out. Preserve the previous-messages or summary detail and old-message fallback.

Leave out: a power-event API, new timer or process, quota fetching changes, the wait-for-quota card, pace text and Auto routing.

Done: A fake-clock test fails today and proves an eight-hour gap starts a refresh at the next tick and does not count the first jump as foreign use. Screen checks fail today for unexplained unknown quota and same-AI quota wording, then cover both surfaces, different and same-AI moves and old messages; an engine test checks the saved model. Walk at 720 px and in the longest language. Update the map and affected design pages, add a what's-new entry in every language, and get fresh and coherence reviews. Report the per-message string and any other cost.
