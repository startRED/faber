# Claude Desktop "Cowork" agent sandbox on Windows: behavior spec (clean room)

Scope: what a person and an administrator observe, what limits exist, what the model is told, and what the parts say to
each other in words. No code, no internal names. Evidence tags: (observed: static) read from the shipped app, helper
programs and VM image contents without running them; (observed: decompiled) read in decompiler output of the Windows
service and the three helper programs; (inferred from strings) names and messages seen in binaries. No REA Evidence IDs
were used for this spec; the underlying private notes are in the owner's corpus. The in-VM daemon itself was not
decompiled yet, which is stated under Unknowns.

## 1. Inventory

- A lightweight Linux virtual machine ("Claude's workspace") that the desktop app starts on demand and where the agent's
  Claude Code runs. (observed: static)
- A privileged Windows background service that creates and supervises that VM for each signed-in user and speaks to the app
  over a local named pipe. (observed: static)
- A daemon inside the VM that creates a separate Linux user per agent session, mounts the user's chosen folders, enforces
  network rules and runs commands. (observed: static)
- Three small helper executables in the VM image: one that drops privileges and applies the sandbox before launching the
  agent, one stand-in for a "claude" command that asks the host to run a model call, one stand-in for plugin command-line
  tools that asks the host for permission first. (observed: static)
- A sandbox settings file shipped in the image with the default network allowlist and a short file deny list. (observed: static)
- A downloadable VM bundle (root disk, kernel, initial ramdisk) fetched by the app, plus a small "binaries disk" shipped with
  the app that carries newer daemon binaries. (observed: static)
- Optional enterprise "EDR integration": a way for an administrator to install a security-vendor sensor inside the VM. (observed: static)

## 2. Feature specs

### 2.1 What the VM is, and what it costs
- Windows runs it with the built-in hypervisor platform ("Virtual Machine Platform"). It is not WSL. If the feature is off the
  app shows a card asking the person to enable "a Windows feature ... to give Claude a secure workspace" and offers the action.
  (observed: static)
- It needs hardware virtualization switched on in firmware, and the Hyper-V worker account needs the Windows right to "log on as
  a service". Group policy can deny that; the app then shows an administrator-oriented card with the exact policy to change.
  (observed: static)
- Default memory is 4 GB; memory and processor count can be set in settings. The root disk is about 8 GB uncompressed
  (about 1.3 GB to download for x64), plus a growable per-user session disk. The guest is Ubuntu-based. (observed: static)
- One VM per Windows user (and per app data location), not one per chat. All agent sessions of that user share the VM and are
  isolated from one another inside it. (observed: static)

### 2.2 First run, start, and stop
- States the person can see: not downloaded, downloading (percent), booting, ready, offline, error. The first start downloads
  the bundle (compressed, hash-checked, resumable, with binary patches when an older bundle exists) and the matching Claude
  Code Linux binary, then boots. Startup is reported as named steps (download and prepare, load native layer, VM boot, guest
  ready, install agent) with started/completed/failed marks. (observed: static)
- The app waits for the guest to announce itself. It checks every 0.5 second and logs "still waiting" every 10 seconds. The
  deadline moves: 60 seconds after the last startup step it saw, but never later than 120 seconds after the start. On timeout
  it reports "VM connection timeout after N seconds" and adds a VPN hint if a VPN adapter was detected. The Windows service
  itself has no connect deadline: its start call returns once the VM is booted and the app does the waiting. A start call is
  given 60 seconds. (observed: static, in the app script; observed: decompiled for the service)
- While running, the app pings the VM every 30 seconds as keep-alive and checks health every 30 seconds with a 15 second
  timeout; five failures in a row restart the VM by itself. A "restart workspace" action exists for the person. (observed: static)
- On app quit the VM is shut down gracefully, forced if it does not stop in time. When no app is subscribed to the service's
  events, the service stops the VM: it checks every 10 seconds and acts on a session that has had no subscriber for more than
  60 seconds, giving the stop 30 seconds. A graceful stop asks Windows for a clean shutdown with a 30 second limit and
  force-ends the VM after that. (observed: decompiled)
