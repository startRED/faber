# A temporary chat that leaves no history

Area: Chat
Files: engine/src/model.rs, engine/src/store.rs, engine/src/engine/closed.rs, ui/Sidebar.tsx, ui/Composer.tsx, ui/api.ts, ui/locales/, tests/ui/sidebar-exit.check.ts, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 300 changed lines

Why: `docs/product/parity.md` row F6: ChatGPT has a temporary chat that is deleted when closed; Faber has none. A person who asks something private (health, money) has no way to keep it out of the sidebar, the message search and the data folder.

Build: a conversation flag `temporary` set when the person starts the chat from a "Temporary chat" entry of the new-chat menu (the same place as the other ways to start a Chat, never a Settings row). A temporary chat is shown with a small quiet mark, is left out of the sidebar's recents, the message search, Auto's learning and the export of recents, and is deleted (file, search words, attachments, CLI session) when it is closed, when another conversation is opened, and at the next start for any left behind by a crash. The mark says in one short line what happens. Words by hand in every language.

Leave out: Projects (Chat only), a retention timer, and any cloud meaning of "temporary".

Done: engine tests that fail today: a temporary conversation never appears in the summaries or the search, is gone after close, and a leftover one is removed on the next start; a screen check starts one, sees the mark and finds it gone afterwards. Extends the map rows "Conversation" and "Other stored state". What's-new entry in every language. Memory and processor cost stated (none beyond one flag check).
