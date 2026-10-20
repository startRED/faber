# The running column

What runs now, on the right of the window: its rows, its question cards, the done cards and when it opens by itself (the
engine's side is the Running column row of [product-map.md](../product-map.md)). The window widths it shares with the
rest of the shell are in [layout.md](layout.md).

- With nothing running it says "Nothing running" once and has no footer sentence.
- A project conversation with changed files has one "Changes" control in the list's head. It replaces the list, like a
  helper or a page, with a back arrow, title and close. Each file is a disclosure with the work log's `+N −N` counts;
  opening it closes the previous file and reads only its patch, so at most one patch stays mounted in total. Added and
  removed lines carry their signs and a quiet background, code and paths wrap, and nothing in the patch moves.
  Closing a file cancels its entrance, so switching quickly cannot reopen it. Loading, empty, binary and too-large
  files have plain words; a failed or timed-out patch says it could not load and offers "Try again" within that open
  file, never the empty state; a new file past
  the counting limit says its count is unavailable. A partial list keeps the files found, says files are still missing
  in one quiet line and offers "Show more files"; it never claims nothing changed. Closing a file releases its patch;
  ending a turn refreshes the list. Agent, page, quick-question and changes views exclude one another. Opening Changes
  in a column that opened by itself uses the other side views' transition: over the stage it becomes modal and traps focus.
- A run that ended while nobody looked keeps a calm card in its block (R242): no fill, a thin line around it, a check
  and "done" (amber when it needs the person), the time it worked and its last line. A click opens it, its × clears it;
  nothing else moves. The sidebar row shows a 6 px accent dot until the conversation is seen.
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
- A reply whose agent has finished, with only the CLI's turn still closing, is not a row: it is done for the person. Keep going,
  Faber's checks, a compaction and a queued message keep the row. The topbar's count follows the column.
- With no items it shows an empty state with Curioso.
- "Quick question" (R250) is a quiet button at the end of the list's head, before the close button. It opens the quick
  question in place of the list, with the back arrow and the close button in its head like an opened subagent; `/btw`
  in the conversation's box opens it the same way, from any width (over the stage below 1,100 px). Each question sits in
  the same bubble as a subagent's brief, its answer below it as a reply, and a quiet "Send to the agent" under a finished
  one; while it runs, "Looking…" and a quiet Stop. The box is the side chat's (Enter asks, Shift+Enter breaks the line),
  with the model picker beside Ask when the conversation's AI has more than one model. Nothing in it moves but the
  list's own entrance (`animateList`).

## Known limits

Changes uses git's path-based reads for tracked-file counts, like git status; the list shows only names and counts locally, and must use pinned reads if it ever shows content.
