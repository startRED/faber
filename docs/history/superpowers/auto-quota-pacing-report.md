# Auto quota pacing implementation report

Tasks 8 through 12 are implemented; Task 7 was folded into Task 8. R89 denies unknown billing in Auto. R90 was applied to the
spec, plan and code in Task 9 (`5e09d23`). Own API-equivalent USD is persisted separately from shared account percentages,
and user turns, routers, pulses and hand-over summaries all count. Unknown or partial counters leave an explicit lower
bound. The account shape forecasts the whole account; model weights and plan-size learning exclude foreign stretches.

## Review disposition

All twelve findings in the independent review were confirmed and corrected; none were rejected. The suggested removal
of learned weights was unnecessary: only fully metered, temporally explained stretches calibrate them under R90.

| Finding | Correction |
| --- | --- |
| 1 | `5e09d23`, `d47ba93`: covered-time and metered-cost checks; foreign-use regression; idle movement is flagged separately from its numerical estimate. |
| 2 | `2bae704`: persist paid-fallback evidence; unknown remains possible. |
| 3 | `c3731b7`: detect inherited billing overrides by variable names; missing native quota evidence leaves billing unknown. |
| 4 | `bf104f5`: use only the blocking Codex windows for recovery. |
| 5 | `9921c29`: ignore and prune expired windows; choose a live primary window. |
| 6 | `b64c1fa`: proportion movement over the actual two-hour overlap. |
| 7 | `261dcdd`: preserve the gradient above the daily line; correct the spec. |
| 8 | Task 9: meter both vendors and auxiliary requests; follow-up `734fdb0` prices individual long-context Codex requests without charging duplicate or malformed recoveries twice. |
| 9 | `06bac29`: compare capacity only over common models. |
| 10 | Task 8: wire account watching and the local clock before enforcing the gate. |
| 11 | Task 8 pulse timeout; `85eb8df` independent partial account replies and conservative missing scopes; Task 10 admission recovery; Task 11 subscription-only plan questions; Task 12 probe baseline/reset/overage checks. |
| 12 | Temporal learning tests and missing expired-window, blocking-window, restart and overnight regressions. |

Task 10's described test assertions have executable bodies. Move/wait tests cover scoped paid consent, partial history,
pinned families, a single retry, preserved source state on failure, stopping and shutdown, and guarded resumption.
Screen checks cover paced reason order, account bars versus own money, scoped consent, draft attachments, and accepted
sends whose later snapshot fails. Resumed quota requests cannot leave a restored duplicate draft in the composer.

## Validation and limits

Every commit was preceded by the seven required proofs and followed by plain `git push`. Screen/shell work used
`cargo t --all`; screen tests stayed on the runner's hidden desktop with fake CLIs. No real CLI ran in tests.
The testkit now resolves its files relative to `CARGO_TARGET_DIR`; `target/faber-test` is no longer a junction.
Owner commits including `3d3d502`, `ca66b60` and `39b22b5` were preserved. Protected or unrelated paths were not staged.

The monthly simulation results are recorded in `docs/design.md`. The full 200-seed acceptance and four-seed smoke passed;
the manual probe's three event-only tests passed. Task 12's final seven proofs passed, including
`cargo t --all`: 606 passed, zero failed. The full monthly acceptance was run separately because it is ignored by default. They are synthetic evidence,
not a guarantee about a live shared account. Task 11 CI finished successfully on Windows and macOS (`87751e0`).

The live cache probe and real CLI pulse memory peak remain owner-run measurements. The probe now excludes its baseline,
checks the unchanged reset and paid-use evidence, and reports account movement as an upper bound on pulse usage;
its own API-equivalent USD is separate. `docs/roadmap.md` was not staged, as explicitly required. No actual-account cache
numbers or pulse memory result is claimed. Non-English locales still need native-speaker review before release.

## New locale keys

English is the source; German, Spanish, French and Brazilian Portuguese were written by hand. All five locales include
these 42 new keys:

- `error.billingUnknown`
- `error.billedPerUse`
- `error.quotaOut`
- `error.quotaWait`
- `error.moveFailed`
- `reason.waiting`
- `reason.moved.full`
- `reason.moved.summary`
- `reason.pace.step`
- `reason.pace.low`
- `run.waiting`
- `quota.credits.offer`
- `quota.credits.confirm`
- `quota.credits.send`
- `quota.chosen.out`
- `quota.chosen.move`
- `quota.chosen.wait`
- `usage.plan`
- `usage.planUnknown`
- `usage.planAsk`
- `usage.planSave`
- `usage.state.ok`
- `usage.state.spare`
- `usage.state.low`
- `usage.state.out`
- `usage.state.unknown`
- `usage.learning`
- `usage.relearning`
- `usage.outside`
- `usage.overageOn`
- `usage.billedPerUse`
- `usage.pace`
- `usage.expect`
- `usage.authNeeded`
- `usage.ownSpend`
- `usage.shared`
- `usage.foreign`
- `usage.meterIncomplete`
- `answer.meterIncomplete`
- `quota.seven_day_opus`
- `quota.seven_day_sonnet`
- `quota.seven_day_haiku`
