# OpenAI models on Artificial Analysis: deep dive

Date seen: 2026-10-11
Provider page: https://artificialanalysis.ai/providers/openai (page text says "updated Sep 26, 2026"; the page header says 81 models; the table was truncated in the extraction)
Model pages: https://artificialanalysis.ai/models/<slug>

## Method and caveats

- 22 model pages read with WebFetch (summaries produced by a small model from the page HTML). Numbers below are as extracted.
- Verified in the rendered page (Browser, get_page_text) for `gpt-6-1-sol` (max): the per-benchmark charts all read "Not publicly available". The Intelligence Index is listed with its 10 evaluations by name only.
- Rendered tooltip on the latency chart: "Seconds to first answer token received · Accounts for reasoning model 'thinking' time". The "TTFT" values below are the page's time-to-first-answer-token figures, so they include thinking time. That explains the 100 to 400 s values on high/xhigh/max efforts.
- Result: **no model page publishes per-benchmark scores** (SWE-bench, Terminal-Bench, tau2, GPQA Diamond, HLE, AIME, AA-LCR, IFBench, AA-Omniscience accuracy/hallucination, coding index, agentic index are all n/a). The only quality metric available is the composite Intelligence Index v4.3.2 (10 evaluations).
- Cache discount: 95% on Sol 6.1 and GPT-5.6 Sol; 90% on Astra and Luna. No absolute cache-hit price on any page, so cache prices are n/a.
- Total cost to run the full index: n/a on every page. Cost is shown per task only.
- Ranks: "/227" is the reasoning-model class on the Sol/Astra pages; Luna pages rank "/180" (a different class).

## Current OpenAI models (effort variants listed separately)

| Model | Slug | Index | Rank | Cost per index task | Index output tokens | Output tok/s | TTFT s (incl. thinking) | $/1M in | $/1M out | Context |
|---|---|---|---|---|---|---|---|---|---|---|
| GPT-6 Astra (max) | gpt-6-astra | 53 | #7/227 | $3.26 | 60M | 63.3 | 391.21 | 10.00 | 50.00 | 1M |
| GPT-6 Astra (xhigh) | gpt-6-astra-xhigh | 52 | #9/227 | $2.31 | 38M | 61.5 | 181.28 | 10.00 | 50.00 | 1M |
| GPT-6 Astra (high) | gpt-6-astra-high | 51 | #15/227 | $1.73 | 26M | 56.9 | 101.55 | 10.00 | 50.00 | 1M |
| GPT-6 Astra (medium) | gpt-6-astra-medium | 50 | #20/227 | $1.54 | 19M | 54.7 | 8.01 | 10.00 | 50.00 | 1M |
| GPT-6 Astra (low) | gpt-6-astra-low | 46 | #31/227 | $0.82 | 10M | 50.5 | 2.35 | 10.00 | 50.00 | 1M |
| GPT-6.1 Sol (max) | gpt-6-1-sol | 52 | #11/227 | $0.72 | 67M | 65.4 | 347.78 | 2.00 | 10.00 | 1M |
| GPT-6.1 Sol (xhigh) | gpt-6-1-sol-xhigh | 51 | #14/227 | $0.39 | 36M | 60.4 | 173.40 | 2.00 | 10.00 | 1M |
| GPT-6.1 Sol (high) | gpt-6-1-sol-high | 50 | #17/227 | $0.32 | 25M | 58.9 | 64.60 | 2.00 | 10.00 | 1M |
| GPT-6.1 Sol (medium) | gpt-6-1-sol-medium | 48 | #24/227 | $0.21 | 15M | 55.4 | 7.53 | 2.00 | 10.00 | 1M |
| GPT-6.1 Sol (low) | gpt-6-1-sol-low | 42 | #47/227 | $0.13 | 9.0M | 54.1 | 2.50 | 2.00 | 10.00 | 1M |
| GPT-6 Luna (max) | gpt-6-luna | 38 | #8/180 | $0.07 | 140M | 140.3 | 102.41 | 0.10 | 0.50 | 1M |
| GPT-6 Luna (medium) | gpt-6-luna-medium | 30 | #21/180 | $0.02 | 29M | n/a | n/a | 0.10 | 0.50 | 1M |
| GPT-6 Luna (low) | gpt-6-luna-low | 22 | #43/180 | $0.0045 | 8.2M | 139.7 | 2.55 | 0.10 | 0.50 | 1M |
| GPT-5.3 Codex (xhigh) | gpt-5-3-codex | 33 (estimate) | #86/227 | n/a | n/a | 114.8 | 72.79 | 1.75 | 14.00 | 400k |

