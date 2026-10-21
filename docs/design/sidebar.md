# Sidebar

The area's conversation lists, pinning, export, archive and footer. Window widths and the sidebar overlay stay in
[layout.md](layout.md). Search details are in [search.md](search.md).

Pinned and date groups share one labelled section with a semantic heading, styled like the existing quiet labels.
Chat group rows are reconciled synchronously by id during a state refresh, including after a rename changes the group.
The row keeps its rename draft and focus; Enter keeps focus on its button after the save refresh completes
(`tests/ui/chat-groups.check.ts`, `chat groups: renaming keeps its draft and focus through enter`).
Its Pinned and date sections are keyed by their names, so a chat's rename draft, selection and focus also survive a list
refresh or the minute clock tick (`tests/ui/chat-groups.check.ts`, `chat groups: chat rename`).
Escape and Enter in the group's rename box return focus to its row, as they do for a conversation.
When a refreshed group list no longer contains the open group, its selection is cleared in the screen and browser
storage, so New conversation and Ctrl+N start an outside chat. Moving a chat with the keyboard restores focus to its
row once the destination group has it; focusing another control meanwhile cancels that restoration, even after blur
(`tests/ui/chat-groups.check.ts`, `chat groups: deleting` and `chat groups: keyboard Move to group`).
When a refresh replaces a focused conversation row, focus follows its replacement after regrouping, unless the person
has already moved to another control, even if they then leave that control. Moving an open Chat row by keyboard waits
for the destination row in the later project snapshot; its area can change before that snapshot arrives. Pin and Unpin
remain usable from the keyboard in both areas.

- At the top: "New conversation" (Ctrl N), then Chat, Projects (with a count) and Catalog. Catalog keeps New conversation, navigation and footer in place, with no conversation search/list. New conversation returns to the last work area. Switching back opens the chosen work area.
- The sidebar lists only the open area's items (R168): Chat lists the chats and no Projects block, not even an empty
  one; Projects lists the projects with their conversations and the add-folder control, and no chats. Rows keep their DOM
  when the list refreshes, so a refresh moves nothing the person is pointing at.
- Chat groups extend the same project row, expand/collapse and menu parts: a folder name opens its [short page](chat-groups.md), and its + starts a chat there. New group is the quiet + beside Groups; naming happens in the same inline rename box as chats. Groups precede the existing outside-chat lists. A group keeps its own Pinned and date sections; search lists matching chats once in the usual flat list, with matching group names above. The chat menu adds Move to group after Export, and Remove from group when assigned; moving uses an inline labelled group picker, with Escape to cancel. The group menu offers Rename and Delete; Delete asks once and keeps its chats outside the group.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- In both areas, a conversation’s right-click menu offers Rename, Pin (or Unpin), Export, Archive (or Unarchive), then Delete. Export
  opens the native save picker, parented to the window, with the title as a plain `.md` filename. Cancelling is silent.
  The Markdown holds messages with names and local times, filenames for attachments and changed files, and Faber’s own
  messages marked as Faber. Replies replace only the known working-folder prefix with its folder name and the home prefix with `~`,
  including links and literal picture targets. Both slash styles match, with case ignored on Windows; unrelated text,
  division operators, comments and web URLs stay unchanged. Embedded image bytes become generated filenames. Generated labels stay literal;
  unfinished Markdown cannot consume the next turn. Only http, https, mailto and schemeless message links stay clickable;
  other destinations and raw HTML are literal text. It leaves out tools, the work log and
  internal metadata; a failed save uses the usual error line.
- An unarchived Chat row also offers Move to project between Export and Archive, only when projects exist. It opens a
  short picker of project names using the same menu surface, keyboard navigation, focus return and reduced-motion
  behaviour. Long names wrap and a long list scrolls inside the window, without dismissing the picker. A busy
  conversation, quota recovery (even an errored reply), queued message or pending question uses the usual calm line: Stop it first.
  Background commands and unconfirmed writers keep the conversation busy after its reply ends; a Stop request waits for confirmed completion.
  A restore in progress instead asks the person to try again when it ends. A successful
  move puts the row under its project, keeping its title and pin; if open, it stays open in Projects with that project
  expanded. It leaves its Chat group, whose instructions and files stay with the group. Earlier file cards keep
  their source folder for Save, Show and image previews. The next turn uses the
  project's folder, checks and tools, applying the separate-copy setting as it is at that first project turn, whether
  sent by the person, Keep going or another conversation. Warm-up waits for the sent message's file baseline. Archived
  rows and project conversations have no move action. No new screen or animation.
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
