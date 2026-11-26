# After an update, Faber checks that the new version opened and tries again when it did not

Area: Public self-update
Files: update/src/handoff.rs, update/src/updater.rs, update/src/flow.rs, docs/product-map.md
Depends: none
Size: about 450 changed lines

Why: plan item A2. Between 1 and 5 November the owner's Faber stayed closed for up to 66 hours after updates
(`update.log`: `install-failed installer exit code 1`, `relaunch-failed … (os error 193)`, `install-wait Faber was still
closing`). Read in the code on 2026-11-25:

- `update/src/handoff.rs` `finish` starts the executable and returns 0 once the process is created, without checking it
  lives; success logs only `installed in N ms`; nothing ever logs that Faber came back.
- `electron/main.ts`: `if (!app.requestSingleInstanceLock()) app.exit(0)`. A new version started while the old one
  still holds the lock exits silently with 0.
- `update/src/updater.rs` `Live::relaunch`: when the helper copy cannot start, it logs and calls `self.end()`, so the app
  ends without reopening.
- After `INSTALL_WAIT` (300 s) it logs `install-slow` and starts the app while the installer may still run; a failed
  process query also reads as "no result".

Build (inside `update/` only):

- Come back for real: after starting the app, the helper waits a bounded time for proof that it is up (the new host line
  in `startup.log`, which the host writes once the engine answers, or the process still alive after the single-instance
  check), logs `reopened in N ms` and exits; with no proof it waits for the old process to be gone and starts it once
  more, then logs `reopen-failed` with the reason.
- When the helper copy cannot start, `relaunch` starts the installed app directly instead of ending with nothing open.
- Unknown installer state after the wait is told apart from "finished", and Faber is not started over files still being
  replaced.
- The idle install already reopens to the tray and the button to a window (`Reopen::Tray`, `Reopen::Window`): that is
  already "the way it was", keep it. No window and no focus from an update (C004, R551).

Leave out: the real install proof (task update-real-install-proof); any screen.

Done: unit tests in `update/src/` with a fake launcher and fake clock (no real process, as the AGENTS rule says) for:
the app exits at once and is started again; it never comes up and `reopen-failed` is logged; the helper copy fails and
the app is started directly; `reopened` is logged on success. Each fails on today's code. Extends the product-map row
`Public self-update`. Processes: review by a fresh session of the other vendor (R146). Landing waits for the owner's yes
(night plan of 2026-11-25: a broken updater reaches everyone and cannot be fixed by another update): ship it proven and
reviewed on its branch, and leave the yes to him.
