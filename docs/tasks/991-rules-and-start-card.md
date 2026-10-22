# A project's rules, start card and trust (own work method, slice 1)

Build slice 1 of `docs/superpowers/specs/2026-10-22-own-work-method.md`. The owner approved that file as of commit
4fc1324d (R435). If the design must change, ask the owner first.

## What to build

Read the spec's sections "Building `rules.md`", "Trust", "The start card", "Adopt before create" and "Cost".

- **`rules.md`.** The engine builds `.faber/rules.md` from fixed fields only: manifests, workflow command names, the
  "Proof commands" heading, and instruction file names. It records the file's hash and sources in the data folder, and
  rebuilds it when a turn starts and a source hash changed.
- **Provenance.** Every person-kept or person-approved item gets its ID and content hash recorded in the data folder,
  extending `decisions-kept.json`. A forged `[by person]` mark counts as an unreviewed AI item.
- **The start card**, for both CLIs (`engine/src/claude.rs`, `engine/src/codex.rs`, after `persona()`).
  - It is at most 6 KB, with shares of 3 KB, 2 KB and 1 KB.
  - It carries no repository free text: the description and unreviewed items reach the agent only through Faber's
    tools.
  - Overflow is counted, never hidden.
  - Technical details shows the card as sent.
- **The data folder.** Writes to it are denied in every sandboxed mode of both CLIs (`engine/src/permissions.rs`), with
  a test of an attempt from each.
- **Adoption pointers.** They live in the data folder, read-only until slice 4.
  - Containment is checked on the opened handle at every open.
  - Before the first write, the person reconciles existing items once.
- **Kits.** The stack facts feed the session-kit matcher. A check shows that a project's kit is unchanged or better.
- **Hostile-repository checks:** injected text in the source files, a forged mark, and a planted file read by a later
  session.

## Gates

- **Tokens.** Run the spec's token benchmark: the R410 requests, with and without the card, on both CLIs, in cold, warm
  and resumed sessions. The gate is the one the spec declares. If no card passes, ask the owner.
- **Memory and processor.** At most 2 MB extra peak and 20 ms per session start, on the spec's fixtures.

## Done means

AGENTS.md "Done means":

- the product map, including the `.faber/` files and the data-folder records;
- `docs/design/` for Technical details;
- a what's-new entry in every language;
- a check that fails without each part;
- the measured cost in the commit body;
- two reviews, because this is security (R146).
