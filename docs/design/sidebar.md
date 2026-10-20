# Sidebar

The area's conversation lists, pinning, archive and footer. Window widths and the sidebar overlay stay in
[layout.md](layout.md). Search details are in [search.md](search.md).

Pinned and date groups share one labelled section with a semantic heading, styled like the existing quiet labels.
When a refresh replaces a focused conversation row, focus follows its replacement after regrouping, unless the person
has already moved to another control. Pin and Unpin remain usable from the keyboard in both areas.

- At the top: "New conversation" (Ctrl N), then the two areas, Chat and Projects, the latter with a count.
- The sidebar lists only the open area's items (R168): Chat lists the chats and no Projects block, not even an empty
  one; Projects lists the projects with their conversations and the add-folder control, and no chats. Rows keep their DOM
  when the list refreshes, so a refresh moves nothing the person is pointing at.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- Pin and Unpin extend the conversation row's right-click menu in both areas. A short Pinned group uses the existing
  group heading and rows: above Today, This week and Earlier in Chat, and first inside each project's expanded list.
  Pinned rows are flat, newest pin first, and appear only once. Unpin returns the row to its normal order and nesting;
  the empty heading disappears. Search uses the usual list, without pin grouping or a new filter. Deleting still asks
  for confirmation. The existing list motion handles regrouping, instant under reduced or no motion; no timer is added.
- An unpinned conversation an agent started sits right under the one that started it, indented 14 px with a hairline guide at
  its left; one whose starter is not listed stays in its own place.
- Archiving removes the pin; Unarchive does not restore it. Faber's own archiving spares pinned conversations.
  An archived conversation leaves the list. A search still shows it, its title followed by a quiet "archived" in the
  smallest size and the third ink. Opened, once its reply ends, the composer's place holds one card of the composer's
  width: the line "This conversation is archived…" in the second ink and a small Unarchive button at its right.
- The footer is one row, the person's name when they gave one and Settings, then one thin line per connected vendor: its
  shape, a 3 px bar and the used share of its weekly quota window (R177; a vendor that never reported one shows its tightest), in the
  small size. Nothing on the line wraps. The window, the reset, the other windows with their shares (the five-hour one), how old the reading is, the not-yet-counted estimate and the vendor's name are its tooltip. While
  use is still being counted the share reads "≥ 44%" (the real share can only be higher), with the words in the tooltip
  (C010). The bar is amber only when the engine says the quota is nearly used. The line opens Settings > Accounts and limits.
  A vendor with no reading yet, or a weekly reading past its reset, shows "—". The version is not on the sidebar: it is the
  tooltip of the Settings button.
- Search filters titles and project names, then message text, and opens the message ([search.md](search.md#sidebar-search)). The percentage and "stopped" tails
  come with the progress view in Phase 3.
