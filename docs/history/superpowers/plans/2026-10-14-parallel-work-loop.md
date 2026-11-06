# Parallel work loop: plan, run, measure, replan

Date: 2026-10-14. Decisions: R128 (Erick, goal), R129 (AI, Erick delegated the mechanism).
Scope: how Claude Code and Codex develop Faber in parallel today, then how Faber handles its users' requests. It
changes no product code by itself; what it learns becomes the defaults of the self-development scheduler (R91).

## Goal

Most finished, proven Faber work per day and per unit of quota, with nothing waiting: no lane idle on CI, review,
merges, the owner, disk, RAM or quota when it could be working. The loop never ends by decree; it settles when two
rounds in a row find no measurable gain, and restarts when something changes (new model, new plan, new PC load).

## Rules every round keeps

- AGENTS.md and the proof commands stay as they are. No test or guard is weakened to go faster.
- Erick's PC: no visible windows, below-normal priority for heavy builds, games never lose focus.
- Quota: read it fresh before launching a lane (`get_usage`, scratchpad `pace.sh`); the vendor most behind its pace
  gets the next lane that fits it. Parallelism spends quota faster, it never creates it.
- Cross-vendor review per chunk, two reviews for security (verify-model-output). The author keeps working while the
  review runs; findings come back as follow-up commits.
- R126/R127: each finished part is pushed at once and main follows the newest green phase-1-base.
- A change to the workflow stays only if the round's numbers show a gain at equal or lower cost.

## Metrics

