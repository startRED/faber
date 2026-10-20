# Go back before a message

The message menu's confirmation and its saved-message notice, in Chat and Projects. The shell around it is in
[layout.md](layout.md); engine and storage limits are in [product-map.md](../product-map.md).

- A person's message keeps Copy and Edit in its existing menu, followed by Go back before this message. That message
  offers Redo while its restore can be redone; if it was removed, the last remaining message offers it. Recovery lives there
  instead, including after a restart. An empty conversation keeps that menu on its calm saved-message notice. There
  is no toolbar action, setting or second entry point. A message whose file history is incomplete or unavailable
  carries a quiet, named mark.
- One modal confirmation offers Files only, Conversation only and Files and conversation (the default). It names the
  file and message counts, incomplete coverage and redo expiry. Details explain that files over 8 MB are not saved and
  history keeps up to 1 GB for 30 days, deleting the oldest copies automatically. Exclusions use saved project versions,
  permissions, spending and published work, plus ignored and protected files, without storage or filesystem jargon. The file list
  reuses Changes' disclosure preview, one file open at a time; each conflicting file defaults to Keep my file, with
  Restore saved version as its explicit alternative. Paths and choice labels wrap at a 720 px window.
- Preparing, restoring, no file changes, unsaved history and expired history use plain words. An interrupted or partial
  restore says "Restore was interrupted" and offers "Finish restore", even if its preview cannot load. Recovery without
  a complete restore result keeps the partial notice, never claims all files came back. A failed request shows its
  translated error key. Running conversations and their helpers, commands and checks are named with the existing Stop
  confirmation. Go back stays disabled until a fresh preview says all overlapping work is idle; opening or confirming
  never stops work by itself.
  Changing model or effort keeps earlier background work in that same waiting state until its exit is confirmed;
  Stop still reaches its original session.
  Removing an idle project does not block going back in another folder. Normal session closure confirms its tracked
  work has stopped before releasing the waiting state; an unexpected disconnect is still unknown.
  Deleting a conversation keeps its unfinished work in that same waiting state until exit is confirmed, even though
  its messages and list entry are gone.
  That tracking begins before work reaches the session. An ended main command or closed output does not release it
  while a child process may still write; Stop and normal closure wait for the whole owned group to end.
  This includes hooks and other background work that no tool event announced. With no active turn, Stop still ends
  the owner's work. A shared server keeps other conversations running; Stop asks the AI tool to end only the stopped
  conversation's commands. On Unix, an ended command or missing terminal still waits for its sampled children in other
  process groups to end; every conversation on that shared server stays waiting meanwhile. Windows keeps its job proof.
  If exit cannot be confirmed, that conversation says Background work and explains
  that Faber has not confirmed it stopped. Check again enables restore only after confirmation.
- The confirmation says another app with a file already open may change it after the restore. Go back and Cancel
  are its footer. Focus stays inside, Esc cancels while idle and closing returns focus to the menu's message. Repeated
  activation cannot launch another restore. Every state has a static word or mark; reduced and no motion need no animation.
- Restoring, redoing and recovery reload the conversation before releasing the controls. Removed messages disappear,
  and an empty saved conversation keeps its notice and menu instead of returning to the new-chat home. Continuing from
  saved messages leaves one calm thread line: "The AI will continue from the saved messages". Undo and redo need no
  connected AI and never rerun commands or turn Keep going back on.
- Redo remains discoverable as interrupted work until the caller durably saves the conversation and acknowledges it;
  after acknowledgement it disappears and cannot run again. A missing working folder leaves recovery pending with
  `error.rewind.recovery`; bringing the same folder back allows retry without losing its journal.
- Recovery that cannot save a displaced file, including when history is full, returns it to its absent original name without reading or replacing
  another file. It keeps the other saved versions and reports partial recovery instead of blocking work forever.
- Recovery also finishes an empty temporary created before its identity was recorded, preserving unexpected contents
  before cleanup. On Unix startup reaches validated cleanup folders in known project folders and private history,
  including empty ones. Uncertain versions are kept instead of being deleted to free space.
- A conversation with an interrupted restore cannot be deleted, even after confirming the loss of its separate copy.
  The existing error display asks the person to finish the restore first, so recovery keeps the conversation it needs.
  Saving ordinary history may briefly delay deletion, but never replaces the question about unsaved work with a busy error.
