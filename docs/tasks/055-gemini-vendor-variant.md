# Faber knows a third AI, Gemini through Antigravity, though nothing can start it yet

Area: Accounts
Files: engine/src/model.rs, engine/src/ledger.rs, engine/src/account.rs, engine/src/router.rs, engine/src/helper.rs, engine/src/mcp_servers.rs, engine/src/tool_inventory.rs, engine/src/engine.rs, engine/src/engine/, ui/api.ts, docs/product/README.md, docs/product-map.md
Depends: none
Size: about 380 changed lines

Why: the owner's request names Gemini (Antigravity) as the third AI the leader spreads work over. The code is built for two:
`pub enum Vendor { Claude, Codex }` and `Vendor::ALL: [Vendor; 2]` (`engine/src/model.rs`), the ledger has one field per AI
(`claude`, `codex` in `engine/src/ledger.rs`, read by `of()`/`of_mut()`), about twenty `match` expressions list both
(`account.rs` login and status arguments, `mcp_servers.rs`, `tool_inventory.rs`, `router.rs` `pick_model`, `toolbox.rs`,
`engine/src/engine/{quota_watch,pick_up,helpers,auto,aside}.rs`, `src-tauri/src/setup.rs`), `helper::other` returns "the" other
vendor, and `ui/api.ts` mirrors the type. The product pages say the opposite of the owner's request:
`docs/product/README.md` has the principle "Only Claude Code and Codex" and the non-goal "Vendors other than Claude Code and
Codex: no Gemini ...", and the decision of this direction is `R-next` (the owner's, amending that non-goal).

Build: add `Vendor::Gemini` (name `gemini`; the person-facing name is "Gemini"; the CLI is Antigravity's `agy`), `Vendor::ALL` of
three, a `gemini` ledger entry that older files read as empty (`#[serde(default)]`, so every saved ledger and budget file opens in
any version), and every exhaustive match handled: Gemini is never "connected" (no CLI is discovered for it yet), never routed
and never offered, so each match arm for it is a documented "not yet" that returns nothing, and the code that walks `Vendor::ALL`
skips vendors with no CLI (check each of the 14 uses). Starting a session for it (`engine/src/engine.rs`, the `match pick.vendor`)
returns a plain error key, never a panic. `ui/api.ts` gets the type with the same name. Edit `docs/product/README.md`: the
principle becomes "Claude Code and Codex, and Gemini through Antigravity's CLI for Projects", the non-goal loses Gemini, and
`docs/product-map.md` gets a row "Gemini (Antigravity)" saying what is built so far (the variant; nothing runs yet) that the later
tasks update. If the change passes 400 lines, split the ledger first and say so in the pull request.

Leave out: discovering the `agy` CLI, any session, quota reading, routing table, screen marks (the tasks that follow).

Done: tests that fail today: `Vendor` round-trips `"gemini"`, an old ledger file without the field loads, `Vendor::ALL` has three,
`pick_model` and the helper never choose Gemini when no CLI is known, starting a Gemini session returns the error key. The
existing suites (`engine/tests/`, `src-tauri` setup tests) still pass. Extends: the Accounts rows of `docs/product-map.md` and
`engine/tests/product_map.rs` stays green. No what's-new entry (nothing a person sees). Review: the other vendor. Cost: none
(no process, no memory).
