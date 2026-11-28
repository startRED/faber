# Update proof: a real install that updates itself

`.github/workflows/install-update-self.yml` installs Faber for real on a throwaway GitHub Windows runner, on a private
desktop nobody sees, with a fresh profile and install folder (`scripts/install-update-self.mjs`). The feed is served from
the runner itself, under github.com's name with a certificate trusted only there, and hands out the original signed
release, so the signature check stays on. The `tauri`, `clean` and `electron` jobs install published releases; they prove
moving data and a running conversation across an update, but only with the hand-off code of the release they start from.

The reopen jobs build Faber from the code under test, at a version just below the newest release (`builtVersion`), and
let it update itself to that release. The helper that hands off the install and opens Faber again is then this code's
`update/`. Each scenario passes only when Faber runs again, the way it was opened, and `update.log` says `installed`
followed by `reopened` with no `reopen-failed` (`reopenEvidence`):

- `reopen-window`: the person presses "Update ready, restart"; the window comes back.
- `reopen-tray`: a Faber started with `--autostart` installs once idle and comes back in the tray.
- `first-start-exits`: while the install runs, the proof holds Chromium's single-instance window for the profile, so
  the first start finds Faber "already running" and exits at once; the helper must start it again (`reopen-retry`).

An old Faber still running past the helper's wait has no scenario: it ends about 0.3 s into the hand-off, before a proof
can freeze it (runs 46103155141 and 46104395985), so the wait-then-end path is unproven on a real install and covered only
by `update/`'s fake-clock tests `an_old_faber_that_does_not_close_is_ended_before_the_installer_runs` and
`relaunch_waits_then_ends_a_hung_shell_before_launching` (`update/src/handoff.rs`).

The dispatch input `update_code` builds with the `update/` folder of another commit, so a scenario can be shown to fail
without a fix: `first-start-exits` fails with the code before the reopen check (`ecd78493b^`), since that helper started
Faber once and never looked again. The plain-script tests (`scripts/install-update-self.test.mjs`) cover the reading of
`update.log` and never start a process.