Collected by one report script that lives outside the repository (it reads local Claude Code transcripts for token
counts, which Faber's own code never does), written as one JSON line per round to `faber-notas/parallel-rounds.jsonl`.

| # | Metric | Source |
|---|---|---|
| M1 | Lead time: chunk start to its commit on phase-1-base | git log, lane start time |
| M2 | Throughput: chunks merged per day | git log on phase-1-base |
| M3 | Wait share per lane, by cause: CI, review, merge conflict, owner, build slot, quota | lane reports, `gh run list` |
| M4 | API-equivalent cost per merged chunk, and the cache-write share of it | transcripts priced with `engine/src/usage.rs` rates; Codex usage from its own quota reading |
| M5 | PC load: peak RAM, CPU, disk free on E:, focus steals (must stay 0) | `df`, job-object meter from the test rework |
| M6 | Quality: red CI after merge, reverts, review findings per chunk | gh, git |

## Baseline (round 0, measured 2026-10-14)

- Four open branches stacked on each other, 90+ commits ahead of and 46 behind phase-1-base.
- CI 17 to 28 min per push; one Windows job runs every suite in a row.
- `cargo t --all` about 562 s locally; one engine file changed peaks at 7.8 GB RAM.
- 10 to 13 GB of build output per worktree; 63 GB free on E:. PC: 16 threads, 36 GB RAM.
- Claude, Faber only, week from 13/10: US$ 263 API-equivalent over 4,164 requests; cache writes US$ 133, cache reads
  US$ 108, output US$ 27. Fixed prompt per main-session request about 81k tokens (app tools 34k, MCP tools 25k, skill
  list 10k, system and style 9k, plugin hooks 3 to 4k).
- Quota pace, all ahead: Claude 27% used for a 20% target, Codex 32% for 22%, Antigravity Claude/GPT bucket 98%.

## The loop

Each round is one working day, or about six merged chunks, whichever comes first.

1. **Plan.** Pick chunks from the roadmap as big as one agent does well (big-chunks). Give each a lane: an area of
   files it owns for the round, a vendor by pace and strength, and its proof. Two lanes never own the same files: the
   brief names the lane's claim, which it writes to `.faber-lane` before its first edit and deletes once it lands, and
   `cargo t` enforces it (R154, [design.md](../../design.md#lane-claims)).
2. **Run.** Lanes are separate sessions (Claude) or `codex exec` runs, each in its own worktree cut from
   phase-1-base. One integrator lane merges finished parts in order and keeps main following.
3. **Measure.** The report script fills M1 to M6 for the round.
4. **Find the worst wait.** The largest share in M3, or the largest cost in M4, is the round's bottleneck.
5. **Change one thing** from the experiment list (or a new one the numbers suggest), with a stated hypothesis and the
   gain it should show.
6. **Keep or revert** by the next round's numbers. Record the result and the decision in `faber-notas/decisoes.md`
   with its author.

## Experiments, in order of expected gain for their cost

| # | Change | Hypothesis | Proof of gain |
|---|---|---|---|
| E1 | Merge the four stacked branches, then branches live under a day and come from phase-1-base | Conflicts and late merges stop eating hours | M3 merge share near 0; M1 drops |
| E2 | CI split into parallel jobs (Windows screen, Windows rust and ui, macOS) with stale runs cancelled | CI wait drops from 17-28 to about 10-12 min | M3 CI share; run durations |
| E3 | Build slots: `cargo t` waits for one of 3 machine-wide slots before a heavy build | No RAM spikes or disk crises with 6 or more lanes | M5 peak RAM, disk floor 30 GB |
| E4 | Drop what Faber never uses from its sessions: rea, video skills, caveman, adhd | About 6 to 8k fewer tokens per request, US$ 8 to 10 a week | M4 per request, same work |
| E5 | Fewer, longer lanes instead of many short agents | Each new agent pays the full prompt as a cache write (about US$ 0.64 on Opus) | M4 cache-write share drops from 50% |
| E6 | Codex takes the main code lanes while Claude runs ahead of pace; Claude plans, designs and reviews | Both vendors end the week near 100%, neither runs out early | Pace per vendor at the reset |
| E7 | Cloud lanes (Codex Cloud, Claude cloud sessions) for rust and ui work; screen tests stay in CI | More lanes without PC load | M5 flat while M2 rises; setup cost recorded |
| E8 | The test system rework steps (already planned) | Local proofs shrink from minutes to seconds | M3 build/test share |
| E9 | Shared compile cache across worktrees | Fewer rebuilds per lane | Only if E3 shows builds as the top wait |
| E10 | Owner questions batched into one card set per day | Lanes stop waiting on Erick mid-chunk | M3 owner share |

Round 1 runs E1 and E2 (cheap, reversible). Round 2 runs E3, E4 and E5. Round 3 runs E6 and E7. After that the
order comes from the numbers.

## Rounds so far

The numbers of each round are in `faber-notas/parallel-rounds.jsonl`; `faber-notas/parallel-report.mjs [since]` collects
M3 (CI), M4 and the fixed prompt size from Claude Code's local transcripts and GitHub.

| Round | Date | Changed | Result |
|---|---|---|---|
| 1 | 2026-10-14 | E2 CI in five parallel jobs; sweep spares folders used in the last 6 h | CI 17-28 min to 7.6 min, green; two lanes had lost a warm build to the sweep before the fix |
| 2 | 2026-10-14 | E3 build slots; screen parts dealt by CI time; E4 lean prompt for the owner checkout; E5, E6, E10 handed to the lead session as lane rules | Proofs green (853 tests). First CI run 11.3 min: one slow Windows runner (its part ran 318 s of scenarios against 181 and 205 s, one scenario 63 s instead of 8 s), so the dealing is judged by the median of the next runs. E4 and E5 need a day of sessions |
| 3 | 2026-10-15 | Judged round 2 on a day of sessions; nothing new started | Kept E3 and the CI-time dealing: the last 5 green `phase-1-base` runs took 7.3 min (median), parts 5.6, 6.9 and 6.2 min against lints 7.1 and 5.5; the two slow runs (19 and 15 min) were slow in every job, macOS included, so the runner or a cold cache, not the dealing. E4 gave nothing: new owner-checkout sessions still open at 81-88k tokens (87.7k before), because the turned-off skills were a small part; tool schemas (32k), MCP tools (25k) and skills (10k) are the bulk. Cost per request rose to US$0.071 and the cache-write share to 56% (from 0.06 and 42%), which no round-2 change explains; long lane sessions are the likely cause. The report counted Faber's own test runs (missions, live tests) as sessions and must keep only development folders |

Waiting, and on what:

- E1: the stacked branches merge in the lead session's order once each passes its review (security reviews for #23 and #24).
- E4: judged in round 3 with no gain; the next lever is fewer MCP tools and hook text in the fixed prompt, not skills.
- E5: cost per request and cache-write share went up on 2026-10-15; the lead session checks whether longer lanes cause it.
- E6, E8: every vendor was still ahead of its weekly pace on 2026-10-15 (Claude 56/38, Codex 12/8 after its week
  restarted that day). More lanes would only end the week earlier, so the test system rework (E8) starts when one vendor
  is back under its pace.
- E7: cloud lanes need a cloud environment for this repository, set up by Erick.
- E9: sccache is already in place user-wide; revisit only if build slots show long waits.

## Problems to look for while running

- Two lanes editing the same file or the same locale keys.
- Disk filling (it hit 0 bytes on 2026-10-12 with 11 worktree build folders).
- Heavy builds stealing frames or focus from games.
- A vendor running out before its reset.
- Screen-test flakes blamed on a lane's change ([screen-test-flakes.md](../../screen-test-flakes.md)).
- A model weakening a test or guard to get green; read every diff to tests and guards.
- The GitHub job limit (20 at once on a free public repository) queueing CI.
- The orchestrator's context filling with lane output; lanes report a file and a short summary.
- Stale worktrees and folders left behind; the integrator removes them after each merge.

## Where it ends up: Faber does the same for its users

R128 has two halves. The loop above is the first: how we build Faber. The second is the product: Faber handles every
request a person makes, development or not, as efficiently as possible in quality, cost and speed together, never
trading one away silently.

- Every limit the loop settles on (lanes per PC, build slots, chunk size, vendor split, review timing, fewer longer
  workers, a lean fixed prompt) becomes a default of Faber's own scheduler (R91), so Faber developing itself, and any
  project people build with it, starts from measured numbers instead of guesses.
- The same loop then runs on the product, with the benchmark of about 40 real requests
  ([roadmap](../../roadmap.md#fabers-own-test-system)) as its round: quality (task done and proven, review findings),
  cost (API-equivalent and quota share per request) and speed (time to a proven answer), each against plain Claude
  Code and plain Codex. A Faber version ships only if it is no worse on any axis and better on at least one.
- M3's causes map to the product too: a person's request waits on the same things (workers, reviews, proofs, quota,
  the person), so each experiment above that wins here is a candidate for the product's scheduler.
