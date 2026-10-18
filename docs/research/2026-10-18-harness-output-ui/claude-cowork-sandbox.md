# Claude Desktop "Cowork" agent sandbox on Windows: behavior spec (clean room)

Scope: what a person and an administrator observe, what limits exist, what the model is told, and what the parts say to
each other in words. No code, no internal names. Evidence tags: (observed: static) read from the shipped app, helper
programs and VM image contents without running them; (inferred from strings) names and messages seen in binaries. No REA
Evidence IDs were used for this spec (the REA tools belonged to another worker); the underlying private notes are in the
owner's corpus. Where the shipped helper programs could not be decompiled yet, this is stated under Unknowns.

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
- The app waits for the guest to announce itself up to a deadline; on timeout it reports "VM connection timeout after N
  seconds" and adds a VPN hint if a VPN adapter was detected. (observed: static)
- While running, the app pings the VM every 30 seconds as keep-alive and checks health every 30 seconds with a 15 second
  timeout; five failures in a row restart the VM by itself. A "restart workspace" action exists for the person. (observed: static)
- On app quit the VM is shut down gracefully, forced if it does not stop in time. When nobody is connected to the service for a
  while the service also stops an idle VM (the delay was not read). (observed: static; inferred from strings for the idle stop)
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
  runtime for proxy-based network control. If a spawn step overruns its deadline it is abandoned, new spawns of the same name
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
- The service only accepts pipe clients whose program is signed by the expected publisher and, when packaged, belongs to the
  same app package. It does file operations for a client while impersonating that client, and refuses links, junctions and hard
  links in paths it is given. It cleans up leftover VMs from earlier runs. (observed: static)
- Guest-reported errors without a process id are discarded on purpose; duplicate or stale "ready" messages are ignored.
  (observed: static)

### 2.8 Enterprise EDR integration
An administrator can point the app at a signed package and name a vendor. The service checks the request against registry
policy (allowed paths, vendor, install arguments), shows the package to the VM on a share, the daemon verifies a detached
signature against embedded trust roots and a revocation list, and runs the vendor's installer in a transient system unit with
bounded output. Outcomes (declined, refused with a reason, unknown, completed) are returned and logged without secrets. A
server-side switch can turn it off. (observed: static)

### 2.9 Diagnostics
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
- The helper programs' decompiled code was not available when this was written; the privilege-dropping helper's exact mount
  list and system-call rules, the idle-stop delay, the guest connect deadline and the exact per-process environment were not
  read. Statements about them are from names and messages only (inferred from strings).
- Nothing was run; no network capture; no VM bundle disk was opened (only the helper executables extracted from it).
- The macOS and Linux variants use different native layers; only Windows behavior is described here.
- Credentials were not read or recorded; token-related items are described by purpose only.
