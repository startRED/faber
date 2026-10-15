# R2 raw data: GPT-6.1 Sol and GPT-6 Astra on Artificial Analysis

Date seen: 2026-10-11.
Method: each model page was downloaded with curl. The embedded Next.js payload (`self.__next_f.push`) was decoded and the per-model JSON objects were read directly, so the values are the stored page data, not summarized text.
Percent-like benchmarks are stored as fractions in the source and are shown here as percentages. Costs are USD. Speed is tokens/s. TTFT and E2E are seconds as stored by the page.

## Table: one row per model x effort

| Slug | Page URL | Effort | Release | Intel Index | Coding Idx | Agentic Idx | Terminal-Bench Hard | Terminal-Bench 4.0 % | Terminal-Bench 2.1 % | SWE-bench | tau2 | tau2 Banking % | GPQA % | HLE % | AA-LCR % | IFBench | Omniscience idx | Omniscience acc % | Omniscience halluc. % | Output tok (index) | Index cost total $ | $/index task | $/1M in | $/1M out | $/1M cache hit | Blended 7:2:1 $/1M | Speed t/s | TTFT s | E2E s | Context | Max output |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `gpt-6-1-sol` | https://artificialanalysis.ai/models/gpt-6-1-sol | max | 2026-09-29 | 51.83 | n/a | n/a | n/a | 56.1 | n/a | n/a | n/a | n/a | n/a | 52.9 | 83.0 | n/a | 41.53 | 62.1 | 54.3 | 67.2M | $1081.55 | $0.724 | $2 | $10 | $0.1 | $1.47 | 65.4 | 347.78 | 355.4 | 1,000,000 | n/a |
| `gpt-6-1-sol-xhigh` | https://artificialanalysis.ai/models/gpt-6-1-sol-xhigh | xhigh | 2026-09-29 | 51.04 | n/a | n/a | n/a | 54.0 | n/a | n/a | n/a | n/a | n/a | 52.6 | 79.7 | n/a | 40.87 | 60.8 | 50.9 | 36.0M | $662.28 | $0.393 | $2 | $10 | $0.1 | $1.47 | 60.4 | 173.40 | 181.7 | 1,000,000 | n/a |
| `gpt-6-1-sol-high` | https://artificialanalysis.ai/models/gpt-6-1-sol-high | high | 2026-09-29 | 50.24 | n/a | n/a | n/a | 51.5 | n/a | n/a | n/a | n/a | n/a | 51.4 | 82.3 | n/a | 41.45 | 60.8 | 49.4 | 25.4M | $521.32 | $0.319 | $2 | $10 | $0.1 | $1.47 | 58.9 | 64.60 | 73.1 | 1,000,000 | n/a |
| `gpt-6-1-sol-medium` | https://artificialanalysis.ai/models/gpt-6-1-sol-medium | medium | 2026-09-29 | 47.78 | n/a | n/a | n/a | 48.0 | n/a | n/a | n/a | n/a | n/a | 49.9 | 83.3 | n/a | 39.95 | 60.4 | 51.6 | 15.3M | $361.37 | $0.214 | $2 | $10 | $0.1 | $1.47 | 55.4 | 7.53 | 16.6 | 1,000,000 | n/a |
| `gpt-6-1-sol-low` | https://artificialanalysis.ai/models/gpt-6-1-sol-low | low | 2026-09-29 | 42.08 | n/a | n/a | n/a | 30.8 | n/a | n/a | n/a | n/a | n/a | 47.4 | 84.0 | n/a | 37.63 | 58.9 | 51.6 | 9.0M | $250.48 | $0.131 | $2 | $10 | $0.1 | $1.47 | 54.1 | 2.50 | 11.7 | 1,000,000 | n/a |
| `gpt-6-astra` | https://artificialanalysis.ai/models/gpt-6-astra | max | 2026-09-03 | 52.67 | n/a | n/a | n/a | 59.1 | 88.4 | n/a | n/a | 41.4 | 96.1 | 54.7 | 80.7 | n/a | 43.40 | 62.6 | 51.3 | 60.0M | $5324.10 | $3.258 | $10 | $50 | $1 | $7.7 | 63.3 | 391.21 | 399.1 | 1,000,000 | n/a |
| `gpt-6-astra-xhigh` | https://artificialanalysis.ai/models/gpt-6-astra-xhigh | xhigh | 2026-09-03 | 52.39 | n/a | n/a | n/a | 59.6 | 89.1 | n/a | n/a | 43.1 | 96.3 | 54.6 | 80.0 | n/a | 43.42 | 61.9 | 48.3 | 37.9M | $3802.98 | $2.309 | $10 | $50 | $1 | $7.7 | 61.5 | 181.28 | 189.4 | 1,000,000 | n/a |
| `gpt-6-astra-high` | https://artificialanalysis.ai/models/gpt-6-astra-high | high | 2026-09-03 | 50.92 | n/a | n/a | n/a | 54.0 | 89.9 | n/a | n/a | 40.0 | 94.9 | 53.1 | 80.0 | n/a | 43.73 | 61.1 | 44.8 | 26.2M | $2925.01 | $1.725 | $10 | $50 | $1 | $7.7 | 56.9 | 101.55 | 110.3 | 1,000,000 | n/a |
| `gpt-6-astra-medium` | https://artificialanalysis.ai/models/gpt-6-astra-medium | medium | 2026-09-03 | 49.57 | n/a | n/a | n/a | 49.5 | 89.5 | n/a | n/a | 35.5 | 93.9 | 52.7 | 79.7 | n/a | 42.22 | 60.6 | 46.5 | 18.9M | $2434.12 | $1.541 | $10 | $50 | $1 | $7.7 | 54.7 | 8.01 | 17.1 | 1,000,000 | n/a |
| `gpt-6-astra-low` | https://artificialanalysis.ai/models/gpt-6-astra-low | low | 2026-09-03 | 45.78 | n/a | n/a | n/a | 41.9 | 88.0 | n/a | n/a | 32.0 | 93.1 | 49.2 | 80.0 | n/a | 40.55 | 59.5 | 46.9 | 10.0M | $1536.78 | $0.818 | $10 | $50 | $1 | $7.7 | 50.5 | 2.35 | 12.3 | 1,000,000 | n/a |

