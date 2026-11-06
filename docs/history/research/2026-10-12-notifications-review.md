# Attention notification review

Review of `158566b`, verified against the pinned dependencies and Windows documentation on 2026-10-12.

1. **Real permissive-state bug.** `QUNS_APP` means a Store app is running; it does not prove that app is full-screen.
   Only state 5 permits a toast now. The unit test pins all seven numeric states and rejects unknown values.
2. **Real silence-policy gap.** States 1 (not present) and 6 (quiet time) now suppress both channels. State 6 describes
   initial login/upgrade quiet time, not Focus Assist or Do Not Disturb. This follows the owner's review request.
   [Windows enum](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state),
   [notification query](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate).
3. **False duplicate-sound claim.** In the locked versions, `tauri-plugin-notification` 2.6.0 passes no sound name;
   `notify-rust` 4.18.2 calls WinRT `.sound(None)` and macOS `.maybe_sound(None)`. The Windows result is
   `<audio silent="true"/>`; macOS has no notification sound. Linux already sets `SuppressSound(true)`. The chime
   is the only requested sound, independent of the toast. No real toast was sent to verify this.
   [WinRT sound control](https://docs.rs/tauri-winrt-notification/0.8.1/tauri_winrt_notification/struct.Toast.html#method.sound).
4. **Real conservative-detection gap; the proposed multi-monitor guarantee is too broad.** A foreign foreground
   window covering its own monitor now selects sound only. Read failures do too. Both rectangles use the same
   physical coordinates, and the worker's previous DPI context is restored. Tests cover negative monitor origins,
   invisible borders, a visible taskbar and invalid bounds; the C layouts are tested without calling Windows.
   This probes the foreground window, so it cannot promise to detect a background game on another monitor.
   [Foreground window](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow),
   [window bounds](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect),
   [monitor selection](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfromwindow),
   [DPI context](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext).
5. **False R92 completion claim.** R92 explicitly excludes task-finished alerts, regardless of duration. Permission
   questions already use the same attention path: a test parses `AskUserQuestion` and verifies one notice, then no
   notice when work finishes. Unsupported CLI request methods remain a driver capability gap; this change does not
   add question protocols or infer a question from arbitrary answer text.
6. **Real switch semantics issue, corrected by the owner's review request.** The toast switch no longer mutes the
   chime. The decision matrix covers both switches independently, with focus and silent states overriding both.
   Settings explains this in all five languages, using `set.notifyHint` and the new `set.soundHint`.
7. **Known R92 platform gap, not a regression.** macOS/Linux still send a silent toast plus the optional chime while
   Faber is unfocused. Their existing backends expose no foreign-window full-screen probe. X11's EWMH can expose it,
   but there is no X11, Wayland or macOS global-window adapter in Faber today. The agreed detection deferral remains
   explicit in the code, Settings and roadmap; no extra tools, permissions or dependencies were added for it.
   [X11 EWMH](https://specifications.freedesktop.org/wm/latest-single/).

The states and independent switches were reproduced as failing plain unit tests before the fixes. No test opens a
visible window, plays a sound or sends a toast. Real audio, games and macOS/Linux runtime behavior remain untested.
