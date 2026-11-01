# Long conversations open fast

Area: Chat conversation
Files: ui/App.tsx, ui/api.ts, ui/Thread.tsx, host command table, tests/ui/thread.check.ts, docs/product-map.md
Depends: newest-messages-first
Size: about 400 changed lines

Why: The screen expects the whole history even when the engine can open the newest messages first. A 300-message conversation needs a measured opening and smooth access to its older history.

Build: After PR #507 lands, measure opening time, memory and CPU for the same 300-message conversation before and after the change. Open the newest 40 and request older pages on scrolling to the first drawn message or when a link, search result or rewind needs one. Prepend without moving the reading position; use `Conversation.earlier` as the offset for displayed numbers and jumps. Add the command to the existing host table after checking where PR #442 landed, never a second table.

Leave out: dropping loaded messages and search across history.

Done: A screen check opens 300 messages at the end with 40 loaded, scrolls to load the next page without a jump, and rewinds to message 5 after loading what it needs. It fails without the change. Record before/after open time, memory and CPU, show no visible stall while opening and scrolling, and name the CI run measuring end-to-end memory under R125. Update the product map and design page, add what's-new in every language, and get a fresh review.