## Per-model detail

### GPT-6.1 Sol (Max) (max) - `gpt-6-1-sol`
- URL: https://artificialanalysis.ai/models/gpt-6-1-sol
- Release date 2026-09-29; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 51.8333
  - AA-Briefcase Elo: score 1557.2400; output tok/task 114994; cost/task $2.3126; time/task 1736.9 s
  - GDPval-AA Elo: score 1591.9700; output tok/task 45847; cost/task $0.8935; time/task 692.5 s
  - AutomationBench (AA): score 0.6487; output tok/task 13956; cost/task $0.2968; time/task 210.8 s
  - Terminal-Bench 4.0: score 0.5606; output tok/task 108185; cost/task $1.8228; time/task 1634.1 s
  - SciCode: score 0.5417; output tok/task 3885; cost/task $0.0434; time/task 58.7 s
  - HLE: score 0.5292; output tok/task 7694; cost/task $0.0776; time/task 116.2 s
  - GDP PDF: score 0.3100; output tok/task 9718; cost/task $0.4199; time/task 146.8 s
  - CritPt: score 0.3171; output tok/task 23477; cost/task $0.2424; time/task 354.6 s
  - AA-Omniscience: score 41.5333; output tok/task 1649; cost/task $0.0167; time/task 24.9 s
  - AA-LCR: score 0.8300; output tok/task 1074; cost/task $0.1994; time/task 16.2 s
  - Index cost total $1081.55: input 409.56, cache read 126.46, cache write 227.27, output 671.99, reasoning 498.38, answer 173.61
  - Index tokens: input 1,383,452,045, output 67,198,855, reasoning 49,838,197, answer 17,360,658
  - Price per 1M: input $2, output $10, cache hit $0.1 (95% discount), cache write $2.5; blended 7:2:1 $1.47
  - Speed median 65.40 t/s; TTFT median 347.78 s; E2E total 355.42 s (answer part 7.65 s)
  - Omniscience: index 41.53333333333333, accuracy 0.6210, hallucination rate 0.5427

