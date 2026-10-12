# Auto: quota pacing, plan detection and quality from spare quota

Status: complete; the cross-vendor review is done and its findings are in section 8; the implementation plan is
`docs/superpowers/plans/2026-10-11-auto-quota-pacing.md`. Sections 1 and 2 were agreed with Erick (11/10/2026); sections 3
to 7 were delegated to the AI ("you analyze everything yourself and do it the way you think is best for Faber's goal")
and are the AI's decisions.
Decision tags: **Erick** decided; **AI** decided on Erick's behalf (he delegated technical choices). Reference numbers
like R85 are the ones in the coordinator's decision list.

## Why

Auto today (`engine/src/route.rs`, `engine/src/engine/auto.rs`) reduces each vendor's quota to one number, the highest `used`
across its windows, and prices a candidate at `cost / left²`. It ignores when each window resets, the weekday, the
difference between the 5-hour and the weekly window, how much quota each model really consumes, use of the account
from outside Faber, and the plan. The goal: Auto balances Claude and Codex better than TL-ADE did, with quality first,
then speed and economy, by reading the quota the way a person would: how much is left, how long until it resets, and
what the week looks like from here.

## Decisions

| # | Decision | By |
|---|---|---|
| D1 | Quota that would expire unused near its reset is spent on quality (a higher effort or model on tasks that matter). Not only on background work. | Erick |
| D2 | Pace each window against the time elapsed, instead of reacting to the used share alone. | AI |
| D3 | Detect the plan of each CLI on every start and keep watching while Faber is open; a plan change resets what was learned about weights for that vendor. | Erick asked, AI designed |
| D4 (R85) | Use of an account from outside Faber (another app, another device, another person) counts in the projection like any other use; it is not an alarm. Faber does not assume, require or encourage that anyone shares a login. Whether sharing a login is allowed stays an open question for Erick (the terms of use are in `docs/research/2026-10-11/respostas-perguntas.md`, item 15). | Erick |
| D5 | Third-party "API-equivalent value of a plan" numbers (30 to 37x and similar, from social posts) are not used. Faber measures the quota a dollar consumes instead. | AI |
| D6 | When the quota of the vendor a conversation runs on is exhausted, the conversation moves to the other vendor with a summary of its history, and the screen says so. | Erick |
| D7 | The latency cap for spare-quota steps depends on the task: hard code and math accept about 3 minutes, everything else about 1 minute. | Erick |
| D8 | Model and effort are chosen once, when the conversation starts, and stay for its life. Changing either mid-conversation makes the CLI re-read the whole context, so Auto never does it for quality or balance. The only moves are the ones already in place (an easy start that moves up once) and D6, where the old vendor cannot answer at all. Confirmed in the vendors' docs (see "Cache facts"). | Erick |
| D9 | Between two vendors that both clear the task, Auto picks the best one overall by weighing spare quota, price, speed and the task's difficulty together, not by one rule. | Erick |
| D10 | Spare quota only buys quality on tasks that matter (code, math and research of medium or high difficulty). Easy chat stays on the fast model. | Erick |
| D11 (R86) | Faber never spends paid extra credits or usage-based billing without asking. When quota runs out it moves to the other vendor or waits, and it uses paid credits only if the user approves at that moment, for that one message. | Erick |
| D12 (R87) | The plan is detected automatically (`claude auth status` for Claude Code; `account/read` and `account/updated` for Codex). The user is asked only if detection fails. Codex keeps using the subscription login, not an API key. (The research recommends an API key until OpenAI confirms in writing that a third-party app may use the ChatGPT login; that stays open for Erick, and nothing in this design depends on it.) | AI, Erick delegated |
| D13 (R88) | On a weekly plan the daily budget is 100% / 7 = 14.28% of the week per day, and Auto paces against that line for both vendors (Claude 20x `seven_day`, Codex 20x 10080-minute window), so each weekly quota lasts all 7 days. The line is the expected-use curve of section 1: uniform (1/7 a day) until the learned shape has data, then the learned shape. Section 3.1 prices a vendor against it: the safe pivot depends on time left, pricing pressure begins below the daily line, and the multiplier keeps increasing above it so a vendor far ahead is left for the other. The idea comes from per-day quota management in TL-ADE; no code is reused. | Erick, via coordinator |

## 1. What Auto knows about each subscription

Inputs, per vendor and per window (the CLIs already send them; nothing reads credential files):

- Claude Code: `rate_limit_event`. The recordings show `rate_limit_info.status`, `rateLimitType`, `overageStatus`,
  `overageDisabledReason`, `isUsingOverage` and `unifiedWindows.{five_hour,seven_day}` with `utilization` (0 to 1,
  two decimals, so a step is 1 point) and `resetsAt` (Unix seconds). Fields are read defensively. A window for a model
  family (such as `opus`) is used if the CLI sends one. **Vendor-wide windows** are `five_hour`, `seven_day` and Codex's
  `<n>_minutes`; every other window is a **family window** and only touches the profiles of that family (its name
  contains `opus`, `sonnet` or `haiku`). The event's `status` counts as vendor-wide only when its `rateLimitType` is a
  vendor-wide window.
- Codex: `account/rateLimits/updated` (and `account/rateLimits/read`) with `usedPercent` (divided by 100 by the engine),
  `windowDurationMins`, `resetsAt`, `planType`, `credits`, `rateLimitReachedType` and `spendControlReached`. A missing
  window is unknown; Faber never assumes one (a Pro account reports only the 7-day window).
- The engine keeps the latest reading of each window in `Core.quotas`, and also on disk (`budget.json` in the data
  folder, see section 5). A reading whose `resetsAt` has passed is ignored. A window with no live reading counts as
  "no information", which prices as normal.

What the data allows (AI, from the recordings):

- Readings come only when a CLI answers, in steps of 1 point, so use from outside Faber appears as a jump between two
  readings. Claude gives no reading without a turn, so Faber asks for one: a **quota pulse** (a one-word question to the
  cheapest Claude model, no tools, no saved session) on the first keystroke in Auto when the Claude reading is older
  than 20 minutes. It runs in the background and is never waited for. Codex readings come free from
  `account/rateLimits/read` on the process Faber already starts for the model list.
- **Use outside Faber** is a *lower bound*, not a total. Faber knows when its own turns run (user turns, router
  questions, pulses and summaries all count as its own), so movement of a window between two readings with none of
  those turns in between, and the same `resetsAt` in both, is outside use. A gap that moved less than 2 points is
  ignored (one point is the resolution). Use that happens during Faber's own turns is not seen, and an interval whose
  end is unknown is left out. The screen says "at least" (section 4); the outside share over the last 7 days is observed
  outside points over observed gap points, and is never presented as the account's total outside use.

Derived values, per window:

- **Pace**: the share of the window's time already elapsed, against the share of its quota already used. Elapsed is
  weighted by the usage profile (below). The window length comes from its name (Claude) or `windowDurationMins` (Codex).
- **Projection `p`**: the used share expected at the reset, `p = used / max(elapsed, 0.10)`. The floor stops a burst in
  the first minutes of a window from looking like a runaway; the other windows still bind.
