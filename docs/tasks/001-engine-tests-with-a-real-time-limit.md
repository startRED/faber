# Engine tests that read the real clock stop failing on a busy runner

Area: none
Files: engine/tests/project_facts_cost.rs, engine/src/look.rs
Depends: none
Size: about 70 changed lines

Why: two engine tests judge the result by wall-clock time, so a runner that stalls for a moment turns the queue red.

1. `project_facts_cost_rejects_observed_growth_above_two_mib` and its two siblings (`..._toml_escaped_string_...`,
   `..._toml_multiline_...`) failed in `queue/pr-464` run 41013910227 (2026-10-30 06:24): "the measured reader exceeded
   20 ms" with samples of 24 to 52 ms, and 20.3 to 28 ms for the multiline one, so even the best sample was over. The same
   pull request landed on its third attempt (run 41016350039) with no change to the test. The test lives on the base now
   (`99116659`, #464), so every pull request that runs the Rust suite can meet it. Cause in the file: `measure` takes the
   time of ten reads while `Sampler` (same process, a thread that refreshes the process list every 1 ms through `sysinfo`)
   competes for the same cores, and then rejects when any one sample is over a fixed 20 ms. The 20 ms is the budget of the
   shipped build and the file already says "a sample above the budget can reject it"; on a 4-core shared runner a single
   preempted sample is enough. (The failing message prints 20, not the 120 of the unoptimized branch, so that build ran with
   debug assertions off; check which profile `cargo t --suite rust` builds this test in before choosing the number.)
2. `look::tests::the_search_finds_the_same_text_in_other_files_and_skips_what_says_nothing` (`engine/src/look.rs:434`)
   and three tests next to it (the ones at lines 439, 457 and 470) call `search(..., Limits::AFTER_TURN)`, whose `time` is
   300 ms of real clock; `search` stops walking when `start.elapsed() >= limits.time`. Creating nine files and a tree on a
   runner that scans new files can take longer than that, and then the second expected hit is simply not found. No queue run
   in the last 7 days hit it (the lead saw it locally), but the cause is the same and the fix is two lines.

Build: (1) time the reads before the sampler starts (a first loop with no sampler thread), keep the sampler loop only for the
memory growth, and judge the time by the fastest of the ten reads (noise only adds time, so the minimum is the cost of the
reader and a stalled sample cannot fail it). If the minimum is still over 20 ms on the Windows runner, do not raise the
number: measure the same parse of the 1 MiB control file as a calibration on that machine and compare the reader to a small
multiple of it, and write the measured figures in the pull request. Keep the memory-growth assertion as it is. (2) In the
`look` tests that are not about the time bound, pass `Limits { time: Duration::from_secs(30), ..Limits::AFTER_TURN }` (one
helper in the test module); leave `the_search_stops_in_a_tree_of_empty_folders`, which tests the bound with `Duration::ZERO`.
Never skip or delete either check.

Done: `cargo t --filter project_facts_cost` and `cargo t --filter look::tests` pass 20 times in a row locally, one of them
while another `cargo t --suite rust` runs on the same PC to load the cores; the pull request names the profile and the
measured minimum of the reader. One review by the other vendor.
