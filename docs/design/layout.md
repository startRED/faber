# Layout

The window's widths, titlebar, sidebar, the homes, answers, the work log, subagent steps and the running column.

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
  composer does not repeat it), and each answer keeps the effort actually used, in its header's tooltip. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here, and the empty composer idles with a typing bar and rotating hints.
- With no AI connected, the greeting, the suggestions and the list give way to the empty state (Curioso, "No AI connected",
  one line and the button "Connect", which opens account management). The composer stays on screen but cannot send. An open conversation with no AI connected
  shows no composer and no permission line.

### Answers

- The header is the vendor's mark and the model's name, nothing else (Auto shows its reason beside the name only while it is
  still choosing). The effort and Auto's reason are its tooltip. The pace line stays (C002). A wait for quota is told once,
  by the quota card, never also in the header or the red note.
- Under the answer, one line that never wraps, at a 660 px column or a 720 px window: three icon buttons from
  `ui/Glyphs.tsx` (copy, retry, open in terminal, each with a tooltip and an accessible name), a short "Retry stronger"
  when a step up is offered, then the cost in the faint data style. The tokens and how long the cache stays warm are the
  cost's tooltip. There is no model select under an answer: a retry (and the right-click Redo, which names the model)
  uses the composer's current choice.
- There is no permission sentence over a conversation: the mode is a Settings matter.
- While an answer streams the engine sends the whole message every 33 ms, so the text is drawn block by block
  (`ui/markdown.tsx`): a block whose source did not change keeps its DOM and only the open one is parsed and sanitised
  again. A tick that changed only the tools builds no text. Everything that reaches the page passes DOMPurify.

### Work log

- The work log and a helper's row share one disclosure style (`.disclosure`: a quiet card, a chevron, the hover).
- Each step is a quiet chip: a verb in the ink color ("Reading", "Running") and what it acts on in the mono face, cut
  in the middle. A tool Faber does not know keeps its own name; an MCP tool reads "Using a tool" with its server's name.
- While a reply runs and has ended at least one check, a quiet line under the status line counts them (R161 step 9):
  "Ran 3 checks · none failed" after a check mark in the accent's text ink, or "· 1 failed" with the error note's 2 px
  edge and no mark. The words are the subagent facts' own. It goes when the reply ends; nothing moves.
- A file change's chip ends with its line count in the data face, `+N` in the accent's text ink and `−N` in the error
  ink (R161 step 9). While the newest step of a running reply is a file change, the lines it changed sit under the status
  line until the next step or the answer: at most four, one line each in the data face and the third ink, the sign in
  the count's ink, then "N more lines". Nothing moves; the status line already says the step, so the lines are hidden
  from a screen reader.

### Subagent steps

When the agent hands part of the work to a subagent (R161), the reply shows one row per helper between the work log and
the answer, in the work log's surface and type: a check, a cross or the live mark, the few words it was given in bold, and
on the right its kind, its state and its step count in the third ink, and an arrow pointing right. A click opens the
subagent in the running column (R161 step 3, R176), which is the one place on the right: its head has a back arrow to
the running list, the subagent's title and the column's close; under it the meta line, the brief word for word in the
person's own bubble (the agent wrote it to the subagent; Codex gives none, and then nothing shows), the tool chips it
ran, then, once it ended, "What it did" (R161 step 4): a quiet block in the work log's surface with the files it changed
in the data face, its checks and how many failed (a failed count gets the error note's 2 px edge), and the changed files
its brief never named. These are counted by the engine from its steps (`engine/src/engine/facts.rs`), never taken from the
main agent's word. Then its report as Markdown. While it works, a quiet line says a CLI subagent cannot be written to while it runs and
"Tell it…" starts a message to the main agent. The row open there has the hover surface and a 2 px accent edge on its
left (amber on a stuck row). The column opens the way it always does; the content swaps without motion, since it is a
click the person just made. Opening another conversation goes back to the list. Rows sit in a `minmax(0, 1fr)` grid, so a long
tool line ellipsizes instead of widening the reply. A helper that never finished reads "stopped" once the reply has
ended; one sent to the background keeps the live mark until its task notice arrives (a ring, never the square of a stop button); a refused one reads "declined" in
the third ink, never as a failure. Known limit: when a reply moves to another AI after a quota stop, its open tools,
helpers included, read as working again with the new reply.

A Codex subagent (`spawn_agent`, started only when the person asks for one) is the same row. Codex gives it a path instead of a description, so its title is the
path's last part with spaces (`/root/read_docs` reads "read docs"), and it has no kind. Its approvals show on the
conversation's own card, since the person cannot open the subagent; one that arrives after the reply ended is refused,
and Stop and closing the conversation reach the subagent too. What a subagent still does after its reply ended reaches
no reply, so its step reads "stopped". Codex counts each thread's usage on its own, so the
engine adds the subagent's tokens and cost to the reply that started it, and marks the reply's usage incomplete when a
subagent is still at work as it ends.

A subagent waiting on the person's answer reads "waiting for you" with its live mark in amber; the card under the rows is
that answer. One that seems stuck (R161 step 6, R174) is the conversation's other amber: the mark and the row's edge
turn amber, its state reads "seems stuck", and one line under the row says in plain words what the engine saw, with
"Look", which opens it in the running column, and "Tell it…", which focuses the conversation's box with "About “title”: " at
its start (Codex takes it into the running turn; Claude Code after it, or at once with "Send now"). The line arrives with
the normal entrance (`--dur-arrive`, none in reduced motion) and goes when the subagent moves again or the reply ends; the
words carry the meaning, the amber only repeats it. The signs are counted by the engine (`engine/src/stuck.rs`): the
same step six times in a row, the same step failing three times with no file changed between (twice and its retry), or
no step for 10 minutes (30 while a command runs, since builds say nothing until they end). The time a question waits for
the person never counts. Neither CLI lets Faber speak to the main agent while it waits for its subagent, so the main
agent's own chance is the retry the numbers leave it. Background subagents are not followed.

A helper from the other AI (Faber's helper tool, R161) is the same row: its title in bold and, as its kind, the AI that
ran it, so a Claude Code reply reads "Codex". Its commands show under it and its report is its final words. The person
never sees its approval cards: in a mode that asks, what the helper needed approval for is refused and named in its
report, and the agent does that step itself, where the card shows. Known limit: a conversation resumed from before the
helper tool existed keeps its Codex thread without it, since Codex takes dynamic tools only when a thread starts.

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
  A question the agent asks (`AskUserQuestion`) shows instead as a question card, in the thread and in the column alike:
  each question in bold with its short header chip, its options as full-width rows to tap (amber ring when chosen,
  checkboxes when several are allowed), one line to write another answer, then Send answers (enabled once every
  question has an answer) and Skip. No motion beyond the card's arrival; option hovers do not animate under reduced motion.
- Clicking a row opens that conversation, setting the area, the project and the conversation together.
- The column is closed when the window opens. It opens by itself once for each question it has not yet shown, 1.5 s
  after the last keystroke in any text field, including questions already waiting at launch. If the user closes it
  while questions are waiting, those questions never reopen it. Only the button stays amber.
- Items are grouped by project. Projects that have something needing the user come first, and inside a project the
  amber item comes first. The order is stable, so rows do not jump.
- With no items it shows an empty state with Curioso.
