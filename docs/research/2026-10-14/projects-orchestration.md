# Claude Code Projects: what Faber takes from it

Read 2026-10-14. Sources: the official page (code.claude.com/docs/en/claude-projects) and the launch article
(claude.com/resources/articles/projects-redesigned, 2026-09-17). Projects is in public beta on Pro and Max, cloud-first,
Anthropic-only, one user. Every item below is an AI proposal ("IA, not reviewed") until Erick decides.

## How it works

- One coordinator conversation (Opus, low effort) plus threads (Opus, high effort by default). Each thread is a full
  session on its own branch that opens a pull request and keeps watching it: it fixes CI and answers review comments.
- The coordinator routes each message: a quick question is answered in place, new work goes to a new thread or to the
  thread already working in that area, and several tasks in one message become several threads. It can list
  "Suggested threads" with a "start all" button.
- The coordinator sees only what threads report back, never their steps. It works from recent messages, recent threads
  and project memory, never the full history, so it can run for weeks.
- Overview groups threads: Ready for review, Waiting on you (reply, approval or failure), Working, Landing (approved or
  queued to merge), Idle, Resolved (by the user, by Claude after the last step, or automatically after a week idle).
- Thread cards carry the next step as a button: Resolve conflicts, Fix CI, Address comments, Merge it, Create PR.
- Standing context: project instructions (up to 16,000 characters, sent to every thread), project memory (files with a
  `MEMORY.md` index, written by Claude when told "remember this"), a Library (uploads and files threads produced) and
  the environment.
- Behaviour is tuned in plain words ("propose threads and wait", "at most two at a time", "only post when something
  finishes or is blocked"). These are saved to memory as preferences, not enforced caps.
- On creation, Claude reads recent sessions and proposes repositories, routines and starter threads, and its first
  turn is a read-only exploring thread that proposes next steps.
- Routines (scheduled threads) live inside a project. A thread can run on the user's machine through Remote Control.
- Cost advice: a follow-up to a thread idle past the 1-hour cache re-reads the whole conversation, so a fresh thread can
  be cheaper. A thread at the plan limit waits and resumes at the reset. Hard cap of 200 new threads per day.
- Their fix for threads that guessed: ask "for every open thread, list what you asked it to do, what it assumed or
  couldn't reach, and what it's waiting on", fix the gap once in the instructions, then send one thread before
  re-sending the batch.

## Mechanisms to recreate (Faber already plans neither of these in this form)

| # | Mechanism | Faber version |
| :- | :- | :- |
| 1 | Message routing in the lead | answer in place, new worker, or the worker already in that area; several tasks become several workers |
| 2 | Lead context policy | the lead reads worker reports, recent turns and `.faber/` memory, never the full history, at low effort |
| 3 | Richer states | add "Waiting on you" and "Landing"; auto-resolve after a week idle |
| 4 | Next-step buttons | plain verbs for anyone ("Fix the failing check", "Publish"), publishing still behind guardrail 3 |
| 5 | Watch after delivery | a worker that opened a pull request keeps fixing CI and review comments until it lands |
| 6 | Brief template | the five-part brief (purpose, where, how to check, what to do when something is missing, what needs approval), written by Faber, not by the user |
| 7 | Setup proposals | on a new project, read recent work and propose starter tasks; the first worker explores read-only |
| 8 | Library | one list of files the user added and files workers produced, per project |
| 9 | Diagnosis question | "what was asked, what was assumed, what was missing, what it waits on" per worker, on demand |
| 10 | Daily worker cap | a sanity cap on new workers per day |

## Where Faber goes further (from the ADHD run)

1. **Missing-item gate.** Projects only asks threads in text to stop when something is missing. In Faber a worker
   finishes through a `worker.report` with required `assumed` and `could_not_reach` fields; a non-empty
   `could_not_reach` or a stand-in (mock, fake key, invented path) keeps the step in Waiting, and "done" is refused.
   The user's one-tap answer goes into the project brief, and every new worker brief is checked against past gaps
   first. Risk: a worker that mocks and reports nothing, so the reviewer from the other vendor audits `assumed`
   against the diff. First step: `engine/src/gate.rs` with the report type and a fixture test that a mocked report
   stays Waiting.
2. **Canary before a batch.** Projects suggests "send one thread first" only as a repair. In Faber, when the lead fans
   out N similar workers, one runs first, passes its proof and a cross-vendor review, its mistakes become rules in the
   shared brief, then the rest start in parallel. Skip it for 3 or fewer workers or a very cheap proof. Risk: a
   canary that is not representative; one canary per cluster of similar members reduces it. First step: a `fanout`
   group in the mission step graph held until the canary is green.
3. **Worker lease, written state and cache-aware revive.** Projects leaves idle threads open for a week and leaves the
   warm-or-fresh choice to the user. In Faber a worker's lease renews only with evidence of progress; when it lapses
   or ends, the worker writes its state (step, branch, next step, proof, files) to `.faber/` and its process exits.
   A follow-up resumes the session while the 1-hour cache is warm and starts a fresh one seeded with that state when
   it is cold, showing both token estimates. The same file serves vendor switches and crash recovery. Risk: the
   written state loses details a resumed session would keep. First step: a pure `revive_plan` next to
   `engine/src/handover.rs`, tested at 59 and 61 minutes.

Other ideas from the run worth keeping: when one subscription runs out, move the work to the other one instead of
waiting (Projects can only wait); replace soft "at most two at a time" sentences with real caps in code; a
plain-language "what changed for you" line at the end of each worker; tag each memory line with its author and source
worker so a wrong memory can be found and struck; spend spare quota on a background A/B of a finished task with the
other vendor to feed the router.

Traps found: a cross-vendor audit of every routing decision (doubles the lead's cost for a router already right 81 of
88 times); running two or three competing workers by default (two to three times the cost against the 70% goal);
pruning memory automatically (it could drop the owner's decisions); batching all attention notifications into one
card (contradicts R92, one notification per question).
