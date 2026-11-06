# R4: context, cost, speed, tools (official docs only)

Date seen: 2026-10-11 (all pages fetched this date unless noted).
Sources: platform.claude.com/docs, code.claude.com/docs, developers.openai.com/api/docs (redirects to learn.chatgpt.com/docs for Codex pages).

## 1. Long context pricing and behavior

Claude
- 1M window, default (no beta header) on: Fable 5.1, Mythos 5.1, Fable 5, Mythos 5, Opus 5.5, Opus 5, Opus 4.8, Opus 4.7, Opus 4.6, Sonnet 5.5, Sonnet 5, Sonnet 4.6, Haiku 5.5, Mythos Preview. Others 200K. Max output 128k tokens for 1M models. [https://platform.claude.com/docs/en/build-with-claude/context-windows]
- Long-context surcharge: none on Claude 4.6+ except Haiku 5.5 (a 900k request bills at the same per-token rate as 9k). [https://platform.claude.com/docs/en/about-claude/pricing#long-context-pricing]
- Haiku 5.5 tiers by prompt length (prompt includes cache reads and writes; decided per request):
  - up to 100K: input $0.10, 5m cache write $0.125, 1h write $0.20, cache hit $0.01, output $0.50 per MTok
  - over 100K: input $0.50, 5m write $0.625, 1h write $1, cache hit $0.05, output $2.50 per MTok (5x on every line)
  - Batch over 100K: $0.25 in / $1.25 out. Data residency 1.1x applies on top.
  - [pricing page, Model pricing and Batch tables]
- Opus 5.5 $4 in / $20 out; Opus 5 and 4.8 $5 / $25; Sonnet 5.5 $2 / $10; Fable 5.1 $10 / $50. [pricing page]
- Context rot: "As token count grows, accuracy and recall degrade, a phenomenon known as context rot." [context-windows page]. Compaction page: "response quality degrades as a conversation grows."
- Overflow: input alone above window gives 400 "prompt is too long". On 4.5+ models, input + max_tokens above window is accepted and generation stops with stop_reason model_context_window_exceeded. [context-windows page]
- Claude Code: 1M models run 1M by default on Opus 4.7+, Sonnet 5+, Haiku 5.5, Fable. Opus 4.6 and Sonnet 4.6 need [1m]. Sonnet 4.6 with 1M needs usage credits on every plan. Opus 4.6 1M included on Max/Team/Enterprise. Subscription billing for Haiku >100K or for 1M generally is not stated. [https://code.claude.com/docs/en/model-config]
- Claude Code auto-compact default: at the model's context limit; ~967K tokens on native-1M models; 200K boundary for Sonnet 4.6 / Opus 4.6 without [1m]. [https://code.claude.com/docs/en/model-config#set-the-auto-compact-window]

OpenAI / Codex
- GPT-6.1 Sol: context 1,050,000; max input 922,000; max output 128,000; knowledge cutoff Apr 30, 2026. [https://developers.openai.com/api/docs/models/gpt-6.1-sol]
- GPT-6 Luna: same 1,050,000 / 922,000 / 128,000. [https://developers.openai.com/api/docs/models/gpt-6-luna]
- GPT-6 Astra: 1,050,000 / 922,000 / 128,000; cutoff Apr 30, 2026. [https://developers.openai.com/api/docs/models/gpt-6-astra]
- Long-context surcharge above 272K input tokens, for the whole request: 2x input and cached input, 1.5x output. Examples: Sol $4 / $0.20 / $15 (short: $2 / $0.10 / $10); Luna $0.20 / $0.02 / $0.75 (short $0.10 / $0.01 / $0.50); Astra $20 / $2 / $75 (short $10 / $1 / $50). [pricing, https://developers.openai.com/api/docs/pricing; model pages]
- Cache writes for Astra long context $25 (2x short $12.50). [pricing]
- Context rot: no equivalent statement found in fetched pages. Codex page: "extended sessions that require the agent to hold more context will use significantly more per message." [learn.chatgpt.com/docs/pricing]
- Codex subscription credits per 1M tokens (standard, input/cached/output): Astra 250/25/1250; Sol 50/2.5/250; Luna 2.5/0.25/12.5. No long-context credit multiplier stated. [learn.chatgpt.com/docs/pricing]

Subscription cost
- Claude subscription: long context uses plan usage; the 1M-on-subscription detail is in the Claude Code model-config page above. Per-token surcharge does not translate to a published subscription multiplier.
- Codex: Plus estimates of local messages per 5h: Astra 5-45, Sol 15-160, Luna 350-3,000. Pro has no 5h limit. Estimates only. [learn.chatgpt.com/docs/pricing]

## 2. Context management

Claude API, server-side compaction
- Beta compaction edit `compact_20260112`, beta header `compact-2026-01-12`. Models: Fable 5.1, Mythos 5.1, Fable 5, Mythos 5, Opus 5.5, Opus 5, Opus 4.8, Opus 4.7, Opus 4.6, Sonnet 5.5, Sonnet 5, Sonnet 4.6, Haiku 5.5. [https://platform.claude.com/docs/en/build-with-claude/compaction-threshold]
- Trigger: `trigger` default input_tokens 150,000; minimum 50,000; no stated maximum.
- Summary replaces everything before the compaction block. Default prompt asks for "state, next steps, learnings". On 5.1+ and the 5.5 models, earlier thinking blocks are not carried forward; custom instructions see only the visible conversation.
- `pause_after_compaction` (default false) returns stop_reason "compaction" so the client can re-insert recent turns.
- Newer on-demand compaction: beta header `compact-2026-09-04`, parameter `compaction`. Options: keep recent turns, background, custom summarization prompt. [https://platform.claude.com/docs/en/build-with-claude/compaction]
- Cache: summary write invalidates the conversation cache. Put `cache_control: ephemeral` at the end of the system prompt so system stays cached. [compaction-threshold page]

Context editing (beta, header `context-management-2025-06-27`)
- Tool result clearing `clear_tool_uses_20250919`: trigger default 100,000 input tokens; keep default 3 tool uses; `clear_at_least`; `exclude_tools`; `clear_tool_inputs` default false.
- Thinking clearing `clear_thinking_20251015`: `keep` default is all turns on Haiku 5.5+, Sonnet 4.6+, Opus 4.5+, and all turns on Fable/Mythos; `keep: all` maximizes cache hits.
- Cache: clearing invalidates cached prefixes at the clear point; use `clear_at_least`. Keeping thinking preserves cache. On 5.5-class models server-side management does not invalidate thinking blocks. [https://platform.claude.com/docs/en/build-with-claude/context-editing]

Memory tool
- Client-side only. Tool `{"type":"memory_20250818","name":"memory"}`. Commands: view, create, str_replace, insert, delete, rename. Paths under /memories. Your handler must block traversal. Works with context editing (warning before clearing) and with compaction. [https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool]

Context awareness
- Injected budget tags on Sonnet 5, Sonnet 4.6, Haiku 4.5. NOT injected on Opus 4.7+, Sonnet 5.5, Fable/Mythos 5.1, Fable/Mythos 5, Haiku 5.5. [context-windows page]

Claude Code
- Auto-compact setting `autoCompactWindow` (100K to 1M; accepts 200000, 500k, 1M); command `/autocompact`; flag `--autocompact`; env `CLAUDE_CODE_AUTO_COMPACT_WINDOW` (highest precedence). [model-config page]
- `/compact` accepts focus text; CLAUDE.md "Compact instructions" section customizes it. [costs page, https://code.claude.com/docs/en/costs#reduce-token-usage]
- Compaction runs a summary request that reads the cached prefix when warm; cold after a long idle gap. [prompt-caching page]

Prompt cache (API)
- Min cacheable: 512 tokens for Haiku 5.5, Opus 5.5, Sonnet 5.5, 5.x; 1,024 for Sonnet 5 / 4.6; 4,096 for Haiku 4.5. [https://platform.claude.com/docs/en/build-with-claude/prompt-caching]
- TTL: 5m write 1.25x; 1h write 2x; read 0.1x (0.05x Opus 5.5 and Sonnet 5.5; 0.025x Fable 5.1 and Mythos 5.1).
- Invalidation order tools -> system -> messages. Changing tools invalidates all. Changing speed invalidates tools level (fast vs standard). Changing effort or thinking params invalidates messages.
- 20-block lookback per breakpoint.
- Thinking blocks: kept by default on Opus 4.5+, Sonnet 4.6+, Haiku 5.5; cache survives.

Claude Code cache
- Main conversation TTL: 1 hour on Claude subscription within plan usage; 5 minutes on usage credits, API key, or cloud. Subagents: 5 minutes by default. Settings `promptCacheTtl`, `subagentPromptCacheTtl`; env `ENABLE_PROMPT_CACHING_1H=1`, `FORCE_PROMPT_CACHING_5M=1`. [https://code.claude.com/docs/en/prompt-caching#cache-lifetime]
- Invalidators: model switch; effort change (not on Opus 5.5, Sonnet 5.5, Haiku 5.5, Fable 5.1 with API key or subscription); turning fast mode on; MCP connect/disconnect when tools loaded upfront; compaction; plugin MCP changes. Keeps: file edits, skill and command invocation, permission mode, output style, /rewind, subagent spawn, CLAUDE.md edits (not applied until /clear, /compact, restart). [prompt-caching page]
- `/usage` prints `Prompt cache (main)` line with hit share and misses (v2.1.251+). [costs page]

## 3. Speed and cost modes

Claude
- Fast mode: Opus 5.5, Opus 5, Opus 4.8 only. Up to 2.5x output tokens per second; not TTFT. Same weights. [https://platform.claude.com/docs/en/build-with-claude/fast-mode]
- Price: Opus 5.5 $8 in / $40 out (standard $4 / $20 = 2x); Opus 5 and 4.8 $10 / $50 (standard $5 / $25 = 2x). Applies across full window including >200K. Caching and data residency stack on top. [pricing page]
- Not available with Batch API, Priority Tier, Bedrock, Claude Platform on AWS, Vertex, Foundry. Opus 4.7 with speed fast returns error. Opus 4.6 silently runs standard.
- Switching fast/standard invalidates cache (API). [fast-mode page]
- Claude Code: `/fast` toggles; `fastMode` setting; `CLAUDE_CODE_DISABLE_FAST_MODE=1` disables. Subscription plans (Pro/Max/Team/Enterprise): usage credits only, NOT included in subscription limits. Console orgs: pay per token, access provisioned. Team/Enterprise: Owner must enable. Rate-limit fallback to standard speed with cooldown. Enabling mid-conversation costs full uncached input once. [https://code.claude.com/docs/en/fast-mode]
- Batch: 50% off both in and out (Opus 5.5 $2 / $10; Sonnet 5.5 $1 / $5; Haiku 5.5 $0.05 / $0.25 up to 100K). Within 24h; most finish under 1h. Not available in Claude Code. [batch-processing page; pricing]
- Priority Tier: capacity commitments no longer sold. `service_tier` "auto" or "standard_only". Not on Haiku 5.5 or Sonnet 5.5, Opus 5.5. [https://platform.claude.com/docs/en/api/service-tiers]
- Effort is the main cost/speed lever. Levels low, medium, high, xhigh, max. Defaults: medium on Opus 5.5, Sonnet 5.5, Haiku 5.5; high elsewhere. [effort page]

OpenAI / Codex
- Fast mode: 1.5x speed for GPT-5.6 and GPT-5.5 (stated); page lists GPT-6.1 Sol, Astra, Sol, Luna as "where available" with no GPT-6 multiplier. [https://learn.chatgpt.com/docs/agent-configuration/speed]
- Fast cost: included subscription usage at 2.5x Standard; purchased credits and Enterprise pay-as-you-go at 2x. API key: API pricing; ChatGPT multipliers don't apply.
- Ultrafast: GPT-6 Astra and GPT-6.1 Sol only. Claims up to 8x faster than Astra Standard in Codex. Included usage 8x; purchased credits 6x. Availability: Pro $500 and eligible Enterprise/Edu; Enterprise off by default; Pro $500 uses included usage first. Residency: Sol Ultrafast US and EEA+CH; Astra Ultrafast US only. [speed page; pricing page; learn.chatgpt.com/docs/pricing]
- Enable: `/fast` in CLI; `/statusline`; config `service_tier = "fast"` and `[features].fast_mode = true`. Available in desktop app, CLI, IDE when signed in with ChatGPT.
- API pricing for fast/priority: pricing page: "Priority processing was renamed Fast mode on July 30, 2026." Fast = 2x standard in API tables (inferred, not stated as multiplier). Batch = 50% of Standard. Flex matches Batch rates (inferred from tables).
- Codex `service_tier` maps `fast` to request value `priority`. [config-reference]
- Codex config: `model_auto_compact_token_limit` (unset = model default), `model_auto_compact_token_limit_scope` total|body_after_prefix (default total), `model_context_window` (no default listed), `model_reasoning_effort`, `web_search` (default cached; live under full-access sandbox), `model_verbosity`. [https://learn.chatgpt.com/docs/config-file/config-reference]

## 4. Tool search and MCP cost

Claude API
- Variants `tool_search_tool_regex_20251119` (Python regex, 200-char limit) and `tool_search_tool_bm25_20251119` (natural language, 500-char limit). Set `defer_loading: true` on deferred tools; at least one non-deferred tool required; never on the search tool itself. [https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool]
- Results: 5 by default; Claude sets `limit` up to 10,000. Max 10,000 deferred tools per request.
- Typical multi-server setup ~55k tokens of definitions; tool search cuts over 85% (vendor claim in page). Accuracy degrades past 30 to 50 tools (page claim).
- Cache: deferred definitions stay out of the system prefix; cache preserved. `cache_control` cannot sit on a deferred tool (400). [same page]
- Usage: search is not a separate metered tool; loaded definitions count as input tokens.

Claude Code
- MCP tools deferred by default on supported models; only names and server instructions load until used. `/context` shows usage. `/mcp` disables servers. [https://code.claude.com/docs/en/costs#reduce-mcp-server-overhead]
- Cache: with deferral, server connect/disconnect keeps cache; without deferral (tool search disabled or below auto threshold), adding tools invalidates cache. [prompt-caching page]
- MCP output limit: warning at 10,000 tokens; default cap 25,000 tokens; `MAX_MCP_OUTPUT_TOKENS` raises it. [https://code.claude.com/docs/en/mcp]
- Disable tool search: `ENABLE_TOOL_SEARCH=false` (named in the page excerpt; the value set for auto threshold was not captured).

Codex
- Responses API tool search supported on GPT-6.1 Sol, Astra (per model page supported tools list). Codex MCP details not fetched.

## 5. Reasoning and thinking tokens

Claude
- Thinking billed as output tokens once generated. Summaries (`display: summarized`) cost nothing extra to generate; billed tokens are the full thinking. `display: omitted` hides text, still billed. [https://platform.claude.com/docs/en/build-with-claude/thinking-steering-and-cost#pricing]
- Read `usage.output_tokens_details.thinking_tokens`. `output_tokens` is authoritative. [same]
- Adaptive thinking (only mode on Sonnet 5+, Opus 4.7+, Haiku 5.5, Fable). Claude decides whether to think; effort is the control. `thinking: {type: "adaptive"}`. Manual `budget_tokens` (min 1,024) rejected on 4.7+ and Haiku 5.5. [extended-thinking page]
- Haiku 5.5: thinking on by default; can be disabled with `thinking: {type: "disabled"}` at effort high or below; at xhigh/max must use adaptive. `thinking: {type: "between_tools"}` option on Sonnet 5.5. [effort page]
- `max_tokens` is a hard cap on total output including thinking. Set high at high effort. Opus 4.7+ and Sonnet 5.5 guidance: start at 64k for xhigh/max; Sonnet 5.5 agentic 128,000. If stop_reason is max_tokens: raise cap or lower effort. [effort page; thinking-steering-and-cost#cost-control]
- Haiku 5.5 effort guidance: start medium; low for chat and short tool tasks; low skips checks in long agent prompts. [effort page]
- Per-message effort (beta `mid-conversation-output-config-2026-07-01`) on Haiku 5.5 keeps cache. Top-level effort change invalidates cache. [effort page]

OpenAI
- Reasoning tokens billed as output; not visible; in `output_tokens_details.reasoning_tokens`; count toward context. [https://developers.openai.com/api/docs/guides/reasoning]
- `reasoning.effort`: none, minimal, low, medium (default), high, xhigh, max. Sol: low, medium, high, xhigh, max (no none or minimal). Astra: no none (HTTP 400). Luna: none supported. [model pages]
- `max_output_tokens` includes reasoning; status incomplete with reason max_output_tokens; OpenAI suggests reserving at least 25,000 tokens for reasoning plus output when experimenting. [reasoning guide]
- Reasoning summaries: `reasoning.summary` auto|detailed|concise; billing of summary tokens not stated.

## 6. Built-in capabilities (model support)

Web search
- Claude API web_search: $10 per 1,000 searches plus content tokens; web fetch no extra charge beyond tokens. [pricing]
- Claude Code: WebSearch and WebFetch tools exist (permission required; WebSearch allow/deny as whole). Plan or model limits for WebSearch not captured. [https://code.claude.com/docs/en/tools-reference]
- OpenAI web search: $10 per 1,000 calls plus content tokens. Supported on Sol and Luna (features list). Codex `web_search` config: cached default, live under full-access sandbox. [pricing; config reference]

Image input
- Claude: all current models accept images. Standard tier max 1568 px long edge / 1568 visual tokens; 4.7+ high-resolution tier 2576 px / 4784 tokens. Formula ceil(w/28) x ceil(h/28). Max 600 images per request (100 on 200K models); max 8000x8000 px; stricter 2000 px rule above 20 images. [https://platform.claude.com/docs/en/build-with-claude/vision]
- OpenAI: image input on Sol and Luna. [model pages]

Image generation
- Claude: none. The vision page states Claude cannot generate or edit images.
- OpenAI: image generation in Responses API tools list for Sol and Astra. Price for gpt-image models not captured (n/a in what was fetched).

File attachments
- Claude Files API: upload once, reference by file_id; PDFs and documents count toward 600 or 100 page cap. [vision page; context-windows page]
- OpenAI: file search tool, $2.50 per 1K tool calls; storage $0.10 per GB per day (1 GB free). File input not listed as feature for Sol or Luna. [pricing; model pages]

Computer use
- Claude API: computer_toolset_20260801 adds about 4,500 input tokens (about 4,590 on Sonnet 5). Browser toolset about 6,600. Screenshots billed as image tokens. [pricing page]
- Claude Code: not confirmed in tools reference list fetched.
- OpenAI: computer use in Responses API tools list for Sol and Astra. Luna list did not include it.

Code execution
- Claude: code execution billed by time, $0.05 per hour per container beyond 1,550 free hours per org per month; free when web_search_20260209 or web_fetch_20260209 is present; minimum 5 minutes per execution. [pricing]
- OpenAI: code interpreter containers $0.03 (1 GB) to $1.92 (64 GB) per 20-minute session. [pricing]

Review commands
- Not confirmed for either CLI in fetched official pages. Unknown.

## 7. Structured output and cheap classification

Claude
- Structured outputs: `output_config.format` `{type: json_schema, schema: ...}`; supported on claude-haiku-5-5 and claude-opus-5-5, claude-sonnet-5-5, claude-sonnet-4-6, claude-haiku-4-5. Strict tool use: `strict: true`. [https://platform.claude.com/docs/en/build-with-claude/structured-outputs]
- Schema limits: `additionalProperties: false` required for objects; no numeric constraints (minimum, maximum, multipleOf); no string length constraints; array minItems only 0 or 1; enum only string, number, bool, null; no recursive schemas. Unsupported returns 400.
- Truncation: stop_reason "max_tokens" means output may not match schema; retry with larger max_tokens. Refusal: 200, billed, schema may not match.
- Enum case not guaranteed.
- Compatibility with thinking: not confirmed (page cut off).
- Haiku 5.5 price for one-line JSON: input $0.10 per MTok (<=100K), output $0.50 per MTok. Thinking disabled at effort low: send `thinking: {type: "disabled"}` with `output_config: {effort: "low"}` (allowed at low, medium, high; 400 at xhigh/max). [effort page]
- Output limit per request: up to 128k (1M models); set small max_tokens (e.g. 64 to 256) for one line, watch for stop_reason max_tokens.

OpenAI
- GPT-6 Luna: `reasoning.effort` "none" supported; structured outputs supported; function calling in Chat Completions only when reasoning_effort is none. $0.10 / $0.01 cached / $0.50 per MTok (<=272K). Max output 128,000. [model page; pricing]
- Luna long-context (>272K) $0.20 / $0.02 / $0.75.

Comparison: Haiku 5.5 and GPT-6 Luna have the same short-context per-token price for input and output; Haiku surcharges above 100K, Luna above 272K.

## 8. Subagent model inheritance and cost

Claude Code
- `model` frontmatter: sonnet, opus, haiku, fable, full model ID, or inherit. Omitted: resolution order is per-invocation parameter, frontmatter (inherit = main model), CLAUDE_CODE_SUBAGENT_MODEL, then main model. So omitted = main model. [https://code.claude.com/docs/en/sub-agents]
- Built-ins: Explore runs main model (except when main is Fable); Plan main model; general-purpose CLAUDE_CODE_SUBAGENT_MODEL or main; statusline-setup Sonnet; claude-code-guide Haiku.
- Each subagent has its own context window; only the final result returns to parent. Its requests count toward the same usage limits. Subagents inherit extended thinking config (v2.1.198+).
- Cost: subagents spend their own tokens; cache for subagents 5 minutes default even on subscription. [prompt-caching page]
- Agent teams: about 7x tokens when teammates use plan mode; Sonnet recommended for teammates. [costs page]
- Env `CLAUDE_CODE_SUBAGENT_MODEL` (value cut off in source; accepts alias or full model name). [model-config page]

Codex
- Subagent inherits parent model and reasoning effort unless configured. Settings `agents.default_subagent_model`, `agents.default_subagent_reasoning_effort`; per agent file `model` and `model_reasoning_effort` under `~/.codex/agents/` or `.codex/agents/`. [https://learn.chatgpt.com/docs/agent-configuration/subagents]
- "subagent workflows consume more tokens than comparable single-agent runs." Parallel write-heavy work discouraged.

## Unknown or unclear
- Subscription billing of 1M context and of Haiku 5.5 over-100K pricing: not stated.
- Codex credit multiplier for requests above 272K: not stated.
- GPT-6 Fast speed multiplier: not stated (only 1.5x for GPT-5.5 and 5.6).
- Ultrafast for GPT-6 Sol and Luna: not listed (only Astra and 6.1 Sol).
- Claude Code tool search auto threshold value: not captured.
- OpenAI compaction supported models list: not found; examples use gpt-5.3-codex and gpt-6-astra.
- Claude structured output with thinking enabled: page cut off, not confirmed.
- OpenAI reasoning summary token billing: not stated.
- Claude compaction "what is kept": default prompt only; no exhaustive list.
- Codex model_auto_compact_token_limit default: "model defaults" only; numeric default not stated.
- OpenAI gpt-image price: not captured.
- Review commands (Claude Code /review, Codex /review): not confirmed in fetched docs.
- Claude Code computer use and image generation in CLI: not confirmed.
- Codex 1M context toggle: no setting found in config reference excerpt; Sol and Luna list 1.05M.

## Questions worth asking
1. Should the Claude classifier run on Haiku 5.5 at effort low with thinking disabled, given 1M context but a 5x price step above 100K? Confirm the classifier prompt stays under 100K.
2. Should Faber cap Claude Code auto-compact (autoCompactWindow, e.g. 200K) by default to limit context rot, even on 1M models?
3. Should exploration subagents run on Haiku (CLAUDE_CODE_SUBAGENT_MODEL or frontmatter model: haiku) to save quota, accepting lower quality?
4. Should Faber expose Claude Fast mode (usage credits only, 2x price) and Codex Ultrafast (Pro $500 only) in the UI with a cost warning, or hide them?
5. Is it acceptable that Batch and Flex (50% off) stay out of the CLI flows, since the subscription CLIs cannot use them?
6. Should Faber pick GPT-6 Luna or Claude Haiku 5.5 for the one-line classifier, given equal short-context per-token prices and different caching behavior?
7. Do we run a measured quota test (same task, Claude 1M vs 200K window, Codex under vs over 272K) before setting thresholds, since subscription multipliers for long context are not published?
8. Should the default cache TTL policy be changed to 1h on API or usage-credit accounts (promptCacheTtl) to cut re-read costs after breaks, at the 2x write price?
