# Let the project home's folds say what each one is for

Area: Project goals
Files: ui/ProjectGoals.tsx, ui/ProjectMemory.tsx, ui/Welcome.tsx, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/goals.check.ts, tests/ui/project-memory.check.ts, tests/ui/homes.check.ts, docs/design/projects.md
Depends: none
Size: about 200 changed lines

Why: the live look of 2026-10-29, next to T3 Code (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"):
on the project home half the window stays empty until the dotted art fades in, the box's hint changes while you read,
and "Metas permanentes", "Memória" and "Sessões do terminal" are bare grey rows with no sentence that says what they
are. Verified in the code: the two folds are plain ghost buttons that show only their title
(`ui/ProjectGoals.tsx` line 22, `goals.title` "Standing goals"; `ui/ProjectMemory.tsx` line 16, `memory.title`), and the
terminal sessions list is a section titled `s1.terminal` ("Sessions from the terminal", `ui/Welcome.tsx` lines 130 to
134). The empty half and the changing hint are the Dark Room art (`ui/Hole.tsx`) and the composer's rotating hints
(`ui/Ghost.tsx`); they are the owner's taste.

What to build, taste first: put each fold's one plain line under its title, closed or open ("Metas permanentes: o que
o projeto deve sempre manter verdadeiro, como 'todas as verificações passam'"; "Memória: as instruções do projeto que a
IA lê"; "Sessões do terminal: conversas que você começou no terminal desta pasta"), using the product map's own
definitions ("Standing goal", "Project memory") in short words, in five languages by hand. Do not add a setting or a
control. The rotating hint and the late art are not part of this task: they are the owner's taste, so the commit
describes both in a short card with a screenshot and asks whether to change them.

Check the taste cards whose "Touches" name the project home and card C011 (`docs/product-rules.md`: a work screen holds no settings and no long
explanations; one short line is the limit) first. A rule that conflicts goes to the owner before code.

Leave out: the art, the hints, the layout of the page, the folds' contents.

Done: screen checks in `tests/ui/goals.check.ts`, `tests/ui/project-memory.check.ts` and `tests/ui/homes.check.ts` fail
today (each fold shows only its title); they pass with the line present, not wrapping at 720 px in the longest language,
and the fold still opening and closing by keyboard. Extends: product-map "Project goals" and "Project memory". Update
`docs/design/projects.md`. What's-new entry in every language. Walk it at 720 px, light and dark. Visual critique before
and after. No memory or processor cost. Review by the other vendor (R146) and the coherence review.
