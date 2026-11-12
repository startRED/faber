# Auto quota pacing

How Auto paces the shared quota, and what was measured about it.

## Auto quota pacing

Auto prices the 2026-11-10 Artificial Analysis per-task runs republished by [Slopalytics](https://slopalytics.com/),
using rounded intelligence and coding sub-index, seconds per task and US$ per task. The class floors and exclusions
are in [models.md](models.md#auto-a-blind-router). Spare quota buys one rung within 600 seconds per task, or 1,200
for hard-or-higher code, non-easy math and expert-or-higher work where quality matters (code, math, research).
Frontier can step to summit (60/50), currently Fable high, only through spare quota or the existing learned rung;
an outright ask for the best lands there directly (R567) and nothing steps above it. Multipliers, gates and learned adjustments keep their rules.
The strongest sufficient row within 15% of the cheapest factored price wins if neither score falls and one improves,
within the task's time cap. Easy/medium chat permits slower quality upgrades within its eligible 300 s pool.
Ties favor lower price then shorter time. This includes token-read cost and learned weights.
Fable low is removed; Luna high (21/13, 336 s, US$0.073) is added. Medium chat uses a 31 intelligence floor and
excludes rows over 300 s when a sufficient row within 300 s exists. At neutral factors: Sonnet medium with Claude
alone, Terra medium when available, otherwise Sol low. Terra medium improves both scores for about 5% more.
Other medium work still uses Sonnet high with Claude alone. The concrete easy/medium/hard task guide, unchanged
scope/check contract and stronger-answer order are in [auto-task-guide.md](auto-task-guide.md):
below high raise effort first, then compare that row against the next stronger model's lowest sufficient effort by
price and time. Sources read 2026-11-11: [Anthropic task guide](https://claude.dev/blog/which-claude-model-should-you-build-with/),
[Anthropic cost guidance](https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence),
[OpenAI models](https://developers.openai.com/codex/models) and [reasoning](https://developers.openai.com/api/docs/guides/reasoning).

Auto requires billing confirmed as a subscription before any user, router or summary request. The same quota gate
checks every dispatch and rechecks waiting requests; plan checks spend no quota. Unknown billing is denied. The shell
starts account watching before Auto, and the screen supplies the local UTC offset on start and each minute. Vendor-wide
windows pace the whole shared account against approximately one seventh of the weekly quota per day; family windows
restrict only that family. Paid-fallback evidence survives restart, and missing evidence remains possible. Approval for
paid credits is bound to one conversation, reply and vendor, is consumed by that dispatch, and cannot authorize an
auxiliary request or bypass unknown billing.

Quota readings never ask a model anything (see "Quota readings that stay current" below). `budget.json` stores bounded quota history, metered spans, learned whole-account shape, explained model weights and
own spend. User turns, routing and hand-over summaries count in Faber's own API-equivalent USD, including cache
reads and writes. Percentage changes and shared-account credits never become Faber's own dollars. Codex prices each
request's token delta at its context tier; a multi-request turn does not inherit a long-context fee just from its sum.
Incomplete meters show a lower bound and cannot teach weights or plan size. Idle account movement is flagged as foreign,
even below the two-point resolution of the numerical outside-use estimate. Own-cost learning accepts only explained,
non-overlapping stretches; the weekly shape continues to describe the whole account. The spec's R89/R90 sections are
in `docs/history/superpowers/specs/2026-10-11-auto-quota-pacing-design.md`.

When included quota runs out, Auto tries another eligible profile once, carrying the conversation and partial reply;
long histories use a bounded, tool-free summary. It waits only for a usable reset within six hours, with guarded
resumption, Stop and shutdown cancellation. A pinned vendor asks before moving. The composer retains an unsent request
and its attachments without overwriting newer input; explicit resumption clears only the matching restored draft.
Settings separates whole-account bars from Faber's own spend and explains foreign use, incomplete meters and billing.

### Quota readings that stay current

Both vendors' counters are read by commands that answer locally, so a reading costs no model tokens:

- Claude Code: a control-only stream sends `initialize`, then `get_usage` with `skip_behaviors: true`. No user message,
  model request, transcript scan or saved session is needed. The correlated successful reply must confirm zero session
  cost, no model usage, available plan windows and a present `model_scoped` array (absent on cached replies). Window
  utilization is a percentage, with a UTC ISO reset timestamp. This experimental contract is documented by SDK 0.3.312 (paired with CLI 2.1.312); unsupported commands or
  invalid/missing windows retain the previous reading. Synthetic `tests/fixtures/claude/usage-control.jsonl` models that
  contract; it is not an authenticated capture. Plan checks use this reader; scheduled refreshes try it when the text
  command returns no percentages. The text command is `claude -p /usage --output-format json` (isolated, no session saved).
  `/usage` is a local
  command; the result reports `num_turns` 0, zero tokens and zero cost, and Faber checks that on every reading: if a CLI
  version ever answered it with a model, the result is thrown away and `/usage` is never run again. Its text gives the session
  and weekly percentages; the reset times are printed in the PC's own time, which the screen's offset converts. A reset time
  within two minutes of the one already held is the same window (the stream prints seconds, `/usage` the minute).
  Claude Code 2.1.307 and later print no percentages there (only what contributes to the limits). The structured slash
  report exposes only the server
  `limits[]` rows, which may be null even for a subscriber. The control reply instead exposes the named windows. If both
  local readers yield no windows, the same scheduled reading may send one tiny request instead (`account::claude_probe`):
  cheapest model, no tools, no skills, one turn, about 500
  cached tokens (US$0.0001 at list price, measured 2026-10-28). Its rate-limit event carries both windows. A probe that failed
  or cost over US$0.01 is no reading; it is never stamped as zero-turn evidence for R131 and never counted as Faber's own
  spend. The probe runs only when `/usage` says the login is on its subscription, extra usage is not running, and a turn or
  probe of the last hour said extra usage is off, so a request past the limit is refused, not charged. With nothing known
  it waits for a real turn to say so, and a probe that gave no clean reading holds the next until newer word. Left open:
  extra usage switched on within that hour while a window is out, one probe under its US$0.005 budget, whose own event
  then stops the next.
- Codex: `account/rateLimits/read` on a short-lived `codex app-server` (handshake, the two reads, nothing else).
- A successful scheduled reading also refreshes the ledger's confirmed no-model evidence. A failed reading or any result with model turns, tokens or cost never refreshes that evidence.
- During Faber's own requests both CLIs already send their windows with the stream (Claude's rate-limit events, Codex's
  rate-limit updates); those go through the same recording.

When it is read (`engine-core/src/cadence.rs`, tested with a fake clock): on start, then every 15 minutes whether the window is
visible or hidden (the owner’s ceiling is a reading never more than 30 minutes old). Three things may ask for an earlier
reading: a request that ended, the window gaining focus, and the PC waking up (a loop that was not heard from for 90 seconds
slept). All three share ONE debounce per vendor: no reading sooner than 5 minutes after the last one for Claude Code (whose
reading starts a heavy Node process, about 12 CPU-seconds and 330 MB for a few seconds) and 1 minute for Codex (about 1
CPU-second), whatever caused the last one. The timer counts from the last reading too. A sleeping PC reads nothing.

Reads are short-lived, hidden (no console window, no shell), one at a time per vendor (a marker lifted by a guard, whichever way
the read ends), and a failure leaves the old reading in place. Windows creates them at idle priority
(`IDLE_PRIORITY_CLASS` in the creation flags, so not one instruction runs at normal priority); elsewhere there is no safe way to
lower the priority from Rust (it would need unsafe code, which Faber keeps to one Windows file), so they run at normal
priority. `/usage` is accepted only after a clean exit, `is_error` false, and every metering field present and well typed
(`num_turns`, the four token counts, the cost); a missing field is no reading, never zero, and any model work makes it `Spent`.
The reset times it prints are parsed defensively (ASCII am/pm only, minutes 0 to 59, hours 1 to 12, days 1 to 31, checked
arithmetic), because the text comes from a program Faber does not control.

The vendors count with a delay (docs below, "Quota lag, measured"). Faber therefore keeps, per vendor, its own requests (`Work`
in the ledger). A request is pending for a window from the moment it ends until that same window has been read after it ended plus
the lag: reading another window settles nothing, and time alone releases nothing, so when every refresh fails the floor stays up.
Pending requests are never dropped; past 100 the oldest are folded into one record that keeps their convertible points and the
latest end. While anything is pending, or a turn of the vendor is
running (for decisions as well as for the screen), the reading is marked "still counting" and no discount is believed on it.
Where the window is the one the conversion was measured on and the request’s model has a measured points-per-dollar (the
learned weights), the unreported share is added as a separate estimate (cost x points per dollar, minus what the reading has
already risen since the work began, taken off only when all the pending work is convertible). Work that cannot be converted adds
nothing to the number but never takes the known part away. No conversion, no number. Decisions use the reading plus that
estimate as a floor. The floor is only computed for the primary window (the shortest vendor-wide one: Claude’s five hours,
Codex’s week); other windows stay as read, so admission on them can run slightly past what a settled reading would allow.

"Running low" is said only when a vendor-wide window is past 90%: under one day of the daily line (R88: 100% / 7 = 14.3%) is
left. Being ahead of the line is said as "being used fast this week" and never as running out, in the answer’s note and in
Settings alike.

### Quota lag, measured

Measured on 15/10/2026 with a 3-minute poller over 90 minutes (both zero-token sources) plus one deliberate Codex run; the
full table and method are in the maintainer notes (`pesquisa/quota-lag.md`).

| Source | Measured lag | Window Faber assumes |
| --- | --- | ---: |
| Claude Code `/usage` | session counter within one poll (3 min) of work ending; weekly counter seen rising 4.1 min after the last request | 10 min (`LAG_CLAUDE`) |
| Codex `account/rateLimits/read` | weekly counter rose 45 to 46 and was seen 29 s after a turn ended | 5 min (`LAG_CODEX`) |

Both are bounds, not physics: the percentages are whole numbers and the poll was every 3 minutes. The vendors do not make the
owner’s 30-minute ceiling impossible. Slow movement seen while Faber is idle is more likely use from the same shared account
elsewhere than a late counter. Points per dollar are not measured for every model; where they are unknown only the
"still counting" marker shows.

### Synthetic monthly acceptance

`engine/tests/simulation.rs` runs four weeks with 200 deterministic seeds, busy weekday usage, shared-account use and
a burst. Both modes use the same gate and synthetic capacity: US$190.10 for the smaller vendor, US$760.40 for the larger
(100 and 400 old cost units, each US$1.901). The results below describe the original 2026-10-12 profiles;
they are historical measurements, not a rerun of the 2026-11-10 table.
The simulation provides ideal fresh readings; it does not model real account latency or establish a live guarantee.
The full test is ignored in ordinary runs and was run explicitly with
`cargo test --release -p faber-engine --test simulation -- --ignored --nocapture` on 2026-10-12.

| Measure | Pacing on | Pacing off |
| --- | ---: | ---: |
| Mean exhausted hours per four-week run | 0.135 | 0.900 |
| Seeds with any vendor exhausted in four weeks | 2.0% | 14.5% |
| Median unused share of the larger quota at reset | 2.5% | 0.9% |
| Mean profile IQ + agent score on the tasks that matter | 81.42 | 80.58 |
| Dropped task share | 0.597% | 0.151% |
| Mean dropped tasks per four-week run | 65.5 | 16.6 |

No budget constants were tuned for that historical run. Its four-seed smoke used `[0, 1, 2, 12]`; seed 12 added an unpaced-exhaustion
case because the original first four seeds all tied at zero exhausted hours. The smoke asserts strictly fewer exhausted
hours, fewer than 1% dropped tasks, no worse quality, fewer than 5% exhausted seeds, and less than 25% unused larger quota.
It passed with 0.000 versus 1.500 exhausted hours and 0.844% versus 0.325% drops. The full 200-seed run passed all the same
acceptance limits. The current simulation lists Terra alongside Sol and Luna and keeps every original seed and
acceptance limit; the historical results above do not describe the 2026-11-10 profiles.
The 2026-11-10 smoke passed: 0.000 versus 1.000 exhausted hours, about 0.041% versus 1.024% dropped tasks,
and mean quality 76.82 versus 76.41 (pacing on versus off). The updated 200-seed run has not been repeated.

### Cache and router, measured on 13/10/2026

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

### Measurement still requiring the owner

The manual `engine/examples/cache_probe.rs` now obtains one baseline before its measured pulses, retains each window's
latest reading, checks unchanged resets and plausible readings, closes immediately on overage, rejection or lost billing,
and applies the gate before each pulse with a sixty-second timeout. It prints own API-equivalent USD separately from
account movement, which is an upper bound on a pulse's quota effect because the accounts are shared. Its three tests
use fake events only, covering the denominator, invalid/reset/missing readings, duplicate windows and immediate stops.
The cache experiment ran on 13/10/2026 (above). The real CLI pulse memory peak has not been measured, and no pulse peak
is claimed.

Existing hidden-screen memory proofs passed: Task 11 measured 108-110 MB idle, 184 MB after six looks, and 141 MB for
1,000 messages. These measurements used the fake CLI and are not a real pulse memory measurement. With
`CARGO_TARGET_DIR=target/proofs`, testkit artifacts and screenshots live in `target/proofs/faber-test`; no junction is
needed. The locale keys, review disposition and implementation limits are listed in
`docs/history/superpowers/auto-quota-pacing-report.md`.
