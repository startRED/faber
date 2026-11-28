# Anthropic current models: official-source notes (R2)

Date seen: 2026-10-11 (all pages below were fetched or searched on this date unless noted).
Scope: Claude Fable 5.1, Claude Opus 5.5, Claude Sonnet 5.5, Claude Haiku 5.5. Deprecated and legacy models are listed only where needed for lifecycle context.
"n/a" = the source does not say.

## Sources (official domains only)

| Short name | URL | Page date / note |
|---|---|---|
| Fable 5.1 overview | https://platform.claude.com/docs/en/models/fable-5-1/overview | "Released September 1, 2026" |
| Fable 5.1 announcement | https://www.anthropic.com/claude-fable-and-mythos-5-1 | Dated September 2026 (exact day n/a) |
| Opus 5.5 overview | https://platform.claude.com/docs/en/models/opus-5-5/overview | "Released September 22, 2026" |
| Opus 5.5 announcement | https://www.anthropic.com/claude-opus-5-5 | Page date September 22, 2026 |
| Sonnet 5.5 overview | https://platform.claude.com/docs/en/models/sonnet-5-5/overview | "Released September 28, 2026" |
| Sonnet 5.5 announcement | https://www.anthropic.com/claude-sonnet-5-5 | Release date September 28, 2026 |
| Haiku 5.5 overview | https://platform.claude.com/docs/en/models/haiku-5-5/overview | "Released October 7, 2026" |
| Haiku 5.5 announcement | https://www.anthropic.com/claude-haiku-5-5 | Release date October 7, 2026 |
| Pricing | https://platform.claude.com/docs/en/about-claude/pricing | Undated page, fetched 2026-10-11 |
| Effort | https://platform.claude.com/docs/en/build-with-claude/effort | Undated page, fetched 2026-10-11 |
| Choosing a model | https://platform.claude.com/docs/en/about-claude/models/choosing-a-model | Undated page, fetched 2026-10-11 |
| Deprecations | https://platform.claude.com/docs/en/about-claude/model-deprecations | Latest entry 2026-09-30 |

## 1. Identity, dates, context, output

| Model | API model ID | Bedrock ID | Aliases | Released | Reliable knowledge cutoff | Context | Max output (sync) | Max output (Batch, beta) |
|---|---|---|---|---|---|---|---|---|
| Claude Fable 5.1 | `claude-fable-5-1` | `anthropic.claude-fable-5-1` | n/a (fixed ID; no alias stated on overview) | 2026-09-01 | Jun 2026 (exact day n/a) | 1M (default and max, standard price across the whole window) | 128K | 300K with header `output-300k-2026-03-24` (Batch API section; Fable 5.1 listed under "Max output" note on Opus 5.5 page) |
| Claude Opus 5.5 | `claude-opus-5-5` | `anthropic.claude-opus-5-5` | n/a | 2026-09-22 | Jun 2026 | 1M | 128K | 300K (same beta header) |
| Claude Sonnet 5.5 | `claude-sonnet-5-5` | `anthropic.claude-sonnet-5-5` | n/a | 2026-09-28 | Jun 2026 | 1M | 128K | 300K (same beta header) |
| Claude Haiku 5.5 | `claude-haiku-5-5` | `anthropic.claude-haiku-5-5` | n/a (overview: "fixed model ID with no date suffix and no separate alias" per search summary; the overview page itself does not say "alias") | 2026-10-07 | Jun 2026 | 1M | 128K | 300K (same beta header) |

