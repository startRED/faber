# Native bridge, SSH helpers, native addon and OpenAI launchers: behavior spec (clean room)

Scope: the small native programs shipped beside the Claude desktop app (browser bridge, three SSH helpers, the native addon the
app loads) and the two OpenAI desktop launchers. Behavior only: no code, no internal names. Marks: (observed: static) = read from
shipped files; (inferred from strings) = deduced from text in a binary or from how the app calls it. Nothing was run.
REA evidence IDs: Codex launcher ev_2e46652671668a3862a1438972967103dada2bbf4310fe1073755ec7d3173c19 and
ev_37af621dc6f4d22e9f03455124f9dcee91ac029c7bf58b0d0713e46cebdc49b5; Chromium launcher policy lookup
ev_fa179dc4d0b3e10aa240fa35a0174dbae7721044ff090cfe677b20bd2a7a245c; sign-in helper flow
ev_d4199a30eec4a781614efd0b9b7e3149be893cb6695beaaff888a90dc29c4085; relay broker pipe
ev_21a1cf3f43b7dc6b70ea90dae817a072080e3c643346096219203a817c1ee01f; command wrapper environment scrub
ev_bf7703fac9555e6c66f8327b5e18ff1ff737916fe12e447fa4bbc0efaacfa564. The browser bridge and the native addon are
inferred from strings and from headless decompiler output plus the app's own scripts (REA sessions timed out on those two).
A later headless decompile of the bridge host, the three SSH helpers and the Chromium launcher confirmed the numbers below
(marked "observed: decompiled").

## 1. Inventory

| Piece | What it is, one line |
|---|---|
| Browser bridge host | Small Rust program the browser starts as a native-messaging host; relays between the browser extension and local tool clients over a per-user pipe. |
| Native addon | One Node addon (109 functions): synthetic input, window and app listing, screenshot masking, voice hold-to-talk hook, input recorder, TPM key, DPAPI, Windows Hello, safe file operations, process and port inspection, wake timers, registry, VM image delta. |
| SSH sign-in helper | Tiny program OpenSSH runs when it needs a password, passphrase or host-key answer; forwards the question to the app. |
| SSH relay broker | Per-connection server the app starts; owns the private pipe the sign-in helper talks to and tells the app exactly which process asked. |
| SSH command wrapper | Stand-in for a user's own ProxyCommand: runs it with the sign-in plumbing removed and kills it when ssh ends. |
| Chromium launcher (OpenAI, "ChatGPT" file name) | Stock Chromium launcher for the app runtime; profile folder policy, single-instance hand-off, crash reporter. |
| Update trampoline (OpenAI, "Codex" file name) | 20 KB stub that relaunches the packaged app after an update. |

## 2. Feature specs

### 2.1 Browser bridge (Claude in Chrome)

What the person sees: in Claude (desktop or Code), the model can drive their real browser: read a page, click, type, go to a
URL, manage tabs, read console and network, upload a file, make a GIF of what happened. A small "Claude in Chrome" integration
card in settings shows whether the extension is found in any browser profile. When several browsers are connected the person
picks one; the pick is remembered.

How it is wired (observed: static + inferred from strings):
- The browser starts the host through its native-messaging registration (written per browser in the user registry, for Chrome,
  Edge and several enterprise or alternative Chromium browsers). The registration file lives in the app's data folder.
  Detection of "extension installed" reads the browsers' profile folders (Default and numbered profiles).
- Browser side framing: a 4-byte little-endian length, then one JSON message. The host refuses inbound frames larger than
  64 MiB (67,108,864 bytes) with a log line, and drops any outbound frame over 1 MiB (1,048,576 bytes) with a log line instead
  of crashing (observed: decompiled). The app keeps its own
  requests under roughly 1 MB and cuts larger sealed requests into slices of about 750 KB.
- Local side: one named pipe per Windows user (name ends with the user name). Access control lists only the current user and
  SYSTEM. The pipe is two-way, byte oriented, with 64 KiB buffers each way, no instance limit and remote clients rejected
  (observed: decompiled). Any number of local clients can connect (the desktop app, the Claude Code CLI). Same 4-byte-length JSON framing.
  The app dials it only through a native check that the pipe's server process belongs to the same user; if that check is missing
  it refuses to connect ("refusing to dial ... unverified").
