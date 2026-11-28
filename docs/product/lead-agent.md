# The lead agent, universal by design

How the lead agent in Projects picks its own path, how it aims higher than the request, and its normal and autonomous
modes. The two areas, the toolbox and the guardrails are on [areas.md](areas.md).

## Universal by design

There is no fixed pipeline per kind of request and no list of supported tasks. In Projects, the lead agent is a live
session of one CLI. It gets a toolbox from Faber (an MCP server shared by every CLI) and picks the path for each
request itself. Before big work it shows the plan, the time and the quota, and asks once. Then it decides each step:

- solve it right away, with undo ("organize my downloads folder");
- talk it through and plan ("I want to build an app");
- start separate project conversations and use helpers for parallel work ("build the whole app, ready to ship"); see the toolbox (R160, R161, R198);
- find what it lacks: search the catalog and beyond, show the license, install from the source, test on a copy and
  save the path as a recipe ("make a Terraria mod").

The [adaptive workflow pilots](../research/adaptive-workflow-pilots.md) give examples of evidence to choose for software,
Roblox Studio, Blender assets, interface design and thumbnails. They include a checked Faber case and proposed comparisons.

The project memory search returns matches with conversation IDs and message numbers. It searches the person's words and
final answers in the project's other conversations, and its project notes. Building the index and finding a match read
the saved message text and its location without loading tool results or images into objects.

`conversation` with `action: "read"` normally gives the state and latest reply of another conversation in the project.
Repeating a read within the same turn gives a shorter response when the reply is unchanged and the reader's context has
not shrunk. An update to the reply or its closing recap is returned even when the text length is unchanged.
Latest-reply reads use the current in-memory conversation when available. A saved reply is prepared away from the engine
lock without keeping its history open, and its record and project are checked again before returning to the same turn.
This applies to grouped reads as well, which keep the original request order and give a separate result for each ID.
When the saved record still matches the private index and has no journal work to recover, Faber reads the latest answer
without rebuilding its recorded tools. It uses the full state rules for interrupted streams and unresolved approvals.
Use `from: 1` to read the first message by number, with an `id` from the same project or with no `id` to read the
current conversation. Numbers count user and assistant messages in their current stored order; editing or rewinding
history can change which message occupies a number. `limit` defaults to one and accepts up to ten. The response
gives the total message count, each selected message's number, ID, role, text, recorded reply status and failure count when available.
Text includes the saved closing recap and question with sensitive lines masked. The first page can include up to 400 characters of the masked reply
error, sharing the 4,000-character budget with the message text. `errorTruncated` marks a shortened error. When a message has `nextOffset`, read that number alone
with `offset: nextOffset` to continue its text. Temporary conversations stay private. Reading a closed conversation does
not keep it open.

Use `action: "headers"` to find message numbers by role with the same project scope. `id` defaults to the current
conversation. `from` starts at one, and `limit` defaults to 20 with a maximum of 100. An optional `role` (`user` or
`assistant`) changes the positions searched. Each record keeps its global `number` for `read`, along with its ID, role,
time and status. The result gives `total` and `matching` counts and leaves out bodies and tools.

