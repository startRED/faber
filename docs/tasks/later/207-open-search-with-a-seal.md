# Search skills and servers beyond the list, with the checked ones first

Area: Faber's catalog search and install
Files: engine-projects/src/toolbox.rs, engine/src/engine/toolbox.rs, engine/tests/toolbox.rs, docs/design/catalog.md, docs/product-map.md
Depends: faber-kits-by-goal
Size: about 500 changed lines

Why: plan item D2 and the owner's answer 5 (R33 stays: an unchecked item asks once). Open search endpoints: `https://skills.sh/api/search?q=` and `https://registry.modelcontextprotocol.io/v0.1/servers?search=`; skills.sh terms for automated use were not found, so check them first.

Build:

- One search tool for the AI with one-line results in three shelves: Faber Kit (checked), open world, mine.
- Unchecked results are marked and install only after one yes.

Leave out: installing unchecked items without asking.

Done: tests with recorded responses for ranking and marking; a network failure falls back to the checked list. Security (third-party code): two reviews from different vendors (R146).
