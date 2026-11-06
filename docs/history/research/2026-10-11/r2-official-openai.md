# OpenAI GPT-6 family: official facts (research r2)

Date seen: 2026-10-11 (all pages below were read on that date).

Source domains: developers.openai.com (API model pages, pricing, changelog, reasoning guide, deprecations, latest-model guide), openai.com (announcement posts; loaded via browser in the pt-BR locale, so figures are translated by me and the numbers are unchanged). Codex docs at developers.openai.com/codex/* redirect (308) to learn.chatgpt.com/docs/* (OpenAI-operated, but outside the three listed domains; used only because the redirect was the official Codex doc). openai.com URLs returned HTTP 403 to WebFetch, so those were read with the browser tool. No SWE-bench Verified/Pro or tau-bench figures appear in any OpenAI page read here.

Note on the "effort" labels in pt-BR posts: "baixo" = low, "médio" = medium, "alto" = high, "muito alto" = xhigh (my inference, not stated), "máximo" = max.

---

## 1. Model inventory (current, per OpenAI)

| Model (API id) | Status per OpenAI | Released | Knowledge cutoff | Source |
|---|---|---|---|---|
| `gpt-6-astra` | Current, flagship ("Our most capable model for the most demanding work") | Sep 3, 2026 (changelog "Released GPT-6 Astra") | Apr 30, 2026 | https://developers.openai.com/api/docs/models/gpt-6-astra ; https://developers.openai.com/api/docs/changelog |
| `gpt-6.1-sol` | Current; Codex page says "Current, recommended, staged rollout" | Sep 29, 2026 (changelog) | Apr 30, 2026 | https://developers.openai.com/api/docs/models/gpt-6.1-sol ; changelog |
| `gpt-6-luna` | Current ("Our most efficient model for focused, high-volume tasks") | Sep 22, 2026 (changelog "Released GPT-6 Sol (gpt-6-sol) and GPT-6 Luna (gpt-6-luna)") | May 18, 2026 | https://developers.openai.com/api/docs/models/gpt-6-luna |
| `gpt-6-sol` | Superseded: model page says "See GPT-6.1 Sol for the newer Sol model". No deprecation date stated. Still listed on pricing page. | Sep 22, 2026 | Apr 20, 2026 | https://developers.openai.com/api/docs/models/gpt-6-sol |

Context and output (identical for all four): context window 1,050,000; max input 922,000; max output 128,000. Source: each model page above.

Modalities (all four): input text + image; output text. No audio, no video, no fine-tuning. Model pages.

Endpoints: Responses API for all tools. Chat Completions: Astra, Luna and Sol 6 supported (Luna and Sol 6 only allow function calling in Chat Completions with `reasoning_effort: "none"`); Sol 6.1 Chat Completions supported for requests without tools. Batch supported on all four. Model pages and https://developers.openai.com/api/docs/guides/latest-model.

---

## 2. Pricing (per 1M tokens, USD) — source https://developers.openai.com/api/docs/pricing (seen 2026-10-11)

Short-context rule (<=272K input tokens) shown. Above 272K input: 2x input and cache rates, 1.5x output for the whole request (model pages and pricing page).

| Model | Input | Cached input | Cache write | Output | Batch / Flex in / cached / out | Fast (in / out) | Ultrafast (in / out) |
|---|---|---|---|---|---|---|---|
| gpt-6-astra | $10.00 | $1.00 | $12.50 | $50.00 | $5.00 / $0.50 / $6.25 / $25.00 | $20.00 / $100.00 | $60.00 / $300.00 |
| gpt-6.1-sol | $2.00 | $0.10 | $2.50 | $10.00 | $1.00 / $0.05 / $1.25 / $5.00 | $4.00 / $20.00 | $12.00 / $60.00 |
| gpt-6-luna | $0.10 | $0.01 | $0.125 | $0.50 | $0.05 / $0.005 / $0.0625 / $0.25 | $0.20 / $1.00 | n/a |
| gpt-6-sol | $2.00 | $0.20 | $2.50 | $10.00 | $1.00 / $0.10 / $1.25 / $5.00 | $4.00 / $20.00 | n/a |

- Flex = same numbers as Batch for every GPT-6 model (pricing page).
- Priority: renamed "Fast mode" on July 30, 2026 (pricing page). No separate "priority" tier for GPT-6.
- Fast = 2x Standard; Ultrafast = 6x Standard (model pages, pricing page).
- Long-context (>272K input) prices: Astra $20 / $2 / $25 / $75; Sol 6.1 $4 / $0.20 / $5 / $15; Luna $0.20 / $0.02 / $0.25 / $0.75 (input / cached / cache write / output).
- Regional processing / data residency: 10% uplift for models released on or after Mar 5, 2026 (pricing page footnote). Luna model page: "10% premium where offered".
- Data residency: Sol 6.1 US and EU (also Fast and Ultrafast; changelog says Ultrafast has US + EU + global). Astra: Fast does not support EU residency; Astra Ultrafast = US residency + global only (latest-model guide). Luna: EU available with Standard, Fast, Flex, Batch (Luna model page).

Codex credits (learn.chatgpt.com/docs/pricing, seen 2026-10-11), credits per 1M tokens input / cached / output at Standard: Astra 250 / 25 / 1,250; Sol 6.1 50 / 2.5 / 250; Sol 6 50 / 5 / 250; Luna 2.5 / 0.25 / 12.5. Fast uses 2.5x subscription usage and 2x purchased-credit usage. Ultrafast (Astra and 6.1 Sol only) uses 8x included usage and 6x purchased-credit usage; listed for "Pro $500 and eligible Enterprise/Edu plans".

---

## 3. Reasoning effort levels

Source: https://developers.openai.com/api/docs/guides/reasoning (seen 2026-10-11), plus model pages.

Support per model:
- gpt-6-astra: `low`, `medium`, `high`, `xhigh`, `max`. Does NOT support `none` (HTTP 400). Default not stated on model page. `minimal` not listed.
- gpt-6.1-sol: `low`, `medium` (default), `high`, `xhigh`, `max`. Does NOT support `none` or `minimal`.
- gpt-6-luna: `none`, `low`, `medium` (default), `high`, `xhigh`, `max`.
- gpt-6-sol: `none`, `low`, `medium` (default), `high`, `xhigh`, `max`.

What OpenAI says each level is for (reasoning guide, quoted in English as fetched):
- `none`: "Latency-critical tasks that do not benefit from any reasoning or multi-chained tool calls." Use cases: "voice, fast information retrieval, and classification."
- `minimal`: listed, no description.
- `low`: "Efficient reasoning with a modest latency increase." "Ideal for use cases requiring tool-use, planning, search, or multi-step decision making."
- `medium`: "Default configuration for most workloads." "When quality and reliability matter, and the task involves planning, complex reasoning, and judgement."
- `high`: "Hard reasoning, complex debugging, deep planning, and high-value tasks"; quality and intelligence matter "more than latency."
- `xhigh`: "Deep research, asynchronous workflows and agentic tasks that require long runs." "Only use when your evals show a clear benefit that justifies the extra latency and cost."
- `max`: "Maximum reasoning for your most complex tasks." "If you are currently using `xhigh`, evaluate if `max` results in stronger performance."
- `ultra`: appears only in the Codex UI/config text (learn.chatgpt.com/docs/models: "Light to Ultra"; "Max and Ultra depend on your settings"). Not an API value in the reasoning guide.

Mid-conversation change: `configuration_update` can raise or lower effort for GPT-6 family in single-agent mode; it changes only effort and does not invalidate the cache (reasoning guide; announcement "Ajuste o esforço de raciocínio ... sem invalidar o cache"). Cannot be combined with automatic compaction/truncation.

Sampling restriction: when effort is not `none`, remove `temperature`, `top_p`, `top_logprobs` (latest-model guide). Astra does not support custom temperature/top_p/logprobs at all (changelog Sep 3).

Default in API: only Sol 6.1 is stated as "defaults to `medium`" in its guide entry; Luna and Sol 6 also default to medium (reasoning guide). Astra default: n/a.

---

## 4. Published benchmarks (exact numbers as printed; effort noted where OpenAI states it)

### gpt-6-astra — https://openai.com/index/gpt-6-astra/ (seen 2026-10-11)
Footnote: "As pontuações das avaliações são as máximas alcançadas em qualquer nível de esforço." = scores are the maximum reached at any effort level. Effort per number is not stated, except Terminal-Bench Science 0.1 where the lower-cost setting gives 61.1%.

- Terminal-Bench 4.0: 57.9% (GPT-5.6 Sol 37.3%; Claude Fable 5.1 55.8%); OpenAI text: estimated API cost per task "aproximadamente 9% e 63% menor, respectivamente" (order of comparators as written; the page does not make the mapping more explicit)
- DeepSWE v1.1: 74.1%
- FrontierCode 1.1 Extended: 64.5%; FrontierCode 1.1 Main: 53.3% (footnote 8: run with a developer message similar to Codex's; prompt not optimized for the eval)
- Terminal-Bench Science 0.1: 64.6% (61.1% at lower-cost setting)
- GPQA Diamond: 96.0%
- Humanity's Last Exam (with tools): 57.2%
- FrontierMath Tier 4 (v2): 97.6% (text says "praticamente 98%")
- ARC-AGI-3: 99.9%; ARC-AGI-2: 95.0%; ARC-AGI-1: 98.5%
- OSWorld 2.0 (offline v2026.08.08, partial score): 72.6% at ~40 min per task (GPT-5.6 Sol 65.7% at ~75 min)
- Agents' Last Exam: 59.3%
- ScreenSpot-Pro (without tools): 92.7%
- AutomationBench: 41.4%
- BrowseComp: 91.5%
- HealthBench Professional (length-adjusted): 63.4%
- ExploitBench: 100.0%; ExploitGym: 42.4%; SRE-Bench: 88.0%; SEC-Bench Pro: 85.4%
- OpenAI MRCR v2 8-needle: 256K-512K 100%; 512K-1M 96.3%
- Internal hallucination benchmark: 4.2% (lower is better)
- Internal computer-use safety benchmark: 2.4% (lower is better)
- SWE-bench Verified / SWE-bench Pro: not published on this page (n/a).
- tau-bench: n/a.

### gpt-6.1-sol — https://openai.com/index/introducing-gpt-6-1-sol/ (seen 2026-10-11)
The post gives relative claims, not absolute scores, for most benchmarks. Effort settings are stated where given.
- DeepSWE v1.1: "se iguala ao GPT-6 Astra" (Astra = 74.1%) at lower effort and cost; +6.4 pp over GPT-6 Sol's best score. Sol 6.1 absolute: n/a.
- AutomationBench: beats Claude Opus 5.5 by 2.2 pp at medium effort (about one third the cost); +4.8 pp over GPT-6 Sol at the same setting. Absolute: n/a.
- OSWorld 2.0 offline: +7 pp over GPT-6 Sol at max effort; 2.1 pp below Astra at max effort. Absolute: n/a.
- Terminal-Bench Science 0.1: more than 2x GPT-6 Sol's score at max effort; average cost $5.47 per task at max (Opus 5.5 $23.21, Astra $23.80). Absolute: n/a.
- GDP.pdf: beats Opus 5.5 at less than half the cost per task; near Astra at about one fifth the cost. Absolute: n/a.
- Factual error rate (hard prompts, low effort): 7.7% vs 11.4% for GPT-6 Sol; within 1.9 pp of Astra at max effort reasoning settings.
- Search-tool failure non-reporting (max effort): Sol 6.1 2.1%; GPT-6 Sol 4.9%; Astra 1.5%; Luna 28.7%.

### gpt-6-sol and gpt-6-luna — https://openai.com/index/introducing-gpt-6-sol-and-luna/ (seen 2026-10-11)
- gpt-6-sol, DeepSWE v1.1: 68.8% at max effort (Fable 5 69.9% at very high effort; Sol is 1.1 pp below).
- gpt-6-sol, AutomationBench: 33.2% at very high effort, $0.27 per task (Astra at low effort 30.3%, 3.9x Sol's cost; Opus 5 at max 26.9%; Fable 5.1 w/ fallback 31.4%).
- gpt-6-sol, Agents' Last Exam: 56.4% at max effort.
- gpt-6-sol, OSWorld 2.0 offline: 60.5% at very high effort (Opus 5 at medium 60.3%).
- gpt-6-luna, DeepSWE v1.1: 66.6% at max effort.
- gpt-6-luna, AutomationBench: +5.4 pp over GPT-5.6 Luna at high effort; cost per task 58% lower.
- Factuality: Sol makes about half the errors of GPT-5.6 Sol; Luna at higher effort matches GPT-5.6 Sol at about 1/100 the cost.

Source note: the GPT-6 Sol and Luna post says competitor figures come from public reports.

### Other benchmark sources checked
- Artificial Analysis (third-party, not official): not used.

---

## 5. OpenAI recommendations by model and task

- gpt-6-astra: "Highest intelligence", for "the most demanding reasoning, coding, and professional work" (latest-model guide). "Choose Astra when a task needs the strongest capability across steps and tools" (Codex models page). Practical guide: "para as tarefas de raciocínio mais difíceis, que exigem o máximo de inteligência." Jane Street quote on programming: "o melhor modelo até hoje para engenharia de software" (Astra post).
- gpt-6.1-sol: "Balanced speed, cost, and intelligence" (latest-model guide). "Near-Astra performance for complex work at a lower cost" (models page). Practical guide: "complex coding, research and computer use." Codex-page wording: use "when available to your account and client" for complex coding and agentic work.
- gpt-6-luna: "Fastest and most cost-effective" (latest-model guide); "Use Luna for focused, repeatable tasks" (Codex models page); practical guide examples: extracting invoice fields, classifying requests, structured summaries.
- Codex default model: not stated. learn.chatgpt.com/docs/models: "If you don't specify a model, the ChatGPT desktop app, Codex CLI, or IDE extension uses a recommended model." Config example uses `gpt-6.1-sol`, not described as the default. Codex default effort: n/a.
- Codex starting effort: the Codex models page (fetched) says to start Sol 6.1 at the client's default effort and lists Astra as "Light (`low` in config)". The "High for GPT-6 Luna" suggestion came only from a WebSearch snippet of the Codex best-practices page; that page was not fetched, so treat it as unverified. Practical guide (fetched): "No Codex, comece com o nível de raciocínio padrão do modelo."
- Speed: Fast (2x price) for lower and more consistent latency in chat or coding tools. Ultrafast (6x price, Astra and 6.1 Sol only) for the fastest token generation; it accelerates token output regardless of effort. Astra Fast: "até o dobro da velocidade ... pelo dobro do preço"; no latency SLA (Astra latest-model guide). Sol 6.1 Ultrafast: "até 8 vezes mais rápida" than standard speed in Codex (Sol 6.1 post).
- Astra caveat: more likely than earlier models to ask clarifying questions, which can stop work early; prompt for autonomous follow-through (latest-model guide).
- Migration: Astra and Sol 6.1 do not accept `none`; use `low`. For `minimal` requests on Sol 6.1, start at `low` and compare (latest-model guide). Tool calling requires Responses API.

---

## 6. Deprecations (https://developers.openai.com/api/docs/deprecations; https://developers.openai.com/api/docs/changelog; seen 2026-10-11)

- GPT-6 family (astra, sol 6.1, luna, sol 6): no deprecation or shutdown date found. gpt-6-sol: superseded by gpt-6.1-sol on its model page; no date.
- gpt-5.3-codex: removed from API Apr 1, 2027; recommended replacement gpt-6-sol.
- gpt-5.4-nano: Apr 1, 2027; replacement gpt-6-luna.
- gpt-5.4-cyber: Oct 1, 2026 (replacement: "the most capable cyber model available to you").
- gpt-5-codex, gpt-5.1-codex, gpt-5.1-codex-max, gpt-5.2-codex: July 23, 2026 (already past); replacement gpt-5.6-sol.
- gpt-5.1-codex-mini: July 23, 2026; replacement gpt-5.6-terra.
- codex-mini-latest: Feb 12, 2026 (past).
- Codex page (learn.chatgpt.com/docs/models): GPT-5.5 retires from ChatGPT, Work and Codex on Oct 14, 2026; GPT-5.3-Codex-Spark retired Sep 14, 2026; GPT-5.4/5.4-mini retired from Codex with ChatGPT sign-in Aug 31, 2026.
- Transcription models (whisper-1, gpt-4o-transcribe, gpt-4o-mini-transcribe, gpt-4o-transcribe-diarize): shutdown Feb 26, 2027 (changelog Aug 26, 2026).

---

## 7. Changelog entries for GPT-6 family (https://developers.openai.com/api/docs/changelog; seen 2026-10-11)

- Sep 3, 2026: Released GPT-6 Astra (v1/responses, v1/chat/completions). Astra does not support `none`, custom temperature/top_p, or logprobs; tool calling requires Responses.
- Sep 22, 2026: Released GPT-6 Sol (gpt-6-sol) and GPT-6 Luna (gpt-6-luna). Standard pricing: Sol $2 input / $0.20 cached / $10 output; Luna $0.10 / $0.01 / $0.50.
- Sep 25, 2026: fix to image encoding for Sol and Luna; rerun evals on image workflows.
- Sep 29, 2026: Ultrafast mode added for GPT-6 Astra (US residency and global only; EU not supported).
- Sep 29, 2026: Released GPT-6.1 Sol (gpt-6.1-sol) "for complex coding and professional work at a lower cost than GPT-6 Astra"; multi-agent in beta.
- Oct 6, 2026: Decisions API beta with gpt-6-luna.
- Oct 7, 2026: chat-latest updated; OpenAI recommends the GPT-6 family for production API use.
- Oct 8, 2026: Ultrafast mode added for GPT-6.1 Sol (service_tier "ultrafast"; US and EU residency plus global; all API users subject to rate limits).
- Earlier GPT-5.x: GPT-5.5 reasoning default changed to medium (Apr 24, 2026). GPT-5.6 family (Jul 2026) added max effort and persisted reasoning.

---

## 8. Caveats OpenAI states

- Benchmark footnotes: Astra scores are max across any effort level; effort per cell not given.
- Astra FrontierCode score used a developer message similar to Codex's own; "não foi otimizado para a avaliação" (footnote 8).
- Astra: no latency SLA on Fast mode (latest-model guide).
- Astra: asks clarifying questions more often (latest-model guide).
- Luna: 28.7% failure to report search-tool failures (vs 1.5% Astra, 2.1% Sol 6.1), max effort (Sol 6.1 post).
- Sol 6.1: not yet in ChatGPT Chat (Sol 6.1 post).
- Enterprise/Edu: Sol 6.1 is off by default until an admin enables it (learn.chatgpt.com/docs/pricing).
- Rate limits: model page shows Build tier 1,000,000 TPM for Sol 6.1, compare page shows Tier 2 at 1,000,000 TPM for the same model. Conflict on OpenAI's own pages; check the dashboard.
- The name "Luna" is also used by GPT-5.6 Luna. The GPT-6 Sol/Luna post says GPT-6 Luna costs $0.20 -> $0.10 input and $1.20 -> $0.50 output versus GPT-5.6 Luna promotional prices; GPT-6 Sol costs $4 -> $2 input and $20 -> $10 output versus GPT-5.6 Sol. Do not mix GPT-5.6 and GPT-6 figures.

---

## 9. Items not found (n/a)

- SWE-bench Verified, SWE-bench Pro, tau-bench: not on any page read.
- Release date on each model page: n/a (taken from the changelog instead).
- Per-level effort descriptions for `minimal`: n/a.
- Codex default model name and default effort: n/a.
- Absolute benchmark scores for gpt-6.1-sol: n/a (relative claims only).
- Latency numbers in seconds/tokens per second: n/a (only relative multipliers and OpenAI's OSWorld time-per-task numbers for Astra).
- Batch and Flex for Astra, Sol 6.1 and Luna: given in pricing table above, not stated on model pages.

## Sources (all visited 2026-10-11)

- https://developers.openai.com/api/docs/models/gpt-6-astra
- https://developers.openai.com/api/docs/models/gpt-6.1-sol
- https://developers.openai.com/api/docs/models/gpt-6-luna
- https://developers.openai.com/api/docs/models/gpt-6-sol
- https://developers.openai.com/api/docs/models
- https://developers.openai.com/api/docs/pricing
- https://developers.openai.com/api/docs/guides/reasoning
- https://developers.openai.com/api/docs/guides/latest-model
- https://developers.openai.com/api/docs/deprecations
- https://developers.openai.com/api/docs/changelog
- https://openai.com/index/gpt-6-astra/ (read as pt-BR)
- https://openai.com/index/introducing-gpt-6-1-sol/ (read as pt-BR)
- https://openai.com/index/introducing-gpt-6-sol-and-luna/ (read as pt-BR)
- https://openai.com/index/practical-guide-building-gpt-6/ (read as pt-BR)
- https://learn.chatgpt.com/docs/models (redirect target of developers.openai.com/codex/models)
- https://learn.chatgpt.com/docs/pricing (redirect target of developers.openai.com/codex/pricing)