### GPT-6.1 Sol (Xhigh) (xhigh) - `gpt-6-1-sol-xhigh`
- URL: https://artificialanalysis.ai/models/gpt-6-1-sol-xhigh
- Release date 2026-09-29; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 51.0378
  - AA-Briefcase Elo: score 1503.3400; output tok/task 43513; cost/task $1.0386; time/task 721.9 s
  - GDPval-AA Elo: score 1538.3200; output tok/task 25610; cost/task $0.5585; time/task 424.9 s
  - AutomationBench (AA): score 0.6657; output tok/task 10626; cost/task $0.2506; time/task 176.3 s
  - Terminal-Bench 4.0: score 0.5404; output tok/task 56902; cost/task $1.0259; time/task 944.1 s
  - SciCode: score 0.5567; output tok/task 1747; cost/task $0.0220; time/task 29.0 s
  - HLE: score 0.5264; output tok/task 4219; cost/task $0.0428; time/task 70.0 s
  - GDP PDF: score 0.3180; output tok/task 4536; cost/task $0.3681; time/task 75.3 s
  - CritPt: score 0.3171; output tok/task 11156; cost/task $0.1191; time/task 185.1 s
  - AA-Omniscience: score 40.8667; output tok/task 798; cost/task $0.0082; time/task 13.2 s
  - AA-LCR: score 0.7967; output tok/task 480; cost/task $0.1935; time/task 8.0 s
  - Index cost total $662.28: input 302.22, cache read 84.17, cache write 162.23, output 360.06, reasoning 222.80, answer 137.25
  - Index tokens: input 934,466,155, output 36,005,814, reasoning 22,280,455, answer 13,725,359
  - Price per 1M: input $2, output $10, cache hit $0.1 (95% discount), cache write $2.5; blended 7:2:1 $1.47
  - Speed median 60.37 t/s; TTFT median 173.40 s; E2E total 181.68 s (answer part 8.28 s)
  - Omniscience: index 40.86666666666667, accuracy 0.6082, hallucination rate 0.5091

### GPT-6.1 Sol (High) (high) - `gpt-6-1-sol-high`
- URL: https://artificialanalysis.ai/models/gpt-6-1-sol-high
- Release date 2026-09-29; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 50.2378
  - AA-Briefcase Elo: score 1464.5200; output tok/task 34116; cost/task $0.8369; time/task 585.3 s
  - GDPval-AA Elo: score 1509.5200; output tok/task 18768; cost/task $0.4277; time/task 322.0 s
  - AutomationBench (AA): score 0.6450; output tok/task 9189; cost/task $0.2258; time/task 157.6 s
  - Terminal-Bench 4.0: score 0.5152; output tok/task 43650; cost/task $0.8282; time/task 748.9 s
  - SciCode: score 0.5579; output tok/task 843; cost/task $0.0130; time/task 14.5 s
  - HLE: score 0.5139; output tok/task 2600; cost/task $0.0266; time/task 44.6 s
  - GDP PDF: score 0.3200; output tok/task 2671; cost/task $0.3494; time/task 45.8 s
  - CritPt: score 0.3000; output tok/task 6833; cost/task $0.0757; time/task 117.2 s
  - AA-Omniscience: score 41.4500; output tok/task 433; cost/task $0.0046; time/task 7.4 s
  - AA-LCR: score 0.8233; output tok/task 272; cost/task $0.1914; time/task 4.7 s
  - Index cost total $521.32: input 267.55, cache read 69.74, cache write 142.01, output 253.77, reasoning 129.30, answer 124.47
  - Index tokens: input 782,097,221, output 25,376,903, reasoning 12,930,309, answer 12,446,594
  - Price per 1M: input $2, output $10, cache hit $0.1 (95% discount), cache write $2.5; blended 7:2:1 $1.47
  - Speed median 58.93 t/s; TTFT median 64.60 s; E2E total 73.08 s (answer part 8.48 s)
  - Omniscience: index 41.45, accuracy 0.6080, hallucination rate 0.4936