- If the Windows service itself is not running the app starts it, backing off between attempts (5 seconds growing to 5
  minutes), and finally says "VM service not running ... Restart your computer". It does not reinstall the workspace in that case.
  (observed: static)
- Failure policy: environment problems (virtualization off, policy denial, disk file compressed or encrypted, service
  unreachable, data folder refused, guest connected then stalled) leave the workspace offline with an explanatory card and retry
  at next app start. Other startup failures trigger one automatic "reinstall workspace" (delete bundle, relaunch) and never a loop.
  Low disk space shows its own message. (observed: static)

### 2.3 Failure cards (what the person reads)
Short title plus one plain line, then expandable administrator details with commands to check. Titles seen: "Failed to start
Claude's workspace", "Restarting Claude's workspace...", "Windows has compressed Claude's workspace disk", "A Windows security
policy is blocking Claude's workspace", "Windows is blocking access to Claude's workspace disk", "Claude's workspace service
isn't running"; messages about not reaching the Claude API from the workspace ("Trying a different networking mode"), not enough
disk space, and "Claude couldn't connect your X: drive to its workspace, so it can't access your local files. Reinstalling won't
fix this. Your chats and files are safe." Each card says plainly when reinstalling will not help. (observed: static)

### 2.4 How the person's files appear in the VM
- Every fixed drive and mapped network drive on the PC is offered to the VM as a network file share served by the Windows
  service, acting with the signed-in user's identity (not as the system account). A required drive share that fails to mount is
  a hard, visible error. (observed: static)
- Each agent session sees only the folders granted to it, at a path under its own session directory (a "mnt" folder with one
  entry per grant). Typical entries: an outputs folder, an uploads folder (read-only), the agent's config folder, skills,
  plugin and project folders (read-only), the user-chosen working folders. (observed: static)
- Three access modes per entry: read-only; read and write but no delete; read, write and delete. The default for user folders
  is write without delete. When the agent's delete fails it is told to ask for permission through a dedicated tool; the person
  sees a prompt naming the exact folders; approval enables delete for those folders for the rest of the session (a task set to
  skip all approvals may be granted at once, and the tool result then replaces the prompt). (observed: static)
- An entry may carry a list of hidden paths or name patterns; hidden items are not visible and cannot be changed from inside.
  The session home folder can be a mostly empty skeleton that exposes only chosen files. Links inside shares are not followed.
  (observed: static)
- Office documents are converted to PDF for preview by running the converter inside the VM against a temporary folder, with a
  cache keyed by content. (observed: static)
- The app can list, size and delete old session folders and prune caches when free space is low. (observed: static)

### 2.5 How the agent runs
- The host app downloads a pinned, hash-verified Linux build of Claude Code and shares it with the VM; the daemon verifies and
  installs it. The host's SDK talks to it through the same message stream as a local child process, only carried over the VM link.
  (observed: static)
- Per session the daemon creates a dedicated unprivileged Linux user and home, a resource group that limits and measures memory
  (peak bytes and the number of out-of-memory kills are reported with the exit event), mounts the grants, writes a per-process
  sandbox policy, then launches the command through the privilege-dropping helper with namespaces, a restricted mount view, a
  system-call filter that prevents the sandboxed code from opening the VM-to-host socket type, and an open-source sandbox
  runtime for proxy-based network control (details in 2.10). If a spawn step overruns its deadline it is abandoned, new spawns of the same name
  are refused until it returns, and the user and mounts are kept so nothing leaks. (observed: static)
- The model-credential never lives in the sandbox environment: the host passes the session token to the daemon separately and
  the proxy attaches/permits it; requests carrying credentials the host has not approved are blocked. (observed: static;
  inferred from strings for the exact flow)
