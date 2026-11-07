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
      Evidence (2026-11-06, candidate 0adba5bb): full hidden first-user walks pt-BR 720x480 and English 1280x800 finished clean: no blocker or high finding (g390n.result.md). Real sign-in, project tools and subagents stay with the owner.

## Real use (the owner's PC, real subscriptions)

- [ ] One working day of the owner's own work through Faber, with a list of every time he went back to the plain CLI
      and why; each reason is fixed or accepted by him.
- [ ] The eight-request sample bench
      Evidence (2026-11-04): NOT run: both comparisons stopped before any request. The harness rejects billing environment variables (now removed from the launcher) and then waits forever for quiet-account samples because the Claude usage reading returns no weekly or session windows. Needs a working zero-token Claude quota source in `bench/account_quiet.rs` (or an owner-approved bypass), and complete routing/auxiliary token accounting for the "no more tokens" condition. (`docs/dev/benchmark.md`) ties or beats plain Claude Code and Codex on quality and
      costs no more tokens.
      Evidence (2026-11-06, candidate 0adba5bb): watcher built and run (branch bench-1-0, 7d0b876d, reviewed): quiet PC reached after 10 min, but all 16 comparison rows were skipped before any model work, spend US$0. Two blockers: (1) the harness cannot enforce a hard US$25 ceiling for plain Codex (engine/examples/bench.rs:288, :397), so it refuses; (2) the zero-token Claude quota reading and the app's own reader both return no verified windows (bench/account_quiet.rs:52-56), so no paired quota readings. Complete token comparison is also unavailable (bench/pure.rs:525, :685). Needs an owner/manager decision to run with subscription-only spend and no hard cap, or a harness change. Details: reports/reforma/helpers/g390b.result.md.
      Evidence (2026-11-06, Codex rows only, candidate 0adba5bb; Claude rows wait for the weekly reset on 2026-11-10 08:00 UTC): all eight requests ran, each side answered and every applicable automatic check passed. Blind verdicts: Faber won 3, plain Codex won 1, 4 ties (reports/reforma/helpers/g390c2.result.md). Faber was slower (560 s against 381 s of side time) and its counted tokens were higher (at least 1.54 million against 0.94 million); this item's "no more tokens" line is therefore NOT met on these rows. Limits to read it with: Faber's token count is partial (it leaves out the routing and helper tokens); the quota readings are account-wide and rounded, so they do not isolate this run from other use (Codex stayed at 47% before and after each request); the judge was Codex judging its own side's work, one blind opinion each, with no swapped second pass.

## Numbers

- [x] Memory with no CLI running under the R125 test limit, measured by CI on the release commit (name the run).
      Evidence (2026-11-04): CI run 41760708119 (commit bf6b8640): the Electron screen test "with no CLI running, Faber stays within its memory budget" passed at 107 MB idle. Re-measure on the release commit.
      Evidence (2026-11-06): CI 42190658930 (candidate 0adba5bb, gate green, 165 screen + 866 Electron + 899 Tauri ui tests): Electron with no CLI running used 114 MB, limit 400.
- [ ] No idle processor use with the window hidden (CI's idle check).
      Evidence (2026-11-04): same run: one core, focused 3.9%, unfocused 0.3%. Re-measure on the release commit.
      Evidence (2026-11-06): CI 42190658930 (candidate 0adba5bb): focused 4.7%, unfocused 0.3% of one core. The test does not measure a hidden window separately; the unfocused state is the nearest proof.
- [ ] A 300-message conversation opens and scrolls without a visible stall (W1's measurement).
      Evidence (2026-11-04): frame gaps measured by requestAnimationFrame in the hidden test build (two runs): longest open 79.4 ms, longest scroll 287.2 ms; the old timer-based check read 489 ms of timer delay, a different metric. A 287 ms scroll gap is a visible hitch, so this stays open until W1 accepts it or it is reduced.
      Evidence (2026-11-06, candidate 0adba5bb, local replay of the CI test builds): Electron longest open 58.3 ms, longest scroll 112.6 ms; Tauri 87.7 ms and 108.4 ms (was 287.2 ms on 2026-11-04). The check has no threshold; the scroll gap is below the earlier hitch but still the owner's call to accept.

## Install and update

- [ ] A clean Windows user installs from the release page, signs in through each CLI's own flow and sends a message.
      Evidence (2026-11-04): run 41800600209: the installed .90 reaches the first-run subscription connection screen (hidden, repeated passes). Real sign-in and a sent message are not covered.
      Evidence (2026-11-06): run 42194923871 clean install of .20 reached the first-run subscription screen. Real sign-in and a sent subscription message are still the owner's day of use.
- [ ] The current Tauri install updates itself to the Electron build without losing a conversation or a setting.
      Evidence (2026-11-04): run 41800600209 (windows-latest, `install-update-self.yml`): the real preview.84 app downloaded, signature-checked and ran the preview.90 installer itself; the new app read the sample conversation and setting through its API and the backup holds both files. Not proved: the automatic relaunch of the new app from .84 (the harness starts it). Harness details: a local HTTPS fixture serves the original signed .90.
      Evidence (2026-11-06): run 42194923871 Tauri .84 to Electron .20 passed (conversation, language and appearance read back, backup holds both). Still not proved: the automatic relaunch of the new app from .84 (the harness starts it).
- [x] 1.0 updates itself to a 1.0.1 built for the test, while a conversation is open, and the conversation goes on.
      Evidence (2026-11-04): FAILED in run 41800600209 with preview.89 to .90: after the real restart button the old Electron processes stay alive, the installed package stays .89 (a shutdown race in `update/src/updater.rs` and `sidecar/src/main.rs`, same code on base). Task 460 fixes it.
      Evidence (2026-11-06): PASSED. Run 42194923871 (harness 961830c6, preview.19 to preview.20, the candidate 0adba5bb): the old app downloaded, signature-checked and ran the .20 installer itself while the conversation was working; old processes ended, the same conversation resumed and answered. Earlier run 42159232370 (.16 to .17) also passed. Details: reports/reforma/helpers/g390k.result.md, g390n.result.md.
- [x] macOS and Linux builds pass CI and ship marked as preview.
      Evidence (2026-11-04): release v0.3.18-preview.90 (Pre-release) ships mac dmg/zip, Linux .deb and AppImage, Windows setup; CI run 41760708119 passed `check (macos-latest, ...)`. Linux: the "Linux engine and runner compile guard" step of the docs job in the same run passed (a compile check, not a full app build); the packages come from the release build.
      Evidence (2026-11-06): v0.3.19-preview.20 (the release candidate's own build) ships Windows setup, macOS and Linux packages with checksums (release run 42193359478); CI 42190658930 green on mac lints and all builds.

## Sign-off

- [ ] Two reviews from fresh sessions of the other vendor on the release diff since the last stable (security included).
      Evidence (2026-11-06): three fresh read-only gpt-6.1-sol reviews, none written by this lane: security of v0.3.17..e4828567 (no blockers; scripts can still bypass credential-name matching, an accepted limit); correctness and data loss of the same range (2 high and 1 medium on failed-write paths, all fixed in #711 and re-reviewed twice, last NO BLOCKERS); and what changed from e4828567 to the candidate 0adba5bb (no blockers). Reviewers ran no code and tested no real Electron; the CI gate did. Claude is this lane's vendor; Codex is the other.
- [ ] The owner's OK, given through the manager, recorded as a decision row.
