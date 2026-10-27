# T3 Code remote control: study and a minimal safe Faber version

Status: read-only study. Nothing was run, built or installed in the T3 clone. Claims come from T3's own docs and source files read in that clone. Items marked "inference" are my reasoning, not stated in T3's docs.

Sources read in full: `docs/internals/remote.md`, `t3-connect.md`, `environment-auth.md`, `connection-runtime.md`, `devices.md`; `docs/user/guest-links.md`, `mobile-notifications.md`, `devices.md`; `docs/operations/connect-setup.md`, `relay-observability.md`, `android-notifications.md`. Skimmed: `docs/user/remote-access.md`, `packages/ssh/src/tunnel.ts`, `packages/tailscale/src/tailscale.ts`, and the file names under `infra/relay/src/` and `apps/mobile/src/`.

---

## 1. Summary

- T3 Code is a local server on the user's PC. It owns providers, execution, files and state. Every client (desktop, web, phone) talks to that server over HTTP and WebSocket. The server is the security boundary.
- Remote access is a routing problem on top of that: LAN, Tailscale, SSH, or T3 Connect (a hosted identity and tunnel broker). Identity does not depend on the route.
- T3 Connect is the costly, complex part. It needs Clerk (identity), a relay Worker, PlanetScale Postgres via Hyperdrive, Queues, Durable Objects, per-environment Cloudflare tunnels and DNS, FCM and APNs credentials, and Axiom for traces. Push notifications and guest links depend on it.
- Application traffic does not pass through T3's servers. The relay brokers credentials and tunnels and proxies only webhooks. Bandwidth cost is low; operations cost is high.
- Faber can get most of the value with a much smaller design: a tailnet-only HTTPS endpoint, one-time pairing, scoped device tokens, a narrow remote API (no shell, no file writes), and an off switch. Push needs an always-on sender with a Google or Apple credential. Recommended: S now, M later for push, and do not build a T3-style hosted relay (L) under the current "no paid extras" rule.

---

## 2. Architecture

**One server per environment.** The environment server runs on the PC and owns providers, command execution, files, the terminal and durable state. A client is a UI. A route is only how the client reaches the server (`docs/internals/remote.md`).

**Identity is independent of the route.** An environment keeps its ID across restarts and endpoint changes. Saved connections live in the client profile. A project and its threads belong to one environment and are never routed to another.

**Ordered routes with failover.** A saved environment holds an ordered route list. The driver connects over the first route that works. Each direct route is first checked with the public descriptor, so a saved LAN address answered by a different machine receives no credential. A route that fails moves to the next; only an incompatible server stops the walk. While connected over a later route, the supervisor preflights earlier ones and switches when one would connect. A route that still fails afterwards goes into a cooldown. Jittered backoff is capped at 5 minutes (`connection-runtime.md`).

**Learned routes.** A connected server reports the LAN and tailnet addresses it is bound to. The client saves them as learned routes and reuses the credential of the route it learned them over. A token paired over HTTPS never starts travelling over plain HTTP.

**Headless listener.** With no configured host, the background service listens on all IPv4 interfaces but closes each connection unless both peer addresses are loopback, private or tailnet. A VPS's public address stays closed. The docs say this is not a firewall: a proxy that rewrites the source address looks private.

**Hosted web is only a client.** The browser stores the connection catalog and connects directly to each environment. Pairing secrets arrive in the URL fragment, which never reaches the hosting origin, and the browser strips them from history.

**Desktop can be remote-only.** The setting `localEnvironmentEnabled` turns off the local environment. The desktop then connects only to saved environments (pairing, relay, SSH).

**SSH.** Desktop main spawns SSH, can launch a remote server, and forwards its port. Cleanup stops only a server the launcher started itself.

**Tailscale.** An ordinary endpoint provider (`tailscale serve` for HTTPS, status probes with timeouts) through the `tailscale` CLI. No separate environment type. Authentication stays the environment's job on every route.

---

## 3. Pairing and authentication

**Scoped pairing grants.** Pairing delegates a set of scopes. Token exchange can narrow a grant but never widen it. Scopes include `terminal:operate` (a shell as the server user), `orchestration:operate` (start a full-access agent), `access:write`, and `filesystem:read`. The docs say the default grant includes management, because `terminal:operate` or `orchestration:operate` can mint a session with every scope, so withholding management would not be a real boundary.