- Messages from the browser extension to the host: a liveness ping (answered with a small message of type "pong" carrying one timestamp field), a status request
  (answered with a message of type "status_response" holding exactly two fields, the host version and the computer name,
  used to tell machines apart; observed: decompiled), tool responses (sent back to the client
  that asked) and notifications (copied to every connected client). Messages from clients: tool requests with a method, params and
  a tool-use id. The host tells the extension when a client connects or disconnects and how many remain. A client request without a method
  is rejected with a log line naming the client. Anything else from the browser logs "unknown message type". If the browser closes the
  pipe (end of input) the host exits; the app then keeps polling for the host with growing back-off (x1.5 up to 30 s) and says
  "still polling" in logs every 10 attempts.
- Pairing and secrecy between app and extension (observed: static, in the app): before any tool call, both sides prove who they
  are with short-lived signed assertions fetched from the vendor account service with the user's sign-in; they check the other
  side's assertion, do an ephemeral key agreement, confirm the keys with a MAC in both directions, and from then on every
  tool request, response, permission request and inference request is encrypted and authenticated per message, bound to its
  kind and tool-use id and, when sliced, to its slice number. Clear-text frames are dropped once paired. A pairing lasts at most 8 hours
  and is renewed when idle; an in-flight call is allowed to finish. A liveness probe (an empty tool call) goes out every 5 s while idle
  and must answer within 10 s, else the pairing is rebuilt. Refusals are classified: account mismatch, organisation mismatch,
  wrong peer kind, pairing disabled by the vendor (durable), not signed in (waits for a token), rate limited (back off).
- Alternative route: a hosted bridge over a secure websocket for a browser on another machine; same pairing request/response
  vocabulary; routing acknowledgement and a pong age are reported so the app can tell "browser asleep" from "tool failed".
- Choosing a browser: persisted choice wins; a single extension is chosen automatically; several: prefer the one on this machine
  that is live; else the person is asked (cards list local ones first). Tools to list, select and switch browser exist for the model.
- Model contract: tools named read_page, find, form_input, computer (click, type, scroll, key, drag, screenshot, zoom), browser_batch
  (a list of those run in order), navigate, resize_window, gif_creator, upload_image, file_upload, get_page_text, tabs_context_mcp,
  tabs_create_mcp, tabs_close_mcp, read_console_messages, read_network_requests, shortcuts_list, shortcuts_execute,
  list_connected_browsers, select_browser, switch_browser. The model is told to start by reading the tab context, to work in its own
  new tab and to reference elements by ids returned by read_page or find. Tool calls carry settings: permission mode, allowed
  domains, whether this client handles permission prompts, and a session scope.
- Permission round trip: the extension can send a permission request in the middle of a call (what kind of tool, the page URL, the action
  details, a category). The app shows it, the person allows or denies, and the answer returns on the same call. If the pairing
  expires while a prompt is open the call fails with a clear reason.
- Error taxonomy mapped from the extension's text: script permission denied, script timeout, script error, tab not found (tab closed or in
  another group), action permission denied, element not found, action failed, page text too large or empty, zoom shortcuts not
  supported. Other typed errors: no extension connected, call superseded by a newer call, timeout (with or without routing ack),
  extension disconnected mid-call, browser changed, request too large.
- Timeouts: calls wait on the order of 2.5 minutes by default and about 4.5 minutes for long ones (inferred); pairing steps 5 to 15 s.
- States: not installed, installed but no browser connected, connecting, pairing, paired, refused (with reason), reconnecting.
- Why it helps: one shared, authenticated door lets any local agent use the person's logged-in browser, with prompts they can see.

### 2.2 SSH remote sessions (Claude Code on a remote machine)

What the person sees: they add an SSH host (from their normal ssh configuration) and open sessions on it. Progress lines appear
while connecting: "Detecting remote OS and shell...", "Deploying Claude Code server to ~/.claude/remote/...", "Downloading Claude Code CLI"
(locally), "Downloading Claude Code CLI on the remote machine", "Uploading Claude Code CLI to remote", "The network is slow to
respond — retrying the connection...", "Your SSH key was refused. Retrying once...", "Waiting for you: Answer the host's sign-in
request." (and a jump-host variant), "Waiting for this computer's network...". Download and upload lines show "N / M MB". Sessions
left running on the host keep running; when the app comes back it reattaches ("Your sessions there are kept").

Transport (observed: static, in the app):
- The app uses the operating system's own OpenSSH client, version 7.6 or newer (older ones are refused with a message). It reads
  the person's ssh configuration (HostName, ProxyJump, ProxyCommand, Match blocks, known-hosts settings) and passes the needed
  settings back as command-line options, because the app addresses the host by host name and ssh would otherwise skip some Match blocks.
