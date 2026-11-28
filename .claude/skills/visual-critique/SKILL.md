---
name: visual-critique
description: "Use when a change a person sees is ready for review, or when asked to critique Faber's screenshots: scores the screens the diff touches out of 40, before and after, against the taste cards and the composition checklist of faber-ui; and before asking the owner to look at the installed app, photographs it next to the apps it is measured against."
---

# Faber screen: visual critique

Checks see code and motion; they do not see a screen as a whole. A change a person sees therefore gets a critique of its
screenshots before it merges (R172). The writer takes the photos; a fresh session that did not write the change scores
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

Give the reviewer the two folders, the diff, `docs/product-rules.md` (the cards), the affected living design page and
section 4 of `.claude/skills/faber-ui/SKILL.md` (composition). Nothing of the writer's reasoning. "Nothing to report"
is a correct answer. Judge visible composition from the photos; use the current design and relevant decisions to
interpret intended behavior. A stale checklist is not authority to remove a documented control or scope exception.
Score each screen, before and after, 0 to 5 on each line:

| # | Line | 5 means |
| :--- | :--- | :--- |
| 1 | Main action | One action leads; everything else is quieter |
| 2 | Said once | No name or title repeats across sidebar, crumb and heading |
| 3 | Type | At most three strong weights in view; sizes step clearly |
| 4 | Art and text | Art never behind text a person reads; only a faint veil (C011) |
| 5 | Alignment | Columns share a baseline; edges line up or differ on purpose |
| 6 | Its own area | Words and rows follow the documented area scope; Chat's pick-up list includes both areas (C005, R175) |
| 7 | Calm | Only the work and its documented controls; no crowding or unrelated settings (C011, R175) |
| 8 | Fits | Reads at both sizes: nothing cut, no awkward wrap, no empty band |

The score of a screen is the sum, out of 40. For each point lost, one row: the photo, what is there, what it should be,
the line or card it breaks. A score with no rows is not a score.

### The council (the default scorer)

One judge sees through one model's eyes, and some judges invent defects. `node scripts/council.mjs <label>` sends the
photos of a label to several judges of different vendors at once (`--judges codex,agy,claude`, the default; two at
least), each in an empty temp folder with only the photos, no code and no tools:

- A line's score is the judges' median; with two judges, the harsher one.
- From three judges on, one whose total for a photo is more than 6 points from the middle judge is that photo's
  **jester**: its numbers leave the score and its rows go to "Unconfirmed". A jester is expected; it is why there is a
  council.
- Only rows two judges agree on (the same line of the same photo) are work. The rest is listed, never fixed by itself.
- `--refs <folder>` (or `FABER_COUNCIL_REFS`) adds photos of the apps Faber is measured against (section 4). The
  judges score them on the same lines; the report gives each screen's gap to the best reference and what they do better.
  Those photos stay on the PC, never in the repository.

It writes `council.md` and `council.json` in the label's folder. A point a judge took with no row counts as 5 and is
named in the report; the rest of its answer still counts. A judge that is signed out or gives no fair answer is
named and left out. The council replaces the single fresh session for the score; the rule below does not change.

## 3b. The climb (hill climbing to a target)

Asking once for a perfect screen does not work; a score and a loop do. For a screen below its target, or when the owner
asks for polish:

1. `node scripts/shots.mjs r0` and `node scripts/council.mjs r0`: the starting point, and the first best round.
2. Fix only the council's "Work" rows (and the references' "better" lines that fit the cards), as one commit.
3. `node scripts/shots.mjs r1`, then `node scripts/council.mjs r1 --against <best> --target 36`. The climb line says:
   - **keep**: no screen dropped, one rose past the noise; this round is the new best.
   - **revert**: a screen dropped, or nothing rose; `git revert` the round's commit and try a different fix.
   - **stop**: every screen reached the target.
4. Stop too after five rounds or two reverts in a row, and say where it ended. Never edit the rubric, the photos or the
   judges to climb; a climb that games its score is a revert.

The target of a climb is 36 of 40; the rule's floor stays 32.

## 3. The rule

- The score of a screen never drops. A drop blocks the merge, like a failing check.
- A new screen, or a screen the change touches, reaches 32 of 40 (the owner, 2026-10-16). One already below 32 may rise in
  steps, as long as it does not drop.
- The pull request says `Visual critique: before X/40, after Y/40, by <session>` (the lowest screen of each), and links
  the rows; by the council it says `by council <judges>` and names any jester.
- A point lost that a check could catch (art under text, a repeated name, uneven columns) is the sentinel's job: if it
  passed green, say so, so the check gets better.

## 4. Live look (the installed app, next to the apps it is measured against)

The photos above come from a test build with short made-up conversations. Most of what the owner had to point out by
hand only shows in the installed app after hours of real work: two counts of the same thing that disagree, a raw command
in the main column, a screen that does not change for a minute, an English helper title, a menu over the heading. So
before asking the owner to look, and after a change to how progress, helpers or status show, the session looks itself.

1. Photograph the installed app with computer use, never sending a message or pressing Stop: the new-conversation
   screen, its model and access menus, a conversation working alone, one with a helper, the moment between two turns, a
   finished turn with its work opened, each right panel, and Settings. Photograph the same moments in the reference
   apps the owner named (T3 Code, the Codex and Claude desktop apps).
2. On every working screen take two photos 15 s apart. Something a person can see must change between them (a ticking
   time, a new line); a still screen reads as stuck even when the work goes on.
3. Hand the photos, and nothing of your own reading, to two fresh sessions of other vendors (R146). Each looks as a
   person who does not program and as a designer, and answers the three questions a person has while the AI works: is
   it working or stuck, what is it doing now, do I need to do anything.
4. A still photo hides some faults, so check these by name: the same fact said twice, counts or times that disagree,
   raw commands and file paths, English or technical words, meaning only on hover, text cut short, a menu covering what
   you were reading, an icon that means something else (a stop square shown for "running").
5. Find each fault in the code before it becomes a task or a fix, and say "seen" or "guess" for each.
