# Native macOS resident-memory proof

Status: cross-compiled and independently reviewed; native macOS execution is unverified.

`winproc/tests/macos_memory.rs` exercises the existing production process-identity
and resident-memory reader against its own test process. It reads a positive
baseline, allocates 64 MiB, touches each native page and requires at least a
32 MiB resident increase. The allocation remains live through the second reading.
It also requires rejection of a changed creation identity and a nonexistent PID.
No child process, model request, credentials, UI or persistent background work is
involved. The test temporarily allocates 64 MiB and touches one byte per page;
production memory and processor costs are unchanged.

## Local evidence

- `node scripts/cargo.mjs check -p faber-winproc --test macos_memory --target aarch64-apple-darwin`
  exited 0 on Windows. This checks the macOS code, not native behavior or linking.
- A fresh read-only `gpt-6.1-sol` review found no concrete issue in page sizing,
  allocation lifetime, identity refusals or automatic test discovery.
- `testkit/src/runner.rs` includes `--workspace --lib --bins --tests` in its Rust
  suite; `.github/workflows/ci.yml` runs that suite on `macos-latest`. No runner or
  workflow change is needed for this integration target.

The native test must run on a Mac before claiming that the native sampler works.
This proof does not establish complete Unix admission behavior, pressure/release
behavior with real workers, or the full acceptance of task 1000. The earlier
Windows real-worker resource receipt and Linux native sampler proof remain
separate evidence, with their original limitations.
