# The project as a list of rooms with a picture, lit while the AI works there

Area: Projects
Files: engine-core/src/map.rs, engine/src/engine/, ui/ProjectMap.tsx, ui/locales/, tests/ui/, docs/design/projects.md, docs/product-map.md
Depends: before-and-after-pictures
Size: about 700 changed lines

Why: plan item B2, small version: a side panel (the work area shows only the work), not a graph. `engine-core/src/map.rs` already indexes screens, settings, scripts and exports for the AI.

Build:

- Rooms from the existing index (screens and major parts), each with its latest picture and plain recent changes.
- The room the current answer touches lights up; clicking offers go back and ask about this part.

Leave out: a graph drawing.

Done: tests of the room list from a fixture project; a screen check of lighting and the actions. Walked as a newcomer and a heavy user.
