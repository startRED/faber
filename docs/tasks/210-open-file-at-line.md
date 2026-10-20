# Open a file at a line from the reply (parity P2)

Row P2 of docs/product/parity.md: a file path in a reply (`src/app.ts:42`, a path the agent wrote or changed) opens in
place, like the file cards, at that line, read only, with the line marked; inside the conversation's folder only, the
same safe reads as the changes view. Extends file cards; "Open in editor" goes through the system's own handler.

Done when: a unit test of the path and line parsing (spaces, Windows paths, outside the folder refused); a screen check
with the fake CLI opens a file at a line; the product map and docs/design are updated; a what's-new entry in five
languages.
