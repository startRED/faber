# Private Chat work

On Windows, default Chat work lives outside Faber's data folder. Startup recovers existing work by its recorded
directory identity before loading conversations; an ambiguous destination refuses startup without replacing either
folder. Reopening keeps the same files, and removing a chat removes its private work. A folder the person picked
remains theirs and is never removed with the conversation. On macOS and Linux, startup also moves private Chat
work to a sibling named by appending `-chat-work` to the resolved data-folder name. A separate
`chat-workspace-unix.json` record binds the work root by device and inode before
an atomic move that never replaces a destination. Reopening or recovering an interrupted move preserves the
same files, including after restoring the data directory; a missing or replaced recorded root, link,
damaged record or collision refuses startup.

Both data and work paths resolve system aliases such as `/var` before the sibling is chosen. The data folder
stays protected in every sandboxed mode; private Chat work needs no exception. Migration runs once per
startup, holds only a few directory handles temporarily, and starts no background process or timer.

This extends the existing Chat flow without a picker, setting or first-run step. The conversation
keeps its existing screen; see [conversation.md](conversation.md).
