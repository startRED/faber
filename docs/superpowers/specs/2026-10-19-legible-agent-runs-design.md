# Legible agent runs: what the agent is doing, how far it got, and when it is done, 2026-10-19

Asked by Erick on 2026-10-19 after a 15-hour Keep going run in the faber project conversation: study how Claude Code
desktop and the ChatGPT/Codex desktop app lay out an agent's work, and make Faber clearer about what the agent is doing
and how far it got. In his words, today it is "muito texto bruto". Sources: the clean-room study pages in
[../../research/2026-10-18-harness-output-ui/](../../research/2026-10-18-harness-output-ui/README.md) (behavior only)
and Faber's code at `origin/phase-1-base` on 2026-10-19.

Status: proposal. The tasks in [../../tasks/](../../tasks/README.md) numbered 016 to 019 build it, in that order.

## What went wrong in that conversation

All of this was counted by hand from the conversation's screen text and three screenshots:

- **One reply's work group:** it held 345 steps and about 95 narration lines. About 30 of those lines say only "still
  waiting", "running in the background" or "I'll continue when the notices arrive". About 20 start with "Erick,".
  Several switch between Portuguese and English.
- **The same hook warning six times.** It was about spring easing in code nobody touched, and each time it was a turn
  of its own.
- **System rows with nothing between them:**
  - "O Faber pediu para continuar" three times in a row;
  - "Interrompida." three times;
  - "Não foi possível medir partes deste turno." six times;
  - "O que falhou" over an empty block.
