# R3: prompt cache and quota when switching MODEL / EFFORT / THINKING mid-conversation

Date seen: 2026-10-11 (all pages fetched that day). Only official vendor docs were used.
Sources marked (F) were fetched and read. Items marked (n/a) are not stated in the official docs.

## Source URLs

Anthropic
- A1 (F) https://platform.claude.com/docs/en/build-with-claude/prompt-caching : invalidation table, TTL, multipliers, min lengths
- A2 (F) https://platform.claude.com/docs/en/about-claude/pricing : per-model cache write / hit prices
- A3 (F) https://platform.claude.com/docs/en/api/rate-limits : ITPM rules for cache reads and writes
- C1 (F) https://code.claude.com/docs/en/prompt-caching : Claude Code cache behavior (model, effort, MCP, compaction, TTL, /model prompt)
- C2 (F) https://code.claude.com/docs/en/model-config : /model, /effort, opusplan, subagent inheritance
- C3 (F) https://code.claude.com/docs/en/costs : cost levers, thinking tokens, /usage cache stats, plan usage
- C4 (search only, not opened) https://code.claude.com/docs/en/settings-reference : promptCacheTtl and related keys (names taken from C1)

OpenAI
- O1 (F) https://developers.openai.com/api/docs/guides/prompt-caching : prefix-matching rules, retention, discount, write fee, rate limits
- O2 (F, redirected) https://developers.openai.com/codex/models -> https://learn.chatgpt.com/docs/models : Codex model list, effort levels
- O3 (F, redirected) https://developers.openai.com/codex/config-reference -> https://learn.chatgpt.com/docs/config-file/config-reference : model_reasoning_effort, model keys
- O4 (F, redirected) https://developers.openai.com/codex/pricing -> https://learn.chatgpt.com/docs/pricing : Codex credit rates, cache write, usage drivers, plan estimates

## 1. Claude: API and Claude Code

### 1.1 What invalidates the cache (A1 table, C1)
- Cache hierarchy is tools -> system -> messages. "Changes at each level invalidate that level and all subsequent levels." (A1)
- Tool definitions (names, descriptions, parameters) change: invalidates the ENTIRE cache. (A1)
- Model change: each model has its own cache. "Switching models recomputes the entire request even when the content is identical." (C1, "Switching models"). The A1 table has no model row.
- Effort change (`output_config.effort` in API; /effort in CC):
  - API: "always invalidates message blocks", tools and system stay cached. (A1)
  - Claude Code, most models: "changing effort mid-session means the next request reads the entire conversation history with no cache hits." (C1)
  - Claude Code exception: on Opus 5.5, Sonnet 5.5, Haiku 5.5, and Fable 5.1 with an API key or Claude subscription, "changing effort keeps the cache". Not on Bedrock, Google Cloud, Claude apps gateway, CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS, or HIPAA. (C1)
- Thinking config on/off (API): "changing it always invalidates message blocks". Tools and system are "Model-specific". (A1)
  - Claude Code doc gives no explicit cache statement on the thinking toggle (n/a for CC). C3 says thinking tokens bill as output, and you cannot turn thinking off on Opus 5.5, Sonnet 5.5, Haiku 5.5, or Fable.
- Tool list change (MCP connect or remove, deny rule):
  - API: tool definition change invalidates everything. (A1)
  - CC with tool search (default on supported models): tools deferred, cache survives. Tools loaded upfront: adding or removing a definition invalidates. (C1 table)
- System prompt change: invalidates system and messages (A1). CC: CLAUDE.md edits mid-session do not apply and do not invalidate; new content loads on /clear, /compact, or restart. (C1)
- Other items from A1 and C1:
  - tool_choice change: invalidates messages only (A1)
  - images added or removed: messages only (A1)
  - web search toggle: changes the system prompt, so invalidates system and messages (A1)
  - fast mode toggle (CC): first request with fast mode reads with no hits; turning it off or back on later keeps the cache (C1)
  - plan mode with opusplan: each toggle is a model switch and starts a fresh cache (C1)
  - permission mode change, output style change, skills and commands, /recap, /rewind: cache kept (C1)
  - CC version upgrade: first conversation after an upgrade rebuilds the cache (C1)

### 1.2 TTL and refresh
- API default: 5 minutes. "The cache is refreshed for no additional cost each time the cached content is used." (A1)
- API 1h option: `cache_control` `"ttl": "1h"`, write priced at 2x base. (A1)
- Lifetime is measured "from the start of the request that writes or reads the cache entry". (A1)
- Refresh on a 1h entry: A1 does not say explicitly. The pricing page lists a "Cache hits and refreshes" column with the same price for 1h. (A2) (n/a for explicit wording)
- Claude Code main-conversation TTL (C1, "Which TTL each request gets"):
  - Claude subscription, within plan usage: 1 hour by default.
  - API key, usage credits, or cloud provider: 5 minutes by default.
  - Drops to 5m once the subscription draws on usage credits.
  - Subagents, forks, compaction, titles: 5m by default, even on a subscription.
  - Override: `promptCacheTtl` setting or `CLAUDE_CODE_PROMPT_CACHE_TTL` (main); `subagentPromptCacheTtl` or `CLAUDE_CODE_SUBAGENT_PROMPT_CACHE_TTL` (subagents). Values `5m` or `1h`. Requires CC v2.1.242+. (C1)
  - `FORCE_PROMPT_CACHING_5M=1` forces 5m for both. (C1)
