# Composer

The same message box serves Chat and Projects, at home and below a conversation. Its controls and model picker are
listed in the Composer row of [product-map.md](../product-map.md); placement follows [layout.md](layout.md).

The access chip keeps its place in the composer and uses Settings' `PickMenu`: each existing mode shows its name and
plain description while choosing. The bounded, scrollable list opens above the chip and wraps long translations.
The current mode is selected; arrows move, Enter or Space chooses, Escape dismisses without changing the mode and
returns focus to the chip. A choice does not send a message. Captions, restricted mode lists and disabled controls
remain supported. The list leaves the page after its existing exit; no background process or polling is added.
`tests/ui/composer.check.ts` covers the descriptions in all five languages, wrapping, unchanged composer position,
selection and Escape focus; the full Settings keyboard walk belongs to task 350's second slice.

The attachment picker copies its selected files and immediately clears the native input. Removing a file and choosing
the same one again therefore follows the ordinary attachment path in both areas, with no new UI or persistence rule
(`tests/ui/composer.check.ts`: `composer: removing a picked file allows picking the same file again`).

Each image/file removal button uses the existing small-control size (`--h-sm`) and small-corner radius (`--r-xs`,
capped at 6 px even with increased corner roundness), without scaling its hit area.
A centered 24 CSS px square remains clickable, including near its rounded corners;
file text reserves room for the control. Hover/focus disclosure and no-hover visibility stay on the existing path.
The four-attachment composer check verifies the bounds and pointer hits with normal and increased corner roundness,
as well as removing only the chosen file.

## Unsent attachments

The same draft key keeps text and attachments. Text retains its synchronous small browser-storage write; image and
file bytes use IndexedDB Blobs, with names and inline-text removal metadata. Typing does not rewrite attachment bytes.
Only a draft with a saved marker or a pending write opens that store. No resident service or polling is added.

Restoring bytes can be slower than restoring text. The person may type while they load; sending waits until the saved
attachments are known, and hydration preserves the newer words. A load failure leaves the record intact and offers
the existing Remove attachments action for explicit discard. A save failure retains the in-memory draft, keeps the
update-restart protection through navigation, and offers Try again beside the field. These warnings use the existing
quiet hint line, are associated with the textarea and announced as status; no modal, setting or motion is added.
When the person edited away an inline file's text, recovery deletes its obsolete removal metadata as well. That
cleanup preserves unrelated file reads still in flight; an explicit send, discard or conversation deletion cancels them.

A successful save means the storage transaction completed; writes request the browser's strict durability hint.
This is not a power-loss guarantee. A pending or failed write is not promised to survive a crash, and this draft store
does not close the later send/engine-acknowledgement gap. The outgoing-intent work remains separate. Component restoration
and storage-failure checks live in `tests/ui/composer.check.ts` and `tests/ui/unsent.check.ts`; the native restart scenario
lives in `testkit/src/screen/draft_attachments.rs`.

Removal has a durable intent too: removing attachments preserves newer typed words, while a confirmed conversation
deletion clears its main and side drafts. An engine refusal leaves both intact. An interrupted removal is retried on
the next opening; a failure uses the existing save-error toast and its Try again action. No retry loop or new screen.

Sending first saves the message or queued intent. If that write fails, the existing save-error notice appears and the
message returns through the same rejected-send draft restoration in Chat and Projects. Retrying adds it once; no AI
turn or steering begins before admission (`engine/src/engine/send_admission.rs`). Queued edits remain in memory when
writing fails; Send now, automatic draining and handover at the end of an agent step must save their current words
before sending them. A failed step handover keeps the edited queue intact and retries storage at the next handover. Files copied before
admission may remain in the attachment folder after a refusal; the guard does not delete files.
Refusal also preserves an archived conversation's date and follow-up choice, and its draft route. Research changes the
shared permission mode only after its intent is saved. If that later Settings write fails, the message stays sent with
an ordinary failed reply; Keep going ends with its existing failure reason, leaving human queued messages untouched
and scheduling no continuation. The composer stays empty for this accepted message (`tests/ui/save-error.check.ts`).

The native refused-send scenario in `testkit/src/screen/draft_attachments.rs` blocks only its conversation's atomic
write with a fixture-owned empty temporary directory. It checks saved history, draft PNG/PDF bytes and native retry
once after ordinary warmup, then reports the complete flow's app-tree memory and CPU separately from idle overhead.
It removes only that empty blocker; copied attachments and GPU cache cleanup remain outside this proof.

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
