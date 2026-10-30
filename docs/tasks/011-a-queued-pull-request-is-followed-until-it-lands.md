# A queued pull request is followed until it lands

Area: engine
Files: engine/src/follow.rs, engine/src/engine/pr_watch.rs, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: AGENTS.md promises that a red queue run wakes the session that opened the pull request, so no session watches CI.
On 2026-10-30 two team conversations (pull requests #467 and #469) got a red `queue` status and were never woken; the
lead had to tell them. Likely cause (read from the code, not yet seen in a transcript): Faber's follow of a pull request
ends as soon as its checks read Passing (`follow.rs`, the Passing branch near line 341). A pull request's own run checks
only the lints and turns green in minutes, while `land.mjs` (run in the background, as the workflow says) posts the
`queue` status only after its local suites pass, and the queue run turns it red half an hour later. By then the follow has
ended, and `pr_seen` (`pr_watch.rs`) re-arms only when it sees `land.mjs` print "is queued as", which a backgrounded run
does not show in the tool result.

Build: first confirm the cause from one real conversation's follow state and tool results. Then the smallest change
that is correct: an open pull request is followed until it is merged or closed (Passing no longer ends it while it is
open; the looks keep growing to their 10 minute cap and the 6 hour quiet limit still holds), or Passing ends it only when
the pull request is merged. A red `queue` status after a green one wakes the conversation once, as any other red check.

Done: an engine test that fails today: a follow that reads Passing, then Failing on the `queue` status of the same open
pull request, wakes the conversation once. The product map's entry for the pull-request follow says when it ends. One
review by the other vendor.
