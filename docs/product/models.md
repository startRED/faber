# Models and CLIs

Why only Claude Code and Codex, how Auto routes between them, and the rules for driving their CLIs.

## Only Claude Code and Codex

Faber works with Claude only, Codex only, or both. It supports no other vendor.

- **The subscription is the main path.** An API key is optional and goes only through the CLI's own login, so Faber
  never sees it. Nobody needs an API.
- **First run.** The user connects what they have. Faber finds or installs each official CLI and opens its own sign-in,
  and the router and the toolbox only use what is connected. A person who is new to Claude, Codex and AI apps gets a
  short tutorial; an experienced one skips it. After that Faber does not keep teaching: there is no help page, and
  people ask Faber about Faber.
- **Both vendors:** reviews and second opinions come from the other vendor.
- **One vendor:** reviews come from a different model of the same vendor in a fresh session, and the screen says the
  review is weaker.

### Auto: a blind router

Each vendor's model tends to prefer its own company's models, so in Auto no model ever sees a model's name or a vendor's
name. Auto aims to pick a model that does the task well, without waste and quickly.

- A small, fast model that is already running (GPT-6 Luna or Haiku at medium effort, whichever is connected) reads the
  request alone and says what kind of task it is and how hard. It never sees the candidates.
- A fixed table then picks the model, tuned for quality first: each model's profile has general intelligence and
  agentic coding scores, seconds per task, and cost per task priced by the quota left on its subscription right now, so
  a nearly spent subscription hands its work to the other one (done, `engine/src/route.rs`).
- Measured on 10/10/2026 with the real router on 22 requests, run twice: 81 of 88 picks right, half the routes under
  2.8 s and nine in ten under 3.9 s. An earlier design that let the router choose among anonymous, shuffled profiles
  got 76 of 88, spent more and was slower: the router judges requests well, the table weighs price against quality.
- Profiles start from public benchmarks (Artificial Analysis, read 22/09 to 08/10/2026, compiled in for now) and move
  to Faber's own scoreboard as it fills: which answers users kept, redid or rejected. Later they ship in the live
  manifest, so a new model joins without an app release.
- No local model and no hosted classifier: routing spends only the subscriptions the user already has.
- Routing runs on the first message of a conversation, and on the next ones while only an easy model was needed (a
  greeting first). Those stay with their subscription, so the CLI keeps the history, and only move up. Once a message
  needed more, the conversation stays with that model. A sent message waits at most 30 s for its route; past it, the
  turn starts with the everyday default. The answer shows the chosen model and the reason. While the person types, the
  draft is routed at most twice per message, and sending it starts the count again (done, `engine/src/engine/auto.rs`).
- The router also says how the work can be checked: by a machine (a test, an exit code, a number to recount), at a
  glance, or blind (the person cannot tell a wrong answer from a right one). A blind medium request in code, math or
  research gets the hard bar, as math always did, because the model's own checking matters most where nobody else can
  check (done, `engine/src/route.rs`; measured in [quota-pacing.md](quota-pacing.md#cache-and-router-measured-on-13102026)).
- An answer whose last check failed, or whose code changed with nothing checking it, offers one step up: the same
  model one effort higher, else the next model at medium. It runs as a new branch, so a conversation's model and effort
  never change (done, `engine/src/nudge.rs`).
- **Still to build.** The table learns from what the user approves or redoes.
- **Quota pacing** (built; [spec](../superpowers/specs/2026-10-11-auto-quota-pacing-design.md)). Auto
  reads each window against its reset time, the weekday and the plan (detected at start and watched while open, on a
  shared account), and spends quota that would expire on quality: one rung up when a vendor has spare quota.
- The router is measured too: on the benchmark of real requests, its picks must match always using the strongest
  model while spending less quota.

## Rules for driving the CLIs

These are product rules. They come from research on driving Claude Code and Codex and from the model-use notes. Each is
re-measured when a CLI version changes. Every piece of code that starts a CLI or talks to one follows them, in every
phase.

1. **Never use Codex app-server login.** Faber never calls `account/login/*` and never stores `auth.json`. Sign-in
   happens only through `codex login` or the CLI's own flow.
2. **Always pass the permission mode, the full model id and the effort when starting a CLI session.** An omitted flag is
   a hidden choice.
3. **Set the model and the effort before a conversation starts, and never change model, effort, tools or system prompt
   in the middle of one without need.** The prompt cache is the biggest saving. A change of model always starts a new
   cache, and a change of tools or system prompt invalidates everything after it. A change of effort kept the Claude cache
   through `--resume` and cost Codex nothing measurable (13/10/2026, [quota-pacing.md](quota-pacing.md#cache-and-router-measured-on-13102026)). Facts that change travel as
   text inside the next message, never as a system-prompt edit. A change has need when the person made it (the model
   picker, the permission mode, a setting), when Codex's catalog tool servers were revoked or changed, when a plan limit
   moves the conversation to the other AI (rule 4's hand-over), or once when Auto met an easy request first (a greeting)
   and the next one needed more; nothing else changes them
   (`engine/tests/auto.rs`, `a_second_message_keeps_the_process_the_model_and_the_effort_of_the_first`).
4. **Hand work to another model through a written summary or a diff, not the conversation.** Thinking blocks are tied to
   the model that wrote them.
5. **Interrupt gracefully before killing, watch for silence, and kill the whole process tree on Windows.** The stop uses
   the CLI's control-protocol interrupt, waits for the result, then kills the tree (a job object on Windows, the process
   group elsewhere). An answer whose CLI says nothing for 15 minutes, with no step running and no question waiting, is
   stopped the same way and ends with `reason.silent` (`engine/src/engine/silence.rs`, R396).
6. **Read the CLI's start-up capabilities, the result subtype and the terminal reason, and map them to `reason.*`
   keys.** Faber detects features from the `capabilities` list in `system/init`, never by comparing version strings, and never
   shows a raw reason string.
7. **Keep Faber's own running budget, and apply the big-work rules when it is passed.** The CLI's budget flag is only a
   client-side estimate, so Faber counts spend per turn itself. Passing the estimate only warns and changes to economy
   mode.
8. **No SDK prewarm.** A spare process waiting for a folder costs about 230 to 260 MB, which breaks the idle budget.
9. **Keep each Haiku request under 100K tokens and each GPT-6 request under 272K.** Above those points the whole request
   is billed more.

Rules 1, 2, 3, 5, 6, 8 and 9 hold today (rule 5's silence watch covers an answer; a tool step it reports as running, a helper's own process and a quick question are not watched). Every session start passes its mode, model and effort (the CLI probes that start no
session carry no model: Codex's lists of models, quota and threads, and Claude Code's `--help` and its `/usage` read), the engine keeps each session's start-up
`capabilities`, and every result becomes a `reason.*` key. The engine that already drives the CLIs has known legacy
exceptions to the others. They are known defects of the old code, no new code may add to them, and each phase closes some:

| Rule | What the engine does today | Closed in |
| :--- | :--- | :--- |
| 4 | There is no lead agent yet, so there is no hand-over | Phase 3 |
| 7 | The engine keeps no running budget | Phase 3 |
