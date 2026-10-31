# A project whose folder moved, was renamed or sits on a disconnected drive

Area: Projects
Files: engine/src/project.rs, engine/src/recents.rs, engine/src/engine/pick_up.rs, ui/Projects.tsx, ui/Recents.tsx, ui/locales/, tests/ui/projects.check.ts, engine/tests/project.rs, docs/product-map.md
Depends: none
Size: about 300 changed lines

Why: on Windows a project often lives on a USB or second drive, or the person renames the folder in Explorer. Faber then shows a project whose folder does not exist and every action fails with a raw error. First check what each screen does now with a missing path.

Build: the engine reports a project's folder state (present, missing, drive not connected) when the project list is read, with one cheap metadata check per project and no read of content. A missing project shows quietly in the list with its name and one line ("The folder is not there. If the drive is disconnected, plug it in."), with two buttons: "Find the folder" (the native folder picker, which updates the stored path and keeps the conversations) and "Remove from Faber" (forgets it, never deletes files). The state refreshes when the window regains focus. Sending in a missing project is refused with a key before any CLI starts.

Leave out: searching the disk for the folder and moving files.

Done: engine tests that fail today: a project with a missing path is marked, a send is refused with the key and starts nothing, a re-pointed path restores it and its conversations; a screen check shows the row and both buttons. Extends "Projects". What's-new entry in every language. Cost: one metadata call per project on a refresh.
