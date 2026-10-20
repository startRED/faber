# Layout

The window's widths, titlebar, sidebar, the homes, answers, the work log, pages in Faber's browser and the
running column. Search, the command box and find are in [search.md](search.md).

## Layout

The window has a 36 px titlebar across the full width, a 248 px sidebar, a stage with a 52 px topbar, a reading column of
at most 660 px centred in the stage, and a running column on the right. The window can be as narrow as 720 px,
so the layout changes with its width:

| Window width | Sidebar | Running column | Reading column |
| :--- | :--- | :--- | :--- |
| 1,100 px and wider | 248 px, beside the stage | 360 to 560 px when open, beside the stage | Shrinks with the stage, 660 px at most. The stage is never narrower than 440 px |
| 900 to 1,099 px | 248 px, beside the stage | When open, an overlay on the stage | Up to 660 px |
| 720 to 899 px | Hidden. A button in the topbar opens it as an overlay | When open, an overlay on the stage | Up to 660 px |

Text size up to 18 px must not push the page past the window width at any of these widths: content wraps or scrolls
inside its own area.

### Arranging the shell

The numbers above are the usual widths. The person can change them, and the shell keeps what they choose (R211).

- **Edges.** The sidebar's right edge and the running column's left edge are separators (`role="separator"`, focusable).
  A drag moves them between 200 and 360 px (sidebar) and 300 and 560 px (column). A docked column also leaves the stage 440 px:
  the stylesheet clamps it with the same numbers, so a window that shrinks after a drag is right before any script runs.
  A thin accent line and a small grip appear on hover (after 120 ms, so crossing the edge flashes nothing), and the grip is
  accent while dragging or focused. The arrow keys move an edge 16 px (48 with Shift), Home and End go to its limits, a double
  click or Enter glides it back to the usual width. The sidebar has no edge below 900 px, where it is an overlay.
- **The column's usual width follows the window** (`autoRun`): the window less the sidebar less 700 px, 360 to 560 px. A
  drag ending on it, a double click or Enter goes back to following. A subagent can fill the stage ([subagents.md](subagents.md)).
- **How a drag stays light.** The pointer events of the window (listened to only while dragging) set `--side-w` and `--run-w` on the
  shell once per frame; nothing is measured and no state changes until the drag ends, when the width is stored (`faber.layout`)
  once and the lists that animate layout measure again. `html.sizing` sets the cursor and stops text selection meanwhile.
