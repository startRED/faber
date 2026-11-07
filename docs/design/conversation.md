# Conversation

Answers with their bar, the person's messages and the viewer. The steps and thinking inside an answer are in [work-log.md](work-log.md). Window widths, the sidebar
and the running column are in [layout.md](layout.md); helper rows, memory waits and creative comparisons are in
[subagents.md](subagents.md).

Electron requests a browser-storage flush whenever the remembered place changes. After a forced close, the previous
thread reopens with its saved reply and interrupted turn; Continue resumes its existing native session. The hidden
`projects recovery: a hard kill` scenario checks this separately from automatic recovery, with no new control.

## Faber's decision rows

At a project close, the existing closing reminder may ask either agent to tidy notes over their target. A normal
conclusion can receive that one bounded follow-up too. The exact input stays in the existing wake disclosure; a kept
decision missing after folding is a calm notice in the next agent input, with no error styling or new control.

A quiet labeled row records each decision or message Faber made on its own. It is never the person's voice or the AI's reply. The row names its actor, decision and reason, with a Change action where available; the exact words sent to an AI stay selectable inside a closed disclosure. Wake rows explain why a question or child result was passed on and omit a separate reason when their sentence already explains it; helper rows disclose the brief sent to the helper. Rows wrap in narrow windows.

Quota handoff reasons name the exhausted AI and its saved model, then the AI taking over, retaining whether previous messages or a summary were carried. A move within the same AI, or an old message with no source AI, only says where the conversation continued and what history it carried. Recovery decision rows say where work continued without claiming that a whole AI ran out. The existing header tooltip and quiet reason wrap at 720 px; no animation or new control is added.

## Answers

If Electron's engine process exits unexpectedly, it restarts and the live screen refreshes engine state and the open conversation after the replacement answers. The composer stays mounted, retaining newer text whose storage write failed and its existing dirty protection; process-local running/background revisions start over (`electron/recovery.test.mjs`, `electron/migration.test.mjs`). The main process allows at most three restarts per minute; 30 seconds of host uptime resets that count, even when short-lived replacements answer state. An unrequested exit with code 0 or repeated failures closes the app. No message is resent automatically; interrupted replies keep their existing recovery actions. Every normal Quit waits for confirmed engine exit, requesting termination after 60 seconds. Failed termination leaves the window, panes and tray available for another Quit attempt, without a new dialog or motion; engine input remains closed to new calls (`electron/quit-shutdown.test.mjs`, `electron/quit-admission.test.mjs`).

Go back uses the same file history for non-temporary chats with or without a chosen folder; see
[the saved-history states](rewind.md). It adds no control or animation to the answer.

Saved file comparisons follow [the answer review rules](answer-review.md).

- Retry and Branch from here retain the exact native boundary recorded on the last completed answer of the chosen
  AI. If the CLI cannot fork at that boundary, continuing from saved messages uses the existing calm notice.
  Another AI, an old conversation without a boundary, changed session configuration or moving into a Project uses that same fallback.
  This changes no menu, model picker or file-restoration choice. A native fork never resumes the source session itself.
  A launch rejected before a process starts leaves Go back available, including after an AI tool update.

The newest reply has a quiet, wrapping line of what the conversation delivered and its work time and API-price cost; a second, quieter line names a run of at least three turns with nothing delivered. The project home lists the same facts by conversation, most spent first, with the total above them. Neither needs motion or a card.

- Native visual pieces, HTML/SVG cards and their web-access rules are in [visual-answers.md](visual-answers.md).
- The header is the vendor's mark, the model's name and its effort in the quiet header ink ("· high effort"; the owner,
  2026-10-17), nothing else (Auto shows its reason beside the name only while choosing). Auto's reason is its
  tooltip. With one usable model/effort profile after availability, quota and conversation filters, Auto answers directly and its reason says only one profile is available; no router request is made. Otherwise it uses a completed draft route or starts immediately with the previous/default choice, explaining that it did not wait for a route. These explanations do not replace the stored route's difficulty: a conversation that was still easy remains eligible for a later draft to move it up. The pace line stays (C002); it also says, in the same calm words, when Auto started one rung up because the person often retries that kind of task, only when that changed the pick. A wait for quota is told once,
  by the quota card, never also in the header or the red note.
