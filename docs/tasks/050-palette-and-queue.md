# Command palette search and message queue

From the T3 Code study (`04-general-ux.md`). The palette also finds threads, message text and settings; messages typed
while the agent works show as queued bubbles with "Send now" and, when the limit is reached, "Resume at reset".
Check the product map first: extend what exists, never a second way. Longest language and 720 px window walked.

Inventory on the refreshed base: `ui/Palette.tsx` already finds conversation titles, projects, actions and settings
sections; `ui/Sidebar.tsx` has message-text search. Reuse its engine search in the palette with stale-result and
empty-query handling, rather than loading all messages. Existing checks: `tests/ui/palette.check.ts`,
`tests/ui/search-messages.check.ts`.
Queued bubbles with Edit, Remove and Send now already exist in `ui/Thread.tsx` and are covered by
`tests/ui/steer.check.ts` and `tests/ui/edit-queued.check.ts`. Preserve the stored queue and quota recovery path;
first prove whether a new Resume at reset action is needed or the current automatic quota wait already does the
job. Never duplicate a queued send or imply a known reset when the provider did not supply one.
