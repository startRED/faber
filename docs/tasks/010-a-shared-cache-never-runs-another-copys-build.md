# A shared build cache never runs another copy's test build

Area: none
Files: testkit/src/testbuild.rs, scripts/cargo.mjs, docs/dev/testing.md
Depends: none
Size: about 120 changed lines

Why: `scripts/cargo.mjs` lends the same three caches (`target/build-1` to `build-3` of the main checkout) to every
worktree on the PC, one at a time (`acquireTarget`, the `.faber-lease` folder). The lease keeps two runs apart but says
nothing about whose build the cache holds. Cargo decides freshness by file times, and the workspace's crates get the same
artifact names in every worktree, so when this copy's sources are older than the `faber.exe` another copy last linked in
that cache, Cargo calls it fresh and the screen checks launch the other copy's app
(`$CARGO_TARGET_DIR/faber-test/debug/faber.exe`, `testkit/src/app.rs`). `testbuild.rs` (`build_shell`) only checks that
the file exists. Team 2 saw its run use another team's build twice on 2026-10-30, so any screen result on a busy PC can
be about the wrong code.

Build: the test build remembers which copy built it (a stamp next to the app with the worktree's root) and, when the
copy that runs now differs, makes Cargo rebuild the shell for this copy before it is launched (remove the shell's
fingerprints, or the smallest other way that is correct), without throwing away the whole cache. Say in
`docs/dev/testing.md` (build folder budget) that a cache moves between copies and how the test build stays this copy's.

Done: a unit test in testkit that fails today: a stamp from another root makes the build step ask for a rebuild, the
same root does not. One run on the PC where two worktrees take the same cache one after the other and the second
launches its own app (say how you checked it). One review by the other vendor.
