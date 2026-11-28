# Catalog search and shelves

The Catalog list opens on one question, "What do you want to do?", the label of its only search field and the first
control on the screen ([catalog](catalog.md) holds browsing, cards and actions). Typing filters Faber's own catalog at
once, on the device, as before. Search public directories (or Enter) sends the same words to the public directories,
only on that submit and only with live web search on in Settings; opening the tab and typing never reach the network.
The host command is `tools_search`, the screen's own: it runs the agent tool's `open_search` with its plain-word filter,
cache and two-search limit. With web search off or the directories down, the open shelf says so in words
(`error.tools.searchOff`, `error.tools.searchUnavailable`) and the local shelves stay.

## Shelves

The results use the three shelves of the AI's catalog tool, each a section with a heading and one line under it:

- **Yours**: catalog items Faber holds a copy of (installed, switched off or waiting for a repair), first, because what
  the person has needs no install.
- **Faber Kit**: the rest of the checked catalog, under Good start and the job groups (`h3`). Good start leaves out
  what is already installed.
- **Open world**: up to eight public hits, after a submitted search. Each shows its id, kind and repository, a dashed
  Unchecked seal and the existing `catalog.search.unchecked` words. It has no install, switch, select box or link:
  Faber cannot install an unchecked item yet. A changed query hides the answer to the earlier words; going back to
  them shows it again without asking twice.

Yours and Faber Kit share the one list and its paging (48 cards a page), so each item appears once and a long list never
draws more than one page. The open shelf holds at most eight items and drops with the words or the tab.

## On each card

Every catalog card and detail carries a Checked seal. Size shows when known (the audio runtimes' download size). An
installed item says who installed it only when the install log (`self-changes.jsonl`) says so: its newest successful
`tools.install` line, unless a removal came after. The agent's lines name its conversation ("Installed by the AI in
“title”", with "in Full access" when that mode let it skip the card; a gone or untitled one reads "in a conversation").
Installs and removals from this screen write the same kind of line with how `catalog` and no conversation, so a
reinstall here reads "Installed by you". An item the log does not mention (installed before the log, or trimmed from
its 200 KB cap) shows no origin, never a guess. Undo after a removal shows none either. Last use is not recorded
anywhere yet, so cards do not show it. With no local match, public hits replace the empty state; the Installed filter
never shows the open shelf.

## Cost

`tools_list` reads only a plain log file and at most its newest 256 KB, once per list read, on the existing blocking thread; a line that does not parse is skipped. The open shelf
keeps one small answer in a signal while the Catalog is open; nothing stays alive after it closes. No new process.

Checks: `tests/ui/catalog.check.ts` (question first, three shelves, unchecked with no control, origin line);
`host/src/tools.rs` (`origin`).
