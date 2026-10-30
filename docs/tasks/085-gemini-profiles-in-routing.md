# Auto and the team can pick Gemini models for project work

Area: Auto
Files: engine/src/route.rs, engine/src/router.rs, engine/src/nudge.rs, engine/src/sibling.rs, engine/src/engine/siblings.rs, engine/src/engine/auto.rs, engine/tests/, docs/product/models.md, docs/product-map.md
Depends: agy-turn-session, agy-quota-reading, team-start-options
Size: about 300 changed lines

Why: with a Gemini driver and quota in place, the last step is choosing it. Auto picks from a fixed table of profiles (`PROFILES`
in `engine/src/route.rs`: 12 rows of model, effort, reasoning score, agent score and cost, measured against Artificial Analysis
and re-verified 2026-10-11), `candidates()` keeps those the account can run, `choose_paced` weighs them by quota pace and a
conversation then stays in its own subscription (`engine/src/engine/auto.rs`). The team `start` of task `team-start-options` takes
`ai: "same" | "auto" | "claude" | "codex"`. `agy models` lists Gemini 4 Argon and Gemini 3.8 Flash at low, medium and high, plus
Claude Opus 5.5 and Sonnet 5.5 and GPT-OSS 120B (those use their own quota bucket in Antigravity).

Build: Gemini rows in `PROFILES` for the Gemini models only, with scores taken from the same source and date as the existing rows
and written in the models page (`docs/product/models.md`) with the cost basis; `candidates` includes them only for Projects and
only when Gemini is connected with a usable reading or an explicit choice; `nudge::next_rung` stays within a vendor; `ai:
"gemini"` is accepted by `start`; the leader's brief for a Gemini member says it has no Faber tools and reports in its last
answer. Auto still routes the small model call with the existing ROUTER pair (Luna, Haiku), never Gemini. No Gemini in Chat in
this task (see the owner question).

Leave out: Chat and the router, Antigravity's Claude and GPT-OSS models (they would spend another AI's quota under another
bucket), effort tables beyond what the recorded CLI accepts.

Done: tests that fail today: `candidates` includes Gemini rows only with the CLI connected and for Projects, a spread test where
three team conversations started under Auto land on all three AIs when each has room and avoid the one out of quota, `start` with
`ai: "gemini"` runs on the fake `agy`, and the table's invariants (one row per model and effort, scores in range) hold. The
`leader_eval` example is run once more to show the leader names `ai` sensibly. Extends: "Auto" and "Conversation started by the
agent" in `docs/product-map.md`, and the models page. What's-new entry in every language ("Gemini can work on your projects when
you connect it"). Review: the other vendor and a Gemini-neutral check of the scores (the table is the owner's taste: cards checked
in the body). Cost: none at rest.