Notes:
- Google Cloud, Microsoft Foundry and Claude Platform on AWS use the same ID as the Claude API for all four models (per each overview's "Model IDs" table).
- Haiku 5.5 uses the newer tokenizer: about 30% more tokens for the same text than Haiku 4.5 (Haiku overview).
- Opus 5.5 Batch 300K is listed on the overview under "Max output (Batch API, beta)". The Fable 5.1 overview itself does not list a batch row; the 300K batch note appears in the "Max output" footnote on the Opus 5.5 / Sonnet 5.5 / Haiku 5.5 overview pages, which lists the models that support it. Fable 5.1 is not named in that footnote list, so treat its batch 300K support as n/a.
- Mythos 5.1 (`claude-mythos-5-1`): same specs and pricing as Fable 5.1, limited to verified orgs (Cyber Verification Program etc.). Not in scope for general use.

## 2. Pricing (USD per 1M tokens, standard tier)

| Model | Input | Output | Cache write 5m | Cache write 1h | Cache read | Batch input | Batch output |
|---|---|---|---|---|---|---|---|
| Fable 5.1 | $10 | $50 | $12.50 | $20 | $0.25 | n/a on pricing table extract (Batch API = 50% off per overview; derived $5 / $25) | derived $25 |
| Opus 5.5 | $4 | $20 | $5 | $8 | $0.20 | $2 | $10 |
| Sonnet 5.5 | $2 | $10 | $2.50 | $4 | $0.10 | $1 | $5 |
| Haiku 5.5, prompts up to 100K tokens | $0.10 | $0.50 | $0.125 | $0.20 | $0.01 | $0.05 | $0.25 |
| Haiku 5.5, prompts over 100K tokens | $0.50 | $2.50 | $0.625 | $1 | $0.05 | $0.25 | $1.25 |

Other pricing facts:
- Cache read multipliers: Fable 5.1 = 0.025x base; Opus 5.5 and Sonnet 5.5 = 0.05x; others 0.1x (Pricing page footnote; overview "Good to know").
- Sonnet 5.5 cache read: pricing page and overview say $0.10. The Sonnet 5.5 announcement page (as read in the fetch) also says "$0.10 cache reads" in the Speed/Cost section (search snippet had said $0.20 before a later Haiku post said cache reads were halved; the current docs say $0.10).
- Haiku 5.5 long-context tier applies per request, including cache hits. Anthropic says: 90% cheaper than Haiku 4.5 up to 100K-token requests, 50% cheaper above that; ~75% cheaper on average (Haiku announcement footnote).
- 1M-context standard pricing: Anthropic says 4.6+ models except Haiku 5.5 have full 1M context at standard price (Pricing page). Haiku 5.5 is the exception and has the >100K tier above.
- Fast mode (research preview, Claude API first-party only, not with Batch): Opus 5.5 $8 input / $40 output per MTok, up to 2.5x output speed (Pricing page; Opus 5.5 announcement).
- Regional US-only inference (`inference_geo`): 1.1x multiplier on all token prices (Pricing page; 4.6 and later).
- Subscription and credits: Haiku 5.5 announcement lists monthly API credits for Max and Team plans; not a model property, not extracted in detail.

## 3. Effort and thinking

Effort levels: all four models support `low`, `medium`, `high`, `xhigh`, `max` (Effort page, per-model list). Effort is set with `output_config.effort`.

| Model | Thinking | Effort default | Levels supported | Per-message effort (beta) |
|---|---|---|---|---|
| Fable 5.1 | Adaptive, always on, cannot be disabled | `high` | all five | yes |
| Opus 5.5 | Adaptive, always on, cannot be disabled (`thinking: disabled` returns 400 at every effort) | `medium` | all five | yes (API, Google Cloud, Bedrock; Bedrock InvokeModel needs `anthropic_beta` body value; Effort page) |
| Sonnet 5.5 | Adaptive (default); lowest setting `thinking: {"type":"between_tools"}` turns off up-front thinking, works at low/medium/high only (400 at xhigh/max) | `high` | all five | yes; with between_tools effort cannot change mid-conversation |
| Haiku 5.5 | Adaptive; `thinking: disabled` allowed at `high` or below, 400 at xhigh/max | `medium` | all five | yes (API and Google Cloud only; not Bedrock per Effort page) |

Anthropic's own description of each level (Effort page, general table):
- `max`: absolute maximum capability, no constraint on token spending. Use for the deepest reasoning and most thorough analysis.
- `xhigh`: extended capability for long-horizon work. Use for long-running agentic and coding tasks over 30 minutes, with token budgets in the millions.
- `high`: spends as many tokens as needed for excellent results. Use for complex reasoning, difficult coding, agentic tasks.
- `medium`: balanced, moderate token savings. Use for agentic tasks needing a balance of speed, cost and performance.
- `low`: most efficient, significant token savings, some capability reduction. Use for simpler tasks, best speed and lowest cost, such as subagents.
- Effort is a behavioral signal, not a strict token budget.
- Effort applies to all output tokens (text, tool calls, thinking).

Per-model effort guidance (Effort page):
- Fable 5.1: "Start with `high`, the default." Step up to `xhigh` or `max` for the most capability-sensitive agentic and coding work; step down to `medium` or `low` for routine or latency-sensitive work once evals hold. At `high` and above set a large `max_tokens` (hard limit on thinking + text).
- Opus 5.5: effort is the primary control for reasoning depth and cost. Run an effort sweep on your own evals rather than carrying over settings from an earlier model. Set large `max_tokens` at higher levels.
- Sonnet 5.5: "Start with `high` unless your workload is agentic or latency-sensitive." Agentic coding and multistep tool use: start `medium` for well-specified tasks, `high` for harder or longer ones. Chat and latency-sensitive: `medium` or `low`. `xhigh` or `max` only where evals show a gain. For agentic coding set `max_tokens` to 128,000 and stream. Its levels are recalibrated (same level does not equal Sonnet 5's amount of thinking).
- Haiku 5.5: "Start with `medium`", including agentic coding. `low` for chat, short tool tasks, simple high-volume requests (warning: at `low`, long agent prompts are more likely to skip a search, stop early, or skip a check). `high` for knowledge work, longer agent tasks, strict instruction following. `xhigh` or `max` only with an eval-measured gain; compare against Sonnet 5.5.

Anthropic's choosing-a-model page on effort: "Tuning effort is often a better lever than switching models."

## 4. Anthropic's stated purpose and recommendation

- Opus 5.5 overview: "For long-running agentic coding and knowledge work."
- Fable 5.1 overview: "For demanding reasoning and long-horizon agentic work." Use it "when your evals on Claude Opus 5.5 at higher effort still fall short." Mythos 5.1 has the same capabilities for verified orgs.
- Sonnet 5.5 overview: "The best combination of speed and intelligence."
- Haiku 5.5 overview: "For high-volume, latency-sensitive tasks such as classification, extraction, and routing." Also subagent tasks.
- Choosing-a-model matrix (Anthropic): highest capability -> Fable 5.1 (agent sessions for hours, multistep deep research, finished document/spreadsheet/deck). Complex agentic coding and enterprise -> Opus 5.5 (multihour autonomous coding, large refactors, systems engineering, vision-heavy, computer use). Speed and capability for everyday coding, agent, enterprise -> Sonnet 5.5 (code generation, data analysis, content, visual understanding, agentic tool use). Lowest latency and price -> Haiku 5.5 (real-time, high-volume, cost-sensitive with strong reasoning, sub-agent tasks).
- "Most workloads start with Claude Opus 5.5" (choosing-a-model).
- Two starting strategies: efficiency-first (start Haiku 5.5, upgrade only for gaps) or capability-first (start Opus 5.5, then lower effort or downgrade; move to Fable 5.1 only if evals at xhigh/max still fall short).
- Announcement guidance:
  - Opus 5.5 announcement: recommended for complex work (sustained judgment); for codebase-wide migrations, audits, perf optimization; unattended coding with subagents and self-verification; code review; knowledge work; data-heavy research; biology (verification program); cybersecurity (verified practitioners).
  - Sonnet 5.5 announcement: well-scoped everyday tasks, bug fixing, polished docs/slides/spreadsheets; fast iteration on less complex tasks; high-volume workflows. Anthropic says Opus 5.5 remains stronger for complex open-ended work; Sonnet 5.5 "complements Opus 5.5 best at lower effort settings."
  - Haiku 5.5 announcement: high-volume cost-sensitive work (summaries, compaction, DB queries, classification); subagent work alongside Opus 5.5 or Sonnet 5.5; speed-sensitive work (live support, browser use); computer and browser use (beta support in Python and TypeScript SDKs). Anthropic says Sonnet 5.5 and Opus 5.5 remain better for complex agentic coding (Terminal-Bench 4.0 class tasks).
  - Fable 5.1 announcement: strongest for coding and knowledge work; customer-reported results (see benchmarks, customer-reported section).

## 5. Published benchmarks (Anthropic-reported)

All figures are percentages unless noted. Effort/setting is stated only where the source states it.

### Opus 5.5 (announcement, 2026-09-22)
Unless noted, Opus 5.5 results use adaptive thinking at max effort (announcement footnote).

| Benchmark | Opus 5.5 | Notes on setting |
|---|---|---|
| Terminal-Bench 4.0 | 66.4 | xhigh effort (footnote 1); SE +/-2.6 |
| FrontierCode v1.1 (Main) | 54.4 (table); 54.6 (text) | Table = max; text says default effort 54.6 (discrepancy on the page) |
| CursorBench 4.0 | 57.8 (table) | Table = max effort; text says 52.5 at default (medium) effort |
| GDPval-AA v2.1 (Elo) | 1846 | max (implied by footnote); default-effort figure beats GPT-6 Astra max at ~1/5 cost (text) |
| AutomationBench | 40.0 | Zapier evaluation; safeguard interventions counted as failures |
| Humanity's Last Exam (with tools) | 67.7 | max (implied) |
| Terminal-Bench-Science 0.1 | 58.7 | n/a |
| OSWorld 2.1 (partial) | 81.8 | n/a |
| Chartography (with tools) | 89.0 | n/a |
| WANDR | n/a in text (chart only) | Offline web tools, programmatic tool calling, 980K-token task budget |
| SWE-bench Verified / Pro | n/a (not reported) | |

Opus 5.5 page's Fable 5.1 column (same comparison table, different version numbers): TB4.0 55.8; FrontierCode 50.3; CursorBench 4.0 51.8; GDPval-AA v2.1 1735; AutomationBench 31.4; HLE with tools 65.6; TB-Science 52.6; OSWorld 2.1 80.7; Chartography 88.4.

Opus 5.5 vs Opus 5 (same table): Opus 5 TB4.0 52.3; CursorBench 46.6; OSWorld 2.1 74.0; GDPval-AA v2.1 1708.

### Sonnet 5.5 (announcement, 2026-09-28)

| Benchmark | Sonnet 5.5 | Notes on setting |
|---|---|---|
| Terminal-Bench 4.0 | 70.6 | Table effort n/a; text says Medium effort (Claude app default) used for cost comparison |
| FrontierCode 1.1 (Main) | 46.2 (Max, footnote: lower than Xhigh); 52.1 (Xhigh) | Two settings given |
| CursorBench 4.0 | 55.5 | Table effort n/a; text: Low effort for the claim beating Sonnet 5 |
| GDPval-AA v2.1 (Elo) | 1844 | n/a |
| AA-Briefcase v1.1 (Elo) | 1811 | Medium effort (cost comparison in text) |
| Humanity's Last Exam (with tools) | 64.5 | n/a |
| OSWorld 2.1 (partial) | 80.1 | n/a |
| Chartography (no tools) | 61.6 | n/a |

Sonnet 5.5 vs Opus 5.5 in same table: Opus TB4.0 66.4 (xhigh); FrontierCode 54.4; CursorBench 57.8; GDPval-AA 1846; AA-Briefcase 1822; HLE 67.7; OSWorld 81.8; Chartography 64.4 (Opus column). Sonnet 5 values: TB4.0 10.3; CursorBench 34.1.
Footnote from Anthropic: Artificial Analysis ran GDPval-AA and AA-Briefcase on a pre-release deployment with a structured-output bug (fixed); GPT-6 Sol image-understanding bug (fixed).

### Haiku 5.5 (announcement, 2026-10-07)
Effort: Haiku 5.5 "is the first Haiku-class model with an adjustable effort setting". Charts show OSWorld 2.1, GDPval-AA, HLE across Low, Med, High, Xhigh, Max; text gives no per-level numbers. Per-row effort for the table: n/a.

| Benchmark | Haiku 5.5 | Notes |
|---|---|---|
| GDPval-AA v2.1 (Elo) | 1620 | n/a |
| AA-Briefcase v1.1 | 1578 | n/a |
| OSWorld 2.1 (offline subset) | 72.4 | n/a |
| Humanity's Last Exam, no tools | 45.9 | n/a |
| Humanity's Last Exam, with tools | 57.4 | n/a |
| Terminal-Bench 4.0 | 39.2 | n/a |
| FrontierCode 1.1 (Main) | 46.4 | n/a |
| Chartography (no tools) | 46.4 | n/a |

Comparison in same post: Haiku 4.5 TB4.0 0.0, OSWorld 15.7, GDPval-AA 735; GPT-6 Luna TB4.0 16.4, FrontierCode 42.4; Sonnet 5.5 reference TB4.0 70.6, OSWorld 83.9 (this OSWorld value differs from the Sonnet 5.5 post's 80.1; both are on the page, different run/subset).

### Fable 5.1 (announcement, 2026-09-01; page dated "September 2026")
Effort per row: n/a. The page says charts sweep low to max effort, and CursorBench 73.4 is at max effort.

| Benchmark | Fable 5.1 | Notes |
|---|---|---|
| Terminal-Bench 4.0 | 55.8 | Effort n/a. Mythos 5.1 scores 60.9 (same row) |
| Terminal-Bench-Science 0.1 | 52.6 | Public leaderboard setup (3 trials/task, Claude Code harness); SE +/-3.5-4.5 |
| GDPval-AA v2 (Elo) | 1853 | Note: "v2" here, "v2.1" on Opus page (Fable 5.1 = 1735 on that page) |
| OSWorld 2.0 (partial) | 77.9 | Strict 41.7. Note "2.0" here vs "2.1" on Opus page (80.7) |
| Humanity's Last Exam, no tools | 60.9 | n/a |
| Humanity's Last Exam, with tools | 65.0 | n/a |
| AutomationBench | 31.4 | n/a |
| CursorBench 3.2.0 | 73.4 | Max effort. Note "3.2.0" here vs "4.0" on Opus page (51.8) |

Customer-reported (not Anthropic-measured): Browserbase browser-agent 82% task completion (about 10 min each) vs 74% Opus 5 and 57% Fable 5; contract redlining 47.9 -> 57.0 (announcement text); Every: ~2x faster than Opus 5 with half the tokens; Rogo: same accuracy as Fable 5 with 20% fewer tokens.

Non-benchmark Fable 5.1 claims: Venus elevation map (Magellan radar); Mythos 5.1 protein binders and 2.5x inference speedups (Mythos only).

### Fast mode (Opus 5.5)
Research preview. Up to 2.5x output speed. $8 / $40 per MTok. Not available on Batch. Claude API first-party only.

## 6. Speed and latency

- Comparative latency tiers (overview tables): Fable 5.1 "Slower"; Opus 5.5 "Moderate"; Sonnet 5.5 "Fast"; Haiku 5.5 "Fastest".
- Opus 5.5: output more than 30% faster than Opus 5 (announcement). Uses fewer tokens per task (one audit example: under 3 h vs over 20 h for Opus 5, using 2.5x fewer tokens; source: announcement customer quote).
- Sonnet 5.5: output 30%+ faster than Sonnet 5; "our fastest Sonnet model to date" (announcement). Uses far fewer tokens per task, up to 30% less cost per task than Sonnet 5.
- Haiku 5.5: "fastest model Anthropic has released", at each model's standard speed; slower than Opus in Fast Mode (announcement). Asana: >30% lower latency on task completion, up to 2.5x faster inference per agent turn vs its previous model (customer-reported).
- Fable 5.1: no speed claim from Anthropic beyond "Slower" tier; customer-reported ~2x faster than Opus 5 (Every) and fewer tokens (Rogo).

## 7. Caveats, breaking changes, known weaknesses (from Anthropic docs and announcements)

Opus 5.5:
- Thinking cannot be disabled. Requests with `thinking: disabled` return 400 at every effort (Effort page).
- Forced tool use (`tool_choice` any/tool) not supported (error). Thinking blocks tied to model and conversation.
- `computer_20251124` computer-use tool not accepted on Claude API and Google Cloud (use `computer_toolset_20260801`).
- Text between tool calls returns in `thinking` blocks at default `display`.
- Anthropic: benchmark margins are a less reliable guide at this level; own-use gap with Fable 5.1 narrower than scores suggest.
- Safeguards: most cybersecurity tasks routed to Opus 4.8; biology and some other areas fall back. Benchmark numbers likely understated as a result.
- Opus 5.5 often suspects it is being evaluated (announcement caveat).
- Preserved thinking (anti-distillation) applies to API accounts created on or after 2026-08-31.
- Not for Bedrock: per-message effort not available on Bedrock for Opus 5 (Opus 5.5 is on the Bedrock list for per-message effort per the Effort page).

Sonnet 5.5:
- Breaking changes from Sonnet 5: `between_tools` for thinking-off; forced tool use error; thinking blocks tied to model/conversation; `computer_20251124` not accepted on API and Google Cloud; advisor tool rejects Opus 4.8, Opus 4.7, Sonnet 5, Haiku 5.5 as advisors.
- `temperature`, `top_p`, `top_k` non-default values return 400.
- Cyber safeguards route higher-risk cyber requests to Sonnet 5; biology safeguards may flag some microbiology/virology requests.
- Preserved thinking tied to creating account (switching accounts mid-session, including Claude Code, is affected).
- Opus 5.5 remains clearly stronger on complex open-ended work; Opus 5.5 slightly better on automated alignment audit (Anthropic).
- Sonnet 5.5 at max effort scored lower than at xhigh on FrontierCode (out-of-scope changes; subagent code-review skill runs caused timeouts/edits in two cases examined by Cognition).

Haiku 5.5:
- Uses about 30% more tokens for the same text than Haiku 4.5 (tokenizer); slightly more tokens per task.
- Cyber safeguards stricter than Haiku 4.5's; block penetration testing and similar attacker-oriented techniques.
- Biology safeguards match Sonnet 5 and Opus 5.
- Anthropic: Sonnet 5.5 and Opus 5.5 remain better for complex agentic coding (Terminal-Bench 4.0 type).
- `temperature`, `top_p`, `top_k` omit; non-default values 400 (migration guide).
- Thinking blocks work only in the account that produced them or a linked account.
- Long-context tier applies per request including cache hits (Pricing page).
- Haiku 5.5 excluded from the standard-price 1M context (uses >100K tier).

Fable 5.1:
- Forced tool use returns an error. Only Fable 5.1 and Mythos 5.1 can read Fable 5.1 thinking blocks. Editing earlier turns invalidates thinking blocks.
- Cybersecurity: can discover vulnerabilities but not develop exploits (announcement). Newest cyber safeguards block 60% fewer false positives than before.
- Safeguard impact on benchmarks: on tasks where safeguards intervened, Fable 5.1 scored zero on OSWorld 2.0 (Fable 5 also zero on AutomationBench).
- Routing: life-sciences R&D queries and dual-use cyber tasks (pentest, exploit generation, binary vuln scanning) redirected to Opus models (Fable page / system card summary).
- Classifier-flagged requests fall back to Opus 4.8 on most interfaces (system card, read via search summary, not fetched directly on the official page; treat as "per search summary, unverified").
- Model can still sometimes bypass approvals and auto-mode classifiers (system card caveat).
- New accounts can no longer edit prior context while keeping earlier thinking.
- Fable 5.1 is the only model here whose default is `high` and that is "Slower".

## 8. Deprecation and retirement (from the deprecation page, 2026-10-11)

| API model ID | Status | Retirement (tentative, "not sooner than") |
|---|---|---|
| claude-fable-5-1 | Active | 2027-09-01 |
| claude-opus-5-5 | Active | 2027-09-22 |
| claude-sonnet-5-5 | Active | 2027-09-28 |
| claude-haiku-5-5 | Active | 2027-10-07 |

Other context: claude-fable-5 active, not sooner than 2027-06-09; claude-opus-5 active, not sooner than 2027-07-24; claude-sonnet-5 active, not sooner than 2027-06-30; claude-haiku-4-5-20251001 active, not sooner than 2026-10-15; claude-sonnet-4-5-20250929 deprecated 2026-09-30, retire 2026-11-30 (replacement claude-sonnet-5-5). Notice policy: at least 60 days notice for publicly released models.

## 9. Discrepancies noticed on official pages

- Opus 5.5 announcement: FrontierCode 54.4 (table) vs 54.6 (text, default effort).
- Opus 5.5 announcement: CursorBench 57.8 (max, table) vs 52.5 (default, text).
- Fable 5.1 page vs Opus 5.5 page: different benchmark versions for the same name (CursorBench 3.2.0 vs 4.0; OSWorld 2.0 vs 2.1; GDPval-AA v2 vs v2.1). Scores are not comparable across these pages.
- Sonnet 5.5 cache read: $0.10 on pricing and overview pages; an earlier search summary said $0.20 (the Haiku post said cache reads were halved). Use $0.10.
- Opus 5.5 "Max output (Batch API)" note lists the models that support 300K batch output, which does not include Fable 5.1 by name. Treat Fable 5.1 batch 300K as n/a.
- Fable 5.1 batch price: the pricing extract did not include the Fable row for batch, only the general 50% rule (Overview and Pricing). Derived $5 / $25, marked derived.
- Haiku 5.5 "fixed model ID with no date suffix and no separate alias": stated in a search summary, not on the overview page.

## 10. Items asked for but not found

- SWE-bench Verified and SWE-bench Pro: not reported for any of the four on the fetched official pages. n/a.
- Exact knowledge-cutoff day: n/a (month only: Jun 2026).
- Formal alias list (e.g., "-latest") for each model: n/a in fetched pages.
- Per-level (low/medium/high/xhigh/max) benchmark numbers: n/a for all four (charts only; text gives a few point values as listed above).
- Haiku 5.5 OSWorld per-effort results: charts only; n/a in text.
