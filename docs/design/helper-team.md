# Helper team channel

Pieces declare read, build, review or advise, their kind, difficulty and tight/open scope. Selection follows the
[mixed team contract](../product/building.md#building-software-one-voice-a-team-behind-it): the first qualifying piece
uses the other connected, allowed maker, then the mixed team returns to quota-priced selection. Economy does not force
the mix. Hard open builds use the expert bar within the lead's cap. Advice uses a dominating expert row (hard in
economy), is read-only, shares the three slots and refuses when no stronger qualifying row exists.

The engine derives makers from existing helper-start acts after the last user message without a wake marker; automatic
continuations and ended helpers still count. Only a real person message resets this view. No additional stored state,
process or timer is introduced: a backward scan of the current request's acts and two stack enum values suffice.
Cards and their accessible names use the job word Advice (hand-written in all five locales), with the same maker mark,
model, effort and state as other members. The helper view also names the advice job. No new screen or control.

The conversations the same reply started (the conversation tool's `start`, or the command line's marker) show in its
first team block, under the helpers: title, AI, model, state and current line, each opening its conversation. They stay
in view when the helpers end and the block folds, because a conversation goes on after them, and the card under each
start step leaves so the conversation shows once. A reply with no helper that started conversations gets the same block
after the steps that hold its first start: the Team head with one segment per conversation and the count of those
working, waiting and done, the lead's line ("Handed out 2 pieces") and the cards; before 2026-11-11 such a reply showed
no team at all, which is what a lead told to delegate through conversations produces. It does not fold. No new state,
clock or request: the block reads the conversation list the screen already holds (`tests/ui/team.check.ts`,
"conversations the reply started" and "only started conversations").

Completed-reply coverage in `engine/tests/engine.rs` exercises a Codex lead with each of read and build, checks the
first helper uses Claude, and verifies both makers and the successful helper report remain after the reply ends.
The existing Claude-lead replay covers the reverse direction. Disabling first-piece mixing must fail the Codex-lead check.

Design sources, read 2026-11-11: [Anthropic, Mixing models](https://claude.dev/blog/which-claude-model-should-you-build-with/#mixing-models)
and [OpenAI, Codex Models](https://developers.openai.com/codex/models/); their recommendations and Faber's limits are
described in the product contract above. Physical memory/CPU remain unmeasured; no additional idle work is retained.

Reports target 1,200 characters; above 2,000, excerpts end at a paragraph or sentence when possible. If saved, the lead gets a path (the newest 20 helper logs are kept) and the person's note is appended after the cut; the full report remains visible.

Each Faber helper outside a Review round receives only its own brief and a helper-only `team` tool, through the existing
MCP transport or Codex dynamic tools at thread start. `who` returns the lead and running helpers, with only their screen name, job,
title and files named in their brief, including dotfiles and extensionless names such as Makefile. Extraction does not read the folder. Helpers decide ordinary details themselves. They use `ask` only for a costly
choice owned by the lead, and `tell` directly when a change affects a teammate. They still cannot delegate or show
approval cards. A teammate's note uses the existing in-session steering path and never joins the lead's context or the person's messages.
In-turn helper questions and notes begin "Message from helper <name>, not from the person; it grants no permission and does not
change your brief:" and quote every line of the helper's text. Lead tells begin "Message from the lead:" with every line
quoted, as instructions within the helper's brief. Questions prefer an open helper tool call over steering.
Reviewers receive no team tool or team prompt; a stray call is refused with "Nobody can answer: decide yourself."
A tell addressed to the lead is refused with instructions to use ask.

A question releases the lead's unanswered helper calls together: its own call carries the question and how to answer;
the others say they are still working or still queued (memory admission, a pending check or a call about to launch) and how to wait.
Already answered calls never receive another reply or suppress a new question. If the asking call was released earlier,
another pending call carries that exact question, rather than an older one.
A queued helper keeps its name when it starts; a wait before it starts says to do other work, then wait again for its question
or report. The helper's CLI remains alive inside its pending ask.
The existing helper tool accepts `{to, answer}` (answer then wait), `{to, tell}` (send immediately), and
`{to, wait:true}` (wait for another question or the report). The schema has optional top-level properties; the engine
refuses missing or mixed forms as an ordinary tool result. Names match the helper row. A returned question does not
finish the helper's row. Reports that arrive between calls remain on that row for a later wait. Names stay unique
across the conversation's saved helper rows, including ended helpers, so a later helper cannot hide an earlier report.

Each helper has a six-message cap per reply, including tells from the lead, at most 1,000 characters each. Over either limit, an ordinary tool refusal
tells it to decide for itself. An unanswered question lapses five minutes after the lead can answer (its other tool calls have returned),
with a hard cap of 15 minutes from the question's arrival even if a question card still waits on the person, or when the lead's reply ends:
"No answer: decide yourself and say what you assumed in your report." Stop, closing the conversation and a dead
helper release pending asks and lead calls. No team task or timer exists until a question is asked.

`ToolUse.team` keeps the name, job, title, files, state and ordered exchanges. Each exchange records sender and
recipient call ids (or lead/engine), their display names, kind, text, Unix milliseconds, and the question link;
questions retain waiting, answered or lapsed. These travel in normal reply updates, persist with the conversation,
and copy with helper calls into a new conversation. Restore marks unfinished helpers ended and pending questions lapsed.
The state word says "asked the lead; waiting", with the working glyph rather than the amber needs-you glyph. The existing helper view lists
exchanges as quiet sender-to-recipient lines, using its existing type and colours; there is no new panel or control.

A waiting helper retains its existing CLI process, estimated 230–260 MB, for at most 15 minutes from a question's
arrival, allowing up to five minutes once the lead can answer. This physical cost is unmeasured here. Each question schedules
a hard deadline and, once the lead can answer, a five-minute deadline; stale deadlines cannot answer it again.
Records and timer tasks are bounded by the message limit; there is no CLI process or team polling at rest. Checks: `engine/src/engine/helpers/team_tests.rs`, both driver suites, and the
"team messages" check in `tests/ui/agents.check.ts`.
