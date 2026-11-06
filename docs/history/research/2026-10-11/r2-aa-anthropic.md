# Artificial Analysis: Anthropic current models, every reasoning-effort variant

Date seen: 2026-10-11 (AA shows no separate data-collection date; release dates shown on pages: Opus 5.5 2026-09-22, Sonnet 5.5 2026-09-28, Haiku 5.5 2026-10-07, Fable 5.1 2026-09-01).

## Method

- Providers list: https://artificialanalysis.ai/providers/anthropic (51 rows; Intelligence Index, cost per task, speed, TTFT, end-to-end only).
- Each variant's page under https://artificialanalysis.ai/models/ was loaded in the browser. Its own evaluation record is embedded in the page's server-rendered payload (JSON keys such as `intelligenceIndex`, `terminalBench40`, `hle`, `lcr`, `gpqa`, `omniscienceHallucinationRate`, `intelligenceIndexCostPerTask`, `timescaleData`, `endToEndResponseTime`). Values below were read from those records and cross-checked against the visible page text and the providers list (they match on Intelligence Index, cost per task, tokens, speed, TTFT, end-to-end).
- Score fields are fractions in the payload; shown here as percent.
- Terminal-Bench shown is **Terminal-Bench 4.0** (`terminalBench40`). The `terminalbenchHard` field is null for every Claude row here, so Terminal-Bench Hard is n/a.
- Cost to run the full index is `intelligenceIndexCost.total` as stored (units not labelled on page; assumed USD, consistent with per-task cost x task count). Cost per task is `intelligenceIndexCostPerTask.cost.total` (matches the "Cost per Intelligence Index task" shown on each page).
- Output tokens for the index are `canonicalIntelligenceIndexTokenCount.output`.
- Speed = `timescaleData.medianOutputSpeed` (t/s). TTFT = `timescaleData.medianTimeToFirstChunk`. E2E = `endToEndResponseTime.total` (seconds; page shows the same figure as "Total Response").
- Context window = `contextWindowTokens` (1,000,000 for all rows).

## Page URLs (one per row)

| Row | URL |
|---|---|
| Opus 5.5 max | https://artificialanalysis.ai/models/claude-opus-5-5 |
| Opus 5.5 xhigh | https://artificialanalysis.ai/models/claude-opus-5-5-xhigh |
| Opus 5.5 high | https://artificialanalysis.ai/models/claude-opus-5-5-high |
| Opus 5.5 medium | https://artificialanalysis.ai/models/claude-opus-5-5-medium |
| Opus 5.5 low | https://artificialanalysis.ai/models/claude-opus-5-5-low |
| Sonnet 5.5 max | https://artificialanalysis.ai/models/claude-sonnet-5-5 |
| Sonnet 5.5 xhigh | https://artificialanalysis.ai/models/claude-sonnet-5-5-xhigh |
| Sonnet 5.5 high | https://artificialanalysis.ai/models/claude-sonnet-5-5-high |
| Sonnet 5.5 medium | https://artificialanalysis.ai/models/claude-sonnet-5-5-medium |
| Sonnet 5.5 low | https://artificialanalysis.ai/models/claude-sonnet-5-5-low |
| Haiku 5.5 max | https://artificialanalysis.ai/models/claude-haiku-5-5 |
| Haiku 5.5 xhigh | https://artificialanalysis.ai/models/claude-haiku-5-5-xhigh |
| Haiku 5.5 high | https://artificialanalysis.ai/models/claude-haiku-5-5-high |
| Haiku 5.5 medium | https://artificialanalysis.ai/models/claude-haiku-5-5-medium |
| Haiku 5.5 low | https://artificialanalysis.ai/models/claude-haiku-5-5-low |
| Fable 5.1 max | https://artificialanalysis.ai/models/claude-fable-5-1 |
| Fable 5.1 xhigh | https://artificialanalysis.ai/models/claude-fable-5-1-xhigh |
| Fable 5.1 high | https://artificialanalysis.ai/models/claude-fable-5-1-high |
| Fable 5.1 medium | https://artificialanalysis.ai/models/claude-fable-5-1-medium |
| Fable 5.1 low | https://artificialanalysis.ai/models/claude-fable-5-1-low |

