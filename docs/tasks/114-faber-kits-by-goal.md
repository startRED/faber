# Faber Kits: the checked tools grouped by goal (site, game, video, voice)

Area: Catalog
Files: engine-projects/src/tools.json, engine-projects/src/tools/kits.rs, engine-projects/src/tools.rs, engine/tests/tools.rs, docs/design/catalog.md, docs/product-map.md
Depends: checked-tools-install-in-full-access
Size: about 400 changed lines

Why: plan item D1, second half. Once a checked item installs without a card in Full access, the AI still has to know
which items a goal needs. Read on 2026-11-25: `engine-projects/src/tools.json` lists 118 skills, 4 servers and 11 local
audio runtimes; `tools/kits.rs` builds each session's kit under 16 KB of context and 32 KB of discovery.

Build:

- A small set of kits by goal in the catalog data (site, game, video, voice), each a list of checked item ids already
  in `tools.json`, with one line saying what it is for.
- The AI's catalog search answers a goal with its kit, one short line per item, inside the existing budgets.
- No new installer: installing goes through the previous task's path.

Leave out: open search outside the list (D2) and the new Catalog screen (D3).

Done: tests in `engine/tests/tools.rs`: every kit item exists in `tools.json`; a goal search returns its kit within the
budget (fails without the code). Extends `docs/design/catalog.md` and the product-map row `Catalog`. Ships on its
proofs unless it changes what installs without asking, which the previous task owns.
