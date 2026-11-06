# Native helpers: behavior-only summary (clean room)

Scope: eight small Windows helper programs shipped with two desktop AI apps. Evidence is static only (headers, imports, version resources, printable strings, PE resource entries). Nothing was executed and no pseudocode or decompiled code was used. Statements marked "inferred from strings" are hypotheses to verify with runtime observation, not established behavior.

## 1. Inventory

| Helper | Shipped with | Observed role |
|---|---|---|
| Browser bridge | Claude desktop | Relays messages between a browser extension and a local pipe that several clients share (observed: static) |
| SSH broker | Claude desktop | Brokers remote-shell connections for SSH-backed sessions (inferred from strings) |
| SSH proxy | Claude desktop | Connection-side companion of the broker (inferred from strings) |
| Cowork service | Claude desktop | Windows service that runs an isolated VM for agent sessions, with its own network and admission rules (inferred from strings) |
| Command runner | Codex app | Runs one command under a restricted identity, with time and network limits (inferred from strings) |
| Sandbox service | Codex app | Prepares the sandbox identity, file permissions and firewall rules (inferred from strings) |
| Sandbox setup | Codex app | One-time installer of the sandbox group, identity and firewall rules (inferred from strings) |
| Code-mode host | Codex app | Runs model-written JavaScript in an embedded engine (inferred from strings) |

## 2. Feature specs

### 2.1 Browser bridge (observed: static)
- Behavior: frames from the browser extension arrive on standard input, each prefixed by a 4-byte length. A frame with an invalid length is rejected, and an end-of-input from the browser ends the session.
- Server side: the helper opens a local named pipe and accepts several simultaneous clients. Connection and disconnection of a tool client are reported to the extension as status events.
- States: idle (no client), connected, browser disconnected (end of input). Logs go to a per-user folder.
- Why it matters for Faber: a single long-lived bridge can serve several local clients without each one owning the browser connection.

### 2.2 SSH broker and proxy (inferred from strings)
- Behavior: an SSH-capable helper pair, one side local and one side reached over a secure channel. Errors are reported as text about pipes and process tokens. Details of the protocol are not established.

### 2.3 Cowork service (inferred from strings)
- Behavior: runs as a Windows service. A client connects over a named pipe, identifies itself by application name, then sends requests and receives responses and events.
- Session model: a VM and guest session manager, a user-space network stack with network address translation and a bridge, and mapped network drives into the guest.
- Admission gate: before install or start, the service checks arguments and refuses known security products by vendor and certain install paths. Refusals are logged.
- States: started (a start message is logged), session running, refused (admission failed).

### 2.4 Command runner (inferred from strings)
- Behavior: starts one command with a restricted identity. Standard input, output and error are carried over pipes. A duration limit applies, and the message "Execution duration limit of" is shown when it is hit.
- Network: rules are declared per protocol (HTTP, HTTPS, SOCKS5 over TCP and UDP). Filesystem access is limited to listed entries.

### 2.5 Sandbox service and setup (inferred from strings)
- Setup (one time): creates a dedicated local group and a dedicated local account for sandboxed commands, registers the account, builds access rules, and installs a persistent firewall provider with a rule that blocks outgoing ICMP connects.
- Service: maintains the sandbox directories, grants the sandbox identity access to the profile and project paths it needs, refreshes the identity's token when needed, and keeps the firewall layer in place.
- No secret values are described here.

### 2.6 Code-mode host (inferred from strings)
- Behavior: hosts an embedded JavaScript and WebAssembly engine for model-written code. It accepts CA-certificate settings through environment variables, and can export traces through an OpenTelemetry-style listener when asked.

## 3. How the app talks to each helper
- Browser bridge: stdio from the browser plus a local named pipe for tool clients.
- Cowork service: named pipe with a request, response and event message model, identified by app name on connect.
- Command runner and code-mode host: pipes for standard streams, plus process limits.
- Sandbox service and setup: installed and run by the app; the runtime handshake is not established.

## 4. Sandbox and permission model (inferred from strings unless marked)
- Layers: restricted identity; job and time limits; access control entries on the profile and sandbox folders; a firewall provider with network rules by protocol; filesystem allowlists; an admission gate for the VM service.
- The app grants and revokes these through its helpers; the user is not asked per command (inferred).

## 5. Ideas for Faber (behavior only)
1. Show a command's time limit and network rules on the card before it runs, so a stop is explained rather than surprising.
2. Let one bridge serve several local tools, reporting connect and disconnect as status lines.
3. Keep logs per user, with refusals named in plain words.

## 6. Unknowns and bounds
- No decompiler, no signature check and no runtime observation were possible on this host, so procedures, call graphs and protocol message layouts are unknown.
- Runtime behavior of every helper is unverified.
- The two sandbox helpers contain credential-related strings; no values were read or recorded.
- Evidence IDs for each helper are in the per-binary notes under E:/Dev/re-corpus/native/ and in evidence-bundle.json (8 records).
