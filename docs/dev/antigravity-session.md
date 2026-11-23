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

Projects get Faber's own server in both Ask and Full access. It uses Claude's MCP definitions and the engine's existing
conversation tool handlers. Browse, verify and map still require Full access and the corresponding host capabilities.
Faber answers ACP's extra permission request for its own available tools during a prompt. Permission requests for other
servers still follow the catalog policy. Each launch picks a fresh server name so a project-configured server cannot
inherit its approval. Antigravity uses that name in `mcp_<server>_<tool>`. The work log uses the same names and
links as Claude, and the final answer uses the same conclusion block as the other drivers.

The ACP launch starts the engine host in a private stdio mode, before app startup. An IPv4 loopback listener belongs to one
live driver. A new random secret goes only into that MCP child's launch environment. The first authenticated
connection consumes the secret and closes the listener. Each call reaches the fixed conversation through its existing
event sink. Calls outside a live prompt are refused. Ending the driver drops the connection before process teardown,
and starting or resuming in a new process creates a new endpoint and secret. Faber keeps the secret in memory and out
of its diagnostics. The relay opens no window or shell and exposes no app command table.
If the relay disconnects, the current answer can finish and the next turn starts with a fresh relay.

There is one relay process while the tool server is live, and none when idle without an Antigravity process. The
engine shares its existing runtime, with bounded message queues, a socket and no added background thread or polling.