- **Usage profile**: a *shape* of 168 numbers (weekday x hour, in the PC's local time; daylight-saving shifts are
  ignored) that sum to 1: how the account's weekly burn is spread, kept locally. Elapsed and remaining time are weighted
  by it, so a Saturday with the week at 70% is not "tight" when nobody uses the account on Sunday. Being a shape, it does
  not depend on plan size and survives a plan change. How it is learned:
  - Each pair of consecutive readings of one window with the same `resetsAt` and a gap of 12 hours or less gives a
    movement `d` (points, zero included). `d` is spread over the hour bins the gap overlaps, in proportion to the seconds
    in each bin; it is never put on one hour. Longer gaps and pairs across a reset are discarded.
  - Every bin starts with a prior mass equal to one average hour; observed mass is added on top, and the 168 values are
    normalized to sum to 1. A bin never observed stays at the prior.
  - The shape in use is `(1 - c) x uniform + c x learned`, with `c = min(1, covered hours / 336)`, where covered hours
    is the total length of the accepted gaps. Elapsed share of a window is the shape's mass over the hours already
    passed, divided by its mass over the whole window (a 5-hour window uses the same shape over its five hours).
- **Real weight `w` of each model in the quota**, the price tweak between a vendor's own models:
  - Codex publishes credits per 1M tokens (input / cached / output): GPT-6 Astra 250 / 25 / 1,250; GPT-6.1 Sol 50 /
    2.5 / 250; GPT-6 Luna 2.5 / 0.25 / 12.5. Fast costs 2.5x and Ultrafast 8x the included usage. Plus allows about 5
    to 45 Astra, 15 to 160 Sol and 350 to 3,000 Luna messages per 5 hours.
  - Claude does not publish it. Faber measures quota points per dollar in *stretches*. A stretch is accepted only
    when: it is one Faber turn, or several in a row, on **one model** and one `resetsAt`; no other Faber turn on that
    vendor overlapped it (so parallel conversations are rejected); no outside movement of 2 points or more was seen in
    the gaps on both sides; and the window moved at least **3 points** (a one-point resolution then bounds the error at
    about a third). Because outside use can only add points, the **lower quartile** of the last 20 accepted stretches is
    kept, not the median. It can still be too high when outside use hides inside the stretches, so it is a ceiling on
    the truth, and when the median is more than 2x the lower quartile the model's estimate is marked unreliable and `w`
    stays 1. For Codex the cost is the credits implied by the tokens and the published rates (only the ratio between
    models matters). For Claude it is the per-turn `total_cost_usd` (section 6), which covers sub-model calls too.
  - `w` of a model is its raw points per dollar over the median raw value of its vendor's models, clamped to 0.5 to 2,
    and 1 until 8 accepted stretches exist for it.
  - The raw **capacity** of a vendor (the median raw points per dollar over all its models, before dividing and
    clamping) is kept too. It is the plan-size signal: a change of more than 2x in it is a plan change (section 2),
    because the normalized `w` would hide a fourfold change.

A vendor gets one state from its **worst vendor-wide window** (the one with the highest price multiplier `m`, section
3). A family window adds to the state only of that family's profiles and never changes the vendor's label. The state is
a label for the screen; the router uses `m` itself.

| State (screen words) | When | Effect |
|---|---|---|
| Running low | `m` is 4 or more, or less than 10% is left in a window | priced up; chosen only when the other vendor costs more still |
| On pace | everything else | today's behavior |
| Plenty left | `m` is 0.6 or less (the projection is well under the safe point for the time left) | the router may step up one rung (section 3) |

Plenty left (and any `m` below 1) is never granted while the guards of section 3 hold: the reading is older than 6
hours, outside use just jumped, or the plan is being relearned. The guards only remove discounts; Running low and
exhausted still show and still price up. (Draft change: the agreed text also withheld it while outside use was "high".
Steady heavy outside use already raises the projection, and D4 says it is not an alarm, so only a *jump* blocks.)

## 2. Plan detection and a changing plan

- **Codex**: the app-server Faber already speaks pushes `account/updated` (`authMode`, `planType`) right after start, and
  `planType` also rides every `account/rateLimits/updated`. `account/read` and `account/rateLimits/read` give the first
  reading without a turn. The plan names are `free`, `go`, `plus`, `pro`, `prolite`, `promax`, `team`, `business`,
  `enterprise`, `edu`... Only `planType` and `authMode` are kept (a mode name such as `chatgpt` or an API-key mode is
  not a credential); the rest of those answers (account details) is dropped unread, never logged or stored.
- **Subscription or paid billing (D11, D12).** For each vendor Faber keeps one transient verdict, `subscription` or
  `usage-billed`, from `authMode` (Codex) and from `claude auth status` (a Console or API-key sign-in is reported as
  `api_key`, and then has no `subscriptionType`). Auto only routes to a vendor whose verdict is `subscription`; a
  `usage-billed` login is used only when the user picks that vendor by hand. A plan the user types in never overrides
  this verdict and never authorizes usage-based billing. An inherited API key, auth token, cloud-provider switch or
  custom endpoint is an independent usage-billed override; only environment variable names are inspected, never their
  credential values. `apiKeySource` and `apiProvider` also take precedence over `subscriptionType`. A Claude turn that
  finishes without a quota notice clears subscription confirmation after its own cost is settled.
- **Claude Code**: `claude auth status` prints `subscriptionType` (for example `max`). It does not say 5x or 20x.
  `engine/src/account.rs` already runs this command without a shell for the sign-in check; the same run now also keeps
  `subscriptionType` and drops everything else in the output. It runs on start and about every 15 minutes while Faber is
  open. It is never on the routing path: a check is started at the same moment as the quota pulse, nothing waits for it.
  A discount (any `m` below 1) needs a plan that was *successfully* detected less than 30 minutes ago, or a plan the
  user typed in (D12) that no later successful detection contradicts. A failed or timed-out attempt does not refresh
  the time. (Draft change: the agreed text ran it "before any Spare decision", which would put a process start inside the
  30 s routing budget.)
- The plan name is a change alert and a starting guess. The size of the plan matters only through how much quota a dollar
  consumes, which Faber measures continuously, so it follows a change on its own.
- When the plan name changes, or the vendor's raw capacity (section 1) changes by more than a factor of 2, that vendor's
  learned weights, capacity and outside-use averages are cleared and it gets no discount (`m` is at least 1) until 8
  stretches exist or 48 hours pass, whichever comes first. Running low and exhausted still show and still price up. The
  48 hours matter: a vendor Faber barely uses would otherwise never relearn.
- **D12**: if a plan cannot be detected (the command works but the field is missing, or it failed three times in a row),
  Faber asks once, in Settings > Usage limits, "Which plan do you have?" with the plans of that vendor, and keeps the
  answer locally. A single failed or slow attempt asks nothing. Without a plan, no discount is given (the age rule above
  cannot be met), exhaustion and Running low still work, and the plan shows "not detected".
- **D11**: credits and overage count as exhausted for Auto, not as extra room (section 3).

## 3. How the router chooses (AI, Erick delegated)

The choice happens once, at the start of a conversation (D8). The router keeps today's two steps (the small model reads
the request; the table picks) and changes what the table compares. The draft of 11/10 was re-checked against the
recordings and the profile numbers; what changed, and why:

| Draft | Now | Why |
|---|---|---|
| Spare vendor's profiles judged *only* against "the next level's bar" | Every profile stays in the pool at its own bar; a profile that clears the next distinct bar is priced with the discount twice | Replacing the pool made a vendor's best price jump up the moment it became spare (found in the review: Codex at `m` 0.588 lost to Claude, at 0.675 won, at 0.585 lost again). Now becoming cheaper can only add cheaper options, and at `m` of 1 or more the picks are exactly today's. |
| Next level = the next named level | Next level = the next *distinct* bar, and a new top level `expert` above `hard` | `hard` has no next level, and medium math already uses the hard bar (47, 0), so those tasks could never step up. |
| Quota multiplier `m = ((1 - 0.7) / (1 - p))²`, floor 0.35 | Same shape, but the 0.3 becomes a *margin* that shrinks with the time left, floor 0.1 | With the floor at 0.35 and a fixed 0.7, a spare Claude never beat Codex: Sonnet high at 0.88 x 0.35 = 0.31 against Sol low at 0.13. The feature was dead whenever both vendors were connected. The margin shrinks because a projection is reliable near the reset, so a window at 45% with a day left is nearly all waste. |
| Tight vendor eligible only if no other vendor has a profile | Removed. Price x25 does it, and *exhausted* vendors are not eligible | The hard rule and the price said the same thing twice. |
| Latency cap on every eligible profile | The cap applies to a step-up only | D7 is about spare-quota steps. Applying it to normal picks changes the picks that measured 81 of 88 right. |
| `m < 1` for any task | `m < 1` only on tasks that matter | D10. Without it a spare vendor pulls easy chat to a slower model (Haiku 16.6 s against Luna 8.8 s). |
| "At 40% its quota costs a third" | At 40% with the full window left, a quarter | The draft's own formula gave 0.25; the floor hid it. |
| Section 1's weight `w` was never used | Price = cost x `w` x `m` | The two sections contradicted each other. |
| Reading is the whole truth | Reads older than 6 hours, an outside jump or a plan change keep `m` at 1 or more | Spare quota must not be believed when the reading may be wrong. |

### 3.1 The multiplier

For each window with a live reading: `elapsed` is the profile-weighted share of the window already passed, `left = 1 -
elapsed`, `p = used / max(elapsed, 0.10)`, `margin = 0.3 x sqrt(left)`, and

`m = clamp((margin / (1 - p))², 0.1, 25)`, and `m = 25 x p` when `p >= 1` (bounded by the maximum plausible projection, 15).

The **vendor's `m`** is the highest over its vendor-wide windows. A **profile's `m`** is the highest of its vendor's `m`
and the `m` of its own family window, if there is one; so a full Opus window prices Opus up and leaves Sonnet and Haiku
alone. Examples with the full window left (`margin` 0.3): `p` 0.7 gives 1 (normal), 0.5 gives 0.36, 0.4 gives 0.25, 0.2
gives 0.14, 0.94 or more gives 25. With one day of 7 left (`margin` 0.113): `p` 0.45 gives the floor 0.1, 0.887 gives 1,
0.95 gives 5.1. Guards: `m` is never below 1 while the reading is older than 6 hours, while outside use *jumped*
(observed outside movement in the last 2 hours of more than 3 points and more than 3 times the average observed outside
rate; with no history any 3 points count), while the plan is relearned or not known (section 2), or while the vendor's
verdict is not `subscription`. A vendor with no live reading has `m = 1`.

**The daily line (D13).** `elapsed` is the uniform share (day 1 of 7 = 1/7 = 14.28%) until the learned shape has data, so 14.28% of the account per day is the reference line. Pricing pressure begins below that line at `p > 1 - 0.3 x sqrt(left)`; the line itself costs 25, and a vendor at twice its rate costs 50. A discount requires a projection below that safe pivot. Shared-account use always contributes to the projection. The same formula runs for both vendors; there is no separate daily counter, and engine test `a_week_spent_faster_than_one_seventh_a_day_is_priced_up_on_either_vendor` pins it.

### 3.2 The choice

The table is picked when the message is sent, from the verdict the router cached (kind, level, confidence) and the
quota **as it is at that moment** (3.4), not from the quota at the time the user began typing.

1. **Eligible.** A profile is eligible when the gate of 3.4 lets its vendor spend, its model family is not exhausted, and
   every vendor-wide window of its vendor has `used` of 0.98 or less. Exhausted means: any vendor-wide window at 99% or
   more with its reset in the future; Claude `status` other than `allowed` when `rateLimitType` is vendor-wide, or
   `isUsingOverage` true; Codex `rateLimitReachedType` set, `spendControlReached` true; or the last turn failed with a
   plan-limit error (not a 429 or "temporarily limiting requests", which are short throttles). A family window only
   removes that family's profiles. Paid credits never make anything eligible (D11).