**Pairing links.** One-time. The read model holds metadata only; the raw credential is returned only at creation.

**Session types.** Browser cookies, bearer tokens and DPoP-bound tokens share one scoped session model. An invalid DPoP proof must fail, never fall back to bearer.

**WebSocket tickets.** Bearer and DPoP clients get short-lived WebSocket tickets through authenticated HTTP, so long-lived tokens stay out of socket URLs. A DPoP RPC ticket is bound to the proof key, and the upgrade carries a fresh proof. Media tickets stay bearer because `<img>` and `EventSource` cannot send proofs.

**Per-RPC scope checks.** Every RPC declares a required scope. `RpcScopeAuthorization` checks it before the handler runs. A successful handshake grants nothing extra.

**MCP OAuth for outside agents.** Agents T3 did not launch sign in to `/mcp` through a narrow authorization-code server. Clients are public and use PKCE. The owner grants either read-only access or a runtime-mode ceiling, never a scope list. Only `/mcp` accepts these sessions; every other HTTP and WebSocket path rejects them. HTTPS redirects are allowed for hosted agents, and the approval page names the host that receives access.

**Guest link tokens.** HMACs of the thread and share ID under a server key, so nothing secret is stored. They never enter the session store, so no RPC, WebSocket or MCP path accepts them. Guest routes also require T3 Connect to serve the link's hostname, because the Host header can be forged on a LAN.

**Reusable dev credential.** `T3CODE_DEV_AUTH_TOKEN` is an administrative credential for web development environments only. Desktop and non-development servers ignore it. Its cookie lasts 30 days.

**Asset URLs are bearer credentials.** A signed URL grants access to one file (by device and inode), not its folder. `filesystem:read` is checked when the URL is minted, not when served, so a URL outlives a revoked grant until it expires.

---

## 4. T3 Connect (the hosted layer)

**Identity.** Clerk. The relay accepts session-template JWTs and CLI OAuth tokens. The CLI is a public OAuth client using PKCE. SSH and headless logins use Clerk's OAuth device-authorization grant.

**The relay is a trusted broker.** It does not proxy HTTP or WebSocket sessions. To connect, a client:
1. Gets a relay token, after the relay checks the user has an active link to the environment.
2. The relay asks the environment to mint a one-time bootstrap credential bound to the client's DPoP key.
3. The client exchanges it directly with the environment for an ordinary environment session. The relay never receives that session token, and the bootstrap credential alone cannot be redeemed without the client's private key.

Both sides authenticate the exchange. The environment accepts only bounded, replay-guarded relay proofs for its own identity, linked user and requested operation. The relay verifies signed environment responses bound to the request nonce.

**Limits of that trust model (from the docs).** The relay holds the signing authority for mint requests. DPoP protects an honest exchange from credential reuse; it does not make a compromised relay signing key harmless.

**Managed tunnels.** Each linked environment gets a Cloudflare tunnel and a hostname. The host runs a pinned `cloudflared` (`CLOUDFLARED_VERSION` in `packages/shared/src/relayClient.ts`). The relay resolves endpoints from its own allocations, never from a caller-supplied URL. Health and mint requests must not follow redirects.

**Idle tunnel reaper.** Cloudflare bills a tunnel whether or not a connector is attached. A five-minute job reclaims tunnels that Cloudflare reports down for at least five minutes, or never-connected tunnels older than an hour. `RELAY_TUNNEL_CLEANUP_MODE` is `off` by default and is read at deploy time. Each sweep is bounded: at most ten list requests, 100 deletions, a two-minute deadline, and an early stop on a Cloudflare rate limit. Tunnels with no host recovery registration, and tunnels with no allocation row, are never deleted automatically.

**Recovery.** A host registers its tunnel ID and loopback origin at startup with a signature from the environment key. If the connector exits or `cloudflared` reports repeated rejections, the host asks for a replacement at most once every two minutes. The replacement keeps the same allocation, so the hostname and DNS record survive.

