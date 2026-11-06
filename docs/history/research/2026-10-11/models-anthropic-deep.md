# Anthropic models on Artificial Analysis: deep dive

Date seen: 2026-10-11. None of the model pages shows a data-as-of date (footer reads "© 2026").
Intelligence Index version: v4.3.2 (10 evaluations). Class size 227 (reasoning and non-reasoning mix).

Method: provider page https://artificialanalysis.ai/providers/anthropic (51 rows listed; header says 46). Model pages fetched with WebFetch; Opus 5.5 (max) and the per-benchmark charts re-checked with the browser (get_page_text). Browser render confirms the per-benchmark charts read "Not publicly available" on the page.

## Key finding: per-benchmark scores are not published

Every model page lists these evaluations by name only, with no scores:
AA-Briefcase v1.1, GDPval-AA v2.1, AutomationBench-AA, Terminal-Bench 4.0, SciCode, Humanity's Last Exam, GDP.pdf, CritPt, AA-Omniscience (accuracy and non-hallucination rate), AA-LCR v1.1 (long context), Harvey LAB-AA v1.1, EnterpriseOps-Gym-AA, Terminal-Bench-Science 0.1, AA-AnalystAgent, ITBench-AA, MMMU-Pro, MLCR-AA.
Not listed at all: SWE-bench, GPQA Diamond, AIME/math, tau-bench, IFBench.
Capability sub-indexes (Finance & Accounting, Strategy & Ops, Legal, Engineering, Economics, Healthcare): n/a on every page.
Only headline numbers exist: Intelligence Index, output speed, time to first token, price, cost per index task, output tokens used for the index, context window.

## Per-model data (top 8 by Intelligence Index, plus Haiku 5.5 for cheap tasks)

Columns: Index = Artificial Analysis Intelligence Index (rank of 227); Speed = output tokens/s (rank); TTFT = time to first token, seconds (median 3.79 s); Price = USD per 1M input / output (cache discount); Cost/run = USD per Intelligence Index task (rank); Index tokens = output tokens used to run the index (class median 82M); Blended = 7:2:1 cache-hit/input/output blend.

