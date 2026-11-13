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
