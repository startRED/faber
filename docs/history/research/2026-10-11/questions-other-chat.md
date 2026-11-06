# Questions for the "faber meta-harness redesign" chat

Context to paste along with it: Faber runs the official Claude Code and Codex CLIs unmodified, with the user's subscriptions, in headless mode (stream-json for Claude, app-server for Codex). We are designing Auto with quota pacing (spec in docs/superpowers/specs/2026-10-11-auto-quota-pacing-design.md). Research of 11/10/2026 in official docs; each question carries the fact that prompted it.

## A. Cache (high priority)

1. After Faber closes the idle process (about 2 min), does Claude Code's `--resume` hit the server cache if it comes back within the TTL? There is issue anthropics/claude-code#66005: `--resume` drops `--effort` and breaks the cache. How should Faber resume to keep an identical prefix (same model, effort, tools, system prompt)?
2. Claude Code uses a 1-hour cache in the main subscription chat and a 5-minute cache in subagents and with an API key. Running on stream-json, does Faber get the 1 hour? How do we check it (`cache_creation` per TTL in `result.usage`), and when is it worth setting `promptCacheTtl` / `subagentPromptCacheTtl` (2.1.243)?
3. `result.usage` carries `cache_read_input_tokens` and `cache_creation_input_tokens`; Codex sends `cachedInputTokens` and `cacheWriteInputTokens` in `thread/tokenUsage/updated`. What is the best format to store this per message (hit rate, cost avoided) without weighing on RAM?
4. Changing only the effort keeps the cache on the 4 current Claude Code models, but it is not documented for Codex (effort is part of the prefix in the API). Is it worth testing with the real Codex whether `turn/start` with a different `effort` keeps the cache?
5. Does a ping to keep the cache alive (one read every ~55 min on Claude, ~25 min on Codex) pay off? Cache reads spend subscription quota and the account is shared. What probability of returning to the conversation justifies it?
6. After 1 hour idle and more than 100K tokens, Claude Pro/Max offers "resume from summary". Should Faber do the equivalent (summarize and restart the session) in long, cold conversations?
7. Compacting invalidates the cache. Which `autoCompactWindow` (100K to 1M) should Faber set, given that the default compacts near 967K and accuracy drops with context? And in Codex, `model_auto_compact_token_limit` (default not documented)?

## B. Quota and plan (high priority)

