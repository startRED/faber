# Private Chat work

On Windows, default Chat work lives outside Faber's data folder. Startup recovers existing work by its recorded
directory identity before loading conversations; an ambiguous destination refuses startup without replacing either
folder. Reopening keeps the same files, and removing a chat removes its private work. A folder the person picked
remains theirs and is never removed with the conversation. Unix still uses the existing private Chat layout.

This extends the existing Chat flow without a picker, setting or first-run step. The conversation
keeps its existing screen; see [conversation.md](conversation.md).