Notes on this table:
- Luna high (index 33, from the provider summary) was not opened as its own page, so it is not in the table.
- GPT-5.3 Codex is the only Codex model on the provider list. Released Feb 2026, knowledge cutoff Aug 2025. Its index is labeled "estimated; independent evaluation forthcoming".
- Medians on the pages (reasoning class): index 26, output speed 83.3 tok/s, TTFT 3.79 s, output tokens per index run 82M. Luna class: index 13, speed 107.1, TTFT 2.09 s, tokens 100M.
- Input and output prices for Sol 6.1 equal the class medians ($2 / $10). Astra is 5x that ($10 / $50) and the page calls it "expensive". Luna is below median on both prices.

## Deprecated (listed on provider page, not current)

| Model | Slug | Index | Cost per task | Output tokens | Tok/s | TTFT s | $/1M in / out | Note |
|---|---|---|---|---|---|---|---|---|
| GPT-6 Sol (max) | gpt-6-sol | 48 | $1.04 | 77M | 117.1 | 144.05 | 2 / 10 | Page says deprecated; replaced by GPT-6.1 Sol |
| GPT-5.6 Sol (max) | gpt-5-6-sol | 47 | $1.99 | 90M | 90.5 | 123.96 | 4 / 20 | Page says deprecated; replaced by GPT-6 Sol |

Not opened (from provider summary only): GPT-6 Sol (xhigh 44, high 42, medium 40, low 34, non-reasoning 29), GPT-5.6 Terra (max 42), GPT-5.6 Luna (max 37), GPT-5.5 (xhigh 38), GPT-5.4 family, o3-pro, o3, o4-mini, and older models.

## Per-model detail

