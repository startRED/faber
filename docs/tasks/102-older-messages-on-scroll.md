# Load older messages when the person scrolls up

Area: Chat conversation
Files: ui/App.tsx, ui/api.ts, ui/Thread.tsx, src-tauri/src/main.rs, tests/ui/thread.check.ts
Depends: newest-messages-first
Size: about 400 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. The engine can now open a conversation with its
newest messages and give older ones on request (`newest-messages-first`). The screen still expects the whole list:
`ui/App.tsx` keeps `current().messages` and a window `from`/`until` over it (`PAGE = 40`, lines 99 to 1160), and
`Thread.tsx` indexes into it (`data-message-index`, the runs of `ui/Run.tsx`, rewind and branch by message id).

What to build: `api.open` asks for the newest 40; when the person scrolls to the first drawn message, or a link,
search result or rewind needs an older one, the screen calls the new command and prepends the page to
`current().messages`, keeping the scroll position (the screen already does the same when it grows `from` upward,
`setFrom((f) => Math.max(0, f - PAGE))`, line 782). The shell command goes into the command table the Electron move
introduces (PR #442, `src-tauri`): check `gh pr view 442` first and add it in whichever form is on `phase-1-base`;
do not build a second table. Anything that indexes the list from the start (message numbers shown, "jump to message")
uses `Conversation.earlier` as an offset.

Leave out: dropping messages again once they are far above (a later memory task), search across history.

Done: a screen check opens a conversation of 300 messages, sees 40, scrolls up and sees the next page with the same
reading position, and a rewind to message 5 loads what it needs; the product-map "Chat conversation" entry says the
screen holds a window. The commit states the memory and processor cost (R125) and names the CI run that measured the
end-to-end memory. Review by a fresh session of the other vendor (R146).