- A stand-in "claude" command lets tools inside the sandbox ask the host to run a model call (prompt plus piped input up to
  8 MB, limited concurrent calls per session, time-boxed to the session's remaining time). (observed: static)
- Plugin command-line tools run through stand-ins: the host classifies the invocation into operations, applies organization and
  user connector settings, may prompt the person (allow once or always), injects the needed credential only for that run, and
  tells the daemon which network destinations that tool may reach; the tool runs in its own sandbox; crashes are reported back.
  Denials come back to the model as a short explanation naming who blocked it (organization policy, the user's setting, the
  user declining) and what to try next. (observed: static)
- Scheduled and background tasks run the same way; one-shot commands (converter runs) have a spawn acknowledgement timeout that
  grows with the number of optional mounts. (observed: static)

### 2.6 Network policy
- All VM traffic leaves through a user-space virtual network inside the Windows service; the host network stack is never
  bridged. Guest addresses are private; IPv6 is optional and can be enabled by setting; the app warns when the PC has no global
  IPv6. (observed: static)
- Default allowlist in the image: package registries (npm, yarn, PyPI, crates), github, Ubuntu archives, and Anthropic domains.
  The administrator can replace the list or use a wildcard for unrestricted; a compliance mode removes the wildcard and keeps
  only Anthropic domains. Anthropic and claude domains go through an inspecting proxy. (observed: static)
- Inside the VM an inspecting proxy with an ephemeral, memory-only certificate authority enforces: allowed domains only;
  no connection upgrades; local and link-local destinations denied after name resolution; a "read-only zone" that only allows
  credential-less GET and HEAD; limits on the beta-feature header sent to the model API; clear blocked-response pages and an
  error header the agent can read. (observed: static)
- The host's proxy settings are mirrored: system proxy, automatic detection and PAC scripts (downloaded by the host, capped at
  1 MB, evaluated inside the VM); a loopback proxy is rewritten to reach the host; the person's corporate root certificates are
  copied into the VM trust store. A background reachability probe of the model API drives the "taking a while to connect" notice.
  (observed: static)

### 2.7 Admission and safety of the service
- Size and time limits (observed: decompiled): one message between the app and the service is at most 10 MiB; one between the
  service and the VM at most 100 MiB; both are a 4-byte big-endian length followed by JSON. Calls the service forwards to the
  guest wait 30 seconds by default (mounting the drive shares 30 s, installing company root certificates 60 s, configure and
  "is running" checks 30 s). Proxy auto-config scripts are capped at 1 MiB and fetched with a 5 second limit, detection 3 s.
  The VM's console log is read in 4 KiB chunks every 0.5 second and buffered up to 256 KiB while the app is not collecting;
  past that, text is dropped and a note says how many bytes. The Windows service is set to restart itself after failing:
  after 5 s, then 30 s, then 60 s; a stop is given 30 s.
- The service only accepts pipe clients whose program is signed by the expected publisher and, when packaged, belongs to the
  same app package. It does file operations for a client while impersonating that client, and refuses links, junctions and hard
  links in paths it is given. It cleans up leftover VMs from earlier runs. (observed: static)
- Guest-reported errors without a process id are discarded on purpose; duplicate or stale "ready" messages are ignored.
  (observed: static)

### 2.8 Enterprise EDR integration (the link used is in 2.9)
An administrator can point the app at a signed package and name a vendor. The service checks the request against registry
policy (allowed paths, vendor, install arguments), shows the package to the VM on a share, the daemon verifies a detached
signature against embedded trust roots and a revocation list, and runs the vendor's installer in a transient system unit with
bounded output. Outcomes (declined, refused with a reason, unknown, completed) are returned and logged without secrets. A
server-side switch can turn it off. (observed: static)

### 2.9 How host and VM talk (observed: decompiled)
- The link is the Hyper-V socket type (a stream socket addressed by VM identity plus a service identity), not TCP. The service
  listens; the daemon in the VM connects out to it, which is how "guest ready" is known. The service identifiers follow the
  platform's standard template that maps a plain port number to a service identity, so inside the VM they look like ordinary
  vsock ports: one fixed high port for control and one low port (1024) for the virtual network.
