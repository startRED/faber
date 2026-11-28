# Saved answer file review

The saved comparison attached to an answer in Chat and Projects. The surrounding answer follows
[conversation.md](conversation.md); file restoration follows [rewind.md](rewind.md).

- A settled answer with saved file changes has one quiet Changed files disclosure, in Chat and Projects. It reviews
  that turn's saved before/after versions, even after later edits or a restart; the running column's Changes still
  reviews the current whole conversation. Plain answers with no changed files add no row. The disclosure uses the
  existing one-file-at-a-time preview. Lists page by 50 names, and unified diffs by 200 lines or 128 KiB, replacing
  the previous page. Closing a file releases its patch. Long paths wrap, code scrolls locally, and native details,
  labeled page buttons and status text work with a keyboard and a screen reader at 720 px and reduced motion.
  Paging keeps focus during loading and on the last page; unavailable controls announce their state and ignore repeated activation.
  A failed page can go back without retrying. Retry keeps focus while pending or after retryable failures; success returns it to the disclosure only if the person has not moved it.
  Loading, interrupted capture, expired history, binary or oversized files and retryable read failures use calm words.
  Copied or expired points explain missing history without an ineffective retry; transient read failures can retry.
  Empty files and property-only changes say what changed instead of showing an empty content diff as "No changes".
  Restoring remains the existing Go back before this message confirmation, with its per-file conflict choices,
  redo and recovery; reading a saved comparison changes no project files.
