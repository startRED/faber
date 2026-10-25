# Kept-server catalog-to-session proof

Status: focused Windows runtime and related existing kit checks passed; source-only independent review found no concrete defect. No new catalog item or production behavior change.

## Gap and scope

Earlier checks separately covered npm fixture installation, checked server selection and driver-specific launch configuration. This check connects those boundaries for the one kept server rather than treating separate successes as one end-to-end result.

`engine/tests/kits.rs` clones the shipped `chrome-devtools` metadata, retaining its identity, pinned revision, npm package/version, entry, arguments and environment. Only the archive's code bytes and corresponding tree digest are replaced by an inert original fixture. The package contains the declared entry, but it is never executed. No browser, actual external server, real CLI or AI model starts.

For each supported vendor the check installs through the existing `Store::install_from`, selects through `kit_for` for the same browser-performance task sent to the fake session, and verifies the selected delivery through `verified_for_launch`. It then runs the actual driver over the existing in-memory fake transport. Claude's captured arguments and Codex's recorded `thread/start` must contain the exact selected command, arguments, environment and enabled state.

The same sequence opens fresh sessions after Off, On and Remove. Off/Remove must exclude the server from both the report and the verified delivery and from the actual outgoing native configuration. The final installation ledger must be empty.

The existing replay helper now accepts the task explicitly and adjusts only the expected input text in its existing fixtures. All three earlier callers continue to pass the original task. The local package helper uses the item's declared entry so this check exercises the kept package layout, while previous fixtures still use `run.js`.

## Review and execution

A fresh read-only reviewer checked the complete test change, supporting APIs and existing callers, and found no concrete problem. A follow-up review confirmed the exact per-vendor transport expectation. The reviewer did not execute tests or approve the production catalog.

The corrected focused Windows check passed: one selected test exercises eight vendor/state combinations; the runner reports two passes including its aggregate, not two independent scenarios. Total runtime was 17.5 seconds, with 11.1 seconds in the Rust step. The retained evidence is `catalog-kept-server-root-retry.log` and `catalog-kept-server-focused-run.log` in `faber-recovery/c77e-20261024`.

The related `kits::` selection passed with 113 reported passes, zero failures and zero skipped/not-run checks in 20.7 seconds. This includes the runner aggregate; the retained `catalog-kept-server-related-run.log` identifies the actual cases. It exercises the replay helper's existing callers as well as the new case. No production-source change was needed.

The seven required local checks passed in the full verification recorded as `1792870161889-full`. The earlier expanded fast verification, `1792869854193-fast`, also passed eleven checks including engine, shell, runner and native-library static analysis. The final saved revision's receipt will identify the checks that cover its exact contents.

The first focused invocation refused the new lane's area label because it was not an existing product-map entry. It did not execute the test. The corrected lane uses the existing `Catalog` entry; no product-map feature or production fix was necessary.

The first actual runtime execution failed in the new assertion: Claude's existing launch configuration includes `type: stdio`, which the expected object omitted. Source tracing confirmed this is the driver's established transport field. The assertion was corrected to require it; no production code changed.

## Limits

This proves a checked handoff to fake sessions. It does not prove the real Chrome DevTools package starts, connects to a browser or performs a useful action, and it is not live CLI capability validation. Existing permission and containment checks remain necessary. Task 995, the complete catalog qualification and the first stable version remain open.

Memory and processor cost: test and research changes only; no production code, shipped catalog data or persistent app process changes. No full-app resource measurement was made by this check.
