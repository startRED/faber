# Claude Desktop resources: feature inventory from UI strings (behavior only)

Scope: resources folder of the installed desktop app (lawful copy). Excluded: app.asar and ion-dist/assets JS (other worker).
Evidence tags: (observed: static) = seen in a resource file; (inferred from strings) = deduced from names/strings only.
Nothing was executed or launched. No code is reproduced; on-screen strings are paraphrased unless short.

## 1. Inventory (one line per feature area)

1. Cowork (background tasks in a managed VM): VM requirements, disk cleanup, WSL restart/update prompts.
2. Remote Control (phone and claude.ai reach this computer): folder allowlist, cloud-session auto mode.
3. Code sessions: side chat, split view, import of CLI sessions, trust-folder prompts, bypass-mode warning.
4. SSH remote sessions: connect/reconnect states, host-key and unencrypted-connection warnings.
5. Computer use and simulator control: per-app allow once / always allow, screenshot disclosure.
6. Browser pane and web reads: show/hide browser, allowed-host reads, browser-login access, Chrome opener.
7. Permissions and tool approvals: allow once, always allow, re-approval after SDK change, Auto mode.
8. Needs Your Input / Sessions Waiting for You: attention queue for sessions blocked on the person.
9. Artifacts: artifact window, open-link confirmations, copy-files-between-artifacts, comments, loading and error states.
10. Previews and files: Show/Hide Preview, Show/Hide Files, Terminal, Browser, Changes panel, Dev Tools.
11. Skills: upload, 30 MB limit, pointer to Customize › Skills.
12. Extensions and MCP: install-extension gate, signed/blocked extensions, MCP add/reload, third-party MCP found.
13. Usage and plans: 5-hour limit, weekly limit, extra usage, credits, plan names (Free, Pro, Max, Team, Enterprise).
14. Hardware Buddy and maker devices: BLE pairing over a serial-style UART, JSON-lines protocol, device status fields.
15. Diagnostics, updates, tray: crash/graphics prompts, diagnostic report, log folder, tray background running, update install.
16. Sign-in: device-code style GitHub sign-in, sign-in cancelled and expired-session states.
17. Dictation, Quick Entry, text tools: Add to dictionary, Copy as image, Copy link, Tap to open.
18. Web web-app shell (ion-dist frames): Artifact frame, Connect and Share windows, locale-aware shell.
19. Helper executables (names and strings only): cowork service, SSH broker/proxy/askpass, Chrome native host.

## 2. Feature specs

### 2.1 Artifacts and the artifact frame (most relevant to "less raw text")
- Behavior: an artifact is a published page shown in its own sandboxed window ("Artifact" title). The header shows
  the artifact title and an owner byline. (observed: static, frame HTML)
- Byline and trust: the header line reads that content is user-generated and unverified. The app-wide label says
  artifacts may contain unverified or potentially unsafe content. (observed: static, en-US and frame HTML)
- States: loading spinner; after 60 s visible time with no finish, a "took too long to load" message with a
  "Try again" button (reload); a failed script load shows "Couldn't load artifact"; a "Try again" button reloads.
  (observed: static, frame HTML)
- Host capabilities the page may use only if the host grants them: comment mode, comments list, comment composer,
  export, save file, save blob, open-in-Claude, stage-in-chat, summon-into-chat/session/channel, device camera,
  microphone, speech, geolocation, motion, screen share, MIDI, keyboard-done, edge-to-edge, transparent background.
  Each is a flag on the host, not a visible setting. (observed: static, frame HTML; names paraphrased)
- Themes: light, dark, system; a font choice (Anthropic or system). (observed: static, frame HTML)
- Open confirmations: "This artifact wants to open {url}" and "Always allow {protocol} links from artifacts".
  (observed: static, en-US.json)
- Copy files: "Allow Claude to copy files from another artifact into {title}?" and a note that some files come from
  another artifact that may belong to someone else. (observed: static, web i18n)
