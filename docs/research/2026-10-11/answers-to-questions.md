# Answers to the 29 questions ("faber meta-harness redesign" chat)

Research date: 2026-10-11. Sources: the notes in this folder (r2-*, r3-*, r4-*) plus official pages re-checked today where marked. Tags: **confirmed in official docs**, **inferred**, **unknown**.

Verified today (WebFetch): consumer terms (anthropic.com/legal/consumer-terms), Claude Code legal page (code.claude.com/docs/en/legal-and-compliance), Anthropic vision doc (platform.claude.com/docs/en/build-with-claude/vision), Codex auth page (learn.chatgpt.com/docs/auth), Codex config reference (learn.chatgpt.com/docs/config-file/config-reference). OpenAI Terms of Use returned HTTP 403 again and were not read.

Note on sources: items marked "snippet" or "issue" come from search results or GitHub issues, not from official docs.

---

## A. Cache

**1. `--resume` and the cache after a 2-minute pause**
Yes, if the resume comes back inside the TTL and the prefix is identical. The 2-minute gap is well inside the 5-minute and 1-hour windows. The prefix must match exactly: same model, same effort, same tools, same system prompt. Issue #66005 reports that `--resume` drops `--effort`, so Faber should pass `--model` and `--effort` explicitly on every resume and check `cache_read_input_tokens` on the first turn. The system prompt is recorded on the first request and reused on resume, so `--system-prompt-snapshot` is relevant.
Sources: https://code.claude.com/docs/en/prompt-caching ; https://code.claude.com/docs/en/sessions ; https://github.com/anthropics/claude-code/issues/66005 (issue, not official)
Tag: inferred (TTL and prefix rules confirmed; the `--effort` drop is an open issue)

**2. TTL on stream-json, and when to set `promptCacheTtl`**
The TTL is chosen by the CLI, not by the output format. On a subscription, within plan usage, the main conversation gets 1 hour by default. It drops to 5 minutes once usage credits, an API key, or a cloud provider is in use. Subagents, forks, compaction and titles get 5 minutes even on a subscription. To check, read `usage.cache_creation` (`ephemeral_1h_input_tokens` / `ephemeral_5m_input_tokens`) in the stream. Leave the default on subscriptions. Set `promptCacheTtl: "1h"` only when a long session could be billed as API or credits, because the 1-hour write costs 2x against 1.25x. Set `subagentPromptCacheTtl` only if subagents resume after more than 5 minutes with large context. The docs say `promptCacheTtl` needs v2.1.242+, while the changelog dates it to 2.1.243.
Sources: https://code.claude.com/docs/en/prompt-caching ; https://code.claude.com/docs/en/settings-reference (name only, not opened) ; https://code.claude.com/docs/en/changelog (via notes)
Tag: confirmed in official docs

**3. Storing cache numbers per message**
Store one small record per turn, not per token: model, effort, input, cache read, cache write 5m, cache write 1h, output. Hit rate is `cache_read / (input + cache_read + cache_write)`. Codex sends the same concepts in `thread/tokenUsage/updated` as `last` (`inputTokens`, `cachedInputTokens`, `cacheWriteInputTokens`, `outputTokens`). Keep only the last turn, not the running total, to avoid RAM growth.
Sources: https://code.claude.com/docs/en/agent-sdk/streaming-output ; https://learn.chatgpt.com/docs/app-server (generated schema, local)
Tag: confirmed in official docs (field names); the storage layout is inferred

**4. Effort change and the cache**
Claude: confirmed. On Opus 5.5, Sonnet 5.5, Haiku 5.5 and Fable 5.1, changing effort keeps the cache in Claude Code when using an API key or a subscription. It does not hold on Bedrock, Vertex, Foundry, a gateway, or with `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`. Codex: unknown. OpenAI's reasoning guide says `configuration_update` changes effort "without invalidating the cache", but that is a Responses API mechanism, not `turn/start`. Test on GPT-6 Luna (cheap) with a fixed prefix above 1,024 tokens: run effort `low`, then `medium` on the same prefix, and compare `cachedInputTokens`.
Sources: https://code.claude.com/docs/en/prompt-caching ; https://developers.openai.com/api/docs/guides/reasoning ; https://developers.openai.com/api/docs/guides/prompt-caching
Tag: unknown (Codex part); Claude part confirmed in official docs

