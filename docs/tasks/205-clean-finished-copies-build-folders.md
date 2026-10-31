# Free the build folders of finished separate copies

Area: Projects
Files: engine/src/engine/copies.rs, engine/src/project/copy.rs, engine/src/workspace.rs, engine/tests/conversation_copy.rs, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: each project conversation works in its own copy, and a Rust or Node project fills it with `target/` or `node_modules/` that nobody uses after the work is done; on a small Windows system disk this adds up to tens of GB (the owner's C: filled this way). Faber must never bloat the user's disk.

Build: extend `sweep_copies` so a copy whose conversation has no running reply and has been idle for a set time (for example 7 days) loses only its rebuildable folders, from a fixed short list (`target`, `node_modules`, `.next`, `dist`, `__pycache__`), and only when git ignores the folder. Never the source, never a folder git tracks, never a link (no-follow, as the rest of the workspace code). The copy stays openable; the next run rebuilds. Log the freed size to the engine's own log, and show it nowhere else.

Leave out: a Settings control, deleting whole copies, the user's own project folder (only Faber's copies).

Done: engine tests that fail today: an idle copy loses an ignored `target`, a tracked folder of the same name and a running copy are untouched, and a junction inside is not followed. Extends "Separate copy". Cost: one directory walk per sweep, at background priority.
