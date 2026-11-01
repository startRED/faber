# A catalog tool whose pinned commit changed is downloaded again, not silently dropped

Area: Toolbox
Files: engine/src/tools.rs, engine/src/catalog.rs, engine/tests/tools.rs, ui/locales/, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: `engine/src/tools.rs:467` says "a catalog that pins a newer commit drops the old copy here; reinstall it on update once one exists." Today an installed tool disappears from the CLIs after the catalog moves its pin, and nothing brings it back. First check how `verify_item` fails for a moved pin.

Build: when `prune` discards a tool only because its pinned commit differs from the catalog's, record it as needing an update. The next toolbox pass downloads it again from its original source at the new pin, through the same checked install path, with no new way to install. The person's enable state is kept. On a failed download the tool stays off and its Toolbox row says the update failed (a key in `ui/locales/`).

Done: an engine test with a fake source that fails today: install at pin A, move the catalog to pin B, run `prune`; the tool is back at B with the same enable state. A second test: a failed download leaves it off with the reason key. Extends "Toolbox". Cost: one download per changed pin.
