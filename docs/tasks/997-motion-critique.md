# Every animation of Faber looked at, scored and polished

The owner asked for every animation to be analysed, improved, and added where one is missing (2026-10-21). The motion
rules live in `docs/design/looks.md` ("Motion") and the checklist in `.claude/skills/faber-ui/SKILL.md`; the motion rows
of the product map are done (R209, `tests/ui/motion.check.ts`), but no visual critique has scored them.

- List each moving surface (panels, menus, cards, the composer, the running column, the sidebar, the catalog, first
  run), play it slowed down in the test build (0.25x, `docs/dev/testing.md`), and judge it against the faber-ui list:
  why it moves, tokens, reduced and none paths, exit shorter than entrance.
- Fix what breaks a rule; add motion only where a state jumps or a place is lost, never for style.
- A fresh session runs the visual critique (`.claude/skills/visual-critique/SKILL.md`) on the touched screens.

Done means the list with a verdict per surface in `docs/design/looks.md` or a research note, the fixes with checks, and
the critique's scores in the pull request.
