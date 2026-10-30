# Put a word next to the context ring and the quota bars

Area: Context meter
Files: ui/Context.tsx, ui/Sidebar.tsx, ui/Accounts.tsx, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/context.check.ts, tests/ui/quota.check.ts, docs/design/layout.md, docs/design/sidebar.md
Depends: none
Size: about 260 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3: the context ring ("43 %") and the sidebar quota bars
("≥ 24 %", "16 %") say what they measure only on hover. Claude desktop writes "Contexto 31 % usado" beside its meter.
Verified: `ContextMeter` (`ui/Context.tsx`) draws a ring and `ctx.share` (a bare percentage); its meaning sits in
`ctx.tip` ("Context: {used} of {window} tokens. The AI compacts it by itself near the end.") as `title` and
`aria-label` only. In the sidebar (`ui/Sidebar.tsx` lines 592 to 622) each AI is a button with its mark, a bar
and `share`; the vendor name is a `sr` span and the window, reset, age and "still counting" are in the tooltip, where
`quota.atLeast` ("≥ {share}") shows with its note `quota.countingNote` only on hover. `ui/Accounts.tsx` line 149 shows
the same "≥".

What to build, in five languages by hand:

- Context ring: a visible word beside the ring, "Memória da conversa 43 %" ("Conversation memory 43 %"), and what happens
  when it fills in the panel's first line (`ctx.tip` already says the AI compacts it by itself; move that into the
  panel, visible, and keep the tooltip). Check the Words table first: the product map calls it the Context meter and
  never "memory"; if "memória" collides with Project memory, pick the word that does not, and say so in the commit.
- Quota bars: "Claude · 24 % usado", "Codex · 16 % usado" as visible text on the button (the 288 px sidebar must not
  wrap; abbreviate "usado" only if it does not fit, never the vendor); the reset time stays on hover and shows in the
  expanded view of Settings > Accounts; "≥" gets its sentence visible there (`quota.countingNote`) instead of only on
  hover.

Leave out: the topbar's icons and branch (task `clear-topbar`), the quota numbers themselves, the pace sentences.

Done: screen checks in `tests/ui/context.check.ts` and `tests/ui/quota.check.ts` fail today because the ring and the
bars hold only a number; they pass with the words in the DOM text of the buttons (not only in `title`), no wrap at the
sidebar's width in the longest language, and the tooltips unchanged. Extends: product-map "Context meter" and
"Quota window, pace, plan". Update `docs/design/layout.md` and `docs/design/sidebar.md`. What's-new entry in every
language. Walk it at 720 px, longest language, light and dark. Visual critique before and after. No memory or processor
cost. Review by the other vendor (R146) and the coherence review.