- Control traffic is request, response and event messages as JSON, each request with a text id such as "req-N". Guest errors
  without a process id are dropped. The console is a separate virtual serial port read through a pipe.
- File shares are 9P shares mounted by the daemon on a service call; the helper later detaches the raw host view of them
  (2.10), so a session only keeps the paths granted to it.

### 2.10 What the privilege-dropping helper does, in order (observed: decompiled)
Run as "helper uid gid [flags] command"; flags: skip the network sandbox runtime, plugin mode, working folder, settings file.
1. Create new mount and IPC namespaces and make all mounts private to them.
2. Remount the message-queue filesystem; mount a fresh in-memory /dev/shm (mode 1777, 512 MB). In plugin mode also in-memory
   /tmp and /var/tmp (same size and mode), plus a private 64 MB home (mode 0700) for the plugin user with its own tmp folder.
3. Detach the hidden-file layer, then lazily detach the host's raw drive shares: on the Hyper-V build every per-drive share
   under the shared root must be a real mount point and is unmounted (finding none is fatal); on other hosts the single host
   share. Any unexpected failure ends the helper with a short message.
4. Set the supplementary group list to just the session group, then set the group id.
5. Normal agent: turn core dumps off, set the user id, then apply the system-call filter. Plugin mode: turn core dumps off,
   apply the filter, then set the user id (the filter is in place before the identity changes), then change directory.
6. Launch. Normal: run the network sandbox runtime through node with the session's settings file (default is the image's base
   settings; a missing file is fatal: "network allowlist config missing"). The child keeps the helper's stdio; the helper
   forwards hang-up, interrupt, quit and terminate signals and returns the child's exit code. With the skip flag it runs
   bubblewrap instead: whole root bound read-write including device nodes, a fresh /proc, a new process-id namespace,
   die-with-parent. Plugin mode: the helper replaces itself with the runtime, which must supply the process namespace.
   The helper passes its environment through unchanged; trimming it happens earlier, in the daemon.
- System-call filter: no-new-privileges first, then a short deny list on top of allow-everything. Wrong CPU architecture or the
  32-bit-compatibility call range: the process is killed. The three io_uring calls fail with "not permitted". Creating a socket
  of the VM-to-host family (vsock) fails with "not permitted". Everything else is allowed; network control is the runtime's
  proxy job, not this filter.
- The network runtime is started with a 2.5 second per-attempt fallback timeout for dual-stack connects.

### 2.11 The two stand-in commands (observed: decompiled)
- The "claude" stand-in takes a prompt as an argument (-p or --print) or on piped input. It refuses more than 8 MB combined
  with "prompt too large (max 8MB)" and says "no prompt ... pass one with -p or pipe it on stdin" when empty. It connects to a
  local socket file of the daemon, sends one framed request (type byte, 4-byte length, JSON with prompt and stdin; at most about
  9 MB) and reads one framed reply of at most 32 MiB: result text goes to standard output; an error reply exits 1; an
  unreachable socket gives "claude is unavailable in this session". It sets no timeout of its own; the host and daemon time-box it.
- The plugin stand-in connects to another local socket of the daemon, sends the command words, working folder, its own folder
  and the tool name, and hands over its own standard input, output and error file handles on the socket so the daemon can
  attach them to the real tool. Interrupt and terminate are forwarded as signal frames. It reads frames (type byte plus 4-byte
  length) until an exit frame carrying the exit code; an error frame prints and exits 2; a close without an exit frame is
  "coworkd closed without exit frame". It fails clearly if its working folder was deleted.