### GPT-6.1 Sol (Medium) (medium) - `gpt-6-1-sol-medium`
- URL: https://artificialanalysis.ai/models/gpt-6-1-sol-medium
- Release date 2026-09-29; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 47.7833
  - AA-Briefcase Elo: score 1360.5200; output tok/task 18712; cost/task $0.4644; time/task 337.8 s
  - GDPval-AA Elo: score 1448.7900; output tok/task 10138; cost/task $0.2312; time/task 183.0 s
  - AutomationBench (AA): score 0.6257; output tok/task 7492; cost/task $0.1963; time/task 135.3 s
  - Terminal-Bench 4.0: score 0.4798; output tok/task 31952; cost/task $0.6138; time/task 576.8 s
  - SciCode: score 0.5324; output tok/task 481; cost/task $0.0093; time/task 8.7 s
  - HLE: score 0.4986; output tok/task 1199; cost/task $0.0126; time/task 21.6 s
  - GDP PDF: score 0.3000; output tok/task 1479; cost/task $0.3375; time/task 26.7 s
  - CritPt: score 0.2771; output tok/task 3261; cost/task $0.0395; time/task 58.9 s
  - AA-Omniscience: score 39.9500; output tok/task 197; cost/task $0.0022; time/task 3.6 s
  - AA-LCR: score 0.8333; output tok/task 156; cost/task $0.1902; time/task 2.8 s
  - Index cost total $361.37: input 208.71, cache read 47.63, cache write 105.32, output 152.66, reasoning 55.34, answer 97.32
  - Index tokens: input 546,325,961, output 15,265,677, reasoning 5,534,068, answer 9,731,609
  - Price per 1M: input $2, output $10, cache hit $0.1 (95% discount), cache write $2.5; blended 7:2:1 $1.47
  - Speed median 55.37 t/s; TTFT median 7.53 s; E2E total 16.56 s (answer part 9.03 s)
  - Omniscience: index 39.95, accuracy 0.6040, hallucination rate 0.5164

### GPT-6.1 Sol (Low) (low) - `gpt-6-1-sol-low`
- URL: https://artificialanalysis.ai/models/gpt-6-1-sol-low
- Release date 2026-09-29; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 42.0836
  - AA-Briefcase Elo: score 1116.4300; output tok/task 6713; cost/task $0.1771; time/task 125.0 s
  - GDPval-AA Elo: score 1316.6800; output tok/task 5085; cost/task $0.1050; time/task 94.7 s
  - AutomationBench (AA): score 0.5257; output tok/task 5459; cost/task $0.1594; time/task 101.6 s
  - Terminal-Bench 4.0: score 0.3081; output tok/task 17351; cost/task $0.3829; time/task 323.0 s
  - SciCode: score 0.5324; output tok/task 400; cost/task $0.0084; time/task 7.4 s
  - HLE: score 0.4736; output tok/task 668; cost/task $0.0073; time/task 12.4 s
  - GDP PDF: score 0.2700; output tok/task 1145; cost/task $0.3341; time/task 21.3 s
  - CritPt: score 0.2486; output tok/task 2075; cost/task $0.0272; time/task 38.6 s
  - AA-Omniscience: score 37.6333; output tok/task 121; cost/task $0.0015; time/task 2.2 s
  - AA-LCR: score 0.8400; output tok/task 126; cost/task $0.1899; time/task 2.3 s
  - Index cost total $250.48: input 160.33, cache read 31.27, cache write 73.35, output 90.14, reasoning 24.51, answer 65.63
  - Index tokens: input 369,938,580, output 9,014,298, reasoning 2,450,857, answer 6,563,441
  - Price per 1M: input $2, output $10, cache hit $0.1 (95% discount), cache write $2.5; blended 7:2:1 $1.47
  - Speed median 54.14 t/s; TTFT median 2.50 s; E2E total 11.74 s (answer part 9.23 s)
  - Omniscience: index 37.63333333333333, accuracy 0.5887, hallucination rate 0.5162

