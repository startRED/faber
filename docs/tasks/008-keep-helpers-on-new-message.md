# A new message never stops the agent's background helpers

On 2026-10-17 the owner wrote a new message in a project conversation while its Claude Code agent had four background
subagents building. The turn ended as "[Request interrupted by user]" and the CLI stopped all four with it, so the work
was lost and had to be started again. The owner asked, word for word: "colocar nos doc ou sei la oque do faber para
corrigir isso no futuro para não acontecer novamente".

Find which path ran: "Send now" on a queued message, Stop, or Faber's own send while busy (`engine/src/claude.rs`,
`SessionCommand::Interrupt` and `Steer`, the queue in `ui/Thread.tsx` `Queued`). A message written while the agent
works must reach it without an interrupt (queued for the next turn, or folded in as Claude Code allows), and while
subagents or helpers run, every action that would interrupt the turn must say so first ("This stops 4 helpers at work")
and offer to wait instead. Stop stays a real stop.

Done when: an engine test replays a turn with background subagents, a message sent during it is delivered, and no
interrupt is written to the CLI; a screen check shows the warning before an interrupting action while helpers run.