- Claude Code keeps a cache of "one cache TTL" after the last request or response. /model asks for confirmation only while the cache is warm. (C1)

### 1.3 Prices (A2, per MTok; base input / 5m write / 1h write / cache hit)
- Claude Opus 5.5: $4 / $5 / $8 / $0.20 (hit = 0.05x)
- Claude Opus 5: $5 / $6.25 / $10 / $0.50 (hit = 0.1x)
- Claude Sonnet 5.5: $2 / $2.50 / $4 / $0.10 (hit = 0.05x)
- Claude Sonnet 5: $2 / $2.50 / $4 / $0.20 (hit = 0.1x)
- Claude Haiku 5.5, prompts up to 100k: $0.10 / $0.125 / $0.20 / $0.01 (hit = 0.1x)
- Claude Fable 5.1 and Mythos 5.1: $10 / $12.50 / $20 / $0.25 (hit = 0.025x)
- Multipliers (A1): 5m write 1.25x, 1h write 2x, read 0.1x (0.025x Fable 5.1 and Mythos 5.1; 0.05x Opus 5.5 and Sonnet 5.5).
- Break-even (A1): 5m write pays off after 1 read; 1h write after 2 reads.
- Model switch cost (inference, not written in the docs): a switch makes the whole history uncached on the new model, and the write is billed at the write rate (1.25x for 5m). Compared with a warm hit at 0.1x, a full re-read is about 12.5x per token at 5m.
- Pricing page date: as fetched 2026-10-11. The Sonnet 5.5 hit price was cut to $0.10 per MTok (A2, consistent with the search note; not verified against a changelog).

### 1.4 Quota and rate limits
- API ITPM (A3): for most models cache reads do NOT count toward ITPM. `cache_creation_input_tokens` DO count. Haiku 3.5 is the exception and counts cache reads.
- Claude Code subscription plan usage: C1 and C3 say cache reads "still draw usage for the whole conversation" and that cache misses are flagged in /usage. No formula or weight for cached tokens against plan limits is published (n/a). /usage shows cache read and write tokens separately (C3).
- Subscription "session" and "weekly" windows are shared across models (C3).

### 1.5 Official advice on switching
- C1: "Pick your model and effort level at the top of a session, then save /compact for natural breaks between tasks. The fewer changes you make mid-task, the higher your cache hit rate."
- C2 / C3: switching to Opus means subagents that inherit the model also run on Opus. To keep a subagent cheaper, set `model` in its definition. `model: haiku` for simple subagent tasks.
- C3: lower effort or disable thinking to reduce cost on simple tasks. Thinking tokens bill as output.
- opusplan (C1, C2): Opus during plan mode, Sonnet during execution. Each plan toggle is a model switch and a fresh cache.
- /model Enter saves default; `s` applies to this session only. (C2)

### 1.6 Compaction
- /compact replaces history and invalidates the conversation layer. System prompt layer is reused. (C1)
- The summary request reuses the prefix while the cache is warm. After a cold gap it re-reads the full history uncached. (C1)
- The turn after compaction rebuilds the cache only for the shorter summary. (C1)
- A1 has no compaction statement.

### 1.7 Minimum cacheable length (A1)
- 512 tokens: Opus 5.5, Sonnet 5.5, Haiku 5.5, Opus 5, Sonnet 5, Fable and Mythos 5.x
- 1,024: Opus 4.8, Sonnet 5, Sonnet 4.6
- 2,048: Opus 4.7
- 4,096: Opus 4.6, Opus 4.5, Haiku 4.5

## 2. OpenAI: API and Codex CLI

### 2.1 What invalidates the cache (O1)
- "Cache reuse requires the entire rendered prefix to match."
- "If content or a relevant setting changes before a breakpoint, the prefix after that change cannot match."
- Settings listed as affecting the prefix: `model`, `tools`, `parallel_tool_calls`, `text.format`, `reasoning.effort`, `text.verbosity`, `context_management`.
- "A different model can use different weights and caching behavior."
- "Changing a request does not necessarily discard an existing cache entry."
- Compaction: "Compaction replaces earlier conversation context with a shorter representation." It can break reuse.
- Effort change on GPT-6: append a `configuration_update` input item. "The latest configuration update controls the reasoning effort for subsequent responses." O1 says changing top-level `reasoning.effort` "can rewrite instructions in the hidden system instructions". Whether `configuration_update` keeps the cache: n/a.
- Thinking on/off as a separate knob: n/a (OpenAI exposes reasoning effort, not a thinking toggle, in these pages).
- Codex mid-session switches (O2, O3): `/model` and `/reasoning` commands are mentioned in search results, but the fetched pages did not confirm them. O3 says subagents inherit parent model and effort unless configured. No cache advice from Codex docs (n/a).

