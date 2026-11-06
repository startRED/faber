# Anthropic models on Artificial Analysis

Source: https://artificialanalysis.ai/providers/anthropic (table, read via browser text on 2026-10-11)
Per-model pages: https://artificialanalysis.ai/models/<slug>
Verified with browser get_page_text: claude-haiku-5-5, claude-opus-5-5, and the provider table.
Extracted with WebFetch (small-model summary, not re-checked by hand): claude-sonnet-5-5, claude-fable-5-1, claude-opus-5, claude-sonnet-5, claude-fable-5, claude-opus-4-8.
Note: the page says "Total 46 models" but the table lists 51 rows.

## Global notes

- "*" on Intelligence Index values is not defined in the visible page text.
- Names with "(with fallback)" are the page's labels. The page header for Opus 5.5 reads "(Max, Default Fallback)". The page does not define the fallback in the text I read.
- Coding index, agentic index and math/reasoning sub-indexes are not published as numbers on the provider or model pages. The Intelligence Index v4.3.2 is built from Terminal-Bench 4.0, SciCode, AA-Briefcase, GDPval-AA, AutomationBench, HLE, GDP.pdf, CritPt, AA-Omniscience and AA-LCR. Individual scores are behind the Benchmarks tab, which I did not open.
- API model ids: not shown on any page. The "slug" in the URL (e.g. claude-opus-5-5) is not confirmed as the API id.
- Release dates: taken from each model page's header or FAQ, as listed below.
- Time to first token (First Chunk) values look anomalous for several max variants (e.g. Opus 5.5 max 748.61 s, Haiku 5.5 max 300.00 s). The page shows them as Anthropic API figures; verify before relying on them.
- Cost per task is the page's "weighted average cost per Intelligence Index task".

## Current and recently current models (detailed)

