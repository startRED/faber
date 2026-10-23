# Work log

Project conversations have one plain line under the thread opening Technical details in the Running column. Full check
results and followed-request records live there; failures, fixing, needed decisions and unfinished work stay in
the plain thread line from the latest reply’s engine state; a personal Stop is neutral. The folded log retains its trace. Replies and closing recaps use
everyday words without dropping a problem; technical words are allowed when asked for or used first by the person,
and a useful file location is allowed when the person needs to find or open an artifact.

- The chips' line counts extend into the project's "Changes" view in the running column: git supplies the conversation's
  whole change, including files changed by commands, so its totals can differ from the tool-input counts below.

- The work log and a helper's row share one disclosure style (`.disclosure`: a quiet card, a chevron, the hover).
- At a catalog kit's session boundary, the reply keeps one quiet line inside that same work-log fold: the catalog
  localized names of task-picked items and selection reason. Names share Catalog's `catalog.name.*` entries in all five
  locales. Claude's available descriptions are not this selection.
  A reused prepared session records the sent task's choice, even when preparation started before the first message.
  It appears only when the session starts with different ids from the last recorded kit in that conversation; an idle
  close and resume with the same kit does not repeat it. The kit a conversation has now is always one click away in the
  topbar's context panel ("Catalog tools in this conversation", [layout](layout.md)), in Chat and Projects alike.
  Claude also omits the resource prefix from an unchanged native resume, including a warmed one; changed ids or snapshot identity send the
  current resources while keeping native history. Projects names native plugin skills without outside Read paths when
  the installed CLI supports the plugin flag; older CLIs and helpers receive the checked Read index instead.
  A nonempty kit with no other work uses "Session tools" as the fold's heading; empty kits show nothing unless preparation
  failed, when the same quiet line explains that catalog tools could not be prepared in the screen's language.
  A clearly missing item may be suggested there
  once across the app, using its localized display name and pointing to Catalog for the person's installation decision. No setting, automatic install, timer or motion
  comes with it. Chat and Projects use the same line, including at 720 px and with reduced motion.
- Each step is a quiet chip: a verb in the ink color ("Reading", "Running") and what it acts on in the mono face, cut
  in the middle. A tool Faber does not know keeps its own name; an MCP tool reads "Using a tool" with its server's name.
- Steps of one kind in a row (changing, reading, running, searching) read as one phrase with a count: "Changing 3 files",
  "Reading 4 files", "Running 2 commands" (R411). It is the step chip's size and look with a small chevron, and opened it
  takes the whole row and shows the single chips under it, indented. A file counts once however many steps touch it, so three
  edits to one file stay three chips; a tool server's tool, a skill, a plan or a question is never grouped. When a step in
  the run failed, "1 failed" sits beside the phrase in the error ink, since the phrase itself is not struck through. A run the
  person opened stays open as it grows. The status line follows the run: "Reading 3 files · a.ts" (the count so far, then
  the newest step) instead of each file in turn. Nothing moves and nothing animates beyond the chevron's turn (none in
  reduced motion). Every step chip, single or grouped, has a real 1 px border, so forced colours still draws its edge.
- A conversation the agent starts by itself (R198) is one chip like any other, "Working in parallel" with its title (or
  the id when the agent reads it back); the conversation itself is a normal row in the sidebar, never a step inside.
- The agent running Faber's checks (the `verify` tool) is one chip with no detail: "Running the quick checks", or
  "Running every check" for the full level. What failed is the agent's to read; the chip only says it ran.
- Subagent and helper rows have their own page, [subagents.md](subagents.md).
- While a reply runs and has ended at least one check, a polite line counts them (plain words in Projects) (R161 step 9):
  "Ran 3 checks · none failed" after a check mark in the accent's text ink, or "· 1 failed" with the error note's 2 px
  edge and no mark. The words are the subagent facts' own. It goes when the reply ends; nothing moves.
- A file change's chip ends with its line count in the data face, `+N` in the accent's text ink and `−N` in the error
  ink (R161 step 9). While the newest step of a running reply is a file change, the lines it changed sit under the status
  line until the next step or the answer: at most four, one line each in the data face and the third ink, the sign in
  the count's ink, then "N more lines". Nothing moves; the status line already says the step, so the lines are hidden
  from a screen reader.
- While a reply runs and has read a web page, a quiet line under the status line names the sites it opened (R161 step 9):
  "Opened 5 sites", then "+N" for any past four, then the four last opened, joined by middle dots in the third ink, the
  one it reads now last and in the first ink. A site opened again moves to the end. It goes when the reply ends; nothing moves,
  and the status line already says the page, so the list is hidden from a screen reader.
- A step that has run for half a minute says for how long at the end of the status line (R161 step 9): " · 2 min" in the
  same ink, in tabular figures, from when the engine saw it start. A long command cuts its own words, never its time.
  The clock ticks every 15 s only while such a step runs and the window shows; the time is hidden from a screen reader,
  since a live region that changed that often would keep talking.

Catalog resources are refreshed only at a session boundary, including a native resume. Both providers compare a saved
digest of the exact catalog prefix, covering instructions, the discovery index and resource paths. Unchanged saved kits
omit the repeated prefix; changed or unknown resources travel in the first user input even if a loaded thread ignores
resume overrides. An empty replacement explicitly supersedes earlier skill instructions once; fresh empty sessions add
no prefix. Older native history stays intact. Chat
never needs to read catalog files outside its working folder; its supporting files remain unavailable. A preparation
error uses the existing quiet work-log line, and a later retry does not insert resources mid-turn.

Agent catalog changes share the existing self-change permission card; [Catalog](catalog.md#agent-requests) describes
their separate grants, version disclosure, backups and preservation of current conversation tools.

Native foreground-window text reads use the existing work-log row and status treatment. The localized action label has no supplied title, selector or target detail; native window content remains tool-result data. Unsupported visible-range text is omitted and marks the result partial; no extra row or notice is added. No new screen, control, focus movement or animation is added.
