# Chat groups

A group is a named folder in the Chat sidebar, using the project row's controls and folding style. It keeps the
instructions and knowledge files every chat in it receives at its next CLI launch (R318). Projects continue using
their folder's AGENTS.md; Chat groups never write a CLI's configuration files.

## The group's page

Clicking the folder name opens one short, scrollable page, 720 px wide at most, padded at narrower widths. The name
appears once as its heading; the crumb says Chat. Its main action is New chat here. There is no decorative art behind
the instructions and no new motion, timer, preview or hidden file reader.
It occupies the stage's first grid row and column, the same as a conversation, so an overlay running panel shares its
area where the column does not dock (under about 1,030 px). While that panel is modal (or fills the stage), the group page is inert: instructions, file controls
and New chat here cannot take clicks or keyboard focus. Closing the panel restores them.

- Instructions are plain text, at most 8,000 characters, saved as typed through an ordered save queue. Saving keeps
  focus and the editor's current words; a failed save keeps the draft and offers Save instructions again. A short line
  says changes apply when the AI next starts in a chat.
- Files use the composer's browser file reader, picker and drop. Only images and text files are accepted, at most ten
  files and 5 MB in all. Images use JPEG, PNG, GIF or WebP; SVG travels as text, including a previously saved SVG.
  BMP and AVIF are refused with the accepted formats named. Faber copies bytes into its data folder on addition; the list draws names only. Removing
  a file affects the next launch. The page keeps no thumbnails or file bytes alive.
- New chat here waits for saves before starting, so the new chat cannot miss an instruction or file still being saved.
  A sidebar + has the same effect. Outside chats stay in the existing Chat list.

The sidebar's group menu has Rename and Delete. Delete asks once, removes the group's files and keeps its chats,
without their membership. A chat's Move to group opens a native labelled select inline; Escape cancels it. Remove
from group keeps the chat and puts it back in the outside list.

## Checks and cost

`tests/ui/chat-groups.check.ts` checks creation, ordered saving without focus loss, drop, starting a grouped chat,
membership removal, limits and save errors. `testkit/src/screen/chat_groups.rs` exercises the complete creation flow
and checks the page's shared grid area and modal protection at a real 720 px viewport (`chat groups: narrow`),
and measures process memory and CPU with ten copied files totalling 5 MB, then reopens the page after deleting the
original files. Instructions and metadata are loaded with the group list; knowledge bytes are read only when adding
files or launching the CLI. No background process or idle timer is added.
