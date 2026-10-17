# Following the work

How a person follows and watches the agent work, visual answers, and learning and memory.

## Following the work

**Progress view.** The self-development MVP uses Ready, Running, Review, Done and Blocked. Ready tasks can wait only
for an explicit resource brake; Blocked tasks name their missing dependency or decision. The remaining Projects
detail view follows the later full implementation.

- **Rows** show the AI's name and a verb, such as "Claude drawing".
- **Top:** one plain sentence, the overall bar, the time left, the share of the plan's limit used, and who is working
  now. **Bottom:** a feed of short sentences.
- **Detail screen.** Clicking a part opens it. It shows what is happening now with a preview that fills in, the steps
  (who makes the part and who reviews it), the time and the limit spent, the choices made alone each with "Change",
  review findings and the proof, a message box to the part, pause, redo and open files, and technical details behind a
  click.
- **Words** are plain and explain themselves. There is no agent map.

**Running list.** A column on the right, closed by default.

- A button at the top is always visible and reads, for example, "2 running".
- When the user is needed, the button turns amber and the column opens by itself once per question. It never opens
  while the user is typing and it never moves the cursor. If the user closes it, only the button stays amber until they
  answer.
- Items are grouped by project, one block per project. Projects with something that needs the user come first, and
  inside a project the amber item comes first.

**Tray.** The tray icon shows a number. A notification appears only when an agent needs the user, never each time
something finishes.

- **Attention notifications (R92, owner).** One per question, approval or blocked request, and none while Faber is
  focused. Toasts and their original short chime default on, with independent switches in Settings: turning off
  "Notify me" leaves the sound choice alone.