**5. Keep-alive pings**
Recommendation: do not ping. The Claude docs say cache reads "still draw usage for the whole conversation", so each ping spends a full-context read of the plan. How plan limits weight cached tokens is not published. A ping pays only when the user is very likely to return within the hour and the context is large (over ~100K). Otherwise the cold re-read costs less than repeated pings.
Sources: https://code.claude.com/docs/en/prompt-caching ; https://code.claude.com/docs/en/costs ; https://developers.openai.com/api/docs/guides/prompt-caching
Tag: confirmed in official docs (cache reads draw usage); the break-even is inferred

**6. "Resume from summary" for long cold conversations**
The CLI already offers this in interactive use: after more than about 1 hour idle and over 100K tokens, it asks "Resume from summary" (runs `/compact`), "Resume full session as-is", or "Don't ask me again". Whether that dialog appears under `-p` is not stated. Recommendation: Faber should ask the user, not summarize silently, because a summary replaces history and the next turn rebuilds the cache.
Sources: https://code.claude.com/docs/en/sessions ; https://code.claude.com/docs/en/costs
Tag: confirmed in official docs (the dialog exists); behaviour under `-p` unknown

**7. Auto-compact window**
Claude: the setting accepts 100K to 1M; with no value set, native 1M models compact at about 967K. Anthropic says accuracy degrades as context grows ("context rot"), but gives no recommended number. Suggested test value: 200K for routine work, measured, not assumed. Codex: `model_auto_compact_token_limit` has no published default ("model defaults"). Suggested test value: 272K, which keeps GPT-6 requests below the API long-context surcharge threshold. Codex credit billing states no long-context multiplier, so this matters for the API, not for subscription credits.
Sources: https://code.claude.com/docs/en/model-config ; https://platform.claude.com/docs/en/build-with-claude/context-windows ; https://learn.chatgpt.com/docs/config-file/config-reference ; https://developers.openai.com/api/docs/pricing
Tag: inferred (range and rot confirmed; the numbers are test values)

## B. Quota and plan

**8. `rate_limit_event` windows and their reliability**
The stream event carries `rate_limit_info` with `status`, `resets_at`, `rate_limit_type`, `utilization`, `overage_status`, `overage_resets_at`. The statusline carries `rate_limits.five_hour` and `rate_limits.seven_day` with `used_percentage` and `resets_at` (Pro and Max only, after the first response). Per-model weekly rows (Fable, Opus, Sonnet) appear in `/usage` (v2.1.261), not clearly in the stream event. Issue #91857 says the event carries per-bucket `unifiedWindows` that the SDK type does not declare; issue #75408 says `five_hour` drifts across terminals. Use the signal as advisory only, and show "unknown" when it is missing or stale.
Sources: https://code.claude.com/docs/en/statusline ; https://code.claude.com/docs/en/agent-sdk/streaming-output ; https://code.claude.com/docs/en/costs ; https://github.com/anthropics/claude-code/issues/91857 (issue) ; https://github.com/anthropics/claude-code/issues/75408 (issue)
Tag: inferred

**9. Detecting that the subscription is now paying credits**
The stream event has `overage_status` and `overage_disabled_reason`, and changelog 2.1.295 says headless warnings now say whether extra usage is on. The statusline `spend_limit` object exists only behind a gateway, not for direct Pro and Max. Official behaviour: once credits are on, requests "switch to pay-as-you-go credits and keep working", so the CLI will spend without asking. Faber should therefore: treat any `overage_status` that is not off as a stop; stop on the spend and credit error strings ("monthly spend limit", "Credit balance is too low", "Now using credits"); and rely on the account setting being off as the only hard guard. The exact status values were not captured.
Sources: https://support.claude.com/en/articles/12429409-extra-usage-for-paid-claude-plans ; https://code.claude.com/docs/en/agent-sdk/streaming-output ; https://code.claude.com/docs/en/changelog (via notes)
Tag: inferred

**10. Codex five-hour window**
Do not assume a window. Read `windowDurationMins` and `resetsAt` for each window in `RateLimitSnapshot`, and treat a missing window as unknown. The pricing page says Plus has five-hour windows and "Pro plans currently have no five-hour limit"; changelog 0.153.0 warns when Plus or Team is under half of its ~5-hour allowance; the July removal and August restoration are unresolved. Use `rateLimitReachedType` (`rate_limit_reached`, `workspace_*_credits_depleted`, `workspace_*_usage_limit_reached`) to explain a stop, and `spendControlReached` for spend caps. Reset credits exist (0.144.0) but the help article returned 403, so only a snippet is available.
Sources: https://learn.chatgpt.com/docs/pricing ; https://learn.chatgpt.com/docs/app-server ; https://github.com/openai/codex/releases (0.144.0, 0.153.0)
Tag: confirmed in official docs (schema and pricing); the window history is unknown

