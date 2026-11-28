# Branches and versions

How a conversation takes another way at a message (parity T7). Each way is a conversation of its own; nothing else is
stored.

- An answer's right-click menu has "New conversation from here" after Copy and "Retry with {model}". It opens a new conversation
  holding every message up to that answer, with nothing sent; the composer keeps its choice, and the next message
  carries the earlier ones to the CLI as any branch does.
- Under a message of the person's that has other versions (an edit, a retry, or a branch that went another way there),
  a quiet row at the bubble's right edge: a left arrow, "2/3" in the faint data style, a right arrow. Each arrow is a
  small icon button from `ui/Glyphs.tsx` with a tooltip and an accessible name, dimmed at either end; the row's name
  says "Version 2 of 3". An arrow opens that version's conversation at the same message, with the search outline, and
  the left and right arrow keys walk them while focus is on the row. The row has no motion of its own.
- The versions are found when a conversation opens; an archived conversation shows as no version.
