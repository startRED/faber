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
says until when it waits once every AI is out. The sidebar and the running column say "Waiting for quota until …".
It wraps at 720 px and does not move.
Coming back to it (R224): its row in the running column has, under the title, one quiet line in the data face with
the turns of work done and how long it has run, and under that the last line the agent wrote on one line with an
ellipsis (its full text is the tooltip); the verb beside the AI's name says the wait it is in ("resting after an
error", "waiting for an update to finish"), and "Waiting for quota until …" for a quota wait. After half an hour on its
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
drawn. The newest turn stays open under the header, with "While you were away" and the answers card at its top.
The agent's own closing box (R248, [layout.md](layout.md)) ends Keep going when it says the request is done, checked
once like the stop line. A box that asks the person something makes Keep going wait ten minutes, its row saying "waiting
for your answer"; then Faber sends the first reply for them and the box says which. A conversation with a plan shows
"2 of 5" before its title in the running column, and the sidebar row's tooltip adds it; the agent's now line is the
row's quiet line while Keep going is off.
