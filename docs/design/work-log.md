# Work log

The steps and thinking inside an answer, part of the open conversation ([conversation.md](conversation.md)).

Project conversations have one plain line under the thread opening Technical details in the Running column. Full check
results and followed-request records live there; failures, fixing, needed decisions and unfinished work stay in
the plain thread line from the latest reply’s engine state; a personal Stop is neutral. The reply's steps retain their trace. Replies and closing recaps use
everyday words without dropping a problem; technical words are allowed when asked for or used first by the person,
and a useful file location is allowed when the person needs to find or open an artifact.

- The steps' line counts extend into the project's "Changes" view in the running column: git supplies the conversation's
  whole change, including files changed by commands, so its totals can differ from the tool-input counts below.

- Words, thinking and steps appear in the order they happened (`ui/timeline.ts`): the engine records where each top-level
  tool call started in the reply's text (`ToolUse.at`) and where each piece of thinking fell (`Message.thinkingMarks`). The
  agent's words never fold. Old replies saved before `at` keep `activityEnd`, so their steps sit above the words.
- Each step is a thin row (`div.step` > `button.step-line`): a small icon for its family (`stepIcon`: sheet, edit,
  terminal, search, globe, plug; or the live dot while running), a verb in bold (present tense while running like
  "Reading", "Running"; past tense once ended like "Read", "Ran"), what it acted on in the mono face cut in the middle
  to one line (`middle`), `+N −N` line counts on file changes, "failed" in the error ink when it failed, and elapsed time
  (`· 2 min`) past half a minute while running. Unknown tools keep their name; an MCP tool reads "Using a tool" (or
  "Used a tool"), its server.
- A click on a step row (`aria-expanded`) opens `div.step-detail` below it: for a file change, the changed lines
  (`ChangedLines`, up to 40 lines, then "N more lines"); otherwise the full command or path (`code.step-what`) and what
  it returned (`pre.step-out`, up to 30 lines, then "N more lines").
- Thinking (`Thought`, `div.step.thought`) is a quiet row in its place among the steps: a feather icon (or the live dot
  while thinking), "Thought" (or "Thinking" while live) in bold, and its first line (or latest line while live) in one
  middle-cut line. A click opens `div.step-detail.thought-text` with all of its paragraphs (`thinkingParts`).
- One step or thought between two stretches of words stands directly as its own row (`div.steps.one`). Two or more steps
  or thoughts between two stretches of words fold into one counted line (`div.steps` > `button.steps-head`): a right
  chevron (or the live dot while working), a summary by kind in the order they first came (`stepsSummary`:
  "Read 3 files · Ran 2 commands", "Thought", a file counted once however many steps touch it), the step or thinking it
  is on now while working, how many failed (`1 failed` in the error ink), and elapsed time past half a minute. A click
  opens `div.steps-list` with every row in order. Nothing animates beyond the chevron's turn (none in reduced motion).
- At a catalog kit's session boundary, the reply shows one quiet `.kit.disclosure` fold above the reply ("Tools ready when
  the AI started"): the catalog localized names of task-picked items and selection reason. Names share Catalog's
  `catalog.name.*` entries in all five locales. Claude's available descriptions are not this selection.
  A reused prepared session records the sent task's choice, even when preparation started before the first message.
  It appears only when the session starts with different ids from the last recorded kit in that conversation; an idle
  close and resume with the same kit does not repeat it. The kit a conversation has now is always one click away in the
  topbar's context panel ("Catalog tools in this conversation", [layout](layout.md)), in Chat and Projects alike.
  Claude also omits the resource prefix from an unchanged native resume, including a warmed one; changed ids or snapshot identity send the
  current resources while keeping native history. Projects names native plugin skills without outside Read paths when
  the installed CLI supports the plugin flag; older CLIs and helpers receive the checked Read index instead.
  An empty kit shows nothing unless preparation failed, when the same quiet fold explains that catalog tools could not be
  prepared in the screen's language. A clearly missing item may be suggested there once across the app, using its
  localized display name and pointing to Catalog for the person's installation decision. No setting, automatic install,
  timer or motion comes with it. Chat and Projects use the same fold, including at 720 px and with reduced motion.
- A conversation the agent starts by itself (R198) is one step row, "Working in parallel" (or "Started conversation" once
  done) with its title (or the id when the agent reads it back); the conversation is a normal sidebar row. A linked child also shows a button with its title, AI and model, state, waiting note and latest short line; opening it goes to the conversation. An archived child stays visible as archived. Existing conversation events update it, with no row timers.
- The agent running Faber's checks (the `verify` tool) is one step row with no detail: "Running the quick checks" /
  "Ran the quick checks", or "Running every check" / "Ran every check" for the full level. What failed is the agent's to
  read.
- Subagent and helper rows stand as their own rows in place in the reply and have their own page, [subagents.md](subagents.md).
- While a reply runs and has ended at least one check, a polite line under the reply counts them (plain words in Projects)
  (R161 step 9): "Ran 3 checks · none failed" after a check mark in the accent's text ink, or "· 1 failed" with the error
  note's 2 px edge and no mark. The words are the subagent facts' own. It goes when the reply ends; nothing moves.
- A file change's row ends with its line count in the data face, `+N` in the accent's text ink and `−N` in the error
  ink (R161 step 9). While the newest step of a reply is a file change, the lines it changed sit under its own row (or
  under the folded line it sits in), lined up with its label, until the next step or words: at most four, one line each in the data face and the third ink, the sign in
  the count's ink, then "N more lines". Nothing moves, and the step row already names the change, so the lines are hidden
  from a screen reader.
- While a reply runs and has read a web page, a quiet line under the reply names the sites it opened (R161 step 9):
  "Opened 5 sites", then "+N" for any past four, then the four last opened, joined by middle dots in the third ink, the
  one it reads now last and in the first ink. A site opened again moves to the end. It goes when the reply ends; nothing
  moves, and the step row already names the page, so the list is hidden from a screen reader.
- A step that has run for half a minute says for how long at the end of its row or folded summary (R161 step 9): " · 2 min"
  in the same ink, in tabular figures, from when the engine saw it start. A long command cuts its own words, never its time.
  The clock ticks every 15 s only while such a step runs and the window shows; the time is hidden from a screen reader,
  since a live region that changed that often would keep talking.

Catalog resources are refreshed only at a session boundary, including a native resume. Both providers compare a saved
digest of the exact catalog prefix, covering instructions, the discovery index and resource paths. Unchanged saved kits
omit the repeated prefix; changed or unknown resources travel in the first user input even if a loaded thread ignores
resume overrides. An empty replacement explicitly supersedes earlier skill instructions once; fresh empty sessions add
no prefix. Older native history stays intact. Chat
never needs to read catalog files outside its working folder; its supporting files remain unavailable. A preparation
error uses the existing quiet kit fold, and a later retry does not insert resources mid-turn.

Agent catalog changes share the existing self-change permission card; [Catalog](catalog.md#agent-requests) describes
their separate grants, version disclosure, backups and preservation of current conversation tools.