- Connections are multiplexed (one master, several channels) or discrete one-shots; a file channel uses the SFTP subsystem (if the
  server refuses it the person sees a specific message: enable the sftp subsystem).
- On the remote side the app deploys a small server program under a hidden folder in the person's home (plus the CLI), by SFTP,
  with checksums, with retry of interrupted uploads. The server speaks line-delimited JSON-RPC over the ssh channel. A reattach
  sends a "capabilities" request with a secret to the already-running server; if it answers, sessions continue. The server can check
  that the caller is the same user (modes: off, on, by process id, network only, unavailable) and refuses callers from other namespaces.
- Platform detection picks the right server and CLI build (Windows, macOS, Linux, musl, x64, arm64). If the host's platform changes the
  app says the sessions cannot reconnect.

Sign-in prompts (the part that needs a UI) (observed: static):
- When ssh needs a secret it runs a helper instead of a terminal. The helper sends the question to the app through a private pipe; the
  app answers and ssh continues. The helper never asks the person anything itself.
- The app classifies each question by its text: host-key confirmation, key passphrase, account password, smart-card PIN, other
  keyboard-interactive (one-time codes, long texts over 8 KiB are treated as generic), plain notice, and "confirm" (denied: there is no
  confirm UI). Questions about the helper itself (a probe at setup) are swallowed.
- Host-key card: shows the host name, the address if different, key type, fingerprint (SHA-256 or MD5 form), up to six other names the key is
  known by and a "more omitted" mark, and flags names it will not trust (control characters). The person trusts or declines; trusting is
  recorded by ssh in the person's own known-hosts file. If ssh accepts a typed fingerprint the app answers with the fingerprint, else "yes".
- Order is a safety rule: secrets are asked only after the server's key is confirmed (by the person now, or already in known-hosts). Otherwise
  the prompt is withheld and the connection aborted with "peer unconfirmed".
- Other rules: passphrase declined -> the key is skipped; empty password or PIN counts as dismissed and aborts; prompts arriving after the
  connection authenticated are denied; prompts from a child process of the master (not the master itself) are refused; if the asker cannot be
  verified the connection is abandoned. Only up to a bounded number of prompts per attempt; repeated identical prompts are de-duplicated or
  answered from what was just typed ("answer unseen").
- Messages for configuration problems (all seen as UI text): host key changed -> connection refused, do not trust the new key unless the
  host was rebuilt; known-hosts disabled in config -> explains the two valid fixes; Match host vs Match originalhost; ssh configuration error;
  helper blocked by policy; ssh from the organisation can no longer be used; signing in keeps failing (open the session and send a message to
  type the password or unlock the agent); local-network permission on macOS.

How the helpers cooperate (observed: static):
- The relay broker is started by the app with a minimal environment (only the program search path and the system root). The app tells it a random pipe name; it creates the pipe so only the
  current user can use it, then reports its own process id and start time. The app gives the sign-in helper that identity.
- The sign-in helper, started by ssh, finds a small file in the connection's control folder (readable only by the user) that holds the pipe
  name and a random token. It connects with a 10 s limit, checks that the process at the other end is the broker the app announced (id plus
  start time, defeating squatting), and sends one line of JSON: protocol version, token, its process id, its parent, up to eight ancestors with
  program paths, a hint (notice, confirm or normal prompt) and the prompt text base64-encoded. It waits for one line back: ok plus answer, or not
  ok. It prints the answer for ssh. For a notice it prints nothing and exits successfully.
- The broker holds each connection open until the app answers (so the person can take minutes), and for each request reports the real client
  process id from the operating system and the chain of ancestors with program paths. Inside the broker (observed: decompiled):
  every accepted connection gets a number and goes into a table keyed by that number; one line per request goes to the app
  (standard output) and each reply is written to the held connection followed by a line break, after which the connection and its
  table entry are removed; "close" from the app drops one entry; on shutdown all held connections are closed. Client lines are
  read through a 64 KiB buffer, and a client gets 15 minutes to send its line. The line writer to the app serialises each
  message as one JSON line, one at a time. The sign-in helper itself allows 10 s to connect, 10 s to write its request and
  10 minutes to wait for the answer (observed: decompiled), so the broker's 15 minutes always outlasts it. When the helper
  cannot reach the relay for a real password or passphrase prompt (not a notice), it also ends the parent ssh process, so ssh
  does not keep retrying; for notices it just exits. The app accepts only requests whose token matches, whose
  claimed id equals the real one, and whose first program is the staged sign-in helper.