### GPT-6 Astra (max), slug gpt-6-astra
- Index 53 (rank #7/227). Released Sep 2026 (FAQ: Sep 3, 2026). Knowledge cutoff Apr 30, 2026.
- Speed 63.3 tok/s (rank #124). TTFT 391.21 s. End-to-end 500 tokens: n/a.
- $10 in / $50 out, blended $7.70. Cache: 90% discount (no absolute price).
- Index run: $3.26 per task (rank not stated); 60M output tokens (page: "more concise than median 82M").
- Page comparisons: index "well above average"; price "expensive"; speed "slower than average"; latency at high end.

### GPT-6 Astra (xhigh), slug gpt-6-astra-xhigh
- Index 52 (rank #9/227). Speed 61.5 tok/s (rank #130). TTFT 181.28 s.
- $10 / $50; blended $7.70. Index run $2.31 per task (rank #87/227); 38M tokens.
- Page: "expensive versus similar-priced models"; "fairly concise"; speed slower than average.

### GPT-6 Astra (high), slug gpt-6-astra-high
- Index 51 (rank #15/227). Speed 56.9 tok/s (rank #146). TTFT 101.55 s.
- Index run $1.73 per task (rank #76/227); 26M tokens (rank #23/227).
- Page: "very concise"; input and output price "expensive".

### GPT-6 Astra (medium), slug gpt-6-astra-medium
- Index 50 (rank #20/227). Speed 54.7 tok/s (rank #158). TTFT 8.01 s.
- Index run $1.54 per task (rank #72/227); 19M tokens (rank #13/227).
- Page: "amongst the leading models in intelligence" but "particularly expensive relative to similarly priced models"; "notably slow", "highly concise".

### GPT-6 Astra (low), slug gpt-6-astra-low
- Index 46 (rank #31/227). Speed 50.5 tok/s (rank #172). TTFT 2.35 s.
- Index run $0.82 per task; 10M tokens.
- Page: "well above average" on index; "notably slow"; "highly concise".

### GPT-6.1 Sol (max), slug gpt-6-1-sol
- Index 52 (rank #11/227). Released Sep 2026 (FAQ: Sep 29, 2026). Proprietary, text and image in, text out.
- Speed 65.4 tok/s (rank #118). TTFT 347.78 s (includes thinking). End-to-end: n/a.
- $2 in / $10 out. Cache discount 95%. Blended $1.47.
- Index run $0.72 per task (rank #39/227); 67M tokens (rank #49/227).
- Page: "amongst the leading models in intelligence and reasonably priced"; "slower than average"; "fairly concise".
- Verified in rendered page: per-benchmark breakdown not published.

### GPT-6.1 Sol (xhigh), slug gpt-6-1-sol-xhigh
- Index 51 (rank #14/227). Speed 60.4 tok/s (rank #136). TTFT 173.40 s.
- Index run $0.39 per task (rank #23/227); 36M tokens (rank #29/227). Blended $1.47.
- Page: "very concise" vs median 82M (the 36M figure is below the median); speed "slower than average"; TTFT "at higher end".

### GPT-6.1 Sol (high), slug gpt-6-1-sol-high
- Index 50 (rank #17/227). Speed 58.9 tok/s (rank #141). TTFT 64.60 s.
- Index run $0.32 per task (rank #14/227); 25M tokens (rank #23/227). Blended $1.47.
- Page: "well above average", "very concise"; price at median; speed "slower than average".

### GPT-6.1 Sol (medium), slug gpt-6-1-sol-medium
- Index 48 (rank #24/227). Speed 55.4 tok/s (rank #154). TTFT 7.53 s.
- Index run $0.21 per task (rank #9/227); 15M tokens (rank #10/227). Blended $1.47.
- Page: speed "notably slow"; TTFT "somewhat higher than average".

### GPT-6.1 Sol (low), slug gpt-6-1-sol-low
- Index 42 (rank #47/227). Speed 54.1 tok/s (rank #159). TTFT 2.50 s (better than median).
- Index run $0.13 per task (rank #1/227); 9.0M tokens (rank #4/227). Blended $1.47.
- Page: "well above average" on index; "very concise"; "notably slow".

### GPT-6 Luna (max), slug gpt-6-luna
- Index 38 (rank #8/180). Released Sep 2026 (FAQ: Sep 22, 2026). Proprietary.
- Speed 140.3 tok/s (rank #52/180; class median 107.1). TTFT 102.41 s (class median 2.09 s).
- $0.10 in / $0.50 out (medians $0.24 / $0.88). Blended $0.08. Cache discount 90%.
- Index run $0.07 per task; 140M tokens (class median 100M, "somewhat verbose").
- Available via 7 API providers.

### GPT-6 Luna (medium), slug gpt-6-luna-medium
- Index 30 (rank #21/180). Cost rank #8/180. Verbosity rank #12/180.
- Speed and TTFT: n/a (page shows N/A / Unknown).
- $0.10 / $0.50. Index run $0.02 per task; 29M tokens.
- Page: "well priced" against similar-price models; "very concise" vs median 100M.

### GPT-6 Luna (low), slug gpt-6-luna-low
- Index 22 (rank #43/180). Speed 139.7 tok/s (rank #53/180). TTFT 2.55 s (median 2.09 s).
- $0.10 / $0.50. Index run $0.0045 per task (rank #1/180); 8.2M tokens (rank #5/180).
- Page: "amongst the leading models in intelligence" (for its class); "faster than average"; "highly concise".

### GPT-5.3 Codex (xhigh), slug gpt-5-3-codex
- Index 33, labeled estimated; rank #86/227. Released Feb 2026 (FAQ: Feb 5, 2026). Knowledge cutoff Aug 31, 2025.
- Speed 114.8 tok/s (rank #49/227; "faster than average"). TTFT 72.79 s.
- $1.75 in / $14.00 out. Cache discount 90%. Blended $1.87. Context 400k.
- Cost to run index, tokens for index: n/a.
- Page: index "above average"; output price "somewhat expensive"; TTFT "at the higher end".

## Effort scaling (same model)

| Effort | Sol 6.1 index | Sol 6.1 $/task | Sol 6.1 out tokens | Sol 6.1 TTFT s | Astra index | Astra $/task | Astra out tokens | Astra TTFT s | Luna index | Luna $/task | Luna out tokens |
|---|---|---|---|---|---|---|---|---|---|---|---|
| low | 42 | 0.13 | 9.0M | 2.50 | 46 | 0.82 | 10M | 2.35 | 22 | 0.0045 | 8.2M |
| medium | 48 | 0.21 | 15M | 7.53 | 50 | 1.54 | 19M | 8.01 | 30 | 0.02 | 29M |
| high | 50 | 0.32 | 25M | 64.60 | 51 | 1.73 | 26M | 101.55 | 33 (provider summary; not opened) | n/a | n/a |
| xhigh | 51 | 0.39 | 36M | 173.40 | 52 | 2.31 | 38M | 181.28 | n/a | n/a | n/a |
| max | 52 | 0.72 | 67M | 347.78 | 53 | 3.26 | 60M | 391.21 | 38 | 0.07 | 140M |

Observations from the data:
- Going from low to max on Sol 6.1 adds 10 index points (42 to 52) for 5.5x cost per task ($0.13 to $0.72), 7.4x output tokens, and about 140x TTFT (2.5 s to 348 s, includes thinking).
- Sol 6.1 max (index 52, $0.72) matches Astra xhigh (52, $2.31) at about one-third of the cost per task.
- Sol 6.1 high (index 50, $0.32, TTFT 65 s) matches Astra medium (50, $1.54, TTFT 8 s) at about one-fifth of the cost; Astra medium is faster to first answer.
- Output speed rises with effort on both families (Sol 54 to 65 tok/s; Astra 50 to 63 tok/s). This is the page's measured value; no explanation given.

## Page-level comparison statements (verbatim paraphrase)

- Sol 6.1 max: "amongst the leading models in intelligence and reasonably priced"; "slower than average, however fairly concise".
- Astra max: "well above average" on index; "expensive" pricing; "slower than the median"; latency at high end.
- Luna max: "well above the median of 13 for comparable reasoning models in its price tier"; prices "competitive".
- The page does not compare to named competitors.
