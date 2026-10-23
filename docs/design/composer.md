# Composer

The same message box serves Chat and Projects, at home and below a conversation. Its controls and model picker are
listed in the Composer row of [product-map.md](../product-map.md); placement follows [layout.md](layout.md).

The attachment picker copies its selected files and immediately clears the native input. Removing a file and choosing
the same one again therefore follows the ordinary attachment path in both areas, with no new UI or persistence rule
(`tests/ui/composer.check.ts`: `composer: removing a picked file allows picking the same file again`).

## Completion list and earlier messages

- `@` in a conversation with a working folder lists matching files and folders over the box, on the context menu's
  surface, left-aligned with the text, with no motion; the entry the arrows reach is highlighted and kept in view, and a
  long path ends in an ellipsis. `/` at the start opens the same list for the agent's commands and skills (`/btw` and
  `/recap` first in an open conversation); a pick inserts the CLI's spelling without sending. Faber's `/recap` is
  labelled "Recap" and instead opens the quick question and asks at once, inserting nothing (R266).
  Both lists follow the caret and ignore
  older answers and non-main mouse buttons. A pending request or populated list keeps its arrow keys; a completed
  file or command request with no results releases Up to history, even while its unmatched token remains in the box.
- `/research` in that same list starts web research in Chat or Projects, with either AI or Auto. Picking it inserts
  `/research ` without sending or changing Auto; the person adds their question. It is absent when web search is off
  or no AI is connected; account or installed-tool limitations are left to the CLI to explain. Sending switches to
  the shared Plan mode and asks for a short numbered research plan, without searching yet. The existing plan card
  offers Keep planning (then write edits in this composer) or Approve and run with its ordinary permission choice.
  Approval starts one normal turn, paced by Auto, whose report is the reply's usual Markdown with linked claims and
  a Sources list ([conversation.md](conversation.md#answers)). No new surface, motion, timer, setting or research loop
  (`tests/ui/research.check.ts`, `engine/tests/engine.rs`: `research_claude_plan_edit_approve_report`, `research_codex_plan_edit_approve_report`).
- In both areas, Up in an empty box or with the caret on its first visual line recalls earlier messages, unless an `@` or `/`
  list is open. A temporary mirror copies the textarea's typography, wrapping and content width (excluding its scrollbar);
  the browser's text range measures the caret's line. It is removed in the same call, with no cache or idle work.
  The open conversation comes first, then conversations in the same area (and project), most recently active first;
  messages within each are newest first, deduplicated, at most 200. Faber's own messages and side questions stay out.
  Sending waits for recall to finish; Enter while it loads sends nothing. Down past the newest, Escape or leaving the
  box restores the draft and its caret; every content edit makes the current message the draft, including typing,
  pasting, dropping or attaching a file, and removing an attachment. File reads append to the latest draft, so typing
  while a file loads is preserved; Escape cannot undo the addition. Clicking an editing control keeps focus until
  it makes its change. Clicking Send keeps focus until
  the recalled message has been sent, so the click cannot restore and send an unrelated draft.
  Ctrl+R opens the same list, with the
  box as its search field; typing filters, Enter inserts without sending, Escape or leaving the box restores the draft.
  Empty results and a read failure use the list's quiet status line. No extra surface, motion, setting or stored value;
  one blocking engine read, cached only during focus (`tests/ui/prompt-history.check.ts`, `engine/tests/prompt_history.rs`).
- History uses the ready sidebar index's area, project and last-activity metadata to choose files before opening them.
  While that index is still building, one directory listing orders files by modification time, with the open conversation
  first; the bounded reader checks each file's own area and project before reading messages. Recall never waits for
  indexing, and stops at 200 unique prompts. A buffered reader skips attachments and replies, checks cancellation on
  each 8 KiB refill, and limits input to 16 MiB per file and 64 MiB per request, retaining at most 2 MiB of prompt text. Oversized
  files and prompts are skipped intact. A newer request, Escape during a read, editing during recall, blur or unmount
  cancels abandoned work; old cancellations never cancel a newer request. No prompt text lives in the sidebar index.
- The 500-conversation measurements cover both states of that existing sidebar index. The warm check times the command
  after the index is ready; the cold check starts sampling before engine initialization and asserts the index is still
  building immediately before first Up. Both use the same valid saved files, with 10,000 prompts and 10,000 replies;
  cold here describes the index, not an emptied operating-system disk cache.
