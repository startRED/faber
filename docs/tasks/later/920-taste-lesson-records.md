# Write taste and lessons only through Faber, with a record of the person's yes

Area: Project memory
Files: engine/src/project/memory.rs, engine/src/store.rs, engine/src/permissions.rs, engine/src/engine.rs, engine/tests/project_security.rs
Depends: none
Size: about 350 changed lines

Why: slice 3 of `docs/superpowers/specs/2026-10-22-own-work-method.md` (approved as of commit 4fc1324d, R435). `taste.md`
(1 KB) and `lessons.md` (2 KB) are proposals until the person says yes. Only the engine writes them, after the yes, with a
provenance record in the data folder; the agent never writes them directly, and a line without a matching record is an
unreviewed proposal. Today only decisions have this (`decisions-kept.json`, `memory::apply` with `keep`).

Build:

- A record store beside `decisions-kept.json` in the data folder (a new file under `faber-trust/`, protected like the
  other records by `permissions::FABER_RECORDS` / the data-folder denial): per project id, the hash of each approved
  taste or lesson line. Damaged records read as nothing approved, never as an error that blocks work.
- An engine function that appends one approved line to `.faber/taste.md` or `.faber/lessons.md` through the existing
  no-follow write path (`write_faber_file` / `edit`), records its hash, and refuses a line that would pass the file's
  target without the person choosing to (the soft target of task over-limit-in-memory, when it exists).
- Reading: an item whose hash has no record is listed as "not reviewed" and never goes to the card.
- Add both files' names to the agent's write refusal: a file change or command by the agent that targets
  `taste.md` or `lessons.md` is refused before any card (as the records are), so only the engine writes them.

Leave out: how a proposal is shown (task proposals-on-the-closing-box) and deleting (task delete-memory-item).

Done: engine tests that fail today show the engine writing a line and its record, an agent write refused for both CLIs,
a hand-edited line shown as not reviewed, and a damaged record file harmless. Extends: the product-map rows "Project
instruction and learning files" and "`decisions-kept.json`". The commit body states the cost (none while idle). Two
reviews, one by the other vendor, because this is security (R146).
