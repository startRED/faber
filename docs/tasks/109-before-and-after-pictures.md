# Faber takes a picture of the running app's pages before and after each project answer

Area: Page in Faber's browser
Files: engine/src/engine/browse.rs, engine/src/engine/ended.rs, engine/src/engine/pictures.rs, engine-core/src/browser.rs, host/src/pane/agent.rs, host/src/pane/tests.rs, docs/product-map.md
Depends: none
Size: about 600 changed lines

Why: plan item B1, first half (the second half shows them: task before-and-after-in-the-reply). Read on 2026-11-25:
pictures exist only when the agent asks (`browse` capture and screen-size checks), stored as
`attachments/<conversation>/site-<call>.jpg` with 20 MB per conversation (`engine/src/engine/browse.rs`,
`engine-core/src/browser.rs`), read by the screen through `site_picture`. There is no "before" picture. A turn has a
start (`TurnStarted` in `engine/src/engine.rs`) and one end (`turn_ended`, `engine/src/engine/ended.rs`). The app panel
knows a page only when a command announced a local URL (`ui/Site.tsx` `appsOf`) or a page is open in Faber's browser.

Build:

- For a project answer whose conversation has a known local app page (open in Faber's browser or announced), take a
  hidden picture of each such page when the answer starts and again when it ends, through the host's browser, with no
  window and no focus (C006). At most a few pages, one size, the existing attachment limit; skipped silently with no
  page, when the page does not load, and in temporary chats.
- Keep the pair with the answer (its message id) so a later screen can show it; a deleted conversation deletes them as
  today's attachments do.
- Avoid `engine/src/engine.rs` (task start-answers-before-history-loads changes it): hook the start where the turn is
  admitted, in a module of its own.

Leave out: showing the pair (next task), choosing which pages a turn "changed" beyond the open ones, comparing pictures
(that is the later "guard of what worked", G2).

Done: host and engine tests with a fake browser: an answer with an open local page stores a before and an after
picture under its message (fails today); none without a page; the limit holds. Memory and processor cost of two
pictures per answer in the commit body. Extends the product-map row `Page in Faber's browser`. Stored data and
processes: review by a fresh session of the other vendor (R146).
