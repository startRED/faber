# Build folders

Build-cache cleanup and preserving proof artifacts. See [Testing](testing.md) for running checks and build slots.

Local compiling commands use `node scripts/cargo.mjs <arguments>` (or `npm run cargo -- <arguments>`). The existing
`cargo t` alias still works, but direct Cargo bypasses storage management. `npm run dev` and `npm run dist` use the
launcher too; CI keeps using Cargo directly on its disposable checkout.
The launcher needs write access to the main checkout's build folder. Sandboxed lanes without that access retain the
direct Cargo workflow in the legacy temporary build root described below.

The launcher locates the main checkout through Git and leases one of three reusable caches, `target/build-1` through
`target/build-3`, for the entire command, including tests. Concurrent lanes never share an occupied cache; a busy pool
waits rather than making another. `CARGO_TARGET_DIR` explicitly selects another cache, still leased; a relative value
is resolved from the calling checkout. Incremental output is disabled unless the caller explicitly enables it, trading
some rebuild speed for lower disk use. Dependencies remain reusable.

A cache moves between copies, and Cargo judges the workspace's own crates by the file times of their sources, named
relative to each copy: a copy whose sources are older than another copy's last build would get that build, the test
app included. So each cache keeps the copy that last built in it (`.faber-copy`), and when another copy leases it, the
launcher first removes the workspace crates' fingerprints in every profile, which rebuilds only those crates for it.

Each command gets private `TEMP`, `TMP` and `TMPDIR` under its cache's `.faber-tmp/run-<id>`. Successful and failed
commands remove their temporary tree. Later invocations retry abandoned cleanup only after a day and when both the
recorded launcher and child are gone. Live processes, incomplete receipts and links are preserved. Unknown system-wide
`.tmp*` folders are never deleted: their names do not prove Faber owns them.
Stopping a command first stops its process tree. If that cannot be confirmed, its cache and temporary files stay
reserved rather than being reused under potentially live descendants.

Before each local command, a small standard-library Rust helper sweeps output at most once an hour. The executable is
reused by source digest, without compiling the app or its dependencies. The runner uses the same sweep. The 12 GB
budget (the owner, 2026-10-25: all of Faber's builds on one PC under 15 GB) includes the main checkout's `target` and Cargo-tagged `target` folders of all registered worktrees, including
nested ones. A cache nothing used for two hours is cleared, whether its worktree is alive or gone; builds replaced
more than two hours ago go; then idle caches go in least-recently-used order. Caches used within half an hour, the caller's explicit cache, and the main checkout's own profiles stay. This is a soft budget:
if protected builds exceed it, maintenance reports that rather than interrupting work.

The sweep and launcher acquire the same `.faber-lease` directory atomically. Cargo profile locks stay held during
removal and their files remain in place, so another build cannot enter through a replaced lock. Running Windows
programs, cache markers and maintenance records stay. Source worktrees are never removed.

Nothing stays resident or scans on a timer: maintenance runs at command boundaries and temporary cleanup runs when the
command ends. One short-lived Node process supervises a command; the helper streams directory entries, without loading
artifacts into memory. No desktop-app process or idle timer is added. Rust maintenance tests live in
`testkit/src/sweep.rs`; launcher checks run with `npm run test:scripts`.

The legacy `%TEMP%\faber-target` root remains covered by the same hourly sweep and its own separate 12 GB cap.

Build folders are disposable, including a benchmark's executable after its run. Before calling an artifact frozen,
copy the exact executable and any required runtime files to an evidence folder outside both `target` and
`%TEMP%\faber-target`; keep its source revision, SHA-256 hashes and run receipt beside it, and verify the copied hashes.
Keep only the files needed to inspect or reproduce that result, not a whole build cache. A live worktree, a Cargo lock
or a recent build is temporary protection, not evidence retention. Rebuilding after a sweep does not recover the
original executable or prove its identity; report that artifact as lost while preserving the existing results.