| Model (variant) | URL slug | Index (rank) | Speed t/s (rank) | TTFT s | Price in/out, cache disc. | Blended $/1M | Cost/run (rank) | Index tokens | Context | Released |
|---|---|---|---|---|---|---|---|---|---|---|
| Claude Opus 5.5 (max) | claude-opus-5-5 | 58 (#1) | 97.0 (#74) | 748.61 | $4 / $20, 95% | $2.94 | $5.98 (#109) | 260M (#111, very verbose) | 1M | Sep 22 2026 |
| Claude Sonnet 5.5 (max) | claude-sonnet-5-5 | 56 (#2) | 137.4 (#31) | 492.96 | $2 / $10, 95% | $1.47 | $5.46 (#106) | 420M (#113, very verbose) | 1M | Sep 28 2026 |
| Claude Opus 5.5 (xhigh) | claude-opus-5-5-xhigh | 56 (#3) | 83.8 (#95) | 154.80 | $4 / $20, 95% | $2.94 | $3.46 (#97) | 100M (#68, somewhat verbose) | 1M | Sep 22 2026 |
| Claude Opus 5.5 (high) | claude-opus-5-5-high | 54 (#4) | 82.6 (#100) | 44.34 | $4 / $20, 95% | $2.94 | $1.82 (#79) | 53M (#42, fairly concise) | 1M | Sep 22 2026 |
| Claude Fable 5.1 (max) | claude-fable-5-1 | 53 (#5) | 65.3 (#119) | 259.93 | $10 / $50, 98% | $7.17 | $7.63 (#110) | 190M (#102, very verbose) | 1M | Sep 1 2026 |
| Claude Fable 5.1 (xhigh) | claude-fable-5-1-xhigh | 53 (#6) | 60.7 (#135) | 106.04 | $10 / $50, 98% | $7.17 | $5.98 (#108) | 120M (#78, somewhat verbose) | 1M | Sep 1 2026 |
| Claude Sonnet 5.5 (xhigh) | claude-sonnet-5-5-xhigh | 52 (#10) | 107.6 (#58) | 37.67 | $2 / $10, 95% | $1.47 | $2.01 (rank n/a) | 110M (#72, somewhat verbose) | 1M | Sep 28 2026 |
| Claude Opus 5.5 (medium) | claude-opus-5-5-medium | 51 (#12) | 83.0 (#99) | 21.54 | $4 / $20, 95% | $2.94 | $1.34 (#66) | 38M (#31, fairly concise) | 1M | Sep 22 2026 |
| Claude Haiku 5.5 (max) | claude-haiku-5-5 | 43 (#2 of 180 reasoning class) | 235.0 (#14 of 180) | 300.00 | $0.10 / $0.50, 90% | $0.08 | $0.21 (#46 of 180) | 440M (verbosity #62 of 180) | 1M | Oct 7 2026 |

Haiku 5.5 price is per the Haiku page (reasoning-class medians: input $0.24, output $0.88).

Other current rows from the provider page (not deep-fetched, index and cost only, per provider page): Opus 5 (max) 51 / $5.86; Opus 5 (xhigh) 50 / $4.88; Fable 5 (with fallback) 50 / $8.75; Fable 5.1 (medium) 49 / $2.98; Opus 5 (high) 48 / $3.61; Fable 5.1 (low) 47 / $2.37; Sonnet 5.5 (high) 47 / $0.88; Opus 5 (medium) 45 / $2.19; Haiku 5.5 (xhigh) 41 / $0.12; Sonnet 5.5 (medium) 41 / $0.48; Opus 5 (low) 39 / $1.10; Sonnet 5 (max) 38 / $5.09; Haiku 5.5 (high) 38 / $0.08; Sonnet 5.5 (low) 36 / $0.35; Haiku 5.5 (medium) 34 / $0.05; Sonnet 5 (xhigh) 34 / $2.87; Sonnet 5 (high) 32 / $1.79; Sonnet 5 (medium) 28 / $1.00; Sonnet 5 (low) 24 / $0.51; Haiku 5.5 (low) 29 / $0.02; Opus 5.5 (low) 42 / $0.55; Opus 4.8 (max) 42 / $4.08 (older).
Older 4.x and 3 entries are in the provider page and were not deep-fetched.

## Comparison statements on the pages

- Opus 5.5 (max): "amongst the leading models in intelligence, but particularly expensive when comparing to other models of similar price"; "faster than average"; "very verbose".
- Sonnet 5.5 (max): intelligence "well above average" (#2); speed "notably fast" (#31); cost per task #106; input and output prices "moderately priced"; verbosity #113 ("very verbose").
- Opus 5.5 (xhigh): intelligence #3; speed "faster than average"; cost "expensive"; TTFT "higher end"; "somewhat verbose".
- Opus 5.5 (high): intelligence #4; speed "below average" (page FAQ: median 83.3); TTFT "higher end"; "fairly concise" (53M vs median 82M).
- Fable 5.1 (max): intelligence #5; speed "slower than average"; cost #110, "particularly expensive"; input and output "expensive"; "very verbose".
- Fable 5.1 (xhigh): intelligence #6; speed "slower than average" (#135); cost #108, "particularly expensive"; "somewhat verbose".
- Sonnet 5.5 (xhigh): intelligence #10; speed "faster than average" (#58); cost "moderately priced", FAQ says "better than average"; "somewhat verbose".
- Opus 5.5 (medium): intelligence "amongst the leading models"; speed "slower than average"; TTFT "somewhat higher than average"; "fairly concise", "better than average" on verbosity; cost per task #66.
- Haiku 5.5 (max): intelligence "among the leading models" (#2 of 180 reasoning); speed "notably fast"; price "very competitive" (FAQ); "very verbose" (440M vs median 100M).

## Source notes

- Output speed and TTFT come from the Anthropic API. Each model has 1 API provider, except Fable 5.1 (6 providers) and Sonnet 5.5 (9 providers, per its summary). Opus 5.5 has 9 providers listed in its page's availability section.
- The Opus 5.5 (max) TTFT of 748.61 s and Haiku 5.5 (max) TTFT of 300.00 s are shown as stated on the pages; these values look like max-effort thinking time, not a measurement error we can verify.
- Sonnet 5.5 pages have a data-consistency note: several charts say "Not publicly available" while the summary and FAQ give figures. Figures above come from the summary and FAQ.
- Total cost to run the full index and total tokens per task: n/a on every page.