**11. Plan from `claude auth status` and tracking % per dollar**
The notes do not show a `subscriptionType` field, so whether `claude auth status` reports it is unknown; do not depend on it. Recommendation: ask the plan once at first use and let the limit signal confirm it. A "% per dollar" measure is not sound on a subscription, because `total_cost_usd` is a client-side API estimate, not plan usage. Plan changes: an upgrade (Pro to Max) applies immediately with proration; downgrades are not documented, so re-check the plan when the limit numbers stop matching.
Sources: https://support.claude.com/en/articles/11049752-how-do-i-sign-up-for-the-max-plan ; https://code.claude.com/docs/en/agent-sdk/streaming-output
Tag: unknown

**12. Points per dollar from two people on one account**
Codex publishes credit rates per 1M tokens (Astra 250/25/1,250; Sol 50/2.5/250; Luna 2.5/0.25/12.5). Claude publishes no plan weights. Two people on one account blur the measurement, so only separate accounts give clean numbers (see decision a). No official method or sample size exists. A pilot of identical, repeated runs per model is a reasonable start, but the minimum size is not known.
Sources: https://learn.chatgpt.com/docs/pricing ; https://code.claude.com/docs/en/costs
Tag: unknown

**13. Limiting subagent parallelism by quota**
Confirmed defaults: up to 20 concurrent Claude subagents (`CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS`) and nesting depth 3 (`CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH`); subagent requests count against the same limits. Codex does not publish its concurrency default or depth. Recommendation: Faber should cap concurrency itself (2 to 3 is a sensible start) and lower it when the quota signal is low. Issue #87106 reports that resumed subagents replay the full transcript and used 75% of a weekly limit in one afternoon.
Sources: https://code.claude.com/docs/en/sub-agents ; https://learn.chatgpt.com/docs/agent-configuration/subagents ; https://github.com/anthropics/claude-code/issues/87106 (issue)
Tag: confirmed in official docs (defaults); the cap of 2 to 3 is inferred

## C. Terms and shared accounts