2. **Does the task matter (D10).** Kind is code, math or research and the level (after the existing "unsure gets the next
   level" rule) is medium or hard. Then the profile's factor is `f = m`; otherwise `f = max(1, m)`.
3. **Bars.** Today's bars stay, with one new level, `expert` (code: 50 and 52; everything else: 50 and 0), above
   `hard`. The **next bar** of a task is the bar of the nearest higher level whose bar is different: `medium` code goes
   to `hard`, `medium` research and writing to `hard`, `medium` math (already (47, 0)) and every `hard` task go to
   `expert`. The **cap** on a step is 180 s of whole-answer time for hard code and for math at medium or hard, and 60 s
   for everything else (D7; this reading of "hard code and math" is the AI's, and `secs` in `PROFILES` is the number
   used).
4. **Price and choice (D1, D9).** Every eligible profile that clears the task's own bar is in the pool at price
   `cost x w x f`. On a task that matters, a profile that also clears the **next bar**, fits the cap and sits in a vendor
   whose windows are all at 0.90 or less is priced `cost x w x f x min(f, 1)` instead: a rung up bought with cheap
   quota gets the discount twice, because spending quota that would expire on quality is the point of it (D1). The lowest
   price wins; a tie goes to the faster profile, as today. That is D9: spare quota (`f`), price (`cost`), speed (the cap
   and the tie) and difficulty (the bars) in one comparison. Where `f` is 1 or more the two prices are equal, so with
   every `m` and `w` at 1 the picks are exactly today's; where `f` falls, each price only falls, so a vendor becoming
   cheaper never removes an option. If nothing clears, the bar is dropped and the strongest profile is taken, as today.
5. **The reason.** The router also computes today's pick `U` (`f = 1`, `w = 1`, no step). The answer shows a quota reason
   only when the pick differs from `U`: **step** when the pick has higher quality (intelligence, then agentic) than `U`
   and was priced with the discount twice, **low** when `U`'s vendor has `m >= 4` and the pick moved to the other vendor.
   Learned weights alone never produce a reason.

Routing happens once per conversation (D8), so there is no flip-flopping to damp inside a conversation; the readings
themselves are smoothed by the guards above. All constants (0.3, the 0.10 floor of elapsed, 0.1 and 25, 0.90, 0.98, 4,
6 hours) are starting values. The simulation of section 5 tunes them; nothing else may.

**Limit of this version (ponytail).** The admission check is on the window's *used* share, not on an estimate of the
turn's size, and in-flight turns reserve nothing. A turn that starts at 0.97 can still run the window out; the CLI then
stops with a plan-limit error and 3.3 takes over. Add a per-turn estimate once the weights are trusted.

Worked examples (rows are acceptance tests; the window is full unless said; the vendor not named has `m` = 1, `w` = 1
everywhere; costs and times are the `PROFILES` numbers):

| Case | Pick | Why |
|---|---|---|
| Code, medium; Claude `p` 0.2 (`m` 0.14) | Sonnet 5.5 high | Clears the next bar (47, 44) in 17.5 s: 0.88 x 0.14 x 0.14 = 0.017, below Sonnet medium 0.067 and Sol low 0.13 |
| Same, Claude `p` 0.5 (`m` 0.36) | Sonnet 5.5 high | 0.88 x 0.36 x 0.36 = 0.114 still beats Sol low 0.13 |
| Same, Claude `p` 0.6 (`m` 0.5625) | Sol low | 0.88 x 0.5625 x 0.5625 = 0.28 loses to 0.13 |
| Same, one day left, Claude `p` 0.45, Codex `p` 0.9 (`m` 1.29) | Sonnet 5.5 high | Claude at the floor: 0.88 x 0.1 x 0.1 = 0.009 |
| Code, hard; Claude `p` 0.2 | Opus 5.5 medium | Clears the expert bar (50, 52) in 27.9 s: 1.34 x 0.14 x 0.14 = 0.027; Sol medium costs 0.21 |
| Same, Claude `p` 0.45 (`m` 0.30) | Opus 5.5 medium | 1.34 x 0.30 x 0.30 = 0.119 beats Sol medium 0.21 |
| Same, Claude `p` 0.55 (`m` 0.44) | Sol medium | 1.34 x 0.44 x 0.44 = 0.26 loses to 0.21 |
| Code, hard; both `p` 0.2 | Sol high | 0.32 x 0.14 x 0.14 = 0.006, the lowest (64.8 s is within 180 s) |
| Math, medium; Claude `p` 0.3 (`m` 0.18) | Opus 5.5 medium | Medium math steps to the expert bar (50, 0): 1.34 x 0.18 x 0.18 = 0.045; Sonnet high 0.16, Sol medium 0.21 |
| Chat, easy; Claude `p` 0.2 | Luna | The task does not matter, so `f` stays 1 (D10) |
| Code, medium; Codex `p` 0.95 | Sonnet 5.5 medium | Sol low 0.13 x 25 = 3.2 and Sol medium 5.3, against Sonnet medium 0.48. Reason: low |
| Chat, easy; Codex `p` 0.95 | Haiku 5.5 | 0.02 x 25 = 0.5 against 0.05. Reason: low |
| Research, hard; only Codex, `p` 0.2 | Sol medium | Sol high takes 64.8 s, over the 60 s cap, so no step |
| Research, hard; only Claude, `p` 0.2 | Opus 5.5 medium | One rung above Sonnet high, 27.9 s |
| Code, hard; one hour left, Claude weekly used 0.92 | Sonnet 5.5 high | A window above 0.90 gets no step (0.98 is the admission limit); `m` is about 0.1, so 0.88 x 0.1 = 0.09 beats Sol medium 0.21 |
| Code, medium; Claude reading 7 hours old | Sol low | `m` stays 1 |
| Any task; Claude `seven_day` at 100% | Codex | Not eligible |
| Any task; `opus` window at 100% | Sonnet 5.5 or Codex | Only Opus profiles go; Claude's `m` for Sonnet is untouched |
| Any task; Claude login is API-key billing | Codex | The verdict is not `subscription` (section 2) |
| Code, medium; Claude `w` 0.5, Codex `w` 2, all `m` 1 | Sonnet medium, no reason shown | Weights change the pick (0.24 against 0.26); `U` uses `w` = 1 and the reason stays empty |

PROFILES changes: GPT-6 Luna medium `iq` 29 becomes 30. No `xhigh` row is added in this version: Opus 5.5 high matches
Sonnet 5.5 xhigh on Terminal-Bench (56.6% against 57.1%) for less money ($1.82 against $2.01), Opus xhigh is 161 s and
Sol xhigh 182 s (over the 180 s cap) for 2 to 3 points.

### 3.3 When a vendor cannot answer (D6, D8, D11)

A conversation moves only when its vendor cannot answer at all, never for balance.

- **When it is noticed.** Before sending, from the engine's data (exhausted, 3.2 step 1); or when a turn fails with a plan
  limit. The failed turn may have done work first, so it is handled by what it did:
  - *Nothing happened* (no tool ran, no text streamed): the original message is sent again, once, on the new vendor.
  - *Something happened* (a tool ran or text streamed): it is **not** resent. The new vendor gets a continuation
    request: the earlier messages as below, the user's message, the partial answer kept, and a list of the tool calls
    already done with their results shortened to 200 characters, with the instruction to continue and not to repeat
    them. The partial answer and the tool cards stay in the history.
  - *The hand-over itself fails* (the summary or the second vendor errors): nothing is sent twice, the conversation
    stays as it was on the first vendor, the message goes back to the composer, and the reply shows `error.moveFailed`.
    Faber never retries the hand-over on its own.
- **Same vendor first.** If only a model family is out (Opus), the conversation stays on the vendor and continues on
  another family with `--resume`; the CLI re-reads the context, which is unavoidable and needs no summary.
- **Other vendor.** The conversation starts a session there with its stored text as context: every user and assistant
  message when they total 24,000 characters or less (tool results shortened to 200 characters), otherwise a summary
  written by the other vendor's cheap router model from the first message plus the last 40,000 characters, up to 600
  words. The profile is chosen by 3.2 on that vendor alone. The conversation stays there (D8); the first vendor's saved
  session is kept but not used again unless the user picks it. The text of the conversation goes to the other vendor;
  the screen says so in the same sentence as the move. Ponytail: only text moves; in Projects, files on disk are the
  continuity.
- **Explicit choice.** If the user picked the model themselves, Faber does not move the conversation by itself: it asks
  (section 4).
- **Exhaustion comes first.** In a conversation under way, the existing rule that keeps Auto on one subscription
  (`only` in `auto.rs`) and the move up from an easy start both give way to an exhausted vendor: the exhausted check
  runs before them, and a conversation that moved is routed on the new vendor alone.
- **Both out.** A vendor's recovery time is the **latest** reset among the windows that block it (a full 5-hour window
  that resets in 30 minutes does not help while the weekly window resets in 48 hours). The wait is the **earliest**
  recovery across vendors. A block with no reset time (a limit error with no timestamp) has an unknown recovery and
  counts as longer than 6 hours. If the wait is 6 hours or less, the message waits and sends itself at that time (a
  timer, then checked again through the gate of 3.4; the readings will be past their resets, so they count as no
  information). Otherwise the message is not queued: it stays in the composer with the earliest known reset shown.
- **Paid credits (D11).** Never used on the way. If both vendors are out and one of them could continue on paid credits
  (Claude: `overageStatus` is not off; Codex: credits available), the waiting card offers "use paid credits for this
  message", and a second, plain confirmation names the vendor. The approval is bound to **one vendor and one message**
  and is consumed when that turn starts; nothing carries over to the next message or to a retry.

### 3.4 One gate before every request (fail closed on evidence)

Every request Faber makes to a CLI goes through one function, `gate(vendor, family, purpose)`, where purpose is a user
turn, a router question, a quota pulse, a summary for a hand-over or the plan check. It answers allow or deny with a key:

- Deny when the vendor's verdict is not `subscription` (section 2), unless the user picked that vendor by hand for a
  user turn.
- Deny when the profile's vendor or family is exhausted (3.2 step 1), or any vendor-wide window is above 0.98.
- Deny when **paid fallback is possible** for that vendor (Claude: `overageStatus` is present and not off; Codex:
  `credits.hasCredits` or an unlimited balance) and any vendor-wide window is above 0.90, because the CLI may switch to
  paid use by itself and tell Faber only afterwards. This applies to routers, pulses and summaries as well; a denied
  router falls back to the other vendor's router, or to the everyday default when there is none. A one-message
  approval (3.3) lifts this rule for that message only.
