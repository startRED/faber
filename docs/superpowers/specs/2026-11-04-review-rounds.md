# Review rounds: a council scores the work and the agent climbs to a target

Status: proposed, waiting for the owner's choice. Nothing is built.

Asked by the owner on 2026-11-04 (direction: bring the council of judges and the hill-climb of Theo's T3 Code video to
Faber's users); the details are the AI's. The developer version, which scores Faber's own screens, is
`scripts/council.mjs` (PR #639). This page is the product version for Projects.

## What a person gets

Today "Review" in the Changes pane (R279, R280) asks the agent for one review by the other AI and gets a list of findings.
Nobody says how good the work is, a reviewer that invents a problem is believed, and fixing is a second request the
person must write. Review rounds turn that one button into a short loop:

> **Review, round 1: 17 of 25.** Both reviewers found 3 problems; fixing them.
> **Round 2: 22 of 25.** Good enough, stopped. 2 notes only one reviewer saw.

1. Two reviewers score the conversation's changes on five lines, 0 to 5 each: **Does what was asked**, **Correct**,
   **Safe**, **Simple**, **Checked** (tests or proof ran and passed). Each lost point names its place (`file:line`) and
   what it should be; a score with no reason is not counted, as in `council.mjs`.
2. A line's score is the reviewers' median; with two reviewers that is the lower one, so a generous one moves nothing.
3. Only a problem **both** reviewers raised on the same line and file goes back to the agent. A problem only one saw is
   kept apart as a note, never as work: this is what stops an invented problem from becoming a change.
4. With a third AI connected (Gemini through agy, once task 055 routes it), a reviewer whose total sits more than 6
   points from the middle one is left out of the round and the screen says so in one line ("One reviewer was far from
   the others and was left out").
5. The agent fixes only the agreed problems, runs the project's checks, and the reviewers score again. Faber keeps a
   round that rose (no line dropped, the total rose at least 1), puts the files back with Rewind when the total
   dropped, and stops when the target is reached, when no agreed problem is left, when two rounds in a row did not
   rise, or at the round limit.

## Who the reviewers are

- **The other AI**, through the existing helper (R161, `engine/src/helper.rs`), with its helper model (Sonnet high or
  Sol medium; economy mode Sonnet medium or Sol low), read-only, in the same folder.
- **The agent's own AI in a fresh session** that never saw the conversation, read-only, on the same helper model. A
  fresh session is a second opinion because it does not share the writer's reasoning.
- Neither sees the conversation, only the brief: the request, the diff against the copy base and the five lines.
- Only one AI connected: both reviewers are fresh sessions of it, and the result says "both reviews by the same AI".

## Where it lives

- It **extends the existing Review** of the Changes pane head (R279) and replaces what it sends; "Review with security"
  stays a single review with its own two-review rule (R146). No second review button anywhere.
- After a finished project task that changed files, the answer's actions row may offer "Review" where a step up or
  "Look around" would sit, never two of them (the R369 rule). It only starts the rounds, like "Look around" starts the
  box.
- The agent may start it itself (the owner chooses, below) at the close of a task that changed many files, through the
  same rounds, never on a small task.
- No setting on a work screen (C011). The target and the round limit are good defaults with no setting; if one is ever
  needed it is one row in Settings > Projects.
- Never in Chat: Chat has no Changes pane and no helper.

## What the person sees

- One row per round in the work log, the same row as a helper step (R161), its kind "Review", its title "Round 2: 22
  of 25". Opening it shows the five lines, the agreed problems and, folded, the notes only one reviewer saw.
- The two reviewers run as two helper rows under it; the person can write to them as to any helper (R196).
- One quiet line under the answer with the latest score. Stop on the reply stops the rounds.
- Every word is in `ui/locales/`; the engine returns keys.

## Limits and quota

- **Target: 21 of 25.** A round stops there; most good work lands 20 to 23, and the last points cost the most.
- **Round limit: 3 rounds** (the first score and up to two fixes). The video's lesson is that the climb flattens fast;
  a third fix rarely rises past the noise of one point.
- **Cost of one round**: two helper reviews (each a read of the diff, about the price of one normal turn of the helper
  model) plus one fix turn of the agent. A full climb of 3 rounds costs about six reviews and two fix turns, about four
  to six times one plain Review. The first round alone costs about twice today's Review.
- **Quota guard**: before each round Faber reads the pacing of both AIs (R36, quota pacing). A round never starts on an
  AI past its pace guard; it runs with the other reviewer only, and says so, or stops with "stopped to save quota".
  Economy mode drops the helper models a rung, as it already does.

## How it stays light (R125)

- No new process type and nothing kept alive: the reviewers are helper sessions that end with their report, and
  `helper::MAX_PER_CONVERSATION` (2) already caps them at two at once.
- No timer, no polling: a round starts when the previous step ends, from the events the engine already gets.
- Stored state is one small `review` object on the reply: per round the five numbers of each reviewer, the agreed
  problems and the notes, at most 20 rows of 300 characters, about 4 KB per climb in the conversation's file.
- The screen draws the rows from that object; the open round's details load only when opened.
- Expected cost: 0 MB while no round runs; while one runs, the two CLI processes a helper already costs, and under
  1 MB in Faber itself. The commit that builds it names the CI run that measured it.

## Build steps (after the owner's yes)

1. Engine: the scoring functions (`engine/src/review.rs`): parse a reviewer's answer, median, agreed rows, jester with
   three, keep/revert/stop, with unit tests ported in idea (not code) from `council.mjs`.
2. Engine: the rounds (`engine/src/engine/review.rs`) on top of the helper runner: two reviewers, the fix turn with only
   the agreed problems, Rewind on a drop, the quota guard, Stop.
3. Screen: the Review button sends rounds; the round row, its details and the line under the answer; locales in every
   language; the product map, design page and what's-new entry.
4. Optional, if the owner says so: the agent may start rounds itself at the close of a big task.

Each step lands as its own pull request through `land.mjs`; steps 1 and 2 get a fresh review by the other vendor.