**14. Is the desktop wrapper inside the Claude terms?**
Partly protected. The Claude Code legal page says developers may not "offer Claude.ai login into their own applications", nor "route requests through Free, Pro, or Max plan credentials on behalf of their users", nor "collect, store, or intermediate" credentials or session tokens. It also says sign-in must complete through Anthropic's flow and that this does not prevent "an end user from signing in to the unmodified Claude Code binary with their own Claude subscription". Faber's design (unmodified binary, the user's own sign-in, Faber never touches tokens) is the case the page describes, but it does not say whether an app that launches `claude -p` counts as the excluded case. The 7/10/2026 notice (monthly API credits for Max and Team covering `claude -p` and the SDK) is a sign that `claude -p` use is contemplated, but the support article is only seen through a summary and conflicts with the older "credits cannot be used in Claude Code" text. Get written confirmation before public release.
Sources: https://code.claude.com/docs/en/legal-and-compliance ; https://code.claude.com/docs/en/agent-sdk/overview ; https://support.claude.com/en/articles/15036540 (summary only) ; https://support.claude.com/en/articles/11145838-using-claude-code-with-your-pro-or-max-plan
Tag: unknown

**15. Father and Erick on one subscription**
The Consumer Terms say: "You may not share your Account login information, Anthropic API key, or Account credentials with anyone else." and "You also may not make your Account available to anyone else." Two people on one subscription, each signed in to their own CLI, is the same account used by two people, so it very likely breaches these terms. Two separate subscriptions are fine. The official alternative is a Team plan with one seat per person (standard and premium seats exist; minimum seat count is unresolved). No family plan was found in the notes.
Sources: https://www.anthropic.com/legal/consumer-terms ; https://claude.com/pricing ; https://code.claude.com/docs/en/legal-and-compliance
Tag: inferred (quotes confirmed; application to this case is inferred)

**16. Codex and the ChatGPT login used by a third-party app**
OpenAI's auth page says: "Use API key authentication for programmatic Codex CLI workflows, such as CI/CD jobs." and "Access tokens are intended for trusted scripts, schedulers, and private CI runners." The app-server docs say the app server is for "build[ing] custom clients that handle authentication", which points toward a client using the user's login, but no official text explicitly allows a third-party desktop app to drive it with the user's ChatGPT sign-in. The Terms of Use returned 403 today; the older text forbids sharing credentials or making the account "available to anyone else". Recommendation: until OpenAI confirms in writing, keep API-key mode as the default for Codex in Faber.
Sources: https://learn.chatgpt.com/docs/auth ; https://learn.chatgpt.com/docs/non-interactive-mode ; https://learn.chatgpt.com/docs/app-server ; https://openai.com/policies/terms-of-use (403)
Tag: unknown

**17. Cutting telemetry**
Codex: the config reference says `analytics.enabled` "When unset, the client default applies"; `otel.metrics_exporter` defaults to `statsig`; `feedback.enabled` defaults to true. Setting `analytics.enabled = false`, `feedback.enabled = false`, and `otel.exporter`, `otel.trace_exporter`, `otel.metrics_exporter` to `"none"` removes the documented flows, and `history.persistence = "none"` stops local transcripts. No single key was found that cuts all OpenAI-bound data; the "off by default" statement conflicts with the statsig default. Claude: set `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`, `DISABLE_FEEDBACK_COMMAND=1`, `CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY=1`; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` also disables feature flags and may make Remote Control unavailable; the WebFetch hostname check stays unless `skipWebFetchPreflight: true`. Side effect: with telemetry off, `-p` starts in auto mode unless a permission mode is set (see item 20).
Sources: https://learn.chatgpt.com/docs/config-file/config-reference (verified today) ; https://learn.chatgpt.com/docs/security ; https://code.claude.com/docs/en/data-usage
Tag: confirmed in official docs (keys); "cuts all flows" is unknown

## D. Protocol and reliability

**18. Relying on the undocumented control protocol; pinning the Claude version**
The stdin control_request/control_response protocol is not public, so depend only on the public surface: `--input-format stream-json` user messages, hooks, `--permission-prompt-tool`, and `--permission-prompts none`. Pinning options: `claude --version` checked at startup against a tested range (the notes list version floors: Opus 5.5 2.1.280, Sonnet 5.5 2.1.284, Haiku 5.5 2.1.293); `DISABLE_UPDATES=1` blocks all update paths (env, since 2.1.118); `DISABLE_AUTOUPDATER=1` stops only the background check; `minimumVersion` is a floor; `requiredMinimumVersion` and `requiredMaximumVersion` are managed settings, so whether a user-level setting can use them is unknown. Recommendation: check the version at startup and refuse untested versions, and do not disable updates for the user's global CLI.
Sources: https://code.claude.com/docs/en/headless ; https://code.claude.com/docs/en/setup ; https://code.claude.com/docs/en/env-vars ; https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md
Tag: inferred

**19. Codex app-server versus `codex exec --json`**
The docs say `codex app-server` is experimental and "may change without notice", and the app-server README says it is not supported for production workloads. Recommendation: regenerate the schema with `codex app-server generate-json-schema` on each Codex release in CI, diff it, and fail on change; pin the tested range. `codex exec --json` is a weaker fallback: its event types are only partly listed, there is no schema, and open issues #50586 to #50596 report missing usage, compaction and model-request events, and #53017 reports drifted TypeScript types.
Sources: https://learn.chatgpt.com/docs/developer-commands ; https://learn.chatgpt.com/docs/app-server ; https://github.com/openai/codex/issues/53017 (issue) ; https://github.com/openai/codex/issues/50586 (issue)
Tag: confirmed in official docs (experimental label); the schema workflow is inferred

**20. Silent defaults to pin**
Claude Code: `-p` and the SDK start in auto mode when no permission mode is set, on third-party providers or with telemetry off (2.1.283, 2.1.285); pass `--permission-mode`, `--model` and `--effort` on every run. Task tools (TaskCreate/TodoWrite) were removed on Opus 4.8, Sonnet 5 and newer (2.1.233, then 2.1.268); `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` restores them only if Faber's parser needs them. MCP protocol 2026-07-28 is negotiated by default for local stdio (2.1.292); `MCP_PROTOCOL_NEGOTIATION=legacy` opts out, so pin only if an MCP tool breaks. Saved effort from before per-model `/effort` no longer applies to newer models (2.1.280). Codex: fresh sessions respect the server's model default unless overridden (0.154.0), so pass `model` explicitly; the default is now GPT-6.1 Sol in the bundled catalog (0.159.1, 0.161.0), while the models page still says GPT-6 Sol; unknown TUI config keys now fail startup (0.162.0), so Faber must not write stale keys; `update_plan` is off by default (0.152.0).
Sources: https://code.claude.com/docs/en/permission-modes ; https://code.claude.com/docs/en/model-config ; https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md ; https://github.com/openai/codex/releases
Tag: confirmed in official docs

**21. Sending a message while a turn runs**
Claude TUI: a message sent mid-turn is passed "as soon as those tool calls finish, within the same turn"; `Ctrl+Enter` (2.1.275, 2.1.281) sends now. SDK streaming input mode queues messages in order with interrupt available; calling `query()` during a running turn is undocumented. Codex: `turn/steer` appends input to the active turn and needs a matching `expectedTurnId`; it fails if no turn is active. Recommendation: queue on Claude, steer on Codex, and interrupt only when the user asks. Test on the cheapest model (Haiku 5.5 low effort, Luna low effort) with a trivial prompt that runs one short tool call, measuring whether the second message is folded in or waits.
Sources: https://code.claude.com/docs/en/interactive-mode ; https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode ; https://learn.chatgpt.com/docs/app-server
Tag: inferred (test design; the behaviours are confirmed in docs)

**22. Open issues and what the engine covers**
Checked by grep of `engine/src`. Covered: `.cmd` shims are resolved without a shell (`cli.rs`, the #40752 class); children are closed then killed on stop (`proc.rs`); account checks and `model/list` have timeouts (`account.rs`, `codex.rs`), which partly covers #53197. Not covered: no silence watchdog for an open turn was found. Claude needs one for #33949 (no timeout), #93155 (900 s silent stall), #100353 (keep-alive lost after one error), #88900 and #91502 (stalls), #46987 (stream idle), and #91982 (no user frame for cross-session messages on stream-json). Codex exec needs one for #50775, #51258, #31376, #34397 and #50141 (stays alive after `turn.completed` on Windows). Codex app-server crashes on build 26200 (#12962) need restart logic; none was found. #89272 and #78903 are macOS-only (inferred not to apply on Windows); #27019 (stdin hang) is avoided by passing the prompt as an argument.
Sources: https://github.com/anthropics/claude-code/issues/33949 ; https://github.com/anthropics/claude-code/issues/93155 ; https://github.com/openai/codex/issues/12962 ; https://github.com/openai/codex/issues/50775 (all issues)
Tag: inferred

**23. Listing models programmatically**
Codex: `model/list` on app-server returns `supportedReasoningEfforts`, `defaultReasoningEffort`, `isDefault` and a cursor; the engine already calls it with a timeout (`codex.rs`). Claude: no headless listing command is documented; `GET /v1/models` needs an API key (`X-Api-Key`), and whether subscription login can call it is unknown. Recommendation: keep a curated Claude list with the documented effort levels per model and version floors, and treat `[claude-code:unrecognized_model]` on stderr as the check.
Sources: https://learn.chatgpt.com/docs/app-server ; https://platform.claude.com/docs/en/api/models/list (via notes) ; https://code.claude.com/docs/en/model-config
Tag: confirmed in official docs

## E. Cost and context

**24. Cutting context at the price steps**
Yes for API-priced routes. Haiku 5.5 goes from $0.10 to $0.50 input per MTok above 100K, and the prompt size includes cache reads and writes, so a long cached prefix also triggers it. GPT-6 family input and cached input double, and output rises 1.5x, for the whole request above 272K. Keep router and subagent prompts under those limits and warn on screen. On subscriptions the effect on plan usage is not published.
Sources: https://platform.claude.com/docs/en/about-claude/pricing ; https://developers.openai.com/api/docs/pricing ; https://platform.claude.com/docs/en/build-with-claude/context-windows
Tag: confirmed in official docs (prices); the subscription effect is unknown

**25. Router on Haiku 5.5 or Luna**
Both support structured output. Haiku 5.5 allows `thinking: {type: "disabled"}` at effort `low`. Luna supports effort `none`, and OpenAI lists classification as a `none` use case. Prices per MTok are the same for short prompts ($0.10 input, $0.50 output). Measure `none` and `low` on Luna against the current `medium` (p50 2.8 s) on the real router prompts; the latency numbers are Faber's own, not official.
Sources: https://developers.openai.com/api/docs/guides/reasoning ; https://platform.claude.com/docs/en/build-with-claude/structured-outputs ; https://developers.openai.com/api/docs/pricing
Tag: confirmed in official docs (capabilities and prices); latency is measured by Faber

**26. Fast and Ultrafast modes in Auto**
Claude Fast mode is 2x the API price and draws only usage credits, never plan limits. Codex Ultrafast is 8x the included usage and is available only on Pro $500 and eligible Enterprise or Edu plans. Recommendation: keep both out of Auto and offer them only as an explicit choice with a cost warning.
Sources: https://code.claude.com/docs/en/fast-mode ; https://learn.chatgpt.com/docs/pricing ; https://learn.chatgpt.com/docs/agent-configuration/speed
Tag: confirmed in official docs

**27. Cheaper subagents**
Claude: `model: haiku` in the subagent frontmatter, or `CLAUDE_CODE_SUBAGENT_MODEL` (2.1.251). Built-in Explore runs the main model, not Haiku, unless the main model is Fable. Codex: `agents.default_subagent_model` and `agents.default_subagent_reasoning_effort`; subagents inherit the parent by default. Recommendation: use Haiku or GPT-6 Luna only for read-only exploration agents, and keep implementation on the parent model; the quality cost is a trade-off to test.
Sources: https://code.claude.com/docs/en/sub-agents ; https://code.claude.com/docs/en/model-config ; https://learn.chatgpt.com/docs/agent-configuration/subagents
Tag: confirmed in official docs (settings); the choice is a recommendation

## F. Visual review

**28. Visual review kit**
Confirmed: Claude reads only JPEG, PNG, GIF and WebP images, with animations reduced to the first frame; there is no video input and no image generation. Image limits: standard tier 1,568 px long edge; 2,576 px on the 4.7+ models; up to 600 images per request (100 on 200K-context models); above 20 images, every image must be 2,000 px or less. OpenAI lists image input for Sol and Luna. Recommendation: mechanical checks first (contrast ratio, clipped text, loudness, closed mesh, computed by scripts), then screenshots per state and size; for video, extract frames at scene cuts; for 3D, 8 to 12 angles with wireframe and normals; use two-vendor judges only on disagreement or at the end, and keep each request at 20 images or fewer.
Sources: https://platform.claude.com/docs/en/build-with-claude/vision (verified today) ; https://learn.chatgpt.com/docs/models
Tag: confirmed in official docs (capabilities); the kit design is inferred

**29. Fair measurement against tl-ade**
No official source. Recommendation: use a fixed set of requests run identically for both tools, with blinded judges, a hard cap of tokens or plan percentage per round, and no judging on the user's plan when a cheaper or separate account is available. The round size and budget are decisions for Erick, because the quota is shared with his father (see the decision list).
Tag: unknown

---

## Decisions for Erick (not for the AI)

(a) **Shared login with the father.** Consumer Terms (verified today): "You may not share your Account login information, Anthropic API key, or Account credentials with anyone else." and "You also may not make your Account available to anyone else." (https://www.anthropic.com/legal/consumer-terms). OpenAI: the Terms of Use could not be read (403); the auth page says "Use API key authentication for programmatic Codex CLI workflows" (https://learn.chatgpt.com/docs/auth).
Recommendation: each person signs in to his own account and plan; a Team plan (one seat per person) if you want one bill.

(b) **Paid extra credits or usage-based billing.** The CLI can switch to credits automatically once extra usage is on (https://support.claude.com/en/articles/12429409-extra-usage-for-paid-claude-plans).
Recommendation: Faber never spends money without asking; keep extra usage off in both accounts and let Faber stop on any overage signal.

(c) **Plan at first use: ask or detect?** The CLI does not clearly expose the tier (item 11), and the plan changes the thresholds.
Recommendation: ask once at first use, then confirm against the limit signal and re-ask if the numbers stop matching.

(d) **Written confirmation from Anthropic for the wrapper.** Item 14 is unknown; the legal page neither clearly allows nor forbids a desktop app that launches `claude -p`.
Recommendation: ask Anthropic sales in writing (https://www.anthropic.com/contact-sales) before any public release; keep it local until then.

(e) **Codex with the user's ChatGPT login.** Item 16 is unknown; the Terms could not be read.
Recommendation: default Codex in Faber to the user's API key until OpenAI confirms in writing; offer ChatGPT sign-in only as a labelled advanced option.

(f) **Telemetry off by default.** Items 17 and 20 show the keys and a side effect: with telemetry off, `-p` starts in auto mode unless a permission mode is passed.
Recommendation: set the telemetry keys off by default and always pass `--permission-mode`, `--model` and `--effort`.

(g) **Fast and Ultrafast in Auto.** Item 26.
Recommendation: keep both out of Auto; explicit choice only, with a cost warning.

---

**Counts:** 29 questions answered (each with a tag); 6 tagged unknown (items 4, 11, 12, 14, 16, 29).
