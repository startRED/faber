# Layout

The window's widths, titlebar, sidebar, the homes and the running column. An open conversation (answers, work log) is in
[conversation.md](conversation.md); search, the command box and find are in [search.md](search.md).

## Layout

The window has a 36 px titlebar across the full width, a 248 px sidebar, a stage with a 52 px topbar, a reading column of
at most 760 px centred in the stage, and a running column on the right. The window can be as narrow as 720 px,
so the layout changes with its width:

| Window width | Sidebar | Running column | Reading column |
| :--- | :--- | :--- | :--- |
| 1,100 px and wider | 248 px, beside the stage | 360 to 560 px when open, beside the stage | Shrinks with the stage, 760 px at most. The stage is never narrower than 440 px |
| 900 to 1,099 px | 248 px, beside the stage | When open, an overlay on the stage | Up to 760 px |
| 720 to 899 px | Hidden. A button in the topbar opens it as an overlay | When open, an overlay on the stage | Up to 760 px |

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
- **The column's usual width follows the window** (`autoRun`): the window less the sidebar less 800 px, 360 to 560 px. A
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
- Before the run button, the open conversation's Context meter (R221): a 16 px ring and "{pct}%" in the quiet second ink, with the tokens in its tooltip. It is status (C003): it shows once the CLI has said both numbers, with no motion or setting (C011). A click opens a small panel under it (R274), on the right-click menu's surface and motion and anchored to its right edge: the tokens line, what fills the context (a quiet two-column list, the part at the left and its tokens at the right) when Claude Code counted it, and below a hairline "Compact now" with one quiet line under it. Nothing in it is a setting; Esc or a click outside closes it. It turns amber only from 85% of the window, near where the CLI compacts by itself (C002), never sooner.
- Between the Context meter and the run button, the open conversation's pane buttons (R303): quiet 32 px icon
  buttons with their names in the tooltip: Quick question (Ctrl+;), Changes (only while the project conversation has
  changed files) and Faber's browser (a project conversation on Windows). Each puts its own view in the right column,
  apart from the running list; the one on screen stays lit (`aria-pressed`) and pressing it again closes the column.
  The run button is the way to the running list, so these views have no back arrow. Below 1,100 px a column the person
  opened sits over the stage as a modal dialog and the topbar waits behind it: its close button or Esc closes it.

### Sidebar

The area lists, pinning, archive and footer are in [sidebar.md](sidebar.md); search is in [search.md](search.md).

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
- File mentions, commands and earlier messages share the list over the box ([composer.md](composer.md)).
- Auto chooses both the model and its effort. A manual model adds an adjacent low, medium or high effort choice,
  defaulting to medium (R102). Each conversation remembers its choice. Changes apply to the next message only (the
  composer does not repeat it), and each answer keeps the effort actually used and shows it in its header. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here, and the empty composer idles with a typing bar and rotating hints.
- With no AI connected, the greeting, the suggestions and the list give way to the empty state (Curioso, "No AI connected",
  one line and the button "Connect", which opens account management). The composer stays on screen but cannot send. An open conversation with no AI connected
  shows no composer and no permission line.

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

Its rows, cards, file changes and when it opens by itself: [running.md](running.md). The topbar's pane buttons put
Changes, the quick question or Faber's browser in that same column in place of the running list, each a view of its own
with its title and close; it never adds a second column. Like the other side views, opening it makes an
automatically opened overlay modal. At most one file's patch stays open, and a partial list offers continuation.
