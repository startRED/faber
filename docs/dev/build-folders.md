# Build folders

Build-cache cleanup and preserving proof artifacts. See [Testing](testing.md) for running checks and build slots.

Each worktree builds into `target/<name>` of the main checkout (`faber-wt-projhome` into `target/projhome`), and Cargo
never deletes old artifacts. The runner (`testkit/src/sweep.rs`, R106) sweeps that `target` before choosing the suites, at
most once an hour (the `target/.faber-sweep` stamp), and prints a line only when it freed space:

1. A `target/<name>` folder with a `CACHEDIR.TAG` whose worktree is gone (no `git worktree list` entry named `<name>` or
   `*-<name>`) and that nothing used for a day is removed whole.
2. In every build profile (a folder with `.fingerprint`, nested ones included), a file in `deps/` that a newer build of
   the same crate and kind replaced more than a day ago is removed, and so is an `incremental/` folder a newer one of the
   same crate replaced. A crate's newest build stays however old it is. Measured on 2026-10-14, `debug/incremental` held
   52 folders for `faber_engine` alone, all but one dead.
3. If `target` is still above 60 GB, whole worktree folders are removed, the least recently used first, until it fits.
   A folder used in the last six hours belongs to a lane still at work and stays (a cold rebuild costs it ten minutes).
   The main checkout's own folders (`debug`, `release`, `faber-test`) and this run's own folder are never removed whole;
   if they alone exceed the cap, the runner says so.

A folder is in use while Cargo holds one of its lock files (`.cargo-lock`, `.cargo-build-lock`, `.cargo-artifact-lock`)
or, on Windows, while a program in one of its profiles runs (Windows refuses to open it for writing). In use, it is
neither swept nor removed. A lane whose sandbox may not write the main checkout builds in `%TEMP%\faber-target\<lane>`
instead; that root is swept by the same rules and cap (R440), since on 2026-10-22 it held 83 GB in 22 folders made that
day. The sweep holds the profile's locks while it deletes, so a build that starts meanwhile waits.
sccache (user-wide, outside the repository) makes the rebuild of a swept dependency cheap.

Build folders are disposable, including a benchmark's executable after its run. Before calling an artifact frozen,
copy the exact executable and any required runtime files to an evidence folder outside both `target` and
`%TEMP%\faber-target`; keep its source revision, SHA-256 hashes and run receipt beside it, and verify the copied hashes.
Keep only the files needed to inspect or reproduce that result, not a whole build cache. A live worktree, a Cargo lock
or a recent build is temporary protection, not evidence retention. Rebuilding after a sweep does not recover the
original executable or prove its identity; report that artifact as lost while preserving the existing results.