**Webhooks.** The only proxied traffic. The public path `/v1/hooks/...` is forwarded to the environment's tunnel. Opt-in `hold_webhooks_while_offline` makes the relay store raw requests (including the hook token in the path) in a per-environment Durable Object with SQLite. Requests go out oldest first when the environment returns, and are deleted after the environment answers, after 24 hours, or when no user has the environment linked. Each forward carries `x-t3-relay-delivery-id`, so a retried request runs once. The relay never checks the hook token.

---

## 5. Encryption

What the docs say:
- Transport is HTTPS. Managed tunnels and Tailscale Serve provide it. Direct LAN can be plain HTTP.
- Tokens are never sent over plain HTTP to a learned route unless they were paired over plain HTTP.
- There is no application-level end-to-end encryption between phone and PC. I found no such layer in the docs or in the relay source.

Inference: a managed tunnel terminates TLS at Cloudflare's edge and forwards to `cloudflared` on the host. Cloudflare can therefore read plaintext application traffic. T3's model protects credentials (DPoP, scoped tokens, bootstrap binding) from misuse, not message content from the relay operator or the tunnel provider.

Implication for Faber: state this plainly in any design. Over Tailscale (WireGuard, device to device) the content is encrypted between the PC and the phone. Over a Cloudflare tunnel it is not end to end.

---

## 6. What runs on T3's servers vs. the user's PC

| Component | Where it runs | Notes |
|---|---|---|
| Agents, CLIs, files, terminal, git | User PC | Environment server |
| Chat traffic (HTTP, WebSocket) | User PC via route (LAN, Tailscale, SSH, tunnel) | Relay does not proxy |
| Login | T3 (Clerk) | Holds user accounts and OAuth clients |
| Relay token and bootstrap brokering | T3 relay Worker | Per request |
| Environment links, allocations, recovery | T3 (PlanetScale Postgres via Hyperdrive) | |
| Managed tunnels and DNS | T3's Cloudflare account, one per linked environment | Billed per tunnel |
| Held webhooks | T3 Durable Object, opt-in | Up to 24 h, 1 MiB per request |
| Agent activity for push | T3 relay (`agentActivity/`) | |
| Push delivery | T3 relay to Google FCM and Apple APNs | Needs T3's service accounts |
| Guest link pages and messages | Served by the environment via the T3 Connect hostname | Needs the owner's PC online |
| Device streams (simulators) | User PC (or SSH host) | Never through T3 |
| EAS Simulator (cloud devices) | Expo's cloud | Billed to the user's Expo account |
| Traces | T3 Axiom account | Operations telemetry |

---

## 7. Costs T3 carries

Per linked environment:
- One Cloudflare tunnel and DNS record while linked. Idle tunnels are billed until the reaper removes them, and the reaper is off by default.
- Cloudflare API calls for provisioning and recovery. The observability doc records a real failure mode: the account-wide Cloudflare API rate limit stalls provisioning.

Shared:
- Cloudflare Workers, Queues (FCM and APNs delivery, with a dead-letter queue), Durable Objects (held webhooks, agent-activity alarms).
- PlanetScale Postgres (a branch per non-production stage) and Hyperdrive.
- Clerk (users, OAuth, JWT templates, passkeys and native sign-in).
- Axiom (traces, dashboards, alerts).
- Google FCM and Apple APNs credentials (APNs needs an Apple developer key).
- An Android signing setup and a Firebase project.

Operational effort, not just dollars: T3 maintains Axiom alerts and runbooks for Cloudflare rate limits and for expired APNs keys or FCM service accounts (an expired key fails every push job).

Billed to the user, not T3: EAS Simulator sessions.

---

## 8. Push notifications

How it works (`docs/user/mobile-notifications.md`, `docs/operations/android-notifications.md`):
- Ongoing activity cards (Android) and Live Activity Updates (iOS) follow agent work. Finished results stay visible for up to 15 minutes.
- Ordinary alerts stay quiet while the mobile app is in the foreground. Ongoing activity keeps updating in the foreground.
- Viewing a thread on another device does not silence the phone's alerts.
- The relay sends data messages directly to FCM (HTTP v1) and APNs. It does not use Expo Push.
- Background delivery requires T3 Connect. A LAN or Tailscale connection alone does not enable push.
- Messages expire after five minutes. An `UNREGISTERED` FCM response invalidates only that device token.
- Android 7.0 minimum. Android 16 (API 36) can promote activity to a Live Update, subject to device support.
- The Firebase service account lives on the relay, not in the app. OTA updates cannot change the native notification handler, so push changes need a native build.

