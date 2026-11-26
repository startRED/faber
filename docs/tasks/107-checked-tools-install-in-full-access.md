# In Full access, the AI installs a checked catalog item the task needs without a card

Area: Faber's catalog search and install
Files: engine/src/engine/toolbox.rs, engine-projects/src/toolbox.rs, engine/tests/toolbox.rs, docs/design/catalog.md, docs/product-map.md, docs/next/
Depends: none
Size: about 350 changed lines

Why: plan item D1 ("install by itself what is checked") and the owner's answers 5 and 6 of 2026-11-25: the AI operates
Faber in everything except signing in, spending money and changing its own permissions, trust and hooks; an unchecked
item still asks once (R33 unchanged). Read on 2026-11-25: a project agent's `catalog` `install` asks for consent
`faber.tools.install` unless `faberTrust` is on (off by default, `engine/src/engine/toolbox.rs`; product-map row
`faberTrust`). Voice runtimes already download on first use (`fcebb46ef`); the Roblox pipeline (`94c559d6a`) added
copyable requests and a skill, not a general installer. `tools.json` has no `checked` field: an entry in it is the
curated list (R33), and `digest` pins what is downloaded.

Build:

- When the conversation runs in Full access, a project agent's install of an item that is in `tools.json` runs with no
  card and is logged with the conversation that asked (task Catalog shows "installed by the AI in …" later). Other
  permission modes keep the card. Unchecked items keep asking once (R33). Removal, switching, updates and repair keep
  their own consent: this task is install only.
- A decision row in `docs/next/<branch>.json` (`R-next`) citing the owner's answers 5 and 6 of 2026-11-25.

Leave out: kits by goal (task faber-kits-by-goal), open search (D2), the Catalog screen (D3), and the wider "Full access
runs what asks today" of the next wave (F2).

Done: tests in `engine/tests/toolbox.rs`: in Full access a checked install needs no card (fails today); in Accept edits
it still asks; an unchecked item asks in every mode; the log names the conversation. Extends `docs/design/catalog.md`
and the product-map rows `Faber's catalog search and install` and `faberTrust`. Permissions: review by a fresh session
of the other vendor; it touches what runs without asking, so two reviews from different vendors (R146).