### 2.2 TTL and refresh (O1)
- GPT-5.6 and later: `prompt_cache_options.ttl`, only supported and default value `30m`. "A cached prefix remains eligible for reuse for 30 minutes after its most recent write or reuse."
- "Reusing the prefix refreshes its lifetime without another cache-write charge."
- Earlier models, `in_memory`: about 5 to 10 minutes of inactivity, up to 1 hour.
- Earlier models, `24h`: about 30 minutes, up to 24 hours.
- Defaults: orgs without ZDR default to `24h`; ZDR orgs to `in_memory` (for models supporting both).
- GPT-5.5 and 5.5 Pro: only `24h`.
- `prompt_cache_retention` is deprecated. Migration guide: replace with `prompt_cache_options.ttl: "30m"` when moving from GPT-5.5 or earlier.
- Manual invalidation: not offered (O1).

### 2.3 Prices (O1, O4)
- API: cached input discount "up to 95%". GPT-5.6 and later: cache writes 1.25x standard uncached input, reads 0.1x (0.05x GPT-6.1 Sol). Write is not an extra fee; the token is billed at one rate.
- Codex credit billing (O4, credits per 1M tokens, Standard speed):
  - GPT-6.1 Sol: input 50, cached 2.5 (0.05x), output 250
  - GPT-6 Luna: input 2.5, cached 0.25 (0.1x), output 12.5
  - "Codex credit billing has no separate cache-write charge." (differs from the API 1.25x write)
- Speed tiers in Codex (O4): Fast counts 2.5x against included subscription usage; Ultrafast 8x (GPT-6 Astra and 6.1 Sol). Purchased credits: 2x and 6x.
- Minimum cacheable: 1,024 tokens for GPT-5.6 and later (O1).

### 2.4 Quota
- Codex usage: "Model choice, context, reasoning, tool use, retrieval, and caching all affect usage." (O4)
- Cached input is billed in the credit formula at the cached rate (O4).
- Plan estimates per 5-hour window (Plus and Standard Business): GPT-6.1 Sol 15 to 160 local messages; GPT-6 Luna 350 to 3,000. "These estimates are not fixed message limits." Pro: "currently no five-hour limit"; weekly limits may apply (O4).
- API rate limits (O1): "Cached input tokens still count toward tokens-per-minute limits." Opposite of Anthropic's ITPM rule.
- Codex CLI display of cached tokens: n/a in fetched docs. Enterprise/Edu desktop token history splits uncached input, cached input, output (O4, search summary only, not opened).

### 2.5 Official advice on switching
- O2: "Higher reasoning effort can improve results for complex tasks, but it takes longer and uses more tokens."
- O2: on Max, "Use it for the hardest problems, when depth matters more than speed or usage."
- O4: "consider whether a smaller model or tighter task scope would still produce a useful result."
- Switching models mid-session: n/a (no cache or quota note in O2 or O4).

## 3. Summary table

| Change | Claude API | Claude Code | OpenAI API | Codex |
|---|---|---|---|---|
| Model switch | Invalidates all (per-model cache) | Invalidates all; /model asks only if cache warm | Prefix includes `model`; different weights | n/a (no note); cache effect follows API |
| Effort change | Invalidates messages | Invalidates all, except Opus 5.5, Sonnet 5.5, Haiku 5.5, Fable 5.1 (API key or subscription) | Prefix includes `reasoning.effort`; can rewrite hidden instructions | n/a |
| Thinking on/off | Invalidates messages | n/a (no explicit CC statement) | n/a (not a separate knob) | n/a |
| Tool list change | Invalidates all | Invalidates if tools loaded upfront; kept with tool search | Prefix includes `tools` | n/a |
| System prompt change | Invalidates system and messages | CLAUDE.md edits do not apply mid-session | Hidden instructions in prefix | n/a |
| /compact | Conversation layer rebuilt | Same; summary reads prefix if warm | Compaction can break reuse | n/a |

## 4. Caveats
- Search-only (not opened): Codex nudge to lighter model at 90% of 5-hour limit; Sonnet 5 price change Sept 1; OpenAI changelog Sol $0.10 vs $0.20 conflict. Excluded from the answer.
- Subscription quota weight of cached tokens: n/a for Anthropic and OpenAI.
- Claude Code main TTL default depends on billing mode (subscription 1h, API 5m). Changes with usage credits.
- GPT-5.5 retires October 14, 2026 (O2).