### Claude Opus 5.5 (max, default fallback) - slug claude-opus-5-5 - VERIFIED
- Released: September 2026 (FAQ: 22 Sep 2026)
- Context: 1M; max output: n/a
- Price: $4.00 in / $20.00 out per 1M; cache discount 95%; cached input price n/a; blended (7:2:1) $2.94
- Intelligence Index: 58 (#1 of 227)
- Coding / agentic / math sub-indexes: n/a
- Output speed: 97.0 tok/s (#74 of 227)
- TTFT: 748.61 s (page, Anthropic API), median 3.79 s per the page; Total response 753.76 s
- Reasoning: yes (non-reasoning variant may also exist). Effort: only "max" named in the page title; other variants listed in the table: xhigh, high, medium, low ("with fallback")
- Cost per AA index task: $5.98; output tokens used on index: 260M (median 82M)
- Text + image input, text output
- Page note: "amongst the leading models in intelligence, but particularly expensive ... faster than average, however very verbose"

### Claude Sonnet 5.5 (max, default fallback) - slug claude-sonnet-5-5 - WebFetch extraction
- Released: September 2026 (FAQ: 28 Sep 2026)
- Context: 1M; max output: n/a
- Price: $2.00 in / $10.00 out per 1M; cache discount 95%; cached price n/a; blended $1.47
- Intelligence Index: 56 (#2 of 227)
- Output speed: 137.4 tok/s
- TTFT: 492.96 s (page, Anthropic API) - page-described as at the higher end; Total 496.60 s
- Cost per AA index task: $5.46; output tokens on index: 420M
- Reasoning: yes; effort: n/a beyond "max" label; table variants: xhigh (52, 108 tok/s, $2.01/task), high (47, 109 tok/s, $0.88/task), medium (41, 106 tok/s, $0.48/task), low (36, 106 tok/s, $0.35/task)
- Text + image input, text output
- Page note: "amongst the leading models in intelligence and reasonably priced ... notably fast, however very verbose"

### Claude Fable 5.1 (max, default fallback) - slug claude-fable-5-1 - WebFetch extraction
- Released: September 2026 (FAQ: 1 Sep 2026)
- Context: 1M; max output: n/a
- Price: $10.00 in / $50.00 out per 1M; cache discount 98%; cached price n/a; blended $7.17
- Intelligence Index: 53 (#5 of 227)
- Output speed: 65.3 tok/s (#119 of 227)
- TTFT: 259.93 s (page, Anthropic API); Total 267.59 s
- Cost per AA index task: $7.63; output tokens on index: 190M
- Reasoning: yes; effort: only "max" named; table variants: xhigh (53, 61 tok/s, $5.98/task), high (51, 56 tok/s, $3.91/task), medium (49, $2.98/task), low (47, $2.37/task)
- Page note: "amongst the leading models in intelligence, but particularly expensive"; no use-case note

### Claude Haiku 5.5 (max) - slug claude-haiku-5-5 - VERIFIED
- Released: October 2026 (FAQ: 7 Oct 2026)
- Context: 1M; max output: n/a
- Price: $0.10 in / $0.50 out per 1M; cache discount 90%; cached price n/a; blended $0.08 (the "$0.08" in the provider table's "Blended price" column is the blended figure, not the input price)
- Intelligence Index: 43 (#2 of 180 in its class)
- Output speed: 235.0 tok/s (#14 of 180 in class; table #1 of 46 Anthropic variants)
- TTFT: 300.00 s (page, Anthropic API); median per page 2.09 s; Total 302.13 s
- Cost per AA index task: $0.21; output tokens on index: 440M (median 100M, "very verbose")
- Reasoning: yes; effort levels in table: max (43), xhigh (41), high (38), medium (34), low (29)
- Text + image input, text output
- Page note: "amongst the leading models in intelligence and well priced ... notably fast, however very verbose"

### Claude Opus 5 (max) - slug claude-opus-5 - DEPRECATED per page - WebFetch extraction
- Released: July 2026 (FAQ: 24 Jul 2026). Page says Anthropic has launched Opus 5.5 and suggests it instead.
- Context: 1M; max output: n/a
- Price: $5.00 in / $25.00 out per 1M; cache discount 90%; blended $3.85
- Intelligence Index: 51 (#16 of 227)
- Output speed: 53.7 tok/s (#164 of 227)
- TTFT: 67.21 s (page); Total 76.52 s
- Cost per AA index task: $5.86; output tokens on index: 140M
- Reasoning: yes; effort: only "max" named; table variants: xhigh (50), high (48), medium (45), low (39)
- Page note: "particularly expensive, notably slow, and very verbose"

### Claude Fable 5 - slug claude-fable-5 - DEPRECATED per page - WebFetch extraction
- Released: June 2026 (FAQ: 9 Jun 2026). Page says Fable 5.1 is available and suggests it instead.
- Context: 1M (FAQ: 1.0M); max output: n/a
- Price: $10.00 in / $50.00 out per 1M; cache discount 90%; blended $7.70
- Intelligence Index: 50 (#19 of 227)
- Output speed: 65.5 tok/s (#117 of 227)
- TTFT: 135.45 s (page); Total 143.08 s
- Cost per AA index task: $8.75; output tokens on index: 130M
- Reasoning: yes; effort: n/a
- Page note: "particularly expensive ... slower than average, somewhat verbose"

### Claude Sonnet 5 (max) - slug claude-sonnet-5 - DEPRECATED per page - WebFetch extraction
- Released: June 2026 (FAQ: 30 Jun 2026). Page suggests Sonnet 5.5 instead.
- Context: 1M; max output: n/a
- Price: $2.00 in / $10.00 out per 1M; cache discount 90%; cached price n/a
- Intelligence Index: 38 (#66 of 227)
- Output speed: 78.7 tok/s
- TTFT: 213.66 s (page); Total 220.01 s
- Cost per AA index task: $5.09; output tokens on index: 370M
- Reasoning: yes; effort: n/a; table variants: xhigh (34), high (32), medium (28), low (24)
- Page note: "above average in intelligence, but slower than average, very verbose, somewhat expensive"

### Claude Opus 4.8 (max) - slug claude-opus-4-8 - DEPRECATED per page - WebFetch extraction
- Released: May 2026 (FAQ: 28 May 2026). Page suggests Opus 5 instead.
- Context: 1M (FAQ: 1.0M); max output: n/a
- Price: $5.00 in / $25.00 out per 1M; cache discount 90%; blended $3.85
- Intelligence Index: 42 (#49 of 227)
- Output speed: 59.6 tok/s
- TTFT: 40.47 s (page); Total 48.86 s
- Cost per AA index task: $4.08; output tokens on index: 170M
- Reasoning: yes; effort: n/a
- Page note: "amongst the leading models in intelligence, but particularly expensive"

## Skipped (not opened individually)

These are older generations. The page does not mark them deprecated, but they are clearly superseded by the 5.x line. Their table rows are kept in the full list below.
- Claude Opus 4.7 (max; non-reasoning), Claude Opus 4.6 (max; non-reasoning), Claude Sonnet 4.6 (max; non-reasoning variants)
- Claude Opus 4.5, Claude 4.1 Opus, Claude 4 Opus, Claude 3 Opus
- Claude 4.5 Sonnet, Claude 4.5 Haiku
The Opus 4.7 / 4.6 / Sonnet 4.6 rows are still in the table below as data, but I did not open their model pages.

## Full provider table (51 rows, as displayed)

Columns: model (page name) | context | AA Intelligence Index | cost per AA task | median output tok/s | First chunk TTFT (s) | total response for 500 tokens (s)

| # | Model (page name) | Context | AA Index | Cost/task | tok/s | TTFT s | Total s |
|---|---|---|---|---|---|---|---|
| 1 | Claude Opus 5.5 (max with fallback) | 1M | 58 | $5.98 | 97 | 748.61 | 753.76 |
| 2 | Claude Sonnet 5.5 (max with fallback) | 1M | 56 | $5.46 | 137 | 492.96 | 496.60 |
| 3 | Claude Opus 5.5 (xhigh with fallback) | 1M | 56 | $3.46 | 84 | 154.80 | 160.77 |
| 4 | Claude Opus 5.5 (high with fallback) | 1M | 54 | $1.82 | 83 | 44.34 | 50.39 |
| 5 | Claude Fable 5.1 (max with fallback) | 1M | 53 | $7.63 | 65 | 259.93 | 267.59 |
| 6 | Claude Fable 5.1 (xhigh with fallback) | 1M | 53 | $5.98 | 61 | 106.04 | 114.27 |
| 7 | Claude Sonnet 5.5 (xhigh with fallback) | 1M | 52 | $2.01 | 108 | 37.67 | 42.31 |
| 8 | Claude Opus 5.5 (medium with fallback) | 1M | 51 | $1.34 | 83 | 21.54 | 27.57 |
| 9 | Claude Fable 5.1 (high with fallback) | 1M | 51 | $3.91 | 56 | 19.94 | 28.81 |
| 10 | Claude Opus 5 (max) | 1M | 51 | $5.86 | 54 | 67.21 | 76.52 |
| 11 | Claude Opus 5 (xhigh) | 1M | 50 | $4.88 | 54 | 31.99 | 41.27 |
| 12 | Claude Fable 5 (with fallback) | 1M | 50 | $8.75 | 66 | 135.45 | 143.08 |
| 13 | Claude Fable 5.1 (medium with fallback) | 1M | 49 | $2.98 | 56 | 6.69 | 15.57 |
| 14 | Claude Opus 5 (high) | 1M | 48 | $3.61 | 54 | 22.01 | 31.27 |
| 15 | Claude Fable 5.1 (low with fallback) | 1M | 47 | $2.37 | 57 | 2.37 | 11.12 |
| 16 | Claude Sonnet 5.5 (high with fallback) | 1M | 47 | $0.88 | 109 | 12.38 | 16.98 |
| 17 | Claude Opus 5 (medium) | 1M | 45 | $2.19 | 53 | 13.16 | 22.54 |
| 18 | Claude Haiku 5.5 (max) | 1M | 43 | $0.21 | 235 | 300.00 | 302.13 |
| 19 | Claude Opus 5.5 (low with fallback) | 1M | 42 | $0.55 | 80 | 10.29 | 16.54 |
| 20 | Claude Opus 4.8 (max) | 1M | 42 | $4.08 | 60 | 40.47 | 48.86 |
| 21 | Claude Haiku 5.5 (xhigh) | 1M | 41 | $0.12 | 192 | 66.24 | 68.84 |
| 22 | Claude Sonnet 5.5 (medium with fallback) | 1M | 41 | $0.48 | 106 | 0.89 | 5.61 |
| 23 | Claude Opus 4.7 (max) | 1M | 41* | n/a | 53 | 22.49 | 32.00 |
| 24 | Claude Opus 5 (low) | 1M | 39 | $1.10 | 54 | 2.40 | 11.70 |
| 25 | Claude Sonnet 5 (max) | 1M | 38 | $5.09 | 79 | 213.66 | 220.01 |
| 26 | Claude Haiku 5.5 (high) | 1M | 38 | $0.08 | 179 | 24.32 | 27.12 |
| 27 | Claude Sonnet 5.5 (low with fallback) | 1M | 36 | $0.35 | 106 | 0.57 | 5.31 |
| 28 | Claude Haiku 5.5 (medium) | 1M | 34 | $0.05 | 175 | 13.67 | 16.51 |
| 29 | Claude Sonnet 5 (xhigh) | 1M | 34 | $2.87 | 70 | 39.86 | 46.99 |
| 30 | Claude Opus 4.6 (max) | 1M | 32* | n/a | 46 | 25.99 | 36.94 |
| 31 | Claude Sonnet 5 (high) | 1M | 32 | $1.79 | 68 | 12.18 | 19.54 |
| 32 | Claude Opus 4.7 (non-reasoning, high) | 1M | 31* | n/a | 47 | 1.40 | 11.97 |
| 33 | Claude Sonnet 4.6 (max) | 1M | 30 | $2.49 | 61 | 124.24 | 132.45 |
| 34 | Claude Haiku 5.5 (low) | 1M | 29 | $0.02 | 176 | 9.40 | 12.25 |
| 35 | Claude Opus 4.5 | 200k | 29* | n/a | 55 | 18.24 | 27.30 |
| 36 | Claude Sonnet 5 (medium) | 1M | 28 | $1.00 | 66 | 2.41 | 9.99 |
| 37 | Claude Opus 4.6 (non-reasoning, high) | 1M | 26* | n/a | 41 | 2.09 | 14.24 |
| 38 | Claude Sonnet 4.6 (non-reasoning, high) | 1M | 25* | n/a | 45 | 1.09 | 12.14 |
| 39 | Claude Sonnet 5 (low) | 1M | 24 | $0.51 | 62 | 1.36 | 9.46 |
| 40 | Claude Opus 4.5 (non-reasoning) | 200k | 24* | n/a | 48 | 0.96 | 11.34 |
| 41 | Claude Sonnet 4.6 (non-reasoning, low) | 1M | 23* | n/a | 46 | 0.94 | 11.82 |
| 42 | Claude Sonnet 5 (non-reasoning) | 1M | 23* | n/a | 64 | 0.82 | 8.58 |
| 43 | Claude 4.1 Opus | 200k | 23* | n/a | n/a | n/a | n/a |
| 44 | Claude 4.5 Sonnet | 1M | 21 | $0.56 | 50 | 17.70 | 27.67 |
| 45 | Claude 4 Opus | 200k | 21* | n/a | n/a | n/a | n/a |
| 46 | Claude 4.5 Sonnet (non-reasoning) | 1M | 19* | n/a | 43 | 1.00 | 12.74 |
| 47 | Claude 4.1 Opus (non-reasoning) | 200k | 19* | n/a | n/a | n/a | n/a |
| 48 | Claude 4.5 Haiku | 200k | 17 | $0.28 | 98 | 21.67 | 26.76 |
| 49 | Claude 4 Opus (non-reasoning) | 200k | 17* | n/a | n/a | n/a | n/a |
| 50 | Claude 4.5 Haiku (non-reasoning) | 200k | 15* | n/a | 89 | 0.50 | 6.11 |
| 51 | Claude 3 Opus | 200k | 9* | n/a | n/a | n/a | n/a |

## Not on the page for any model

Release date (except the detailed models above), API model id, max output tokens, cached-input dollar price, coding index, agentic index, math/reasoning sub-scores, reasoning time. These are n/a.
