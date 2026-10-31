# Every Settings section works with the keyboard alone

Area: Settings screen
Files: testkit/src/screen/settings.rs, tests/ui/settings.check.ts, ui/Settings.tsx, ui/Accounts.tsx, ui/controls.tsx, ui/Appearance.tsx, ui/ProjectSettings.tsx, ui/styles.css, docs/design/settings.md, docs/product-map.md (change only the components the checks implicate)
Depends: none
Size: about 200 changed lines; mostly checks, plus the fixes they find

Why: the first-user review of 2026-10-25 (`docs/research/first-user-2026-10-25.md`) lists "full keyboard traversal" as not
proven. The first run already has keyboard checks (`tab_to` and `focus_on_next` in `testkit/src/screen/first_run.rs`), but
no Settings check presses Tab: none of `tests/ui/settings.check.ts` or `testkit/src/screen/settings.rs` does. Settings is
where a newcomer connects an AI, picks a language and changes access, so someone who cannot use a mouse must reach every
section and control there.

Build:
- A screen check in `testkit/src/screen/settings.rs`, on the hidden test build: open Settings with its shortcut, move
  through every section of the side list with the keyboard, and in each section Tab through every control: each one gets
  focus in reading order, shows a visible focus ring (`:focus-visible` outline not `none`, non-zero size), can be changed
  with Enter, Space or the arrow keys as its role expects, and Esc leaves a menu or dialog with focus returned to what
  opened it. Run it at 720 px wide and in the longest language too.
- A `tests/ui/settings.check.ts` check for the parts a page test can see faster: the order of focusable controls in
  each section, and no focusable element hidden from view.
- Fix what the checks find in `ui/Settings.tsx`, `ui/Accounts.tsx` and `ui/styles.css`, using the existing focus styles
  (`.claude/skills/faber-ui/SKILL.md`). A fix that needs a new control or new words goes to a follow-up task instead.

Leave out: the first run (covered), the work screens, screen-reader wording beyond labels the checks find missing.

Done: the new checks fail on the first control they find unreachable or without a visible focus, and pass after the
fixes; if no defect exists, show the guard by making one covered control unreachable or removing its focus ring for one
run, the check failing, then restore it. Extends: "Settings screen" in
`docs/product-map.md`; update `docs/design/settings.md` if a focus behaviour changed. What's-new: one "better" line in
every language only if a fix changed what a person can do. Review: coherence review (cards checked) and a fresh read-only
session. Cost: none at run time; one more screen scenario in CI (state its seconds).
