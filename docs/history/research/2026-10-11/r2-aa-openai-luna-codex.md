# AA raw data: OpenAI GPT-6 Luna, GPT-5.6 Luna/Terra, GPT-5.5, GPT-5.3 Codex

Date seen: 2026-10-11. Sources: artificialanalysis.ai model pages (record JSON embedded in each page), the Coding Agent leaderboard (https://artificialanalysis.ai/agents/coding-agents) and the provider list (https://artificialanalysis.ai/providers/openai).

## Method notes

- Intelligence Index is v4.3.2 (10 evals: AA-Briefcase v1.1, GDPval-AA v2.1, AutomationBench-AA, Terminal-Bench 4.0, SciCode, HLE, GDP.pdf, CritPt, AA-Omniscience, AA-LCR v1.1). The names "Coding Index", "Agentic Index" and "Terminal-Bench Hard" are not published as indexes in this version. No SWE-bench page exists (swe-bench-verified and swe-bench-pro return 404).
- "Coding Agent Index v1.5" (agents/coding-agents) is a composite of DeepSWE v1.1, Terminal-Bench v4 and SWE-Atlas-QnA run through the Codex agent harness. It is published only for listed variants. The only OpenAI Codex variants on that page are GPT-6 Luna (max) and GPT-5.6 Luna (max).
- Model values come from each page record whose shortName matches the page title. Speed and latency are first-party medians when shown. "Cost to run index" = intelligenceIndexCost.total (USD) over the full index. Output tokens = canonicalIntelligenceIndexTokenCount.output (whole run) and intelligenceIndexOutputTokensPerTask.output (per task).
- GPT-5.5 (low), GPT-5.5 (non-reasoning) and GPT-5.3 Codex have a shorter record in the payload, with no token counts and no index-run cost, so those cells are n/a. GPT-5.5 (low) and the 5.3 Codex page carry intelligenceIndexIsEstimated = true for the index score.
- Max output tokens are not present on any page: n/a on every row.
- GPT-6 Luna pages show gpqa, tau2 and ifbench as null (not published for these records). GPT-6 Luna (medium) has no speed/TTFT/E2E values in its record.

## Main table

| Model (effort) | Page URL | Intel idx v4.3.2 | Coding Agent Idx v1.5 (Codex harness) | AA-Briefcase Elo | GDPval-AA Elo | AutomationBench-AA | Terminal-Bench 4.0 | Terminal-Bench Hard | SWE-bench | DeepSWE v1.1 (Codex) | SWE-Atlas-QnA (Codex) | tau2 / tauBanking | GPQA | HLE | AA-LCR | IFBench | CritPt | AA-Omniscience index | Omniscience accuracy | Omniscience hallucination rate | Enterprise Ops Gym | Harvey LAB | Output tokens / index task | Output tokens total (index run) | Cost to run index | Cost / index task | Input $/1M | Output $/1M | Cache hit $/1M | Cache write $/1M | Output tok/s (median) | Tok/s p05-p95 | TTFT s (median) | TTFT p05-p95 s | E2E s, 500 tokens | Index run time s/task | Context window | Modality in | Release | Date seen |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| GPT-6 Luna (max) | https://artificialanalysis.ai/models/gpt-6-luna | 38.1 | 41.1 | 1336 | 1432 | 53.2 | 12.6 | n/a | n/a (no SWE-bench page) | 63.7 | 44.4 | n/a | n/a | 38.5 | 83.3 | n/a | 19.4 | 0.7 | 43.8 | 76.7 | n/a | 3.3 | 50,012 | 144,380,994 | $122.05 | $0.0678 | $0.1 | $0.5 | $0.01 | $0.125 | 140.3 | 112-175 | 102.41 | 54.28-226.18 | 106.0 | 352 | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-6 Luna (xhigh) | https://artificialanalysis.ai/models/gpt-6-luna-xhigh | 34.6 | n/a | 1237 | 1364 | 47.8 | 8.1 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a | n/a | 34.3 | 80.0 | n/a | 17.4 | -1.8 | 44.2 | 82.4 | n/a | n/a | 27,487 | 68,515,556 | $66.81 | $0.0422 | $0.1 | $0.5 | $0.01 | $0.125 | 130.1 | 97-160 | 34.27 | 11.69-82.00 | 38.1 | 209 | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-6 Luna (high) | https://artificialanalysis.ai/models/gpt-6-luna-high | 32.9 | n/a | 1186 | 1340 | 47.8 | 4.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a | n/a | 32.9 | 79.3 | n/a | 15.4 | -5.5 | 42.8 | 84.4 | n/a | n/a | 19,703 | 46,829,428 | $47.85 | $0.0290 | $0.1 | $0.5 | $0.01 | $0.125 | 134.3 | 94-162 | 20.71 | 3.38-47.50 | 24.4 | 147 | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-6 Luna (medium) | https://artificialanalysis.ai/models/gpt-6-luna-medium | 29.9 | n/a | 1099 | 1262 | 40.5 | 2.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a | n/a | 28.3 | 78.3 | n/a | 10.6 | -5.0 | 43.1 | 84.7 | n/a | n/a | 11,456 | 28,532,183 | $31.17 | $0.0175 | $0.1 | $0.5 | $0.01 | $0.125 | n/a | n/a | n/a | n/a | n/a | n/a | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-6 Luna (low) | https://artificialanalysis.ai/models/gpt-6-luna-low | 21.5 | n/a | 741 | 1033 | 12.0 | 0.0 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a | n/a | 20.3 | 74.0 | n/a | 2.6 | -9.2 | 40.8 | 84.3 | n/a | n/a | 2,087 | 8,160,682 | $10.63 | $0.0045 | $0.1 | $0.5 | $0.01 | $0.125 | 139.7 | 114-169 | 2.55 | 1.42-5.92 | 6.1 | 15 | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-6 Luna (non-reasoning) | https://artificialanalysis.ai/models/gpt-6-luna-non-reasoning | 18.5 | n/a | 903 | 1045 | 9.1 | 1.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a | n/a | 8.6 | 39.7 | n/a | 1.1 | -21.9 | 31.9 | 78.9 | n/a | n/a | 3,761 | 6,926,643 | $19.16 | $0.0113 | $0.1 | $0.5 | $0.01 | $0.125 | 118.4 | 98-179 | 0.79 | 0.60-3.95 | 5.0 | 32 | 1,000,000 | inputModalityText, inputModalityImage | 2026-09-22 | 2026-10-11 |
| GPT-5.6 Luna (max) | https://artificialanalysis.ai/models/gpt-5-6-luna | 37.3 | 43.2 | 1342 | 1459 | 50.2 | 11.6 | n/a | n/a (no SWE-bench page) | 66.4 | 48.7 | n/a / 31.1 | 91.1 | 39.5 | 83.7 | n/a | 20.6 | -10.3 | 42.7 | 92.6 | 40.8 | n/a | 41,235 | 153,656,724 | $319.93 | $0.1783 | $0.2 | $1.2 | $0.02 | $0.25 | 120.8 | 94-159 | 112.63 | 54.30-252.20 | 116.8 | 337 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Luna (xhigh) | https://artificialanalysis.ai/models/gpt-5-6-luna-xhigh | 34.6 | n/a | 1268 | 1399 | 42.6 | 3.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 28.7 | 89.5 | 37.0 | 81.7 | n/a | 20.6 | -10.8 | 42.4 | 92.5 | n/a | n/a | 23,625 | 85,174,201 | $179.33 | $0.0853 | $0.2 | $1.2 | $0.02 | $0.25 | 111.5 | 93-144 | 48.98 | 21.70-106.91 | 53.5 | 211 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Luna (high) | https://artificialanalysis.ai/models/gpt-5-6-luna-high | 32.1 | n/a | 1170 | 1330 | 35.6 | 2.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 25.2 | 89.2 | 33.4 | 80.3 | n/a | 16.6 | -12.0 | 41.8 | 92.4 | n/a | n/a | 13,862 | 49,812,339 | $108.15 | $0.0440 | $0.2 | $1.2 | $0.02 | $0.25 | 111.9 | 96-137 | 17.81 | 7.21-57.46 | 22.3 | 123 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Luna (medium) | https://artificialanalysis.ai/models/gpt-5-6-luna-medium | 25.0 | n/a | 935 | 1129 | 23.1 | 0.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 17.7 | 85.9 | 25.8 | 75.0 | n/a | 4.9 | -13.2 | 40.7 | 90.9 | n/a | n/a | 4,486 | 17,919,951 | $44.32 | $0.0156 | $0.2 | $1.2 | $0.02 | $0.25 | 105.5 | 86-130 | 2.93 | 1.71-7.29 | 7.7 | 42 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Luna (low) | https://artificialanalysis.ai/models/gpt-5-6-luna-low | 21.0 | n/a | 741 | 994 | 11.7 | 0.0 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 12.8 | 83.5 | 19.8 | 70.0 | n/a | 2.6 | -14.7 | 39.6 | 89.9 | n/a | n/a | 2,514 | 10,118,765 | $27.02 | $0.0098 | $0.2 | $1.2 | $0.02 | $0.25 | 105.0 | 87-139 | 1.89 | 1.14-2.86 | 6.6 | 24 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Luna (non-reasoning) | https://artificialanalysis.ai/models/gpt-5-6-luna-non-reasoning | 15.5 | n/a | 706 | 919 | 5.0 | 1.0 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 9.7 | 64.5 | 7.2 | 42.7 | n/a | 0.3 | -24.9 | 28.6 | 75.0 | n/a | n/a | 2,027 | 4,847,490 | $22.97 | $0.0101 | $0.2 | $1.2 | $0.02 | $0.25 | 109.7 | 88-146 | 0.69 | 0.56-2.04 | 5.2 | 18 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (max) | https://artificialanalysis.ai/models/gpt-5-6-terra | 42.1 | n/a | 1334 | 1451 | 59.6 | 35.4 | 57.6 | n/a (no SWE-bench page) | n/a | n/a | 86.3 / 40.2 | 92.5 | 42.9 | 83.0 | 71.2 | 30.0 | 0.1 | 46.8 | 87.9 | 38.5 | n/a | 38,897 | 121,832,989 | $2,500.72 | $1.3987 | $2 | $12 | $0.2 | $2.5 | 126.3 | 94-179 | 149.29 | 79.09-254.81 | 153.2 | 306 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (xhigh) | https://artificialanalysis.ai/models/gpt-5-6-terra-xhigh | 38.0 | n/a | 1333 | 1447 | 47.1 | 10.1 | 62.9 | n/a (no SWE-bench page) | n/a | n/a | 80.4 / 29.7 | 90.8 | 41.9 | 79.0 | 66.3 | 27.1 | -3.0 | 45.5 | 89.0 | n/a | n/a | 20,367 | 51,995,246 | $1,186.77 | $0.6318 | $2 | $12 | $0.2 | $2.5 | 118.0 | 90-162 | 43.03 | 4.95-85.29 | 47.3 | 170 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (high) | https://artificialanalysis.ai/models/gpt-5-6-terra-high | 34.2 | n/a | 1202 | 1376 | 42.0 | 1.5 | 57.6 | n/a (no SWE-bench page) | n/a | n/a | 78.4 / 28.7 | 89.6 | 38.5 | 77.7 | 64.4 | 22.9 | -3.5 | 45.5 | 89.8 | n/a | n/a | 11,294 | 33,205,685 | $767.78 | $0.3379 | $2 | $12 | $0.2 | $2.5 | 106.9 | 80-162 | 7.05 | 1.52-22.78 | 11.7 | 104 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (medium) | https://artificialanalysis.ai/models/gpt-5-6-terra-medium | 30.1 | n/a | 1039 | 1264 | 33.5 | 1.0 | n/a | n/a (no SWE-bench page) | n/a | n/a | 72.8 / 25.6 | 87.2 | 33.3 | 74.0 | 62.2 | 17.4 | -5.1 | 44.6 | 89.8 | n/a | n/a | 5,478 | 16,684,399 | $446.53 | $0.1834 | $2 | $12 | $0.2 | $2.5 | 102.5 | 82-157 | 1.72 | 1.00-7.91 | 6.6 | 54 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (low) | https://artificialanalysis.ai/models/gpt-5-6-terra-low | 27.5 | n/a | 1014 | 1108 | 29.1 | 1.5 | 43.9 | n/a (no SWE-bench page) | n/a | n/a | 60.5 / 18.8 | 84.3 | 29.2 | 71.3 | 59.7 | 9.4 | -6.8 | 43.7 | 89.9 | n/a | n/a | 3,871 | 11,297,248 | $350.36 | $0.1445 | $2 | $12 | $0.2 | $2.5 | 103.3 | 85-165 | 1.45 | 1.01-3.98 | 6.3 | 37 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.6 Terra (non-reasoning) | https://artificialanalysis.ai/models/gpt-5-6-terra-non-reasoning | 20.8 | n/a | 947 | 1097 | 12.5 | 0.5 | n/a | n/a (no SWE-bench page) | n/a | n/a | n/a / 15.7 | 74.6 | 11.4 | 58.7 | n/a | 2.0 | -23.2 | 36.8 | 95.0 | n/a | n/a | 3,129 | 5,818,048 | $255.77 | $0.1397 | $2 | $12 | $0.2 | $2.5 | 105.2 | 87-164 | 0.72 | 0.58-0.87 | 5.5 | 30 | 1,000,000 | inputModalityText, inputModalityImage | 2026-07-09 | 2026-10-11 |
| GPT-5.5 (xhigh) | https://artificialanalysis.ai/models/gpt-5-5 | 38.4 | n/a | 1137 | 1350 | 47.3 | 14.6 | 60.6 | n/a (no SWE-bench page) | n/a | n/a | 93.9 / 39.0 | 93.5 | 45.8 | 84.3 | 75.9 | 27.1 | 20.5 | 58.0 | 89.0 | 46.6 | n/a | 23,658 | 87,605,592 | $5,294.36 | $2.6340 | $5 | $30 | $0.5 | n/a | 98.6 | 70-152 | 71.20 | 10.41-143.89 | 76.3 | 243 | 922,000 | inputModalityText, inputModalityImage | 2026-04-23 | 2026-10-11 |
| GPT-5.5 (high) | https://artificialanalysis.ai/models/gpt-5-5-high | 37.0 | n/a | 1089 | 1326 | 44.3 | 9.1 | 59.8 | n/a (no SWE-bench page) | n/a | n/a | 93.0 / 36.7 | 93.2 | 45.0 | 84.3 | 71.6 | 25.4 | 18.8 | 57.0 | 89.1 | n/a | n/a | 15,611 | 54,656,794 | $3,410.76 | $1.5413 | $5 | $30 | $0.5 | n/a | 91.5 | 69-137 | 30.20 | 5.93-85.23 | 35.7 | 170 | 922,000 | inputModalityText, inputModalityImage | 2026-04-23 | 2026-10-11 |
| GPT-5.5 (medium) | https://artificialanalysis.ai/models/gpt-5-5-medium | 33.8 | n/a | 1000 | 1231 | 40.1 | 5.1 | 57.6 | n/a (no SWE-bench page) | n/a | n/a | 91.8 / 29.9 | 92.6 | 42.4 | 83.0 | 71.0 | 18.6 | 18.1 | 56.8 | 89.6 | 44.1 | n/a | 8,554 | 29,788,332 | $2,095.12 | $0.9017 | $5 | $30 | $0.5 | n/a | 87.4 | 69-134 | 9.00 | 1.42-31.72 | 14.7 | 98 | 922,000 | inputModalityText, inputModalityImage | 2026-04-23 | 2026-10-11 |
| GPT-5.5 (low) | https://artificialanalysis.ai/models/gpt-5-5-low | 30.7 | n/a | n/a | 1044 | n/a | n/a | 52.3 | n/a (no SWE-bench page) | n/a | n/a | 83.9 / 24.9 | 91.0 | 32.7 | 81.0 | 64.4 | 8.0 | 15.1 | 54.9 | 88.0 | n/a | n/a | n/a | n/a | n/a | n/a | $5 | $30 | $0.5 | n/a | 84.6 | 70-153 | 1.40 | 1.07-1.65 | 7.3 | n/a | 922,000 | inputModalityText, inputModalityImage | 2026-04-23 | 2026-10-11 |
| GPT-5.5 (non-reasoning) | https://artificialanalysis.ai/models/gpt-5-5-non-reasoning | 23.2 | n/a | n/a | 969 | n/a | n/a | 49.2 | n/a (no SWE-bench page) | n/a | n/a | 69.3 / 14.8 | 76.8 | 13.7 | 64.0 | 46.1 | 1.4 | -4.8 | 45.6 | 92.7 | n/a | n/a | n/a | n/a | n/a | n/a | $5 | $30 | $0.5 | n/a | 88.1 | 68-132 | 0.83 | 0.69-1.42 | 6.5 | n/a | 922,000 | inputModalityText, inputModalityImage | 2026-04-23 | 2026-10-11 |
| GPT-5.3 Codex (xhigh) | https://artificialanalysis.ai/models/gpt-5-3-codex | 32.5 | n/a | 868 | n/a | n/a | n/a | 53.0 | n/a (no SWE-bench page) | n/a | n/a | 86.0 / n/a | 91.5 | 42.5 | 83.3 | 75.4 | 16.9 | 10.9 | 52.9 | 89.2 | n/a | n/a | n/a | n/a | n/a | n/a | $1.75 | $14 | $0.175 | n/a | 114.8 | 83-152 | 72.79 | 39.79-119.48 | 77.1 | n/a | 400,000 | inputModalityText, inputModalityImage | 2026-02-05 | 2026-10-11 |

## Coding Agent Index v1.5 detail (Codex harness)

Source: https://artificialanalysis.ai/agents/coding-agents (per-variant records).

| Variant | Index score | DeepSWE v1.1 | SWE-Atlas-QnA | Terminal-Bench v4 | Cost/task USD (mean) | Agent wall s/task | Total tokens/task |
|---|---|---|---|---|---|---|---|
| Codex - GPT-6 Luna (max) | 41.1 | 63.7 | 44.4 | 15.2 | $0.176 | 1286 | 10,218,069 |
| Codex - GPT-5.6 Luna (max) | 43.2 | 66.4 | 48.7 | 14.6 | $0.438 | 1412 | 14,854,525 |

No Codex-harness variant was found for the other rows. Their Coding idx is n/a.

## Per-row page URLs

- GPT-6 Luna (max): https://artificialanalysis.ai/models/gpt-6-luna
- GPT-6 Luna (xhigh): https://artificialanalysis.ai/models/gpt-6-luna-xhigh
- GPT-6 Luna (high): https://artificialanalysis.ai/models/gpt-6-luna-high
- GPT-6 Luna (medium): https://artificialanalysis.ai/models/gpt-6-luna-medium
- GPT-6 Luna (low): https://artificialanalysis.ai/models/gpt-6-luna-low
- GPT-6 Luna (non-reasoning): https://artificialanalysis.ai/models/gpt-6-luna-non-reasoning
- GPT-5.6 Luna (max): https://artificialanalysis.ai/models/gpt-5-6-luna
- GPT-5.6 Luna (xhigh): https://artificialanalysis.ai/models/gpt-5-6-luna-xhigh
- GPT-5.6 Luna (high): https://artificialanalysis.ai/models/gpt-5-6-luna-high
- GPT-5.6 Luna (medium): https://artificialanalysis.ai/models/gpt-5-6-luna-medium
- GPT-5.6 Luna (low): https://artificialanalysis.ai/models/gpt-5-6-luna-low
- GPT-5.6 Luna (non-reasoning): https://artificialanalysis.ai/models/gpt-5-6-luna-non-reasoning
- GPT-5.6 Terra (max): https://artificialanalysis.ai/models/gpt-5-6-terra
- GPT-5.6 Terra (xhigh): https://artificialanalysis.ai/models/gpt-5-6-terra-xhigh
- GPT-5.6 Terra (high): https://artificialanalysis.ai/models/gpt-5-6-terra-high
- GPT-5.6 Terra (medium): https://artificialanalysis.ai/models/gpt-5-6-terra-medium
- GPT-5.6 Terra (low): https://artificialanalysis.ai/models/gpt-5-6-terra-low
- GPT-5.6 Terra (non-reasoning): https://artificialanalysis.ai/models/gpt-5-6-terra-non-reasoning
- GPT-5.5 (xhigh): https://artificialanalysis.ai/models/gpt-5-5
- GPT-5.5 (high): https://artificialanalysis.ai/models/gpt-5-5-high
- GPT-5.5 (medium): https://artificialanalysis.ai/models/gpt-5-5-medium
- GPT-5.5 (low): https://artificialanalysis.ai/models/gpt-5-5-low
- GPT-5.5 (non-reasoning): https://artificialanalysis.ai/models/gpt-5-5-non-reasoning
- GPT-5.3 Codex (xhigh): https://artificialanalysis.ai/models/gpt-5-3-codex

## Evaluation pages checked

- https://artificialanalysis.ai/evaluations/artificial-analysis-intelligence-index (v4.3.2)
- https://artificialanalysis.ai/evaluations/terminalbench-4-0
- https://artificialanalysis.ai/evaluations/gdpval-aa
- https://artificialanalysis.ai/evaluations/aa-briefcase
- https://artificialanalysis.ai/evaluations/automationbench-aa
- https://artificialanalysis.ai/evaluations/omniscience
- https://artificialanalysis.ai/evaluations/artificial-analysis-long-context-reasoning
- https://artificialanalysis.ai/evaluations/humanitys-last-exam
- https://artificialanalysis.ai/evaluations/gpqa-diamond
- https://artificialanalysis.ai/evaluations/ifbench
- https://artificialanalysis.ai/evaluations/tau2-bench
- https://artificialanalysis.ai/evaluations/terminal-bench-science
- https://artificialanalysis.ai/agents/coding-agents
