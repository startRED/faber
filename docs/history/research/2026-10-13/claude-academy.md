# Claude Academy: what applies to Faber (2026-10-13)

Source: academy.claude.com/all (409 items: 27 courses, 124 tutorials, 157 use cases, 101 webinars), read by 9 research agents. Webinars are registration-gated, so only their landing copy was read. Several short tutorials are video-only.


## Effort / model / cost
- Effort = how hard, not what it knows. Start at default; step one level at a time; down until quality drops; up for long or hard-to-check tasks.
- Too low: stops early, re-prompt costs more. Too high: overthinks, invents tasks.
- Change model only if more effort did not fix it. After model change, reset effort to default. New model: re-test recurring tasks at lower effort.
- Effort can be set per skill/subagent frontmatter (`effort`). --effort is per-run; /effort persists; max session-only.
- Defaults: high on most models; Opus 5.5 and Haiku 5.5 default medium. medium matched default at 70-87% cost on research; low lost 1-3 pts at 1/3-1/2 cost. SWE-bench Pro Opus 5.5: medium -2.5 pts at 70% cost; low -8 pts at 1/3 cost; xhigh +1.4 at 2.5x.
- Escalation pattern: run low, re-run only failures at high: ~97% pass at $0.17 vs $0.29 all-high.
- Route by cost per solved task, not token price. Haiku 5.5 ~1/20 Opus cost on GPQA (85 vs 91%).
- max_tokens 16k truncated ~25% of Opus 5.5 hard coding attempts.
- One-line answer format cut output tokens 39%. Prompts written for older model cost 36% more on newer, no gain; trimming over-obeyed instructions cut ~1/3.
- Orchestrator+workers only for bulk work >1 context window; single chain = one model lower effort cheaper.
- Advisor tool def alone added 12-25% cost; Haiku executor consulted advisor 0/198.
- Blog: raise effort if it skipped a file, didn't run tests, didn't double-check; if wrong with right context, move up a model. Drop to smaller model after a hard stretch. Long multi-step tasks can be cheaper on bigger model (fewer steps).

## Cache
- Cache reads 0.1x (0.05x Opus 5.5); 5-min write 1.25x; 1h write 2x. Agent loops median 84% input from cache; <80% means something breaks it. Cache cut loop cost 2.7-5.3x.
- 25-token status line at front of system prompt cost more than no cache. Keep prefix byte-identical; stable first.
- Platform doc: mid-session effort or tool change breaks cache. Claude Code doc: 5.5 models keep cache on effort change (subscription/API key). CONFLICT -> measure (engine/examples/cache_probe.rs).
- Keep-alive: resend with max_tokens 0 every 4 min (API only).
- Watch cache_creation_input_tokens vs cache_read_input_tokens.

## Claude Code practice
- Explore -> Plan -> Code -> Commit. Plan mode read-only; approve plan; success criteria in plan.
- Permission modes: Manual, Accept edits, Plan, Auto (classifier blocks deploys, force push, curl|sh, exfil, destroying files; checks intent not correctness), Don't ask (unattended, auto-deny unapproved), Bypass (containers only).
- claude -p skips CLAUDE.md, hooks, skills, plugins, MCP discovery(!). --output-format json --json-schema -> structured_output. --max-turns caps loops. --bare for repeatable CI.
- Verify by exit code and JSON result, not model summary. Stop hook runs tests, exit 2 blocks completion. PostToolUse lint/typecheck. PreToolUse updatedInput replaces whole input.
- SessionStart matcher compact re-injects context after compaction. /compact <focus>. Auto-compaction can lose details.
- /goal <condition> works until evaluator confirms from transcript. Worktrees for parallel agents.
- MCP tool defs cost context even unused; >10% of window switches to tool-search mode. Skills load only name+description until used.
- Rewind does not undo shell side effects (package installs).
- CLAUDE.md advisory, degrades as it grows; name the replacement when banning something.
- Fresh-context reviewer before commit; reviewer checks unrequested changes, weakened tests, new packages, hard-coded keys.

## Skills / subagents / MCP
- CLAUDE.md always-on; skills on demand (procedures); subagents isolated context, own tools; hooks event-driven must-happen; MCP external tools.
- Skill description is the trigger; check "Reading [skill]" to confirm it loaded. allowed-tools restricts. Progressive disclosure.
- Make a skill when you explain the same thing repeatedly; build by packaging a successful run; fix by hand what you still edit and feed back.
- Subagent: delegate when intermediate work doesn't matter. Frontmatter name, description (one line), tools, model, color; "proactively" in description. Output format with stop point + "Obstacles Encountered". Minimal tools. Not for sequential pipelines or test runners needing full output. No "expert" personas.
- MCP primitives: tools model-controlled, resources app-controlled, prompts user-controlled. Before adding a layer, ask a question whose answer you know.

## API course (65 lessons)
- Eval loop: naive baseline, change one thing, keep only if score rises. Haiku generates datasets. Model grader returns strengths/weaknesses/reasoning or scores cluster at 6. Combine code grader + model grader.
- Workflows over agents when steps known. Routing = one low-temp categorization call then one pipeline; only if specialization beats extra step. Chaining with non-LLM checks between steps. Parallel reviewers + aggregator. Evaluator-optimizer loop with cap.
- "How will Claude know if this action worked?" for every action.
- Tool descriptions 3-4 sentences (what, when, returns); is_error with clear message lets model retry.
- Temperature 0-0.3 for classification.

