# Subagent steps

How a reply shows the subagents and helpers its agent hands work to: the rows, the running column, the stuck signs, and the pages
the agent opens in Faber's browser, which open in the same place.
The work log they sit under is in [layout.md](layout.md).

## Subagent steps

When the agent hands part of the work to a subagent (R161), the reply shows one row per helper between the work log and
the answer, in the work log's surface and type: a check, a cross or the live mark, the few words it was given in bold, and
on the right its kind, its state and its step count in the third ink, and an arrow pointing right. A click opens the
subagent in the running column (R161 step 3, R176), which is the one place on the right: its head has a back arrow to
the running list, the subagent's title and the column's close; under it the meta line, the brief word for word in the
person's own bubble (the agent wrote it to the subagent; Codex gives none, and then nothing shows), the tool chips it
ran, then, once it ended, "What it did" (R161 step 4): a quiet block in the work log's surface with the files it changed
in the data face, its checks and how many failed (a failed count gets the error note's 2 px edge), and the changed files
its brief never named. These are counted by the engine from its steps (`engine/src/engine/facts.rs`), never taken from the
main agent's word. Then its report as Markdown. While it works, a quiet line says a CLI subagent cannot be written to while it runs and
"Tell it…" starts a message to the main agent. A helper from the other AI is a session of its own, so it gets a box
instead (R196): a line saying the words go straight to it, a field and "Send" (Enter sends, Shift+Enter breaks the
line). What the person wrote stays above the box in their own bubble, with a line in the third ink under it: sent, read,
or never read because the helper had finished. Once a subagent ended, in a project with both AIs connected, a quiet line
and "Continue with a helper" start the conversation's box asking the main agent to hand the rest to a helper. The row open there has the hover surface and a 2 px accent edge on its
left (amber on a stuck row). The column opens the way it always does; the content swaps without motion, since it is a
click the person just made. Opening another conversation goes back to the list. Rows sit in a `minmax(0, 1fr)` grid, so a long
tool line ellipsizes instead of widening the reply. A helper that never finished reads "stopped" once the reply has
ended; one sent to the background keeps the live mark until its task notice arrives (a ring, never the square of a stop button); a refused one reads "declined" in
the third ink, never as a failure. Known limit: when a reply moves to another AI after a quota stop, its open tools,
helpers included, read as working again with the new reply.

A Codex subagent (`spawn_agent`, started only when the person asks for one) is the same row. Codex gives it a path instead of a description, so its title is the
path's last part with spaces (`/root/read_docs` reads "read docs"), and it has no kind. Its approvals show on the
conversation's own card, since the person cannot open the subagent; one that arrives after the reply ended is refused,
and Stop and closing the conversation reach the subagent too. What a subagent still does after its reply ended reaches
no reply, so its step reads "stopped". Codex counts each thread's usage on its own, so the
engine adds the subagent's tokens and cost to the reply that started it, and marks the reply's usage incomplete when a
subagent is still at work as it ends.

A subagent waiting on the person's answer reads "waiting for you" with its live mark in amber; the card under the rows is
that answer. One that seems stuck (R161 step 6, R174) is the conversation's other amber: the mark and the row's edge
turn amber, its state reads "seems stuck", and one line under the row says in plain words what the engine saw, with
"Look", which opens it in the running column, and "Tell it…", which focuses the conversation's box with "About “title”: " at
its start (Codex takes it into the running turn; Claude Code after it, or at once with "Send now"). The line arrives with
the normal entrance (`--dur-arrive`, none in reduced motion) and goes when the subagent moves again or the reply ends; the
words carry the meaning, the amber only repeats it. The signs are counted by the engine (`engine/src/stuck.rs`): the
same step six times in a row, the same step failing three times with no file changed between (twice and its retry), or
no step for 10 minutes (30 while a command runs, since builds say nothing until they end). The time a question waits for
the person never counts. Neither CLI lets Faber speak to the main agent while it waits for its subagent, so the main
agent's own chance is the retry the numbers leave it. Background subagents are not followed.

A helper from the other AI (Faber's helper tool, R161) is the same row: its title in bold and, as its kind, the AI that
ran it, so a Claude Code reply reads "Codex". Its commands show under it and its report is its final words. The person
never sees its approval cards: in a mode that asks, what the helper needed approval for is refused and named in its
report, and the agent does that step itself, where the card shows. Known limit: a conversation resumed from before the
helper tool existed keeps its Codex thread without it, since Codex takes dynamic tools only when a thread starts.

### Pages in Faber's browser

A page a project's agent opened in Faber's browser (R161 step 7) is a card among the reply's files, in the file card's
surface: the picture the browser took of the page's top (a 16:10 crop at most 480 px wide, the page loaded at 1280 by 800
and drawn at 0.375), then the page's title over its site in the third ink, both ellipsized, and "See the page". The
title and address come from Faber's own first lines of the answer, never from the page's words. "See the page" opens it
in the running column, the subagent's place: the same head (back arrow, the title, close; once a link or redirect
leaves the card's site, the title is the site the page is on now), a meta line with the site it is on now,
and under it a box that fills the rest of the column, in the third surface with the hairline edge, over which the page
itself is drawn by a second webview of the same window. The box is measured and the page placed again when it changes
size, when the window does and once the column has slid in; when the box goes, the page moves out of the window, where
it keeps drawing, and it closes after five minutes nobody used it. Opening a subagent closes the page and the other way
round. Known limit: the page is a native view over the screen, so a menu or toast that would cover the column passes
under it. While it is on screen the page is the person's: they click and type in it, and the agent's call waits for them
to go back. A page clicked through several times is one card, its latest. Where the page asks for a login, a captcha or a
payment the card has one line in the conversation's other amber, with the 2 px edge of a stuck subagent's line: "Your
turn: log in on this page", and no picture, since one could hold what the person typed; the agent cannot click or type
there.
