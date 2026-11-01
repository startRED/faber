# Give a project's agent the start card at every session start

Area: Project memory
Files: engine/src/project/rules.rs, engine/src/claude.rs, engine/src/codex.rs, engine/src/persona.rs, engine/src/store.rs, engine/tests/persona.rs
Depends: none
Size: about 350 changed lines

Why: slice 1 of `docs/superpowers/specs/2026-10-22-own-work-method.md` (section "The start card"; approved as of commit
4fc1324d, R435; a design change goes to the owner first). Today `.faber/rules.md` is rebuilt at each turn start
(`rules::refresh`, called from `engine/src/engine.rs`) and `rules::card` / `start_card_sections` format the card in fixed
3 KB, 2 KB and 1 KB shares, but nothing sends it: the product map says "Not yet connected to session loading".

Build:

- In `engine/src/claude.rs` (the `--append-system-prompt` words, after `persona()` and `PROJECT_DETAILS`) and in
  `engine/src/codex.rs` (the developer instructions, same place), append `rules::card(...)` for a project conversation
  (`Area::Project`) at each session start and resume. Never for Chat, helpers or quick questions.
- Its facts come from `rules::facts` of the conversation's folder. Its decisions are only those whose hash matches
  `decisions-kept.json` (the person's record, kept by `Store`), newest first; its taste lines are only kept ones. Until
  the taste task lands there are none, so pass an empty list.
- The card carries no repository free text: the description stays out, as `card` already does. An item over 500
  characters loads as its first 200 and an ellipsis (already in `card`). Overflow is counted in the card, never hidden.
- A session never rebuilds its own card: it is built when the session starts and kept for its resume.
- Keep the built text a field of the launch, so a test can read what both CLIs would receive.

Leave out: the Technical details view of the card (task card-in-technical-details), adopted sources (task
adoption-in-the-card), and any change of what the card holds.

Done: engine tests that fail today show the card in the arguments of both CLIs for a project conversation, absent for
Chat and helpers, never containing the manifest description or an unmatched `[by person]` decision, and at most 6 KB;
the commit body states the measured processor time per session start (at most 20 ms) and extra peak memory (at most
2 MB) on the spec's fixtures. Extends: the product-map row "Internal start-card formatter" (rewrite it as built) and
"Project instruction and learning files". What's-new entry in every language. Two reviews, one by the other vendor,
because repository text sits next to the system prompt (R146).
