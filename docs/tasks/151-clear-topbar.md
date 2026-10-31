# Keep the branch out of the topbar and name its icons

Area: Topbar
Files: ui/App.tsx, ui/Details.tsx, ui/styles.css, testkit/src/screen/app.rs, testkit/src/screen/real_work.rs, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/shell.check.ts, tests/ui/details.check.ts, docs/design/layout.md
Depends: none
Size: about 280 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3: the project header shows the copy's branch name
("faber/https-github-com-pingdotgg-185a"), and three header icons have no visible label. Verified in `ui/App.tsx`: the
crumb adds a quiet `copy` span with `copy().branch` after the title (near lines 1763 to 1775, `crumb.copy` in its
tooltip, test id `crumb-copy`); the right side holds three `iconbtn` buttons with only an `aria-label` and a `title`:
the side chat (`pane-aside`, `aside.title`), the changes (`pane-changes`, `changes.title`) and the browser
(`pane-site`, `site.browser`), after the context meter.

What to build:

- Done in its first pull request: the crumb keeps the title only, and the branch and path are the first row of
  Technical details ("Cópia separada: <branch>", test id `details-copy`, checked in `tests/ui/details.check.ts`).
- The three pane buttons get a visible short label or one labelled menu, also at 720 px: this is a taste choice (a
  label per icon fills the bar at 720 px; one "Painéis" menu hides three actions behind a click), so it waits for
  the owner's pick between the two. Whatever is chosen keeps the shortcut hints in the tooltip (`keysOf('aside')` and the others) and
  the pressed states.

Check the product map's "Topbar" row, the "Technical details" row and the taste cards whose "Touches" name the topbar
first: extend, never a second way.

Leave out: the context meter and the quota bars (task `meters-say-what-they-measure`), the title itself (task
`short-title-from-the-first-exchange`), new buttons.

Done: a screen check in `tests/ui/shell.check.ts` fails today because the branch name is in the crumb and the icons have
no text; it passes with the branch only inside the details pane, every pane button reachable by its name, and no
overlap at 720 px in the longest language. Update `docs/design/layout.md`. What's-new entry in every language. Walk it
at 720 px, reduced motion, light and dark. Visual critique before and after (`visual-critique`). No memory or processor
cost. Review by the other vendor (R146) and the coherence review.
