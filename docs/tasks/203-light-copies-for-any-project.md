# Faster copies for any project: each copy starts from the project's build, with a cap

Area: Separate copy
Files: engine-projects/src/project/copy.rs, engine/src/engine/copies.rs, engine/tests/copy_cleanup.rs, docs/product-map.md
Depends: storage-in-settings
Size: about 500 changed lines

Why: plan item C3, after C1 measures. Faber already shares build output between copies of Faber itself (`docs/dev/build-folders.md`); other people's projects do not, so each copy rebuilds and keeps its own `node_modules` or `target`.

Build:

- See State below; it replaces the first plan.

Leave out: sharing working files.

Done: build time and disk per copy, before and after, measured on a fixture project; a test that a copy whose code
differs never runs the project's binary; tests that two copies building at once stay correct. Stored data and processes: review by the other vendor (R146).

State (2026-11-28): the owner chose: each copy keeps its own build folders (more disk, never another copy's binary).
Sharing `CARGO_TARGET_DIR` between copies is unsafe (cargo judges freshness by modification time and its hashes ignore
the folder), and a per-project `npm_config_cache` saves nothing. So the build is now:

- A new copy starts its own `target` and `node_modules` as a real copy (never a hardlink or junction) of the project's
  when they exist, so it rebuilds only what its folder changes, not every dependency. In the copied `target`, the
  fingerprints of the project's own crates are removed, so they always rebuild from the copy's files; only
  downloaded dependencies are reused.
- A cap and clean-up of copies' build folders (R106), shown in the storage view.