- Allow when there is no reading yet. The first request is what produces the first reading; the cost of that one
  request is accepted. (No supported switch that forbids paid fallback was found in the research. The only hard guard is
  the account's own extra-usage setting being off, which Settings > Usage limits reminds the user of when a vendor
  reports it on. The implementation plan re-checks the CLI flags and the Codex config reference once.)
- As a last layer, a turn that was not approved and shows `isUsingOverage`, a Codex balance moving, or a window at 100%
  with paid fallback possible is interrupted and its vendor marked exhausted. This limits the spend; it cannot undo
  what was spent before Faber was told.

The gate runs again at **dispatch**: the router's verdict is cached (kind, level, confidence), the table is run when the
message is sent (3.2), and the gate checks the chosen profile just before the request leaves, including the everyday
default used when routing is late and the profile of an existing conversation. A warmed CLI for a draft is only a head
start: if the table now picks another profile, the warmed process is closed by the normal idle rule.

Not in this version (ponytail, add when the numbers justify them): on Claude, changing effort alone keeps the prompt
cache, so Auto could raise effort for one message; and when a conversation has been idle longer than the cache lifetime,
the next message could be re-chosen at no cache cost. Section 6 measures whether that second idea has a real gap to
use.

## 4. What the user sees (AI, Erick delegated)

Plain words, no jargon: the screen says "quota to spare", "running low" and "out of quota", never "spare" or "tight".
Every text is a locale key written by hand in all five languages; the engine sends keys and numbers (a reset time is a
Unix timestamp that the screen formats in the user's language). Examples below are English and Brazilian Portuguese.

**The answer's reason line** (under the model name). Today it reads "Auto: code, medium". A quota reason is added only
when it changed the pick (3.2 step 5). The stored `reason` stays `reason.route.<kind>.<level>`, because the move up from
an easy start reads the level from it (`next_route` in `auto.rs`); the quota reason is a separate field, `pace` (kind
`step` or `low`, and the vendor), and the screen puts the two together:

| Key | English | Português (Brasil) |
|---|---|---|
| `reason.pace.step` | Auto: code, medium · one step up, {vendor} has quota to spare | Automático: código, médio · um degrau acima, sobra cota no {vendor} |
| `reason.pace.low` | Auto: conversation, easy · {vendor} is running low | Automático: conversa, fácil · a cota do {vendor} está acabando |

**A conversation that moved** (a note at the top of the first answer on the new vendor; it stays in the history):

| Key | English | Português (Brasil) |
|---|---|---|
| `reason.moved.summary` | {from} is out of quota until {when}. This conversation moved to {to}, with a summary of what came before. | {from} está sem cota até {when}. A conversa passou para o {to}, com um resumo do que veio antes. |
| `reason.moved.full` | {from} is out of quota until {when}. This conversation moved to {to}, with all the earlier messages. | {from} está sem cota até {when}. A conversa passou para o {to}, com todas as mensagens anteriores. |

**Both out, short wait** (the reply shows a waiting card, the running column says "Waiting for quota until {when}", and
the stop button cancels it):

| Key | English | Português (Brasil) |
|---|---|---|
| `reason.waiting` | Both subscriptions are out of quota. Faber sends this message when the first one is back: {when}. | As duas assinaturas estão sem cota. O Faber envia esta mensagem quando a primeira voltar: {when}. |
| `run.waiting` | Waiting for quota until {when} | Esperando cota até {when} |
| `quota.credits.offer` | Use paid credits for this message | Usar créditos pagos nesta mensagem |
| `quota.credits.confirm` | This message will be charged to your {vendor} account as paid credits. Send it? | Esta mensagem será cobrada na sua conta do {vendor} como créditos pagos. Enviar? |

**Both out, long wait** (the send is refused with `error.quotaWait`; the text stays in the composer):

| Key | English | Português (Brasil) |
|---|---|---|
| `error.quotaWait` | Both subscriptions are out of quota until {when}. Your message is still in the box. | As duas assinaturas estão sem cota até {when}. Sua mensagem continua na caixa. |
| `error.moveFailed` | Could not move this conversation to {to}. Nothing was sent twice, and your message is back in the box. | Não deu para passar a conversa para o {to}. Nada foi enviado duas vezes, e sua mensagem voltou para a caixa. |

**The user picked the model** and its vendor is out (a card above the composer):

| Key | English | Português (Brasil) |
|---|---|---|
| `quota.chosen.out` | {vendor} is out of quota until {when}. | O {vendor} está sem cota até {when}. |
| `quota.chosen.move` | Use {other} for this conversation | Usar o {other} nesta conversa |
| `quota.chosen.wait` | Wait | Esperar |

**Settings > Usage limits** gets, per vendor, above its windows:

| Key | English | Português (Brasil) |
|---|---|---|
| `usage.plan` | Plan: {plan} | Plano: {plan} |
| `usage.planUnknown` | Plan not detected | Plano não detectado |
| `usage.planAsk` | Which plan do you have? | Qual plano você tem? |
| `usage.state.spare` | Quota to spare | Sobra cota |
| `usage.state.normal` | On pace | No ritmo |
| `usage.state.tight` | Running low | Acabando |
| `usage.learning` | Still learning your usage pattern | Ainda aprendendo seu padrão de uso |
| `usage.relearning` | The plan changed: learning again | O plano mudou: aprendendo de novo |
| `usage.outside` | Use outside Faber seen between its answers: {share} of the movement in the last 7 days | Uso fora do Faber visto entre as respostas dele: {share} da variação dos últimos 7 dias |
| `usage.overageOn` | Extra usage is on for this account. Faber never uses it without asking, but the tool can; turning it off is the only full guard. | O uso extra está ligado nesta conta. O Faber nunca o usa sem perguntar, mas a ferramenta pode; desligá-lo é a única garantia completa. |
| `usage.billedPerUse` | This sign-in is billed per use, so Auto does not use it. | Este login cobra por uso, então o Automático não o usa. |

and each window row gets a second line, "{elapsed} of the window has passed · about {share} expected at reset"
(`usage.pace`, `usage.expect`), plus a small tick on its meter at the elapsed share, so a bar past the tick reads as
"faster than an even pace". The sidebar footer does not change (one line per vendor, the used share of the tightest
window). Nothing else is added to the screen; no chart.

Texts and numbers the screen formats itself: `{when}` is a time today ("18:40"), a weekday and time within a week ("Mon
00:00"), or a date beyond; `{share}` is a rounded percentage.

## 5. Tests and files

**Pure functions, no process** (new `engine/src/budget.rs`, `learn.rs`, `ledger.rs`, `signals.rs`, `usage.rs` and
`handover.rs`): elapsed with the profile, projection, margin and `m`, the guards, exhausted detection and `gate` (3.4) in
`budget.rs`; the profile, outside use, weight stretches and capacity in `learn.rs`; what is kept and written to
`budget.json` in `ledger.rs`; what a CLI message says about quota, billing and plan in `signals.rs`; the per-turn cost
in `usage.rs`; the text that moves with a conversation in `handover.rs`. Tests in `engine/tests/` with the same names. In `engine/src/route.rs`: the `expert` level, the next distinct bar,
the cap, `choose_paced`, and the reason; tests extend `engine/tests/route.rs` with every row of the worked examples of
3.2, plus: with every `m` and `w` at 1 the picks are exactly today's (the existing tests pass unchanged, which protects
the 81 of 88 measurement; the same holds for `w` alone differing from 1 only in that `U` ignores `w`), and a sweep of
`m` from 0.1 to 25 for several vendor pairs in which the best price never rises when one vendor's `m` falls (no
discontinuity).

