# A round of fresh AIs using Faber as newcomers, and fixing what confuses them

The owner wants the first stable version tested by AIs acting as people who know nothing of Faber (2026-10-23). The
last pass is `docs/research/first-user-2026-10-19.md`, before the catalog and the own work method landed.

- Run `node scripts/first-user.mjs` on the first run, a Chat turn, a project conversation, the Catalog tab and Settings,
  at 720x480 and 1280x800, with Haiku and with Codex `gpt-6-luna` (two different eyes), in pt-BR and English.
- Sort the findings: what blocks a newcomer, what confuses, what is taste. Fix the first two kinds, each with a check
  that fails without the fix; send taste to the owner as one short card.

Done means a new `docs/research/first-user-<date>.md` with the findings and what became of each, the fixes landed, and
a second pass that no longer finds the blocking ones.
