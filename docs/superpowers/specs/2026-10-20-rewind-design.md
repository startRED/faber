# Rewind before a message

## Decisions

Erick answered on 2026-10-20.

- Erick chose "Segue e avisa (Recomendado)" when a pre-change copy fails, including oversize or locked files.
- Erick allowed saved-message continuation after CLI or AI changes: "Pode, com aviso (Recomendado)".
- Erick chose "1 GB, 30 dias (Recomendado)" for history.

- AI (lead session, technical): Restore kills nothing. Overlapping conversations must be idle, including tracked helpers, commands and checks; unknown status refuses restore (`engine/src/engine/verifying.rs:191`, `engine/src/engine.rs:2437`). The confirm names running work and waits for the person to press Stop (`docs/product-map.md:74`).
- AI (lead session, technical): Nothing Faber keeps stays in the person's folder. Before replacement or removal, save current bytes through validated handles in history within 1 GiB; otherwise skip that file and say so. The safety bar is the one `git checkout` gives: a program outside Faber that keeps an old handle open and writes after the restore is a known limit, named in the confirm.
- AI (lead session, technical): Create the history folder owner-only, with Unix mode 0700. Create temporary files owner-only before writing: use an explicit current-user-only security descriptor on Windows and mode 0600 on Unix. ReplaceFileW must preserve the replaced file's security descriptor; On Unix the temporary stays 0600 while written; then fchown sets the replaced file's group, and only after that fchmod sets its mode, before the rename. If the group cannot be set, the file is not restored. Record recreation protection at capture: the Unix group and mode, or on Windows the file's own security descriptor (GetSecurityInfo through the held handle, stored owner-only). Recreation applies it to the owner-only temporary file before the rename. If it was not recorded or cannot be set, the file is not recreated and the result says so; never fall back to the folder's inherited access. Test: a privately protected file in a shared folder comes back private.
- AI (lead session, technical): Use the three restore cases below (`docs/tasks/090-rewind.md:6`).
- AI (lead session, technical): In the first version, Stop also interrupts the conversation's tracked helpers and background commands with no active turn (`engine/src/engine.rs:682`, `engine/src/codex.rs:1302-1311`).

## Capture

Use private GIT_DIR `<data>/rewind/<folder-id>/`. Disable hooks and filters; never use alternates or alter user indexes, refs or stash (`engine/src/engine.rs:1211`, `engine/src/project/git.rs:1`).

Capture tracked and non-ignored untracked files; non-git folders follow .gitignore. Silently exclude git internals, ignored files, nested repos, Faber logs, special files, links and junctions. Save after showing the message, before CLI input or startup; retry unstable reads once (`engine/src/engine.rs:2322`). Include commands (`docs/tasks/090-rewind.md:4`).

Record folder, parent, message and turn ids, pre/postimages or absence, recreation protection, coverage, writers and cursor. Renames are deletion and creation. Exclude commits, custom ACLs and remote effects (`docs/tasks/090-rewind.md:6`).

Deny secret names and relocated credentials (`engine/src/permissions.rs:520`, `engine/src/permissions.rs:344`). Before every read, refuse multiple hard links (Windows GetFileInformationByHandle link count; Unix fstat st_nlink). Test fake credentials. Use validated handles; real_path cannot protect a later open (`engine/src/project/lease.rs:31`).

AI: On Windows, walk from a trusted root with NtCreateFile and FILE_OPEN_REPARSE_POINT. Hold parents, reject reparse points and omit FILE_SHARE_DELETE while reading. On Unix, walk with openat, O_DIRECTORY and O_NOFOLLOW; open leaves with O_NOFOLLOW | O_NONBLOCK and reject nonregular handles with fstat (`engine/src/map.rs:198-215`). Reject traversal; validate root identity, type and credential exclusions. Never reopen checked names. Unsafe code stays in winproc/ (`Cargo.toml:12`, `winproc/Cargo.toml:9`).

Cap each file at 8 MiB and each baseline at 64 MiB and 20,000 paths. Limits, locks, errors, missing git or storage failure mark incomplete or unavailable history. Say "some files cannot go back" and continue unattended (`docs/tasks/090-rewind.md:7`). Path rules miss embedded secrets (`engine/src/permissions.rs:520`).

## Sealing

Include late helpers and background commands in the idle gate and extended Stop (`engine/src/codex.rs:1182`, `engine/src/claude/background.rs:27`).

AI: Track writers before writing. Keep history open while they may run or are unknown. Drain events and durably scan changes before sealing; leases cannot prove completion (`engine/src/engine/helpers.rs:64`). Capture later turns from that state.

AI: History never gates turns, queues, automatic work or overlapping folders. Preserve Queue and steer (`docs/product-map.md:23`, `engine/src/codex.rs:1197`). Crashes leave postimages unknown.

AI: Lock overlapping conversations' turn admission through idle checks, restore and recovery, blocking queued and automatic turns. Keep going stays off after redo (`engine/src/engine/keep_going.rs:358`).

## Disk

