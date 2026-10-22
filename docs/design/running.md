# The running column

What runs now, on the right: rows, questions, done cards, Technical details' chips and when it opens by itself (the
engine's side is the Running column row of [product-map.md](../product-map.md)). The window widths it shares with the
rest of the shell are in [layout.md](layout.md).

- With nothing running it says "Nothing running" once and has no footer sentence.
- Visual answers use this same resizable pane, version arrows and one isolated frame. Reply pages, saved file pages
  and Mermaid share its dismissal, focus, 720 px and reduced-motion rules ([visual-answers.md](visual-answers.md)).
- "Commands" in the list's head replaces the list with the agent's commands, like an opened helper, with the same back
  arrow, title, close button, modal focus rules and Curioso empty state. Its count appears only while commands run.
  The other side views, the topbar's included, close when it opens. Each row uses the running list's head and quiet conversation name, the
  literal command in the data face, its state in words and a quiet Stop where the AI supports stopping it. The last
  20 output lines (at most 8 KiB) are plain wrapped text, never HTML; at most 32 rows per conversation stay in memory.
  A stop failure says so on its row and leaves retry available; an acknowledgement waits for the AI's end event.
  Finished rows leave when the conversation's turn ends, or immediately when they finish after it. The list updates
  by events, with no polling, new motion, console or terminal; closing the view releases its output elements.
- A project conversation with changed files has one "Changes" button among the topbar's pane buttons. It replaces the
  list, like a page or the quick question, with its title and close. Each file is a disclosure with the work log's `+N −N` counts;
  opening it closes the previous file and reads only its patch, so at most one patch stays mounted in total. Added and
  removed lines carry their signs and a quiet background, code and paths wrap, and nothing in the patch moves.
  Closing a file cancels its entrance, so switching quickly cannot reopen it. Loading, empty, binary and too-large
  files have plain words; a failed or timed-out patch says it could not load and offers "Try again" within that open
  file, never the empty state; a new file past
  the counting limit says its count is unavailable. A partial list keeps the files found, says files are still missing
  in one quiet line and offers "Show more files"; it never claims nothing changed. Closing a file releases its patch;
  ending a turn refreshes the list. Agent, browser page, visual answer, Commands, quick-question, Changes and file views
  exclude one another. Opening Changes in a column that opened by itself uses the other side views' transition: over the
  stage it becomes modal and traps focus.
- With changed files, the Changes head offers "Review" as its main action and a quieter "Review with security" beside it
  (R279). The actions wrap in a narrow column; they use the existing button states and reduced-motion rules. Each sends
  its localized request to the conversation's own agent through the message box's send path, so it queues while busy and
  appears as the person's message. It asks for a read-only review against the copy base (or HEAD), through Faber's helper
  with the other connected AI, or a fresh subagent of its own otherwise (R146, R161). The security action asks for bugs and
  security issues; findings come back in the conversation, worst first in a short list, or a clear no-findings answer.
  Sending closes the column the person opened, so the findings stream in the conversation (a column that shows by itself goes back to its running list). No review controls appear with no changed
  files or in Chat; there is no new setting, screen, timer or stored state.
- A file a reply names (a code span such as `src/app.ts:42`, or a link to a file) is drawn as a link and opens in place of
  the list with the same head: its name as the title, its path and "Line 42" in quiet text, "Show", then the lines in the
  data font, each number in a muted gutter that selection and copying skip. The named line (or range) has the accent's
  soft background, the one mark on the view, and is scrolled to the middle without motion. Nothing moves; refused,
  missing, empty, binary and too-large files have plain words, and a failed read offers "Try again".
- A run that ended while nobody looked keeps a calm card in its block (R242): no fill, a thin line around it, a check
  and "done" (amber when it needs the person), the time it worked and its last line. A click opens it, its × clears it;
  nothing else moves. The sidebar row shows a 6 px accent dot until the conversation is seen.
- One block per project, plus one "Chat" block. Each row shows the vendor's shape (a neutral dot while Auto is still
  choosing the model), the conversation title, the state in words ("Working", "Needs you" or "Choosing the model") and
  the time since the turn started.
