# Release 1.0 gate

Area: none
Files: testkit/examples/ux_audit.rs, walkthrough specs and fixtures, docs/dev/release-1.0.md
Depends: git-missing-says-so, install-without-internet-says-so, unreadable-conversation-is-listed-not-lost, project-folder-names-with-accents-and-spaces, first-run-and-setup-rescue, quota-you-can-trust, recovery-after-a-crash-or-a-moved-folder, cli-processes-never-hang-or-leak, faber-folders-safe-and-small, settings-plain-and-by-keyboard, model-menu-and-conversation-titles, long-conversations-open-fast, newcomer-polish-pass
Size: two pull requests: first fake Codex walkthrough support, then the release gate evidence

Why: The walkthrough exercised only one AI because `ux_audit` cannot start the fake Codex. The first stable release needs evidence on the release commit for both AIs, installation, updates, real use and the owner's sign-off.

Build: First teach `testkit/examples/ux_audit.rs` to start the fake Codex using the existing sandbox and fixtures, and prove the walkthrough exercises both AIs without reading real credentials. Then run every item in `docs/dev/release-1.0.md` on the release commit, including both-AI walkthrough, first-user pass, sample bench, memory and CPU, clean install, update paths, previews, fresh reviews and owner approval through the manager. Record evidence and resolve or explicitly obtain the owner's acceptance where the checklist allows it. This is the last task.

Leave out: publishing a release before the checklist and owner approval are complete.

Done: A check fails without fake Codex startup and proves both fake AIs are exercised. Every checklist item is green with its named proof on the release commit, including the owner's recorded decision; unresolved items keep the gate open. Report the test-only cost and obtain the checklist's two fresh reviews.
