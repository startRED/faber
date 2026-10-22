# Faber's browser remembers its pages

The owner (2026-10-22): when the browser is closed and opened again, the page that was open is gone. Today the address
lives only in the one webview (`src-tauri/src/pane/mod.rs`) and in memory (`lives`, `ui/Site.tsx`); the 5-minute idle
close, a conversation switch and a restart lose it, and reopening shows only the agent's last page. Design: section 2 of
`docs/superpowers/specs/2026-10-22-panes-tabs-and-space.md`.

1. Keep a browser tab list with each conversation in the engine's conversation file (address, title, front tab, the
   agent's tab), the way R350 keeps interactive choices. One tab for now; the shape is the one task 030 uses. Left out
   of exports, gone with the conversation, never in browser storage.
2. Record the address and title on every load the person or the agent makes (the pane already hears loads). Reopening
   the pane, returning to the conversation and restarting Faber load the front tab. The idle close keeps only freeing
   the webview.
3. Cookies and sign-ins stay private and end at quit (R308, R370); no picture is kept (R236). Write the `R-next` row.
4. Engine tests for the record (save, load, export leaves it out) and a screen check: open a page, close the pane,
   switch conversation and back, reopen: the same page (it fails today).

Done when: the last page comes back after closing the pane, switching conversation and restarting, and the checks fail
without the change. Report the memory cost (a few hundred bytes per conversation; no live webview added).