- On Windows, `SHQueryUserNotificationState` selects sound only for a busy full-screen app, an exclusive Direct3D
  game, presentation mode or a Store app. Only the accepts-notifications state permits a toast, and a foreground
  window covering its whole monitor suppresses that toast too (including borderless games, excluding Faber's windows).
  Locked/screensaver and initial-login/upgrade quiet-time states stay silent; quiet time is not Focus Assist/DND.
  Failed or unknown state queries also suppress the toast. Delivery never opens, flashes or focuses a window.
- macOS and Linux send a toast and the chime; full-screen detection comes later. Sound uses Windows' `PlaySound`,
  macOS' `afplay`, or Linux's `paplay` with `aplay` as a fallback. These players come from the system, never the bundle.
- The [notification review](../research/2026-10-12-notifications-review.md) records the verified findings and the detection limits.

## Visual answers

Answers can contain visual pieces, such as checklists, charts and forms. Faber calls them "Visual answers", in both Chat
and Projects.

- They start with about 10 built-in pieces: big numbers, table, chart, slider or −/+ tied to a calculation, checklist,
  tabs, steps or quiz, options form, notice and timeline. Free-form mini apps come later, and only if they are missed.
- What the user checks or moves stays saved, even after the conversation closes.
- Text is the default; a piece appears only when it saves the reader effort.
- A form never sends on its own. It only fills the composer.
- A broken piece falls back to plain text.
- Side-by-side comparison of options is part of visual answers: images, animations or live sites, whatever fits.
- In a project, the agent's own work shows as pieces too: a site it opened, the app it is running, a test result, a
  changed file, each with a small picture and Open ([Watching the agent work](#watching-the-agent-work)).

## Watching the agent work

The owner's ask (2026-10-16): the person sees every part of the work happening and steps in as easily as reading a
chat. Steps 1 and 2 are built: each tool reads as a verb with what it acts on (one verb per tool; grouped phrases such as "Changing 3 files" and the time per subagent are not), and subagent rows are the steps of R161. The rest is planned. The design is an AI proposal, not reviewed, picked from a divergent pass (30 ideas under five
frames) and checked against today's code. It fills the steps of R161 and never becomes a board or an agent map.

**In order, each one extending what exists:**

1. **Words for every step (now, Chat too).** Today the work log names only Codex's four item kinds; Claude's tools show
   their raw names ("Read", "Task"), and the open log is a flat list (`ui/worklog.ts`, `ui/Thread.tsx`). Each tool
   gets a plain present-tense phrase built from its input with no model call ("Reading akitaonrails.com/en/posts",
   "Running the tests", "Changing 3 files"): the engine-side event stays as it is, and `worklog.ts` maps it to a key
   with arguments that each language writes by hand. The status line that already shows the newest step becomes this
   living sentence.
2. **Subagents as rows inside the step (R161 step 2).** Claude's stream marks every event of a subagent with
   `parent_tool_use_id`, which `engine/src/claude.rs` drops today, and the Task/Agent call already carries the whole
   brief in its input. The engine keeps that mark on `Tool`, `Text` and `ToolResult`; the screen groups a child's
   events under its Task step, which opens into one row per child: the short name the agent gave it, its living
   sentence and its time. Past three rows the rest fold into "+2 more", and while nothing is stuck the whole split
   reads as one sentence ("5 collectors reading 5 sites"). Codex's own subagents show the same way (R167).
3. **The side chat lives in the right column.** The running column (`ui/App.tsx`, `ui/Running.tsx`) is already the one
   place on the right; clicking a child row opens that child there as a normal conversation, with a back arrow to the
   running list, so there is no second column. Its first message is the whole brief the main agent sent, word for word,
   then the child's messages, steps and results as they arrive, drawn with the conversation's own message parts and
   loaded only while open. A CLI subagent cannot be messaged while it runs, so its box writes to the main agent about
   that step and says so. Built (R176): the brief, the steps and the report; the child's own words between steps are not
   kept by the engine yet, and "Tell it…" in the conversation's box is the way to write.
4. **Facts, not a verdict.** When a child ends, its row shows what it was asked, what it did (files changed, checks run,
   steps) and anything it touched that the brief never named. The main agent grading its own helper is not trusted:
   each brief ends with a "done when" list, and the row ticks each line the report meets. Built (R180): the facts
   (files changed, checks and failures, files the brief never named); the "done when" ticks are not, since matching a
   brief's lines to a report would be a guess.
5. **Helpers talk back (R161 step 3).** A helper is a real Faber session on the other vendor, so in the side chat the
   box writes to that child, and the main agent gets the message before its next step. A finished subagent offers
   "Continue with a helper". Built (R196): the box writes to the helper (Codex in its turn, Claude Code at its next
   step or right after), and the main agent reads the person's words under the helper's report, not before its own next
   step: it is waiting on the helper then. "Continue with a helper" starts a message to the main agent.
6. **Stuck shows only when it matters.** The engine notices a child repeating the same action, failing the same check
   twice, or silent longer than its kind of work allows; a child waiting on a question is "waiting for you", never
   stuck. The main agent hears first and tries once to retry, narrow the piece or hand it to the other vendor. Only if
   that fails does the row turn amber with "Look" and "Tell it…", and only then is the person notified. It ships once
   the recorded fixtures show no false amber. Built (R174): neither CLI can be told about its subagent while the main agent waits for it, so
   the main agent's chance is the retry the thresholds leave (a check shows only after it failed three times).
7. **Faber's own browser and app pane (R161 step 4, the largest piece).** Built (R195): Faber's own browser, a
   second webview of the window on the same WebView2 (Windows only). A project's agent in full access gets a `browse`
   tool (Faber's MCP server for Claude Code, a dynamic tool for Codex) that opens a page there, answers with its visible
   text marked as the site's words, and keeps a picture in the conversation's attachments; the reply shows a card with
   it, and "See the page" puts the page over the running column. Pages are read-only for now: taking over, the agent's
   clicks and typing, the app pane and "Your turn" are still to come. The rest of the plan: The agent or a child opens websites in a pane that belongs to Faber and runs the app it builds beside the
   chat, through a Faber tool the CLI calls. A card in the reply ("Site open in the browser", with a picture and Open)
   is the way in, next to the file and question cards that already exist. The pane has one owner at a time: "Take over"
   stops the agent at its next action, the person clicks and types, and "Let it continue" hands it back with a short
   note of what the person changed. Logins, captchas and payments are always the person's turn ("Your turn" on the
   card), never the agent's (guardrails).
8. **Recorded, not streamed.** Each pane keeps a log: a small picture at each page change or agent action, with the
   action that caused it, typed secrets masked and password fields blurred. Watching live is playing the end of that
   log, coming back later is scrubbing it, and "Take over from here" starts from the latest picture. Only the pane the
   person opened is a real live view; every other child is a still picture that steps forward. The log follows the
   attachments of a project conversation (`<data dir>/attachments/<conversation id>/`): Faber's own data, never the
   project folder (R35), never in an export, trimmed by size and deleted with its conversation. "While you were away"
   sums it up in a few pictures with one sentence each.
9. **The view fits the work.** The owner's ask (2026-10-16): the person watches each agent work in whatever way makes
   that work easiest to follow. The rule is the goal; the examples below are starting points, not a spec, and the
   references (Google Stitch, Claude's own browser pane) are for the feeling, never the look.
   - Browsing: the page live as the agent moves through it, its pointer and what it clicks or types, and the line of
     what it is looking for.
   - Building a screen: the screen filling in as each change lands (the project's dev server reloads on its own), the
     changed part outlined for a moment, and a named pointer per subagent that opens it in the side chat.
   - Changing files: the few lines that changed, added and removed. Running checks: a count that ticks up, green or red.
   - Researching: the sources it opened and the one it reads now. Long or hidden work: the living sentence and the time.

   Each view is built from the agent's own calls and the pane's log, with no model call and no extra process; only the
   view the person opens is live. Reduced motion keeps the outlines and tags without movement; during a game nothing
   moves.
   Built (R188): changing files, as a line count on the step and the changed lines under the status line while it is
   the newest step.
   Built (R192): running checks, as a count under the status line that ticks up with how many failed.
   Built (R193): researching, as the sites the reply opened under the status line, the one it reads now in the first
   ink. Codex reports its searches only, so its replies list no site yet.
   Built (R194): long work, as the time a step has run at the end of the status line, past half a minute.

**Across all of them:**

- **Quiet during games.** Built on Windows (R191): the tray's check of a game in front is one the screen asks
  (`game_in_front`). While it holds, pictures stop, rows stop repainting and events wait
  as one summary for the person's return. Nothing opens a window or takes focus.
- **Cost first.** Faber idles near 110 MB today against the 400 MB test (R125). Steps 1 to 6 add no process and only
  the rows on screen. A second webview is the big cost: one live view at a time. The Windows spike measured it
  (2026-10-17, [the spike note](../superpowers/specs/2026-10-17-agent-pane-webview-spike.md)): a child webview of the one
  window adds about 30 MB on a simple page and 70 to 80 MB on a real site, uses 8 to 14% of one core while acting and
  nothing while idle, and gives fresh pictures whether the window is never shown, minimized or in the tray, with no
  window seen and no focus taken. It leaves the screen by moving out of the window's area, never by being hidden,
  which stops its pictures.

Parked: a floating picture over the game (takes the screen), a strip board with columns (a board, R161), a shortcut
palette (a second way to do what the composer does), a layout that rearranges itself when the person looks away, and an
approval counter every result must pass before it enters the project.

## Learning and memory

Faber learns from use, in a way that stays visible and organized.

- What belongs to a project stays in that project. Global memory keeps only what holds 100% in every situation.
- A clear objective improvement becomes global by itself after it repeats in 3 or more projects, and it can be deleted.
  A personal preference becomes global only with the user's approval. When in doubt, Faber treats a lesson as a
  preference and asks.
- Everything sits in a visible list with one-click delete.
- Decisions, taste, plans and recipes live in plain files in the project's hidden `.faber/` folder, so other programs
  and AIs can read them. The conversation history of both areas stays private inside Faber.
