# Conversation

Answers with their bar, the person's messages and the viewer. The work log above an answer is in [work-log.md](work-log.md). Window widths, the sidebar
and the running column are in [layout.md](layout.md); helper rows, memory waits and creative comparisons are in
[subagents.md](subagents.md).

## Faber's decision rows

A quiet labeled row records each decision or message Faber made on its own. It is never the person's voice or the AI's reply. The row names its actor, decision and reason, with a Change action where available; the exact words sent to an AI stay selectable inside a closed disclosure. Wake rows omit a separate reason when their sentence already explains it; helper rows disclose the brief sent to the helper. Rows wrap in narrow windows.

## Answers

- Native visual pieces, HTML/SVG cards and their web-access rules are in [visual-answers.md](visual-answers.md).
- The header is the vendor's mark, the model's name and its effort in the quiet header ink ("· high effort"; the owner,
  2026-10-17), nothing else (Auto shows its reason beside the name only while choosing). Auto's reason is its
  tooltip. The pace line stays (C002); it also says, in the same calm words, when Auto started one rung up because the person often retries that kind of task, only when that changed the pick. A wait for quota is told once,
  by the quota card, never also in the header or the red note.
- Under the answer, one line that never wraps, at a 760 px column or a 720 px window: four icon buttons from
  `ui/Glyphs.tsx` (copy, read aloud, retry, open in terminal, each with a tooltip and an accessible name), a short
  "Retry stronger" when a step up is offered (or, on the newest answer after a fix, "Look around" in the same
  style; never both), then the time sent, how long it took and the cost in the faint data
  style. The tokens and how long the cache stays warm are the cost's tooltip; the whole date is the time's. There is no
  model select under an answer: a retry (and the right-click Redo, which names the model) uses the composer's current
  choice. Edit and Retry keep earlier generated-file cards usable for Save, Show and previews, including when a Chat
  conversation moved into a project; the new conversation keeps its own copy of the earlier default Chat folder.
  Every inherited project attachment is copied before the new conversation appears, even one from an earlier message;
  a failed copy refuses the edit rather than publishing broken references. Deleting the original leaves these copies intact.
- That line is a hover bar (R264; the owner, 2026-10-17). It stays in the layout, so nothing moves when it
  shows, and is transparent until the pointer is on the message or the keyboard is inside it; it fades in and
  slides down 4 px with `--dur-ui` and `--ease` (out with `--dur-exit` and `--ease-in`). The last answer is no
  exception. A screen with no hover (`@media (hover: none)`) shows it always; reduced and no motion show it without
  the slide. The person's own message has the same bar under its bubble, right-aligned: the time, copy, edit. An icon
  that answers a press (copy turns into a check for 1.6 s, read aloud into a stop square while speaking) cross-fades
  with a small scale. Read aloud uses the browser's voice for the screen language and skips code blocks.
- The same line carries the reply's state (the owner, 2026-10-18: "you should have said you finished"), after the buttons, in the
  quiet ink and the small size, cut with an ellipsis before it pushes a button: "Working" while the agent writes or works,
  then "Done", "Done · 12 min of work" once the work took a minute or more, and on the newest reply "· updated 3 min ago"
  ("just now" under a minute, none past a day). A working reply shows only this text in the row; the buttons join it when
  done, so the row does not grow. The agent is done when its last message ended and no tool call of the reply is open,
  though the CLI may close its turn up to a minute later (its Stop hooks): from then the work folds into
  its activity, the buttons appear, the running column and the topbar's count leave it out, and the composer reads Send,
  where a message waits in the queue. More words after that undo it. The age ticks once a minute on the newest reply only,
  never while the window is hidden; older replies run no timer. A reply from before times were kept, a stop and an error
  say nothing here. Nothing animates.
- A reply Faber was closed under (still running when Faber closed or crashed) says so in the quiet note style in place
  of "Stopped.", with "Files it already changed stay changed." in a project; the newest such reply has one plain Continue
  button under it, which sends Faber's own turn (`wake.closed`) and, when refused, says why in the red note.
