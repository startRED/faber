# Faber's browser gets tabs and a real browser bar

Start after task 020 (it adds the tab list this task shows). The owner (2026-10-22): several tabs like Claude's, New
tab, Back and Forward, an address field that looks like Google's and is easy to see, and no example in it ("Type an
address, like example.com" wastes room). Design: section 3 of `docs/superpowers/specs/2026-10-22-panes-tabs-and-space.md`,
from a clean-room study of Claude desktop's browser; follow `docs/design/` and the faber-ui skill for the look.

1. One row before the page: the tab strip replaces the "Faber's browser" title (New tab, expand, close at its end);
   under it Back, Forward, Reload (always there, dimmed when they cannot act), the address field and one "..." menu
   holding Open in your own browser, Copy address, Screen size, Sign in with your Firefox and Clear this conversation's
   browsing data (asks first).
2. The address field: a filled pill with normal text contrast, placeholder "Search or type an address" in every
   language (written by hand); focus selects all; words search Google; Escape restores, a second Escape leaves; host
   bright and path quiet when unfocused; a thin loading line. Narrow panes drop Forward first.
3. Tabs: favicon and title (host, or the port for localhost), close on hover and on the front tab, middle click, right
   click (Open in your own browser, Close, Close others), at most 16 with an overflow menu. Keys while the pane has
   focus: Ctrl+T, Ctrl+W, Ctrl+Shift+T, Ctrl+Tab, Ctrl+1..9, Ctrl+L, Ctrl+R, Alt+Left, Alt+Right, in `ui/keys.ts`.
   Closing the last tab closes the pane.
4. The agent's tab: it opens a new tab unless the front one is blank, shows a busy mark while acting, and read, click
   and type act on it only (the engine's `Browser` request gains an optional tab; `reported` and the load counters
   become per tab). Only the front tab is live; the others are records loaded on demand; the agent's tab keeps a second
   live webview only while it works out of view. Measure one and two live webviews against R125.
5. `R-next` rows, the product map, `docs/design/layout.md`, a what's-new entry in every language, screen checks for tabs,
   keys and the address field, engine tests for the agent's tab.

Done when: several tabs open, switch, close and reopen, the agent never acts on the person's tab, memory stays within
R125 with the numbers in the commit body, and the checks fail without the change.
