# Keep going on the screen

How a project conversation that goes on by itself looks, in the composer, the thread, the running column and the sidebar
(the engine's side is the Keep going row of [product-map.md](../product-map.md)). The rest of the conversation's layout is
in [layout.md](layout.md).

Keep going (R199): in a project conversation under way, the composer bar has a "Keep going" chip after the model's,
with the loop icon; pressed (`aria-pressed`) it takes the accent's soft fill and ink, never the amber, and its tooltip
says the composer's Stop ends it. A message Faber sent by itself takes no row (R241): the reply to it shows the same icon,
small, after the model's name, with "Faber asked it to keep going" as its tooltip, and a reply to it with nothing in it
is not drawn. A reply after which it stopped by itself ends with one calm line that says why (C001). Nothing moves.
When every AI it may use is near its limit, spent faster than its daily line, out, or billed per use (R216), one
amber line sits right above the composer, at its width, with the loop icon: before it turns on, the chip's first click shows it with
"Keep going anyway" and "Not now" (turning it off never asks); while it is on, the same line stays without buttons and
says until when it waits once every AI is out. When recent outside use holds it, the same line and running row say it is waiting for the account. The sidebar and the running column say "Waiting for quota until …".
It wraps at 720 px and does not move.
Coming back to it (R224): its row in the running column has, under the title, one quiet line in the data face with
the turns of work done and how long it has run, and under that the last line the agent wrote on one line with an
ellipsis (its full text is the tooltip); the verb beside the AI's name says the wait it is in ("resting after an
error", "waiting for an update to finish", "waiting for its command to finish" while a background command of its own
runs and it has nothing else to do), and "Waiting for quota until …" for a quota wait. A lead waiting for the
conversations it started shows that wait, never a rest that is over. Faber checks which reports already arrived before
saving an explicit child wait, including reports combined into one message. If every named child has reported, the lead
continues with those reports. On startup, current conversation-list rows identify candidates for resuming work and
restoring waits or helper reports. Current member rows also tell whether a report is owed or whether the member has
work that can still wake its lead. This keeps a finished member in its team while its history is busy and avoids opening
finished histories just to classify them. New and changed files are scanned, and recovery still checks the conversation
before sending work. After half an hour on its
own, the newest reply starts, right under the model's name, with one quiet line and the loop icon, in the same style
as the stop line: "While you were away (3 h): 14 turns of work, 1 switch to the other AI, 2 questions answered for you."
(a part that did not happen is left out). The questions Faber answered (the agent's own and its closing box's) are one
quiet card under it on the soft fill: "Faber answered 2 questions for you" with the loop icon, then a row each,
the question cut to one line in the second ink, the answer chosen in the full ink, and a ghost "Change" button. "Change"
sends nothing by itself: it opens, under its row, the question in full (no longer cut) and every choice as a radio list,
Faber's own picked and marked "Faber's choice" in the third ink, then "Change answer" (enabled once another choice is
picked) and a ghost Cancel. Only "Change answer" sends the choice, as the person's message; Cancel and Escape close it.
One row opens at a time. Nothing moves.
One run, one block (R251): two or more turns in a row Faber sent by itself, with no message of the person between, sit
under one header in the work log's disclosure: the loop icon, "Worked on its own · 2 h · 14 turns · plan 5 of 7" on one
line with an ellipsis, its tooltip counting what Faber asked ("Faber asked it to keep going ×12"). Closed by default; open,
it shows the earlier turns, a little dimmer, each with its own work log folded, and a turn with nothing in it is not
drawn. The newest turn with content stays open under the header, with "While you were away" and the answers card at its top.
The agent's own closing box (R248, [layout.md](layout.md)) ends Keep going on its first done conclusion, just like the stop line,
without another request to double-check. A box that asks the person something makes Keep going wait ten minutes, its row saying "waiting
for your answer"; then Faber sends the first reply for them and the box says which. A conversation with a plan shows
"2 of 5" before its title in the running column, and the sidebar row's tooltip adds it; the agent's now line is the
row's quiet line while Keep going is off.
While Keep going waits for a command, a new command announced during the fallback cancels that wake. The next fallback
follows the latest command end. If the original 15-minute limit arrives while a command still runs, Keep going proceeds
as usual without stopping the command. A command that finished in time still gets its fallback.
Without Keep going too, a reply that ended while a background command of its own still runs can stay at work (a render,
a build). This applies when there is no closing box or its box leaves work to do without declaring a blocker. The row
says "waiting for its command to finish" and holds back the "needs you" dot, done card and lead report. When the command
ends the agent goes on in a reply under Faber's quiet line. If another command is announced before Faber's fallback
turn, it cancels that fallback and waits for the last command to end. The original half-hour command limit still holds.
A command still running at that limit lets the reply end as its box said.
The Claude driver tracks Bash and PowerShell calls as possible writers before launch and links shell task notices to
their original reply, including notices that arrive after it ended.
A Claude Stop request whose caller timed out or disconnected leaves room for a later attempt. Acknowledging a Stop
request does not mark the command stopped; the task's own terminal notification does.
When a driver fails, the reply keeps waiting for its commands, including with Keep going off. Confirmed driver exit
releases the wait once the last tracked command is gone. If the driver confirms exit first, Faber releases that same
wait only after removing the idle driver. An idle replacement can serve the ordinary reply that still waits.
Stop, archive, a later reply or a changed wait cancels the old wake.
An open approval stays with the person even if a command was running.
Faber checks capacity and permissions before the next turn as usual. The command outcome remains unknown without its
terminal notification. An unresolved writer or another running command keeps the wait up to its original limit.
With Keep going on or off, the project's conversation list, latest-answer read and plan report that member as working
while it waits for a command. It keeps a working place in the team, and another agent cannot resume it or archive it as
finished. The usual brief hold on a peer's routine
report also considers it active. A lead's saved wait extends when every named member can still report, including a member
waiting for its command after a driver exit. A member's open permission or recorded wait failure still lets the lead
recheck the team's state.
An older Claude session can deliver a last command output before retiring. An output request expires after five seconds
without a reply. Progress or completion during a pending request can trigger one follow-up to that request. After the
command ends, late progress cannot start another read, and an expired reply cannot overwrite a newer tail.
The final output can already be a tail cut by Claude. Faber preserves the provider's truncation flag, even for a
capture within Faber's own limit, so agents see that the output is incomplete without receiving a partial line.
Project agents can tell a progress capture from a terminal capture. Codex needs a completed command item
with an output snapshot and no pending native exit. Claude needs a successful output response for a request
issued after the tracked task's terminal notification. The provenance flag is independent of empty or clipped output.
The project command result reader can report active executions by their existing IDs, scoped to the caller or a direct
child in the project. It takes a bounded metadata snapshot and leaves running output in the screen's command list.
When Codex begins a goal continuation on its own, the reply joins the conversation under “The AI resumed the work on its own.”
Faber follows the running turn's notifications without sending a message. Stop also interrupts a continuation that arrives late.
Stop remembers the native thread across a restart. On reopening, Faber leaves an unfinished native goal parked while
the person's messages still go through. It leaves the stored goal as it was, and a thread with no goal or a completed
one opens normally. Turning Keep going on in a project lets its next session continue the goal again. The app keeps
other conversations on their existing service. When the goal lookup times out, a stopped thread stays parked and a
requested rearm uses the CLI's defaults. In Chat, a new conversation can start a new native goal.
Before asking Codex to compact, Faber waits for a goal turn it already knows about. If their notifications cross,
the goal reply keeps what it said and the queue waits for the compaction's own result. Stop still reaches a compacting
turn whose identifier arrives after the goal turn stopped.
If Codex ends a turn without showing which operation ran, a short wait gives the requested compaction time to identify
itself. If the ambiguity persists, Faber holds the queue and retires that thread instead of assuming it can take new work.
A native command can yield to the agent with a terminal still running. It releases its step so queued messages can reach
the agent. Its work log result and proof stay open until the driver reports a numeric exit code, even if another update
omits the terminal handle.
A closing box that stops short of the job with nothing only the person can give ("Should I continue?", or done with work
left, or blocked over work it can still do, such as "make the labels larger") is not a question for them either: the next reply starts with Faber's quiet line "Faber asked it to finish the job
instead of stopping halfway", and the box's buttons stay unanswered on the reply before it. An agent that asks the same
again without changing anything meant it, and the person gets the question.
