# Release 1.0 checklist

What must be true before Faber's first stable version (1.0). The scope is in [the 1.0 cut](../tasks/README.md); this
page is the gate. Every line names how it is proven; a line is green only with that proof on the release commit.

## Scope done

- [ ] Every task in `docs/tasks/` (the 1.0 queue, at most 15) is landed; `docs/tasks/` holds only `README.md` and
      `later/`.
- [ ] The rework streams that 1.0 counts on have landed: thread (W1), voice and visible intermediaries (W2), honest
      ending (W3), command layer and `faber` CLI (W4), visuals (W6), Electron shell (W7), results and stuck (W8).
- [ ] `docs/product-map.md` "Known incoherences" says "None open now".

## A person's walk (Windows, hidden test build, fake CLIs)

Run `node scripts/first-user.mjs` and the W10 walkthrough specs on the release commit, 720x480 and 1280x800, pt-BR,
English and French. Green when no finding is blocker or high:

- [ ] First run with no AI, with Claude, with Codex; account cards; the health check names a fix for each failure.
- [ ] Chat: send, stream, stop, retry, edit, go back, question card, permission card, attachment.
- [ ] Projects: new conversation, tools, a subagent step, a helper, the changes pane, resume a past conversation.
- [ ] Endings: concluded shows the AI's summary; ended without a conclusion, interrupted (after a forced close) and
      stuck each show their true state and the way on.
- [ ] Quota for both AIs; Settings by keyboard alone; Catalog; Ctrl+K; What's new.

## Real use (the owner's PC, real subscriptions)

- [ ] One working day of the owner's own work through Faber, with a list of every time he went back to the plain CLI
      and why; each reason is fixed or accepted by him.
- [ ] The eight-request sample bench (`docs/dev/benchmark.md`) ties or beats plain Claude Code and Codex on quality and
      costs no more tokens.

## Numbers

- [ ] Memory with no CLI running under the R125 test limit, measured by CI on the release commit (name the run).
- [ ] No idle processor use with the window hidden (CI's idle check).
- [ ] A 300-message conversation opens and scrolls without a visible stall (W1's measurement).

## Install and update

- [ ] A clean Windows user installs from the release page, signs in through each CLI's own flow and sends a message.
- [ ] The current Tauri install updates itself to the Electron build without losing a conversation or a setting.
- [ ] 1.0 updates itself to a 1.0.1 built for the test, while a conversation is open, and the conversation goes on.
- [ ] macOS and Linux builds pass CI and ship marked as preview.

## Sign-off

- [ ] Two reviews from fresh sessions of the other vendor on the release diff since the last stable (security included).
- [ ] The owner's OK, given through the manager, recorded as a decision row.
