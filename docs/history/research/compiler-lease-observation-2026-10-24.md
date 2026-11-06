# Compiler lease observation without filesystem notifications

Status: locally checked and independently reviewed; integration and macOS execution remain pending. No production
behavior changes. This corrects the observation used by one storage-maintenance script test.

## Failure and boundary

The earlier newcomer queue run `40075737439`, head `ada2f2ad`, failed on macOS in
`maintenance claims its compiler temp before spawn and cleans it after a missing compiler`. The expected
`.faber-lease` notification was absent from the `fs.watch` events. The preceding missing-compiler rejection passed.
The log does not distinguish omitted filenames, event coalescing or scheduling; it does not prove an absent lease.

The actual launcher awaits durable `owner.json` publication before returning its lease and calling the compiler.
The test now observes that receipt synchronously at the compiler-spawn boundary and checks its owner, pending child,
target and private temporary directory. A simulated asynchronous ENOENT then exercises the actual maintenance
failure path and its cleanup. No compiler or helper process is started by this test, no global PATH is changed,
and the mocked builtin is restored, including its live ESM binding. Other storage tests remain unchanged.

## Evidence

External evidence: `E:/Documentos/ProjetosIA/faber-evidence/compiler-lease-observation-20261024/`.

- A wrapper that suppresses watcher notifications ran the original actual test: exit 1, expected persisted-lease
  assertion failure (`red-watch-loss.log`, receipt `df11c8`). This reproduces the fragile observation without
  claiming to reproduce macOS event-delivery internals.
- The same wrapper and filter after correction: exit 0, one passed (`green-watch-loss.log`, receipt `6e71f8`).
- A negative control simulates successful receipt publication without persisting the file. The corrected actual
  test fails at its spawn-boundary receipt assertion (`negative-missing-receipt.log`, receipt `77dbea`). The new
  observation therefore still rejects a missing durable owner, rather than merely removing the failing assertion.
- The complete storage script file passes: nineteen passed, zero failed (`whole-cargo-tests.log`, receipt `c3d0ad`).

The old decoded macOS log is retained under `newcomer-integration-40075737439/macos-script-failure.log`, SHA-256
`857608b1943942d9cdf1c504779d1171ac174f55d9d8ab6391de2c37532c7731`. That run independently failed its Windows
visual-memory comparison too; this test-only correction does not fix that separate failure or waive its budget.

A fresh read-only reviewer found no blocker, independently ran all nineteen storage tests, and checked the negative
control and restoration. The seven lightweight repository proofs passed: source checks, TypeScript, documentation,
web build, Rust formatting, script tests and lane claims. The complete script run reports 139 passed and one skipped
(140 total, receipt `cfa80a`); the frontend/documentation receipt is `d142a2`, formatting/claims `5ea926`.
Five existing source warnings and the existing bundle-size warning remain. The independent report is retained
beside the external logs. The final documentation status edit received another documentation check.

## Cost and limits

No shipped app allocation, timer, process, memory or processor behavior changes. The focused test replaces a real
failed compiler spawn and filesystem watcher with one short-lived mock and direct receipt reads. Its full nineteen-
test run took about 1.7 seconds on this Windows PC; that is a test duration, not a product performance measurement.
No macOS execution of the corrected test has been observed yet. A queued cross-platform result remains required;
passing Windows checks alone does not establish an integrated stable version.
