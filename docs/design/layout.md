# Layout

The window's widths, titlebar, sidebar, the homes and the running column. An open conversation (answers, work log) is in
[conversation.md](conversation.md); search, the command box and find are in [search.md](search.md).

## Layout

The window has a 36 px titlebar across the full width, a 288 px sidebar, a stage with a 52 px topbar, a reading column of
at most 760 px centred in the stage, and a running column on the right. The window can be as narrow as 720 px,
so the layout changes with its width:

| Window width | Sidebar | Running column | Reading column |
| :--- | :--- | :--- | :--- |
| About 1,030 px and wider (whenever the stage keeps 440 px beside the column) | 288 px, beside the stage; it folds away while a wide view would squeeze the stage under 560 px | Its view's usual width when open, beside the stage | Shrinks with the stage, 760 px at most. The stage is never narrower than 440 px |
| 900 px to where the column no longer docks | 288 px, beside the stage | When open, an overlay on the stage | Up to 760 px |
| 720 to 899 px | Hidden. A button in the topbar opens it as an overlay | When open, an overlay on the stage | Up to 760 px |

In the narrow sidebar overlay, switching between Chat and Projects keeps navigation open so the chosen area's rows remain reachable. Choosing a project or conversation, opening Catalog, Escape or the scrim closes it. Tab stays inside the modal overlay; Escape returns focus to the topbar menu button. Wide navigation stays beside the stage. The narrow sidebar remains painted through its full-motion exit, then becomes hidden; reduced and no motion hide it immediately. Closing makes it inert immediately, even while the exit is visible.

The What's New page starts below the 36 px titlebar. Its scrolling viewport never extends behind the brand or window
controls, including when focus scrolls to Continue in a short window (`tests/ui/notes.check.ts`, `notes: scrolling`).

Text size up to 18 px must not push the page past the window width at any of these widths: content wraps or scrolls
inside its own area.

### Arranging the shell

The numbers above are the usual widths. The person can change them, and the shell keeps what they choose (R211).

- **Edges.** The sidebar's right edge and the running column's left edge are separators (`role="separator"`, focusable).
  A drag moves them between 200 and 420 px (sidebar, 288 by default, as Claude Desktop opens its own) and from 300 px to as wide as the window lets it (column: a docked one leaves the stage 440 px; one over the stage, the work area). While the column is docked beside the stage, the sidebar
  leaves the column and the stage their minimums (360 px at most in a 1,100 px window). A docked column also leaves the stage 440 px:
  the stylesheet clamps it with the same numbers, so a window that shrinks after a drag is right before any script runs.
  Only a short rounded grip appears on hover (after the fast duration, so crossing the edge flashes nothing): on the frame's side of the
  sidebar's edge and just inside the column's, never a line the window's height. It brightens while dragged, is accent when
  focused, and its name is its tooltip. The arrow keys move an edge 16 px (48 with Shift), Home and End go to its limits, a double
  click or Enter glides it back to the usual width. The sidebar has no edge below 900 px, where it is an overlay.
