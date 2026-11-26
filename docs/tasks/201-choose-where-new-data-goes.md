# Choose the drive and folder for new copies and heavy tools

Area: Settings screen
Files: host/src/lib.rs, electron/main.ts, engine-store/src/, ui/Settings.tsx, ui/locales/, docs/design/settings.md, docs/product-map.md
Depends: storage-in-settings
Size: about 600 changed lines

Why: plan item C2, first part. Read on 2026-11-25: `host/src/lib.rs` and `electron/main.ts` read only `FABER_DATA_DIR` at start; `installed_data_dir` always uses the default; `engine-store/src/workspace/relocation.rs` moves only the Chat's private workspace. The choice must live outside the data folder itself so both the shell and the engine find it before opening anything.

Build:

- A stored choice outside the data folder (beside the install or in the user profile), read by the shell and the host before start.
- New copies and heavy tools go to the chosen folder; what exists stays where it is (the next task moves it).
- On first run the question appears only when the default drive is short of space (first-run rule: no step a good default covers).

Leave out: moving existing data (next task).

Done: tests that a chosen folder is used for a new copy and a new tool download and that an unreachable choice falls back with a notice; a screen check of the setting. Stored data and paths: review by a fresh session of the other vendor (R146).