## Table A: quality (percent unless noted)

| Row | Intel (raw) | Intel | Coding idx | Agentic idx | Terminal-Bench 4.0 | Terminal-Bench Hard | SWE-bench | tau2 | tau-Banking | GPQA | HLE | AA-LCR | IFBench | Omniscience acc | Omniscience halluc. |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Opus 5.5 max | 57.622 | 58 | n/a | n/a | 59.6 | n/a | n/a | n/a | n/a | n/a | 61.4 | 84.7 | n/a | 66.2 | 58.6 |
| Opus 5.5 xhigh | 55.987 | 56 | n/a | n/a | 59.6 | n/a | n/a | n/a | n/a | n/a | 57.5 | 84.7 | n/a | 65.4 | 65.7 |
| Opus 5.5 high | 53.583 | 54 | n/a | n/a | 56.6 | n/a | n/a | n/a | n/a | n/a | 55.6 | 82.7 | n/a | 64.6 | 67.6 |
| Opus 5.5 medium | 51.243 | 51 | n/a | n/a | 52.5 | n/a | n/a | n/a | n/a | n/a | 54.7 | 84.3 | n/a | 64.6 | 68.4 |
| Opus 5.5 low | 42.308 | 42 | n/a | n/a | 31.3 | n/a | n/a | n/a | n/a | n/a | 48.3 | 80.7 | n/a | 63.5 | 67.6 |
| Sonnet 5.5 max | 56.000 | 56 | n/a | n/a | 63.6 | n/a | n/a | n/a | n/a | n/a | 55.0 | 82.7 | n/a | 54.0 | 47.0 |
| Sonnet 5.5 xhigh | 51.896 | 52 | n/a | n/a | 57.1 | n/a | n/a | n/a | n/a | n/a | 50.0 | 79.7 | n/a | 53.0 | 62.9 |
| Sonnet 5.5 high | 46.753 | 47 | n/a | n/a | 43.9 | n/a | n/a | n/a | n/a | n/a | 45.8 | 78.0 | n/a | 52.0 | 64.6 |
| Sonnet 5.5 medium | 40.839 | 41 | n/a | n/a | 29.8 | n/a | n/a | n/a | n/a | n/a | 39.8 | 76.3 | n/a | 47.2 | 51.2 |
| Sonnet 5.5 low | 35.868 | 36 | n/a | n/a | 20.7 | n/a | n/a | n/a | n/a | n/a | 36.2 | 76.0 | n/a | 46.3 | 50.2 |
| Haiku 5.5 max | 43.395 | 43 | n/a | n/a | 32.8 | n/a | n/a | n/a | n/a | n/a | 44.4 | 82.7 | n/a | 36.4 | 40.4 |
| Haiku 5.5 xhigh | 41.249 | 41 | n/a | n/a | 29.3 | n/a | n/a | n/a | n/a | n/a | 42.7 | 78.3 | n/a | 35.0 | 44.4 |
| Haiku 5.5 high | 37.824 | 38 | n/a | n/a | 21.7 | n/a | n/a | n/a | n/a | n/a | 37.3 | 77.3 | n/a | 34.8 | 44.5 |
| Haiku 5.5 medium | 34.465 | 34 | n/a | n/a | 15.2 | n/a | n/a | n/a | n/a | n/a | 33.8 | 77.3 | n/a | 34.0 | 44.8 |
| Haiku 5.5 low | 29.452 | 29 | n/a | n/a | 12.6 | n/a | n/a | n/a | n/a | n/a | 27.0 | 72.0 | n/a | 33.3 | 44.9 |
| Fable 5.1 max | 53.355 | 53 | n/a | n/a | 52.0 | n/a | n/a | n/a | 47.2 | 93.7 | 59.1 | 85.3 | n/a | 67.2 | 72.6 |
| Fable 5.1 xhigh | 53.203 | 53 | n/a | n/a | 55.1 | n/a | n/a | n/a | 45.8 | 93.4 | 58.7 | 83.0 | n/a | 66.2 | 70.5 |
| Fable 5.1 high | 51.152 | 51 | n/a | n/a | 52.0 | n/a | n/a | n/a | 43.1 | 90.6 | 55.9 | 83.7 | n/a | 64.9 | 68.8 |
| Fable 5.1 medium | 48.921 | 49 | n/a | n/a | 44.9 | n/a | n/a | n/a | 41.0 | 88.6 | 53.8 | 84.7 | n/a | 63.1 | 69.1 |
| Fable 5.1 low | 46.816 | 47 | n/a | n/a | 40.4 | n/a | n/a | n/a | 39.0 | 88.1 | 48.9 | 82.3 | n/a | 60.2 | 65.6 |