### GPT-6 Astra (Max) (max) - `gpt-6-astra`
- URL: https://artificialanalysis.ai/models/gpt-6-astra
- Release date 2026-09-03; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 52.6737
  - AA-Briefcase Elo: score 1570.1000; output tok/task 68691; cost/task $9.5021; time/task 1088.7 s
  - GDPval-AA Elo: score 1573.9000; output tok/task 39975; cost/task $4.5295; time/task 633.6 s
  - AutomationBench (AA): score 0.6849; output tok/task 12900; cost/task $1.5956; time/task 204.5 s
  - Terminal-Bench 4.0: score 0.5909; output tok/task 75664; cost/task $8.4981; time/task 1199.2 s
  - SciCode: score 0.5648; output tok/task 4187; cost/task $0.2323; time/task 66.4 s
  - HLE: score 0.5468; output tok/task 7498; cost/task $0.3781; time/task 118.8 s
  - GDP PDF: score 0.3100; output tok/task 9247; cost/task $2.0758; time/task 146.6 s
  - CritPt: score 0.3171; output tok/task 22584; cost/task $1.1662; time/task 357.9 s
  - AA-Omniscience: score 43.4000; output tok/task 1915; cost/task $0.0970; time/task 30.4 s
  - AA-LCR: score 0.8067; output tok/task 1080; cost/task $0.9973; time/task 17.1 s
  - Index cost total $5324.10: input 2323.72, cache read 1066.15, cache write 978.49, output 3000.39, reasoning 2222.39, answer 778.00
  - Index tokens: input 1,172,335,977, output 60,007,725, reasoning 44,447,788, answer 15,559,937
  - Price per 1M: input $10, output $50, cache hit $1 (90% discount), cache write $12.5; blended 7:2:1 $7.7
  - Speed median 63.27 t/s; TTFT median 391.21 s; E2E total 399.11 s (answer part 7.90 s)
  - Omniscience: index 43.4, accuracy 0.6260, hallucination rate 0.5134

### GPT-6 Astra (Xhigh) (xhigh) - `gpt-6-astra-xhigh`
- URL: https://artificialanalysis.ai/models/gpt-6-astra-xhigh
- Release date 2026-09-03; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 52.3863
  - AA-Briefcase Elo: score 1546.4200; output tok/task 44691; cost/task $6.5635; time/task 731.1 s
  - GDPval-AA Elo: score 1554.4700; output tok/task 24036; cost/task $3.0381; time/task 393.2 s
  - AutomationBench (AA): score 0.6718; output tok/task 10314; cost/task $1.4064; time/task 168.7 s
  - Terminal-Bench 4.0: score 0.5960; output tok/task 43275; cost/task $5.8555; time/task 708.0 s
  - SciCode: score 0.5567; output tok/task 1971; cost/task $0.1216; time/task 32.2 s
  - HLE: score 0.5459; output tok/task 5001; cost/task $0.2532; time/task 81.8 s
  - GDP PDF: score 0.3220; output tok/task 5579; cost/task $1.9127; time/task 91.3 s
  - CritPt: score 0.3143; output tok/task 15220; cost/task $0.7980; time/task 249.0 s
  - AA-Omniscience: score 43.4167; output tok/task 979; cost/task $0.0502; time/task 16.0 s
  - AA-LCR: score 0.8000; output tok/task 532; cost/task $0.9699; time/task 8.7 s
  - Index cost total $3802.98: input 1907.92, cache read 843.61, cache write 783.14, output 1895.06, reasoning 1224.56, answer 670.49
  - Index tokens: input 934,379,760, output 37,901,142, reasoning 24,491,270, answer 13,409,872
  - Price per 1M: input $10, output $50, cache hit $1 (90% discount), cache write $12.5; blended 7:2:1 $7.7
  - Speed median 61.51 t/s; TTFT median 181.28 s; E2E total 189.41 s (answer part 8.13 s)
  - Omniscience: index 43.416666666666664, accuracy 0.6185, hallucination rate 0.4832

