# The line under an answer

The buttons under each message, its hover bar, the reply's state and why a reply stopped. The answer itself, its
header and its plan are in [conversation.md](conversation.md); the work log above it is in [work-log.md](work-log.md).

- Under the answer, one line that never wraps, at a 760 px column or a 720 px window: four icon buttons from
  `ui/Glyphs.tsx` (copy, read aloud, retry, open in terminal, each with a tooltip and an accessible name), a short
  "Retry stronger" when a step up is offered (or, on the newest answer, "Check the work" when its code changed with
  nothing checking it, or "Look around" after a fix, in the same style; never two of them; both only start the box
  with a request the person reads and sends, R565), then the time sent, how long it took and the cost in the faint data
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
- A Codex reconnect notice with `willRetry: true` keeps the reply working and adds no error to its history. If retries
  fail, the turn's completion still reports the connection failure in the answer's usual error line.
  [App-server errors](https://learn.chatgpt.com/docs/app-server#errors)
- A silent native Codex web lookup can reach the same 15-minute cutoff as an answer without a running tool. Faber
  requests a stop for that turn only when it sees no other active or unconfirmed work or message delivery in that
  conversation. The existing notice distinguishes a confirmed stop from an unconfirmed one. A confirmed stop follows
  Keep going's usual retry rules when enabled.
- A Claude `WebSearch` at the same cutoff can send one graceful interrupt per reply. The driver rechecks the current
  tool, queued messages, approvals, background work, and its own process tree before sending it. A resident subprocess,
  including a stdio tool server, keeps the request deferred. A matching acknowledgement followed by an interrupted
  result marks the reply as silent if the model has not resumed work. An ignored interrupt leaves Stop available.
- The Codex service has 30 seconds to answer its initial handshake before Faber retires it. No waiting turn has been sent
  at that point. Once writer exit is confirmed, the reply names the startup failure and queued messages follow the usual
  retry rules. A running service with uncertain delivery still needs a deliberate send.
- An isolated or internal Codex conversation has a separate 30-second deadline for the configuration read that
  precedes opening its thread. A timeout or refused read releases that conversation without sending its turn, and
  leaves the service available to other conversations. Closing during that wait discards the pending read. Late
  replies cannot open the discarded thread. If Stop already ended the reply, the unopened session handle is retired
  when its service closes or its read fails.
- An opening request on an already running Codex service has 30 seconds to start a thread and 90 seconds to
  restore or fork one. If the reply is lost, only that conversation fails. Faber keeps its messages, never
  submits the abandoned input, and leaves peers running.
  A late reply can identify a runtime for cleanup if that identity belongs to no peer. The copy stays leased until
  writer exit can be confirmed.
- A native fork has a separate 90-second deadline to verify its last saved turn. Closing the conversation or losing
  that verification discards its waiting input. Faber can clean up the already identified fork without relying on
  the late read, while leaving the source and other conversations alone.
- After a native rewind opens its thread, the history change has its own 90-second deadline. A timeout or close
  discards the waiting input. On the shared service, another request for the same history falls back to saved
  messages while the earlier change is unresolved. A matching success or a request error lets Faber ask the service
  to unload the original runtime if no peer owns it. An unclear reply or a service that cannot confirm unload keeps
  the copy leased until the process exits. The
  [Codex revert response](https://raw.githubusercontent.com/openai/codex/rust-v0.166.0/codex-rs/app-server-protocol/schema/typescript/v2/ThreadRevertResponse.ts)
  identifies the updated thread.
- A protected resume has 30 seconds to confirm that the previous subscription was removed before opening
  with the new identity. If that confirmation is lost, the replacement fails without opening a thread, and a
  late confirmation cannot move it forward. A replacement still waiting for the previous runtime or another
  pending opening has 90 seconds to enter that check. Expiry stops only the replacement.
- A resume on Codex 0.166.0 or later asks for metadata without transferring the stored turns back to Faber.
  Both the native history and Faber's visible transcript remain available. In a Windows control with 24
  saved turns, the reply fell from 190,125 to 6,264 bytes. Six fresh services restored the same native ID and
  history, with one usage announcement each. See the
  [Codex 0.166.0 resume schema](https://raw.githubusercontent.com/openai/codex/rust-v0.166.0/codex-rs/app-server-protocol/schema/typescript/v2/ThreadResumeParams.ts).
- If Codex rejects the request to start an idle conversation's turn, Faber ends that reply and records the error.
  Other conversations on the service keep working. A rejected submission leaves active turns and compaction running.
  A late response to an earlier turn cannot end a newer one. [App-server message schema](https://learn.chatgpt.com/docs/app-server#message-schema)
- When the shared service that drives two or more active replies falls silent, Faber sends a local health check.
  With one active reply and a native MCP tool still open, it checks after 15 minutes of silence. An open native command
  uses 30 minutes instead. The check sends no model request and leaves a responsive service and its ongoing work alone.
  A Stop for a known turn can also trigger one check if its completion notice is still missing after five seconds.
  Acknowledging an interrupt and reporting the end of the turn are separate messages in the
  [service protocol](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/app-server/src/bespoke_event_handling.rs#L1235-L1250).
  A health reply or another service protocol message gives the end notice another five seconds. Unstructured output and
  notifications replayed internally by Faber cannot satisfy the check. [App-server protocol](https://learn.chatgpt.com/docs/app-server#protocol)
  The Stop check protects approvals and questions in other conversations. Silence-triggered checks wait while any
  conversation on the service has a pending approval or question, then start a fresh quiet window.
  A service that responds stays open. If it stays silent and the affected writers can be confirmed stopped, the reply
  says Faber stopped the unresponsive service and kept its history. If writers are still unconfirmed, the reply says
  so and stays open instead of claiming that it stopped.
  If a queued message had already been handed over and its receipt is missing, it stays in the queue for the person
  to check. Keep going stops on that reply so the queued message cannot be sent twice.

## Before and after

Under a finished project answer whose app pages Faber pictured before and after it (`engine/src/engine/pictures.rs`),
one quiet row sits below the reply's files and pages (`ui/BeforeAfter.tsx`): a small label in the third ink, then for
each page its before and after thumbnails side by side, 96 × 60 px, each captioned "Before" or "After" in the small
size. A thumbnail opens the shared picture viewer, whose arrows move through that answer's pictures only, in the row's
order. A page missing either picture, an answer with none, or one still running shows nothing. The thread reads every
answer's pictures in one call when it opens and when its newest answer finishes, and reads again at growing gaps for
about 30 seconds while that answer waits for an after picture; another conversation on screen starts over and never
shows the last one's pictures. Nothing moves. A row's pictures load once it scrolls into view, as data URLs through
`site_picture`, and go with the row (`tests/ui/before-after.check.ts`). The engine reads no record over 64 KiB, at most
the newest 500 records and three pages each, and follows no link or junction.
