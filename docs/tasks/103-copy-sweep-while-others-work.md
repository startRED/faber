# Copies whose work is already in the project leave even while other conversations run

Area: Separate copy
Files: engine/src/engine/copies.rs, engine-projects/src/project/copy.rs, engine/tests/copy_cleanup.rs, docs/product-map.md
Depends: none
Size: about 350 changed lines

Why: plan item A3 (the app side). The owner's data folder holds 3.1 GB of copies in 73 folders. Read on 2026-11-25:
`sweep_project_storage` (`engine/src/engine/copies.rs`) retires a copy that is idle, clean and `merged`, and removes the
orphan of a deleted conversation, but it starts only when `project_storage_idle()` holds for the whole engine (no agent
install, no running goal, no copy being made, verifier idle and no game in front, and every conversation's session
idle) and stops at the first moment that is false. Someone who always has one conversation running never gets a sweep.
`copy::cleanup` (7 days) only clears ignored build folders. An archived conversation's copy follows the same rules as any
other: archiving does not authorize removing it (R560 keeps unique work).

Build:

- Per-copy idleness for the retire step: a copy is retired when its own conversation is idle, clean and merged, whatever
  the other conversations do; the global conditions that protect the disk and the person stay for the parts that need
  them (no copy being made, no game in front, background priority, the set-aside under the lock).
- A count the person can be told: how many copies stay because their work is not in the project yet, how many because
  they are busy, how many left (returned by the sweep, logged once per sweep; no screen here, task storage-in-settings
  shows it).

Leave out: removing a copy whose work is not merged, archived or not (that needs the owner's word against R560: the
night report asks him); the dev-side cleanup (task dev-space-report).

Done: tests in `engine/tests/copy_cleanup.rs` with a fake git runner: a merged idle copy is retired while another
conversation is running (fails today); a copy with unmerged work stays; a copy whose conversation gets a message during
the sweep is not removed. Before and after on the owner's folder (count and GB), measured read-only, in the commit body.
Extends the product-map rows `Separate copy` and `Separate copies (R201)`. Stored data and processes: review by a fresh
session of the other vendor (R146).
