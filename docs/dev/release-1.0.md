# Release 1.0 checklist

What must be true before Faber's first stable version (1.0). The scope is in [the 1.0 cut](../tasks/README.md); this
page is the gate. Every line names how it is proven; a line is green only with that proof on the release commit.

## Scope done

- [ ] Every task in `docs/tasks/` (the 1.0 queue, at most 15) is landed; `docs/tasks/` holds only `README.md` and
      `later/`.
- [ ] The rework streams that 1.0 counts on have landed: thread (W1), voice and visible intermediaries (W2), honest
      ending (W3), command layer and `faber` CLI (W4), visuals (W6), Electron shell (W7), results and stuck (W8).
- [ ] `docs/product-map.md` "Known incoherences" says "None open now".
      Evidence (2026-11-04): met on base bf6b8640 (product-map.md says "None open now."). Task files still open: 003 (#638 queued) and 390.

## A person's walk (Windows, hidden test build, fake CLIs)

Run `node scripts/first-user.mjs` and the W10 walkthrough specs on the release commit, 720x480 and 1280x800, pt-BR,
English and French. Green when no finding is blocker or high:

- [ ] First run with no AI, with Claude, with Codex; account cards; the health check names a fix for each failure.
- [ ] Chat: send, stream, stop, retry, edit, go back, question card, permission card, attachment.
      Evidence (2026-11-04): 60 clean hidden runs (Tauri and Electron, pt-BR/en/fr, 720x480 and 1280x800; first run no AI/Claude/Codex, prepared `ux_audit --codex` walk, What's new) and the Electron UI suite 812/812; exact coverage per run class in reports/reforma/helpers/g390j.result.md. Findings: high, Electron loses the unsent draft after a forced close; medium, Electron hard-kill recovery lands on home; medium, Catalog heading hidden at compact size (task 455). Not covered: stop/retry/edit/go back, question and permission cards, project tools and subagents, real sign-in.
- [ ] Projects: new conversation, tools, a subagent step, a helper, the changes pane, resume a past conversation.
- [ ] Endings: concluded shows the AI's summary; ended without a conclusion, interrupted (after a forced close) and
      stuck each show their true state and the way on.
- [ ] Quota for both AIs; Settings by keyboard alone; Catalog; Ctrl+K; What's new.
      Evidence (2026-11-04): first-user pass (hidden Tauri test build, fake CLIs, Haiku reviewer) ran at 720x480 in pt-BR, English and French and at 1280x800 in the same three: no critical, high or medium finding confirmed against the screenshots; two low (French experience step shows no selected cue, French Projects text omits the local folder; French "Nouvelle conver..." button label truncated at 720x480). Reports in docs/history/research/first-user-2026-11-04-*.md. NOT yet done: the `ux_audit --codex` walkthrough on the release commit, account cards and health-check fixes, and Electron-shell first-user runs. Repeat on the release commit.

## Real use (the owner's PC, real subscriptions)

- [ ] One working day of the owner's own work through Faber, with a list of every time he went back to the plain CLI
      and why; each reason is fixed or accepted by him.
- [ ] The eight-request sample bench
      Evidence (2026-11-04): NOT run: both comparisons stopped before any request. The harness rejects billing environment variables (now removed from the launcher) and then waits forever for quiet-account samples because the Claude usage reading returns no weekly or session windows. Needs a working zero-token Claude quota source in `bench/account_quiet.rs` (or an owner-approved bypass), and complete routing/auxiliary token accounting for the "no more tokens" condition. (`docs/dev/benchmark.md`) ties or beats plain Claude Code and Codex on quality and
      costs no more tokens.

## Numbers

- [ ] Memory with no CLI running under the R125 test limit, measured by CI on the release commit (name the run).
      Evidence (2026-11-04): CI run 41760708119 (commit bf6b8640): the Electron screen test "with no CLI running, Faber stays within its memory budget" passed at 107 MB idle. Re-measure on the release commit.
- [ ] No idle processor use with the window hidden (CI's idle check).
      Evidence (2026-11-04): same run: one core, focused 3.9%, unfocused 0.3%. Re-measure on the release commit.
- [ ] A 300-message conversation opens and scrolls without a visible stall (W1's measurement).
      Evidence (2026-11-04): frame gaps measured by requestAnimationFrame in the hidden test build (two runs): longest open 79.4 ms, longest scroll 287.2 ms; the old timer-based check read 489 ms of timer delay, a different metric. A 287 ms scroll gap is a visible hitch, so this stays open until W1 accepts it or it is reduced.

## Install and update

- [ ] A clean Windows user installs from the release page, signs in through each CLI's own flow and sends a message.
      Evidence (2026-11-04): run 41800600209: the installed .90 reaches the first-run subscription connection screen (hidden, repeated passes). Real sign-in and a sent message are not covered.
- [ ] The current Tauri install updates itself to the Electron build without losing a conversation or a setting.
      Evidence (2026-11-04): run 41800600209 (windows-latest, `install-update-self.yml`): the real preview.84 app downloaded, signature-checked and ran the preview.90 installer itself; the new app read the sample conversation and setting through its API and the backup holds both files. Not proved: the automatic relaunch of the new app from .84 (the harness starts it). Harness details: a local HTTPS fixture serves the original signed .90.
- [ ] 1.0 updates itself to a 1.0.1 built for the test, while a conversation is open, and the conversation goes on.
      Evidence (2026-11-04): FAILED in run 41800600209 with preview.89 to .90: after the real restart button the old Electron processes stay alive, the installed package stays .89 (a shutdown race in `update/src/updater.rs` and `sidecar/src/main.rs`, same code on base). Task 460 fixes it.
- [ ] macOS and Linux builds pass CI and ship marked as preview.
      Evidence (2026-11-04): release v0.3.18-preview.90 (Pre-release) ships mac dmg/zip, Linux .deb and AppImage, Windows setup; CI run 41760708119 passed `check (macos-latest, ...)`. Linux: the "Linux engine and runner compile guard" step of the docs job in the same run passed (a compile check, not a full app build); the packages come from the release build.

## Sign-off

- [ ] Two reviews from fresh sessions of the other vendor on the release diff since the last stable (security included).
- [ ] The owner's OK, given through the manager, recorded as a decision row.