- **No visible plan.** "Everything that was planned" had no list, and the PRs (#141 to #156) and their states lived only
  in prose. The background work (reviews, test suites, the landing queue) also showed only as narration.
- **The end was hard to find.** The good final summary sat under hundreds of lines, and Erick asked "terminou?" four
  times. `[keep going: stop]` showed raw four times, and no notice said the run was over (task 015).
- **A finished reply still looked busy** (screenshot, 11:24):
  - the text was complete, but the header read "Trabalhando · 5 etapas · 4 comandos";
  - the composer showed Parar and a white caret blinked under the last line;
  - the running column said "Claude trabalhando sozinho 4 min" with a promise ("Daqui em diante não escrevo mais…") as
    its status.

  Finished could not be told from stuck.
- **Six questions answered for him** showed as six long sentences.

## What the two apps do

From the study pages (claude-desktop-rendering 2.6-2.8 and 5.3-5.4; openai-desktop-rendering 2.10-2.12) and
[long-run notes](#long-runs-in-the-two-apps):

- **The plan is a tool, not prose.**
  - Claude's Code tab gives the model a task-list tool: short imperative steps, exactly one in progress, sent with the
    first tool call of the turn, and advanced along with the next call at no extra round trip. It is drawn under the
    latest message.
  - Codex draws a "Plan" card that can be collapsed or opened in a side panel.
  - Both tell the model never to repeat the plan in its text.
- **"What I am doing now" is a short label.** Codex has a running-summary tool: 3 to 6 words, at most 50 characters,
  in the person's language. It is set when substantial work starts and again only when the phase changes, never on a
  timer. It fills a pill.
- **The end is a box.** Claude's wrap-up holds:
  - a recap of 2 to 3 sentences that says what is not done or not verified;
  - at most one question, with at most two buttons, the recommended one first.

  The recap's first sentence becomes the session's status in the sidebar. A "blocked" flag marks the session as
  waiting on the person. Codex ends a turn with "Worked for N" and an end-of-turn summary panel.
- **Steps fold.** A run of three or more tool steps folds to its last two under "N steps". Narration inside the
  timeline is clamped to about 200 px with "Show more". Text still streaming is never clamped.
- **No narration of waiting.**
  - Claude's model is told: no progress narration, no closing "next steps" paragraph, and point at things with an
    action, not a sentence.
  - Codex's heartbeat turns are told to stay quiet unless something meaningful changed.
  - Background tasks, monitors and other sessions are shown by the app (task rows, linked-session rows), so the model
    never has to say "waiting on…".
- **System turns look like the system.** Turns the app started ("nudged", "run summary", notifications) carry their
  own quiet marker, never a user bubble.

## Design

Ten parts, ranked by value over effort. Faber already has most of the parts these hang on:
- the MCP server over stdio and Codex's dynamic tools (`engine/src/helper.rs`), with `helper`, `browse`,
  `conversation`, `verify` and `map`;
- the work log (`.activity` in `ui/Thread.tsx`);
- the running column (`ui/Running.tsx`);
- Keep going (`engine/src/engine/keep_going.rs`);
- the tray notice for `halted`.

### 1. Finished means finished (task 016)

A reply that has ended looks ended everywhere, at once:
- no caret;
- the header reads "Done" (`activity.done`) and the composer reads Send;
- the running column drops "working".

The 11:24 screenshot is a bug against R227. Find which signal kept it "working", then fix it with a recorded fixture.
Candidates to check, not conclusions:
- a Stop hook that keeps the CLI's turn open after `end_turn`;
- Keep going's check turn starting before the reply was marked done;
- the caret being drawn from `working` rather than from "text is still arriving".

The caret shows only while text arrives, and goes after 1 s without new text even if the turn is still open (tools
are then the status line's job).

**The done card** (Erick's request, as in both apps' session lists). A run that ends while its conversation is not on
screen keeps its row in the running column, turned into a calm card:
- a check mark, the title, "Done · 2 h 10 of work" in the data face;
- the recap's first sentence (part 4), or the last reply's first sentence until part 4 lands.

A card that ends on a question or a failure says so in the amber note style ("Needs you: …"). Clicking the card opens
the conversation and clears it. Its × clears it without opening. Opening the conversation any other way also clears
it.
- The sidebar row of a conversation with an unseen end has a small dot that the same open clears.
- The card, the dot and the tray notice (task 015) come from one stored "ended, not seen" mark on the conversation, so
  they agree and survive a restart.
- A done card no one opens leaves by itself after 7 days, and a failure card after 1 day, as Codex's overlay does.
- The tray notice is not sent when the window already shows that conversation. It is never sent for Faber's own
  continuation turns, only for the run's end (Claude's rule).
- Nothing moves: the row changes its words in place (C001, C008).

### 2. The plan the person can see (task 017)

A `plan` tool on Faber's MCP server, for Claude Code and Codex alike:
- up to 12 short imperative steps, each pending, in progress or done, with at most one in progress;
- `advance` finishes the current step and starts the next;
- the whole list is sent again when the plan changes.

The plan belongs to the conversation, not to a turn, so it carries through Keep going's turns and across restarts. It
is stored in the conversation's JSON.

Where it shows:
- a quiet checklist under the newest reply: done steps checked and dimmed, the current one in the ink colour with the
  status line beside it, pending ones plain;
- "3 of 7" before the title in the running column and in the sidebar's tooltip.

The tool's description tells the model to send the plan with the first tool call of a task with more than one step,
never alone, and never to repeat it as a list in the text. Claude's own TodoWrite stays a labelled step; Faber's
prompt asks for `plan` instead.

### 3. The now line (task 017)

A `status` tool takes 3 to 6 words, at most 50 characters, in the person's language, with no tool or file names. The
model calls it when substantial work starts and again only when the phase changes. It replaces:
- the work log's live status line text, which today is the newest tool chip; the tool chip stays inside the log;
- the running column's "last line the agent wrote", which today can be a promise or a waiting line.

With no status set, the column keeps today's behaviour.

### 4. The closing box (task 017)

A `wrap_up` tool, called once when a task ends. It takes:
- a recap of 2 to 3 sentences, at most about 300 characters, saying what is now true and what is not done or not
  verified;
- optionally one question of at most 100 characters, with one or two buttons. The first is the recommendation and
  each button is a reply of 1 to 3 words or a Faber action (open the diff, land, run the checks);
- a `done` flag.

Faber draws it as a quiet box at the end of the reply. The recap's first sentence becomes:
- the running column's status;
- the done card's line;
- the tray notice's text.

Under Keep going:
- `wrap_up` with `done` is the stop signal, replacing the `[keep going: stop]` text marker. The marker stays readable
  for a release so old runs still stop, and it is always stripped (task 015).
- A question in the box is answered by Faber after the usual wait (`QUESTION_WAIT`) with the first button, exactly as
  today's cards.

### 5. One run, one block (task 018)

Consecutive turns Faber started by itself render as one run, not as a list of turns:
- one header with the loop icon: "Worked on its own · 2 h 10 · 14 turns", plus "plan 5 of 7" when a plan exists;
- inside, each turn's work log stays folded;
- only the newest turn's final text and the closing box stay open.

A turn whose only content is Faber's "keep going" message and nothing else, an empty turn, is not drawn; it only adds
to the count. "While you were away" stays the run's first line, and its questions become part 9's card.

### 6. Narration diet (tasks 017 and 018)

**Prompt rules**, added in `engine/src/persona.rs` for both CLIs, in Faber's own words:
- Never write that you are waiting on background work, tests, reviews or notices: Faber shows them.
- Do not start messages with the person's name.
- Write every progress line in the person's language.
- Do not restate the plan; do not end with a "next steps" paragraph; the closing box carries it.
- Answer a hook's or check's output only when you will act on it; never repeat an acknowledgement you already gave in
  this run.

**On screen:**
- Short narration (one line, about 20 words or fewer, no links) folds into the work log. The text that ends the turn
  never folds.
- Inside the log, narration folds to its last two paragraphs under "N more". Fewer than three never fold.
- Paragraphs that only announce a wait are kept in the log but never become the run's status.

### 7. Background work as chips (task 019)

Under the plan, one row of quiet chips for what the agent left running:
- a background shell (Claude's `run_in_background`, Codex's equivalent), named by its description;
- a monitor;
- a helper or subagent (already a row: it moves here while running);
- Faber's checks (`verify`);
- each pull request Faber follows (today's `FollowLine`).

Each chip carries its state: running, passed, failed or waiting. A finished one stays as a checked chip until the
reply ends. Faber reads all of these from the tool calls it already parses, with no new protocol. This is what makes
the waiting narration of part 6 unnecessary.

### 8. Notice hygiene (task 018)

- **Notices of the same kind in a row merge into one row with a count.** Identical notices in one run show once with
  "×N" (data face). This covers "Interrompida." from an automatic
  turn and a repeated hook output; the hook output goes in the work log, never as a turn of its own.
- **"Não foi possível medir partes deste turno." leaves the thread.** It becomes a "≈" before the cost, with the
  sentence as its tooltip.
- **"Interrompida." shows only when the person stopped the reply.** A turn Faber cut (Keep going's own stop, a
  restart) says nothing.
- **A failed check line is shown only with its failures.** "O que falhou" with nothing under it is a bug: it shows "no
  details" instead.

### 9. Questions answered for you (task 018)

"Faber answered 3 questions for you" is one quiet card with the loop icon. Each row holds the question cut to one line
and the answer chosen. A row's "Change" sends the other choice as the person's message. It replaces the list of
`away.chose` sentences.

### 10. Links that show state (task 019)

A pull request number or commit hash that Faber follows, written in a reply, renders as a chip with its live state:
queued, landed, red or merged. This reuses `FollowLine`'s data. An unknown number stays plain text.

## Out of scope here

The pieces the research ranks later each get their own spec when they come up:
- the visual answers: inline SVG and HTML, typed cards;
- the review-comment directive;
- the pane-opening tool.

The question card for Codex is in "Rich answers in Faber" (proposed 2026-10-18).

## Cost

- **Parts 1, 5, 8 and 9** reshape existing DOM and add no process.
- **Parts 2 to 4** add three MCP tools to the existing server: a few kilobytes per conversation in its JSON, and one
  short list and box per reply in the DOM.
- **Part 7** adds at most a dozen chips per running reply.

None keeps a process alive. Each task measures memory on its queued commit (R125).

## Cards and decisions to check while building

C001 (calm lines), C003 (show what is known as soon as it is known), C008 (nothing frozen, nothing moving on text),
C011 (the work screen shows only the work). Also R168 (calm screens), R199 and R224 (Keep going and coming back), R227
(finished before the turn ends) and R161 (the MCP server and its tools). Part 4 changes how Keep going stops, so it
names R199 in its commit.

## Long runs in the two apps

Filled from the private corpus by a clean-room study pass. Behavior only.

- **Claude, task list:** one card under the latest message, not pinned. It shows up to 12 rows, with "N earlier
  steps" above and "N more to go" below, and it goes away after the turn unless agents still run.
- **Claude, status line:** a verb that changes every 8 s, then the elapsed time and the token count after 2 s. The
  thinking wording steps up at 15, 30, 45 and 60 s.
- **Claude, narration:** short narration (about 20 words or fewer, one line, no links) folds into the collapsed steps
  group. The text that ends the turn never folds.
- **Claude, automatic turns:** events of the same kind in a row merge into one row with a count. Quiet scheduled
  check-ins fold into one "N check-ins · no change reported · M tool calls" row.
- **Claude, sidebar:** a session's state follows this priority: waiting for a permission, then running, then failed,
  then needs input (from the closing box's blocked flag). The subtitle is the task summary or the status detail.
- **Claude, finished but unseen:** an "Unread response" dot, which the person can also set or clear by hand.
- **Claude, OS notifications:** three kinds (permission, question, task complete), each off, badge or banner, with
  banner the default. A notice waits up to 5 s for the closing summary so it can carry it. None is sent when the window
  is focused on that session, for automatic continuation turns, or for child sessions.
- **Codex, a finished turn:** it folds to "Worked for {time}".
- **Codex, heartbeat turns:** each ends with notify or do-not-notify. The quiet ones never mark the thread unread and
  never reach the floating overlay.
- **Codex, the floating overlay:** it holds the running summary and session cards. A card expires on its own: a
  failure after 1 h, a wait after 24 h, a finished run to review after 7 days.
- **Unknown:** how either app hides the streaming caret, and Codex's OS notification rules.
