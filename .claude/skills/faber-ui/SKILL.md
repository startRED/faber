---
name: faber-ui
description: Use when adding, changing or reviewing anything the user sees or feels in Faber's screen (ui/): motion, hover and press states, surfaces, icons, spacing, radius. Decides first whether something should move at all, then builds it from Faber's tokens, then checks it against a short list.
---

# Faber screen: motion and polish

The rules and numbers live in `docs/design.md` ("Looks" and "Motion"); the tokens live in `ui/styles.css` and
`src-tauri/src/themes.json`. This skill is the order to think in and the list to check. If it disagrees with the docs, the
docs win.

## 1. Should it move at all?

Count how often the user triggers it.

- Dozens of times a minute (keystroke, shortcut, tab or row switch, row hover): no motion, or a `--dur-fast` fade of
  colour or opacity. Never travel, scale or blur.
- Several times a session (panel, menu, toast, permission card): the normal entrance and exit of the docs.
- Rare (first run, a finished task, an empty state): the only place for something expressive.

Name what the motion is for: feedback, where something came from or went, a state that would otherwise jump, or delight
(rare tier only). No name, no motion. Text the user is reading or acting on never moves for style.

## 2. Build it from what exists

- Reuse before writing: `--dur-fast`, `--dur-ui`, `--dur-arrive`, `--dur-exit`, `--ease`, `--ease-io`, the `--motion-*`
  tokens, and the helpers in `ui/motion.ts` (`keepForExit`, `animateList`, `animateDetails`, `reveal`). A second set of
  durations or curves is a defect.
- Only `transform` and `opacity` move; `clip-path` is the fallback. Height changes only on disclosures, measured in JS.
- Entrances ease out, exits are shorter than entrances, nothing starts from `scale(0)`, no chrome animation passes 300 ms.
- State that flips fast (toggles, toasts, anything twice in a second) uses a CSS transition, which reverses from where it
  is. Keyframes are for things that run once.
- Name the properties in `transition`; never `all`. `will-change` only after a measured stutter, only on the property that
  animates, and never on something that sits still (the memory budget in `AGENTS.md` counts layers).
- A surface that has a motion level (`data-motion` full, reduced, none) must be listed where the existing selectors
  switch it off, in the same change.

## 3. Surfaces and icons

- Radius comes from `--r-xs`..`--r-lg`, `--r-pill`, `--r-stage`. A card inside a padded card gets the inner radius plus
  the padding, not the same radius twice.
- Depth comes from the look's shadow tokens (`--lift-shadow`), not from a new `box-shadow` pasted in. A ring made of
  `box-shadow` keeps a transparent border so forced-colors mode still draws the edge.
- Icons are paths in `ui/Glyphs.tsx`, coloured by `svg.i` with `currentColor`: no hex fills, no second icon set on one
  surface, stroke matches the text beside it, and the words beside the icon carry the meaning (the icon is decoration).
- An icon that swaps on a rare state change cross-fades; one that swaps on a tab or row change swaps instantly.

## 4. Composition

Code checks and motion checks do not see the whole screen. Look at a screenshot at 1920x1080 and 1280x800, and check:

- One main action per screen. Everything else is quieter than it.
- Each piece of information shows once: the sidebar, the crumb and the heading never all repeat the same name.
- At most three strong text weights in view (a heading, a label, the body); the rest is regular or muted.
- Decorative art (the black hole, the light, the dust) is never behind text a person reads: under it, only a faint veil.
- Neighbouring columns share a baseline: their bottoms (or their first lines) line up.
- A screen of one area never borrows another area's words, hints or rows (the Projects home says nothing of Chat).

## 5. Before you finish

- Every motion has its reduced/none path and a static cue (colour, icon or label); motion is never the only signal.
- Every new word is a key in `ui/locales/` (English first), not a string in the component.
- Run the screen check for what you touched (`cargo t`; `cargo t --all` when the screen or shell changed) and play any new
  motion slowed down (the test build has the 0.25x playback described in `docs/design.md`) before calling it done.
- Say what you could not check. Feel (a spring, a crossfade) cannot be judged from code: say so instead of guessing.

## Reviewing instead of building

Report one row per root cause, worst first: `file:line`, what is there, what it should be, why (the rule and what the
user feels). Severity: high = breaks an interaction (a toggle that cannot reverse, motion that ignores the motion level,
a state carried by motion alone); medium = two ways of doing the same thing; low = isolated polish. Finish with Block if
any high remains, Approve otherwise, and never approve what you did not read.
