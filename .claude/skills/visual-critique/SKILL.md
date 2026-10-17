---
name: visual-critique
description: "Use when a change a person sees is ready for review, or when asked to critique Faber's screenshots: scores the screens the diff touches out of 40, before and after, against the taste cards and the composition checklist of faber-ui."
---

# Faber screen: visual critique

Checks see code and motion; they do not see a screen as a whole. A change a person sees therefore gets a critique of its
screenshots before it merges (R168). The writer takes the photos; a fresh session that did not write the change scores
them, preferably of the other vendor (R146). CI never calls an AI, so this is a step of the process, not a check.

## 1. Take the photos (the writer)

```bash
node scripts/shots.mjs before
```

Run it before the first edit (or in a detached worktree of the base commit), then again with `after` when the change is
done. It runs the UX sentinel's composition scenario, which photographs the Chat home, a project's home, an open
conversation and Settings at 1920x1080 and 1280x800 in Dark Room, and keeps the screens the diff touches (all four when
it touches none, as before the first edit), in
`<target>/faber-test/critique/<label>/` with an `index.md`. `--filter <scenario>` adds the photos of another screen test
(`--filter homes:`); `--every-screen` keeps all four. It runs hidden like every screen test.

## 2. Score them (a fresh session)

Give the reviewer the two folders, the diff, `docs/product-rules.md` (the cards) and section 4 of
`.claude/skills/faber-ui/SKILL.md` (composition). Nothing of the writer's reasoning. "Nothing to report" is a correct
answer. Score each screen, before and after, 0 to 5 on each line, from the photo alone:

| # | Line | 5 means |
| :--- | :--- | :--- |
| 1 | Main action | One action leads; everything else is quieter |
| 2 | Said once | No name or title repeats across sidebar, crumb and heading |
| 3 | Type | At most three strong weights in view; sizes step clearly |
| 4 | Art and text | Art never behind text a person reads; only a faint veil (C011) |
| 5 | Alignment | Columns share a baseline; edges line up or differ on purpose |
| 6 | Its own area | Only this area's words, hints and rows (C005) |
| 7 | Calm | Only the work on a work screen; no crowding, no settings (C011) |
| 8 | Fits | Reads at both sizes: nothing cut, no awkward wrap, no empty band |

The score of a screen is the sum, out of 40. For each point lost, one row: the photo, what is there, what it should be,
the line or card it breaks. A score with no rows is not a score.

## 3. The rule

- The score of a screen never drops. A drop blocks the merge, like a failing check.
- A new screen, or a screen the change touches, reaches 32 of 40 (the owner, 2026-10-16). One already below 32 may rise in
  steps, as long as it does not drop.
- The pull request says `Visual critique: before X/40, after Y/40, by <session>` (the lowest screen of each), and links
  the rows.
- A point lost that a check could catch (art under text, a repeated name, uneven columns) is the sentinel's job: if it
  passed green, say so, so the check gets better.