**Engine tests with the fake CLI** (`engine/tests/auto.rs`, `claude_session.rs`, `codex_session.rs`, `account.rs`,
new `engine/tests/pacing.rs`), replaying recordings in `tests/fixtures/`. New recordings are the existing ones with only
the numbers changed, named `*.spare.jsonl`, `*.low.jsonl` and so on, so nobody mistakes them for real captures:

- Claude: a `rate_limit_event` with a low `seven_day` (spare), a full one (exhausted), `isUsingOverage: true`, and a turn
  that ends with a plan-limit error; a `claude auth status` answer with and without `subscriptionType`.
- Codex: `account/rateLimits/updated` with `planType` changed from `pro` to `plus` mid-session, `rateLimitReachedType`
  set, credits available.
- Scenarios: a Claude with quota to spare steps a medium code task up; a reading that jumped blocks it; a plan change
  clears the weights and removes discounts but not Running low; a vendor exhausted between the router's answer and
  the dispatch is not used (3.4); an exhausted vendor moves a conversation (with the transcript, with a summary,
  family-only on the same vendor); a turn that failed after a tool ran is continued, not resent, and one that failed
  with nothing done is resent once; a failed hand-over leaves the message in the composer; both exhausted waits for the
  latest blocking reset of the first vendor to recover, and a long wait is refused with the text kept; a request to a
  vendor with paid fallback possible and a window above 0.90 is denied for a router, a pulse and a summary alike; an
  unapproved turn that shows overage is interrupted; a message approved for paid credits is sent once and the next one is
  not covered; an API-key login is never routed to by Auto; an easy start still moves up after a turn that carried a
  quota reason.

**Screen checks** (`tests/ui/settings.check.ts`, `tests/ui/running.check.ts`, a new `tests/ui/pace.check.ts`): the pace
line and tick position from a reading, the state words, the reason keys with their parameters, the waiting state in the
running column, and that every new key exists in all five locales.

**Files to change.** Engine: `engine/src/route.rs`, `engine/src/budget.rs` (new), `engine/src/engine/auto.rs`,
`engine/src/engine.rs`, `engine/src/engine/answer.rs` (the waiting message and the move), `engine/src/claude.rs`,
`engine/src/codex.rs`, `engine/src/events.rs`, `engine/src/model.rs`, `engine/src/account.rs`, `engine/src/store.rs`
(`budget.json`, the cumulative cost per conversation and CLI session, `Message.pace`), `src-tauri/src/main.rs` (fixed commands: tell the engine the PC's UTC offset, answer the plan
question, approve paid credits for a message, move a conversation whose model was chosen by the user; seed readings
when the model list loads). Screen: `ui/api.ts`, `ui/i18n.ts`, `ui/Thread.tsx`, `ui/Settings.tsx`, `ui/sidebar.ts`,
`ui/running.ts`, `ui/styles.css`, `ui/locales/{en,pt-BR,de,es,fr}.ts`. Docs: `docs/design.md` (answer line, Settings),
`docs/roadmap.md` (Auto item).

**What is measured.**

- Cache hit rate and per-turn cost, before anything else (section 6).
- **Simulation** (`engine/tests/simulation.rs`, seeded, pure; the 200 seeds are `#[ignore]` and run with `cargo test
  --release -p faber-engine --test simulation -- --ignored`, while a 4-seed smoke run stays in `cargo t`). Setup, fixed in the test: 200 seeds of four weeks; two
  vendors with capacities of 1 and 4 (units of weekly work); task arrivals follow a Poisson process whose hourly rate
  follows a fixed weekday-hour shape (busy on weekdays 9 to 18, quiet on weekends); kinds and levels follow the mix of
  the 22 real requests; each task's quota use is its profile's `cost` times the vendor's points per dollar; outside use
  is a second Poisson stream at 20% of total capacity, with one burst of 3x for 6 hours in half of the seeds. Demand is
  set to about 85% of the combined capacity, so the unused-quota target is reachable. Pacing on is compared with pacing
  off (all `m` and `w` at 1) on the same seeds. Pass when, over **all** 200 seeds: a vendor is exhausted before its reset
  in at most 5% of the seeds (and not more often than with pacing off); the median unused share of the larger quota at
  the reset is below 25%; and the mean quality (intelligence and agentic score) of the picks for tasks that matter is
  not below pacing off, and at most 1% of the tasks are dropped for want of quota. Constants change only if this fails, and the change is logged here.
- **Replay of the 22 real requests** used for the router (roadmap, 81 of 88): with readings at the pivot (`m = 1`) the
  picks are identical; with a spare Claude the picks that change are only those of tasks that matter.
- **Memory**: the end-to-end test limit (250 MB idle) is unchanged. The quota pulse and the plan check are short-lived
  processes; their peak is measured once and written in `docs/roadmap.md#architecture`. State files stay under 100 KB (the ledger test asserts it).
- **Cost of the pulse**: one pulse moves a window by far less than the 1-point resolution, so it cannot be read from
  one pulse. The probe of section 6 sends 200 pulses in a row on the real Claude account (a few cents of API value) and
  divides the total movement by 200; the result is written in `docs/roadmap.md`. The pulse is kept only if a day of
  pulses (at most one per 20 minutes while the user types) stays under 0.5% of a week; otherwise it runs less often.

## 6. Measure the cache first (the first step of the implementation)

The weights (section 1), the 'idle gap' idea (section 3) and the hand-over cost (3.3) all rest on numbers Faber cannot
see today. `engine/src/claude.rs` (the `result` branch, line 145) adds `cache_creation_input_tokens` and
`cache_read_input_tokens` into `input_tokens`, and the Codex driver keeps only the last model call's counts. Reading
the recordings in `tests/fixtures/` already shows these facts and defects:

- Claude writes its cache with the **1-hour** lifetime in stream-json (`usage.cache_creation.ephemeral_1h_input_tokens`
  is filled, the 5-minute field is 0 in every recording).
- A **resumed** session reads the prefix from the cache: in `claude/resume.jsonl` a new process after `interrupt.jsonl`
  reads 3,185 tokens and writes 74. (The recording keeps no clock, so the gap is unknown; the measurement below fixes
  that.)
- Codex's `inputTokens` **includes** `cachedInputTokens` (14,254 against 6,912 on a first turn; 14,290 against 14,080 on
  the second, a 98.5% hit). `cacheWriteInputTokens` is always 0, as Codex bills no write.
- Codex `last` is the latest model call, `total` is the thread so far; a turn with tools makes several calls, so the
  per-turn usage is the difference of `total` across the turn, not `last`. After a `thread/resume` Codex sends one
  `tokenUsage/updated` **before** the new `turn/started`, carrying the thread so far (in `codex/resume.jsonl`: total
  14,204 input tokens at line 12, 28,435 at line 23, so the resumed turn used 14,231 input, 14,080 of them cached, and
  5 output).
- **Defect:** Claude's `total_cost_usd` and `modelUsage` are cumulative for the conversation, even across a resume (in
  `resume.jsonl`, 0.00023211 = 0.00018326 from the earlier process + 0.00004885 for this turn), while `usage` is for the
  turn. Faber stores `total_cost_usd` as the cost of each message, so from the second message on it is wrong. The weights
  of D5 cannot be measured until this is fixed.
