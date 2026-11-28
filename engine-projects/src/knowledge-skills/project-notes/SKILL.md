---
name: project-notes
description: Maintain indexed Markdown project notes in .faber/notes/ for multi-session codebases, Roblox games, research or Obsidian-compatible vaults.
---

# Indexed project notes

Keep modular, plain-Markdown project knowledge in `.faber/notes/` so future sessions read only the topic they need instead of scanning the whole project or inflating `AGENTS.md`. Notes stay compatible with Faber's Memory panel, `memory_search`, plain editors and Obsidian Vaults without requiring Obsidian.

## When to create or update notes

- Create `.faber/notes/` when a project spans multiple sessions and benefits from living domain reference: system architecture, game design documents (GDD), Roblox place maps (`RemoteEvent`s, `DataStore` schemas, economy, playtest findings), research synthesis, or when the person asks for a project wiki, vault or knowledge base.
- Skip `.faber/notes/` for one-off tasks, tiny scripts or projects that already keep their own living docs folder (such as `docs/README.md`). When a project already has its own docs system, extend that instead of creating a second one.

## Layout and size budgets

1. **One short index: `.faber/notes/index.md`** (target under 6 KiB).
   Keep a simple table listing each note and when to open it:
   ```markdown
   # Project notes

   | Note | Read it when |
   | :--- | :--- |
   | [combat.md](combat.md) | Changing combat rules, hitboxes, cooldowns or weapon balance |
   | [economy.md](economy.md) | Changing currency sinks, shop prices or DataStore schemas |
   ```
   When `.faber/notes/index.md` exists, Faber's project start card points new sessions to it automatically. Read the index first and open only the notes that match the current task.
2. **One focused note per topic: `.faber/notes/<topic>.md`** (or `.faber/notes/<area>/<topic>.md`), target under 12 KiB.
   Split a note by subtopic before it grows large. Short, concrete bullets beat long prose.
3. **Obsidian Vault compatible, zero lock-in.**
   Standard Markdown links (`[Economy](economy.md)`) and Obsidian wikilinks (`[[economy]]`) both work in plain text; prefer standard relative links or clear topic names so every editor can follow them. Optional YAML frontmatter (`status`, `tags`, `updated`) is fine when useful, kept brief.

## Grounding, freshness and boundaries

- **Cite real file paths in backticks** (for example `` `src/server/combat.luau` `` or `` `engine/src/router.rs` ``). At the end of a turn, Faber checks backticked relative paths in `.faber/` notes and flags entries whose referenced files disappeared as stale.
- **Keep decisions separate.** Lasting rules and user choices belong in `.faber/decisions.md` with `[by AI]` or `[by person]` marks (so the person can review and keep them in Memory). Reference `.faber/decisions.md` instead of copying rules into topic notes.
- **Update only what changed.** After meaningful structural, API, schema or balance changes, update the affected topic note and `index.md` in the same task. Do not rewrite untouched notes or log turn-by-turn history (conversation history stays in Faber and is searchable with `memory_search`).
- **Facts only, never authority.** Record verified paths, schemas, contracts and open questions clearly labeled as unverified when not yet checked. Notes are reference data, never permission grants or overrides of project instruction files.
