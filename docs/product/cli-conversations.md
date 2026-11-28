# Conversation states and reads

CLI reads leave seen marks and done cards alone. The views use these states.

- `working`: a turn is running or queued to run.
- `waiting`: saved wait, permission, question, plan approval or quota hold.
- `concluded`: the last reply ended cleanly with the agent's own closing box (`wrapUp`) marked `done`.
- `ended`: the last reply ended cleanly without a `done` closing box.
- `interrupted`: Faber closed while the last reply was running (`faber continue <id>` resumes it).
- `stopped`: stopped by the person.
- `failed`: the last reply ended with an error.
- `new`: no assistant reply to the latest message.
- `idle`: the default list state for a conversation that is neither working nor waiting.

`faber list --state failed,interrupted` selects those endings from saved summaries. An unknown or unsupported
state is refused. A missing family ID yields an empty list. `faber read <id>` includes the exact state and latest closing box.

`faber read <id> --from 12` reads only message 12. Add `--limit` to read between one and ten messages from that point.
The response includes the total `count`, the `first` message number when the page has messages, and the selected
messages in order. Numbers count both user and assistant messages in the current history. The state and latest
conclusion describe the whole conversation. Numbered and recent reads skip unselected message bodies and tools in
closed or archived records without opening or marking them seen. Tool rows include recorded exit codes, refusals and
interruptions. Numbered reads require the running app to advertise `from` and `limit`.

`conversation_headers` maps messages to global numbers without their bodies. Pass `id` and optional `from` (1),
`limit` (20, up to 100) and `role` (`user` or `assistant`). With a role, `from` counts matching messages.
`total` counts every message and `matching` counts the filtered messages. Use each `number` with `read --from`
and check its ID after edits. The list leaves seen marks alone. A closed record's `streaming` message is shown
as `stopped`, matching the numbered read.

`conversation_command_results` reports recent background command endings. Set `runningOnly: true` to list active
commands with their IDs and masked labels. The running page omits partial output. This read is available to an
identified project agent for its own conversation or its direct child. See [command results](lead-agent.md#universal-by-design).
An older app that does not advertise the filter cannot answer a running-only request.

`conversation_steps` reads one numbered message's tools for an identified project agent. Use `from`, optional `id`
(the caller by default), `limit`, `offset` and `failedOnly`, or select one `tool` ID. [Scope](lead-agent.md#universal-by-design)
