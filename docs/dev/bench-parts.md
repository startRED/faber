# Paired project parts

Part of the [light benchmark](benchmark.md).

Preview the three-project plan before starting any model work.

```bash
cargo run -p faber-engine --example bench -- --dry-run --parts all --only organize-folder,todo-app,fix-invoice --vendors codex
```

Use `--parts all` or a comma-separated list of switch names. Both conditions use the same one `--vendors` value, fresh copies of the same fixture, and the same request. One condition leaves all parts on. The other changes one in-memory engine option. Trial order alternates across the plan, and the judge gets a separate alternating A/B order. The judge always runs on the other vendor. If delegation obscures who contributed, the verdict shows that independence is unconfirmed.

The plan estimates spend at the configured per-invocation allowance for two trials and one judgment per pair, before the overall cap. The run can stop early at the cap or if cost becomes unknown. The dry run only reads local request files. The paid run writes `results.json` and `tables.md` with wall time, fresh input including cache writes, cached input reads, output, extra model calls and the revealed verdict for each condition. Tokens cover the recorded replies. Extra calls are engine-started invocations beyond the first foreground reply, including auxiliary turns, reviews, helpers and continuations. Provider-internal requests, native subagents and the separate benchmark judge are outside that count.

| Switch | What Faber adds to a project turn |
| :--- | :--- |
| `persona` | Appends language and reply rules, project workflow and evidence instructions, and conditional team and assumption guidance beside the native instructions. [Persona](../../engine-drivers/src/persona.rs), [Claude arguments](../../engine-drivers/src/claude.rs), [Codex thread](../../engine-drivers/src/codex.rs). |
| `guide` | Offers the app guide and first-party craft and proof skills through a plugin, file index or inline fallback. [Delivery](../../engine/src/engine/kits.rs), [guide and skills](../../engine-projects/src/guide.rs). |
| `start-card` | Supplies bounded project facts, kept decisions and approved taste notes at session start. [Preparation](../../engine/src/engine/mcp_servers.rs), [card](../../engine-store/src/project_rules.rs). |
| `checks` | Offers a self-check when code changes lack proof and supplies the verifier tool for declared project checks. The offer requires a click and adds no call in an unattended pair. The fixture proof runs on both sides. [Offer](../../engine-core/src/nudge.rs), [screen action](../../ui/Thread.tsx), [verifier](../../engine/src/engine/verifying.rs). |
| `review` | Offers the review tool, which can start two reviewers in a round and repeat for blocking findings. Review via the general helper remains available. [Review](../../engine/src/engine/review.rs). |
| `router` | Lets Auto ask a small model to classify the request before applying the model table. The normal unclassified fallback can pick a different model or effort. [Auto](../../engine/src/engine/auto.rs). |
| `title` | Optionally asks a model to name the conversation after its first successful reply. Both sides start with a local provisional title. [Title](../../engine/src/engine/title.rs). |
| `ask-judge` | Reads eligible first requests for material open questions or visual direction, then may add a note to the main input. Parts mode opts into the same judge the app enables. [First-request judge](../../engine/src/engine/clarify.rs). |
| `follow-ups` | Sends another turn when the closing box leaves work, a conclusion needs repair, or a background command finishes. [Carry on](../../engine/src/engine/carry_on.rs), [conclusion](../../engine/src/engine/ended.rs), [command wake](../../engine/src/engine/command_wait.rs). |
| `delegation` | Offers Faber helpers, model assignments for native subagents, and agent-started conversations. The CLI still has its native subagents and the review tool has a separate path. [Helpers](../../engine/src/engine/helpers.rs), [conversations](../../engine/src/engine/siblings.rs). |
| `creative-judge` | Offers an agent-invoked comparison of creative alternatives, which can start two isolated model calls. [Creative judging](../../engine/src/engine/creative_judges.rs). |

Other launch differences include local progress, decisions, project map and catalog tools, catalog resource selection, a separate working copy, and policies for permissions and protected paths. The app can add browser actions when a browser host is attached. These mechanisms can add text or local work. User-driven paths such as a side question, manual compaction or approval, a goal, a pull request wake or Keep going require state outside these fresh requests. [Project launch](../../engine/src/engine.rs), [tool delivery](../../engine/src/engine/kits.rs), [built-in tools](../../engine-drivers/src/helper.rs), [background returns](../../engine/src/engine/by_itself.rs).

## Measured on 2026-11-26

