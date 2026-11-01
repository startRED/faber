# CLI processes never hang or leak

Area: Helper from the other AI, Why a reply stopped
Files: winproc/src/lib.rs, engine/src/proc.rs, engine/src/engine/helpers.rs, engine/src/engine/by_itself.rs, engine/src/model.rs, ui/Thread.tsx, ui/locales/, engine/tests/, tests/fixtures/, tests/ui/thread.check.ts, docs/product-map.md
Depends: typed-failure-reasons
Size: two pull requests: first process lifetime and helper exits, then overload retries

Why: On Windows a suspended CLI can outlive Faber between process creation and job assignment. A helper whose process dies can leave its caller waiting 30 minutes. An overloaded Codex turn currently fails instead of continuing.

Build: Create a Windows process inside its kill-on-close job using `PROC_THREAD_ATTRIBUTE_JOB_LIST` and `STARTUPINFOEXW` in `winproc`, preserving pipes, no window, priority and suspended start. Test helper exits with and without partial words in both AI directions; fix only paths that wait. On typed `Failure::Overloaded` for Codex, keep the answer open and retry at 2, 4, 8, 16 and 30 seconds with one fixed internal continuation on the same thread, never the person's message. Show retry count out of five; clear it on output, ending or Stop. Cancel waits on stop, close or delete, check the quota gate before each retry, and after the fifth failure show the typed failure and usual way on.

Leave out: retrying other failures or the other AI, a retry setting and resending the person's message.

Done: A creation/resume test hook proves killing the creator leaves no process; the draft-attachments scenario passes without its startup wait. Four fake-helper cases end within two seconds with partial words retained, unknown cost, zero helpers and no process; mutation of exit-to-end makes them fail. Paused-time tests prove waits, count, five-failure limit, Stop cancellation, quota gate and no resent message; a screen check shows the count. Update map and design, add what's-new in every language. Get two reviews for process and handle code and a fresh review for the retry. Report one runtime timer per retrying conversation.
