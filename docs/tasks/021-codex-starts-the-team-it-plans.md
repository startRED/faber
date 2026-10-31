# Codex starts the team it plans for a big request

Area: Agent's plan and closing box
Files: engine/src/persona.rs, engine/src/codex.rs, engine/examples/leader_eval.rs
Depends: leader-size-rule
Size: about 60 changed lines

Why: with the lead text of task 020 (`persona::LEADS`), the `leader_eval` trial of 2026-10-30 (user plugins on, the
realistic sample of `grown`, the six big requests that each span three or more areas) gave:

| AI | big: 2+ conversations | big: plan of 3+ steps | small: direct |
|---|---|---|---|
| Claude | 5/6 | 6/6 | 6/6 |
| Codex | 0/6 | 6/6 | 6/6 |

Codex sends the plan (3 to 6 steps) every time but starts no conversation within the trial's 150 s: five of its six big
runs were still working when cut, opening with "I'll first check how…". It reads the whole project before starting, or
builds the first step itself. It also took 79 to 105 s on two small requests, where it said it was "applying the Ponytail
skill" (a user plugin).

Build: first read one cut run's tool calls (keep the trial's folder for one request, as task 020 did) to see whether Codex
starts the team after reading or builds the first step itself. Then tighten the Codex side only: either an order in the
words ("once the plan is sent, start the conversations before reading further") or the conversation tool's description
in `engine/src/codex.rs`, whichever the read shows. If Codex needs more than 150 s to reach its first start on every big
request, say so instead of raising `RUN_LIMIT` silently.

Done: `FABER_EVAL_TMP=<folder outside any repository> node scripts/cargo.mjs run -p faber-engine --example leader_eval --
codex` gives at least 5 of 6 big requests with 2+ conversations and 6 of 6 small ones direct, and the Claude run keeps its
5 of 6 and 6 of 6; both tables are in the pull request. One review by the other vendor.
