# Deny agent writes to Faber's whole data folder in every sandboxed mode

Area: none
Files: engine/src/permissions.rs, engine/src/claude.rs, engine/src/codex.rs, engine/src/store.rs, engine/tests/project_security.rs, ui/locales/
Depends: none
Size: about 300 changed lines

Why: the spec's "Trust" section (`docs/superpowers/specs/2026-10-22-own-work-method.md`) says an agent can write inside
the project, so authority must live in Faber's data folder, and slice 1 "denies writes to Faber's data folder in every
sandboxed mode of both CLIs, and tests an attempt from each". Today only two names are protected:
`permissions::FABER_RECORDS` (`decisions-kept.json` and `faber-trust`) become Claude `Edit(//**/...)` deny rules in
`claude.rs` and are refused by name in `touches_credentials`. The rest of the folder (`settings.json`, `budget.json`,
`projects.json`, `conversations/`, `tools/`, `auto-learned.json`) has no rule of its own. Codex's profile
(`permissions::sandbox_config`) denies everything outside the workspace, but no test proves an attempt there either.
Check the current state of both before changing anything; keep what already holds.

Build:

- One place that names the data folder's real path for a launch (the engine knows it as the `Store` data directory;
  `FABER_DATA_DIR` moves it) and turns it into the deny for each CLI: Claude Code deny rules for Edit (which covers
  Write) on that folder, and the Codex profile staying denied for it even when a project lives inside it.
- Reads stay allowed where they are today (the guide, kits and attachments the agent is meant to read).
- A command that names a path in the folder is refused before any card, like a sign-in, for both CLIs, by the existing
  text net in `touches_credentials` (best effort, as its comment says).
- Full access stays as it is (the person handed the agent their account); Settings must say so where full access is
  chosen, in the plain words of the existing permissions text, in every language. Do not weaken any existing warning.

Leave out: a new permission mode, a change to what Faber stores, and the CLIs' own folders (already denied).

Done: engine tests in `engine/tests/project_security.rs` that fail today attempt a write to a file in the data folder
from each CLI's sandboxed modes and are refused, while an ordinary write in the project works; Full access is unchanged
and its text says why. Extends: the product-map rows "`decisions-kept.json`" and "`faber-trust/adoptions.json`" and the
permissions section. What's-new entry in every language. Two reviews, one by the other vendor, because this is security
(R146).