- An item that needs the user shows the permission card, and the user can answer in place with Allow, Always or Deny.
  A question the agent asks (`AskUserQuestion`) shows instead as a question card, in the thread and in the column alike.
  When the agent asks to install a tool from the catalog, the permission card is Faber’s own: a title ("Install <name> into Faber?"), the tool’s one-line purpose and one line with its kind, version, source repository and licence, always above the buttons (a tool server, or a plugin that brings one, adds a second calm line that it runs code on this PC), then "Allow once", "Always allow installs in this conversation" and Deny. Every word is the screen’s (the Catalog’s names and purposes); the agent writes none of them. It has the permission card’s look and motion, nothing new.
  Several questions are steps: a strip of their header words (a number, a tick once answered), one question open at a
  time in the same cell, so the card keeps one height; the others wait beside it and glide in from the side. Each answer
  is a tile (radio or checkbox mark that fills with a tick, title, muted description of at most two lines, a small
  "Recommended" badge), two columns when the card is 640 px wide or more and one below; the chosen tile gets the amber
  ring. A preview the agent drew sits beside the tiles (below in a narrow card) in a quiet monospace box that fades when
  the tile in focus changes. Another answer is one quiet button that opens its field. The footer holds "n of m answered",
  Skip and Send answers (amber, enabled once every question has an answer; the card of the offer of a bigger version opens with its first tile, the request as asked, already chosen, so it is enabled at once). Once answered the card, in the thread and in the column alike, folds away over the
  exit time while the thread closes up under it. Tiles and steps change colour over the fast time and the mark scales in
  from half size; under reduced motion everything is instant and the state is still carried by the filled mark, the ring
  and the tick.
- Plan approval uses the same card in the thread and column: "Ready to run this plan?", the composer's native permission
  picker labelled "Run in" (its four modes except Plan), one amber "Approve and run" and a quiet "Keep planning". One short
  line says the choice sets the permission mode for all conversations. The plan appears once: in the reply, or as sanitized
  Markdown in the card when the tool input alone carried it. The thread announces politely and focuses its main action only
  when no editable field has focus; the column never takes focus or repeats the announcement.
  Its default never loosens the mode before Plan, including the first conversation: the saved prior choice, the session's
  current mode for agent-entered Plan, or Ask for unknown history. That saved choice is consumed once and cleared by later picks.
  A later permission pick invalidates approval of an older card, even after a restart or a change back. The card explains
  the refusal, disables approval and leaves Keep planning available.
  Approval removes the card, ends planning as Done with its cost, saves the one shared Settings mode and queues a normal work
  turn for both CLIs. Approval drains an idle queue in order, so older messages run before the approved work. Its visible line is Faber's own "Faber started the approved plan", in the person's language. Stop cancels
  that queued turn, or stops it normally once running. Older queued messages then wait for Send now, even when the late
  planning result arrives; list and done-card updates cannot clear Stop. A new message, Send now or Approve and run chosen during that
  closing interval starts once it ends, through normal Send now; untouched messages remain queued. A new actual turn clears Stop.
  The running count and Stop stay available while planning closes;
  a Stop before Codex supplies its thread or turn id still cancels the opening work, as for any message. Keep planning keeps the mode and releases the queue;
  Claude's card stays retryable until its denial is delivered. Approval's settings event uses the same filled defaults and
  shell adjustments as ordinary Settings responses. A Codex project approved into Full access keeps its checks, map and
  browser on the same thread; their definitions persist across modes, but calls in other modes return a plain refusal.
  New messages remove
  older Codex cards. No automatic answer approves a plan. The controls disable while submitting; a failed Settings save shows
  the reason on the ended reply and starts no work. It shares the card's tokens, wraps at 720 px and adds no motion or idle process.
- Clicking a row opens that conversation, setting the area, the project and the conversation together.
- The column is closed when the window opens. It opens by itself once for each question it has not yet shown, 1.5 s
  after the last keystroke in any text field, including questions already waiting at launch. If the user closes it
  while questions are waiting, those questions never reopen it. Only the button stays amber.
- Items are grouped by project. Projects that have something needing the user come first, and inside a project the
  amber item comes first. The order is stable, so rows do not jump.