- **`usage` is the main answer only.** In `claude/parallel.jsonl`, `usage` shows 9,932 prompt tokens (4 + 4,537 + 5,391)
  and 483 output, while `modelUsage` shows 35,552 prompt tokens (25,624 + 5,391 + 4,537) and 2,040 output: the rest is
  sub-model work such as web fetches. `total_cost_usd` covers all of it. So the cache numbers describe the main response,
  the cost describes the whole turn, and a turn whose `modelUsage` prompt total is more than `usage`'s is marked
  *incomplete for hit rate* (the cost stays valid for weights).
- An aborted turn (`claude/interrupt.jsonl`) reports a total cost of 0 with empty usage: that is "no counters", not a
  reset.

**Change.**

- `EngineEvent::Done` gets `cache_read_tokens`, `cache_write_tokens` and `cache_write_1h_tokens` (0 on Codex),
  `cost_total_usd: Option<f64>` (the cumulative figure as the CLI reports it; `None` when the result carries none, and
  on Codex) and `incomplete: bool` (the `modelUsage` rule above). `input_tokens` stays "prompt tokens of the main
  response", so a hit rate is `cache_read / input`. `claude.rs` no longer turns a missing cost into 0.
- The engine keeps the last authoritative cumulative cost **per conversation, vendor and CLI session id**
  (`store.rs`). A new total is accepted only when the result has counters (a total above 0 or non-empty `usage`); the
  turn's cost is `new - stored` when `new >= stored`, and `new` itself when `new < stored` and the session id is new
  (a fresh CLI session starts again from 0). An aborted result (total 0, empty usage) never changes the stored base.
  The difference is stored in `Message.cost_usd`.
- `Message` gets the three cache numbers and the `incomplete` flag: small integers, no running totals in memory.
- Codex: the driver keeps the latest `total` for the thread across turns (the one sent after a resume is the base), takes
  a snapshot of it at `turn/started`, and associates later updates with their `turnId`. `Done` reports the latest `total`
  minus the snapshot. With no snapshot (no update ever seen) the turn reports no usage rather than the whole thread, and
  an interrupted turn reports what arrived before the interrupt; neither is charged again on the next turn.

**Measurement** (a manual probe, `cargo run -p faber-engine --example cache_probe -- <scenario>`, never part of `cargo t` (the testkit has
no engine dependency, so the probe lives with the engine); it spawns the
real CLIs through the engine's own spawner (`engine/src/proc.rs`, which hides the console window), uses the cheapest model of each vendor with a prefix over 2,000
tokens, and writes a table to `docs/research/`). Questions and the decisions they drive:

| Question | How | Decides |
|---|---|---|
| Does a second and third turn in one Claude process hit? | 3 turns | The baseline: expect 90% or more |
| Does `--resume` hit after Faber's idle close (about 2 minutes), and after 6 minutes (past a 5-minute lifetime)? | resume at 3 and 6 minutes with the same model and effort | Whether the idle close costs nothing (keep it for memory) and whether the 1-hour lifetime holds; issue #66005 (resume drops `--effort`) is checked by passing `--effort` explicitly |
| Does changing effort keep the cache on Claude? | Sonnet 5.5, `medium` then `high` | Confirms the docs; if not, section 3's idea stays dropped |
| Does a Codex turn with another `effort` keep the cache? | Luna, `low` then `medium` on one thread | Whether the CLI re-reads; D8 already forbids it, this only sizes the cost |
| Is the Codex cache gone after 35 minutes? | 35 minute gap | The 30-minute lifetime from the docs |
| Is `total` after a Codex resume the whole thread? | resume, compare | Already visible in `codex/resume.jsonl`; the probe confirms it on the real server |
| What does a pulse cost? | 200 pulses, total movement divided by 200 | Whether the pulse stays (section 5) |

Acceptance: each row has a measured number in the table, and the cost fix passes tests built on the recordings, with a
floating-point tolerance of 1e-9: `resume.jsonl` after `interrupt.jsonl` costs 0.00004885 (not 0.00023211), an aborted
result leaves the base alone, and a new CLI session id restarts the base. The Codex test built on `codex/resume.jsonl`
expects 14,231 input, 14,080 cached and 5 output tokens for the resumed turn.

## 7. Order of work

1. Section 6: split the cache numbers, fix the per-turn cost, run the probe.
2. Plan detection and the readings (section 2, the start of section 1): the auth verdict, `budget.json`, the quota
   pulse, `account/read`, the plan question, the stored readings.
3. The pacing router: pure code first (windows, `m`, the choice, the gate), then `auto.rs` and the dispatch re-check, then
   the weights and outside use, then the move (3.3).
4. The screen (section 4).

Each step ends with the proof commands of `AGENTS.md` and one commit.

## 8. Review log

