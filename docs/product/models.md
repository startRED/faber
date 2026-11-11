# Models and CLIs

Why only Claude Code and Codex, how Auto routes between them, and the rules for driving their CLIs.

## Only Claude Code and Codex

Faber works with Claude only, Codex only, or both. It supports no other vendor.

- **The subscription is the main path.** An API key is optional and goes only through the CLI's own login, so Faber
  never sees it. Nobody needs an API.
- **First run.** The user connects what they have. Faber finds or installs each official CLI and opens its own sign-in,
  and the router and the toolbox only use what is connected. A person who is new to Claude, Codex and AI apps gets a
  short tutorial; an experienced one skips it. After that Faber does not keep teaching: there is no help page, and
  people ask Faber about Faber (built: Faber's own guide is a file every session can read, told by one short line,
  [product map](../product-map.md#session-kits)).
- **Both vendors:** reviews and second opinions come from the other vendor.
- **One vendor:** reviews come from a different model of the same vendor in a fresh session, and the screen says the
  review is weaker.

### Auto: a blind router

Each vendor's model tends to prefer its own company's models, so in Auto no model ever sees a model's name or a vendor's
name. Auto aims to pick a model that does the task well, without waste and quickly.

- A small, fast model that is already running (GPT-6 Luna or Haiku at medium effort, whichever is connected) reads the
  request alone and says what kind of task it is, how hard, and whether its scope is tight or open. It never sees the candidates.
- A fixed table then picks the model, tuned for quality first: each model's profile has general intelligence and
  agentic coding scores, seconds per task, and cost per task priced by the quota left on its subscription right now, so
  a nearly spent subscription hands its work to the other one (done, `engine/src/route.rs`).
- Measured on 10/10/2026 with the real router on 22 requests, run twice: 81 of 88 picks right, half the routes under
  2.8 s and nine in ten under 3.9 s. An earlier design that let the router choose among anonymous, shuffled profiles
  got 76 of 88, spent more and was slower: the router judges requests well, the table weighs price against quality.
- On 10/11/2026 the updated router was checked twice on 42 hand-labelled requests. Haiku returned 84 valid lines at each effort: at medium, kind 90%, difficulty 74%, check 85% and scope 98% agreement; at low, 89%, 74%, 79% and 98%. Median time was 2.7 s for both. Luna returned one valid line at medium and none at low, so this run cannot establish its accuracy. These labels are one person’s judgement, not proven answers.
- Profiles use Artificial Analysis per-task runs, republished by [Slopalytics](https://slopalytics.com/) on
  2026-11-10: rounded intelligence and coding sub-index, seconds per task and US$ per task, all on one scale.
  No maximum effort: its extra quota and time are outside Auto's ladder. Fable xhigh and Luna xhigh remain excluded.
  Fable low is removed: its maker starts at medium/high and measured Opus high ahead for less per solved task.
  Luna high is added (21/13, 336 s, US$0.073), the Luna-only fallback. Still 24 rows; the router stays at medium.
  The compiled table later moves to the live manifest and Faber's own scoreboard of kept and retried answers.

  | Level | General intelligence | Code / project coding |
  | --- | ---: | ---: |
  | Easy | 24 | 15 |
  | Medium | 38 | 32 |
  | Hard | 46 | 39 |
  | Expert | 50 | 42 |
  | Frontier | 56 | 46 |
  | Summit | 60 | 50 |

  Medium code has the easy intelligence floor (24); medium math gets the hard intelligence floor (46).
  Medium chat has intelligence floor 31; easy and medium chat exclude rows over 300 s whenever a row within
  300 s clears the whole bar. With neutral factors, Claude-only medium Chat uses Sonnet medium; with Terra
  available it uses Terra medium, otherwise Sol low.
  Projects still add the coding floor to every kind. Other general tasks have no coding floor.
  Claude-only medium work outside chat uses Sonnet high; easy work keeps its quality floor. Fable starts at medium.
  After finding the cheapest sufficient row, Auto prefers the strongest row that is at least as good on both scores,
  improves one, fits the task's time cap, and costs at most 15% more under the same reading, weight and quota factors.
  Easy/medium chat allows slower premium rows within its 300 s pool.
  Ties favor lower price, then shorter time. Quota factors can apply this rule across vendors.
  The existing stronger retry also carries xhigh through the choice contract; the manual picker still offers three buttons.
- **Long reads are priced by the token** (`engine/src/route.rs`, `read_cost`). Per-task prices assume a typical read; a
  message with a long document or many pictures, or a long conversation moved to another model, reads far more, so each
  model's cost adds what that read costs at its own price per token (its long-prompt tiers included), with the context
  the conversation's model already holds read from its cache. The bar does not move: a long read only changes which of the
  models that clear it is cheapest, and keeps a long conversation on its model when spare quota elsewhere would not pay for
  reading it all again.
- No local model and no hosted classifier: routing spends only the subscriptions the user already has.
- Routing runs on the first message and reads later drafts for effort. Only while an easy model was needed (a
  greeting first) may a later verdict also select a stronger model. Those stay with their subscription, so the CLI keeps the history. Once a message
  needed more, the conversation stays with that model. One usable model/effort profile needs no classifier. Otherwise,
  the native router runs while the person types, with up to 30 s for CLI startup and inference. Sending reuses a finished,
  still-matching draft verdict; when none is ready it starts immediately with the previous choice or everyday default,
  without opening a classifier request on send. Reusing an easy route preserves its eligibility for later drafts, even
  after an immediate send or a temporary single-profile limit; the displayed explanation is separate from that marker.
  An injected classifier retains its bounded 2.5 s path on a first send; later sends only read finished verdicts. The first answer shows the chosen model and why. While the person types, the
  draft is routed at most twice per message. An accepted send, including queueing or steering into a running answer,
  resets that conversation's count and discards its queued draft classification; other conversations keep theirs
  (done, `engine/src/engine/auto.rs`).
  Later messages never warm a spare router on keystrokes: a draft classification opens it only when it fires. Each later
  Auto message may ask the router up to twice, where previously it asked none. The router uses a thread on the shared
  Codex app-server when Luna is available and allowed, otherwise a separate Claude Haiku process for that request.
- Open hard work gets the expert bar; an unsure verdict or a blind task where quality matters adds one rung, up to frontier (intelligence 56, coding 46). Missing or unknown scope is tight. Projects use the same agent-work floor as code at every level, even for writing or research. The original difficulty still names the visible reason. A verdict never reaches summit directly; spare quota or a learned rung can lift frontier to summit, currently cleared only by Fable high. Nothing steps above summit.
- The router also says how the work can be checked: by a machine (a test, an exit code, a number to recount), at a
  glance, or blind (the person cannot tell a wrong answer from a right one). A blind medium request in code, math or
  research gets the hard bar, as math always did, because the model's own checking matters most where nobody else can
  check (done, `engine/src/route.rs`; measured in [quota-pacing.md](quota-pacing.md#cache-and-router-measured-on-13102026)).
- An answer whose last check failed, or whose code changed with nothing checking it, offers one step up. Below high,
  raise the same model one effort first. From high onward, compare that row (if present) with the next stronger model
  of the same vendor at its lowest effort that improves at least one score without lowering either. Offer the cheaper,
  then faster on a tie; only account-allowed rows qualify, ordered by measured scores. Sonnet high → Opus medium;
  Opus high → Fable medium; Opus xhigh → Fable high. It runs in a new branch (`engine/src/nudge.rs`).
- **Each later Auto message has its own effort inside the conversation's model.** A finished verdict from typing picks the
  cheapest sufficient effort using the same bars as a first message, or the held model's strongest row. No finished verdict
  means the previous effort, with no wait. Raising is immediate; reducing requires confidence at least 0.7 and a non-blind
  check, plus two consecutive matching reductions for Claude's process restart. Claude lowering is held while its session
  has background work or is not idle. Both vendors hold lowering with no resumable session (including temporary Chat); held
  reductions do not count toward the pair. Raises remain immediate. Manual choices and quota hand-overs keep their rules,
  including after steering. No finished verdict keeps the previous reason silently. Later route records stay saved, but
  their screen row appears only on an effort change or a stronger-model offer; the first answer keeps its row. Technical
  details retain kind, difficulty, scope and check. A bar beyond the held model offers the existing stronger retry in a new
  branch once per conversation, taken or ignored; later requests still use the held model's strongest row.
- **The table learns from what the person retries and keeps** (done, `engine/src/auto_learned.rs`; the owner said yes, with a
  notice). Only two things count, and only for a conversation's first answer, the one Auto itself picked (later answers keep
  its model): a retry of a finished Auto answer (the same request asked again) says the pick was not good enough, and
  counts twice when it takes the step up above; the next message of that conversation says the answer was kept. An edited request is a new request, a stopped answer may only have been slow, and a failed check is Faber's own
  reading that the person has not judged, so none of them counts, nor does a retry on a model picked by hand that is
  weaker than the one that answered. Per kind of task, over the last ten answers: three retry points that are at least half
  of them lift the kind one rung of the table's ladder (the next level's bar), and eight kept answers in a row bring it
  back down; a kind not marked for 60 days is forgotten. The lift only picks from what the table already may pick (models
  the account lists, vendors the gate admits), never from a vendor with a window past 90% or running fast (four times its daily
  line or more), and inside the time cap of a step up; when the quota already spoke (a step up, fast or low note) the table's pick stands, so the total is never more than one rung over the table and a shown change always says it is learned; otherwise the table's pick stands too. Rule 3 holds: it applies only to the
  first message of a conversation, never to a conversation under way. It lives in `auto-learned.json` in the data folder,
  never leaves the PC, and a missing or damaged file means nothing learned. The answer says so in its pace line ("Auto
  chose a stronger model here because you often retry this kind of task"), and Settings > Accounts and limits has an
  "Auto" card with one "What Auto learned" row and a Forget button.
- **Quota pacing** (built; [spec](../history/superpowers/specs/2026-10-11-auto-quota-pacing-design.md)). Auto
  reads each window against its reset time, the weekday and the plan (detected at start and watched while open, on a
  shared account), and spends quota that would expire on quality: one rung up when a vendor has spare quota.
- The router is measured too: on the benchmark of real requests, its picks must match always using the strongest
  model while spending less quota.

### Task guide and maker starting points (2026-11-11)

Difficulty follows the work, not message length; ties use the harder level. The router sees no model or maker names.
Writing includes documents, slides and spreadsheets.

- Easy: greetings and short facts; sorting, tagging, extracting and summarising supplied text; finding files and reading
  logs; one-line rewrites or renames; simple arithmetic.
- Medium: everyday documents, emails, slides and spreadsheets; translations; scoped code (a named bug, one function or
  file, a clear spec or a small script); routine investigations, reviews and explanations.
- Hard: engineering across files, unknown bugs, features across parts and large reviews; many screenshots or computer
  operation; financial models and documents from many sources; proofs; costly subtle errors in concurrency, security,
  legal, medical and money work.

Sources read **2026-11-11**: [Anthropic tasks](https://claude.dev/blog/which-claude-model-should-you-build-with/)
and [cost and intelligence guidance](https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence),
and [OpenAI models](https://developers.openai.com/codex/models) and
[reasoning](https://developers.openai.com/api/docs/guides/reasoning). Measured floors and quota gates still apply.

## Rules for driving the CLIs

These are product rules. They come from research on driving Claude Code and Codex and from the model-use notes. Each is
re-measured when a CLI version changes. Every piece of code that starts a CLI or talks to one follows them, in every
phase.

1. **Never use Codex app-server login.** Faber never calls `account/login/*` and never stores `auth.json`. Sign-in
   happens only through `codex login` or the CLI's own flow.
2. **Always pass the permission mode, the full model id and the effort when starting a CLI session.** An omitted flag is
   a hidden choice.
3. **Keep the conversation's model, tools and system prompt stable; Auto sets each message's effort inside that model.**
   The prompt cache is the biggest saving. A model change always starts a new cache; changing tools or the system prompt
   invalidates everything after it. Changing effort kept Claude's cache through `--resume` and cost Codex nothing measurable
   (13/10/2026, [quota-pacing.md](quota-pacing.md#cache-and-router-measured-on-13102026)). Facts that change travel as text
   in the next message. Auto reuses a finished draft verdict without waiting; no verdict keeps the previous effort.
   It chooses the cheapest row of the held model that clears the same level, scope and project bars, or that model's strongest
   row. It raises immediately; lowering needs confidence of at least 0.7 and a non-blind check. Claude's lowering also needs
   two consecutive messages wanting the same lower row, because it starts a new process with the same model and `--resume`.
   A busy Claude session holds reductions without counting them toward that pair. Both vendors hold reductions without a resumable session.
   Codex uses the existing manual-change path: a resumed thread on the same app-server, with effort in `turn/start`.
   Queued messages keep the verdict ready at Send for their own next turn; later drafts cannot replace it. Missing or
   pending verdicts keep the previous effort, queue edits discard the old verdict, and reopening keeps the previous effort.
   Queued or steered messages never change the running answer. Quota pacing cannot lower the held model's effort below
   the message's bar. Hand-picked model/effort and plan-limit hand-overs keep their existing rules. The answer says when
   effort rose or fell, only on that change; unchanged later routes stay silent and preserve the previous reason without a
   finished verdict. Its technical details retain kind, difficulty, scope and check. If its model cannot clear the bar, the
   completed answer offers the existing stronger retry on a new branch once per conversation when a rung is available.
   Other changes still need the person's picker, permission mode or setting, revoked or changed Codex catalog tools,
   a plan-limit hand-over (rule 4), Faber's guide arriving in a session, or Auto's one move up after an easy greeting.
   Proof: `engine/tests/auto.rs`, `a_second_message_without_a_verdict_keeps_the_process_model_and_effort` and `auto_effort_`.
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

A project agent can hand reading, building or review to a helper. Reading uses a low-cost model that clears the reading bar, with quota factors in its price; building uses the project work bar and quota factors, preferring a model no stronger than the lead. Review uses the other connected AI when possible. Economy lowers the build bar one level and retains the lower review bar. Helpers run on the connected AI when only one is available. The reply's token totals include the helpers' cached input and cache writes as well as their input and output.

Rules 1 to 9 hold today (rule 4 wherever work changes vendor: a conversation that moves to the other AI carries its earlier messages, or a summary of them, as text, `engine/src/handover.rs`, and the helper gets a written brief; rule 5's silence watch covers an answer; a tool step it reports as running, a helper's own process and a quick question are not watched; rule 7: a project agent gives an estimate in its plan, Faber counts the conversation's spend per turn from it, and past half again the estimate it warns once and switches to economy mode until the job ends, `engine/src/engine/plan.rs`, `engine/src/progress.rs`. The spend is a running total on the conversation, added to where each cost is written (a turn, a helper, a quick side question, and the same of the conversations its agent started), so it is read in one step; a cost with no known price counts 0 and a rewind takes no spend back, so the warning can come late; only an estimate sent after a costly first stretch of a turn counts that whole turn. The estimate ends only when `wrap_up` reports done). Every session start passes its mode, model and effort (the CLI probes that start no
session carry no model: Codex's lists of models, quota and threads, and Claude Code's `--help` and its `/usage` read), the engine keeps each session's start-up
`capabilities`, and every result becomes a `reason.*` key.
