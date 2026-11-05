# The line under an answer

The buttons under each message, its hover bar, the reply's state and why a reply stopped. The answer itself, its
header and its plan are in [conversation.md](conversation.md); the work log above it is in [work-log.md](work-log.md).

- Under the answer, one line that never wraps, at a 760 px column or a 720 px window: four icon buttons from
  `ui/Glyphs.tsx` (copy, read aloud, retry, open in terminal, each with a tooltip and an accessible name), a short
  "Retry stronger" when a step up is offered (or, on the newest answer after a fix, "Look around" in the same
  style; never both), then the time sent, how long it took and the cost in the faint data
  style. The tokens and how long the cache stays warm are the cost's tooltip; the whole date is the time's. There is no
  model select under an answer: a retry (and the right-click Redo, which names the model) uses the composer's current
  choice. Edit and Retry keep earlier generated-file cards usable for Save, Show and previews, including when a Chat
  conversation moved into a project; the new conversation keeps its own copy of the earlier default Chat folder.
  Save publishes a complete file before replacing an existing destination; an interrupted copy leaves that destination intact.
  Every inherited project attachment is copied before the new conversation appears, even one from an earlier message;
  a failed copy refuses the edit rather than publishing broken references. Deleting the original leaves these copies intact.
- That line is a hover bar (R264; the owner, 2026-10-17). It stays in the layout, so nothing moves when it
  shows, and is transparent until the pointer is on the message or the keyboard is inside it; it only fades within
  150 ms, using `--dur-fast` and `--ease` (out with `--ease-in`). The last answer is no
  exception. A screen with no hover (`@media (hover: none)`) shows it always; reduced and no motion show it without
  a fade. The person's own message has the same bar under its bubble, right-aligned: the time, copy, edit. An icon
  that answers a press (copy turns into a check for 1.6 s, read aloud into a stop square while speaking) cross-fades
  with a small scale. Read aloud uses the browser's voice for the screen language and skips code blocks.
- The same line carries the reply's state (the owner, 2026-10-18: "you should have said you finished"), after the buttons, in the
  quiet ink and the small size, cut with an ellipsis before it pushes a button: "Working" while the agent writes or works,
  then "Done", "Done · 12 min of work" once the work took a minute or more, and on the newest reply "· updated 3 min ago"
  ("just now" under a minute, none past a day). A working reply shows only this text in the row; the buttons join it when
  done, so the row does not grow. The agent is done when its last message ended and no tool call of the reply is open,
  though the CLI may close its turn up to a minute later (its Stop hooks): from then the buttons appear, the running
  column and the topbar's count leave it out, and the composer reads Send,
  where a message waits in the queue. More words after that undo it. The age ticks once a minute on the newest reply only,
  never while the window is hidden; older replies run no timer. A reply from before times were kept, a stop and an error
  say nothing here. Nothing animates.
- An unconcluded latest reply says so in a quiet decision-style row. A closing request appears as its own row in the thread. A reply Faber was closed under (still running when Faber closed or crashed) says so in the same row in place
  of "Stopped.", with the existing project files note; the newest such reply has a Continue action beside it, which sends Faber's own turn (`wake.closed`) and, when refused, says why in the red note.
- A reply that stopped on its own says why in one line under its words, and never beside "Stopped." (that word is only the
  person's own Stop, which says nothing more). A limit (`reason.turnLimit`, `reason.budget`) is told in the quiet note
  style; a failure (`reason.failed`, and `reason.other` for an unknown ending) keeps the red note (C001).
  A plan limit is the quota card's, and an ending the CLI reports as a stop says nothing. The line is a sentence from
  `ui/locales/`, never the CLI's own word.
