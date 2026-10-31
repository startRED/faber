# Scenes

Scenes are small inline line drawings that help a person recognize a screen at a glance. Their words carry the same meaning.

| Scene | Where it shows |
| :--- | :--- |
| `noAi` | Drawn, not shown yet: No AI connected, on the hairline, once that screen's lane lands |
| `openProject` | Open a project, on the hairline |
| `emptyProject` | Drawn, not shown yet: a project with nothing in it, on the hairline |
| `noResults` | Catalog search has no matches, on the hairline |
| `catalogEmpty` | Catalog has nothing listed, on the hairline |
| `accounts` | Drawn, not shown yet: the first run's accounts step, above the heading |

Scenes are static SVG paths using `currentColor` in the third ink. They contain no text and are hidden from screen readers.
They never sit behind text. The accounts scene is hidden below 800 px height or 900 px width, leaving room for the choices.
The loading line keeps Curioso alone.