8. Claude's `rate_limit_event` carries `five_hour` and `seven_day` windows. The docs mention separate weekly limits per model (Fable at 50% on Max, Opus, Sonnet). Does the event carry those windows? Issue #91857 says the SDK type has no `unifiedWindows`, and #75408 says `five_hour` varies between terminals. How much should we trust this signal?
9. Version 2.1.295 started reporting in the `rate_limit_event` whether extra usage (paid credits) is on, and `statusline` has `rate_limits.spend_limit`. How does Faber detect that the subscription has turned into paid credits and never spend money without asking?
10. Codex's 5-hour window was removed in July, supposedly came back on August 25, and Pro "does not have" it today. Should Faber never assume a window and read `windowDurationMins`? There is `account/rateLimits/read` with credits, spend control and reset credits: how should `rateLimitReachedType` be used?
11. `claude auth status` gives `subscriptionType` (for example `max`) without saying 5× or 20×. Do we ask the user on first use, or let Faber measure "% per dollar"? How do we detect a plan change (the update is immediate on upgrade; downgrade is not documented)?
12. Real weights per model: Codex publishes credits per 1M tokens (Astra 250/25/1250, Sol 50/2.5/250, Luna 2.5/0.25/12.5), Claude does not. Can we measure quota points per dollar in a stable way with two people using the same account? What is the minimum sample size?
13. Subagents spend from the same quota. Should Faber limit parallelism (Claude's default is 20 concurrent subagents, depth 3) according to the quota state?

## C. Terms of use and shared account (Erick's decision)

14. Anthropic says Pro/Max "assume ordinary, individual use", that third parties may not offer claude.ai login nor "collect, store or intermediate" tokens, and that the binary must be unmodified. Does a desktop app that only starts the user's own `claude -p`, signed in through the CLI's own flow, stay within that? The 7/10/2026 notice gives monthly API credits for the SDK and `claude -p` on Max and Team: does that change anything?
15. The Consumer Terms forbid sharing the login and making the account "available to anyone else". Do two people from the same family, each signed in to their own CLI on the same subscription (Erick's case with his father), violate this? What official alternative exists (Team plans)?
16. OpenAI: the recommendation for automation is an API key; ChatGPT login in `codex exec` appears as "advanced". Faber talks to the app-server with the user's login. Is that a supported path for a third-party app? The Terms of Use returned 403 in the research and the version after 1/1/2026 was not read.
17. Codex telemetry: "off by default", but `otel.metrics_exporter` defaults to `statsig`. Is there a set of keys that cuts every data flow to OpenAI? The same for Claude's `DISABLE_TELEMETRY` etc. variables.

## D. Protocol and reliability

18. The raw stream-json control protocol (control_request/control_response) is not public. What is the risk of depending on it, and should Faber pin a minimum version or a range of Claude Code (86 versions in 3 months, patch-only `2.1.X`)? Can it use `requiredMinimumVersion`/`requiredMaximumVersion` or `DISABLE_UPDATES=1`?
19. `codex app-server` is marked "experimental, may change without notice". Should Faber check the schema (`generate-json-schema`) on every Codex version (stable releases every 2 to 5 days)? And `codex exec --json` as plan B, which has no documented schema?
20. Defaults that changed: in Claude Code, `-p` and the SDK start in auto mode in some cases (2.1.283/285); in Codex the default model is GPT-6.1 Sol. Faber already always passes permission mode, model and effort (roadmap rule 2): are there other silent defaults to pin (`CLAUDE_CODE_ENABLE_TODO_TOOLS`, MCP protocol 2026-07-28)?
21. A message sent while a turn is running: Claude queues it until the tools finish, Codex has `turn/steer`. Which behavior does Faber offer, and how do we test without spending quota?
22. Open issues that hit Faber: `-p` hanging (#89272, #78903, #91140), SSE without timeout (#33949), 900 s stall (#93155), Codex app-server crashing on Windows build 26200 (#12962; this machine uses 26200), `spawn EINVAL` on `.cmd` (#40752), `model/list` timing out at 30 s (#53197). Which ones need silence detection and restart, and which are already covered in `engine/src/cli.rs`?
23. How does Faber list models programmatically? Codex has `model/list` (with `supportedReasoningEfforts`); Claude only has `GET /v1/models` with an API key. What should we use with a subscription?

## E. Cost and context

24. Haiku 5.5 costs 5× above 100K prompt tokens and GPT-6 costs 2× on input above 272K (roadmap rule 9). Should Faber cut the router's and subagents' context at those limits and warn on screen?
25. Router: Haiku 5.5 with structured JSON output and reasoning off, or GPT-6 Luna with effort `none`? Today it uses Luna medium (p50 2.8 s). Is it worth measuring `none`/`low`?
26. Should Claude's fast mode (2× the price, only on usage credits) and Codex's Ultrafast (only on the $500 Pro) stay out of Auto and appear only as an explicit choice with a cost warning?
27. Subagents inherit the parent's model and effort. Should Faber set `model: haiku` for exploration and a smaller model in Codex (`agents.default_subagent_model`) to save quota?

## F. Visual review (roadmap gap)

28. Both CLIs accept images, neither reads native video. What is the best review kit for screens, video, multi-angle 3D and images: captures in states and sizes, frames at scene cuts, a spin of 8 to 12 angles with wireframe and normals, mechanical checks first (contrast, clipped text, loudness, closed mesh) and two-vendor judges afterwards?
29. How do we measure Faber against tl-ade fairly (same requests, blind judges, post-delivery bugs, tokens, user interventions) without spending the user's quota? What budget per round?
