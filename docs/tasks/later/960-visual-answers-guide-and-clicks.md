# Visual answers that look designed and answer a click

Area: Visual answers
Files: engine-drivers/src/persona.rs, engine-projects/src/visual.rs, ui/VisualAnswer.tsx, ui/visual-mermaid.ts, ui/locales/, docs/design/visual-answers.md
Depends: release-1-0-gate
Size: about 400 changed lines

Why: the owner asked on 2026-11-05 for visuals like the ones the best chat apps draw inline, after 1.0. Faber already
renders HTML, SVG and Mermaid reply blocks and ten built-in pieces (R348, R311); what it lacks is a shared look and
interaction. Today each answer invents its own colors and sizes, so drawings clash with the theme, break in dark mode
or overflow the column, and a drawing cannot be asked about.

Build:

- A short design guide both CLIs read only when they draw (not in every turn): the column width the drawing gets, two
  text sizes, a small palette from Faber's tokens that works in light and dark, thin strokes, no gradients or shadows,
  sentence case, a tight height. It ships inside the persona's visual section, under 1,500 tokens.
- Theme variables exposed to the drawing's frame, so a drawing that uses them follows light and dark by itself.
- A box marked as askable (`data-ask="<question>"`) puts that question in the composer without sending, the same way a
  built-in form fills the composer today.
- Step-by-step drawings for processes: one panel per step with Next and Back, inside the existing visual frame.

Leave out: new piece kinds, a gallery, anything that keeps a process alive after the drawing closes.

Done: screen checks show a drawing that uses the theme variables changing with the theme, an askable box filling the
composer without sending, and a step drawing moving forward and back; memory measured before, while rendered and after
close, beside the figures in `docs/design/visual-answers.md`. Extends: the product-map row "Visual answers".
What's-new entry in every language.
