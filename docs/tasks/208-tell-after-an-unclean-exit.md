# After a crash or a forced close, say so once and offer the log folder

Area: Shell
Files: engine/src/crash.rs, engine/src/engine.rs, ui/App.tsx, ui/api.ts, ui/locales/, tests/ui/update.check.ts, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 200 changed lines

Why: `engine/src/crash.rs` already writes a crash line and an unclean-exit mark (`start_run`, `end_run`, `since`) without anything of the person, but the person is never told. After a crash the window just opens, a running reply is gone, and the person cannot tell why or what to send. First check where `since` is read now and keep that.

Build: when the start finds the marker of a run that did not end cleanly, the engine exposes one fact (when, and whether a panic line exists) and the screen shows one calm, dismissible note on the home: "Faber closed unexpectedly last time. Your conversations are safe." with "Open the log folder" (the existing way the app opens a folder) and "Dismiss". Shown once per crash, no text of the panic on screen, and the note never appears after a normal close or an update restart (check how an update relaunch ends a run).

Leave out: sending the log anywhere (telemetry stays off) and the Electron shell's own crash handling.

Done: an engine test that fails today: an unclean marker gives the fact once and a clean end, or an update restart, gives none; a screen check shows and dismisses the note. Extends the map's crash row and "First run" home. What's-new entry in every language. No cost.