Cap history at 1 GiB total, 256 MiB and 100 points per folder, and 30 days. Delete oldest eligible points. Count all blobs, metadata, redo, staging and journals by uncompressed bytes and physical allocation; reserve 64 MiB for recovery inside the cap (`docs/tasks/090-rewind.md:7`, `docs/decisions.md:112`).

Clean at startup, sealing and capture admission; pin baselines and journals. Cleanup failure, full pins or under 1 GiB free skips capture as unavailable; work continues. Never exceed caps or delete recovery data; delete only validated owned paths (`docs/decisions.md:112`).

Keep retired and archived copies' history; recheck recreated copies. Deletion needs copy-discard consent. Never expire active messages (`engine/src/engine/copies.rs:73-119`).

## Restore and redo

Files only restores net changes; Conversation only keeps earlier messages; Both does both. Keep later messages, bindings, plan and context for redo. Grants, spend and publication never roll back. Missing history disables only its file range. Preview coverage; compare bytes, type and absence with postimages. Default to "Keep my file"; require "Restore saved version" per conflict or unknown author. Never delete unlisted files or claim partial restore is complete (`docs/tasks/090-rewind.md:6`).

AI: Reserve space per file and durably journal files, binding and cursor. Preserve bytes through validated handles; recompare identity and bytes for redo conflicts. Use protected Faber-prefixed temporaries beside destinations; always clean them. Replace existing files with Windows ReplaceFileW without backup or Unix rename; preserve protection. Recreate deleted files by creating missing parents through validated handles, applying captured protection and using a no-replace rename. Remove files created after the chosen point only after saving their bytes durably in history for redo. If identity, bytes or absence change before mutation, stop that file and keep both versions. Restore is not atomic across files; failures preserve every version and report partial recovery. Redo repeats checks without rerunning commands (`docs/tasks/090-rewind.md:6`).

M3 found hidden flags in Claude 2.1.302's binary help strings: `--resume-session-at <entry uuid>` purportedly retains that prefix, and `--resume-drops-turn <prompt uuid>` purportedly rejects other cuts. Semantics remain unverified. Prove them and `--resume <id> --fork-session` with a live probe and fixture before native continuation. Persist each turn's last entry uuid. On refusal or missing flags, use saved messages and disclose "The AI will continue from the saved messages". Transfer omits image bytes; include attachment references without replaying tools (`engine/src/engine.rs:241`).

M1's Codex 0.162.1 schema probe (2026-10-20) suggests forking at the last retained completed turn id (`engine/src/codex.rs:1175`). Missing cursors or CLI or AI changes require disclosed saved-message continuation with redo. Invalidate later bindings. Files only keeps messages but starts fresh with a restore summary. Never edit CLI session files (`engine/src/model.rs:658`).

## Screen

Extend Copy/Edit with "Go back before this message" and Redo (`ui/Thread.tsx:561`, `docs/tasks/090-rewind.md:6`). Offer Files only, Conversation only and Files and conversation, defaulting to both. Show counts, conflicts, coverage and redo expiry. Offer extended Stop after the turn ends, keep its confirmation and recheck idle (`docs/product-map.md:74`). State the old-handle limit.

Show waiting, recovery and missing history; reject repeats. Support offline undo, locale keys, keyboard access, focus return, 720 px and reduced motion. Cards C001/C006/C008/C009/C011 fit (`docs/product-rules.md:66-71`).

## First version

Defer rich diffs, packing and configurable limits. Native resume needs live proof; ship saved messages first (M1, M3).

## Cost and tests

M2 captured 1,000 random 16 KiB files in Node: 770.8 ms wall, 624 ms CPU; eight changes took 174.3 ms wall, 172 ms CPU. Its 16,539,513 bytes exclude git, fsync, manifests, restore and UI; peak RAM is unmeasured.

Target one worker, 1 MiB buffers, 16 MiB added RAM and no idle work. Measure capture, restore, cleanup and peak RAM; enforce 400 MB (`docs/decisions.md:131`).

Test junction and Unix symlink swaps to fake credentials; assert zero outside reads. Swap a regular leaf for a writerless FIFO before open; assert bounded completion without reading (`engine/src/map.rs:198-215`). Replay late helper writes, command deletions and checks; history stays open while turns and automatic work proceed (`engine/tests/codex_session.rs:438`, `docs/product-map.md:23`).

Use barriers to mutate bytes, identity or absence before each operation; stop that file and keep both versions. Test temporary cleanup on failure and crash recovery, budget and Windows replace refusals. Tracked overlapping work or unknown status blocks restore. Restore of A kills nothing; B and descendants keep running on their shared server (`engine/src/engine.rs:2437`, `engine/src/codex.rs:1182`).

Test owner-only history and temporaries before writes in a shared folder, replacement and recreation protection, and protection failure. Test replacement, missing-parent recreation, no-replace races, removal and redo. With no active turn, Stop must interrupt tracked helpers and commands; restore waits for confirmed idle (`engine/src/engine.rs:682`, `engine/src/codex.rs:1311`).

Test exclusions, unattended failures, dirty, staged, untracked and binary files, conflicts, redo, restart and disk failure. Forbid credential reads, user git changes and redo spending. Test accessibility and screen states (`docs/tasks/090-rewind.md:11`, `tests/ui/contextmenu.check.ts:1`).