- The command wrapper is put on ssh's command line when the person's own config already uses a proxy command or jump host. It reads the user's
  real command from a file, removes every sign-in variable (including a display value that marks the helper) so the user's command cannot
  reach or confuse the relay, probably forces "never use askpass" for it (inferred), starts it inside a job that dies when the wrapper dies, and
  pipes standard input and output through. Decompiled detail: the wrapper adds exactly one environment entry, ssh's own setting
  that says never to use an askpass program (25 characters, confirmed), after removing the others. The user's command is created
  suspended, put into a job with the "kill everything when the last handle closes" limit, and only then resumed, so it can never
  run outside the job. If the job cannot be made or joined, it logs a warning and runs the command outside the job rather than
  failing. Handles are inherited so standard input and output pass straight through; the wrapper waits without a time limit and
  returns the command's exit code.
- ssh options set by the app: no stdin, batch mode off only when a relay is possible, strict host-key checking chosen per situation (ask, yes, or
  accept-new for first contact the person confirmed), multiplexing options, an explicit known-hosts list. If the broker dies mid-session the
  connection continues without prompts (batch mode) and later prompts fail visibly.
- Why it helps: real ssh behavior, the person's own keys, config and known_hosts, while the UI stays inside the app.

### 2.3 Native addon (what the app can ask Windows to do)

Describe-only list, grouped by what the person experiences (inferred from strings and from call sites):
- Computer use: type text (optionally paced like a person), press key combinations (validated first), move/click/scroll the mouse, read the mouse
  position; list running apps, installed apps with icons, open windows (title, owner, process id), displays, the app or the UI control under a
  point or focused; detect a remote-desktop session (some features refuse there); bring a window forward; encode a screenshot as JPEG.
  Screenshots can be taken with chosen windows blacked out: either every window that is not on the allowed list, or windows from a deny list; the
  app refuses to capture if masking is unavailable.
- Hold-to-talk voice: a global keyboard hook with three hold-to-activate chords for talk, a quick "card" answer and dictation; a probe of the focused
  text field and optional reading of that window's text for context; the chord keys are swallowed while held; the result is inserted at the caret
  (by paste, flagged so clipboard history ignores it). A separate microphone capture streams audio chunks with a device list and mic state.
- Watch and learn: a global recorder of mouse and keyboard events with timestamps, blocked in remote sessions.
- Identity and secrets: a TPM-backed signing key per device (public key plus signing, flag if hardware-backed); user-scoped data protection for a
  secret shared between app profiles; Windows Hello prompt with cancel and availability; checks that connect to a pipe only if the other end is owned
  by the same user or by the system.
- File safety: open a root folder once and then open, stat, create, delete or rename entries relative to it without ever following a link out of it.
  The app refuses to fall back to ordinary path opens if this is missing.
- Processes: process tree, lineage (parent chain, group), creation time, memory footprints, a "pre-load this process's memory when RAM allows" call, a job
  object that holds the app and its children, which processes are attached to a console, and the list of TCP listeners with their owning process. The app
  joins listeners with lineage to find dev servers started by a session and offers them in its preview.
- System chores: schedule or cancel a timer that can wake the computer; read/write/delete registry values and keys; edit the user PATH without expanding
  variables; start or query a Windows service (the sandbox VM service); enable the Virtual Machine Platform feature; check whether the compute service is
  available and which services are missing; apply a compressed binary delta to rebuild a VM disk image (returns checksum and bytes written);
  list network drives; find the default app and icon for a file type; package identity and "is the package being serviced".

### 2.4 OpenAI desktop launchers

- Chromium launcher (observed: static): a stock Chromium executable stub for the app runtime. It reads a machine or user policy value (under the vendor's
  policy key, product "Codex") that can override the user-data folder, loads the browser library and runs it, hosts the crash reporter, and carries the
  integrity digest of the packaged app archive. A second launch of the app finds the running one through a hidden window message and hands over its
  command line (observed: decompiled: it posts a "START" text with the working folder and the arguments, plus the id of the
  launching shortcut or app when known, and waits up to 20 seconds, giving up early if the first copy is hung; a timeout or an
  unavailable window is logged as such). The launcher can also be started as its own crash-reporter process (the same program
  with a crash-handler switch, a per-process pipe and a database folder), so a crash of the app is recorded by a sibling and
  not by the dying process; it loads the browser library with an optional pre-read of the file that a switch turns off; on top of that the app code takes the single-instance lock and queues the second instance's arguments (deep links). The app name
  and profile folder are "Codex" even though the executable is named "ChatGPT". The runtime marks its user agent with a "CodexBrowser" prefix for the vendor's
  own domains, enables WebMCP and a few WebRTC experiments, and has one switch to turn off background networking.
