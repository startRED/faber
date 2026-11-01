# Recovery after a crash or a moved folder

Area: Projects, Shell, recovery
Files: engine/src/project.rs, engine/src/recents.rs, engine/src/engine/pick_up.rs, engine/src/crash.rs, ui/Projects.tsx, ui/Recents.tsx, ui/App.tsx, ui/locales/, engine/tests/, tests/ui/, testkit/src/screen/killed.rs, testkit/src/screen/mod.rs, testkit/src/desktop.rs, testkit/screen-costs.json, docs/dev/testing.md, docs/product-map.md
Depends: none
Size: two pull requests: first missing folders and crash note, then the hard-kill screen proof

Why: A moved folder or disconnected drive leaves a project present but its actions fail. After a crash the person is not told what happened, and no screen test proves the saved answer and native session survive a hard kill together.

Build: Check current missing-path behavior. On listing, report present, missing or disconnected drive using one metadata check per project and no content read; refresh on window focus. Show a quiet explanation and Find the folder (native picker, retains conversations) and Remove from Faber (forget only, never delete files). Refuse a send with a key before starting a CLI. On an unclean marker expose when and whether a panic line exists, preserving existing `since` use. Show one calm dismissible home note and Open the log folder, without panic text; never after a clean close or update restart. Add a hard-kill screen scenario in its own file for both fake CLIs: stream, kill the app, reopen the same data and send again on the same native session.

Leave out: disk search, moving files, sending logs and shell-specific crash handling.

Done: Tests fail today for missing-path state, refused send with no process, repointing with conversations intact, and the row and both buttons. A crash test proves the fact appears once and not after clean close or update restart; a screen check shows and dismisses it. The hard-kill scenario rejects a missing or still-typing answer, lost partial text, a leftover CLI or a new native session, for both AIs; memory stays under 400 MB and scenario cost is listed. Update map, design, testing docs and what's-new in every language; get fresh and coherence reviews. Report one metadata call per project on refresh and screen cost.