### GPT-6 Astra (High) (high) - `gpt-6-astra-high`
- URL: https://artificialanalysis.ai/models/gpt-6-astra-high
- Release date 2026-09-03; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 50.9191
  - AA-Briefcase Elo: score 1505.8900; output tok/task 32139; cost/task $4.7635; time/task 558.0 s
  - GDPval-AA Elo: score 1523.9000; output tok/task 18674; cost/task $2.4322; time/task 324.2 s
  - AutomationBench (AA): score 0.6661; output tok/task 9189; cost/task $1.2952; time/task 159.5 s
  - Terminal-Bench 4.0: score 0.5404; output tok/task 30430; cost/task $4.0470; time/task 528.3 s
  - SciCode: score 0.5544; output tok/task 939; cost/task $0.0696; time/task 16.3 s
  - HLE: score 0.5306; output tok/task 3174; cost/task $0.1618; time/task 55.1 s
  - GDP PDF: score 0.3100; output tok/task 3221; cost/task $1.7948; time/task 55.9 s
  - CritPt: score 0.2886; output tok/task 7944; cost/task $0.4335; time/task 137.9 s
  - AA-Omniscience: score 43.7333; output tok/task 520; cost/task $0.0272; time/task 9.0 s
  - AA-LCR: score 0.8000; output tok/task 328; cost/task $0.9597; time/task 5.7 s
  - Index cost total $2925.01: input 1613.65, cache read 660.27, cache write 672.39, output 1311.36, reasoning 701.58, answer 609.78
  - Index tokens: input 742,163,538, output 26,227,128, reasoning 14,031,525, answer 12,195,603
  - Price per 1M: input $10, output $50, cache hit $1 (90% discount), cache write $12.5; blended 7:2:1 $7.7
  - Speed median 56.95 t/s; TTFT median 101.55 s; E2E total 110.33 s (answer part 8.78 s)
  - Omniscience: index 43.733333333333334, accuracy 0.6113, hallucination rate 0.4477

### GPT-6 Astra (Medium) (medium) - `gpt-6-astra-medium`
- URL: https://artificialanalysis.ai/models/gpt-6-astra-medium
- Release date 2026-09-03; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 49.5704
  - AA-Briefcase Elo: score 1460.5900; output tok/task 26424; cost/task $3.9400; time/task 478.7 s
  - GDPval-AA Elo: score 1485.3800; output tok/task 12943; cost/task $1.8224; time/task 234.5 s
  - AutomationBench (AA): score 0.6459; output tok/task 7829; cost/task $1.1770; time/task 141.8 s
  - Terminal-Bench 4.0: score 0.4949; output tok/task 29667; cost/task $4.4301; time/task 537.5 s
  - SciCode: score 0.5417; output tok/task 563; cost/task $0.0509; time/task 10.2 s
  - HLE: score 0.5273; output tok/task 1947; cost/task $0.1005; time/task 35.3 s
  - GDP PDF: score 0.3040; output tok/task 1721; cost/task $1.7198; time/task 31.2 s
  - CritPt: score 0.2914; output tok/task 4991; cost/task $0.2851; time/task 90.4 s
  - AA-Omniscience: score 42.2167; output tok/task 276; cost/task $0.0150; time/task 5.0 s
  - AA-LCR: score 0.7967; output tok/task 198; cost/task $0.9533; time/task 3.6 s
  - Index cost total $2434.12: input 1488.44, cache read 614.41, cache write 593.07, output 945.68, reasoning 405.19, answer 540.49
  - Index tokens: input 689,951,701, output 18,913,587, reasoning 8,103,798, answer 10,809,789
  - Price per 1M: input $10, output $50, cache hit $1 (90% discount), cache write $12.5; blended 7:2:1 $7.7
  - Speed median 54.70 t/s; TTFT median 8.01 s; E2E total 17.15 s (answer part 9.14 s)
  - Omniscience: index 42.21666666666667, accuracy 0.6057, hallucination rate 0.4653

