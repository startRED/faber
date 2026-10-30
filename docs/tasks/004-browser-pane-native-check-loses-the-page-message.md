# The browser pane's native check no longer dies on "failed to receive message from webview"

Area: none
Files: testkit/src/screen/browser_pane.rs, src-tauri/src/pane/mod.rs
Depends: none
Size: about 80 changed lines

Why: "browser pane: native navigation and placement within the memory budget" failed in 3 runs on 3 pull requests: pr-376 run
40191709958 (2026-10-25 12:06, "page script failed: the test browser closed"), pr-410 run 40257378643 and pr-415 run
40260884460 (2026-10-26 02:05 and 02:48, "page script failed: runtime error: failed to receive message from webview;
test_site_probe size: era 0->1, made 1->1, owner Some(...b1)->Some(...b1)"). The check dies after 4 to 8 seconds, so it is not
a long wait: `test_site_probe` (a test-hooks command in `src-tauri/src/pane/mod.rs`, `test_probe_error` near line 337) asked
the native child WebView2 for its size and the call failed. `test_probe_error` already compares the pane's era, made count and
owner before and after, and in the last two runs they are equal: the pane was not closed or replaced by the app, yet the
WebView2 getter lost its message. In the first run the probe saw the pane change (the "test browser closed" text). That points
to the child WebView2 itself going away or restarting during the probe (a browser process exit or crash on the runner), or to
the getter being asked while the controller is being moved; the logs do not say which.

Build: first find out, in this order: (1) add the WebView2 process-failed reason and exit code to the failure text of
`test_probe_error` (wry exposes the controller through `with_webview`; read-only, test-hooks only) and run the check 20 times
on a PC that is under load to see whether the child process really exits; (2) if it does, the product must survive it (the
pane recreates the page or reports it as closed instead of leaving a dead handle), so fix `pane/mod.rs` and keep this check as
the proof; (3) if the process is alive and only the getter fails, retry that read-only getter once after a short wait in the
test probe only, and say so in its failure text. Do not retry anything the product itself calls.

Done: `cargo t --filter "browser pane: native navigation"` passes 20 times in a row locally; the pull request states what the
failure was (process exit with its reason, or a getter that fails on a live process) and which of (2) or (3) it took. One
review by the other vendor.
