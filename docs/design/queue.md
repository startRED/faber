# Queued messages

- Messages sent by another conversation's agent, including a finished helper's report, show their existing localized
  origin as one quiet line, without the internal prompt or the person's editing controls. During normal work they
  need no manual send. If work stopped or delivery was refused, the line keeps Remove and Send now so the person can
  discard or explicitly resume it. They join
  the running turn automatically when none of its tools is open, or at its next step boundary; Stop, queue order,
  subscription and quota limits still apply. A message received after Stop stays here even when the conversation
  is idle and its queue was empty; only an explicit resume can start it. A completion report refused before its turn can start stays in this
  same queue with its origin and words intact. Refused questions still fail without leaving a duplicate retry.
  A full inbox keeps a pending notice on the child instead of discarding its completion. When a slot opens,
  the latest answer enters behind the messages already waiting; reopening Faber also retries notices with room.
  A stable child/reply id prevents duplicate notices after an interrupted save. The inbox keeps its ten-message
  limit, and a child has at most one pending notice while its full answers stay in its conversation.
  Other automatic continuations keep waiting for their own turn.
- A queued chip keeps its badge, words and attachments. Edit uses the same quiet text button as Remove and Send now;
  while editing, a small text box replaces only the words and Save and Cancel replace the actions. Enter saves,
  Shift+Enter adds a line and Escape cancels from any control in the editor form; composition input never saves or
  cancels early. Nothing animates, including under
  reduced or no motion. The box and its actions wrap within the conversation column at 720 px.
- A message waits as a queued chip until the agent's step ends, then is handed to the running turn; Send now hands it
  over at once. A handed message swaps the badge for "Sent: the AI reads it at its next step" and drops the actions
  row: the CLI holds it, so Edit, Remove and Send now would do nothing.
- Save changes the text in place, keeping the message's id, position, images and files. A message handed to the CLI
  cannot be edited. A save refused because the message left the queue closes the editor and returns its words to the
  conversation's message box, after a blank line if it already holds a draft, with focus there and one calm status
  line explaining what happened. A queue event removing the row does the same. Cancel returns focus to Edit, or to
  the message box if Edit no longer exists. No detached queue editor is kept alive.
- Drafts use the existing unsent store. Opening a conversation recovers saved queue edits whose messages have left,
  even after Settings or a restart, into that conversation's message box. A recovered edit is consumed once. While
  its message still waits, an edit returns to its own row instead. The editor counts as unsent work so an unattended
  update cannot close it. Chat and Projects use the same chip; the composer itself is unchanged.
- Compact and clear continuations wait behind existing messages. After an error, Faber's own queued messages can
  continue oldest first while the person's queued messages remain held; Stop holds both. Keep going keeps its failure
  waits and limit. Clearing starts the fresh session with any waiting messages before the handoff note.
- Checks: `engine/tests/compact.rs` and `engine/tests/keep_going.rs` cover continuation order and failed-turn wakes.
- Checks: `tests/ui/edit-queued.check.ts` covers keys, focus and draft recovery, including a recreated app;
  `testkit/src/screen/app.rs` sends the edited words through the real command to the fake CLI.

