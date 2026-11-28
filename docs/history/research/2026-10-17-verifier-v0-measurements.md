# Verifier V0: measurements before building (2026-10-17)

Step V0 of the [verifier spec](../superpowers/specs/2026-10-17-built-in-verifier.md#6-steps): measure the speed items
before any product code. PC: Erick's, 16 logical cores, Windows 11 Pro 26200, every drive NTFS (no Dev Drive), Defender
real-time protection on; its exclusions are unreadable without administrator rights. Every build ran at below-normal
priority with 8 jobs, as `cargo t` runs on a person's PC.

## Result

- **CI's time goes to waiting.** Queued jobs waited 3,133 minutes for a runner and ran for 1,854; a queued run takes
  18 minutes at the median and 33 at p90. Taking the suites off GitHub (V2) is the gain; faster tools are not.
- **On the PC the checks are already fast.** The type check takes 0.55 s and the lint 0.11 s once `npm`/`npx` are out
  of the way; a Rust check of a one-file engine change takes 7 s, and its test build 18 to 28 s.
- **Three speed items were done before V0:** `tsgo` (TypeScript 7.0.2 is the native compiler), `line-tables-only`
  debug info and the `rust-lld` linker.
- **Defender costs nothing inside `target/`,** so the Dev Drive is a no-go until measured differently. sccache saves a
  quarter of a cold build only while it still has the entries; its 10 GiB cap is full.
- **A local rust run is wrong in every desktop-app worktree:** 22 tests fail on a path with `.claude` in it.

| Item | Verdict |
| :--- | :--- |
| `tsgo` | Done before V0 |
| Start `tsc.exe` and oxlint directly, not through `npm`/`npx` | Go (V1) |
| `debug = "line-tables-only"`, `rust-lld` | Done before V0 |
| sccache cap from 10 to 30 GiB, then measure hits across lanes | Go |
| Dev Drive with Defender's performance mode | No-go for now (Erick's setting; steps below) |
| Test folders outside the worktree | Go, before V1 trusts a local run |
| Suites off GitHub, screen parts and Windows Rust first | Go (V2) |

## TypeScript and the lint

`typescript` 7.0.2 is already the native compiler, so the `tsgo` item was done before V0. Warm runs, three each:

| Command | Time |
| :--- | :--- |
| `tsc.exe --noEmit` (the native binary) | 0.55 s |
| `npm run typecheck` | 1.1 s |
| `npx tsc --noEmit` | 1.5 s |
| oxlint through `node` (`ui tests/ui`) | 0.11 s |
| `npx oxlint ui tests/ui` | 1.0 s |
| `npx vite build` | 2.4 to 2.9 s |
| `node -e 0` | 0.07 s |

The checks themselves take half a second; `npm` and `npx` add 0.5 to 1 s of their own. The lint does not depend on the
compiler (oxlint has its own parser), so the compiler change could not break it.

**Go:** the verifier starts `tsc.exe` and oxlint directly, never through `npm` or `npx`. **No-go:** nothing else to
change here.

## Rust

`[profile.dev]` already has `debug = "line-tables-only"` (no debug info for dependencies) and `rust-lld` links, so both
items were done before V0. Builds of `cargo test --workspace --no-run -j 8` into a fresh folder of the main checkout's
`target/`:

| Build | Time |
| :--- | :--- |
| Cold, sccache off | 175 s |
| Cold, sccache on but its entries evicted (18 hits, 367 misses) | 181 s |
| Cold, sccache filled by the build before (328 hits, 47 misses) | 128 s |
| Nothing changed | 1.4 s |
| One file touched in `engine` (rebuilds the engine, the shell and the tests that use it) | 18 to 28 s |
| One file touched in `src-tauri` | 6 s |
| One file touched in `testkit` | 7 s |
| `cargo check --workspace --all-targets`, cold | 93 s |
| The same check after one file touched in `engine` | 7.3 to 7.6 s |
| The same check with nothing changed | 1.4 s |
| `cargo t --suite rust`, first run of a worktree (build and tests) | 240 s (the runner counts 197 s, 188 s of them in the suite) |
| `cargo t --suite rust` again, nothing changed | 40 s (35 s in the suite) |
| GitHub, `check (windows, rust)` job (median of the window below) | 309 s run, 232 s wait |