### GPT-6 Astra (Low) (low) - `gpt-6-astra-low`
- URL: https://artificialanalysis.ai/models/gpt-6-astra-low
- Release date 2026-09-03; context window 1,000,000 tokens; max output n/a
- Intelligence Index: 45.7819
  - AA-Briefcase Elo: score 1257.0500; output tok/task 9910; cost/task $1.4430; time/task 194.8 s
  - GDPval-AA Elo: score 1388.9800; output tok/task 6698; cost/task $0.8545; time/task 131.6 s
  - AutomationBench (AA): score 0.5910; output tok/task 6084; cost/task $0.9991; time/task 119.6 s
  - Terminal-Bench 4.0: score 0.4192; output tok/task 14876; cost/task $2.2486; time/task 292.4 s
  - SciCode: score 0.5405; output tok/task 405; cost/task $0.0425; time/task 8.0 s
  - HLE: score 0.4921; output tok/task 714; cost/task $0.0389; time/task 14.0 s
  - GDP PDF: score 0.3040; output tok/task 1251; cost/task $1.6963; time/task 24.6 s
  - CritPt: score 0.2629; output tok/task 2237; cost/task $0.1449; time/task 44.0 s
  - AA-Omniscience: score 40.5500; output tok/task 115; cost/task $0.0070; time/task 2.3 s
  - AA-LCR: score 0.8000; output tok/task 135; cost/task $0.9501; time/task 2.7 s
  - Index cost total $1536.78: input 1037.04, cache read 350.13, cache write 406.26, output 499.74, reasoning 119.75, answer 379.98
  - Index tokens: input 410,696,631, output 9,994,716, reasoning 2,395,051, answer 7,599,665
  - Price per 1M: input $10, output $50, cache hit $1 (90% discount), cache write $12.5; blended 7:2:1 $7.7
  - Speed median 50.46 t/s; TTFT median 2.35 s; E2E total 12.26 s (answer part 9.91 s)
  - Omniscience: index 40.55, accuracy 0.5953, hallucination rate 0.4691

## Field notes

- Coding Index and Agentic Index: no per-model numeric field exists in any decoded page. "Coding Agent Index" appears only as a label; the separate page https://artificialanalysis.ai/agents/coding-agents covers agent products, not these models. Reported as n/a.
- Terminal-Bench Hard (terminalbenchHard): null for all ten variants on their model pages. The page https://artificialanalysis.ai/evaluations/terminalbench-hard lists only 7 models, none of them GPT-6. Terminal-Bench 4.0 (terminalBench40) is populated and shown separately; Terminal-Bench 2.1 (terminalBench21) is populated only for GPT-6 Astra.
- SWE-bench: no SWE-bench field or evaluation page exists (n/a). Slugs swe-bench, swe-bench-verified, swe-bench-pro return 404.
- tau2: null for all; tauBanking (tau2 banking) populated only for GPT-6 Astra.
- GPQA: populated only for GPT-6 Astra; null for GPT-6.1 Sol.
- IFBench: null for all ten (the /evaluations/ifbench page does not list them).
- Max output tokens: not in page data (n/a).
- E2E: endToEndResponseTime.total is the stored median for the index prompt mix.
- TTFT of 391 s (Astra max) and 348 s (Sol max) are as stored; not independently verified and look like queueing artifacts.
- Intelligence Index values here are raw (e.g. Sol max 51.83). The earlier WebFetch summary rounded them (e.g. 52).
- Rank context from the summary pass only (not in raw data): Sol max #11, Sol xhigh #14, Sol high #17, Sol medium #24, Sol low #47 of 227.