- A reply whose agent has finished, with only the CLI's turn still closing, is not a row: it is done for the person. Keep going,
  Faber's checks, a compaction and approved work waiting to advance keep the row. Messages held after a failed or stopped
  turn, or unable to start, do not count as working and keep Send now available. The topbar's count follows the column.
- With no items it shows an empty state with Curioso.
- "Quick question" (R250) is the first of the topbar's pane buttons, and Ctrl+; does the same. It opens the quick
  question in place of the list, with its title and the close button in its head; `/btw`
  in the conversation's box opens it the same way, from any width (over the stage below 1,100 px). Each question sits in
  the same bubble as a subagent's brief, its answer below it as a reply, and a quiet "Send to the agent" under a finished
  one; while it runs, "Looking…" and a quiet Stop. The box is the side chat's (Enter asks, Shift+Enter breaks the line),
  with the model picker beside Ask when the conversation's AI has more than one model. It is offered in any open
  conversation, working or idle (R267); before the first answer the panel says to ask then and has no box.
  "Recap" beside the box asks at once, disabled while another side question runs. It asks in the person's language
  for a few short lines on what was requested, decided and done, and what remains open. Faber's `/recap` next to
  `/btw` in the composer's list opens this same panel and asks without inserting anything; sending `/recap` alone
  does the same (R266). The recap is an ordinary read-only side question with the same model and quota rules;
  the conversation and its session stay untouched (R265). Nothing in it moves but the list's own entrance (`animateList`).

## Known limits

Visual answers in Off/Cached block external scripts; Live allows public HTTPS scripts. Eval and event attributes
generated later by scripts stay blocked; generated controls use `addEventListener`. A fixed visual loader authorized
by its own response-header hash runs the initial page's scripts and converted event
listeners. Non-script resource load/error handlers capture on Document (element load does not reach Window), while page
lifecycle handlers stay on Window. They capture before the parser can finish resources, and deliver
them after preceding classic scripts have supplied their functions, preserving target and currentTarget;
other converted listeners install before the first authored script. A malformed resource handler cannot abort
unrelated page startup. External classic and module load/error callbacks
attach to the executed script before loading starts,
with that script as their `this`. Authored `crossorigin` is preserved: ordinary classics work without CORS headers,
explicit CORS remains enforced, and modules still require CORS. Integrity hashes and safe authored script metadata
(including id and data attributes) survive replacement before fetching starts; nonces and internal markers do not.
Resource completion order still belongs to the browser: event timestamps describe creation, not capture. An image may finish after a later script, but captured events
are delivered as soon as their preceding classic definitions exist. No host nonce enters the page. The loader waits for the browser's own page load (inline modules
do not emit script load events), then delivers one bubbling `DOMContentLoaded` followed by one window `load`,
so their own startup handlers initialize normally.
Native document accessors keep named forms from replacing the serialized root; authored markup cannot replace the
native response-header policy. Static and dynamic connection-hint links, speculation rules and nested browsing contexts
are removed or made inert before connection; stylesheet-only links remain usable. Noscript is dropped, including
templates, so enabling scripting cannot expose a different second parse. Every template loses shadowrootmode and the
legacy shadowroot attribute recursively, keeping declarative open and closed trees from hiding parser-created realms.
A document-start native observer also removes
parser-created nested contexts, watches each discovered open root and roots returned by the locked attachShadow
(including closed ones), drains pending records and disconnects at DOMContentLoaded; it does no idle work.
Parser-created closed roots cannot be inspected by a Document observer; native document denial, response CSP and
the all-frame startup guard remain independent of the cleaner's first parse. The native guard disables WebRTC peer and data-channel globals (including webkit aliases) in every web mode,
WebSocket, WebSocketStream and WebTransport, with non-configurable globals and prototype constructors,
and child navigation stays within single-use visual URLs or about:blank. The opaque frame can send only validated Escape and
context-menu dismissal messages; their origin, exact shape and currently mounted source frame must match.
Sandbox HTML insertion omits custom sanitizers and declarative shadow-root options; `execCommand('insertHTML')` does
not retain the browser's undo history. Explicit `attachShadow` still works, with the same offline guard.

Changes uses git's path-based reads for tracked-file counts, like git status; the list shows only names and counts locally, and must use pinned reads if it ever shows content.
