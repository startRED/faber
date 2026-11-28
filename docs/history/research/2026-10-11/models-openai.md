# OpenAI models on Artificial Analysis (raw data)

- Source: https://artificialanalysis.ai/providers/openai (81 models listed), plus model pages under https://artificialanalysis.ai/models/<slug>
- Date seen: 2026-10-11 (AA cache-hit note says data updated Sep 26, 2026)
- Method: provider table read via browser (full table, all 81 rows). Model pages opened and read: gpt-6-1-sol-xhigh, gpt-6-astra, gpt-6-sol, gpt-6-luna, gpt-5-6-sol, gpt-5-6-terra, gpt-5-3-codex. The page for gpt-6-1-sol (max) did not load (navigation returned other pages), so its price and release date are n/a. Slugs for GPT-5.4 and GPT-5.5 variants are guesses from the naming pattern and were not opened; their data comes from the provider table only.
- Intelligence Index = Artificial Analysis Intelligence Index v4.3.2. "*" = estimated by AA (as shown on page).
- "n/a" = not shown on the page or not checked. Nothing estimated.

## Fields not shown anywhere on these pages

- API ids: the pages show only AA URL slugs, not API model ids. Slugs are given below.
- Max output tokens: n/a.
- Coding index, agentic index, math/reasoning sub-scores: AA shows them only as charts (Terminal-Bench 4.0 = agentic coding, SciCode = coding, AA-Briefcase, GDPval-AA, etc.). No numeric values in the text. n/a.
- Cached input price: shown as "Cache Discount" %, not as a $ figure. Discounts seen: 90% (Astra, Luna, Terra, Sol 6.0), 95% (6.1 Sol xhigh).
- Per-model "best at" notes: n/a (AA's "Comparison Summary" text is generic, e.g. "amongst the leading models in intelligence").
- Reasoning effort levels: taken from the name suffix on AA (max / xhigh / high / medium / low / non-reasoning). AA does not publish a formal effort list.

## Skipped

Deprecated (AA banner on the page says "This model is deprecated" and points to GPT-6.1 Sol):
- GPT-6 Sol (all effort variants; banner checked on the max page)
- GPT-5.6 Sol (all effort variants; banner checked on the max page)

Old, not current generation (listed on the provider page, not analysed):
- GPT-5.2, GPT-5.1, GPT-5 (minimal/low/medium/high), GPT-5 mini, GPT-5 nano, GPT-5 private variants
- o3, o3-mini, o3-pro, o4-mini, o1, o1-pro
- GPT-4.1, GPT-4.1 mini, GPT-4.1 nano, GPT-4o (several dates), GPT-4o mini, GPT-4 Turbo, GPT-4, GPT-3.5 Turbo

AA does not mark these as deprecated itself. The skip is my judgement from generation/naming.

Duplicate rows on the provider page (same model, no metrics): GPT-6.1 Sol (max) appears twice; the second row has no values. GPT-5.4 Pro (xhigh) and GPT-6 Sol (Daybreak Blue, max) have no metrics. Both are noted and not counted.

## Current models (all values as shown; "page" = verified on the model page, "list" = from provider table only)

Columns: AA Index | Cost per AA task (USD) | Output t/s | First chunk / TTFAT (s) | Total response for 500 tok (s) | Context | Notes

### GPT-6 Astra (family). Released Sep 2026 (page). Price $10.00 in / $50.00 out per 1M, cache discount 90% (page, max effort). Knowledge cutoff Apr 30, 2026. Slug: gpt-6-astra (max), gpt-6-astra-xhigh, gpt-6-astra-high, gpt-6-astra-medium, gpt-6-astra-low.

| Effort | Slug | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|---|
| max | gpt-6-astra | 53 | $3.26 | 63 | 391.21 | 399.11 | 1M |
| xhigh | gpt-6-astra-xhigh | 52 | $2.31 | 62 | 181.28 | 189.41 | 1M |
| high | gpt-6-astra-high | 51 | $1.73 | 57 | 101.55 | 110.33 | 1M |
| medium | gpt-6-astra-medium | 50 | $1.54 | 55 | 8.01 | 17.15 | 1M |
| low | gpt-6-astra-low | 46 | $0.82 | 50 | 2.35 | 12.26 | 1M |

Page (max): Intelligence #7/227, speed #124/227, cost #96/227, verbosity 60M output tokens, text+image in, text out, reasoning yes.

### GPT-6.1 Sol (family). Released Sep 2026 (page). Slug: gpt-6-1-sol (max), gpt-6-1-sol-xhigh, gpt-6-1-sol-high, gpt-6-1-sol-medium, gpt-6-1-sol-low.
Price $2.00 in / $10.00 out per 1M, cache discount 95% (page, xhigh effort). Verified on xhigh page only; other effort levels were not checked, so their price is n/a. The gpt-6-1-sol (max) page did not load. Context 1M. Verbosity 36M output tokens (xhigh). Knowledge cutoff not shown.

| Effort | Slug | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|---|
| max | gpt-6-1-sol | 52 | $0.72 | 65 | 347.78 | 355.42 | 1M |
| xhigh | gpt-6-1-sol-xhigh | 51 | $0.39 | 60.4 (page) | 173.40 | 181.68 | 1M |
| high | gpt-6-1-sol-high | 50 | $0.32 | 59 | 64.60 | 73.08 | 1M |
| medium | gpt-6-1-sol-medium | 48 | $0.21 | 55 | 7.53 | 16.56 | 1M |
| low | gpt-6-1-sol-low | 42 | $0.13 | 54 | 2.50 | 11.74 | 1M |

Page (xhigh): Intelligence #14/227, speed #136/227, cost #23/227, "amongst the leading models in intelligence and reasonably priced"; "highly concise".

### GPT-6 Luna (family). Released Sep 2026 (page, max). Price $0.10 in / $0.50 out per 1M, cache discount 90% (page, max). Context 1M. Slug: gpt-6-luna (max), gpt-6-luna-xhigh, gpt-6-luna-high, gpt-6-luna-medium, gpt-6-luna-low, gpt-6-luna-non-reasoning.

| Effort | Slug | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|---|
| max | gpt-6-luna | 38 | $0.07 | 140.3 (page) | 102.41 | 105.97 | 1M |
| xhigh | gpt-6-luna-xhigh | 35 | $0.04 | 130 | 34.27 | 38.11 | 1M |
| high | gpt-6-luna-high | 33 | $0.03 | 134 | 20.71 | 24.43 | 1M |
| medium | gpt-6-luna-medium | 30 | $0.02 | n/a | n/a | n/a | 1M |
| low | gpt-6-luna-low | 22 | $0.0045 | 140 | 2.55 | 6.13 | 1M |
| non-reasoning | gpt-6-luna-non-reasoning | 18 | $0.01 | 118 | 0.79 | 5.01 | 1M |

Page (max): Intelligence #8/180 (in its class), speed #52/180, cost #24/180, verbosity 140M output tokens, "well priced".

### GPT-5.6 Terra (family). Released Jul 2026 (page, max). Price $2.00 in / $12.00 out per 1M, cache discount 90% (page, max). Context 1M. Verbosity 120M output tokens (max). No deprecation banner seen. Slug: gpt-5-6-terra (max), gpt-5-6-terra-xhigh, -high, -medium, -low, -non-reasoning.

| Effort | Slug | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|---|
| max | gpt-5-6-terra | 42 | $1.40 | 126.3 (page) | 149.29 | 153.24 | 1M |
| xhigh | gpt-5-6-terra-xhigh | 38 | $0.63 | 118 | 43.03 | 47.27 | 1M |
| high | gpt-5-6-terra-high | 34 | $0.34 | 107 | 7.05 | 11.72 | 1M |
| medium | gpt-5-6-terra-medium | 30 | $0.18 | 103 | 1.72 | 6.60 | 1M |
| low | gpt-5-6-terra-low | 27 | $0.14 | 103 | 1.45 | 6.29 | 1M |
| non-reasoning | gpt-5-6-terra-non-reasoning | 21 | $0.14 | 105 | 0.72 | 5.47 | 1M |

### GPT-5.6 Luna (family). Release date and price not checked on the model page (provider table only). Context 1M. Slug: gpt-5-6-luna (max), -xhigh, -high, -medium, -low, -non-reasoning.

| Effort | Slug | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|---|
| max | gpt-5-6-luna | 37 | $0.18 | 121 | 112.63 | 116.77 | 1M |
| xhigh | gpt-5-6-luna-xhigh | 35 | $0.09 | 111 | 48.98 | 53.46 | 1M |
| high | gpt-5-6-luna-high | 32 | $0.04 | 112 | 17.81 | 22.28 | 1M |
| medium | gpt-5-6-luna-medium | 25 | $0.02 | 106 | 2.93 | 7.66 | 1M |
| low | gpt-5-6-luna-low | 21 | $0.01 | 105 | 1.89 | 6.65 | 1M |
| non-reasoning | gpt-5-6-luna-non-reasoning | 16 | $0.01 | 110 | 0.69 | 5.24 | 1M |

### GPT-5.5 (family + Instant). Release date, price not checked; provider table only (context 1.05M for the reasoning variants, 400k for Instant). Slug for xhigh per provider table link: /models/gpt-5-5; other variant slugs not verified.

| Effort | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|
| xhigh | 38 | $2.63 | 99 | 71.20 | 76.28 | 1.05M |
| high | 37 | $1.54 | 91 | 30.20 | 35.67 | 1.05M |
| medium | 34 | $0.90 | 87 | 9.00 | 14.72 | 1.05M |
| low | 31* | n/a | 85 | 1.40 | 7.30 | 1.05M |
| non-reasoning | 23* | n/a | 88 | 0.83 | 6.50 | 1.05M |
| Instant (June 2026) | 26 | $0.69 | 162 | 0.88 | 16.31 (reasoning time 12.35) | 400k |
| Instant (May 2026) | 23* | n/a | n/a | n/a | n/a | 400k |

### GPT-5.4 (family). Release date and price not checked. Slugs not verified (AA page for the xhigh variant is /models/gpt-5-4 per the provider table link).

| Model / effort | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|
| GPT-5.4 (xhigh) | 39* | n/a | 99 | 191.82 | 196.87 | 1.05M |
| GPT-5.4 (low) | 28* | n/a | 63 | 2.34 | 10.24 | 1.05M |
| GPT-5.4 (non-reasoning) | 18* | n/a | 67 | 0.84 | 8.31 | 1.05M |
| GPT-5.4 mini (xhigh) | 24 | $0.45 | 254 | 124.25 | 126.22 | 400k |
| GPT-5.4 mini (medium) | 20* | n/a | 187 | 12.87 | 15.53 | 272k |
| GPT-5.4 mini (non-reasoning) | 11* | n/a | 188 | 0.71 | 3.36 | 272k |
| GPT-5.4 nano (xhigh) | 21 | $0.18 | 171 | 121.12 | 124.04 | 400k |
| GPT-5.4 nano (medium) | 20* | n/a | 176 | 5.15 | 7.99 | 400k |
| GPT-5.4 nano (non-reasoning) | 12* | n/a | 176 | 0.61 | 3.46 | 400k |
| GPT-5.4 Pro (xhigh) | n/a | n/a | n/a | n/a | n/a | 1.05M |

### GPT-5.3 Codex (the only Codex-named model in the list). Released Feb 2026 (page). Price $1.75 in / $14.00 out per 1M, cache discount 90% (page). Context 400k. Knowledge cutoff Aug 31, 2025. Reasoning yes; only the xhigh effort is listed. Slug: gpt-5-3-codex (page title GPT-5.3 Codex (xhigh)).

| Effort | AA Index | Cost/task | t/s | First chunk (s) | Total (s) | Context |
|---|---|---|---|---|---|---|
| xhigh | 33* (estimated) | n/a (page: "Unknown") | 114.8 (page) | 72.79 | 77.15 | 400k |

Page: Intelligence #86/227, speed #49/227, "above average in intelligence, somewhat expensive for its price".

## Practical notes (from the data, not from AA)

- Coding index and agentic index were not available as numbers, so the "coding" view here is only the AA Intelligence Index plus cost and speed.
- Cost/task is the AA weighted cost for one Intelligence Index task, not a per-call price; use it to compare models, not to budget.
- For GPT-6 Astra and GPT-6.1 Sol, the max and xhigh levels have long first-token times (over 170 s). Medium and low are the usable ones for interactive work.
- Browser navigation was flaky on some model pages (text from the previous page sometimes came back). Values above were taken only from a page whose title matched the requested model.
