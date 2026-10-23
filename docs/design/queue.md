# Queued messages

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
- Checks: `tests/ui/edit-queued.check.ts` covers keys, focus and draft recovery, including a recreated app;
  `testkit/src/screen/app.rs` sends the edited words through the real command to the fake CLI.

