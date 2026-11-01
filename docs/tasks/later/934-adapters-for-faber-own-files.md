# Let Faber add to its own decision table, taste cards and task folder

Area: Project memory
Files: engine/src/project/memory.rs, engine/src/store.rs, engine/src/engine.rs, engine/tests/project_security.rs
Depends: adoption-in-the-card
Size: about 400 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Adopt before create". Until the adapters exist an
adopted kind is loaded and never written. Each adapter keeps the format's own IDs, dates and structure and does only its
format's own operations: Faber's R-numbered table (`docs/decisions.md`: append a row), its taste-card table
(`docs/product-rules.md`: append a card), and its `docs/tasks/` folder (add a task file, and delete it when the work
lands). Folding and one-click delete never touch adopted sources. The spec proves them on Faber's own repo only, with no
second copy.

Build:

- A small adapter trait in `engine/src/project/memory.rs` with the three operations above, each writing through a
  handle to the file or to the parent folder, checking before any byte that the final path of the opened handle lies
  inside the project (`real_path`, junctions and `..` resolved), refusing links.
- The R-number row uses the `R-next` placeholder rule of Faber's `docs/next/<branch>.json` instead of taking a number
  (read `docs/dev/workflow.md#numbers`); never invent a number.
- The writes happen only for items the person approved (task taste-lesson-records) and only for an adopted kind.

Leave out: any other project's formats, and moving the product map into the `map` tool (the spec keeps it for a later
step with the owner's yes).

Done: engine tests that fail today append a row, a card and a task file in a temp copy of Faber's layout, refuse a
target that is a link or outside the project, and delete a task file when its work lands; the live proof is a dry run on
Faber's own repo that changes nothing. Extends: the product-map row "`faber-trust/adoptions.json`". What's-new entry
in every language. Two reviews, one by the other vendor, because this is security (R146).