- **Focus mode (Ctrl+B).** The sidebar and the running column step aside: the sidebar fades and slides out (under the stage, which
  slides over to the window's edge as it does when the column opens), the column closes the way it does from its own button.
  Ctrl+B again, the button that appears at the left of the topbar (`Leave focus mode`, grown in from nothing, never popped in),
  the run button, Ctrl+. or anything that opens the column brings them back; a column that was open comes back open. Focus mode is
  not stored: a window always opens with its sidebar. With reduced motion or none, all of it is instant. It is a shortcut, not
  a setting, so no work screen has a switch for it.
- **A file over the window.** Anywhere in the window, a dragged file covers it with a calm veil (78% of the stage colour, a dashed
  accent line 12 px inside the edge) and a pill, "Drop to attach". It fades in and out, takes no click, and shows only where a
  composer is on screen and not behind a modal layer. Letting go hands the files to the composer's own drop, so the box's add path is
  the only one. The window listens to one `dragenter` between drags; the rest only from the first one until the drag ends (a drop,
  leaving the window, or a second without movement).

### Titlebar and topbar

- The titlebar has the lockup and the window controls. Dragging and double-click maximize keep working, and macOS keeps
  its native buttons.
- The topbar has a crumb ("Chat › title" or "Project › title") and the run button. The model is not shown there: it is
  chosen in the composer and each answer names the one it used (R168). With nothing running the button reads "Nothing running" as quiet second-ink words with no outline or
  fill (styled from the absence of the live mark, `.run-btn:not(:has(.live))`); the outline comes back on hover,
  keyboard focus and while the column is open. Otherwise it reads "{n} running". When an
  item needs the user it turns amber, shows the speech bubble and reads "{n} running · {m} needs you". The words are
  always visible.
- Before the run button, the open conversation's Context meter (R221): a 16 px ring and "{pct}%" in the quiet second ink, with the tokens in its tooltip. It is status (C003): it shows once the CLI has said both numbers, with no click, motion or setting (C011). It turns amber only from 85% of the window, near where the CLI compacts by itself (C002), never sooner.

### Sidebar

- At the top: "New conversation" (Ctrl N), then the two areas, Chat and Projects, the latter with a count.
- The sidebar lists only the open area's items (R168): Chat lists the chats and no Projects block, not even an empty
  one; Projects lists the projects with their conversations and the add-folder control, and no chats. Rows keep their DOM
  when the list refreshes, so a refresh moves nothing the person is pointing at.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- Chats are grouped as Today, This week and Earlier. Deleting a conversation asks for confirmation.
- A conversation an agent started sits right under the one that started it, indented 14 px with a hairline guide at
  its left; one whose starter is not listed stays in its own place.
- An archived conversation leaves the list. A search still shows it, its title followed by a quiet "archived" in the
  smallest size and the third ink. Opened, once its reply ends, the composer's place holds one card of the composer's
  width: the line "This conversation is archived…" in the second ink and a small Unarchive button at its right.
- The footer is one row, the person's name when they gave one and Settings, then one thin line per connected vendor: its
  shape, a 3 px bar and the used share of its weekly quota window (R177; a vendor that never reported one shows its tightest), in the
  small size. Nothing on the line wraps. The window, the reset, the other windows with their shares (the five-hour one), how old the reading is, the not-yet-counted estimate and the vendor's name are its tooltip. While
  use is still being counted the share reads "≥ 44%" (the real share can only be higher), with the words in the tooltip
  (C010). The bar is amber only when the engine says the quota is nearly used. The line opens Settings > Accounts and limits.
  A vendor with no reading yet, or a weekly reading past its reset, shows "—". The version is not on the sidebar: it is the
  tooltip of the Settings button.
- Search filters titles and project names, then message text, and opens the message ([search.md](search.md#sidebar-search)). The percentage and "stopped" tails
  come with the progress view in Phase 3.

### The new conversation screen

- One column, like the Projects home (C005): the greeting, the composer, the suggestions and what to pick up, top to
  bottom. No line under the greeting: the composer's placeholder already says to ask anything, and its permission chip
  shows whether Chat may change files.
- A greeting, "Where do we start?", ending with the person's name when they gave one.
- Three short hand-written suggestions per language, as chips under the composer: one row at the usual text size, the
  last chip wrapping only on a narrow window or a large text size. A click fills the composer and focuses it, sending nothing; a hover changes only its colour.
- "Pick up where you left off" lists up to three conversations of both areas under the suggestions: those that need the
  person first, then the most recently updated.
- The composer keeps its model picker, "AI: choose for me".
- `@` in a conversation with a working folder lists matching files and folders over the box, on the context menu's
  surface, left-aligned with the text, with no motion; the entry the arrows reach is highlighted and kept in view, and a
  long path ends in an ellipsis. `/` at the start opens the same list for the agent's commands and skills (`/btw` first
  in an open conversation); a pick inserts the CLI's spelling without sending. Both lists follow the caret and ignore
  older answers and non-main mouse buttons.
- Auto chooses both the model and its effort. A manual model adds an adjacent low, medium or high effort choice,
  defaulting to medium (R102). Each conversation remembers its choice. Changes apply to the next message only (the
  composer does not repeat it), and each answer keeps the effort actually used and shows it in its header. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here, and the empty composer idles with a typing bar and rotating hints.
- With no AI connected, the greeting, the suggestions and the list give way to the empty state (Curioso, "No AI connected",
  one line and the button "Connect", which opens account management). The composer stays on screen but cannot send. An open conversation with no AI connected
  shows no composer and no permission line.

### Answers

- The header is the vendor's mark, the model's name and its effort in the quiet header ink ("· high effort"; the owner,
  2026-10-17), nothing else (Auto shows its reason beside the name only while it is still choosing). Auto's reason is its
  tooltip. The pace line stays (C002). A wait for quota is told once,
  by the quota card, never also in the header or the red note.
- Under the answer, one line that never wraps, at a 660 px column or a 720 px window: three icon buttons from
  `ui/Glyphs.tsx` (copy, retry, open in terminal, each with a tooltip and an accessible name), a short "Retry stronger"
  when a step up is offered, then the cost in the faint data style. The tokens and how long the cache stays warm are the
  cost's tooltip. There is no model select under an answer: a retry (and the right-click Redo, which names the model)
  uses the composer's current choice.
- The same line carries the reply's state (the owner, 2026-10-18: "you should have said you finished"), after the buttons, in the
  quiet ink and the small size, cut with an ellipsis before it pushes a button: "Working" while the agent writes or works,
  then "Done", "Done · 12 min of work" once the work took a minute or more, and on the newest reply "· updated 3 min ago"
  ("just now" under a minute, none past a day). A reply that works shows only this text in the row, the buttons join it when
  it is done, so the row does not grow. The agent is done when its last message ended and no tool call of the reply is open,
  though the CLI may close its turn up to a minute later (its Stop hooks): from then the work folds into
  its activity, the buttons appear, the running column and the topbar's count leave it out, and the composer reads Send,
  where a message waits in the queue. More words after that undo it. The age ticks once a minute on the newest reply only,
  never while the window is hidden; older replies run no timer. A reply from before the times were kept, a stop and an error
  say nothing here. Nothing animates.
- In the work log, what the agent wrote between its steps shows its last two paragraphs; earlier ones wait under a quiet
  "1 more" link in the third ink (R251). A code block keeps the narration whole. The answer itself never folds.
- The caret blinks only while words arrive; a reply at work after its words shows its step under them, in the status
  line's style. Faber's plumbing takes no row: its own message is a loop mark after the next reply's model (R241; the
  product map's Work log and Faber's own message say the rest).
- A project agent's plan (R248) sits under the newest reply, above its files, while the agent works and until every step
  is done: a quiet head "Plan · 2 of 5" in the small size, then one line per step, a small square mark before it: a check
  and the third ink when done, an accent ring and the full ink on the current one (`aria-current`), plain when still to
  do; each state is said to a screen reader. What the agent says it is doing now (its `status`, 3 to 6 words) takes the
  place of the newest step's words under the reply while it works. A reply the agent closed with `wrap_up` ends, above
  the actions row, with one box on a hairline: the recap, then at most one question in the semibold weight and its one
  or two replies as buttons, the recommendation the primary one. Only the newest reply, once it is done, offers the
  buttons; a click sends the button's words as the person's message. A question Faber answered while Keep going ran
  keeps no buttons: the reply's answers card says what it chose ([keep-going.md](keep-going.md)). Nothing in the plan or the box
  moves; both wrap at 720 px.
- There is no permission sentence over a conversation: the mode is a Settings matter.
- Keep going (R199): the chip, its quiet lines, its quota warning and what it shows on coming back are in
  [keep-going.md](keep-going.md).
- A project reply that opened, queued or pushed a pull request ends with one pace-style line (R200): its words ("Following
  pull request #93: checks running.") link to the page, and "Stop following" sits beside them while it is open. Only checks
  that wait for the person turn it amber. When the checks fail, Faber's own message is the loop
  mark on the next reply, its tooltip naming the pull request and its failed checks.
- A project reply that changed files, in full access, ends with Faber's own checks (R146) in the same pace-style line,
  above the pull request's: "Faber is running the checks on these changes…", then "Faber's checks passed (3 checks)." or
  "Faber's checks failed: npm run lint." with "What failed" under it, the work log's disclosure holding each failed check's
  first three short failures in the data face. Faber's fix turn is the loop mark on the next reply ("Faber's checks failed (npm run
  lint). Faber asked the agent to fix them." in its tooltip); after two in a row, or where Faber does not ask (the reply asked the
  person something, Keep going stopped on it, the pace is used up), the line turns amber and says it needs the person.
  While the checks run the conversation reads as at work: the composer shows Stop, and a message written then waits in
  the queue. Nothing moves, and no setting or first-run step comes with it.
- While an answer streams the engine sends the whole message every 33 ms, so the text is drawn block by block
  (`ui/markdown.tsx`): a block whose source did not change keeps its DOM and only the open one is parsed and sanitised
  again. A tick that changed only the tools builds no text. Everything that reaches the page passes DOMPurify.

- [Queued messages and editing](queue.md).

### Work log

- The chips' line counts extend into the project's "Changes" view in the running column: git supplies the conversation's
  whole change, including files changed by commands, so its totals can differ from the tool-input counts below.
- The work log and a helper's row share one disclosure style (`.disclosure`: a quiet card, a chevron, the hover).
- Each step is a quiet chip: a verb in the ink color ("Reading", "Running") and what it acts on in the mono face, cut
  in the middle. A tool Faber does not know keeps its own name; an MCP tool reads "Using a tool" with its server's name.
- A conversation the agent starts by itself (R198) is one chip like any other, "Working in parallel" with its title (or
  the id when the agent reads it back); the conversation itself is a normal row in the sidebar, never a step inside.
- The agent running Faber's checks (the `verify` tool) is one chip with no detail: "Running the quick checks", or
  "Running every check" for the full level. What failed is the agent's to read; the chip only says it ran.
- Subagent and helper rows have their own page, [subagents.md](subagents.md).
- While a reply runs and has ended at least one check, a quiet line under the status line counts them (R161 step 9):
  "Ran 3 checks · none failed" after a check mark in the accent's text ink, or "· 1 failed" with the error note's 2 px
  edge and no mark. The words are the subagent facts' own. It goes when the reply ends; nothing moves.
- A file change's chip ends with its line count in the data face, `+N` in the accent's text ink and `−N` in the error
  ink (R161 step 9). While the newest step of a running reply is a file change, the lines it changed sit under the status
  line until the next step or the answer: at most four, one line each in the data face and the third ink, the sign in
  the count's ink, then "N more lines". Nothing moves; the status line already says the step, so the lines are hidden
  from a screen reader.
- While a reply runs and has ended at least one check, a quiet line under the status line counts them (R161 step 9):
  "Ran 3 checks · none failed" after a check mark in the accent's text ink, or "· 1 failed" with the error note's 2 px
  edge and no mark. The words are the subagent facts' own. It goes when the reply ends; nothing moves.
- While a reply runs and has read a web page, a quiet line under the status line names the sites it opened (R161 step 9):
  "Opened 5 sites", then "+N" for any past four, then the four last opened, joined by middle dots in the third ink, the
  one it reads now last and in the first ink. A site opened again moves to the end. It goes when the reply ends; nothing moves,
  and the status line already says the page, so the list is hidden from a screen reader.
- A step that has run for half a minute says for how long at the end of the status line (R161 step 9): " · 2 min" in the
  same ink, in tabular figures, from when the engine saw it start. A long command cuts its own words, never its time.
  The clock ticks every 15 s only while such a step runs and the window shows; the time is hidden from a screen reader,
  since a live region that changed that often would keep talking.

### Recent folders and environments

The "Open a project" screen (Projects with none open) and the Chat folder setting list the last five folders used, newest
first, one click each: a short name and the dimmed parent path. The engine judges each one when the list is read
(`recents` command, `engine/src/recents.rs`): a folder that is gone or refused by R124 shows dimmed with its reason and
cannot be picked, and a refused folder is never stored. Right-click on a row opens Faber's shared one-line menu, "Remove
from recents", which asks once before it forgets (C009). In the project area a "+" on each row builds a selection that
can be opened at once or saved as a named environment (up to 12 environments, 8 folders each; the same name replaces).
Starting an environment opens the folders that still exist as
projects. Faber has no worktree creation for projects, so an environment has no "own worktree" flag. Both lists live in
`recents.json` next to `settings.json`, written through a temp file and renamed.

### Running column

Its rows, cards, file changes and when it opens by itself: [running.md](running.md). "Changes" replaces the running list
inside that same column, with a way back; it never adds a second column. Like the other side views, opening it makes an
automatically opened overlay modal. At most one file's patch stays open, and a partial list offers continuation.
