# Following the work

How a person follows and watches the agent work, visual answers, and learning and memory.

## Following the work

**Progress view.** Planned for missions (Ready, Running, Review, Done and Blocked), which have no screen since R171.
Running work shows today in the running column and in each reply (subagent steps, helpers, conversations the agent
started); a board needs the owner first (R161).

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

- Ten built-in pieces appear within the reply: big numbers, table, chart, slider tied to a calculation, checklist,
  tabs, steps, options form, notice and timeline (R348). Large pages keep the shared pane beside the chat.
- HTML/SVG/Mermaid replies and snapshots of edited files share the pane and saved versions. Mermaid needs Live;
  offline it stays code. See [visual-answers](../design/visual-answers.md) for file rules, isolation and costs.
- Both CLIs learn the same short `faber-piece` JSON fence format through Faber's existing persona instructions. The
  engine accepts only known fields and bounded values in closed blocks; Faber's components draw them, never AI HTML.
- What the user checks or moves stays saved with the conversation in the engine, bound to that piece's source, even
  after the conversation closes. Branches retain choices for their copied messages, and rewind/Redo snapshots retain
  choices with the messages they restore. Export includes the validated content, current choices and calculation result.
  Browser storage is not their source of truth: its existing unsent journal only recovers failed or pending writes,
  preserving the choice and warning across navigation until saved or explicitly discarded.
- Text is the default; a piece appears only when it saves the reader effort.
- A form never sends on its own. It only fills the composer.
- A broken, unfinished or unknown piece stays a normal code block, with its source available to read and copy.
- Only root-level fences become pieces; blocks inside lists or quotes stay code so the surrounding text keeps its
  structure.
- Calculations use Faber's bounded arithmetic parser, without running code. Charts use SVG and the current look's
  colours, with a data table; native controls have labels and keyboard access. Pieces have no animation and fit a
  720 px window, long labels, light and dark looks and forced colours.
- Side-by-side comparison of options is part of visual answers: images, animations or live sites, whatever fits.
- In a project, the agent's own work shows as pieces too: a site it opened, the app it is running, a test result, a
  changed file, each with a small picture and Open ([Watching the agent work](#watching-the-agent-work)).

## Watching the agent work

The owner's ask (2026-10-16): the person sees every part of the work happening and steps in as easily as reading a
chat. Steps 1 and 2 are built: each tool reads as a verb with what it acts on, a run of one kind reads as one phrase with a count ("Changing 3 files"), and subagent rows are the steps of R161, each with its time. The rest is planned. The design is an AI proposal, not reviewed, picked from a divergent pass (30 ideas under five
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
   that step and says so. Built (R176): the brief, the steps and the report; R416 adds a CLI subagent's words between
   steps (16,000 characters at most, read on open); a helper of the other AI keeps only its report. "Tell it…" writes to it.
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
   it, and "See the page" puts the page over the running column. Built (R197): the agent reads the page again, clicks,
   types, scrolls and goes back by the words a person sees; a page with a password, card or captcha field is the person's
   turn ("Your turn" on the card), where the agent reads and scrolls but never clicks or types; while the person looks at
   the pane it is theirs, and the agent's call waits for them and then hears they used it. Built (R238): the app pane, as
   Faber's browser on the app's own address: a local address a command of the reply printed is a card "Your app,
   running here" with "See the page". Still to come:
   pointer-level actions for pages the words cannot reach, and the recorded log. The rest of the plan: The agent or a child opens websites in a pane that belongs to Faber and runs the app it builds beside the
   chat, through a Faber tool the CLI calls. A card in the reply ("Site open in the browser", with a picture and Open)
   is the way in, next to the file and question cards that already exist. The pane has one owner at a time: "Take over"
   stops the agent at its next action, the person clicks and types, and "Let it continue" hands it back with a short
   note of what the person changed. Logins, captchas and payments are always the person's turn ("Your turn" on the
   card), never the agent's (guardrails).
8. **Recorded, not streamed.** Built (R225): the page view in the running column lists every step the agent took in the
   browser during the reply, in plain words with what it typed left out, and picking one shows the picture taken after it
   in the live page's place; the latest step, or "Back to the live page", is the page itself again (taking over from
   there). The pictures of one conversation take at most 20 MB on disk, the oldest going first. "While you were away" shows
   the pages the run left open, a picture each with what the agent last did there. When the person hands the page back, the
   agent hears the sites they went to by the last two labels of their name only (never an address or a subdomain). Pictures
   of the person's own page changes are not kept: the owner chose so (2026-10-18), since they can show their logged-in
   pages. The plan as first written:
   Each pane keeps a log: a small picture at each page change or agent action, with the
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
   Built (R239): browsing, as a ring around what the agent clicked or typed in and a pointer at it, drawn over the page,
   following it as the page scrolls, until its next action, so the picture of each step and the page the person takes show where it acted; nothing moves.
   Pointer-level actions, and the outline of a changed part and a named pointer per subagent while a screen is built, stay
   for later (Erick, 2026-10-18); the app card already opens the app, which its dev server reloads by itself.

10. **A quick question on the side (built, R250; product map "Quick question on the side").** While the agent works, the person can ask
    "how is it going?" or "why that file?" without stopping it or waiting in its queue (the queue only empties when a
    turn ends). The running column gets a "Quick question" tab next to the running list, the same column as the
    side chat of step 3, so there is no new pane; typing `/btw <question>` in the conversation's box opens it with the
    question in.
    - **Context:** a copy of the conversation as it stands. Claude Code resumes the conversation's own CLI session with
      `--fork-session`, so the main session is never touched and its cache is shared; Codex, which has no fork, gets a
      new side request with Faber's own record of the conversation (its last messages and the steps under way). As built,
      Codex answers from that record with no shell: on Windows its shell cannot start under Faber's read-only profile.
    - **Read only:** it may read, search and list files in the working folder and nothing else: no edits, commands,
      browser, helpers or MCP servers. Claude Code runs with only Read, Grep and Glob allowed; Codex with its read-only
      sandbox. An answer that needs action ends with "Send to the agent", which puts the text in the main box.
    - **Nothing goes back:** the side answers never enter the main conversation or its CLI session. They live while
      the tab is open and stay in Faber's data for that conversation until the next turn ends, then go.
    - **Cheap and quiet:** one CLI process per question, gone when it answers, on the same model at low effort (the
      person can change it in the tab's own picker); the quota bar counts it like any turn. Nothing runs before a
      question, and a game in front changes nothing (no window, no focus).
    - **Edges:** a conversation with no turn yet has nothing to copy, so the tab says so; a quota stop shows the usual
      quota message; a 720 px window shows the tab over the running list with a back arrow, as the side chat does.
    - Check: `tests/ui/quick-question.check.ts` asks while a recorded turn runs and proves the main turn, its queue and
      its CLI session did not change; `engine/tests/` proves the fork and the read-only tool list.

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
