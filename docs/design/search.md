# Search

How the command box (Ctrl+K) and find in a conversation (Ctrl+F) look and move. The rest of the shell is in
[layout.md](layout.md).

## Search and commands, and find

- **Ctrl+K** opens one box over everything, centred sideways and set in the upper third so it does not jump as the results
  change. It scales from 0.97 and fades in with `--dur-ui` and leaves faster with `--dur-exit`; behind it the screen dims
  with a veil of its own colour, never a blur (only the sidebar scrim blurs). It is a modal dialog: focus goes to its box,
  Tab stays inside (`ui/focus.ts`), Esc or a click outside closes it and gives focus back, and the shortcuts of the screen
  underneath wait. The box is the shared search field (`.field.search`), taller (`--h-lg`), with no ring of its own; rows use
  `--h-md`, the chosen one on `--s3`, and the project or area of a conversation sits quiet at the right. Empty, it lists
  "Recent" (five conversations) then "Actions"; typing gives one list ranked across everything, eight rows at most.
  Moving the choice by arrow changes its colour only (`--dur-fast`), no travel. It exists in the page only while open.
- **Ctrl+F** in an open conversation slides a bar down from the top right of the thread (12 px and a fade, `--dur-ui`; it
  leaves with `--dur-exit`) with the shared search field, "3 of 12", previous, next and close. Matches are painted with the
  CSS Custom Highlight API: all in the accent at 42 %, the current one in the full accent, so no message is rewritten and
  nothing is wrapped. Enter and Shift+Enter move with a smooth scroll that centres the match unless it is already in plain
  view (instant in reduced and no motion); Esc closes the bar before the sidebar or the running column. When the running
  column covers the thread, the bar slides left of it. Both are instant in reduced and no motion.