- The line under an answer (its buttons, hover bar, the reply's state and why it stopped) is in [answer-line.md](answer-line.md).
- A reply shows as it happened (`ui/timeline.ts`): stretches of the agent's words (`div.md`), steps and thinking
  (`Steps`, see [work-log.md](work-log.md)), and helper rows (`AgentRow`, see [subagents.md](subagents.md)) appear in the
  order they arrived. The agent's words never fold.
- The caret blinks on the last stretch of words only while words arrive (before the first word, and for 1.5 s after each
  text change, never while a step runs). When nothing in the reply already shows the agent at work (no running step, live
  thought, running helper or arriving words, and no question or quota card waiting), one quiet live line
  (`div.activity-latest.live-line`) under the reply says what the agent is doing now (its `status` words, else
  "Thinking…" followed by the latest thinking headline when there is one). Older messages without decision rows keep the loop mark after the next reply's model (R241; the
  product map's Work log and Faber's own message say the rest).
- A project agent's plan (R248) sits under the newest reply, above its files, while the agent works and until every step
  is done: it starts closed as one quiet button in the small size, a chevron, "Plan · 2 of 5" and the step under way cut to one line, so a long plan never fills the thread; a click opens the rest under a fold that grows and shrinks (`Collapse`). Open, the head drops the step and shows an optional contract giving what is extended or new on one quiet line and its check and optional target on another, in the person’s language; a plan without one stays as before. A raised size adds one calm, wrapping status line saying the size and why; an insufficient contract uses the existing question card, defaulting to stating the contract before retrying. Credential refusals remain silent and late requests without a turn are denied without a card. Ordinary one-file reads keep the small plan. Directory searches and directory deletes always require a risky contract for now because their reach cannot be classified; directory contents are not scanned. Single-file actions keep the credential/data, outside-project, network-path, symbolic-link and repository-metadata protections. One line per step follows, a small square mark before it: a check
  and the third ink when done, an accent ring and the full ink on the current one (`aria-current`), plain when still to
  do; each state is said to a screen reader. A done step shows the evidence the agent gave beside its text. Steps the agent hands to conversations of their own (waves, R487) show the same: each one that runs is the current one, so several rings can stand at once, and a step whose conversation stopped stays so until the agent marks it done; each step shows its conversation's title as a quiet link and its state in helper-row words (amber when waiting for you; stopped when its record is missing or unreadable). A step cannot be marked done while its conversation is working or waiting; the agent still marks it done after that work ends. What the agent says it is doing now (its `status`, 3 to 6 words) takes the
  place of the newest step's words under the reply while it works, and an active step shows its current action or thinking line under the steps when it adds to the line above. When the agent gave an estimate and the job went half over it (rule 7), one quiet line in the third ink and the small size closes the block, under the steps: no number, no icon, no colour, it wraps at 720 px. A reply the agent closed with `wrap_up` ends, above
  the actions row, with one box on a hairline: the recap, then at most one question in the semibold weight and its one
  or two replies as buttons, the recommendation the primary one. Only the newest reply, once it is done, offers the
  buttons; a click sends the button's words as the person's message. A question Faber answered while Keep going ran
  keeps no buttons: the reply's answers card says what it chose ([keep-going.md](keep-going.md)). Nothing in the plan or the box
  moves but the plan's fold; both wrap at 720 px.
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
- Loading earlier messages keeps already drawn top-level message rows and their text nodes by message id. Streaming
  still updates that same row in place; paging does not reparse unchanged ordinary replies or move their text selection
  (`tests/ui/thread.check.ts`, `thread: 300 messages open`; `tests/ui/thread-stall.check.ts` measures frame gaps).
