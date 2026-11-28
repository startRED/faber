# Efficient project work

The same workflow applies to every project on every supported PC, including Faber developing itself. It uses the
person's installed Git and package managers; no replacement version-control service, bundled tool or special PC setup.

## Copies and storage

Project conversations already use separate Git worktrees when available. They share Git objects with the original
repository while retaining their own files, branch and index. Agents are instructed to use that working folder rather
than clone the whole repository again, preserve local edits and unique commits, and read the current project rules
and base before starting. They never silently update the person's checkout.

At startup and idle turn boundaries, at most once per engine and hour, the engine revisits Faber-owned conversation
copies and performs incremental Git housekeeping for registered projects. Linked copies share one maintenance pass
per common Git directory. A new active reply, check, install or detected foreground game prevents another pass from
starting. There is no idle timer, watcher, scheduler entry or permanent process.

The native Git tasks are `loose-objects`, `incremental-repack` and `commit-graph`, with `--auto` thresholds. They do
not fetch, push, expire reflogs, prune history or edit working files. Hooks, credential helpers and lazy network fetch
stay disabled. Git runs in a contained process tree, with one packing thread, a 16 MiB packing-window budget and a
60-second deadline. Loose objects are packed in batches of at most 5,000 rather than Git's default 50,000, so a large
repository can progress across idle passes. Inventory scanning and incremental repacking are still best-effort within
the deadline; batch size is not a time guarantee. Windows also requests a 256 MiB job limit and below-normal process
priority; other platforms use the existing process-group containment without claiming those Windows resource limits. Missing Git, refused paths,
unsupported tasks and a failed maintenance pass never stop a conversation.

Existing copy-retirement safeguards still apply: only an idle Faber-owned copy whose work is merged and clean is
automatically retired. Unknown unique work and uncommitted changes stay. Rebuildable ignored folders are reclaimed
only in a stopped copy unused for seven days, with the existing path, tracked-file and idle checks. Source worktrees
outside Faber's data folder are never removed by this sweep.

A new copy starts its own `target` (beside a `Cargo.toml`) and `node_modules` (beside a `package.json`) as a real copy of
the project's, on a thread of its own after the copy is made, so its first build reuses the dependencies already built
(task 203, the owner's choice of 2026-11-28: each copy keeps folders of its own, never a shared one). The copy keeps file
times, and in a copied `target` everything the project's own packages left (fingerprints, build-script output, finished
programs) is removed, so they always build again from the copy's files. Only the copy's top folders are seeded. Each is staged
beside the copies, one at a time in every Faber on the data folder, and moved in with a move that never replaces: a
build the agent started first keeps its own, even an empty folder. `node_modules` comes only when the copy's lockfile
matches the project's, checked again right before the move, and `target` only when every manifest of the project and
the copy reads. A folder above 8 GB or the room above 10 GB free (counted again once it is copied), any link but npm's `.bin` links on macOS and Linux,
or a file that is not a plain file seeds nothing. Staging folders a crash left are swept with the copies, never while
a seeding holds the lock beside them. A small Rust project's first build in a new copy: 8.0 s and 30 crates
without, 1.8 s and 1 crate with.

## Checks and dependencies

The existing `verify` tool is available in Projects. Its fast mode selects affected checks and reuses successful
results on the same watched project inputs; full mode runs all required checks without reusing passes. Missing file
hashes disable reuse, and passes are scoped to the working folder. The cache does not fingerprint external changes
to environment variables, tool binaries or installed dependency contents: agents must use full verification after
those changes and before delivery, rather than trust a previous fast pass.

Both project drivers tell agents only what changes their own work: stay in the working folder instead of cloning,
use verify fast while iterating and verify full before delivery, preserve local edits and unique commits, and keep
the project's CI, required checks and permissions. Caches, copy retirement and Git upkeep are Faber's job and stay
out of the prompt, so a project task is not steered toward them. These instructions do not prove that an external
tool or agent followed every recommendation.

The project's own instructions, CI and permission mode remain authoritative. Faber does not replace another
project's pipeline or install tools automatically. When authorized to optimize one, the agent reuses its existing
checks, keys caches by their real inputs and removes redundant setup without dropping required validation.

## Evidence

`engine/tests/git_maintenance.rs` covers shared-directory deduplication, idle admission, missing or unsupported Git,
path refusal and resource/command pins. `engine/tests/copy_cleanup.rs` covers ordinary projects with no conversation
and preservation of running or unsaved copies. `engine/tests/persona.rs` covers delivery of the portable workflow to
Projects without adding it to Chat. Existing verifier and driver tests continue to cover their own behavior.

Faber's development delivery additionally uses a temporary detached worktree instead of a complete clone
([landing](../dev/landing.md)). A local Windows comparison of the same revision measured about 100 seconds for the
old clone plus checkout and 2.1 seconds for the worktree plus checkout, eliminating a 1.08 GB object copy. This is one
preparation measurement, not a measured reduction in full delivery time, model latency or every user's disk usage.
