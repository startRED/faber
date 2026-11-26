# Each project answer shows its pages before and after, side by side

Area: Reply media
Files: ui/BeforeAfter.tsx, ui/Thread.tsx, ui/api.ts, host/src/table.rs, ui/locales/, tests/ui/before-after.check.ts, docs/design/conversation.md, docs/product-map.md
Depends: before-and-after-pictures
Size: about 450 changed lines

Why: plan item B1, second half. Once Faber keeps a picture of the running app's pages before and after an answer, the
person sees what changed without reading code.

Build:

- Under a finished project answer with a stored pair, a compact row of thumbnails, before and after side by side per
  page, that opens the existing picture viewer; nothing when there is no pair. The work area shows only the work
  (`docs/product-rules.md`): small, quiet, below the answer.
- The host command to read the pair (free: reading) and its API call.
- Texts in every language of `ui/locales/`, by hand.

Leave out: taking the pictures (previous task), comparing them.

Done: a screen check with a stored pair shows both pictures in order and opens the viewer, and shows nothing without a
pair (fails without the code). Walked as a newcomer and a heavy user against the taste cards. Extends
`docs/design/conversation.md` and the product-map rows `Reply media` and `Page in Faber's browser`. Screen only: ships
on its proofs.
