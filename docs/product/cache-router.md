# Cache and router, measured

What the real CLIs did with the prompt cache and how accurate the routers were; moved out of [quota-pacing.md](quota-pacing.md).

## Cache and router, measured on 13/10/2026

`engine/examples/cache_probe.rs` ran against the real CLIs on a Claude Max subscription (Sonnet 5.5) and a ChatGPT
subscription (GPT-6 Luna). Every question carries about 3,800 new tokens, so a second turn that reads the whole earlier
prefix still shows about 72%: the new question is written, not read. The table now has a `1h written` column.

| Scenario | Second turn: read / written | Reading |
| :--- | :--- | :--- |
| Claude, three turns in one process | 10,876 / 4,296 | the baseline: the whole prefix is read |
| Claude, resume at once, same effort | 10,876 / 4,299 | a restart with `--resume` loses nothing |
| Claude, medium, then resume at high (twice) | 10,876 / 4,296 and 4,295 | **an effort change keeps the cache** |
| Claude, resume after 240 s and 360 s | 10,876 / 4,295 and 4,298 | the 120 s idle close costs nothing |
| Claude, resume after 3,700 s | 0 / 15,171 | the cache lives one hour, then everything is written again |
| Codex, three turns on one thread | read 43%, 34%, 86% | Codex's own cache varies a lot between turns |
| Codex, low then medium on one thread (twice) | read 19% and 34% | no loss beyond that variance |
| Codex, a turn after 6 min and after 35 min | read 84% both | still warm after half an hour |

On a subscription, Claude writes every cache entry with the one-hour lifetime, so closing an idle process after 120 s
and resuming it inside the hour is free, and so is a change of effort through `--resume`. Changing effort before the
next message therefore needs no deferral to a natural break. A change of model still starts a new cache.

The result of every Claude turn names which bucket its cache writes went into (`usage.cache_creation.ephemeral_1h_input_tokens` and
`ephemeral_5m_input_tokens`). Faber keeps "warm until" on the reply (`Message::cache_warm_until`: an hour for the 1-hour bucket, five
minutes for the 5-minute one, nothing when neither is named) and the latest reply shows it in the tooltip of its cost, hidden when
unknown or cold. The lifetime counts from the start of the last request (its assistant message start in the stream, less a 5 s margin),
not from the result; a turn whose start was not seen shows nothing. It is display only: Auto does not route by it, because a change of model starts a new cache anyway. Codex names no
lifetime, so it shows nothing.

`engine/examples/route_eval.rs` sends 30 hand-labelled requests (kind, difficulty and the new `check`) to each router,
one fresh session each, the way Auto asks:

| Router | Valid JSON | Kind | Difficulty | Check | Median / slowest |
| :--- | :--- | :--- | :--- | :--- | :--- |
| GPT-6 Luna, medium | 100% | 93% | 70% | 87% | 3.3 s / 9.1 s |
| GPT-6 Luna, low | 100% | 90% | 70% | 83% | 3.3 s / 4.5 s |
| Haiku 5.5, medium | 100% | 100% | 70% | 73% | 9.6 s / 28.7 s |
| Haiku 5.5, low | 100% | 100% | 70% | 77% | 9.5 s / 19.7 s |

Low effort made neither router faster at the median and Luna slightly less accurate, so the routers stay at medium.
The labels are one reader's, so a disagreement is a question, not a proven mistake. By the labels, the requests the
routers called `blind` against the label were easy, of a kind the raise ignores, or already hard; the raise only moves
medium code, math and research.

## Antigravity benchmark profiles

- Antigravity has 14 profiles for its Gemini models. Auto requires a runnable CLI, confirmed sign-in and a recognized
  subscription tier. The quota factor stays neutral without a reading, and an out-of-quota error excludes Antigravity
  until the reported reset or the default backoff. Antigravity is still outside `Vendor::RUNNABLE`, so Auto can't select
  it. The small classifier still uses Luna or Haiku.
- Antigravity's Intelligence Index scores were checked on 2026-11-23 against [Artificial Analysis's model table](https://artificialanalysis.ai/models/creators/google).
  Coding scores use the rounded mean of Terminal-Bench 4.0 and FrontierCode-AA 2.0.
  Costs use weighted US dollars per benchmark task from the [Neon and Hydrogen](https://artificialanalysis.ai/models/comparisons/gemini-4-neon-vs-gemini-4-hydrogen),
  [Neon and Argon](https://artificialanalysis.ai/models/comparisons/gemini-4-neon-vs-gemini-4-argon) comparisons
  and [Artificial Analysis's provider table](https://artificialanalysis.ai/providers/google) for Flash.
  [Google's standard API pricing](https://aistudio.google.com/docs/pricing) confirms the token-rate basis, with Argon
  using its regular rates from December 1, 2026. These are relative routing costs. Times are explicitly labeled
  benchmark task minutes converted to seconds, cross-checked between the comparison pages and the model table where
  both list a time. Independent end-to-end Antigravity timings are unavailable.

| Model | Intelligence Index (low / medium / high / xhigh) | US$ per task (same order) |
| --- | --- | --- |
| Gemini 4 Neon | 44 / 55 / 60 / 61 | 0.75 / 1.90 / 3.74 / 6.44 |
| Gemini 4 Argon | 36 estimated / 48 / 50 | 7.77 estimated / 8.63 / 9.95 |
| Gemini 4 Hydrogen | 27 / 41 / 51 / 54 | 0.29 / 0.80 / 1.87 / 3.16 |
| Gemini 3.8 Flash | 23 / 30 / 31 | 5.63 estimated / 6.26 / 7.77 |

Argon low has no published benchmark row. Its intelligence and coding scores use 75% of medium as a provisional
estimate. Argon low and Flash low use 90% of their medium rows' cost and time as provisional estimates.
