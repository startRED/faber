# Layout

The window's widths, titlebar, sidebar, the homes, answers, the work log, pages in Faber's browser and the
running column.

## Layout

The window has a 36 px titlebar across the full width, a 248 px sidebar, a stage with a 52 px topbar, a reading column of
at most 660 px centred in the stage, and a 352 px running column on the right. The window can be as narrow as 720 px,
so the layout changes with its width:

| Window width | Sidebar | Running column | Reading column |
| :--- | :--- | :--- | :--- |
| 1,100 px and wider | 248 px, beside the stage | 352 px when open, beside the stage | Shrinks with the stage, 660 px at most. The stage is never narrower than 500 px |
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

### Sidebar

- At the top: "New conversation" (Ctrl N), then the two areas, Chat and Projects, the latter with a count.
- The sidebar lists only the open area's items (R168): Chat lists the chats and no Projects block, not even an empty
  one; Projects lists the projects with their conversations and the add-folder control, and no chats. Rows keep their DOM
  when the list refreshes, so a refresh moves nothing the person is pointing at.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- Chats are grouped as Today, This week and Earlier. Deleting a conversation asks for confirmation.
- The footer is one row, the person's name when they gave one and Settings, then one thin line per connected vendor: its
  shape, a 3 px bar and the used share of its tightest quota window, in the small size. Nothing on the line wraps. The
  window, the reset, how old the reading is, the not-yet-counted estimate and the vendor's name are its tooltip. While
  use is still being counted the share reads "≥ 44%" (the real share can only be higher), with the words in the tooltip
  (C010). The bar is amber only when the engine says the quota is nearly used. The line opens Settings > Accounts and limits.
  A vendor with no reading yet, or only an expired one, shows "—". The version is not on the sidebar: it is the
  tooltip of the Settings button.
- Search filters conversation titles and project names on screen; full-text search comes in Phase 2. The percentage and
  "stopped" tails come with the progress view in Phase 3.

### The new conversation screen

- One column, like the Projects home (C005): the greeting, the composer, the suggestions and what to pick up, top to
  bottom. No line under the greeting: the composer's placeholder already says to ask anything, and its permission chip
  shows whether Chat may change files.
- A greeting, "Where do we start?", ending with the person's name when they gave one.
- Three short hand-written suggestions per language, as chips under the composer: one row at the usual text size, the
  last chip wrapping only on a narrow window or a large text size. Clicking one fills the composer and moves focus there. It sends nothing. A hover changes only the chip's colour.
- "Pick up where you left off" lists up to three conversations of both areas under the suggestions: those that need the
  person first, then the most recently updated.
- The composer keeps its model picker, "AI: choose for me".
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
- There is no permission sentence over a conversation: the mode is a Settings matter.
- Keep going (R199): in a project conversation under way, the composer bar has a "Keep going" chip after the model's,
  with the loop icon; pressed (`aria-pressed`) it takes the accent's soft fill and ink, never the amber, and its tooltip
  says the composer's Stop ends it. A message Faber sent by itself is one quiet line with the same icon where the
  person's bubble would be ("Faber asked it to keep going"; what the agent was told is its tooltip), and a reply after
  which it stopped by itself ends with one calm line that says why, in the same style (C001). Nothing moves.
  When every AI it may use is near its limit, spent faster than its daily line, out, or billed per use (R216), one
  amber line sits right above the composer, at its width, with the loop icon: before it turns on, the chip's first click shows it with
  "Keep going anyway" and "Not now" (turning it off never asks); while it is on, the same line stays without buttons and
  says until when it waits once every AI is out. The sidebar and the running column say "Waiting for quota until …".
  It wraps at 720 px and does not move.
- A project reply that opened, queued or pushed a pull request ends with one pace-style line (R200): its words ("Following
  pull request #93: checks running.") link to the page, and "Stop following" sits beside them while it is open. Only checks
  that wait for the person turn it amber. When the checks fail, Faber's own message is Keep
  going's quiet line, naming the pull request and its failed checks.
- A project reply that changed files, in full access, ends with Faber's own checks (R146) in the same pace-style line,
  above the pull request's: "Faber is running the checks on these changes…", then "Faber's checks passed (3 checks)." or
  "Faber's checks failed: npm run lint." with "What failed" under it, the work log's disclosure holding each failed check's
  first three short failures in the data face. Faber's fix turn is Keep going's quiet line ("Faber's checks failed (npm run
  lint). Faber asked the agent to fix them."); after two in a row, or where Faber does not ask (the reply asked the
  person something, Keep going stopped on it, the pace is used up), the line turns amber and says it needs the person.
  While the checks run the conversation reads as at work: the composer shows Stop, and a message written then waits in
  the queue. Nothing moves, and no setting or first-run step comes with it.