For Faber: push cannot be done from the PC alone. Something always-on that holds a Google or Apple credential must send it. Options:
1. Hosted relay (T3's model). Large and paid. Conflicts with the "no paid extras" rule and with Faber's scope.
2. A small sender the owner runs (for example a free-tier function holding one FCM service account). Still an external service and a credential to protect.
3. No push in the first version. The phone shows results when opened, or refreshes over Tailscale while the app is in use.

---

## 9. Guest links

Behaviour (`docs/user/guest-links.md`):
- Beta, behind Settings → Labs. Needs T3 Connect and an owner machine that stays online.
- Share one thread. The link works in one browser and expires after 24 hours. The owner can stop it at once. Replacing or closing the share revokes every token.
- The guest sees the thread history and live stream: messages, replies, reasoning, plans, questions, and command output on expand.
- The guest cannot change the model, see diffs, fork, revert, or open files.
- Guest messages wait for the owner. The owner chooses Send (runs as a normal turn with the thread's current permission mode) or Dismiss.
- Attachments: up to 4 files and 10 MB per message, 20 MB pending.

Security design: link tokens are HMACs of the thread and share ID under a server key. Because the share in the thread projection is the only state, replacing, closing or archiving it revokes every token at once.

For Faber: the "guest message waits for the owner" pattern fits Faber's approval style and is worth copying in a smaller form. Guest links need a web page, expiry, revocation and an approval queue, so they are a Medium/Large feature. They are not in the minimal version.

---

## 10. Devices (simulators)

- The environment owns simulators and emulators. The hub (`expo-device-hub`) binds loopback only and is never exposed; the only door is an authenticated proxy.
- Device settings call `simctl` and `adb` directly through typed actions and bypass the hub.
- Agents drive devices through `agent-device`, enabled by an explicit access toggle.

Useful model for Faber's own remote surface: one authenticated proxy, a default-deny allowlist, typed actions, and no generic shell route.

---

## 11. Security risks (from T3's docs, and what they mean for Faber)

1. **Shell-equivalent scopes.** `terminal:operate` is a shell as the server user. Faber must not expose a shell, or generic command execution, over any remote route.
2. **Default grants include management.** A paired phone should get the narrowest grant. Issuing pairing links or sessions needs its own explicit approval.
3. **Source-address trust.** The headless listener trusts private and loopback source addresses. A proxy that rewrites the source looks private. Faber should not rely on source-address checks alone.
4. **Plain HTTP.** Tokens paired over HTTP travel unencrypted. T3 ties learned plain-HTTP routes to how the token was paired. Faber should refuse to pair or send tokens over plain HTTP outside a private network.
5. **Host header.** A LAN client can forge Host. Any access decision made from Host must also check the route.
6. **Signed URLs are bearer.** Media URLs grant file access until they expire. Keep expiry short.
7. **Reusable dev credential.** An administrative shortcut. Faber should not ship one.
8. **Relay trust.** A relay that signs mint requests can impersonate the environment's authority if its key is stolen. It is a single point of trust.
9. **HTTPS redirects for outside agents.** A link someone else sends can deliver access to that person. T3 shows the receiving host on the approval page. Faber should do the same for any outside access.
10. **Guest content leakage.** Guests see what the agent printed or read, including any secret in a file. A remote viewer is a new disclosure path for file contents.
11. **Clock skew.** DPoP depends on correct clocks. Expect support load.
12. **Cost runaway.** Tunnel recovery loops and reaper failures cost money on T3's side. Any helper process in Faber must be bounded.

Faber rules that apply on top (from `AGENTS.md`):
- CLI credentials (`~/.claude`, `~/.codex`) are never read, copied or forwarded to a phone or a relay.
- Child processes never go through a shell.
- Vendor names do not appear in product or feature names. "Remote" or "phone access" is fine.
- Telemetry stays off by default.
- Memory: a remote listener must be measured and stay idle-cheap.
- Third-party tools (`cloudflared`, `tailscale`) are downloaded from their original source, never bundled.

---

## 12. Minimal safe Faber version

Goal: from a phone on the same tailnet, see the status of Faber's threads and send a message to an existing thread. Nothing else. No shell, no file writes, no provider settings, no new threads, no tool-action approvals in the first version.

**Design**
- **Route:** the Faber engine listens on loopback only. Tailscale Serve exposes it over HTTPS on the tailnet. No public listener, no Cloudflare tunnel, no relay.
- **Pairing:** the desktop shows a one-time code valid for 5 minutes. The phone exchanges it for a device token. The token is shown once, stored only as a hash, and listed in Settings with a Revoke button.
- **Scopes:** two only, `threads:read` and `threads:send`. Everything else is refused, including any route that would expose a shell, file write or settings change.
- **Per-request check:** each call checks the device token, its scope, and that the route is the tailnet hostname. This is one function with unit tests.
- **Off by default.** A setting turns it on. A visible "1 phone connected" indicator on the desktop.
- **Sends are normal turns.** A phone message enters the same path as a message typed on the PC, under the same permission mode. No special privilege.
- **No push.** The phone page refreshes when opened. State that limitation honestly in the UI.
- **Logging:** device name, time and action only. Never message text or tokens. Telemetry stays off.
- **Memory:** the listener is one more idle task in the engine. Measure it with no phone connected; the target is no measurable increase.

**What it does not do:** no relay, no hosted identity, no Cloudflare, no FCM, no guest links, no device streaming, no outside agents on `/mcp`.

**Tests before shipping**
- A token with `threads:read` cannot send. A token with `threads:send` cannot reach any other route.
- A request with a forged Host header from the LAN is refused.
- A plain-HTTP request carrying a token is refused.
- A revoked token fails on the next request, including on an open WebSocket if one is used.
- The listener does not start while the setting is off.
- A phone message appears in the thread exactly like a PC message.

**Effort** (S = a few days of focused work, M = several weeks, L = months)

| Step | Effort | Notes |
|---|---|---|
| Tailscale Serve detection and HTTPS route | S | Thin wrapper around the `tailscale` CLI with timeouts; no shell line |
| Engine HTTP listener on loopback, read and send endpoints | S | Separate module; existing paths unchanged |
| Pairing code, hashed device tokens, revoke | S | |
| Scope and route checks with tests | S | Most of the risk lives here; keep it in one place |
| Phone client as a page served by the engine | M | Avoids a native app; a native app with store and signing work is L |
| Settings UI and status indicator | S | Follow the `faber-ui` checklist; the setting lives in Settings, not on a work screen |
| Measurements and docs (product map, what's new) | S | Required by Faber rules |
| Push notifications | M | Needs a sender with a Google or Apple credential; owner decision |
| Guest links | M | Web page, expiry, revocation, approval queue; large test surface |
| Hosted relay for access outside the tailnet (T3 Connect style) | L | Identity, Worker, Postgres, tunnels, reaper, alerts; conflicts with "no paid extras" |

**Recommended sequence**
1. S: tailnet-only read and send, with pairing, scopes, route checks and the tests above, behind a setting.
2. M (owner decision): phone alerts through one sender the owner controls, only if the owner accepts an external credential.
3. Defer L (hosted relay) until there is a paying need and a cost limit.

**Owner decisions (not technical)**
- Whether a phone may send messages, or read-only is enough for the first version. Read-only is S with almost no risk.
- Whether push notifications justify an external Google or Apple credential.
- Whether access outside the tailnet is wanted at all. If yes, that is the L item and changes the cost picture.

---

## 13. Open items in this study

- Did not read the relay's `EnvironmentConnector`, `ManagedEndpointProvider`, reaper or agent-activity publisher in full. Those claims rest on the docs and file names.
- Did not read the mobile screens or mobile connection code beyond file names.
- Did not read `docs/internals/server-updates.md` or the `cloudflared` checksum flow, which the docs cite.
- Did not run anything. All behaviour is as documented.