### 2.12 Diagnostics
Service log, daemon console log (read from the VM's serial console, rotated, with a notice when output was dropped), startup
step timeline, and an optional "share debug logs" action. Support bundles include tails of these logs. (observed: static)

## 3. Ideas for Faber (Windows, Claude Code/Codex CLIs, low memory, R125 < 400 MB idle)

Faber's budget rules out a resident VM: Cowork's VM asks for 4 GB, an 8 GB image and a privileged service. What is worth
taking is the policy, not the machinery.

1. Egress control without a VM (high value, medium effort). Run every CLI behind a small local forward proxy that Faber owns:
   allowlist of domains per task, deny private/link-local/loopback targets after name resolution, no connection upgrades unless
   allowed, a "read-only mode" that only permits credential-less GET/HEAD, and readable blocked responses (a header the agent
   sees plus a card for the person). Set the proxy through environment variables for the child process. The proxy lives in
   the engine process and costs a few MB.
2. Credentials stay out of the agent (high value, low-medium). Follow the "token is attached by the broker, not by the
   sandbox" idea: the CLI uses its own login, Faber never reads it (AGENTS.md rule), but extra tool credentials (connectors,
   MCP) are injected per call by a Faber broker and shown in an approval card.
3. Folder grants with three modes (high value, medium). Per-task grants: read-only, write without delete, write with delete.
   On Windows approximate with a per-task working copy or restricted-token process whose ACLs deny delete, plus a
   "request delete permission" card when a delete fails. Add hide-lists (secret files like env files and key stores) enforced by
   running the CLI with those paths denied (deny ACEs on the sandbox SID) rather than a FUSE layer.
4. Native Windows isolation tier (high value, medium-high). Use what Faber already has (job objects, Low integrity, process
   group kill) and add an AppContainer or restricted token with a job memory limit. Report peak memory and OOM-like kills per run
   in the exit event, as Cowork does, so the UI can show "used 1.2 GB" and Faber's own R125 test stays honest.
5. Optional VM tier, off by default (medium value, high effort). If ever offered, make it lazy: start on the first risky task,
   hand the person the cost ("uses 4 GB while open"), stop when idle, ship the disk as a download, never at install. Use the
   platform's own lightweight utility VM rather than a resident service; prefer one VM per user, one account per session inside.
6. Honest failure cards (high value, low effort). Map specific platform errors to plain titles, one line, a "this won't be fixed
   by reinstalling" note, and expandable admin steps. Never loop automatic repairs; allow one self-heal for corrupted installs
   only; back off service restarts (5 s doubling to 5 min).
7. Heartbeat and state model (medium, low). A cheap keep-alive and a health check with a failure threshold before auto-restart,
   and a small state machine the screen can render (not installed, preparing, starting, ready, offline, error).
8. Startup as named steps (medium, low). Stream "step started/completed/failed" events so the screen shows real progress and
   telemetry-free local logs show where time went.
9. Mirror the host's proxy and certificates (medium for enterprise users, medium effort). Read system proxy/PAC and corporate
   roots so CLIs behave as the user's browser does; evaluate PAC lazily in the engine.
10. Approval prompts for tool CLIs with a decision vocabulary (medium, low). Allowed, blocked by organization policy, blocked by
    the person's setting, declined by the person, each producing a short explanation the model can act on.
11. Disk hygiene (medium, low). Show session storage, prune caches under low-disk, never touch user folders.
12. Pinned, hash-verified CLI downloads with a `verified` marker and a version file (already Faber's catalog rule; this confirms
    it is how the incumbent does it).

Memory note for Faber: none of the above needs a resident process beyond the engine; the proxy and broker must be measured
under R125 and reported with the change.

## 4. Unknowns and bounds
- The in-VM daemon was not decompiled yet. Still unknown: the exact folders it binds into a session and which are read-only
  (only the helper's own mounts above are known), the per-process environment it builds, its per-session memory limit, and its
  concurrency and time limits for the "claude" stand-in and plugin runs.
- Not recovered: whether the 4 GB memory is a hard ceiling, and any host-side retry policy beyond the 0.5 second polling and
  the service restart back-off.
- Nothing was run; no network capture; no VM bundle disk was opened (only the helper executables extracted from it).
- The macOS and Linux variants use different native layers; only Windows behavior is described here.
- Credentials were not read or recorded; token-related items are described by purpose only.