- **sccache saves about a quarter of a cold build, when it still has the entries.** It sits at its 10 GiB cap
  (`E:\Apps\rust\sccache`), so with many worktrees it evicts what the next one needs, and the first cold build here got
  nothing from it. Workspace crates never hit: Cargo builds them incrementally, which sccache cannot cache.
- **Defender:** a freshly linked test binary ran in 0.18 s the first time and the same afterwards inside `target/`. A
  copy of it in another folder took 2.4 s the first time and 0.18 s after, so `target/` is excluded or trusted and
  the build pays nothing for Defender.
- **An engine change is the slow case** (18 to 28 s), because the shell and every test program link it again. A check
  of the same change takes 7 s, so the verifier's fast check is `cargo check` plus the tests the change selects.
- **The rust suite fails 22 tests on the PC that pass in CI** (1,646 passed, on the commit CI landed as R190). Those
  tests make their folders in `target/test-workspaces` next to the code, and a worktree under `.claude\worktrees\` puts
  `.claude` in that path, which the engine refuses as a credential folder (`error.workingFolder`). Every lane the
  desktop app creates is there, so a lane cannot trust a local rust run today. V1 must give tests a folder outside the
  worktree.

**Go:** raise sccache's cap to 30 GiB inside the build folder budget (R106) and measure the hit rate across lanes
again; V1 keeps one warm `target/<worktree>` per lane and sends a one-file check through the incremental build.
**No-go:** debug info and the linker (done), and anything against Defender (nothing to win in `target/`).

## CI on GitHub

The last 50 completed runs on `queue/*` branches, 12:27 to 15:14 UTC (GitHub's API; wait = a job's `started_at`
minus its `created_at`, run = `completed_at` minus `started_at`; 835 jobs ran, 114 were skipped). 137 other runs (105
pull request lints) shared the runners in the same window.

| | Median | p90 |
| :--- | :--- | :--- |
| Queued run, first job created to last job done | 18.4 min | 33.0 min |
| A job's wait for a runner | 125 s | 585 s |
| A job's run | 162 s | 234 s |

- **Jobs waited longer than they ran:** 3,133 minutes waiting against 1,854 running. Windows carries most of it (522
  jobs, median wait 232 s, 2,652 minutes waiting against 1,513 running); macOS waited 118 s and Ubuntu 28 s at the
  median.
- **The screen parts wait the most** (median 311 to 359 s each) and run for less than that; `check (windows, rust)`
  runs the longest (median 309 s).
- **Outcomes:** 5 green, 16 red, 29 cancelled (23 replaced by a newer push to the same queue branch).
- **Only 6 of the 16 red runs were broken code**, each failing `gate` with `check (…, rust)` on Windows and macOS. The
  other 10 failed only in `land`. Of the 12 `land` failures in the window (cancelled runs included), 7 came from a
  change that touches a workflow file, which CI's token may not push; 2 from a queue branch that was already gone; 3
  from a refused push.

**Go:** the verifier's main gain is taking jobs off the shared runners (V2), not making each job faster; the screen
parts and the Windows Rust suite are the first to move. The `land` failures are queue mechanics for V3, not checks.

## Dev Drive

There is none: `E:`, `D:` and `C:` are NTFS, and checking a volume or reading Defender's exclusions needs administrator
rights. A Dev Drive is a system setting, so only Erick creates one. What the build shows above: inside `target/`
Defender adds nothing measurable, while the same binary run once from another folder pays about 2.2 s. A Dev Drive
could still help the parts that are not excluded (Cargo's registry, sccache, `node_modules`), but the cold build is
mostly compile time, so the gain is a guess until measured.

**No-go for now.** Microsoft's "up to 30%" is against NTFS with Defender scanning; against an exclusion it claims 14%,
and `target/` already behaves as excluded. If
Erick wants to try it, the steps are: Settings → System → Storage → Advanced storage settings → Disks & volumes →
Create Dev Drive (at least 50 GB, as a VHD on `E:` is enough), then move `target/`, `CARGO_HOME`, the sccache folder
and the worktrees there, and run this page's builds again.