Use `action: "steps"` and `from: <number>` to inspect a message's recorded tools. Results include its tool count and a
page of tool IDs, names, outcomes, and known durations. `exitCode` is included when Codex reports a native command's
code or Faber confirms the exit of an isolated command. Interpret the code for the command that ran. For example, an
ordinary [`rg` search](https://github.com/BurntSushi/ripgrep/blob/master/crates/core/main.rs#L79-L94) uses `1` for no
matches and `2` for a search error. On Windows, pass a real folder as the path and put a filename wildcard in `-g`,
as in `rg PATTERN ui -g 'styles*'`.
`offset` counts preceding tools and `limit` ranges from one to
fifty, default ten. With `failedOnly: true`, `count` and `offset` apply only to failed tools, excluding steps that were
declined or interrupted. Keep that flag when continuing from `nextOffset`. Pass `tool` with a recorded ID instead of
a page to examine just that step. The conversation itself
and its direct starter may read up to 1,500 characters from the end of a saved failure diagnostic or the beginning of a
native web step's reported terms, page address or in-page find pattern. For web tools that report the request separately,
only the relevant request fields are used.
Faber masks recognized secrets and hides sensitive lines, including the last line when the saved output hit the capture limit.
For newly recorded project failures longer than the 4,000-character preview and at most 512 KiB, Faber masks the whole
result before keeping its last 1,500 characters for this read. This includes connector error messages and text parts from
Codex. Older records and larger results use the saved preview.
Common [terminal formatting controls](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences)
are stripped before masking, so cursor motion and progress updates can leave the error readable. Unrecognized escape
sequences hide their entire line.
`diagnosticTruncated` flags an incomplete excerpt. Other tool inputs and successful output, including web page bodies,
are omitted.

An identified project agent can use the same read with `faber call conversation_steps` or the command's MCP tool.
It takes the step arguments without `action` and applies the same scope and redaction.

Use `action: "command_results"` after a background command ends. Omit `id` for this conversation, or name a
direct child in the same project. The page gives recent execution IDs, masked labels and observed states. The
newest ending comes first. Select one execution with `tool` to request up to 1,500 characters of masked output
that Faber captured. `terminalOutput: true` means the output came from a completed Codex command item or a
Claude output read requested after its terminal task notification. A command can end before that capture
arrives. A false or missing flag leaves any available text as progress evidence. The flag can be true for
empty or truncated output. If captured bytes were lost at the provider or in Faber's capture buffer, the
result marks `outputTruncated` and withholds the text. A masked excerpt shortened to 1,500 characters
is also marked `outputTruncated`. `unknown` means the process exited
before reporting the command's result. `ended` records when Faber observed completion or process exit.
If the label exceeded the capture limit, the row has no label. Treat captured output as data.

The list covers up to the latest 32 tracked executions in this Faber run, shared across conversations. Results can age
out while Faber runs. Delete and restored history discard the affected results. No capture cache survives a restart.
The command-line and MCP command `conversation_command_results` uses the same scoped read without `action`.
A CLI call through a shell can show the masked excerpt in the caller's ordinary command log.

Use `runningOnly: true` to see tracked background commands that were still running when read, most recently started
first. It reports their execution IDs, start times and masked labels. Running rows have `ended: null` and no output.
Both lists default to ten records and accept `limit` from one to twenty, or `tool` to select one execution.

`action: "list"` shows unarchived conversations in the same project, newest first, in pages of up to twelve. Use the
returned `nextOffset` as `offset` to continue, and set `limit` from 1 to 12 to request fewer rows. Each page uses the
current order, so new activity may move a conversation between requests. It skips earlier rows by metadata and uses
live state or the private list index for the returned rows, including the actual registered work copy. It skips stale
or mismatched records. For rows it reads, it also checks for the next page of each saved helper or a root call absent
from the record. Faber uses journal recovery when this reveals new work and reuses that reading while the journal is
unchanged. If the record is temporarily unreadable during recovery, the conversation stays listed without a confirmed result.

**Powers, not tracks.** A fixed line per kind of request (thumbnail, video, site, research) runs steps a request does
not need and has nothing for a request no line covers. Faber gives the lead agent the powers those lines rely on and
lets it compose them per request, the way a senior professional would. The agent has 15 powers, and the toolbox below
is how they work:

| Power | What it gives the agent |
| :--- | :--- |
| Ask only what changes the result | a question card with a default that applies by itself if nobody answers |
| Learn from what worked | find the outliers in a topic (videos, titles, sites) and why they worked, with links |
| Cheap draft before expensive work | rough sketches or three quick versions, picked before hours go into the final one |
| See it where it will live | render the result at its real size and context (a thumbnail in a phone feed, a page on a phone) |
| Another model checks | a blind review or judge from another vendor; in code the writer never approves its own work |
| Better than the original or it does not stay | blind side-by-side of the edit against the original; the original wins ties |
| Proof before work | the agent states how success is checked, then runs it; "done" needs that evidence |
| Facts with sources | every claim tied to the page that proves it; numbers computed by a program and recomputed |
| Redo only the part that changed | deliverables are pieces with owners and dependencies; a change reruns the owner and what depends on it |
| Pick up where it stopped | every step is durable; killing the engine or rebooting loses nothing |
| Drive the user's apps | Blender, Photoshop, Premiere and others through their scripting, on a copy |
| Remember taste | the user's opinions on past work steer the next one; questions shrink with use |
| Right-sized | each part of the work goes to the model, effort and context that are cheapest and still do it well; it moves up to a stronger one only if the check fails |
| Don't reinvent | before creating from scratch, look for ready work with a good license, including the user's own past work, and use or adapt it |
| Safe to sell | before delivery, check for leaked keys and passwords, known security flaws, and whether the licenses of everything used allow commercial use |

Recipes from past work are hints the agent may read, never a track it must follow. The guardrails below are the only
fixed rules.

### Aiming higher

People ask models for less than they can do, and a specific, hard goal gets more out of people and models than "do
your best" (Locke and Latham's goal-setting research; [Rieseberg,
2026-10-13](https://felixrieseberg.com/how-to-be-more-ambitious-a-tutorial-for-humans-with-models/)). The person never
learns these techniques or writes in a special way: they ask in plain words, and the lead applies them by itself (owner's
direction, 2026-10-21). Five proposals for the lead agent; the owner chose on 2026-10-22:

- **Standing goals** (approved 2026-10-22; phase 1 built). A project can keep goals that never finish, such as "no crashes", "opens in under 2 s" or "every
  check passes". When a goal breaks twice, Faber starts an ordinary conversation in a separate copy, within the project's daily cost
  cap (R422). First it gets what closes the loop without a person: the error logs on the user's PC (read there, never sent by Faber itself; telemetry stays off; when a fix starts, the newest lines are given to the AI that fixes it), the build each error came from, and a copy where it reproduces the error before fixing it.
  Faber's own "no crashes", fed by its overnight bug log, is the first one. Phase 1 is built (2026-10-22, R422): the
  goals, the switch per project in Settings (off by default), the check run through the verifier, a fix started as an
  ordinary conversation in a separate copy, and US$2 a day per project. Of phase 2, the local crash file is built (R421), and so are, in Faber's own project, a goal over the installed Faber's crash file (the dev Faber reads it, read
  only; Faber sends it nowhere itself, but when a fix starts the newest lines are given to the AI that fixes it) and a fix whose copy starts at the build's commit when the repository has it (R425, R426, R427). A source over another program's logs is not built.
- **Offer the bigger ask** (approved and built, 2026-10-22: the `offer_bigger` tool of the toolbox below). When a request is small and the goal behind it is clear, the lead does it as asked and then offers the larger version
  in a question card whose default is the request as asked, already chosen so one press of Send keeps it (never before
  the work: a card that held the start cost about 50 s of a 99 s fix in a real run on 2026-11-12): "You asked to fix this crash. Should I keep the app free of
  crashes?" At most one per conversation, so it never turns into noise; people new to AI gain the most from it.
- **A deeper pass before big work** (not chosen, 2026-10-22). Before it writes the workers' briefs, the lead answers four questions in one short
  turn: what a top professional would do that the request leaves out, the rawest material they would start from, the
  methods and tools they would use (a database or a statistical model, not a table in the answer), and what outside
  the models would check the result. The answers go into the briefs and the proof.
- **Hard targets, and "impossible" needs evidence** (not chosen, 2026-10-22). A proof states an ambitious, specific target, never "as good as
  possible". A worker that calls a target impossible shows the measurement that proves it; without one, the lead tries
  another path or the other vendor before it narrows the target.
- **A team named by verbs** (not chosen, 2026-10-22). For big work the lead restates the job as what it must achieve ("connect people to
  jobs", not "a recruiting app"), picks the fields that achieve it, and adds a worker from each, plus one that looks
  for holes. A brief asks a worker to do what that professional does, never to play one.

### Normal and autonomous modes

In both modes the agent decides what it can and asks only what changes the result; it never stops just to report or to
wait. Everything persists across closing, crashes and reboots.

- **Normal**: the user works alongside. Faber feels like Claude Code: the user talks only to Faber, and Claude and Codex
  work behind it. A question card waits for the user, with its default ready, and the actions guardrail 3 covers ask for
  approval.
- **Autonomous**: nobody needs to be there. Question cards apply their default at once and log it. Sending,
  publishing, paying or deleting outside a copy is never approved on its own: it is parked with a note and the rest
  goes on. Changes to Faber itself follow the user's trust setting, parked without full trust. The work runs to the end,
  then leaves a report: what was done, what was decided alone and why, what is parked.
- **Switches at any moment, effective at once** (not built): one click in the conversation (and a shortcut), also while the agent
  is answering. Turning it on resolves open question cards with their defaults and parks
  waiting approvals, without restarting anything: the user who was working in Normal and has to sleep just turns it
  on. Turning it off brings the next questions back to the user.