- Data: "read this artifact's structured data" lets Claude read the page's stored records in a task; viewers may have
  typed that data. (observed: static, web i18n)
- Versioning and sharing: "Artifact shared", "save a version of this artifact", "Discard unsaved changes?",
  "Artifact retention defaults", "Listed artifacts", "Allow Claude to list your artifacts?". (observed: static, web i18n)
- Model contract: Claude can publish an artifact through an artifact tool; the tool result shows "Using artifact tool".
  (observed: static, web i18n)
- Why it helps Faber: an artifact is a rich, interactive result card that carries the answer instead of a text wall.
  Its trust label, loading watchdog and open-link confirmation are reusable patterns.

### 2.2 Charts, cards and widgets (what the resources show)
- Strings: "About this chart" (a chart with an info affordance), "Answer a question to send" (a question card gating
  a send button), "Tools this artifact can use" (a tool list card), "Annotate page in preview" (annotation on a
  rendered preview). (observed: static, web i18n)
- Hardware Buddy reporting: fields shown as small labelled tiles: Task, Progress, Velocity, Level, Availability,
  Uptime, Heap, Battery, Charging, Connected/Disconnected. (observed: static, en-US.json)
- Limits: no chart-type names or chart rendering code strings were found in the resources; treat chart rendering
  as not evidenced in this pass. (unknown)

### 2.3 Questions, input requests and attention
- "Needs Your Input" and "Sessions Waiting for You", and "Claude needs your input to continue". A session blocked on
  the person is listed in one place. (observed: static, en-US.json)
- "Answer a question to send" shows a question must be answered before a send. (observed: static, web i18n)
- Tool approval: "Allow Claude to use {toolName}?" and "Allow Claude to control {apps}?" with allow once or always
  allow. (observed: static, en-US.json)