- The reading column does not animate message arrivals or reordering: scrolling never runs the list-motion helper's
  per-row style and geometry reads. Disclosures and controls retain their own motion.
- Hidden copy/edit/reply-action glyphs skip painting using `visibility` on the existing SVG or swap span; their layout and the buttons' focus targets remain.
  Hover and focus within the message reveal them with no visibility transition or delay; the visibility exit waits for the existing opacity fade to finish, including on WebKit without discrete display transitions.
  Touch keeps them visible; reduced and no motion disable both transitions through the shared motion rules.
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
  Web searches remain ordinary step rows. There is no separate report surface (`tests/ui/research.check.ts`).
- Keep going (R199): the chip, its quiet lines, its quota warning and what it shows on coming back are in
  [keep-going.md](keep-going.md).
- A project reply that opened, queued or pushed a pull request has one pace-style line in Technical details (R200): its words ("Following
  pull request #93: checks running.") link to the page, and "Stop following" sits beside them while it is open. It turns
  amber when it fails, conflicts or needs the person. When checks fail, Faber's own message is the loop
  mark on the next reply, its tooltip naming the pull request and its failed checks.
- The agent runs project checks explicitly with the verify tool, shown in the work log. Ending a reply starts no
  automatic checks, file-change scan or correction turn (R534). Older histories keep their check rows and fix-wake
  tooltips. Permissions and reply-style changes preserve scheduled compaction and its exact continuation. A retired
  server closes once its last
  session finishes. The shared running-state refresh handles every end of work, including Stop, explicit checks ending,
  Keep going ending by itself or being turned off, archive and quota stops; removing a conversation closes its session
  too. A settings relaunch lets background commands finish on their original driver while the next reply uses the new
  settings. Commands announced after replacement still show their output and keep their own Stop. Waiting for an old
  shared-server thread to close blocks rewind without keeping the current idle session alive. Stop leaves waiting messages in the queue without
  keeping a retired session alive; the person can send them later on a new session. Nothing moves, and no setting or first-run step comes with it.
- While an answer streams the engine reports it at most every 33 ms, and each update carries only what changed: the
  words and thinking added since the last one (`textFrom`, `thinkingFrom` in `host/src/feed.rs`) and the steps that
  changed (`toolsFrom`), rebuilt and combined in `ui/opening.ts`. The screen applies every waiting event in one batch
  once per animation frame (`ui/frame.ts`, with a 100 ms fallback while hidden), keeps unchanged timeline parts as the
  same object (`keepParts` in `ui/timeline.ts`), and draws the text block by block (`ui/markdown.tsx`): an unchanged
  block keeps its DOM and only the open one is parsed and sanitised again. A tick that changed only tools builds no text.
  Everything that reaches the page passes DOMPurify.

- [Queued messages and editing](queue.md).
- Agent-to-agent recados use that same queue with a quiet origin line while pending, never the person's message
  bubble. Ordinary work needs no manual send; held work keeps the queue's recovery controls. Once delivered,
  the reply keeps the origin in its existing header mark.
- [Branches and versions](branch.md).

## Go back before a message

Copy/Edit's menu adds Go back, Redo or pending recovery, including after restart. Its confirmation and saved-message
notice are in [rewind.md](rewind.md). No toolbar action or setting is added.

Faber's decision and automatic-message rows use the actor's mark beside the existing words.

Recovery keeps queued messages and attachments behind its continuation in every saved snapshot. A failed conclusion keeps its follow-up task; completed task files are removed only after the conclusion saves, including parent-directory synchronization on Unix. Repeating the current task in a successful conclusion leaves one unsent offer with its replacement file. Review rows combine votes only for identical evidence; different problems remain separate notes. These corrections add no controls, layout or motion.