- While an answer streams the engine sends the whole message every 33 ms, so the text is drawn block by block
  (`ui/markdown.tsx`): a block whose source did not change keeps its DOM and only the open one is parsed and sanitised
  again. A tick that changed only the tools builds no text. Everything that reaches the page passes DOMPurify.

### Work log

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

- With nothing running it says "Nothing running" once and has no footer sentence.
- One block per project, plus one "Chat" block. Each row shows the vendor's shape (a neutral dot while Auto is still
  choosing the model), the conversation title, the state in words ("Working", "Needs you" or "Choosing the model") and
  the time since the turn started.
- An item that needs the user shows the permission card, and the user can answer in place with Allow, Always or Deny.
  A question the agent asks (`AskUserQuestion`) shows instead as a question card, in the thread and in the column alike.
  Several questions are steps: a strip of their header words (a number, a tick once answered), one question open at a
  time in the same cell, so the card keeps one height; the others wait beside it and glide in from the side. Each answer
  is a tile (radio or checkbox mark that fills with a tick, title, muted description of at most two lines, a small
  "Recommended" badge), two columns when the card is 640 px wide or more and one below; the chosen tile gets the amber
  ring. A preview the agent drew sits beside the tiles (below in a narrow card) in a quiet monospace box that fades when
  the tile in focus changes. Another answer is one quiet button that opens its field. The footer holds "n of m answered",
  Skip and Send answers (amber, enabled once every question has an answer). Once answered the card in the thread folds away over the
  exit time while the thread closes up under it. Tiles and steps change colour over the fast time and the mark scales in
  from half size; under reduced motion everything is instant and the state is still carried by the filled mark, the ring
  and the tick.
- Clicking a row opens that conversation, setting the area, the project and the conversation together.
- The column is closed when the window opens. It opens by itself once for each question it has not yet shown, 1.5 s
  after the last keystroke in any text field, including questions already waiting at launch. If the user closes it
  while questions are waiting, those questions never reopen it. Only the button stays amber.
- Items are grouped by project. Projects that have something needing the user come first, and inside a project the
  amber item comes first. The order is stable, so rows do not jump.
- With no items it shows an empty state with Curioso.

### Search and commands, and find

- **Ctrl+K** opens one box over everything, centred sideways and set in the upper third so it does not jump as the results
  change. It scales from 0.97 and fades in with `--dur-ui` and leaves faster with `--dur-exit`; behind it the screen dims
  with a veil of its own colour, never a blur (only the sidebar scrim blurs). It is a modal dialog: focus goes to its box,
  Tab stays inside (`ui/focus.ts`), Esc or a click outside closes it and gives focus back, and the shortcuts of the screen
  underneath wait. The box is the shared search field (`.field.search`), taller (`--h-lg`), with no ring of its own; rows use
  `--h-md`, the chosen one on `--s3`, and the project or area of a conversation sits quiet at the right. Empty, it lists
  "Recent" (five conversations) then "Actions"; typing gives one list ranked across everything, eight rows at most.
  Moving the choice by arrow changes its colour only (`--dur-fast`), no travel. It exists in the page only while open.
- **Ctrl+F** in an open conversation slides a bar down from the top right of the thread (12 px and a fade, `--dur-ui`; it
  leaves with `--dur-exit`) with the shared search field, "3 of 12", previous, next and close. Matches are painted with the
  CSS Custom Highlight API: all in the accent at 42 %, the current one in the full accent, so no message is rewritten and
  nothing is wrapped. Enter and Shift+Enter move with a smooth scroll that centres the match unless it is already in plain
  view (instant in reduced and no motion); Esc closes the bar before the sidebar or the running column. When the running
  column covers the thread, the bar slides left of it. Both are instant in reduced and no motion.
