# The side column is wide enough to read a subagent without cut lines

The right column that shows an open subagent (`.shell.docked`, `ui/styles.css`) is a fixed 352 px. On a 1920 px window
the thread in the middle is at most 660 px wide and the space around it stays empty, while the column cuts the
subagent's brief, its step chips and every command line at the right edge, with no way to read the rest. The owner
asked for it to be fixed (2026-10-17).

Fix it in this order, inside Faber's tokens and the faber-ui checklist:

1. The column's default width follows the window (about 360 to 560 px), taking the empty space beside the thread, never
   the thread's own width.
2. Its brief and text wrap instead of being cut; a command or step line wraps to at most two lines and shows the rest on
   hover or click. No sideways scroll.
3. A control in the column's header opens the subagent in the middle area as a full thread, and back returns to the
   column.

Before starting, check what the "Resizable panes, focus mode, drop" lane (`ui-shell-resize-focus-drop`) landed: if the
column can already be dragged wider and remembers its width, keep that and build the default and the wrapping on it;
if not, a drag handle on the column's left edge that remembers the width (double click resets it) belongs here too.
Look in the product map for the existing column and pane entries first and extend them.

Done when: at 1920 px the column is wider than today and no line in a subagent's brief or steps is cut; at a 720 px
window and in the longest language (de) nothing overflows; a screen check in `tests/ui/` fails on today's fixed width
and passes after; the product map and the design page of the thread say how the column sizes.
