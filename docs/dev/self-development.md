# Self-development

How Faber builds Faber: the owner opens Faber's own repository as a project and talks to its agent, and the agent works
the way the sessions that built Faber by hand work (AGENTS.md). Nothing here is a mission: it is a normal project
conversation (R160, R161) with the parts below.

## Start it

1. In the installed Faber, open Projects, "Open a project" and pick the repository folder
   (`E:\Documentos\ProjetosIA\faber` on the maintainer's PC). Settings > Projects keeps separate copies on (the default).
2. Start a conversation with Claude Code or Codex in Full access (the default) and send:

   > Take the next task with `node scripts/tasks.mjs next` and do it to the end, following AGENTS.md.

3. Turn on Keep going in the composer to let it go on to the next task by itself, or leave it off to read each result.

The CLI reads AGENTS.md by itself, so the rules, the Rust paths (`export RUSTUP_HOME=…`) and the proof commands come
from there. Faber's own checks need the Rust paths too, from the project folder's untracked `.faber/verify.local.json`
(`{"env": {"RUSTUP_HOME": "…", "CARGO_HOME": "…"}, "path": ["…/cargo/bin"]}`). Faber never reads or forwards CLI credentials; `gh` uses the owner's own sign-in.

## What each part does

| Step | Part | Where it is |
| :--- | :--- | :--- |
| The conversation works away from the owner's checkout | Separate copy on a `faber/…` branch (R201) | product map, Separate copy |
| It picks work | The task queue, `docs/tasks/` (R185) | [workflow](workflow.md#task-queue) |
| It splits work it can run in parallel | Conversations the agent starts, at most 3 per project (R198) | product map, Conversation started by the agent |
| It knows in seconds whether a change works | The `verify` tool and the Checks Faber runs itself when a turn changed files (R203; product map, Checks) | product map, Checks; [spec](../superpowers/specs/2026-10-17-built-in-verifier.md) |
| Someone else reviews it | The helper from the other AI (R161, R146) | product map, Helper from the other AI |
| It lands | A pull request queued with `node scripts/land.mjs <number>` (R182) | [workflow](workflow.md#merge-on-green) |
| A red check wakes it | Follow (a pull request) (R200) | product map, Follow |
| It goes on while the owner is away | Keep going (R199) | product map, Keep going |
| The owner gets the new build | A landed `feat`, `fix` or `perf` becomes a patch release (R130) and the installed Faber updates itself (R156) | product map, Public self-update |

## Measuring when it splits

`cargo run -p faber-engine --example leader_eval [-- claude|codex]` sends twelve hand-labelled requests (six big, six small)
to fresh project conversations through the real engine and CLIs, on the owner's subscriptions, each in a throwaway
repository under the system's temporary folder. A run stops when the reply ends, after three started conversations
or after 150 seconds, and every conversation is then stopped. It prints, per request,
the conversations started, the plan's steps and a verdict, then per vendor how many big requests started two or more
conversations or planned three or more steps, and how many small ones started none (that one must stay 6 of 6), with
tokens and dollars at API price. Run it before and after a change to the agent's instructions or tools
(`engine/src/persona.rs`, `engine/src/sibling.rs`); `cargo test -p faber-engine --example leader_eval` checks its counting.

## What stays the owner's

Direction, taste, money and license (AGENTS.md). The agent asks those in a question card inside its message and goes on
with the next task while the card waits. What goes in `docs/tasks/` and in what order is the owner's too; a session may
add a follow-up it found after the owner's tasks.

## Not built yet

The open steps are tasks in `docs/tasks/`, so the first conversation can take them: a quick Rust check for the verifier's fast level, the rest of verifier V1, its GitHub confirmation
and conflict-aware queue (R190 V2 to V4), and the project map built from the code (R146).
