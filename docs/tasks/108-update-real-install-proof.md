# A real install proves that Faber comes back after an update, even when the first start fails

Area: Public self-update
Files: scripts/install-update-self.mjs, scripts/install-update-self.test.mjs, .github/workflows/install-update-self.yml, docs/dev/testing.md
Depends: update-checks-the-app-came-back
Size: about 300 changed lines

Why: plan item A2, fourth part ("a test that installs for real and checks the return"). Read on 2026-11-25:
`scripts/install-update-self.mjs` installs for real and checks the return to the conversation for Electron; its Tauri
scenario says itself that it does not prove the automatic reopening. Nothing covers a first start that exits, an old
version still holding the single-instance lock, or a slow installer.

Build:

- Scenarios in the real proof, on a throwaway profile and install folder: the button path reopens a window and logs
  `reopened`; the idle path reopens to the tray; a first start that exits at once (a test switch of the built test app)
  is started again; the old process held past the wait still ends with Faber open.
- The plain-script unit tests for the new parsing of `update.log`.

Leave out: any change in `update/` (the previous task) unless the proof finds a bug, which then goes in this commit.

Done: the scenario of the failing first start fails against the base before task update-checks-the-app-came-back and
passes after; CI's install-update-self job runs the new scenarios. Processes: review by a fresh session of the other
vendor (R146). The owner approved landing on 2026-11-28.
