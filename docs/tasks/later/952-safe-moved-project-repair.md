# Recover moved projects and their linked copies without unsafe Git repair

Area: Recovery after a crash or a moved folder; Separate copies
Files: engine/src/engine.rs, engine-core/src/project/copy.rs, engine/tests/project_paths.rs, docs/product-map.md, docs/design/conversation.md
Depends: none
Size: one focused pull request, about 400 to 800 changed lines

Why: medium finding 4 of the 1.0 data-loss review remains deferred. PR #675 keeps Find the folder's base behavior:
it updates the project folder but does not repair Git links or saved copy roots. The attempted repair was removed
after independent review found three P1 blockers in its preflight and repoint path. Git's repair sweeps sibling
worktrees and truncates link files, so validating paths before calling it cannot make its later writes safe.

Build:

- Repair metadata without truncating through a symbolic link, Windows reparse point or hard link, even if another
  process swaps a sibling `.git` between validation and publication (TOCTOU). Preserve external sentinel bytes and
  unrelated worktrees. Prefer never invoking `git worktree repair`: independently publish the selected checkout's
  `.git` and its administrative `gitdir` backlink with exclusive temporary files, synchronization and atomic rename,
  using safe parent traversal and repository-identity checks that remain valid through publication.
- Repair the selected project when it is itself a linked worktree even when it has no conversation copies. Also
  update loaded and unloaded conversations' saved `WorkCopy.root` and `sub`, including reclaimed copies, while
  preserving uncommitted files, detached or switched HEADs and tracked subfolder selection.
- Make interruption recoverable: an empty administrative `gitdir` left by an interrupted write must not block a
  safe retry. Handle interruption between the two link publications and saved-root/project publication without
  losing edits, inventing repository ownership or leaving an unrecoverable partial state.

Leave out: queue recovery, workspace Save, follow-up conclusions and review voting (the four fixes retained in
PR #675); new recovery controls or automatic background repair.

Done: native-Git integration tests reproduce each blocker before its fix: swap a sibling gitfile to a hard-linked
sentinel at the validation/write boundary and assert unchanged sentinel bytes; move a linked selected project with
zero copies and verify both Git operation and registration after pruning; interrupt before/during link publication,
including an empty `gitdir`, and verify a successful retry with unchanged edits. Cover main and linked projects,
loaded/unloaded/reclaimed copies, subfolders and refused unsafe metadata. Keep process-spawning tests out of unit
tests and hide Git windows on Windows. Update the existing recovery/copy map entries, design page and all five
what's-new languages. Record two fresh security reviews and measured on-demand memory/processor cost; no idle work.
