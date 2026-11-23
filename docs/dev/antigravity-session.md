# Antigravity: the session driver

How Faber's engine runs an Antigravity conversation over the CLI's ACP server. The facts it rests on are recorded in
[antigravity-cli.md](antigravity-cli.md#over-acp) and [antigravity-folders.md](antigravity-folders.md). Nothing can pick
Antigravity yet: `Vendor::RUNNABLE` holds the other two.

## Start, stop, resume

`start_conversation_agy` calls `agy::session::start` with the resolved executable. One owned process per live conversation
runs `agy acp --gemini-dir <data>/antigravity/profile` with client name `faber`. Each process gets a new temp folder under `<data>/antigravity/tmp`, removed when its end is confirmed. The child does not inherit `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_GENAI_USE_VERTEXAI`, `GOOGLE_CLOUD_PROJECT`, `GOOGLE_CLOUD_LOCATION`, `GCLOUD_PROJECT`, `GOOGLE_CLOUD_QUOTA_PROJECT`, `GOOGLE_OAUTH_ACCESS_TOKEN` or `CLOUDSDK_AUTH_ACCESS_TOKEN` (removed by name, never read). Stop ends the process if `session/cancel` is not answered in time. Idle close ends the process tree. The next
turn uses `session/resume` (an unknown id is an error there, not an empty session) and checks the id it answers.

Ask and Full access both use `default` so outside file reads still ask the host. The host answers ordinary approvals
in Full access and shows the agent's questions in both modes. Other permission modes return `error.notConnected`.
A file request shows its resolved path (junctions followed); an unresolvable one is refused.

An empty model leaves the CLI default. A listed model id uses `session/set_model`. Otherwise the driver sets `model` and
then `thought_level` with `session/set_config_option`. Resume does the same. The out-of-quota recogniser
looks for `RESOURCE_EXHAUSTED` in an error answer. This is a guess exercised by a synthetic fixture because no real
quota error was recorded; it marks the account out and the reply ends with `error.quotaOut`. The session cannot block access to a protected folder inside the working folder, inside its own temp
folder, within an approved command's or MCP tool's reach, behind a junction changed after the request was judged, or
named by a tool whose input carries no `file_path` (`PROTECTED_FOLDER_LIMIT`). See the [folder experiments](antigravity-folders.md).

## Tools

In Full access the session gets the tool servers installed from Faber's catalog (`Delivery::servers`, the same launches
the other two CLIs get, Chat's already cut to its reviewed set by the store) in `mcpServers` of `session/new` and
`session/resume`; every other mode gets none, because a server runs outside any guard (R195). The reader names their
calls `mcp__<server>__<tool>` as the screen already draws them.

Faber's own tools (plan, conversation, helper, verify, browse, progress and the rest) do not reach an Antigravity
conversation. The other two get them inside their own process link: an in-process server for one, dynamic tools for the
other. ACP's `session/new` only takes servers the CLI launches itself, and Faber has no such server for these tools:
`faber mcp` serves the app's command table, not a conversation's tools. They need a small standard-input server that
calls back into the engine for one conversation, with that conversation's permissions; it is not built.