| # | Source | Finding | Result |
|---|---|---|---|
| S1 | self, 11/10 | The draft's step never fired with both vendors connected (floor 0.35, fixed 0.7) | Fixed: shrinking margin, floor 0.1 (3.1) |
| S2 | self | Hard tasks and medium math had no next level | New `expert` level (3.2) |
| S3 | self | Section 1's weight was never used by section 3 | Price = cost x `w` x `f` |
| S4 | self | "Other people's use is steady" could not be measured: movement not explained by Faber's turns needs the weight to be known | Outside use from the gaps with no Faber turn; weights from the lower quartile of stretches (1) |
| S5 | self | `claude auth status` before a spare decision put a process start on the routing path | Background check, a decision needs one younger than 30 minutes (2) |
| S6 | self | No reading on Claude without a turn | Quota pulse (1) |
| S7 | self | The plan change cleared the profile, but a shape does not depend on plan size | Profile is a shape and survives; only weights and outside averages clear (2) |
| S8 | self | Draft blocked Plenty left while outside use was high, against D4 | Only a jump blocks (1) |
| S9 | self | `total_cost_usd` is cumulative; message costs are wrong | Fixed first (6) |
| S10 | R85, R86, R87 | New decisions from the coordinator | D4, D11, D12; 3.3 and 4 |
| S11 | self | D6 for a model chosen by hand would override the user | Asks instead (3.3, 4) |
| S12 | self | A weekly wait cannot be a queued message | Queue 6 hours or less; otherwise keep the text (3.3) |
| C1 | cross-vendor review (GPT-6.1 Sol, high, read-only, 21 findings) | Billing: authentication mode was discarded, so API billing could not be told from a subscription | **Fixed**: transient `subscription` / `usage-billed` verdict, a typed plan never overrides it (2, 3.4) |
| C2 | same | Interrupting after an overage notice cannot prevent the charge; routers, pulses and summaries can spend too | **Fixed**: one gate before every request, deny near the limit when paid fallback is possible, one-message approval consumed once, interrupt only as the last layer (3.4). No supported "no paid fallback" switch exists in the research, so the plan re-checks the flags once |
| C3 | same | A quota failure may follow tool work; resending the message repeats effects | **Fixed**: resend only when nothing happened, otherwise a continuation with the actions done; a failed hand-over leaves the text in the composer (3.3) |
| C4 | same | Draft routing used the quota from before the classification; fallbacks were not rechecked | **Fixed**: cache the verdict, run the table at dispatch, gate everything (3.2, 3.4) |
| C5 | same | The earliest reset is not the vendor's recovery | **Fixed**: latest blocking reset per vendor, earliest across vendors, unknown counts as long (3.3) |
| C6 | same | A family window contaminated the whole vendor and a blanket `status` overrode the family exception | **Fixed**: vendor-wide and family windows, profile `m` (1, 3.1, 3.2) |
| C7 | same | The cost baseline could be corrupted by an aborted result; the subtraction was written as 0.0000489 | **Fixed**: baseline per vendor and CLI session id, aborted results ignored; the exact figure is 0.00004885 with a tolerance (6) |
| C8 | same | Codex needs persistent thread counters; `codex/resume.jsonl` shows the baseline | **Fixed** and verified against the recording: 14,231 / 14,080 / 5 (6) |
| C9 | same | `usage` omits sub-model work (`parallel.jsonl`: 9,932 against 35,552 prompt tokens) | **Fixed**: cache numbers are the main response, cost is the whole turn, `incomplete` flag (6) |
| C10 | same | A separate quota reason replaced the `reason.route.*.easy` key that `next_route` reads | **Fixed**: `reason` unchanged, `pace` is its own field; exhaustion runs before the `only` restriction (3.3, 4) |
| C11 | same | Outside use is not measured "without guessing" | **Fixed**: stated as a lower bound, pulses and routers count as Faber's, gaps under 2 points ignored, screen wording changed (1, 4) |
| C12 | same | The lower quartile does not isolate model consumption; two points at one-point resolution is up to 50% error | **Fixed in part**: stretches now need one model, one reset, no overlap and 3 or more points (error about a third); unreliable estimates keep `w` at 1. **Rejected**: "exclusive model attribution with carried uncertainty" as a full model; weights only tweak prices between a vendor's own models and are clamped to 0.5 to 2, so a simple filter is enough (ponytail) |
| C13 | same | Normalizing weights removes the plan-size signal | **Fixed**: raw capacity kept and used for plan-change detection (1, 2) |
| C14 | same | The step created a price discontinuity and flip-flop between vendors (the numbers `m` 0.588, 0.675, 0.585 are right) | **Fixed with another design than the one suggested**: every profile stays in the pool and the discount is applied twice to profiles that clear the next bar, so a falling `m` only adds cheaper options. **Rejected**: an explicit hysteresis band, because routing happens once per conversation (D8) |
| C15 | same | Medium math had no effective step (its bar equals the hard bar) | **Fixed**: next distinct bar, so medium math goes to `expert`. `expert` stays internal; the classifier is unchanged |
| C16 | same | Pricing gives no admission control; concurrent turns reserve nothing | **Fixed in part**: admission limits 0.98 (any turn) and 0.90 (a step). **Rejected for now**: a per-turn size estimate and an in-flight reservation; the CLI stops cleanly at a plan limit and 3.3 handles it, so this waits until the weights are trusted (ponytail, written in 3.2) |
| C17 | same | Failed plan detection has contradictory behavior | **Fixed**: attempted against successful checks, typed plan rule, three failures before asking, relearning removes discounts only (2) |
| C18 | same | The usage-profile learner is unspecified | **Fixed**: spreading by overlap, prior mass, local time, 12-hour gaps, blend by covered hours (1) |
| C19 | same | `m` = 1 alone cannot preserve today's picks (`w`) | **Fixed**: tests state `m` and `w` at 1; `U` uses `w` = 1; a row in the worked examples (3.2, 5) |
| C20 | same | Acceptance data wrong: `p` 0.5 state, and the rung table's times differ from `PROFILES` | **Fixed**: examples recomputed under the new rule; the table in "Data basis" says that `PROFILES` is the number used for caps |
| C21 | same | The simulation gate is not reproducible; the pulse cost cannot be read at one-point resolution | **Fixed**: the setup is fixed in the test, all 200 seeds are judged, the pulse is measured over 200 in a row (5) |

## Data basis (read 11/10/2026)

`PROFILES` in `route.rs` was checked against Artificial Analysis, effort by effort, and still matches (Terminal-Bench 4.0
for `agent`, cost to run the index for `cost`). One change is not applied yet: GPT-6 Luna at medium is now 30, not 29.
No maximum effort in the table: it adds 2 to 4 points for 2 to 5 times the time. AA publishes no per-model coding index or
SWE-bench; Terminal-Bench 4.0 is the only agentic number, so vendor-published numbers (a different harness) are not mixed in.

Rungs for the spare-quota step (same vendor, one effort or model up). The times are the ranges Artificial Analysis shows
across its measurements; the `secs` in `PROFILES` (Sol medium 13.4 s, Sol high 64.8 s, Opus high 44.2 s) are the numbers
the router and the caps use:

| From | To | Index | Terminal-Bench 4.0 | Cost per index run | Whole answer |
|---|---|---|---|---|---|
| Sonnet 5.5 high | Sonnet 5.5 xhigh | 47 to 52 | 44% to 57% | $0.88 to $2.01 | 17 s to 42 s |
| GPT-6.1 Sol medium | GPT-6.1 Sol high | 48 to 50 | 48% to 51.5% | $0.21 to $0.32 | 17 s to 73 s |
| GPT-6.1 Sol high | GPT-6.1 Sol xhigh | 50 to 51 | 51.5% to 54% | $0.32 to $0.39 | 73 s to 182 s |
| Opus 5.5 medium | Opus 5.5 high | 51 to 54 | 52.5% to 56.6% | $1.34 to $1.82 | 28 s to 50 s |
| Opus 5.5 high | Opus 5.5 xhigh | 54 to 56 | 56.6% to 59.6% | $1.82 to $3.46 | 50 s to 161 s |

Sonnet xhigh nearly matches Opus high on Terminal-Bench (57.1% against 56.6%) in 42 s, but it costs more than Opus high
($2.01 against $1.82), so with the table as it is, `expert` picks Opus medium or high rather than a new Sonnet row.
Opus xhigh and Sol xhigh pass two minutes for 2 to 3 points, so they stay out.

## Cache facts (official docs, read 11/10/2026)

- Changing the **model** invalidates the whole cache on both vendors. In Claude Code, `/model` asks for confirmation
  while the cache is warm. `opusplan` makes every plan-mode toggle a model switch.
- Changing **effort**: in Claude Code on Opus 5.5, Sonnet 5.5, Haiku 5.5 and Fable 5.1 (API key or subscription) the
  cache is kept; on other models the next request re-reads everything. On the Claude API effort changes invalidate the
  message blocks. For OpenAI, `reasoning.effort` is part of the cached prefix, so a change can break the cache
  (whether a Codex `configuration_update` keeps it is not documented).
- Cache lifetime: Claude 5 minutes by default, refreshed by each hit; Claude Code uses 1 hour for the main chat on a
  subscription within plan limits (5 minutes with an API key or usage credits). OpenAI GPT-5.6 and later: 30 minutes,
  refreshed on reuse.
- A cache hit costs about 0.1 of the input price (0.05 on Opus 5.5, Sonnet 5.5 and GPT-6.1 Sol); a Claude 5-minute write
  costs 1.25 of it. Cache reads still draw plan usage on both subscriptions; no weighting is published.
- `/compact` rebuilds the conversation's cache; a cold one re-reads the full history at full price.
- Anthropic's own advice: pick model and effort at the start of a session.

CLI ids and efforts: Claude Code accepts `opus`, `sonnet`, `haiku`, `fable` and full ids, with `low`, `medium`, `high`,
`xhigh`, `max` (default `medium`). Codex accepts `gpt-6.1-sol` and `gpt-6-astra` (no `none`) and `gpt-6-luna` (also
`none`), set with `-m` and `model_reasoning_effort`. Astra is about 5 times Sol's price at every effort for 1 to 2 more
index points, so it stays out of the table.