- **Each view of the column has its own usual width** (`paneUsual`, R428). A page in Faber's browser, Changes, a visual answer and a file open at 60% of the work area (the window less the sidebar); the quick question at 420 px; the running list, Commands, Technical details and a helper keep `autoRun`: the window less the sidebar less 800 px, 360 to 560 px (`RUN_USUAL`). A drag is kept for the view's kind only (`faber.layout.panes`: `list`, `browser`, `changes`, `visual`, `file`, `aside`), so widening the browser leaves the running list alone. A drag ending on the usual width, a double click or Enter goes back to following it, for that kind. The width of the old single `run` value becomes every kind's, once. Switching views while the column stays open glides the width in the column's own motion (`.shell.glide`, instant with reduced or no motion); opening and closing it are the stage's move.
- **Docking is by room.** The column sits beside the stage whenever the window leaves the sidebar (at most its usual 288 px, it gives way), the column's 300 px minimum and the stage's 440 px (`dockable`, about 1,030 px with the usual sidebar), instead of the fixed 1,100 px; under 900 px the sidebar is an overlay and the column covers the stage. When a wide view (browser, Changes, visual answer, file) would leave the stage under 560 px, the sidebar folds away like focus mode's (`.shell.folded`, the stage keeps the room) and the usual width is then 60% of the whole window; it comes back when the view closes or another kind replaces it. The fold is derived, never stored. A subagent can fill the stage ([subagents.md](subagents.md)).
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
- Before the run button, the open conversation's Context meter (R221): a 16 px ring and "{pct}%" in the quiet second ink, with the tokens in its tooltip. It is status (C003): it shows once the CLI has said both numbers, with no motion or setting (C011). A click opens a small panel under it (R274), on the right-click menu's surface and motion and anchored to its right edge: the tokens line, what fills the context (a quiet two-column list, the part at the left and its tokens at the right) when Claude Code counted it; below a hairline, "Catalog tools in this conversation": each tool the agent has now as an accent link with its Catalog name, and under it, in the quiet third ink, why it was picked ("Picked for: Fixing bugs"); a click closes the panel and opens that tool's Catalog detail, and with none one quiet line says so; then below another hairline "Compact now" with one quiet line under it. Nothing in it is a setting; Esc or a click outside closes it. It turns amber only from 85% of the window, near where the CLI compacts by itself (C002), never sooner.
- Between the Context meter and the run button, the open conversation's pane buttons (R303): quiet 32 px icon
  buttons with their names in the tooltip: Quick question (Ctrl+;), Changes (only while the project conversation has
  changed files) and Faber's browser (a project conversation on Windows). Each puts its own view in the right column,
  apart from the running list; the one on screen stays lit (`aria-pressed`) and pressing it again closes the column.
  The run button is the way to the running list, so these views have no back arrow. Where the column does not dock (about 1,030 px), a column the person
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
with its title and close; it never adds a second column. Docked (about 1,030 px and wider), the column is a card of its own beside
the stage, the frame between them and every corner as round as the stage's; a look with no inset keeps it flush. Faber's
browser's head is its tab strip: tabs share it from 180 px down to 76 px each (the site's letter in a 16 px square, the
title in one line, close on the tab in front and on hover or focus), the one in front on the raised surface, then New tab,
expand and close. Under it one bar: Back, Forward and Reload (dimmed at 40% when they cannot act; under 300 px Forward
goes first), the address field as a filled pill (site in normal ink, the rest quiet until focus, a 2 px line along its
bottom while the page loads) and More. The page shows in a rounded box under the bar, with nothing repeating the site:
the address names it. A newer submitted address or tab choice supersedes older pending action replies; the earlier
reply cannot reset the visible address, page or opening error after another choice or once the pane starts closing.
A native page navigation preserves an address while the person edits it; a completed tab choice explicitly resets the draft.
An accepted navigation keeps its address before returning, so a quick tab switch or reopen cannot restore a blank page.
Background title and redirect updates keep the latest pane generation across conversation changes; their history reads
and writes serialize with navigation, while the existing two-second coalescing interval remains bounded to one worker.
Closing keeps the real column, its content and its layout slot until it slides fully to the right, without a fade.
Hiding or showing the column clears the view-width glide and its deadline, so a quick reopen cannot move its layout
base in the opposite direction while its real content exits.
It declines input and releases modal focus immediately. Reopening restores input, modal state and focus as a new opening does, even before the exit finishes. Reopening or opening another view cancels the exit; completing
the exit never takes a newer focus choice. Reduced or no motion closes immediately. A bounded fallback releases the
pane even if a hidden window holds its animation timeline; an already departed column gets no second exit copy.
During CSS or Web Animations motion of its ancestors, the native page follows the box each frame for the existing bounded motion window.
Only one placement may be in flight; further positions collapse to the latest measurement. Closing drops a deferred
placement and parks the box. Layers covering the page still take it out of the window. A departing inert column is
hit-tested as painted content synchronously, with its input block restored before returning; the stage behind it
does not hide its native page during an overlay exit either.
The pane is a native rectangle, so the shell draws the box's corners over the page in the card's colour, smooth, and gives
the page the screen's thin scrollbars (`src-tauri/src/pane/round.js`, in a world of its own the page's scripts never see,
`site_round`). With no page yet the box stays, calm, with a globe and what it is for in its middle. Where Firefox keeps a profile, the
More menu has "Sign in with your Firefox": a dialog on the rewind dialog's surface (the page steps aside
for it like for any layer) with one primary action per step, Continue, then Bring over; the site list scrolls inside a
hairline box, sensitive sites marked with an amber pill, never a colour alone. Like the other side views, opening it makes an
automatically opened overlay modal. At most one file's patch stays open, and a partial list offers continuation.

A project conversation also opens "Technical details" in this same slot, from one quiet plain line under the thread
or beside its running row. Its head has Back, the title and Close; the existing modal, focus and width rules apply.
It reuses the command rows, check and follow controls, and Changes' file disclosures, with facts newest first within
each history. Only the open view holds DOM and at most one file patch; no extra reader, timer or stored history.

Its first group is one list of background work: existing shell rows (named by their parsed description when unique),
monitors, helpers, Faber's checks and followed pull requests. Quiet wrapping chips carry one state word each, with no
motion: running, passed, failed, stopped, "waiting for you", "no report yet" (a monitor or shell from an earlier reply
with nothing reported, neutral), or the helper's own words ("seems stuck" with what was seen, R174). Amber only for
failed, stuck or needs the person. Helper chips open the existing helper view; a running shell has Stop on its row and
its output behind a disclosure mounted only while open. Completed tool chips stay until their reply ends, then their
existing histories retain the evidence. A pull request that merged or closed leaves the list and shows under Saved
changes; a pending one reads "checks running". A shell reads the same state word as in Commands, and the last Stop ends every background call before it. Helpers keep their rows in the Projects thread (R161, R174). The thread
line counts the same running rows with the same state rule ("N things still running").

Known pull request numbers and unique observed head-hash prefixes in outputs or the "References in replies" group (its
own group, shown only when a reply has a reference, never under Saved changes) carry the existing follow's newest state.
Unknown or ambiguous references stay literal. Passing checks never claim a landing; merged is shown only when observed.
Closing the pane releases its output and reference elements.

This plain line uses only the latest reply’s Faber checks and engine needs-you state, shared with the Running row
and done card. Failures, fixing, needed decisions and unfinished closing boxes stay amber with the column closed
at 720 px; the person’s Stop stays neutral. Saved counts include only engine-confirmed commits. When none of those
needs the person, it counts the work still running, not the work waiting or unreported. A followed pull request of an older reply that
fails or conflicts is amber in the list while the line, which reads the latest reply only, stays neutral. It also carries the
wait while Faber looks for changed files to check; full command names and results stay in the pane. Check endings retain their polite live announcement, in plain words in Projects,
with amber text and edge on failure.

When closing interrupts a column's width glide, the exit temporarily fixes its currently painted box in a manual native top layer, with no DOM reparenting. Ongoing parent grid reflow cannot pull the departing real content backwards; cancelling the exit restores ordinary layout without inline geometry.