Per-index scores by variant, from the same records, for reference (fraction shown as percent): SciCode, CritPt, GDPval-AA Elo, AA-Briefcase Elo, AutomationBench-AA, GDP.pdf, Omniscience index (0-100). Stored per variant in the page payload; not repeated here to keep the table narrow.

## Table B: economics and speed

| Row | Output tokens for index (M) | Cost to run index (total, as stored) | Cost per index task (USD) | $/1M in | $/1M out | $/1M cache hit | Blended 7:2:1 $/1M | Output speed (t/s) | TTFT (s) | E2E (s) | Context | Max output |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Opus 5.5 max | 259.9 | 8708.2 | 5.982 | 4.00 | 20.00 | 0.20 | 2.94 | 97.0 | 748.61 | 753.76 | 1M | n/a |
| Opus 5.5 xhigh | 99.7 | 4056.7 | 3.459 | 4.00 | 20.00 | 0.20 | 2.94 | 83.8 | 154.80 | 160.77 | 1M | n/a |
| Opus 5.5 high | 52.5 | 2172.4 | 1.823 | 4.00 | 20.00 | 0.20 | 2.94 | 82.6 | 44.34 | 50.39 | 1M | n/a |
| Opus 5.5 medium | 38.2 | 1626.8 | 1.336 | 4.00 | 20.00 | 0.20 | 2.94 | 83.0 | 21.54 | 27.57 | 1M | n/a |
| Opus 5.5 low | 19.6 | 860.3 | 0.551 | 4.00 | 20.00 | 0.20 | 2.94 | 80.0 | 10.29 | 16.54 | 1M | n/a |
| Sonnet 5.5 max | 415.8 | 7259.5 | 5.461 | 2.00 | 10.00 | 0.10 | 1.47 | 137.4 | 492.96 | 496.60 | 1M | n/a |
| Sonnet 5.5 xhigh | 105.1 | 2176.8 | 2.012 | 2.00 | 10.00 | 0.10 | 1.47 | 107.6 | 37.67 | 42.31 | 1M | n/a |
| Sonnet 5.5 high | 52.1 | 1027.4 | 0.884 | 2.00 | 10.00 | 0.10 | 1.47 | 108.7 | 12.38 | 16.98 | 1M | n/a |
| Sonnet 5.5 medium | 31.1 | 622.3 | 0.483 | 2.00 | 10.00 | 0.10 | 1.47 | 106.0 | 0.89 | 5.61 | 1M | n/a |
| Sonnet 5.5 low | 22.7 | 482.1 | 0.345 | 2.00 | 10.00 | 0.10 | 1.47 | 105.5 | 0.57 | 5.31 | 1M | n/a |
| Haiku 5.5 max | 435.2 | 330.3 | 0.213 | 0.10 | 0.50 | 0.01 | 0.077 | 235.0 | 300.00 | 302.13 | 1M | n/a |
| Haiku 5.5 xhigh | 180.0 | 157.6 | 0.124 | 0.10 | 0.50 | 0.01 | 0.077 | 192.0 | 66.24 | 68.84 | 1M | n/a |
| Haiku 5.5 high | 97.1 | 94.1 | 0.079 | 0.10 | 0.50 | 0.01 | 0.077 | 178.9 | 24.32 | 27.12 | 1M | n/a |
| Haiku 5.5 medium | 53.8 | 56.4 | 0.047 | 0.10 | 0.50 | 0.01 | 0.077 | 175.5 | 13.67 | 16.51 | 1M | n/a |
| Haiku 5.5 low | 32.3 | 34.3 | 0.024 | 0.10 | 0.50 | 0.01 | 0.077 | 175.6 | 9.40 | 12.25 | 1M | n/a |
| Fable 5.1 max | 188.5 | 13129.0 | 7.630 | 10.00 | 50.00 | 0.25 | 7.175 | 65.3 | 259.93 | 267.59 | 1M | n/a |
| Fable 5.1 xhigh | 120.5 | 9063.1 | 5.978 | 10.00 | 50.00 | 0.25 | 7.175 | 60.7 | 106.04 | 114.27 | 1M | n/a |
| Fable 5.1 high | 62.0 | 5241.6 | 3.913 | 10.00 | 50.00 | 0.25 | 7.175 | 56.4 | 19.94 | 28.81 | 1M | n/a |
| Fable 5.1 medium | 44.2 | 3983.3 | 2.983 | 10.00 | 50.00 | 0.25 | 7.175 | 56.3 | 6.69 | 15.57 | 1M | n/a |
| Fable 5.1 low | 32.5 | 3157.7 | 2.371 | 10.00 | 50.00 | 0.25 | 7.175 | 57.1 | 2.37 | 11.12 | 1M | n/a |