One request (`organize-folder`), Codex on both sides, one pair per part, alternating order, judged blind by the other
vendor. Spent US$ 3.54 in all. One pair is one sample: a verdict here is a lead to check, not a reason to cut a part. The
three-request set, estimated at US$ 148.50, is out: the owner chose on 2026-11-28 cheap requests repeated three times.

| Part | Time on / off (s) | Input on / off | Output on / off | Extra calls on / off | Judge picked |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `persona` | 86 / 108 | 38674 / 39158 | 1915 / 2180 | 3 / 4 | off |
| `guide` | 86 / 88 | 38466 / 39614 | 1722 / 2322 | 3 / 3 | off |
| `start-card` | 75 / 58 | 39244 / 34878 | 2045 / 1038 | 3 / 3 | on |
| `checks` | 54 / 66 | 35320 / 34937 | 1013 / 1026 | 3 / 3 | on |
| `review` | 82 / 109 | 38730 / 40568 | 1689 / 2773 | 3 / 3 | off |
| `router` | 55 / 80 | 17463 / 38700 | 949 / 2141 | 3 / 2 | off |
| `title` | 87 / 75 | 39511 / 40329 | 2240 / 2036 | 3 / 2 | off |
| `ask-judge` | 65 / 55 | 34908 / 34985 | 1042 / 1069 | 3 / 2 | on |
| `follow-ups` | 55 / 69 | 34092 / 19851 | 970 / 1705 | 3 / 3 | off |
| `delegation` | 68 / 100 | 35106 / 37663 | 1008 / 2682 | 3 / 3 | off |
| `creative-judge` | 74 / 90 | 18288 / 39265 | 1207 / 2026 | 3 / 3 | off |

The judge picked the side with the part on for 3 parts (`start-card`, `checks`, `ask-judge`) and the side without it for the
other 8. Input excludes cached reads; time is wall seconds for the whole turn. Raw rows: run `parts-1`.
In 8 of the 11 pairs the picked side wrote the longer answer, so a length bias in the judge is a likely part of these
verdicts; the next run should hold the answer length in view before reading a part as worth cutting.

## Measured on 2026-11-28

Three more runs of eight parts on `organize-folder`, Codex on both sides, judged blind by Claude: US$ 7.04 (runs
`parts-112-1` to `-3`). They showed why one pair says little. Auto picks the model per trial, and on this one request it
picked `gpt-6.1-terra` for some trials and `gpt-6.2-sol` for others, so 14 of the 24 pairs ran the two sides on different
models. In all 14 the side on `gpt-6.2-sol` won, and the side with the longer answer won 19 of 24. The verdicts above
and in these runs measure the model Auto happened to pick more than the part.

`--parts-model model@effort` now runs both sides of every pair on that model. The `router` pair stays on Auto, because
choosing the model is that part (`bench_pure::tests::a_pinned_model_holds_both_trials_of_a_pair_but_the_router_s`).

Three pinned runs on `gpt-6.1-terra@medium` of the parts whose cut looked like it saved time, US$ 2.03 (runs
`parts-112-pin-1` to `-3`). Median and [lowest-highest] of three; input includes cache writes.

| Part | Time on / off (s) | Input on / off | Output on / off | Extra calls on / off | Judge picked | Proofs |
| :--- | ---: | ---: | ---: | ---: | :--- | :--- |
| `review` | 54 [51-57] / 53 [51-54] | 17901 / 35292 | 1199 / 1192 | 1 / 1 | on, off, off | all pass |
| `delegation` | 54 [50-55] / 51 [50-57] | 23117 / 32881 | 1140 / 1117 | 1 / 1 | off, on, off | all pass |
| `creative-judge` | 52 [49-59] / 55 [46-56] | 18005 / 35451 | 1132 / 1318 | 1 / 1 | on, on, on | all pass |

No part was cut. `creative-judge` won every verdict; `review` and `delegation` each lost one with the part off, and their
time differences sit inside the spread. The side without a part writes more fresh input, most likely because its tool list
differs from the one earlier trials left in the provider's cache; that is a cost of the measurement, not of the part.

Two findings are the owner's to decide, because they change what a person gets:

- `router`: with it off, Auto ran `gpt-6.2-sol` every time and the judge preferred that side 3 of 3, while each turn took
  about 15 s longer (median 71 s against 56 s, one model call fewer).
- `title`: off saves one model call per conversation; the judge preferred the side with it on 2 of 3, on different models.
