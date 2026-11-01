# The agent splits big work by size and Faber carries every piece to its end

Area: Agent's plan and closing box, Conversation started by the agent
Files: engine/src/persona.rs, engine/src/engine/siblings.rs, engine/src/engine/plan.rs, engine/src/results.rs, ui/Thread.tsx, docs/product-map.md
Depends: the rework's honest ending (#547), visible acts (#545), the Faber CLI (#544, #548), results (#550), plan steps that open their conversation (#467)
Size: several tasks of about 300 to 600 changed lines; this file is the brief that splits into them

Why: on 2026-11-01 the rework itself ran this way. One manager split the work into eleven streams, each led by a
conversation that kept its goal in a state file, and cheap Codex workers built every piece while the other vendor
reviewed it. Thirty-six pull requests landed in one night. Almost everything that went wrong was plumbing that Faber should own,
not the AI:

- Workers ended or stopped and nobody knew. They were started where no end reached their lead, so hours went by with
  nothing running.
- A piece stopped halfway with its changes unsaved, or reported "risky, stopping" without editing.
- Two pieces claimed the same files and blocked each other.
- A lead restarted without its goal and no longer knew its main mission.
- The middle layer of eleven Claude leads re-reading 150k-300k tokens on every notice spent the most quota.

The owner decided (R-next of this pull request) that this becomes how the project agent works on big requests, inside
its own conversation (R160, R161), never as a missions screen.

Build, in this order, each its own pull request:

1. Size rule. The agent judges the request: small work it does itself, medium work goes to one to three helpers, big work
   to one conversation per piece. Pieces never share a file. Extend the lead words of `persona::LEADS` and the size rule
   of task 021; there is no second way to start a team.
2. Goal per piece. Each plan step keeps its piece's goal, what done looks like and what is left, so a conversation
   restarted or compacted reads it back first. This is the state file of that night, kept by the engine, not by the AI.
3. Faber wakes the agent when any piece ends, stops or goes quiet, with that piece's result (honest ending and results
   ledger). Nothing is ever polled, and a stopped piece is never silent.
4. Proof or an honest stop. A piece ends with its checks and changes listed, or says it stopped and why. A piece whose
   worker stopped without editing twice moves to the stronger model of the same vendor with a narrower brief.
5. Two levels. The agent and its workers only. A middle layer of lead conversations is used only for work too big for
   one agent, and the plan says why.
6. Cost line. The plan shows each piece's cost and the total next to what it delivered, from the results ledger.

Done: on the `leader_eval` big requests, each big request opens one conversation per piece with disjoint files, every
piece ends with proof or an honest stop that reaches the agent without polling, and a restarted agent names every
piece's goal. Memory and processor cost measured per step. One review by the other vendor per pull request.
