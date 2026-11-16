# Named application control

`browse` accepts `app: "faber-live"` or `app: "faber-test"` explicitly. This is a conversation-owned hidden renderer of the actual shipped UI, supported by Electron and Windows Tauri and connected to that instance's host, not a desktop controller or the person's main renderer. Test targets require a test instance with its own data directory; this tool does not create test data isolation or launch another installed app. Other developed web apps still use their local `url` in the browser pane. Unsupported native applications need a separate supported adapter and are not claimed here.

Actions: `read` (default), `click`, `type`, `scroll`, `screenshot`, `close`. Read returns visible text and numeric refs for visible interactive elements. Click/type require a ref from that session's last read; disconnected, relabelled, obscured or offscreen elements are refused. Type validates an enabled editable field before any click and inserts text through Chromium. Text is capped at 2,000 characters. `up` reverses scroll. Optional `size: "720x900"` resizes only this hidden screen (300..2560 pixels per side; Faber supports desktop layouts from 720 pixels). Screenshots are fresh JPEG attachments in the existing browser reply. App content is untrusted data.

Every IPC call from this renderer, even a delayed effect after input returns, is sent through `Host::dispatch_as(Caller::Agent { conversation })`. No timing-based Screen privilege swap exists. The only bootstrap accommodations are read-only app state and family-scoped conversation reads; `screen_ready` and `set_clock` are shell no-ops. Main-screen window controls, folder dialogs, sign-in, permission approvals, money, and other Person/Screen operations remain refused. Own and Ask rules remain checked by the host. Password and one-time-code fields cannot be selected or typed. No arbitrary JavaScript, protocol command, URL, desktop input or remote debugging port is exposed to the agent.

The hidden renderer uses the instance's backend: live operations may change real data when agent policy permits. It is not an isolated copy of live data. One conversation renderer at a time, closed when its owning reply ends or stops, after 60 seconds without a call, explicitly on `close`, and at shutdown. No renderer exists before first use. Measured with the real first-run UI and a temporary host: Chromium working set rose from 62,488 KiB to 340,916 KiB (278,428 KiB increase). This excludes the Rust sidecar and is a one-shot working-set sample, not an idle-memory or CPU qualification. Offscreen drawing is capped at 10 frames per second. Existing browse restrictions, turn protections and installed-app debugging refusal remain in place. The hidden renderer never calls show/focus and does not show an OS window.

Bootstrap state filters background command output and running rows to the authenticated caller's family. Its `by` identity is injected by the host; sequence arguments cannot forge it. Live event forwarding denies unknown and unrelated conversation payloads. Electron admits another conversation's stream only after the host authorizes opening it, and discards those subscriptions with the renderer.


## Tauri on Windows

Rejected stale, disabled or obscured controls retain the renderer and its draft for a new read and retry. Transport failures and timeouts close it. Cancellation during creation reserves the session until the pending action finishes cleanup, so a late completion cannot recreate a closed view. Agent sends retain the host's existing refusal; reading through `app_control_open` never changes the person's selection or closes temporary chats.

The Windows Tauri shell routes named app requests through its actual browser adapter. A permanent registry maps each
private child-webview label to its conversation; every invoke, including asynchronous effects, uses that agent's
authority. Host events are sent to the main webview explicitly; control children receive only their own conversation
content and non-conversation refresh/settings/usage events. Family reads retain host checks, but push updates for
siblings are not forwarded. Main-window authority is unchanged. Only one control renderer may exist, with explicit close, 60-second
idle expiry, cancellation/timeout cleanup and shutdown cleanup. Native WebView2 input and screenshots need no
installed-app debugging port or machine-wide input. The private child shares the main WebView2 environment,
allows only its app origin, rejects new windows/downloads and has event-listening capabilities only.

Targets are backend-local: hidden instances with explicit non-default data accept `faber-test`; normal instances accept
`faber-live`. The tool does not launch a test instance. The existing testkit hidden harness launches isolated builds
with their own data and browser profile without copying sign-ins. Test-only hooks exercise the real parser and browser
adapter and are absent from installed builds. macOS/Linux remain unsupported. Native end-to-end evidence and memory
measurements belong in the Tauri verification report; compilation alone does not establish working UI control.


Tauri frontend event subscriptions explicitly target the current webview, including the main renderer; global
subscriptions would receive targeted emits too. The hidden native canary verifies that unrelated/main-only events
never reach the controlled renderer, own-conversation events and settings do, and child-only messages do not reach main.
The native proof also exercises actual UI at 1280 and 720 pixels, Person-operation refusal and trusted click/type.
Terminal messages (done, stopped or error) for the owning conversation close its renderer immediately; observed
children do not revoke their parent's renderer. Idle timeout remains a fallback. Sustained CPU, timed idle expiry and
forced-abort runtime qualification are not claimed by these receipts.

Electron uses one reusable memory-only control profile, cleared before each owner opens it. Its asset protocol and CSP match the main screen; permissions, devices and downloads are denied. Test identity additionally requires an explicit data path outside the default data tree, resolving existing junctions and Windows case aliases.

Native test identity rejects canonical data paths that equal, contain, or sit below the default data tree, including Windows case aliases and existing junction ancestors. Screen-only conversation-navigation and update notices target the main webview explicitly. A test-only storage canary can retain an incognito sibling on the same environment and compare sequential owners; it is not an installed command or agent capability.

Native refs also discover ARIA options, menu items, tabs, checkboxes, radios and switches. A ref records the element's role and label; either changing after the read invalidates the click. Test-only keyboard observation opens the actual command palette to exercise a native trusted option click.
