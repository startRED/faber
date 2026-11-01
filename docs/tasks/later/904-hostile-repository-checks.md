# Prove a hostile repository cannot steer the agent through Faber's files

Area: Project memory
Files: engine/tests/project_security.rs, engine/tests/persona.rs, engine/src/project/rules.rs, engine/src/project/memory.rs
Depends: start-card-in-sessions
Size: about 300 changed lines

Why: the spec's "Trust" section (`docs/superpowers/specs/2026-10-22-own-work-method.md`) lists three checks slice 1 adds
for a malicious repository: injected text in the files `rules.md` is built from, a forged `[by person]` mark, and a
later session reading a planted file. Pieces exist (`rules_take_only_fields_of_a_checked_shape_from_the_project`,
`project_memory_keep_stores_the_hash_outside_the_file_and_binds_it_to_the_words`), but no test follows hostile text from
the repository to what the CLIs would receive once the card is sent.

Build, as tests that follow the whole path with the fake CLI (no real model call):

- A manifest description, a workflow step name and an instruction file heading that carry instructions ("ignore the
  rules", markdown fences, a fake `faber-start-card` fence) never reach the card, and a workflow line that is not a
  plain command is dropped.
- A `decisions.md` entry marked `[by person]` that the record in `decisions-kept.json` does not match is shown as not
  reviewed and is absent from the card; the same words reworded after a Keep read "not reviewed" again.
- A file planted under `.faber/` (or a link in place of `.faber` or `rules.md`) by an earlier session is refused or
  rewritten by the next turn's rebuild, and its text reaches the agent only as the result of `memory_search`, quoted as
  data.
- Fix what a test finds, in the smallest change; a fix that needs a design change goes to the owner as a card.

Leave out: new features, and the sandbox denial of the data folder (task data-folder-write-denial).

Done: tests that fail if any of the three paths leaks (prove it by reverting each guard once), named in the product-map
rows they extend. Extends: "Project instruction and learning files" and "Faber's project memory search". Two reviews,
one by the other vendor, because this is security (R146).
