# Every wait has a guaranteed wake

Area: project
Files: engine-core and engine-projects (conversation waits and wake events), host table, `docs/product-map.md`, the design page of waits
Depends: none
Size: one pull request

Why: On 2026-11-15 a person went to sleep with five conversations running, and the work stalled three times because a wait never woke. Conversations 839512a2, c890be48 and 337b5e6a (children of the lead fe2e9f88) ended their turn with a wait on a pull request's remote checks. GitHub finished the checks green, yet Faber did not wake them: 35 to 40 minutes on pull requests 134, 135 and 136; about 2 hours on 839512a2, whose pull requests 129 and 132 were already merged. It happened again when a child pushed a fix to the same open pull request (new head commit, wait note about the CI of the update): checks were green at 01:20 UTC and the wait was still open 14 minutes later; a third time 4 minutes after checks passed. The lead fe2e9f88 waited on those children the whole time. Each one resumed only when a person typed a message on the screen. Evidence: `conversations/<id>.json` (`waitingOn`, `since`) in the app data folder, and `gh pr view <n> --json statusCheckRollup`.

Build: A wait names what wakes it and a latest time. A wait on a pull request wakes on a change of the real GitHub state: checks completed (green or red), pull request merged or closed, a new head commit, a new review. Waiting on a conversation wakes when that conversation concludes, stops or fails. Every wait also has a maximum time after which Faber reads the real state itself and wakes the AI with it, even when no event came, so a lost event never leaves a conversation asleep. The wake text states what changed. A wait whose subject is already finished when it is created wakes at once. Start from how `waitingOn` is stored and woken today and find why these did not fire (a missing event, a poll that skips a head commit change, or a wake lost while the app was busy).

Leave out: changing what an AI may do after it wakes; polling that costs tokens while nothing changes (the check is plain code, no AI call).

Done: A test fails without the change: a fake GitHub state sequence (checks pending, then green; a new head commit; a merge) wakes a waiting conversation without a person, and a wait with no event wakes at its deadline. One real run with the cheapest models (Haiku, Sonnet) on a throwaway repository shows a conversation waiting on a pull request and waking by itself. Memory and processor cost of the check is in the commit body.
