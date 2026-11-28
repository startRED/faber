# Lighter copies for any project: shared build folders with a cap

Area: Separate copy
Files: engine-projects/src/project/copy.rs, engine/src/engine/copies.rs, engine/tests/copy_cleanup.rs, docs/product-map.md
Depends: storage-in-settings
Size: about 500 changed lines

Why: plan item C3, after C1 measures. Faber already shares build output between copies of Faber itself (`docs/dev/build-folders.md`); other people's projects do not, so each copy rebuilds and keeps its own `node_modules` or `target`.

Build:

- Shared, capped build and dependency folders across copies of the same project where the tool supports it safely (npm cache, cargo target dir), with the cap and clean-up of R106.
- A copy can never break another's folder: share only what the tool treats as a cache.

Leave out: sharing working files.

Done: before and after disk per copy measured on a fixture project; tests that two copies building at once stay correct. Stored data and processes: review by the other vendor (R146).

State (2026-11-26): blocked before any code; the owner chooses the direction.
- Sharing `CARGO_TARGET_DIR` between copies is unsafe. Two copies of one crate in different folders built into one target dir, and the second build silently reused the first copy's binary: cargo judges freshness by modification time, and its hashes ignore the folder. `docs/dev/build-folders.md` avoids this for Faber's own copies by clearing fingerprints around a cargo call Faber wraps. Faber does not wrap the cargo call of a person's project, so it has no safe point to do that.
- A per-project `npm_config_cache` saves nothing: npm's cache is already one global folder, and `node_modules` must stay per copy.
- Options: seed each copy's own target from a base build (hardlinks are unsafe if cargo writes in place; NTFS has no reflinks); only cap and clean what exists; or wait for cargo's checksum freshness to stabilise.