## Product / UX (Cowork, Chat, Projects)
- Delegation: describe, ask for plan, review, approve, monitor, check. Clarifying questions as clickable options + free text. Steer mid-run ("redirects are cheap").
- Do task once manually, confirm, then /schedule. Each run fresh session; missed runs run on return; local tasks need app open; drafts for review.
- Instructions global/project/org; general rules -> instructions, repeatable process -> skill.
- /setup-claude: pick role, install matching plugin, connect tools; rerunnable banner.
- Dedicated working folder; ask before send/post/delete; bounds (file count, date range, draft only).
- Web search default on Pro/Max. Research = many searches, cited brief, minutes.
- Human-agent teams: rubric for every agent task; widen permissions only after consecutive successes per task type; weekly retro.

## Trust / fluency
- Trust is a dial matched to stakes. Per-claim source quotes; flag unsourced. Fresh agent reviews vs original plan; split producer/checker.
- Sycophancy: ask for weaknesses neutrally, strongest counterargument, rephrase to test stability, fresh chat when drifting.
- Wrong answer: check missing context before blaming model. Long threads degrade near limit; start fresh. Restate constraints after compaction.
- Acceptance tests before code; intent file (goal, rationale, constraints) version-controlled.

## Remaining courses
- SDLC playbook: intent.md template (problem, outcome, users, constraints, open questions) as skill; plan names files, order, proving tests; CLAUDE.md under one page, add a correction when Claude errs twice; skills advisory -> pair must-hold rules with hooks; 2-3 parallel sessions each own worktree; single check command exits non-zero; failing test first, forbid editing it, "fix the code, not the test"; 20-50 real tasks as evals nightly and on any change to CLAUDE.md/skills/hooks; each incident becomes permanent eval; REVIEW.md 3 passes, nits capped at 5; agents act up to production gate, PreToolUse fails closed; control band mean +/- sigma, 1 sigma log, 2 diagnose read-only, 3 propose PR.
- MCP advanced: SDK does not enforce roots -> check paths in every file tool; stdio for same-machine; stateless_http breaks progress/sampling.
- Platform 101: Haiku classify/route, Sonnet most, Opus deep; escalate Haiku->Sonnet->Opus on eval of 20-30 real examples; compare by changing only model. Skip thinking for classification. MCP connector default disabled + per-tool enable for read-only.
- AI Fluency 4Ds: Delegation, Description (Product, Process, Performance), Discernment, Diligence. Diligence statement.
- Teaching: assess outcome + process + reflection; linear path for beginners.
- Enterprise spend: caps org/group/member; at cap pause without queue; start tight, re-base on first month; tiers light/standard/power; cap effort per model; cap increase = "a prompt to look, not an automatic yes"; write tools Always allow / Needs approval / Blocked.

## API course extra
- Model grader without strengths/weaknesses clusters at 6. Prefill fence + stop sequence for JSON. Temperature 0-0.3 for classification. Hybrid retrieval RRF k=60, BM25 for IDs.

## Webinars (landing copy only; videos gated)
- Changing model, effort or fast mode can break cache: change at deliberate moments (Maximizing Value).
- Filter noisy command output; send high-output jobs to subagents; /context to trim CLAUDE.md and MCP.
- Cost per completed task (per merged PR) metric; baseline, change one thing, re-measure.
- Evals: evaluate tool calls and intermediate steps; seed from real failures; decide model upgrades.
- Loops need done condition, budget, checkpoint.
- Warp: agents learn from human corrections by updating their own skills (explicit, reviewable); filter which feedback to learn.
- CodeRabbit: AI code 1.7x more issues, mostly intent mismatch; planner validates plans before coding.
- Advisor (cheap executes, strong directs), orchestrator, eval-gated routing.
- Opus plans / Sonnet executes split.
- Auth trap: OAuth vs API keys, persistent env vars conflict silently.

## Use cases (157)
- Folder as workspace; /schedule recurring on new file in folder; skill from successful run as slash command; calibrate on 3-5 known past cases before trusting.
- Prompt skeleton: context inputs (named files), ordered steps, exact output spec (length, sections, audience), constraints, suffix "flag anything you're not confident about".
- Output shapes: one-page brief, G/Y/R table with driving signal, cited memo, decision record appended to decision-log.md, redline table.
- Safety: draft-only, ask before delete even in act-without-asking, plan preview before touching, open the source not the summary, recount one number from raw data, data-handling line (local vs cloud).
- Effort mentions rare: "for complex, interdependent variables, raise the effort level".

## Tutorials (87 more)
- Fluency index: 85.7% iterate; iterative = ~2x fluency behaviors. Artifact effect: polished outputs -> users evaluate less (missing-context checks -5.2 pts, fact-check -3.7). Only 8.7% verify facts; only 30% set interaction terms ("push back, explain reasoning, flag uncertainty").
- Skill through conversation: describe, Claude interviews, writes SKILL.md, packages, test task, check "Using [skill]", revise. Single-purpose skills listing data sources and outputs.
- Say deliverable and format explicitly. Describe outcome + how success is judged; leave steps to Claude. Start small batch before scaling. Replace vague words with numeric thresholds.
- Verification: rushed inputs (decimals, empty, very long), one hand-worked example, second pass checking first answer, live formulas.
- Scope edits to a selection. Corrections saved as memory so miss doesn't recur. Ask for 2-3 variants + empty/error/loading states.
- Browser use good for named sites/forms (pause before submit), poor for open research.
- Rollout pilot then staged features; spend limits default $0 on usage plans; at limit work is declined.
