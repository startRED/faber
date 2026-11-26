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

