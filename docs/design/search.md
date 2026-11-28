# Search

How the sidebar search, command box (Ctrl+K), find in a conversation (Ctrl+F) and the list of shortcuts (?) look and move. The rest of the shell is in
[layout.md](layout.md).

## Sidebar search

The existing search field filters titles and project names immediately, including archived conversations. After a
180 ms pause it also searches the messages of the open area only, up to 30 hits below the title matches. Each row
shows the conversation title and about 80 characters around the first match; matching words are bold and underlined,
so colour is never the only cue. Text wraps inside the sidebar, including at 720 px and in the longest language.
The group says "In messages", "Searching messages…" while the index or query is pending, "Nothing matches" when no
message matches (title matches stay above), and "Messages could not be searched. Try again." after a failed query; every word is localized.
Clearing the field removes the result rows, and a stale reply or one from the other area cannot replace newer results.
Opening a conversation in the same area keeps the current results without restarting the search.
Refreshing the conversation list keeps matching message rows mounted by conversation and message, preserving focus.
Archived message hits carry the same archived label as title hits.
There is no polling after the index is ready, or while the window is hidden.

A click opens the conversation at that message and loads a bounded page around it. It unfolds the run once per search
navigation, consumed by the app once the message is revealed, so later manual folding survives the next automatic
turn recreating the run. It focuses the message with a static outline for 1.8 seconds. Scrolling centres the message smoothly only with full motion;
reduced and no motion scroll instantly. No text animates. Continuing to scroll loads the next page, and sending a
message returns to the newest page. These are the same controls and behaviour in Chat and Projects.

The engine's volatile index covers every user message and completed final answer, never tools, thinking, attachments
or Faber's own prompts. Per conversation it keeps only distinct words, lowercased and folded for common Latin accents,
split on non-letters/digits, 2 to 32 characters each: one sorted newline-delimited string, with no message text or
positions. A safety net keeps at most 20,000 distinct words in lexical order, independent of message age.
All word payloads together stay within 16 MiB; oldest conversations lose their word set first. Incomplete word sets
(including the per-conversation safety net and pending updates) fall back to reading the file, preserving coverage. Query words
are folded and split the same way; binary prefix searches select conversations containing every query word.
Candidates are read newest first from the store on a blocking worker, outside the engine lock. The first user message
or final answer containing all query prefixes supplies the original snippet and message index, one hit per conversation,
up to 30 hits; stale candidates that no longer match are skipped. Title matching is unchanged.
The index builds on one engine-lifetime background-priority worker. Saves queue only metadata, at most one pending
update per conversation; the worker reads and folds the file, then swaps the entry only if no newer save or removal
superseded it. It sleeps without polling and stops with the engine. One search runs and only the latest waits; a newer
query replaces the waiter and cancels the running scan between files. No index file, process or idle timer is added.

## Search and commands, and find

- **Ctrl+K** opens one box over everything, centred sideways and set in the upper third so it does not jump as the results
  change. It scales from 0.97 and fades in with `--dur-ui` and leaves faster with `--dur-exit`; behind it the screen dims
  with a veil of its own colour, never a blur (only the sidebar scrim blurs). It is a modal dialog: focus goes to its box,
  Tab stays inside (`ui/focus.ts`), Esc or a click outside closes it and gives focus back, and the shortcuts of the screen
  underneath wait. The box is the shared search field (`.field.search`), taller (`--h-lg`), with no ring of its own; rows use
  `--h-md`, the chosen one on `--s3`, and the project or area of a conversation sits quiet at the right. Empty, it lists
  "Recent" (five conversations) then "Actions"; typing gives one list ranked across everything, eight rows at most.
  Moving the choice by arrow changes its colour only (`--dur-fast`), no travel. It stays for its own exit, then leaves
  the page without a second list-exit copy over the conversation.
- **Ctrl+F** in an open conversation slides a bar down from the top right of the thread (12 px and a fade, `--dur-ui`; it
  leaves with `--dur-exit`) with the shared search field, "3 of 12", previous, next and close. Matches are painted with the
  CSS Custom Highlight API: all in the accent at 42 %, the current one in the full accent, so no message is rewritten and
  nothing is wrapped. Enter and Shift+Enter move with a smooth scroll that centres the match unless it is already in plain
  view (instant in reduced and no motion); Esc closes the bar before the sidebar or the running column. When the running
  column covers the thread, the bar slides left of it. While present, including its exit, the bar reserves one
  large-control height of extra thread padding so the first match cannot remain behind it at scroll top zero.
  Closing keeps its displayed count until removal but releases highlight ranges immediately, so the bar never
  widens to show a false no-results state during its exit. Both are instant in reduced and no motion.
- **?** outside a text box opens the list of shortcuts in the Ctrl+K box's own surface, scrim and motion: a heading row
  (`--h-lg`, a hairline under it) with a close button that takes focus, then "Anywhere" and "In the message box" with the
  box's quiet headings and a row per key (`--h-md`): what it does at the left, its caps as `kbd` at the right in the data
  face. Rows never highlight, since nothing in the list is picked. It is a modal dialog like the box, and it exists in the
  page only while open.
