# R2: Official CLI model, effort and quota facts (Claude Code, Codex CLI)

Date seen: 2026-10-11 (all pages fetched that day). Pages marked "no date" show only "Updated this week" or no date at all.
Rule followed: facts only from official domains (code.claude.com, support.claude.com, learn.chatgpt.com which is the current host of developers.openai.com/codex, github.com/openai/codex, github.com/anthropics/claude-code). Secondary sources are labelled as such. "n/a" = not published on the pages read.

Redirects: developers.openai.com/codex/* now 308-redirects to learn.chatgpt.com/docs/* (OpenAI-owned). Used the redirect targets.

---

## 1. Claude Code

### 1.1 Model aliases and full IDs

Source: https://code.claude.com/docs/en/model-config (seen 2026-10-11)

| Alias | Anthropic API resolves to | Bedrock / Google Agent Platform | Microsoft Foundry |
|---|---|---|---|
| `opus` | Opus 5.5 (`claude-opus-5-5`) | Opus 5.5 | Opus 4.6 |
| `sonnet` | Sonnet 5.5 (`claude-sonnet-5-5`) | Sonnet 4.5 | Sonnet 4.5 |
| `haiku` | Haiku 5.5 (`claude-haiku-5-5`) | Haiku 4.5 | Haiku 4.5 |
| `fable` | Fable 5.1 (`claude-fable-5-1`) unless `ANTHROPIC_DEFAULT_FABLE_MODEL` is set | (same page) | (same page) |
| `best` | Fable where available, otherwise `opus` | | |
| `opusplan` | `opus` in plan mode, then `sonnet` for execution | | |
| `default` | clears override; runtime default | | |
| `sonnet[1m]`, `opus[1m]` | 1M-context variants; no effect when the alias already has native 1M | | |

Version history (model-config, "Version history" table): `opus` -> Opus 5.5 at v2.1.280; `sonnet` -> Sonnet 5.5 at v2.1.284; `haiku` -> Haiku 5.5 at v2.1.293 (Anthropic API); `fable` -> Fable 5.1 at v2.1.257.
Minimum versions quoted on the same page: Fable 5.1 v2.1.257, Opus 5.5 v2.1.280, Sonnet 5.5 v2.1.284, Haiku 5.5 v2.1.293.

Full IDs listed on the page: `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-fable-5-1`, `claude-opus-5`, `claude-sonnet-5`, `claude-opus-4-8`, `claude-opus-4-7`, `claude-opus-4-6`, `claude-sonnet-4-6`, `claude-haiku-4-5`, and others.

### 1.2 Where a model is set (accepted values)

Sources: https://code.claude.com/docs/en/model-config; https://code.claude.com/docs/en/cli-reference (seen 2026-10-11)

- `claude --model <alias|name>`: CLI reference example `claude --model claude-sonnet-5`; alias or full name.
- In session: `/model <alias|name>` or `/model` for the picker. Picker key `s` = this session only; Enter saves as default.
- `ANTHROPIC_MODEL=<alias|name>` (env).
- `"model": "<alias|name>"` in settings.json.
- `ANTHROPIC_DEFAULT_MODEL=<alias|name>` (default for new sessions; requires v2.1.236+).
- Precedence (highest first): `/model` > `--model` > `ANTHROPIC_MODEL` > settings `model` > `ANTHROPIC_DEFAULT_MODEL`.
- Mistyped values in `--model`, `ANTHROPIC_MODEL` or `model` are not checked up front; they fail on the first request.

### 1.3 Effort levels

Sources: https://code.claude.com/docs/en/model-config (section "Adjust effort level"); https://code.claude.com/docs/en/cli-reference (flag `--effort`) (seen 2026-10-11)

Accepted names: `low`, `medium`, `high`, `xhigh`, `max`. Plus `auto` (env var `CLAUDE_CODE_EFFORT_LEVEL` and `/effort auto`) and `ultracode` (only via `--effort ultracode`, v2.1.203+; starts at xhigh with ultracode on).

| Model | Effort levels supported | Default effort |
|---|---|---|
| Fable 5.1, Fable 5 | low, medium, high, xhigh, max | high |
| Opus 5.5 | low, medium, high, xhigh, max | medium |
| Sonnet 5.5 | low, medium, high, xhigh, max | medium |
| Haiku 5.5 | low, medium, high, xhigh, max | medium |
| Opus 5, Sonnet 5, Opus 4.8 | low, medium, high, xhigh, max | high |
| Opus 4.7 | low, medium, high, xhigh, max | xhigh |
| Opus 4.6, Sonnet 4.6 | low, medium, high, max (no xhigh) | high |
| Models not listed (e.g. Haiku 4.5) | no effort support | n/a |

- Unsupported level: Claude Code uses the highest supported level at or below it (example on page: xhigh runs as high on Opus 4.6).
- Default resolution, first match wins: explicit (`CLAUDE_CODE_EFFORT_LEVEL`, `--effort`, `/effort`) > settings (`modelSettings` per model or top-level `effortLevel`) > model default. Opus 5.5 note: a top-level `effortLevel` in user settings does not count for Opus 5.5.
- `max` is not accepted in `effortLevel` or `modelSettings`; set via env var or `/effort` (session only unless set in env).
- Per-task wording on the page ("Choose an effort level" table): low = "Quick exchanges where you review each result, such as brainstorming, a first sketch, or a small change like a rename"; medium = "day-to-day engineering work with a clear scope, such as implementing a new feature" on Opus 5.5 and Sonnet 5.5; high = "Work where verification matters or edge cases are likely, such as fixing a bug in an existing codebase"; xhigh = "Deeper reasoning at higher token spend"; max = "Hard problems you want Claude to work through without you ... may show diminishing returns and is prone to overthinking, so test before adopting it broadly".
- Page advice on moving models: "The effort scale is calibrated per model" and "start at medium rather than carrying over the level you used on Opus 5" (moving to Opus 5.5).
- Thinking cannot be turned off on Opus 5.5, Sonnet 5.5, Haiku 5.5 or Fable models (model-config and costs pages).

### 1.4 Default model per plan

Source: https://code.claude.com/docs/en/model-config ("Plan-based and account-based defaults") (seen 2026-10-11)

- `default` resolves to Opus 5.5 for Pro, Max, Team, Enterprise, Anthropic API, Claude Platform on AWS, Bedrock, Google Agent Platform. Microsoft Foundry: Sonnet 4.5. Enterprise admin org default overrides.
- Changelog 2.1.280 (https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md): "Default model on Pro and Team Standard plans changed from Sonnet to Opus."
- Fable is not the account-type default on any plan.

### 1.5 Usage limits and how models consume them

Sources: https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan ; https://support.claude.com/en/articles/11145838-using-claude-code-with-your-max-plan ; https://support.claude.com/en/articles/11647753-how-do-usage-and-length-limits-work ; https://code.claude.com/docs/en/costs (all seen 2026-10-11)

- Pro and Max: a 5-hour session limit and a weekly limit. Max also has a separate weekly limit for Fable (official Max article: "Max plans also have a separate weekly limit for Fable"). Pro/Max article states weekly limit wording without naming which models it covers.
- Claude apps and Claude Code draw from the same pool; IDE usage counts too (search snippet of the official Pro/Max article; not verified on the page itself).
- Usage "depends on which Claude model you're chatting with" (official usage article). No per-model rates given.
- "Higher effort uses more tokens" (official usage article). No multiplier given.
- Official pages read: per-plan message or prompt counts: n/a. Per-model multipliers (Opus vs Sonnet vs Haiku): n/a.
- Unverified: a search snippet attributed to Anthropic gave "about 50-200 prompts ... every 5 hours on 5x" and "10-40 prompts" for Pro; and "Anthropic states that using more compute-intensive models will cause you to hit your usage limits sooner". Neither text appeared on the pages fetched, so treat as unverified.
- Unverified third-party claim: Opus consumes about 5x Sonnet's allocation. Not in any official page read.
- Haiku 5.5 costs more per token on prompts above 100K tokens (model-config page). Token prices are API pricing, not plan limits.
- Usage credits: Fable usage may bill to usage credits instead of plan limits; the `/model` picker labels Fable "Requires usage credits" (model-config page; changelog 2.1.219).
- 1M context: Opus 4.6 [1m] included with Max, Team, Enterprise; Pro requires usage credits. Sonnet 4.6 [1m] requires usage credits on every plan including Max (model-config). Fable 5.1, Fable 5, Sonnet 5+, Haiku 5.5, Opus 4.7+ use 1M by default.
- Plan usage check: `/usage` (https://code.claude.com/docs/en/costs). Shows plan usage bars and attribution by skill, subagent, plugin and MCP server.
- Team/Enterprise: per-seat allowance on rolling 5-hour and weekly windows, shared with Claude chat and Cowork; size depends on seat tier Standard or Premium (costs page).

### 1.6 Vendor guidance on model and effort choice (inside the CLI)

Sources: https://code.claude.com/docs/en/costs ("Choose the right model", "Manage agent team costs"); https://code.claude.com/docs/en/model-config ("opusplan", "Choose an effort level") (seen 2026-10-11)

- "Sonnet handles most coding tasks well and costs less than Opus. Reserve Opus for complex architectural decisions or multi-step reasoning."
- `opusplan`: "uses `opus` for complex reasoning and architecture decisions" in plan mode, then switches to `sonnet` for code generation and implementation.
- Simple subagent tasks: "specify `model: haiku` in your subagent configuration" (costs page).
- Agent teams: "Use Sonnet for teammates. It balances capability and cost for coordination tasks."
- Lower the effort level or disable thinking for simpler tasks to reduce cost; thinking cannot be disabled on Opus 5.5, Sonnet 5.5, Haiku 5.5 or Fable (costs page).
- Plan mode: "Use plan mode for complex tasks" (costs page).
- No official per-plan default-model recommendation beyond the default table in 1.4. No official recommended effort per plan.

### 1.7 Changelog, last ~2 months

Source: https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md (read in 7 chunks, versions 2.1.296 down to 2.1.136; the changelog gives no version dates) and https://github.com/anthropics/claude-code/releases (dates shown for the 10 newest only). Seen 2026-10-11.

Dated by releases page: 2.1.287 = 01 Oct; 2.1.288 = 02 Oct; 2.1.290 = 05 Oct; 2.1.292 = 06 Oct; 2.1.293 = 07 Oct; 2.1.295 = 08 Oct; 2.1.296 = 09 Oct. Versions older than 2.1.287 have no dates in the changelog. Versions 2.1.136 to 2.1.183 were partly read and their dates are n/a. Window (approx. 11 Aug to 11 Oct) therefore covers roughly versions 2.1.2xx to 2.1.296; exact start version n/a.

Model and effort changes found:

- 2.1.296: added `CLAUDE_CODE_WORKFLOW_SUBAGENT_MODEL`; fixed Haiku 5.5 token counts.
- 2.1.295: `/model` effort fix: "Fixed `/model` saying a Max effort pick was saved as your default"; `xhigh`/`max` made web searches and agent hooks much slower (fixed).
- 2.1.293: Haiku 5.5 added; "now the default Haiku model on the Anthropic API"; `/model` effort arrows could save Low as default (fixed).
- 2.1.292: Agent tool gained an `effort` parameter.
- 2.1.290: effort level change on fallback retry (fixed); `/code-review` at medium effort changed.
- 2.1.287: `/advisor`: Sonnet 5.5 can advise Opus 4.7 and 4.8; Fable on claude.ai login follows newest Fable.
- 2.1.284: Sonnet 5.5 added as default Sonnet on the Anthropic API; `/effort` keybindings; "Changed Ultracode into its own `/effort` toggle, which no longer forces xhigh"; `/rate-limit-options` added.
- 2.1.283: `availableModelsMatch`, `deniedModels` settings.
- 2.1.280: Opus 5.5 added as default Opus; default Pro/Team Standard model changed Sonnet -> Opus; per-model `/effort` saved effort no longer applies to newer models such as Opus 5.5.
- 2.1.267: `maxEffortLevel` setting (caps effort on every provider).
- 2.1.257: Fable 5.1 added as default Fable; `s` in `/effort` for current session.
- 2.1.243: `modelPicker` setting; `modelPricing` managed setting.
- 2.1.219 and earlier (in window): Opus 5 default (2.1.219); Sonnet 5 default (2.1.197); Opus 4.8 launch defaults to high, `/effort xhigh` for harder tasks (2.1.154).
- Usage-limit changes in window: `/usage-credits` replaces `/extra-usage` (2.1.144); usage-credits wait and auto-continue after limit reset (2.1.234); per-category breakdown in `/usage` (2.1.149).

### 1.8 Settings and CLI keys for effort and models

Sources: https://code.claude.com/docs/en/model-config; https://code.claude.com/docs/en/cli-reference (seen 2026-10-11)

- Settings keys: `model`, `effortLevel` (top-level), `modelSettings` (per model), `maxEffortLevel`, `availableModels`, `deniedModels`, `modelPicker`, `fallbackModel`, `modelOverrides`.
- Env vars: `CLAUDE_CODE_EFFORT_LEVEL`, `ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_MODEL`, `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`, `ANTHROPIC_DEFAULT_FABLE_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL_FORCE`, `CLAUDE_CODE_WORKFLOW_SUBAGENT_MODEL`, `MAX_THINKING_TOKENS`.

---

## 2. OpenAI Codex CLI

### 2.1 Model IDs

Sources: https://learn.chatgpt.com/docs/models (redirect target of developers.openai.com/codex/models); https://learn.chatgpt.com/docs/config-file/config-reference (seen 2026-10-11)

| Name in docs | Exact ID | Notes |
|---|---|---|
| GPT-6 Astra | `gpt-6-astra` | CLI example: `codex -m gpt-6-astra`. "Most capable model for complex work." |
| GPT-6.1 Sol | `gpt-6.1-sol` | CLI example: `codex -m gpt-6.1-sol`; config example `model = "gpt-6.1-sol"`. Rollout limited (see 2.5). |
| GPT-6 Sol | `gpt-6-sol` | Named as the default for Plus/Pro/Business/Enterprise/Edu "when available". |
| GPT-6 Luna | `gpt-6-luna` | "Most efficient model for focused, high-volume tasks." Enterprise/Edu need admin enablement. |
| GPT-5.5 | `gpt-5.5` | Retires from ChatGPT, Work and Codex on 14 Oct 2026. API unaffected. |

Also named on the page: GPT-5.6 Sol, GPT-5.6 Terra, GPT-5.6 Luna ("remain available during the rollout"); no IDs given.
Retired (Codex with ChatGPT sign-in): gpt-5.3-codex-spark (14 Sep 2026); gpt-5.4 and gpt-5.4-mini (31 Aug 2026); gpt-5.2 and gpt-5.3-codex already deprecated.

Default model: if none is set, Codex "uses a recommended model" (name not stated). Per plan (models page): Plus, Pro, Business, Enterprise, Edu: "choose GPT-6 Sol (`gpt-6-sol`) when available"; Free and Go: GPT-6 Luna in the desktop app. Recommended for complex coding and agentic work: GPT-6.1 Sol "when available to your account and client"; Luna for focused repeatable tasks.

Conflict to note: Codex releases 0.159.1 (29 Sep, https://github.com/openai/codex/releases?page=5 read) "Added GPT-6.1 Sol as the default model in the bundled catalog" and 0.161.0 (7 Oct) "GPT-6.1 Sol is now the default model in the bundled and Amazon Bedrock catalogs." The models page still says GPT-6 Sol. Treat the default ID as unresolved between `gpt-6-sol` and `gpt-6.1-sol`.

### 2.2 Config keys for model and effort

Sources: https://learn.chatgpt.com/docs/config-file/config-reference ; https://learn.chatgpt.com/docs/config-file/config-advanced ; https://learn.chatgpt.com/docs/developer-commands (seen 2026-10-11)

| Key | Accepted values | Default |
|---|---|---|
| `model` | string model ID (e.g. `gpt-6.1-sol`) | "recommended model" (not named) |
| `model_reasoning_effort` | examples listed: `low`, `medium`, `high`, `xhigh`, `max`, `ultra`. Page: "Available levels depend on the model and client." | n/a |
| `plan_mode_reasoning_effort` | string | Plan Mode built-in preset |
| `model_reasoning_summary` | `auto`, `concise`, `detailed`, `none` | n/a |
| `model_verbosity` | `low`, `medium`, `high` | selected model/preset default |
| `review_model` | string | session model |
| `service_tier` | string; `fast` maps to request value `priority` | n/a |
| `agents.default_subagent_model`, `agents.default_subagent_reasoning_effort` | string | n/a |
| `model_context_window`, `model_auto_compact_token_limit` | numbers | model default |

- Profiles: `$CODEX_HOME/<name>.config.toml`, selected with `--profile <name>`. The example profile sets `model = "gpt-6.1-sol"` and `model_reasoning_effort = "medium"`.
- Naming: the app says "Light" and the CLI says "Low" for the lowest level; the docs say "Extra High" for xhigh (models page). Max and Ultra are reached in the CLI through `/model` then "More reasoning…" (models page).

### 2.3 Effort levels per model

Source: https://learn.chatgpt.com/docs/models (seen 2026-10-11). The page has no per-model matrix; these are statements from its prose.

| Model | Effort levels stated | Default stated |
|---|---|---|
| `gpt-6-astra` | Light (CLI: `low`), Medium, Extra High. Presets "Astra Light / Medium / Extra High"; some paid plans omit Extra High. Max and Ultra: n/a. | "start with ... Light for Astra" (no formal default stated) |
| `gpt-6.1-sol` | "ranges from Light to Ultra. Max and Ultra depend on your settings." | n/a ("reasoning effort available by default in your client") |
| `gpt-6-luna` | "up to Max, but not Ultra". Light/Medium: n/a. | "High" |
| `gpt-6-sol` | presets "GPT-6 Sol Light" and "GPT-6 Sol Medium" only | n/a |
| `gpt-5.5` | n/a | n/a |

Ultra uses subagents (models page). Higher effort "takes longer and uses more tokens" (models page).

Non-interactive: `codex exec` flags per https://learn.chatgpt.com/docs/developer-commands: `--model, -m` (string) "Override the configured model for this run"; `-c, --config key=value` (repeatable) "Inline configuration override for the non-interactive run". No dedicated effort flag; effort is set with `-c model_reasoning_effort=<value>` (config key, per config reference). Exact `-c` effort example in an official page: n/a (community sources only).

### 2.4 Plan usage limits (local messages per 5 hours)

Source: https://learn.chatgpt.com/docs/pricing (redirect target of the Codex pricing page; seen 2026-10-11)

| Plan | 5-hour limit (local messages) | Weekly |
|---|---|---|
| Plus | Astra 5-45; Sol 15-160; Luna 350-3,000 | Not quantified ("Weekly limits may also apply") |
| Pro ($100/$200/$500 per month tiers) | "Pro plans currently have no five-hour limit." Per-tier numbers: n/a | n/a |
| Business (Standard) | same estimates as Plus | n/a |
| Enterprise/Edu, flexible pricing | "no fixed rate limits. Usage scales with credits." | n/a |
| Enterprise/Edu, without flexible pricing | "same per-seat usage limits as Plus for most features" | n/a |
| Free, Go | n/a (model access only: GPT-6 Luna at Standard speed in the desktop app, subject to rollout) | n/a |
| API key | API pricing, not subscription limits | n/a |

Limits are estimates and vary by task.

### 2.5 Credit use per model and multipliers

Source: https://learn.chatgpt.com/docs/pricing (seen 2026-10-11)

Standard credit rates per 1M tokens (input / cached input / output):

| Model | Input | Cached input | Output |
|---|---|---|---|
| GPT-6 Astra | 250 | 25 | 1,250 |
| GPT-6.1 Sol | 50 | 2.5 | 250 |
| GPT-6 Luna | 2.5 | 0.25 | 12.5 |

Derived from the rates above (arithmetic, not published as multipliers): Sol input is 1/5 of Astra; Luna input is 1/20 of Sol and 1/100 of Astra.

Speed multipliers:
- Fast: 2.5x included usage, 2x purchased credits.
- GPT-6 Astra Ultrafast and GPT-6.1 Sol Ultrafast: 8x included usage, 6x purchased credits.
- Ultrafast available on Pro $500 and eligible Enterprise and Edu plans (models page).
- Image generation uses included limits "~3-5x faster on average."
- Effort multiplier per level: n/a (not published). Pricing page does not mention effort levels.

Promotional: GPT-5.6 Sol promotional pricing "at least through November 21, 2026" (pricing page).

### 2.6 Changelog, last ~2 months

Sources: https://github.com/openai/codex/releases (pages read: 1-5, 7, 9, 11) and https://raw.githubusercontent.com/openai/codex/main/CHANGELOG.md (only an "Unreleased" entry visible; no model content). Seen 2026-10-11. Dates show month and day only; year assumed 2026 from page footer.

Coverage: releases from about 21 Sep to 10 Oct read. Releases from about 11 Aug to 20 Sep: NOT read (n/a).

Model and effort changes found:
- 0.156.1 (23 Sep): "Add GPT-6 Sol and Luna to the model catalog" (#47405); "Choose GPT-6 Sol or GPT-6 Luna from the model picker"; rate-limit switch prompt recommends GPT-6 Luna.
- 0.158.0 (alpha series, 23-25 Sep): no bullets.
- 0.159.0 (29 Sep): usage-limit windows preserved in turn and compaction analytics (#48174); model and access-program pairs preserved during compaction (#48224). Also "removed follow-up prompt suggestions".
- 0.159.1 (29 Sep): "Added GPT-6.1 Sol as the default model in the bundled catalog" (also added to Amazon Bedrock catalogs).
- 0.160.0 (1 Oct): explicit provider model catalogs no longer include unsupported bundled models; server reasoning-summary and verbosity settings preserved.
- 0.161.0 (7 Oct): "GPT-6.1 Sol is now the default model in the bundled and Amazon Bedrock catalogs"; Bedrock supports Ultra reasoning on compatible models.
- 0.162.0 (8 Oct): "Honor server model defaults when starting fresh TUI threads"; "Enable Ultrafast service tiers for Amazon Bedrock Astra models"; "Show model and reasoning effort near the top of task details".
- No effort-level or credit-rate change found in the window on the releases pages read.

---

## 3. Quota weight per model and effort

| Tool | Model | Published weight vs other models | Published weight by effort | Source |
|---|---|---|---|---|
| Claude Code | Opus 5.5 | n/a (official). Third-party claim "about 5x Sonnet" is unverified | "Higher effort uses more tokens", no multiplier: n/a | support.claude.com usage article; code.claude.com costs page |
| Claude Code | Sonnet 5.5 | n/a | n/a | same |
| Claude Code | Haiku 5.5 | n/a. Costs more per token on prompts above 100K (API pricing) | n/a | model-config page |
| Claude Code | Fable 5.1 | May bill to usage credits; separate weekly limit on Max | n/a | model-config; Max article |
| Codex | GPT-6 Astra | 250 / 25 / 1,250 credits per 1M tokens (in / cached / out); Plus 5-hour 5-45 msgs | "takes longer and uses more tokens"; no multiplier: n/a | learn.chatgpt.com pricing and models |
| Codex | GPT-6.1 Sol | 50 / 2.5 / 250 credits per 1M; Plus 5-hour 15-160 msgs | same, n/a | same |
| Codex | GPT-6 Luna | 2.5 / 0.25 / 12.5 credits per 1M; Plus 5-hour 350-3,000 msgs | same, n/a | same |
| Codex | any | Fast 2.5x included usage; Ultrafast (Astra, Sol) 8x included usage | n/a | pricing page |

---

## 4. Vendor guidance (model and effort choice)

Claude Code:
- Sonnet for most coding; Opus for complex architecture or multi-step reasoning (costs page).
- opusplan: Opus for planning, Sonnet for execution (model-config page).
- Subagents for simple tasks: `model: haiku`. Agent teams: Sonnet teammates (costs page).
- Effort: low for quick and reviewed changes, medium for scoped feature work on 5.5 models, high where verification matters, xhigh for deeper reasoning at higher spend, max for hard problems and "prone to overthinking" (model-config page).

Codex:
- GPT-6.1 Sol for complex coding and agentic work; GPT-6 Luna for focused, repeatable, high-volume tasks (models page).
- Start with High on Luna, or Light on Astra (models page).

---

## 5. Gaps (n/a)

- Claude per-plan message or prompt counts: n/a on official pages read.
- Claude per-model quota multipliers: n/a.
- Claude per-effort quota multipliers: n/a.
- Codex per-model effort defaults for Sol: n/a.
- Codex per-plan default model name when unset: n/a ("recommended model").
- Codex Pro per-tier limits: n/a.
- Changelogs Aug 11 to Sep 20 (Codex) and exact Claude version dates before 2.1.287: n/a.
- Codex `codex exec` official example with `-c model_reasoning_effort=...`: n/a (only community sources).
