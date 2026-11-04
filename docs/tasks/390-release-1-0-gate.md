# Release 1.0 gate: the evidence

Area: none
Files: docs/dev/release-1.0.md, walkthrough specs and fixtures
Depends: none
Size: one pull request

Why: The fake Codex now starts in the walkthrough (`ux_audit --codex`). The first stable release still needs evidence on the release commit for both AIs, installation, updates, real use and the owner's sign-off. This is the last task and waits for the owner.

Build: Run every item in `docs/dev/release-1.0.md` on the release commit: the both-AI walkthrough, the first-user pass, the sample bench, memory and CPU, clean install, update paths, previews, the two fresh reviews and the owner's approval through the manager. Record each proof beside its item and resolve it, or obtain the owner's explicit acceptance where the checklist allows it.

Leave out: publishing a release before every item is green and the owner has approved.

Done: Every checklist item is green with its named proof on the release commit, including the owner's recorded decision; an unresolved item keeps the gate open.