Cache discount shown on pages: 95% (Opus, Sonnet), 90% (Haiku), 97.5% (Fable; page shows 98%). Cache hit prices are derived from the stored `cacheHitPrice` field.

## Anomalies and caveats

- TTFT and E2E for the max variants (Opus 748.6 s, Sonnet 493 s, Haiku 300 s, Fable 260 s) are AA's measured values as shown on the pages; the Haiku max TTFT is labelled "based on Anthropic's API" on the page. Treat as reported, not verified.
- Opus 5.5 max, Sonnet 5.5 max and Haiku 5.5 max carry different name suffixes ("Max, Default Fallback" vs "Max"); the page slugs are the ones listed above.
- Terminal-Bench Hard leaderboard (https://artificialanalysis.ai/evaluations/terminalbench-hard): top 7 shown on the page are GPT-5.6 Sol (Max) 65.9%, Claude Fable 5 (Max, Opus 4.8 Fallback) 62.9%, GPT-5.6 Sol (Medium) 62.9%, and others. None of the Opus 5.5, Sonnet 5.5, Haiku 5.5 or Fable 5.1 rows appears in that top 7, and their Terminal-Bench Hard field is null. Claude Fable 5 (not 5.1) is the only Claude row shown there; not included in the table because it is not one of the requested models.
- SWE-bench, Coding index and Agentic index: no such field in any Claude record and no matching evaluation on the AA evaluations list. n/a.
- max output tokens: no field in the payload; n/a.
- tau2 (tau2-bench telecom): null for all rows. tau-Banking (tau-Knowledge banking) is available only for Fable 5.1 rows.
- IFBench: null for all rows.
- GPQA: available only for Fable 5.1 rows.
- Providers list shows rank/cost values that match these records (for example Opus max 58, $5.98 per task, 753.76 s total response).

## Fields that stayed n/a after digging

Coding index, Agentic index, Terminal-Bench Hard, SWE-bench (any variant), tau2 (Opus/Sonnet/Haiku/Fable), GPQA (Opus/Sonnet/Haiku), IFBench (all), max output tokens (all), cached-price dollar value as a separate field (only the price is stored: see Table B), end-to-end response time for a fixed 500-token response (not stored; the E2E column is the page's "total response" for the index-run prompt mix).