- A reply that stopped on its own says why in one line under its words, and never beside "Stopped." (that word is only the
  person's own Stop, which says nothing more). A limit (`reason.turnLimit`, `reason.budget`) is told in the quiet note
  style; a failure (`reason.failed`, and `reason.other` for an unknown ending) keeps the red note (C001).
  A plan limit is the quota card's, and an ending the CLI reports as a stop says nothing. The line is a sentence from
  `ui/locales/`, never the CLI's own word.
- In the work log, what the agent wrote between steps shows its last two paragraphs; earlier ones wait under a quiet
  "1 more" link in the third ink (R251). A code block keeps the narration whole. The answer itself never folds.
- The caret blinks only while words arrive; a reply at work after its words shows its step under them in the status
  line's style, or "Thinking…" with its thinking headline once words stop. Older messages without decision rows keep the loop mark after the next reply's model (R241; the
  product map's Work log and Faber's own message say the rest).
- A project agent's plan (R248) sits under the newest reply, above its files, while the agent works and until every step
  is done: a quiet head "Plan · 2 of 5" in the small size, then one line per step, a small square mark before it: a check
  and the third ink when done, an accent ring and the full ink on the current one (`aria-current`), plain when still to
  do; each state is said to a screen reader. Steps the agent hands to conversations of their own (waves, R487) show the same: each one that runs is the current one, so several rings can stand at once, and a step whose conversation stopped stays so until the agent marks it done; each step shows its conversation's title as a quiet link and its state in helper-row words (amber when waiting for you; stopped when its record is missing or unreadable). A step cannot be marked done while its conversation is working or waiting; the agent still marks it done after that work ends. What the agent says it is doing now (its `status`, 3 to 6 words) takes the
  place of the newest step's words under the reply while it works, and an active step shows its current action or thinking line under the steps when it adds to the line above. When the agent gave an estimate and the job went half over it (rule 7), one quiet line in the third ink and the small size closes the block, under the steps: no number, no icon, no colour, it wraps at 720 px. A reply the agent closed with `wrap_up` ends, above
  the actions row, with one box on a hairline: the recap, then at most one question in the semibold weight and its one
  or two replies as buttons, the recommendation the primary one. Only the newest reply, once it is done, offers the
  buttons; a click sends the button's words as the person's message. A question Faber answered while Keep going ran
  keeps no buttons: the reply's answers card says what it chose ([keep-going.md](keep-going.md)). Nothing in the plan or the box
  moves; both wrap at 720 px.
- A completed plan in Plan mode ends with the shared plan approval card ([running.md](running.md)): Approve and run,
  its run mode, and Keep planning. A closing box keeps its recap but no reply buttons while plan approval waits. The composer
  reads the shared Settings mode; the card says it applies to all conversations. Faber's work turn is one quiet line.
- A message's pictures and files sit above its text, right-aligned, as one wrapped row of 72 px rounded thumbnails and
  small file cards (icon, name, type), with "+N" as the fifth tile past five. The bubble has 17 px
  of padding, a line height of 1.6 and at most 620 px. The same thumbnail is drawn in the message box at 56 px, where
  the × sits at its top right corner and shows on hover and focus. A picture opens the viewer: a dark layer, the picture
  growing out of its thumbnail and leaving the same way, arrows (and ← →) between the conversation's pictures, Esc and a
  click outside to close, a count at the bottom, Tab kept inside. Its DOM exists only while open. Pictures inside
  an answer are thumbnails too (at most 280 × 200 px).
- Reading width, text selection, scroll anchoring and the latest-message control are in [reading.md](reading.md).
- There is no permission sentence over a conversation: the mode is a Settings matter.
- The project's own web-search change uses the existing self-change card, with localized current and proposed
  choices, Allow once, its own Always allow choice and Deny. The card explains that stricter choices stop replies
  in progress, including the requesting reply. Engine facts alone supply the choices; unknown input never becomes
  consent text. Its work-log row reads "Changing web search" with the localized destination, without raw JSON.
- A reply waiting on quota offers Stop on its quota card, including when it ended with a recoverable quota error.
  Stop cancels that recovery before the conversation can move into a project. Closing Faber preserves explicit recovery.
  After a move, Wait keeps the earlier messages and attachment locations for the fresh project session. Switching to
  another AI keeps those locations in a full handover or alongside its summary, outside the summary's word limit.
  Approval to use paid credits belongs to that reply and AI until its turn is sent, including while its separate copy is made; Stop revokes it.
- A report requested with `/research` uses this same reply, with Markdown links beside its claims and a Sources list
  at the end. Its planning step uses the shared plan approval card above; Keep planning leaves edits to the composer.
  Retry keeps the report's approved research context; a later ordinary message uses ordinary instructions,
  without interrupting another Codex conversation underway.
  Web searches remain ordinary work-log chips. There is no separate report surface (`tests/ui/research.check.ts`).
- Keep going (R199): the chip, its quiet lines, its quota warning and what it shows on coming back are in
  [keep-going.md](keep-going.md).
- A project reply that opened, queued or pushed a pull request has one pace-style line in Technical details (R200): its words ("Following
  pull request #93: checks running.") link to the page, and "Stop following" sits beside them while it is open. It turns
  amber when it fails, conflicts or needs the person. When checks fail, Faber's own message is the loop
  mark on the next reply, its tooltip naming the pull request and its failed checks.
- A project reply that changed files, in full access, has Faber's own checks (R146) in Technical details in the same pace-style line,
  above the pull request's: "Faber is running the checks on these changes…", then "Faber's checks passed (3 checks)." or
  "Faber's checks failed: npm run lint." with "What failed" under it, the work log's disclosure holding each failed check's
  first three short failures in the data face. Faber's fix turn is the loop mark on the next reply ("Faber's checks failed (npm run
  lint). Faber asked the agent to fix them." in its tooltip); after two in a row, or where Faber does not ask (the reply asked the
  person something, Keep going stopped on it, the pace is used up), the line turns amber and says it needs the person.
  While the checks run the conversation reads as at work: the composer shows Stop, and a message written then waits in
  the queue. Changing permissions or reply style leaves a scheduled compaction and its exact continuation waiting for
  the checks; an automatic correction after failed checks may replace the session with the new settings without
  losing that pending work. A retired Codex server closes once its last
  session finishes. The shared running-state refresh handles every end of work, including Stop, passed or failed checks,
  Keep going ending by itself or being turned off, archive and quota stops; removing a conversation closes its session
  too. A settings relaunch lets background commands finish on their original driver while the next reply uses the new
  settings. Commands announced after replacement still show their output and keep their own Stop. Waiting for an old
  shared-server thread to close blocks rewind without keeping the current idle session alive. Stop leaves waiting messages in the queue without
  keeping a retired session alive; the person can send them later on a new session. Nothing moves, and no setting or first-run step comes with it.
- While an answer streams the engine sends the whole message every 33 ms, so the text is drawn block by block
  (`ui/markdown.tsx`): an unchanged block keeps its DOM and only the open one is parsed and sanitised
  again. A tick that changed only tools builds no text. Everything that reaches the page passes DOMPurify.

- [Queued messages and editing](queue.md).
- Agent-to-agent recados use that same queue with a quiet origin line while pending, never the person's message
  bubble. Ordinary work needs no manual send; held work keeps the queue's recovery controls. Once delivered,
  the reply keeps the origin in its existing header mark.
- [Branches and versions](branch.md).

## Go back before a message

Copy/Edit's menu adds Go back, Redo or pending recovery, including after restart. Its confirmation and saved-message
notice are in [rewind.md](rewind.md). No toolbar action or setting is added.
