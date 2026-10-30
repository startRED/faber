# The engine tests' shared waits hold on a loaded runner

Area: none
Files: engine/tests/support/mod.rs
Depends: none
Size: about 40 changed lines

Why: `goals::a_fix_stays_held_to_the_days_money_and_counts_its_cost_after_its_goal_let_go_of_it` failed in `queue/pr-469`
run 41052178816 (2026-10-30 12:15) with "no process was started" (`engine/tests/support/mod.rs:531`), on a change that does
not touch how the fake CLI is started. `next_pipe` counts 300 sleeps of 10 ms, so its budget is whatever 300 timer ticks take
(about 3 to 5 s on Windows), and 151 calls in `engine/tests` lean on it. The same file has more fixed real-clock budgets
that pass on an idle PC and fail on a busy runner: 1 s for a message on the fake wire (line 57), 3 s for the permission
question and for each event (lines 387 and 407), 5 s in `Fake::spawned` (line 375). A deadline only bounds a failure: a
test that passes never waits for it, so a larger one costs nothing when the code is right.

Build: one shared deadline in `support/mod.rs`, and one helper beside it that polls a condition until that deadline (for
example `until(what, || cond).await`, panicking with `what` and the time waited), for the follow-up tasks 013 to 016 to use.
The deadline (a `const`, measured with `std::time::Instant`, so it never depends on how
long a tick lasts) is about 30 s, used by `next_pipe`, `Fake::spawned` and the three `timeout` calls. Each failure message
says how long it waited. Leave the tests that assert something must not happen in time (a short wait there is the check
itself) as they are.

Done: `node scripts/cargo.mjs t --filter goals` and `--filter engine` pass, and `next_pipe` fails with its message after the
shared deadline when no process starts (shown once by hand, then put back). The Rust suite's time does not grow.
