# The quick question becomes a side chat

The owner (2026-10-22): the side chat is "very dry, like plain HTML, it doesn't look like a real AI chat"; he wants a UX
better than ChatGPT's and Claude's. Today `ui/QuickQuestion.tsx` is a form (label, textarea, Recap, a native model
`<select>`, Ask) and answers drop once the next turn ends (`engine/src/engine/aside.rs`). Design: section 4 of
`docs/superpowers/specs/2026-10-22-panes-tabs-and-space.md`; the engine stays R250, R252 and R265 (a read-only fork, its
own process, nothing written back). Use the faber-ui skill and the design skills for the look.

1. A thread: questions on the right in a soft bubble, answers on the left as the thread's markdown, streamed, with the
   model mark, and Send to the agent under an answer that calls for action.
2. A composer pinned at the bottom in the main composer's look: placeholder "Ask about this conversation", then
   "Follow up"; Enter sends, Shift+Enter breaks the line, Esc stops; the model is a chip inside it.
3. An empty state: one line on what it is for and the chips Recap, "What is left to do?", "Explain the last step".
4. The thread stays with the conversation (newest 50 turns), across closing and restart, with Clear in the head; each
   question still forks fresh and carries the side thread's last turns. Its draft uses `keep` and `kept`
   (`ui/unsent.ts`). An `R-next` row extending R250.
5. Product map, `docs/design/`, what's-new in every language, a screen check (ask, close, reopen: the thread is there)
   and engine tests for the kept thread.

Done when: it reads as a chat in both themes at 420 px and at 720 px, the thread survives closing and restart, a visual
critique scores it above today's, and the checks fail without the change.
