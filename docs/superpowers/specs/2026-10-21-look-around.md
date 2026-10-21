# Look around: the same mistake elsewhere

Status: in progress. Steps 1 and 2 are in review; steps 3 and 4 are planned.

Asked by the owner on 2026-10-21 (direction, "um senso comum mas sem ser uma regra que atrapalha em outras coisas e só
usado onde importa"); the details are the AI's (owner delegated).

## What a person gets

When an AI fixes one mistake, the same mistake often lives in other places: a sibling caller with the same missing
check, the same wrong term in six paragraphs, the same broken formula in other rows, the same low contrast on three
screens. A careful person looks around before saying "done". Faber should too, and then ask, never fix unasked:

> Fixed the date parsing in `orders.rs`. The same unchecked `unwrap` is in 3 more places.
> **Fix them too?** [Yes, fix them] [Not now]

## Why not a rule in the persona

A line in `engine/src/persona.rs` ("always look for the class of the bug") would weigh on every turn of both areas: a
greeting, a translation, a quick question. The model would sweep folders nobody asked about, spend quota and answer
later. Common sense here is knowing *when*, so the feature has two parts, each scoped to the moment it matters:

- **The agent's own judgment, only at the close of a fix.** The `wrap_up` tool's description (R248) is read when a
  project's agent closes a task. One sentence there asks it, when the task fixed a mistake, to say whether it saw the
  same kind of mistake elsewhere and to make that the box's one question. Nothing changes in Chat, in helpers, or on a
  turn that fixed nothing.
- **A deterministic sign from the engine, for every model.** The engine already reads a turn's own tools for the step
  up (`engine/src/nudge.rs`): a check that failed, code that changed unchecked. Its mirror is a turn that went from red
  to green: a check failed, the AI changed the work, and the last check passed. That is a fix, whatever the words, the
  vendor or the language. When it is seen and the agent did not already ask, the answer offers "Look around".

Both stay silent everywhere else: no extra process, no sweep, no tokens until the person asks.

## How it fits what exists

| Part | Extends | Why not something new |
| :--- | :--- | :--- |
| The agent asks about the other places | Agent's plan and closing box (R248): the one question with one or two buttons | A second question surface would compete with the box |
| The engine's sign | Step up (`engine/src/nudge.rs`): the same `is_check` and `edited` rules (R180, R192) | A second list of what counts as a check would drift |
| The offer on screen | The answer's actions row, where "Retry stronger" lives (Chat conversation) | A banner or a card is a notice for something optional (C001, C011) |
| What the button does | "Continue with a helper" (R196): it fills the conversation's box with a request the person can edit and send | Sending at once would spend quota on a click the person may not have meant |

Rules it keeps:

- One question at a time: no "Look around" when the reply's closing box already asks something, and never beside a step
  up (a failed last check is a step up, a passed one may be a look around; they cannot both hold).
- The offer says what it does in its tooltip and changes nothing by itself. The request it writes asks the agent to list
  each place with its file and line and why it is the same, change nothing, and ask which to fix.
- Keep going (R199) answers an unanswered closing question with its first button after ten minutes, as for any other
  question. The owner's rule for unattended work is to go on item after item, so the agent's recommendation stands, and
  the "Faber answered for you" card offers "Change" as it does today.
- Chat has no closing box, so there only the engine's sign speaks, in a chat that works in a folder and ran a check.

## Steps

- [x] **1. The sign and the agent's question** (engine only, no screen). `engine/src/look.rs`: `look::of(message)`, pure,
  over the finished reply: done, a check failed, the work changed, the same command passed after the change, every
  command that failed ended green, the last check passed, no code changed after it, and no closing question. The review
  found that a later failure which passed again hid the fix; a command is its whole line, so a narrower rerun stays red.
  Stored on the reply as `look` (key `look.fixed`). One sentence in `WRAP_UP_DESCRIPTION` (`engine/src/progress.rs`). Unit tests in both files. Cost: a
  pass over the reply's tools once per finished turn (microseconds), about 60 tokens more in each project request's tool
  list (cached with it).
- [x] **2. The offer on screen.** "Look around" in the actions row of the newest answer, where "Retry stronger" would
  sit (they never show together), only while the conversation takes messages; a click fills the box with the request
  (R196's path); its words and tooltip in the five locales, by hand; a what's-new entry; the map rows (Words: a "Look
  around" row beside Step up; Chat conversation; Features: a "Look around" row); `docs/design/conversation.md` (answers);
  `tests/ui/look.check.ts`. A fake-CLI fixture with a red-then-green turn, which would prove end to end that the engine
  sets `look`, comes with step 3.
- [ ] **3. Evidence before the click.** A second sign, `look.sameText`, for every kind of file: the text a change
  removed (Claude's `Edit` `old_string`, Codex's removed diff lines; at least one line of 12 characters or more that is
  not only punctuation) still appears in other tracked files of the folder. A bounded read after the turn (git's file
  list, at most 2,000 files and 8 MB, 300 ms, on a blocking worker, never on the turn's path), whose places (at most 8,
  `file:line`) go into the request and the tooltip ("The same text is in 3 more places"). This is what reaches writing,
  data and configuration, where no check runs. Its cost is measured on Faber's own repository and stated in the commit.
- [ ] **4. Measure, then quiet it if needed.** Count, in the local `cache-health.jsonl`-style log that never leaves the
  PC, how often each sign is offered and used, per area. After a week of the owner's use: a sign used under one time in
  ten is narrowed or removed. Telemetry stays off.

## Not doing

- No automatic sweep model after every fix: it would spend quota on turns the person may close at once, and a new
  process needs a reason and a measurement (R125).
- No fix without the person's answer, and no new setting: the offer is quiet enough to ignore, and a switch for it would
  be a setting nobody needs (C011).
- No word lists of "fix", "conserta", "corrige" in five languages: the turn's tools say it better than the words.