- Update trampoline (observed: static, REA): takes no arguments. It finds the single app entry of its own package and asks Windows to activate it, giving
  the new process permission to take the foreground. The real updater (in the app) downloads signed MSIX packages into a per-package state folder, cleans leftovers on
  start, waits about a minute after process start, checks on a timer, and installs on restart; the trampoline is what starts the new version afterwards.

## 3. Top ideas for Faber (value/effort ranked)

Faber context: Tauri desktop app driving Claude Code and Codex CLIs, with its own browser pane and terminal; children never go through a shell;
never read CLI credentials; memory budget matters.

1. SSH remote projects via system OpenSSH with a prompt relay. Reuse the person's ssh config and keys; run ssh with a private askpass helper and show
   host-key, passphrase, password and one-time-code questions as Faber cards. Rules to copy: secrets only after the key is confirmed; trust recorded in
   the user's known_hosts; denial paths for confirm prompts and post-auth prompts; state lines while connecting; reattach to sessions left running.
   Value high (remote dev is a top request), effort medium-high. Faber's engine can host the broker role in-process (a pipe with a user-only ACL).
2. Host-key and credential cards in the chat flow: one card type "Trust this server?" with name, address, key type, fingerprint, other names; buttons
   Trust / Cancel; never auto-accept. Cheap once item 1 exists; also useful for MCP server trust.
3. Local pipe door for the browser pane: expose Faber's browser pane to Claude Code and Codex as a local MCP server over a per-user pipe, with the tool
   set named above (read page with element ids, find, click/type, navigate, tabs, console, network). Copy: owner-only ACL plus a "server owned by me" check
   on the client, 1 MiB frame cap with a clear error, liveness probe, call superseding, typed error classes, permission request mid-call with allow/deny
   buttons that carry the URL and action. Faber already lets the agent click and type on its pages; this makes it available to external CLIs too. Medium effort.
4. Dev-server chips: list TCP listeners, join with process ancestry to the session's CLI, and show "Open preview of :5173" under the reply. Windows
   API is simple (listener table plus snapshot of processes). Low effort, high perceived polish; fits the "less text, more interactive" goal.
5. Job-object kill-on-close for every child (CLI, ssh, MCP servers, wrappers) plus the "app job" marker: nothing survives Faber closing, which also
   protects memory budget. Probably partly there in the winproc crate; confirm coverage for the ssh path.
6. Identity checks for local pipes: refuse to talk to a pipe whose server is another user, and verify the asker by OS-reported pid and ancestor
   chain, never self-reported. Reuse for any Faber helper that gets a secret.
7. Screenshot masking for any future computer-use: black out windows that are not on an allow list before the model sees pixels; refuse to capture if masking fails.
   Medium effort, high safety value.
8. Hold-to-talk dictation chord with a tiny HUD and paste-at-caret flagged to stay out of clipboard history. Optional feature; medium effort.
9. Handle-relative file operations for agent edits (open the project root once, resolve every path beneath it, never follow a link out). Strong containment
   for "edit files" tools; medium effort on Windows.
10. Update relaunch stub pattern (package identity based) if Faber ever ships as MSIX; otherwise skip. Also wake timers for scheduled automations.

## 4. Unknowns and bounds

- The native addon is still described only from strings and the app's scripts; its decompile was not part of this pass.
- Not recovered: the type and unit of the pong timestamp, the crash reporter's upload address and whether uploads are on by default,
  and whether the launcher registers any link scheme (no registration code was seen in it; the app or installer most likely does).
- Nothing was observed at runtime: no pipe traffic, no real ssh prompts, no real browser session. The vendor account endpoints used for pairing were not contacted.
- The remote server program (downloaded by the app) and the browser extension itself were not in scope or not present; their behavior is only what the
  app side expects.
- macOS-only addon functions (clone, plist, dataless files, Mach attestation) are stubs here and were not studied.
