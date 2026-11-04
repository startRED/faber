# A tool server with dependencies installs from a pinned, checked tree

Area: Catalog
Files: engine/src/tools/, engine/src/tools.json, engine/tests/tools.rs, engine/tests/kits.rs, scripts/, docs/research/2026-10-21/catalog-list.md, tests/ui/catalog.check.ts
Depends: none
Size: about 600 changed lines

Why: the owner wants the first stable version to have tool servers in the catalog (2026-10-23), and R253 admits only
npm packages with no dependencies. On 2026-11-04 #671 checked ten servers a software project uses every day (browser
automation, the code host, docs lookup, databases, files, memory, step-by-step thinking) and none passed: every one has
runtime dependencies. So the catalog keeps one server, and the rule, not the research, is what blocks the rest.

Build:

- First check what happens now: how `chrome-devtools` is pinned, downloaded, checked, stored and re-checked at launch.
- A server entry may carry its whole runtime dependency tree, each package pinned by name, version and the registry's
  `sha512` integrity. A script in `scripts/` writes that list from registry.npmjs.org metadata when a server is curated,
  never at install time. The install downloads each tarball from registry.npmjs.org only, checks every integrity, lays
  the tree out as `node_modules` inside the server's store folder, and runs no install scripts. It keeps the same store
  lock, ledger and launch-time re-check, with the digest taken over the whole folder.
- Refuse the whole server, with an `error.*` key, when any package in the tree has an install script (`preinstall`,
  `install`, `postinstall`), a native addon (`binding.gyp` or a `.node` file), a host other than registry.npmjs.org, or
  an integrity that does not match. Also refuse when the tree passes a size cap measured on the candidates and written
  in the R545 row.
- Then admit, under the same rules as `chrome-devtools`, the candidates from the #671 note that pass now. Each must be
  free and need no key, and no account Faber would have to store. Show each licence and give each a Chat declaration
  (R373).
- Record the change as an R545 row that amends R253. Its author is IA (Erick delegated).

Leave out: servers that need a key or an account, plugins, and any install path that runs package scripts.

Done: a fake-source check in `engine/tests/tools.rs` that installs a server with a two-level tree, starts it and removes
it. Refusal checks for a bad integrity, an install script, a native addon and another host, each failing without its
guard. A session-kit check for each admitted server, the catalog count test and `tests/ui/catalog.check.ts` updated.
Extends: the product-map rows "Catalog" and "Session kits". What's-new entry in every language. Security change: two
reviews, one by the other vendor, before queueing (R146).
