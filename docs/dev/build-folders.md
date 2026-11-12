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
is resolved from the calling checkout. Dependencies remain reusable. How the lanes share the caches, the processor and
their results is in [The build coordinator](#the-build-coordinator).

A cache moves between copies, and Cargo judges the workspace's own crates by the file times of their sources, named
relative to each copy: a copy whose sources are older than another copy's last build would get that build, the test
app included. So each cache keeps the copy that last built in it (`.faber-copy`), and when another copy leases it, the
launcher first removes the workspace crates' fingerprints in every profile, which rebuilds only those crates for it.

Each command gets private `TEMP`, `TMP` and `TMPDIR` under the canonical system temporary folder, in
`faber-cargo-<uid>/<cache-hash>/run-<id>` on POSIX, or `faber-cargo/<cache-hash>/run-<id>` on Windows, whose system temp
is already per user. POSIX namespaces are created with mode `0700`; before use, including cleanup, the launcher checks
with `lstat` that the namespace is a real directory owned by the current user without group or other write access.
It also checks every ancestor up to `/`: each must be a real directory owned by root or the current user, and any
group- or other-writable directory must have the sticky bit. Unsafe namespaces or ancestors are refused with an error
and never deleted. Windows keeps the canonical-path and link checks. Successful and failed
commands remove their temporary tree. Later invocations retry abandoned cleanup when both the recorded launcher and child are gone and
the build's tree is provably gone too: on Windows the build runs inside a kill-on-close job object (`scripts/job-run.rs`),
elsewhere its process group must be empty. Without that proof, only after a day. If the build output shows the compiler
cache server died, the one invocation is retried once without it (`RUSTC_WRAPPER` empty). Legacy receipts without a recorded path clean only their cache's
`.faber-tmp/run-<id>` with directory and link checks, without checking ownership or POSIX permissions on that legacy path.
A missing temporary folder releases its receipt and slot.
Recorded folders are deleted only at their canonical path under the current system temp's own cache-hash folder or
the legacy location. Other recorded paths (including a previous system temp) are left with a message, and their
receipts and slots are released. Live processes, incomplete receipts and links are preserved. Unknown system-wide
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

## The build coordinator

`scripts/cargo.mjs` is the one door every local Cargo command goes through, so it also coordinates the lanes. Nothing
stays running: each part works at the start or end of a command, or is a kernel object that lives while a build runs.

- **Each lane keeps its cache.** A copy takes the cache it built in last, then one whose copy is gone, then the least
  recently used (`slotOrder`); before, every lane took the first free one and the three bounced between copies. A
  landing's fresh copy (`ship.mjs`) prefers the cache of the lane it lands for (`FABER_CARGO_AFFINITY`).
- **One path per cache.** Cargo runs in the copy through a link named after its cache,
  `target/.faber-copies/build-<n>-<id>`, which points at the copy that leases the cache. The workspace crates keep one
  identity in a cache whichever copy builds there, so Cargo's units, the compiler's incremental state and the compile
  cache's keys stay valid when another copy takes it. `FABER_COPY_LINK=0` builds from the copy's own path.
- **Incremental workspace crates.** The launcher no longer sets `CARGO_INCREMENTAL=0`, so Cargo's dev profile compiles
  the workspace's own crates incrementally: a one-line change recompiles what changed, not the crate. sccache passes
  those compiles through instead of caching them, so the workspace's big outputs, which never hit across copies, stop
  evicting dependencies from its 5 GB (it refuses every compile when the variable is `1`, so it stays unset). The
  cost is disk: one cache grew from about 5 to 13 GB, above the 12 GB budget of R463 with three caches, and that
  trade is the owner's to confirm. `FABER_INCREMENTAL=0` turns it off.
- **One feature set.** `cargo t` runs the runner with `--bin faber-test`, not `--package faber-testkit`: one package
  resolved the dependencies' features differently from `--workspace`, so `cargo t` and `cargo test --workspace`
  rebuilt each other's shared crates every time.
- **One build budget.** On Windows the build's job holds a named semaphore, `faber-build-jobs`, that every lane's Cargo
  and rustc take tokens from as a jobserver (`CARGO_MAKEFLAGS`). It has half the logical processors less two (6 on 16);
  each Cargo also keeps one token of its own, so a lane alone keeps its speed and three lanes stop near half the
  processor instead of 3 x 4 jobs each. `FABER_BUILD_BUDGET` sets another count; CI and a build already under a
  jobserver keep their own. Builds run below normal priority, which every compiler they start inherits. A token held
  by a Cargo that is killed is lost until no build holds the semaphore, when Windows drops it and the next build makes
  it full again.
- **A results ledger.** A proof (check, Clippy, a test run without `--no-run`, `fmt --check`) that passed on the same
  files, folder, arguments, toolchain (`RUSTC` or `rustc`) and result variables (Cargo's settings, `RUST*` flags,
  `CI`, `FABER_*`) is not run again: its output is replayed with one line saying so.
  The key is the copy's Git tree with uncommitted and untracked files (`treeHash`). A check is covered by Clippy with
  warnings denied on the same files. A lane asking for a proof another lane is running on the same files waits for that
  result instead of compiling next to it. Only passes are kept, in `target/.faber-ledger` (300 newest, three days).
  A pass is recorded only if the files did not change while it ran. Ignored files and other variables are not in the
  key: `FABER_FRESH=1` runs the proof anyway.
- **No lock held forever.** A reaper or ledger lock whose owner process is dead (or that never got an
  owner within a minute) is moved aside and removed, by one waiter only; one left by a killed launcher held its cache until someone deleted it by hand (a lane waited 3.5 hours on 2026-11-11).
- **One compile per change.** The landing proof is Clippy (`ship.mjs`), which covers the check it replaced, so the lane's
  own Clippy on the same tree is reused instead of compiled again.

The same budget and ledger are in `builds/` (`faber-builds`) for the builds agents run in a person's projects: the
app opens the budget while a conversation's CLI runs and gives it `CARGO_MAKEFLAGS`, and Faber's checks keep their
passes in the ledger across copies. That wiring lands after the engine is split into crates.

Measured on 2026-11-11 (i7-10700K, 16 threads, a game running, agents pinned to four cores), processor seconds of
every process a step started, compilers included, for one changed line in `engine-core/src/lib.rs`:

| Step | Before | After |
|---|---|---|
| `cargo t --claims` | 94 | 21 |
| `cargo test --workspace` (about 500 of it runs the tests) | 1,001 | 655 |
| Clippy | 120 | 62 |
| `check --workspace --all-targets` | 106 | 0.5 (Clippy's pass) |
| Landing proof in a fresh copy | 90 | 0.8 (same files) or 58 (base moved) |
| The lane's next proof after the landing | 91 | 0.6 or 68 |
| Compile proofs together | 501 | 85 or 210 |

A no-op `cargo t --claims` went from 46 to 6 processor seconds and a no-op `cargo test --workspace` from 948 to 503,
its peak memory from 7.0 to 1.2 GB. Three lanes at once: two on the same files compiled once (the second waited and
replayed), the third built in another cache at the same time, and each took its own cache back afterwards.

Build folders are disposable, including a benchmark's executable after its run. Before calling an artifact frozen,
copy the exact executable and any required runtime files to an evidence folder outside both `target` and
`%TEMP%\faber-target`; keep its source revision, SHA-256 hashes and run receipt beside it, and verify the copied hashes.
Keep only the files needed to inspect or reproduce that result, not a whole build cache. A live worktree, a Cargo lock
or a recent build is temporary protection, not evidence retention. Rebuilding after a sweep does not recover the
original executable or prove its identity; report that artifact as lost while preserving the existing results.