- Re-approval: "The device tools on this computer changed after Claude last approved them... Click Allow" ("Re-approve
  Claude's device tools?"). (observed: static, en-US.json)
- Why it helps Faber: a structured question or approval card replaces a free-text question; the reply is one tap.

### 2.4 Code sessions (side chat, split view, trust, bypass)
- Side chat: New Side Chat, Side Chat, Hide Side Chat; Split View. (observed: static, en-US.json)
- Continue: "Continue Last Claude Code Session", "Continue {title}", "New session for {title}, or do it here?" (a
  two-choice card). (observed: static, en-US.json)
- Trust gate: "Trust {cwd} and start a code session?" with the note that read and write access is granted and the
  folder is trusted for the Code tab. (observed: static, en-US.json)
- Bypass mode: a warning that skips permission checks, with a confirmation "Run coding task in bypass permissions
  mode?". (observed: static, en-US.json)
- Import: "Import Claude Code CLI sessions", plural count "Imported N sessions", "Some sessions were skipped because
  they are currently running." (observed: static, en-US.json)
- Activity: "Claude is working in N sessions. Quitting now will interrupt that work." (observed: static, en-US.json)
- Changes panel: Show/Hide Changes; "Leave Design?" guard with "Changes you made may not be saved." (observed: static)

### 2.5 Previews, panels and dev tools
- Preview: Show Preview / Hide Preview; "Preview stopped. The server on this port is still running in your session."
  (observed: static, web i18n)
- Panels with show/hide pairs: Files, Terminal, Browser, Dev Tools, Changes. (observed: static)
- Dev tools: Inspect Element, Show All Dev Tools, Developer menu. (observed: static)
- Text tools: "Copy as an image", "Copy Image", "Copy link", "Tap to open", "Add to dictionary". (observed: static)

### 2.6 Usage and plan limits
- Panel: "Your usage limits", "Plan usage limits", "Usage unavailable". Meters: "5-hour limit", "Weekly · all models",
  "Extra usage · {used} of {limit}", "Usage credits · {used} of {limit}". (observed: static, en-US.json)
- Plan names shown: Free, Pro, Max, Team, Enterprise, Unlimited. (observed: static, en-US.json)
- Credits: a note that credits cover usage past plan limits. (observed: static, web i18n)
- Why it helps Faber: a compact meter with a used-of-limit line is clearer than a number in a message.

### 2.7 Cowork (background VM tasks)
- Requirements screens: QEMU needed; hardware virtualization needed; a vsock kernel module needed; macOS 14 needed;
  "isn't available in this build". (observed: static, en-US.json)
- Disk: "Cowork Disk Cleanup", "Free Up Cowork Disk Space…", "Freed {amount} of disk space." (observed: static)
- WSL: "Restart WSL?" with a note that it stops every running distribution and its sessions. "Update WSL?"
  (observed: static, en-US.json)
- Task start: "Trust {directory} and start a Cowork task?" and the same new-session-or-here choice. (observed: static)

### 2.8 Remote Control and cloud sessions
- Off state: "Remote Control is turned off". Turn-on note: "Remote Control will be turned on for this computer".
  Re-sign-in note: "Sign in again to keep Remote Control available". (observed: static, en-US.json)
- Folder list: up to a set number of folders may be listed for phone and claude.ai; an organization may restrict
  folders; pinned folders stay listed. (observed: static, en-US.json)
- Cloud auto mode: "Let cloud sessions run commands on this computer without asking?" with a warning, and a
  per-session note "This session is in Auto mode, so Claude is using {device} without asking." (observed: static)
- Session origin: "Session started from another device". (observed: static)

### 2.9 SSH remote sessions
- States: "Claude Code couldn't connect", "Claude Code couldn't start", "installed on {host} but isn't responding",
  "Can't reconnect to {host}", reconnect stopped because the local ssh is too old. (observed: static, en-US.json)
- Trust: "Connection is unencrypted", "Confirm it's {host} to connect", "Always allow reads on {host}?".
  (observed: static, en-US.json)
- Helper names and strings (inferred from strings): the SSH broker, proxy and askpass programs; askpass is described
  as answering ssh sign-in prompts for the app and doing nothing when run directly. (observed: static, helper strings)

### 2.10 Computer use and simulator
- "Set up computer use", "Allow once", "Always allow {entry}" and "Allow Claude to control {apps}?".
- Disclosure: "Screenshots are sent to Anthropic and kept according to your normal conversation retention settings."
- Simulator: "Claude will control this simulator and take screenshots of its entire screen."
- Site-level warning: "Claude is about to act on {sites}. This action was handed to you to decide."
  (observed: static, en-US.json)

### 2.11 Browser and logins
- "Let Claude read your {browser} logins?" with the note that macOS may ask for the login password; a list-sites
  action. (observed: static, en-US.json)
- Local network: "This covers every site this computer serves on port {port}" and "The name {host} comes from your
  local network, not the public web". (observed: static, en-US.json)
- Opener shim (claude-browser-shim.js, read in full): when a tool opens an http(s) link in a Code session, the
  request goes to the app first and the app may show the link in the session's Browser pane. The app has about 3 s
  to answer. If the answer is "pane" the link stays in the app; "declined" or no answer opens the OS browser. Links
  with user name/password, file links, and non-web targets go straight to the OS. A test mode reports only.
  (observed: static, shim source behavior paraphrased)

### 2.12 Permissions and modes
- Auto mode: described as "Claude is using {device} without asking" in cloud sessions. (observed: static)
- Bypass mode: see 2.4. (observed: static)
- Allow scopes: allow once, always allow. (observed: static)

### 2.13 Skills and extensions
- Skills: "Upload skills", "Skill file too large", "Skill file is too large. Maximum size is 30 MB." Pointers
  "See them in Customize › Skills" and "Find the rest in Settings › Import." (observed: static, en-US.json)
- Extensions: "Install Extension?" and a note that an unsigned extension is not verified; a blocked extension shows
  the reason from a security blocklist; "Could not verify extension install permissions". (observed: static, en-US.json)
- MCP: "Add MCP server", "Add this MCP server to Claude?", "Found N MCP server in Claude (third-party)",
  "Reload MCP Configuration", "Some MCP servers couldn't be loaded". (observed: static, en-US.json)
- Developer mode: "Enable Developer Mode?" gate for the BLE device API. (observed: static, en-US.json)

### 2.14 Hardware Buddy and maker devices
- Pairing: "Hardware Buddy & Maker Devices", "Build your own device", "Pair with {name}", a 6-digit code shown on the
  device, "None found. Make sure yours is on and nearby.", "Device did not respond".
- Protocol described for makers: advertises a name prefix over a serial-style BLE service; UTF-8 JSON, one object
  per line; a device may return a response when a prompt is present. (observed: static, en-US.json)
- Heartbeat: the app sends a snapshot on change and a keepalive every 10 s; a completed turn can fire one event
  with the raw content array; events over 4 KB are dropped. (observed: static, en-US.json)
- Why it helps Faber: a device status card and a plain line protocol are a model for live status tiles.

### 2.15 Sign-in
- "Verify sign-in code", "Enter this code on the GitHub page", "Sign-in was cancelled.", "Your session has expired."
  (observed: static, en-US.json)

### 2.16 Diagnostics, tray and updates
- "Claude crashed repeatedly", "Graphics problems detected", "Hardware acceleration disabled", "Generate Diagnostic
  Report", "Show Logs in File Manager". (observed: static)
- Tray: "Claude runs in the system tray"; closing keeps it running. (observed: static)
- Updates: "Update Available", "Install Update", "Skip This Update", "Restart to Update to {version}", and a policy
  note that the organization requires a newer version. (observed: static, en-US.json)

## 3. Top 10 ideas for Faber, ranked by value/effort (value first)

1. Question and approval cards: a structured card with 2–4 buttons, plus a "why am I asked" line (2.3). Low effort.
2. Session attention queue: a list of sessions waiting for the person, with plain status text (2.3, 2.4). Medium.
3. Artifact-style result card: a sandboxed card with title, "unverified" byline, loading watchdog and retry (2.1). Medium.
4. Usage meter row: "used of limit" bars for window and credits, with an "unavailable" state (2.6). Low.
5. Side-by-side panels with show/hide pairs (preview, files, terminal, changes) instead of inline text dumps (2.5). Medium.
6. Open-link confirmation: ask once before a result opens an external link; "always allow" per protocol (2.1). Low.
7. Trust gate for a folder before a coding task, with read/write scope stated in one line (2.4). Low.
8. Continue or new choice card when resuming earlier work (2.4). Low.
9. Live status tiles for a long task (progress, level, battery-like fields), refreshed on change (2.14). Medium.
10. Copy-as-image and copy-link actions on rendered output (2.5). Low.

## 4. Unknowns and bounds of this search

- Read: en-US.json (desktop UI strings, about 810 lines, fully); ion-dist/scc.json (no circular import groups);
  ion-dist/custom-3p-gated-imports.json (empty map); ion-dist/frame-shell.html, frame-connect.html, frame-manage.html;
  claude-browser-shim.js. Sampled: ion-dist/i18n/en-US.json (about 32,000 key-value pairs; keyword-filtered only,
  not read in full); ion-dist/i18n/dynamic/en-US.json (small, sampled).
- Not read: ion-dist/index.html, third-party-notices.txt, and the other locale files. Helper binaries were only
  grepped for printable strings; they were not executed.
- Chart rendering, diff rendering, and comment-thread UI are not evidenced by name in these files; the web bundle
  JS (excluded) is where those would be found.
- Helper purposes (inferred from strings): the Cowork service runs a Go program that owns the VM bundle, a virtual
  network with a host-NAT address, a named pipe for the app, and a service install/start/stop/status CLI. The SSH
  broker and proxy are Go SSH helpers; askpass answers ssh sign-in prompts. The Chrome native host is a Rust program
  that reports connected clients and forwards tool requests to the browser extension.
- Not included: credentials, account data, and any CLI session content. None was read.
