# B2: Faber's Own Test Runner, in Rust: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Node test suite (`node --test`, Playwright) with one Rust program, `faber-test` (run as `cargo t`), that runs only the affected tests, drives a hidden test build of Faber, keeps every test sandboxed, and ports every current test one to one.

**Architecture:** A new workspace crate `testkit/` (package `faber-testkit`, `publish = false`) holds the runner binary `faber-test`, the fake CLI binary `fake-cli` and a library of small modules (`plan`, `replay`, `fake`, `proctree`, `sandbox`, `server`, `cdp`, `page`, `app`, `report`, `testbuild`, `checks`, `screen`, `runner`). Three suites: `rust` (`cargo test --workspace` as a child), `ui` (TypeScript checks in `tests/ui/` run inside a hidden test build, which reports through a `test-hooks` shell command, with no DevTools port, on every OS), and `screen` (Rust scenarios driving the hidden test build through WebView2's DevTools port, Windows only). One build of the test app per run: Vite `--mode test` into `dist-test/`, then the Tauri CLI with `--features test-hooks` and an override config into `target/faber-test/`, both started as `node <script>` and never through a shell.

**Tech Stack:** Rust 2024 (workspace lint `unsafe_code = "forbid"`), crates `serde_json`, `sha2`, `base64`, `tempfile`, `zip`, `tungstenite`, `sysinfo`, `ctrlc`, and under `cfg(windows)` `wmi` and `winreg`; TypeScript for the screen checks; Vite, `tsc`, `oxlint` and the Tauri CLI stay as development tools.

**Spec:** `docs/superpowers/specs/2026-10-11-faber-redesign-design.md`, sections B0 (commit 3 of the commit order), B2, B12 (items 1 and 2), A19 (rule 1 holds for the fake CLI, which implements no login; rule 5, kill the whole process tree, is what the runner's cleanup does; rules 6 and 9 do not apply here: the runner never calls a real CLI) and the Round 1 and Round 2 lines of the review log about B2. Executors read the spec's B2 section first.

**This plan ends in exactly one commit** (Task 18). Tasks 1 to 17 leave the working tree uncommitted. Each task says which proof commands the tree must already pass when the task ends; the full list, including `cargo t --all`, passes only at Task 18.

**The plan was written against a working copy.** Every line of code in it was compiled and run on Windows 11 in a scratch copy of the repository: `cargo t --all` ran the `rust`, `ui` and `screen` suites (187 result lines, all passing), and a failure case of each suite was shown to fail. The non-Windows branches (`#[cfg(not(windows))]`: the `sysinfo` process times, the `TMPDIR` and `XDG_*` variables, the skipped `screen` suite) were not compiled or run on this PC; their first run is the CI gate described in the spec. A Windows executor must not "fix" them.

---

## Deviations from spec

The spec was checked against the code. Where it is wrong, silent or can be done better, this plan follows the code. Each line says what the plan does and why.

1. **Process identity on Windows is finer than a second.** The spec's launcher and audit use `sysinfo` on every OS. `sysinfo` reports process start times in whole seconds, and Windows reuses process ids, so a recycled id can look like the recorded process. On Windows the plan reads the creation date from WMI (`Win32_Process.CreationDate`, microseconds) through the `wmi` crate; macOS and Linux keep `sysinfo` (whole seconds, scaled to microseconds). This follows the spec change announced during planning; `testkit/src/proctree.rs` has the tests, including two ids reused within one second. The `wmi` crate is therefore used by the `ui` suite's audit on Windows too, not only by the memory scenarios; it stays under `[target.'cfg(windows)'.dependencies]`.
2. **The `dist/` guard fingerprints files, not the folder.** The spec records the modification time of `dist/`. A folder's time does not change when a file inside is rewritten in place, so the plan hashes every file's relative path, size and modification time (`testbuild::fingerprint`) and fails the run if the hash changed.
3. **The fake CLI is built by the runner.** `cargo run --bin faber-test` builds only that binary, but every launch needs `fake-cli.exe` beside it. `testbuild::ensure_fake_cli` runs `cargo build --package faber-testkit --bin fake-cli` first. The spec is silent.
4. **A `winreg` dependency (Windows only) reads the real user `PATH`.** The spec requires "the real user `PATH` unchanged" but names no way to read it without a shell or `unsafe`. `winreg` (a safe wrapper) is declared under `cfg(windows)`.
5. **A `ctrlc` dependency makes "kills the process tree even after Ctrl+C" true.** Rust cannot catch Ctrl+C without a handler, and `unsafe_code = "forbid"` rules out writing one. `ctrlc` is a normal crate; its handler calls `app::emergency_cleanup`, which ends only the recorded processes and removes the sandbox folders.
6. **The `rust` suite prints a line per test, but without a per-test time.** Stable libtest does not report test times (`--report-time` is nightly). The suite's own line carries the total time. `ui` and `screen` lines carry per-test times.
7. **`--filter` on the `ui` suite filters what is reported.** All checks still run, because they cost milliseconds; only the matching ones are printed and counted. A filter that matches nothing in any suite fails the run (exit 1) so that a typo is never mistaken for a pass. The spec does not say.
8. **R5.5 lives in the `rust` suite, not in `screen/setup.rs`.** It needs no app, and the spec itself says it is portable and runs on every OS. It is split into `sandbox::tests::r5_5_the_audit_rejects_writes_outside_the_sandbox` (the file audit) and `app::tests::r5_5_the_scenario_audit_rejects_writes_outside_the_sandbox` (the whole-scenario audit). "Existing recordings still replay unchanged" is `fake::tests::existing_recordings_still_replay_unchanged`, as the spec says.
9. **`Cockpit` stays a literal in `screen/app.rs`.** It is a theme name, not a translation key, in today's appearance screen. B3 rewrites that scenario in its own commit, as the spec says.
10. **`git status` is run with `-c core.quotepath=false -uall`.** Without them, git escapes non-ASCII names (`"caf\303\251.md"`) and lists an untracked folder as one entry, so a new file under `docs/` or `testkit/` could pick the wrong suites. The spec only says `git status --porcelain`.
11. **The Vite build passes `--outDir ../dist-test --emptyOutDir`.** `vite.config.ts` has `root: 'ui'` and `outDir: '../dist'`; the command-line flag overrides it without touching the config. The Tauri build passes Cargo's `--target-dir` as an absolute path, because Cargo reads a relative one from `src-tauri/`.
12. **`.cargo/config.toml` alias:** `t = "run --quiet --package faber-testkit --bin faber-test --"`. `t` replaces Cargo's built-in short name for `test`; this was checked.
13. **The Linux CI step installs `xvfb` and runs `xvfb-run -a cargo t --all`; macOS and Windows run `cargo t --all` directly.** The spec says this; the YAML is spelled out in Task 17. The CI run is a gate for merging the branch, not a condition for finishing B2 (B2 and B12 item 1).
14. **`docs/roadmap.md` changes by one sentence.** Its "Faber's own test system" section says the suite is still Node's built-in runner until the Rust program lands. That is untrue once this commit lands, and the spec's list of changed files does not name the roadmap. Task 17 changes only that sentence.
15. **One more proof command: `cargo clippy -p faber --features test-hooks -- -D warnings`.** The default clippy run never compiles `report_checks` or the checks page, so a lint in them would go unseen. It is added to `AGENTS.md`, `README.md` and the CI job, next to the spec's list.
16. **Two things only a real run showed, now in code and tests.** (a) WebView2's DevTools server keeps the HTTP connection open after it answers `GET /json/list`, even with `Connection: close`, so `cdp` reads the body by its `Content-Length` (a client that reads to the end of the stream times out every time; the mock server in the tests keeps its connection open for the same reason). (b) A profile folder that the webview touched holds `INetCache\Content.IE5`, a link Windows does not let the user list; `std::fs::remove_dir_all` stops there and leaves about 140 files per launch, so `sandbox::remove_tree` removes a link as a folder. Task 14, Step 7 checks that no `faber-test-*` folder is left in the system temp folder.

---

## Global Constraints

Every task's requirements include this section. Values are copied from the spec and from `AGENTS.md`.

- Write code, comments, docs and commit messages in English. Original code only; never copy from TL-ADE, TL-Control or TL-Orchestrator.
- Never read, copy, store or forward CLI credentials or session tokens (`~/.claude`, `~/.codex`, Gemini OAuth). No test may touch the real home folder.
- Child processes never go through a shell. The runner starts `node` directly on `node_modules/vite/bin/vite.js` and `node_modules/@tauri-apps/cli/tauri.js`, never through `npm`, a `.cmd` shim or `cmd.exe`.
- Unit tests never spawn processes, touch real git or call a real CLI. (Reading the process list and creating temporary folders is allowed.)
- Complete test isolation for every launch of the app: temporary `HOME` and `USERPROFILE`, isolated `CODEX_HOME`, `PATH` set to an empty sandbox folder, inherited provider variables and proxies removed, explicit CLI discovery (`FABER_CLI_CLAUDE` and `FABER_CLI_CODEX` are a fake config or the empty string; absent only in the install scenarios), no real CLI, no network (dead proxy, `FABER_SETUP_SOURCE` a loopback server or a dead loopback address), no real git, no write outside the sandbox.
- Remove from the child environment every variable whose name starts (any case) with `FABER_`, `CLAUDE`, `CODEX`, `OPENAI`, `ANTHROPIC`, `GEMINI`, `GOOGLE_API`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, and the Windows spelling `Path`.
- The `ui` suite always sets both `FABER_CLI_CLAUDE` and `FABER_CLI_CODEX` to the empty string. Install scenarios are `screen` scenarios and leave `FABER_CLI_CODEX` absent.
- Do not drop cross-platform coverage: the `rust` and `ui` suites run on Windows, macOS and Linux (Linux under `xvfb-run -a`); only `screen` is Windows only. Everything under `testkit/` compiles on every OS; Windows-only code is behind `#[cfg(windows)]` and the `wmi` and `winreg` crates are under `[target.'cfg(windows)'.dependencies]`.
- `unsafe_code = "forbid"` (workspace lint). No FFI, no `unsafe`.
- Memory: the hidden app with no CLI running stays at or under 250 MB (private working set, app and webview together). The `ui` suite measures no memory.
- Telemetry stays off.
- Every text on screen lives in `ui/locales/`. Screen scenarios take texts from `window.__faberText`, never from sentences written in Rust.
- Files use LF line endings (the repository is LF), with three exceptions that keep the CRLF the repository stores: `AGENTS.md`, `docs/design.md` and `docs/roadmap.md` (`git ls-files --eol` says `i/crlf w/crlf`). Rust: edition 2024, `rustfmt.toml` has `max_width = 140` and `use_small_heuristics = "Max"`; run `cargo fmt --all` before each verify.
- Do not use vendor names in product, feature or package names: the crate is `faber-testkit`, the binaries `faber-test` and `fake-cli`.
- The testkit has no `tests/` integration-test folder. On Windows the running `faber-test.exe` is locked, and `cargo test --workspace` would try to rebuild it for such a folder.
- On the maintainer's PC export these before any `cargo` or `npm run`:
  `export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"`

## Review Focus

Failure modes the spec implies but a straight port would not test. Each has a test in the task named at the end of the line.

1. **Windows environment names are case-insensitive.** A parent environment that holds `path`, `Temp` or `userprofile` must not leave a second spelling beside the sandbox value; a duplicate would let a real folder win. Test in Task 4 (`sandbox`).
2. **`--filter` that matches nothing.** A typo in `--filter` must fail the run, never print an empty summary and exit 0. Test in Task 14 (`runner`).
3. **File names git escapes.** `git status` quotes non-ASCII names and lists untracked folders as one entry; both must still map to the right suites, and a document with an accent in its name must map to none. Tests in Task 1 (`plan`), and the hand check in Task 14, Step 8.
4. **A process id reused within the same second.** The leak check and the kill must not take a new process for the one that was recorded, or end a program the user is running. Tests in Task 3 (`proctree`) and Task 8 (`app`).
5. **The run is interrupted (Ctrl+C), a scenario panics, or the webview leaves a folder Windows will not list.** The apps it started are ended and the temporary folders are really removed. Tests in Task 8 (`app`: `an_interrupted_run_removes_its_sandbox_folders`, and the `Drop` of `Ctx`), Task 4 (`sandbox`: `remove_tree_removes_nested_folders_files_and_a_read_only_file`) and the real-run check in Task 14, Step 7.

---

## File structure

```text
.cargo/config.toml                    NEW   the `cargo t` alias
Cargo.toml                            EDIT  workspace member "testkit"
Cargo.lock                            EDIT  (by Cargo)
src-tauri/Cargo.toml                  EDIT  feature `test-hooks`
src-tauri/src/main.rs                 EDIT  `report_checks` and the checks page, both under `test-hooks`
src-tauri/tauri.test.conf.json        NEW   test-only override of the Tauri config
ui/main.tsx                           EDIT  loads tests/ui/index.ts in test mode only
tests/ui/check.ts                     NEW   check registry and assertions
tests/ui/index.ts                     NEW   window.__faberChecks, window.__faberText, ?checks reporting
tests/ui/{i18n,models,setup}.check.ts NEW   the ported Node tests
testkit/Cargo.toml                    NEW
testkit/src/lib.rs                    NEW   module list
testkit/src/plan.rs                   NEW   which suites a change needs; arguments; half the cores
testkit/src/replay.rs                 NEW   replay of a recorded conversation
testkit/src/fake.rs                   NEW   the fake CLI (plain recordings and setup recordings)
testkit/src/proctree.rs               NEW   process identity, the app's tree, kill, memory (Windows)
testkit/src/sandbox.rs                NEW   sandbox folders, child environment, file audit
testkit/src/server.rs                 NEW   loopback release host for the install scenarios
testkit/src/cdp.rs                    NEW   DevTools client
testkit/src/locator.js                NEW   in-page element finder
testkit/src/page.rs                   NEW   what scenarios do to a page
testkit/src/app.rs                    NEW   launching the app, its processes, the audit
testkit/src/report.rs                 NEW   output lines, summary, exit code
testkit/src/testbuild.rs              NEW   building the test app
testkit/src/checks.rs                 NEW   the `ui` suite
testkit/src/screen/{mod,app,setup}.rs NEW   the `screen` suite
testkit/src/runner.rs                 NEW   `faber-test`
testkit/src/bin/{faber-test,fake-cli}.rs NEW thin binaries
package.json, package-lock.json       EDIT  scripts, playwright-core
.gitignore                            EDIT  dist-test/
.github/workflows/ci.yml              EDIT
AGENTS.md, README.md, docs/design.md  EDIT
tests/*.test.ts, tests/e2e/, tests/support/, tests/fixtures/setup/launcher.cs   DELETE
```

Before Task 1, once: run `npm ci` and `npm run build` from the repository root (with the Rust paths from Global Constraints exported). The Tauri crate embeds `dist/` at compile time, so every `cargo clippy --workspace` and `cargo test --workspace` below fails with "frontendDist ... doesn't exist" until `dist/` exists. From Task 9 on, the runner's own guard fails the run if `dist/` changes while it builds the test app.

How each test-first Rust task works: the module's tests sit in a `#[cfg(test)] mod tests` block at the end of the file. Step 1 creates the file with only that block (and the `pub mod` line), Step 2 runs it and sees it fail to compile (the code under test is missing), Step 3 adds the implementation above the block (with one blank line between the two), Step 4 runs it green. Do not change a test to make it pass. Run `cargo fmt --all` after Step 3; it may reflow lines, and that is fine.

---

### Task 1: Crate skeleton, the `cargo t` alias and the `plan` module

**Model: sonnet**

**Files:**
- Create: `testkit/Cargo.toml`, `testkit/src/lib.rs`, `testkit/src/plan.rs`, `testkit/src/bin/faber-test.rs`, `testkit/src/bin/fake-cli.rs`, `.cargo/config.toml`
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Consumes: nothing.
- Produces (`faber_testkit::plan`): `enum Suite { Rust, Ui, Screen }` with `Suite::ALL: [Suite; 3]`, `fn name(self) -> &'static str`, `fn parse(&str) -> Option<Suite>`; `fn half_cores(logical: usize) -> usize`; `fn jobs() -> usize`; `fn suites_for(paths: &[String]) -> BTreeSet<Suite>`; `fn changed_paths(porcelain: &str) -> Vec<String>`; `struct Options { all: bool, suite: Option<Suite>, filter: Option<String> }`; `fn parse_args(args: &[String]) -> Result<Options, String>`; `fn select(opts: &Options, changed: &[String]) -> BTreeSet<Suite>`.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`, `cargo t` (exit 0, prints nothing).

- [ ] **Step 1: Add the crate to the workspace and write the manifest**

In the root `Cargo.toml` change the members line to `members = ["engine", "src-tauri", "testkit"]`. Create `testkit/Cargo.toml` (its dependency list is complete for all the tasks that follow; unused dependencies are not an error):

```toml
[package]
name = "faber-testkit"
description = "Faber's own test runner, and the fake CLI it replays recordings with."
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[[bin]]
name = "faber-test"
path = "src/bin/faber-test.rs"

[[bin]]
name = "fake-cli"
path = "src/bin/fake-cli.rs"

[dependencies]
base64 = "0.22"
ctrlc = "3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
sysinfo = { version = "0.39.6", default-features = false, features = ["system"] }
tempfile = "3"
tungstenite = { version = "0.30", default-features = false, features = ["handshake"] }
zip = { version = "4", default-features = false, features = ["deflate"] }

[target.'cfg(windows)'.dependencies]
winreg = "0.56"
wmi = "0.18"

[dev-dependencies]
toml = "0.8"

[lints]
workspace = true
```

Create `.cargo/config.toml`. The alias `t` replaces Cargo's built-in short name for `test`; that is intended:

```toml
[alias]
# `cargo t` runs Faber's own test runner (testkit/). Without arguments it runs what the changed files need.
t = "run --quiet --package faber-testkit --bin faber-test --"
```

Create two stub binaries so the manifest resolves. Task 2 and Task 14 replace them:

`testkit/src/bin/fake-cli.rs` and `testkit/src/bin/faber-test.rs`, each containing exactly:

```rust
fn main() {}
```

Create `testkit/src/lib.rs`:

```rust
//! Faber's own test runner, and the fake CLI it replays recordings with.

pub mod plan;
```

- [ ] **Step 2: Write the failing tests**

Create `testkit/src/plan.rs` containing only this block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }
    fn names(set: &BTreeSet<Suite>) -> Vec<&'static str> {
        set.iter().map(|s| s.name()).collect()
    }

    #[test]
    fn half_the_cores_and_never_fewer_than_one() {
        assert_eq!(half_cores(16), 8);
        assert_eq!(half_cores(5), 2);
        assert_eq!(half_cores(2), 1);
        assert_eq!(half_cores(1), 1);
        assert_eq!(half_cores(0), 1);
    }

    #[test]
    fn each_changed_path_picks_the_suites_of_the_spec_table() {
        let cases: [(&str, &[&str]); 16] = [
            ("engine/src/store.rs", &["rust", "screen"]),
            ("src-tauri/src/main.rs", &["rust", "ui", "screen"]),
            ("Cargo.toml", &["rust", "ui", "screen"]),
            ("Cargo.lock", &["rust", "ui", "screen"]),
            (".cargo/config.toml", &["rust", "ui", "screen"]),
            ("ui/App.tsx", &["ui", "screen"]),
            ("vite.config.ts", &["ui", "screen"]),
            ("tsconfig.json", &["ui", "screen"]),
            ("package.json", &["ui", "screen"]),
            ("package-lock.json", &["ui", "screen"]),
            ("tests/ui/i18n.check.ts", &["ui"]),
            ("tests/fixtures/claude/chat.jsonl", &["rust", "screen"]),
            ("testkit/src/plan.rs", &["rust", "ui", "screen"]),
            (".github/workflows/ci.yml", &["rust", "ui", "screen"]),
            ("rustfmt.toml", &["rust", "ui", "screen"]),
            ("tests/other/notes.txt", &["rust", "ui", "screen"]),
        ];
        for (path, want) in cases {
            assert_eq!(names(&suites_for(&paths(&[path]))), want, "{path}");
        }
    }

    #[test]
    fn documents_pick_nothing_at_any_depth_and_nothing_means_no_suites() {
        for path in ["docs/design.md", "docs/superpowers/plans/x.md", "README.md", "engine/NOTES.md", "ui/locales/README.md"] {
            assert!(suites_for(&paths(&[path])).is_empty(), "{path}");
        }
        assert!(suites_for(&[]).is_empty());
    }

    #[test]
    fn several_changes_add_their_suites_together() {
        assert_eq!(names(&suites_for(&paths(&["tests/ui/a.check.ts", "engine/src/x.rs", "README.md"]))), ["rust", "ui", "screen"]);
        assert_eq!(names(&suites_for(&paths(&["tests/ui/a.check.ts", "README.md"]))), ["ui"]);
    }

    #[test]
    fn the_porcelain_output_gives_paths_including_both_names_of_a_rename() {
        let out = " M engine/src/store.rs\n?? testkit/src/plan.rs\nR  old/name.rs -> new/name.rs\nA  \"tests/ui/a b.check.ts\"\n";
        assert_eq!(
            changed_paths(out),
            ["engine/src/store.rs", "testkit/src/plan.rs", "old/name.rs", "new/name.rs", "tests/ui/a b.check.ts"]
        );
        assert!(changed_paths("").is_empty());
    }

    // The runner asks git for `-c core.quotepath=false status --porcelain -uall`: names are not escaped, and every file of
    // an untracked folder is listed, so a new file under docs/ or testkit/ picks its own suites.
    #[test]
    fn accented_names_and_files_of_untracked_folders_pick_their_own_suites() {
        let out = "?? docs/notas-\u{e9}.md\n?? testkit/src/new.rs\n?? testkit/src/screen/extra.rs\n M ui/Caf\u{e9}.tsx\n";
        let found = changed_paths(out);
        assert_eq!(found, ["docs/notas-\u{e9}.md", "testkit/src/new.rs", "testkit/src/screen/extra.rs", "ui/Caf\u{e9}.tsx"]);
        assert_eq!(names(&suites_for(&found[..1])), Vec::<&str>::new(), "a document with an accent in its name is still a document");
        assert_eq!(names(&suites_for(&found[1..3])), ["rust", "ui", "screen"]);
        assert_eq!(names(&suites_for(&found[3..])), ["ui", "screen"]);
    }

    #[test]
    fn arguments_pick_all_one_suite_or_a_filter_and_refuse_anything_else() {
        let args = |list: &[&str]| parse_args(&paths(list));
        assert_eq!(args(&[]).unwrap(), Options::default());
        assert!(args(&["--all"]).unwrap().all);
        assert_eq!(
            args(&["--suite", "ui", "--filter", "i18n"]).unwrap(),
            Options { all: false, suite: Some(Suite::Ui), filter: Some("i18n".into()) }
        );
        assert!(args(&["--suite"]).is_err());
        assert!(args(&["--suite", "gpu"]).is_err());
        assert!(args(&["--filter"]).is_err());
        assert!(args(&["--everything"]).is_err());
    }

    #[test]
    fn suite_beats_all_and_all_beats_the_changes() {
        let changed = paths(&["README.md"]);
        assert!(select(&Options::default(), &changed).is_empty());
        assert_eq!(select(&Options { all: true, ..Options::default() }, &changed).len(), 3);
        let one = Options { all: true, suite: Some(Suite::Screen), filter: None };
        assert_eq!(names(&select(&one, &changed)), ["screen"]);
    }
}
```

- [ ] **Step 3: Run them and see them fail**

Run: `cargo test -p faber-testkit plan::`
Expected: compile errors such as `cannot find function half_cores in this scope`.

- [ ] **Step 4: Write the implementation**

Put this at the top of `testkit/src/plan.rs`, above the test block:

```rust
//! What a run covers: which suites a change needs, what the command line asked for, and how many cores to use.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suite {
    Rust,
    Ui,
    Screen,
}

impl Suite {
    pub const ALL: [Suite; 3] = [Suite::Rust, Suite::Ui, Suite::Screen];

    pub fn name(self) -> &'static str {
        match self {
            Suite::Rust => "rust",
            Suite::Ui => "ui",
            Suite::Screen => "screen",
        }
    }

    pub fn parse(name: &str) -> Option<Suite> {
        Suite::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// Half the logical cores, never fewer than one, so the PC stays usable while the tests run.
pub fn half_cores(logical: usize) -> usize {
    (logical / 2).max(1)
}

pub fn jobs() -> usize {
    half_cores(std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// The suites that cover a set of changed paths (relative to the repository, `/` separators).
pub fn suites_for(paths: &[String]) -> BTreeSet<Suite> {
    paths.iter().flat_map(|p| suites_for_path(p)).collect()
}

fn suites_for_path(path: &str) -> Vec<Suite> {
    use Suite::{Rust, Screen, Ui};
    let path = path.trim_start_matches("./");
    let inside = |dirs: &[&str]| dirs.iter().any(|d| path.starts_with(d));
    let is = |files: &[&str]| files.contains(&path);
    if path.starts_with("docs/") || path.ends_with(".md") {
        vec![]
    } else if inside(&["testkit/"]) {
        Suite::ALL.to_vec()
    } else if inside(&["tests/ui/"]) {
        vec![Ui]
    } else if inside(&["tests/fixtures/", "engine/"]) {
        vec![Rust, Screen]
    } else if inside(&["src-tauri/", ".cargo/"]) || is(&["Cargo.toml", "Cargo.lock"]) {
        vec![Rust, Ui, Screen]
    } else if inside(&["ui/"]) || is(&["vite.config.ts", "tsconfig.json", "package.json", "package-lock.json"]) {
        vec![Ui, Screen]
    } else {
        Suite::ALL.to_vec()
    }
}

/// The paths in the output of `git status --porcelain -uall`; a rename lists both names.
pub fn changed_paths(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter_map(|line| line.get(3..))
        .flat_map(|rest| rest.split(" -> "))
        .map(|p| p.trim().trim_matches('"').replace('\\', "/"))
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub all: bool,
    pub suite: Option<Suite>,
    pub filter: Option<String>,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--all" => opts.all = true,
            "--suite" => {
                let name = it.next().ok_or("--suite needs a name: rust, ui or screen")?;
                opts.suite = Some(Suite::parse(name).ok_or_else(|| format!("unknown suite {name}: use rust, ui or screen"))?);
            }
            "--filter" => opts.filter = Some(it.next().ok_or("--filter needs some text")?.clone()),
            other => return Err(format!("unknown argument {other}: use --all, --suite <name> or --filter <text>")),
        }
    }
    Ok(opts)
}

/// `--suite` wins, then `--all`, then whatever the changed paths ask for.
pub fn select(opts: &Options, changed: &[String]) -> BTreeSet<Suite> {
    match (opts.suite, opts.all) {
        (Some(suite), _) => BTreeSet::from([suite]),
        (None, true) => Suite::ALL.into_iter().collect(),
        (None, false) => suites_for(changed),
    }
}
```

- [ ] **Step 5: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit plan:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 8 tests pass (`plan::tests::...`), clippy prints no warning. The first run downloads crates and builds the Tauri crates; that takes a few minutes.

- [ ] **Step 6: Check the alias**

Run: `cargo t`
Expected: exit 0 and no output (the stub does nothing). If Cargo says `no such command`, `.cargo/config.toml` is in the wrong folder: it belongs at the repository root.

---

### Task 2: `replay`, `fake` and the `fake-cli` binary

**Model: sonnet**

**Files:**
- Create: `testkit/src/replay.rs`, `testkit/src/fake.rs`
- Modify: `testkit/src/lib.rs` (add `pub mod fake;` and `pub mod replay;`), `testkit/src/bin/fake-cli.rs` (replace the stub)

**Interfaces:**
- Consumes: the recordings under `tests/fixtures/{claude,codex,setup}/` (read only). The Node originals, `tests/support/fake-cli.ts` and `tests/support/replay.ts`, stay in place until Task 17; read them if a behaviour is unclear.
- Produces (`faber_testkit::replay`): `struct Entry { input: bool, msg: serde_json::Value }`, `fn load(&Path) -> io::Result<Vec<Entry>>`, `fn key(&Value) -> String`, `struct Replay { pub mismatches: Vec<String> }` with `Replay::new(Vec<Entry>)` and `fn feed(&mut self, line: &str) -> io::Result<Vec<String>>`.
- Produces (`faber_testkit::fake`): `struct Setup { vendor, version, chat, models: Option<String> }` (a `*.setup.json` recording), `struct Env { recording: PathBuf, state_dir: Option<PathBuf> }`, `struct Dispatch`, and `fn run(env: &Env, args: &[String], stdin: &mut dyn BufRead, stdout: &mut dyn Write, stderr: &mut dyn Write) -> io::Result<i32>`.
- The binary `fake-cli` reads `FABER_FAKE_RECORDING` and `FABER_FAKE_STATE_DIR`, calls `fake::run` and exits with its code. A `*.jsonl` recording is replayed as is; a `*.setup.json` recording also answers `--version`, the sign-in status and the sign-in itself, keeps only a marker file `<vendor>.signed-in` (never a credential) and appends every command to `<state>/commands.jsonl` (`{pid, cwd, vendor, args, targets}` and `{exited: pid}`). It never implements `account/login/*` and never stores `auth.json` (A19 rule 1). The vendor comes from the recording, not from the file name, because no scenario runs two vendors at once.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod fake;` and `pub mod replay;` to `testkit/src/lib.rs` (alphabetical). Create `testkit/src/replay.rs` and `testkit/src/fake.rs`, each containing only its test block:

`testkit/src/replay.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(input: bool, msg: Value) -> Entry {
        Entry { input, msg }
    }

    #[test]
    fn the_key_ignores_what_may_vary_and_keeps_what_must_match() {
        assert_eq!(
            key(&json!({"type": "user", "message": {"content": "hi"}, "session_id": "a"})),
            key(&json!({"type": "user", "message": {"content": "hi"}, "session_id": "b"}))
        );
        assert_ne!(
            key(&json!({"type": "user", "message": {"content": "hi"}})),
            key(&json!({"type": "user", "message": {"content": "bye"}}))
        );
        assert_eq!(key(&json!({"method": "initialized"})), "initialized");
        assert_eq!(key(&json!({"method": "thread/start", "id": 2, "params": {"cwd": "x"}})), "thread/start 2");
        assert_eq!(key(&json!({"id": 7, "result": {"decision": "accept"}})), "answer 7 accept");
        assert_eq!(
            key(&json!({"type": "control_response", "response": {"request_id": "r1", "response": {"behavior": "allow"}}})),
            "control_response r1 allow"
        );
    }

    #[test]
    fn the_outputs_after_a_matching_input_come_back_and_a_wrong_input_is_named() {
        let mut replay = Replay::new(vec![
            entry(true, json!({"type": "user", "message": {"content": "hi"}})),
            entry(false, json!({"type": "a"})),
            entry(false, json!({"type": "b"})),
            entry(true, json!({"type": "user", "message": {"content": "again"}})),
            entry(false, json!({"type": "c"})),
        ]);
        assert_eq!(replay.feed(r#"{"type":"user","message":{"content":"hi"}}"#).unwrap(), [r#"{"type":"a"}"#, r#"{"type":"b"}"#]);
        assert!(replay.mismatches.is_empty());
        assert!(replay.feed(r#"{"type":"user","message":{"content":"other"}}"#).unwrap().is_empty());
        assert_eq!(replay.mismatches.len(), 1);
        assert!(replay.mismatches[0].contains("sent user \"other\""), "{:?}", replay.mismatches);
        assert_eq!(replay.feed(r#"{"type":"user","message":{"content":"again"}}"#).unwrap(), [r#"{"type":"c"}"#]);
        assert!(replay.feed("not json").is_err());
    }
}
```

`testkit/src/fake.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
    }

    fn drive(recording: PathBuf, state_dir: Option<PathBuf>, args: &[&str], input: &str) -> (io::Result<i32>, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let result = run(&Env { recording, state_dir }, &args, &mut input.as_bytes(), &mut out, &mut err);
        (result, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    fn records(state: &Path) -> Vec<Value> {
        std::fs::read_to_string(state.join("commands.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    // Exercise every existing recording in memory: setup-command support must not change the replay protocol.
    #[test]
    fn existing_recordings_still_replay_unchanged() {
        for vendor in ["claude", "codex"] {
            let dir = fixtures().join(vendor);
            let mut found = 0;
            for file in std::fs::read_dir(&dir).unwrap().map(|f| f.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
            {
                let entries = load(&file).unwrap();
                let mut replay = Replay::new(entries.clone());
                let mut output = Vec::new();
                for entry in entries.iter().filter(|e| e.input) {
                    output.extend(replay.feed(&entry.msg.to_string()).unwrap());
                }
                let recorded: Vec<Value> = entries.iter().filter(|e| !e.input).map(|e| e.msg.clone()).collect();
                let replayed: Vec<Value> = output.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
                assert!(replay.mismatches.is_empty(), "{}: {:?}", file.display(), replay.mismatches);
                assert_eq!(replayed, recorded, "{}", file.display());
                found += 1;
            }
            assert!(found >= 5, "{vendor}: only {found} recordings found in {}", dir.display());
        }
    }

    #[test]
    fn a_plain_recording_replays_over_stdin_and_stdout_and_reports_a_difference_without_failing() {
        let chat = fixtures().join("claude/chat.jsonl");
        let first = load(&chat).unwrap().into_iter().find(|e| e.input).unwrap().msg.to_string();
        let (code, out, err) = drive(chat.clone(), None, &["-p", "--ignored"], &format!("{first}\n"));
        assert_eq!(code.unwrap(), 0);
        assert!(out.contains("Rayleigh"), "{out}");
        assert!(err.is_empty(), "{err}");
        let (code, out, err) = drive(chat, None, &[], "{\"type\":\"user\",\"message\":{\"content\":\"something else\"}}\n");
        assert_eq!(code.unwrap(), 0);
        assert!(out.is_empty());
        assert!(err.contains("sent user \"something else\""), "{err}");
    }

    #[test]
    fn claude_setup_commands_report_signed_out_then_signed_in_and_record_what_they_may_write() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/claude.setup.json");
        let go = |args: &[&str]| drive(setup.clone(), Some(state.path().to_path_buf()), args, "");
        let (code, out, _) = go(&["--version"]);
        assert_eq!((code.unwrap(), out.trim()), (0, "2.1.296"));
        let (code, out, _) = go(&["auth", "status"]);
        assert_eq!((code.unwrap(), out.trim()), (1, r#"{"loggedIn":false}"#));
        let (code, out, _) = go(&["auth", "login"]);
        assert_eq!((code.unwrap(), out.trim()), (0, "Fake sign-in complete"));
        let (code, out, _) = go(&["auth", "status"]);
        assert_eq!((code.unwrap(), out.trim()), (0, r#"{"loggedIn":true}"#));
        let log = records(state.path());
        let started: Vec<&Value> = log.iter().filter(|r| r["exited"].is_null()).collect();
        assert_eq!(started.len(), 4);
        assert_eq!(started[2]["args"], json!(["auth", "login"]));
        assert_eq!(started[2]["targets"].as_array().unwrap().len(), 2, "a login may write the marker as well as the list");
        assert_eq!(started[1]["targets"].as_array().unwrap().len(), 1);
        assert_eq!(log.iter().filter(|r| r["exited"] == json!(std::process::id())).count(), 4, "every run records its exit");
    }

    #[test]
    fn codex_setup_status_speaks_on_stderr() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/codex.setup.json");
        let (code, out, err) = drive(setup.clone(), Some(state.path().to_path_buf()), &["login", "status"], "");
        assert_eq!((code.unwrap(), out.as_str(), err.trim()), (1, "", "Not logged in"));
        drive(setup.clone(), Some(state.path().to_path_buf()), &["login"], "").0.unwrap();
        let (code, _, err) = drive(setup, Some(state.path().to_path_buf()), &["login", "status"], "");
        assert_eq!((code.unwrap(), err.trim()), (0, "Logged in using ChatGPT"));
    }

    #[test]
    fn a_codex_setup_run_answers_the_handshake_and_picks_the_recording_from_the_first_request() {
        let setup = fixtures().join("setup/codex.setup.json");
        let handshake = r#"{"id":1,"method":"initialize","params":{}}"#.to_string() + "\n" + r#"{"method":"initialized"}"# + "\n";
        let state = tempfile::tempdir().unwrap();
        let (code, out, err) = drive(
            setup.clone(),
            Some(state.path().to_path_buf()),
            &[],
            &format!("{handshake}{}\n", r#"{"id":2,"method":"model/list","params":{}}"#),
        );
        assert_eq!(code.unwrap(), 0, "{err}");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].contains("faber-setup-replay"), "{out}");
        assert!(lines[1].contains("gpt-6.1-sol"), "{out}");
        let (code, out, err) = drive(
            setup,
            Some(state.path().to_path_buf()),
            &["app-server"],
            &format!("{handshake}{}\n", r#"{"id":2,"method":"thread/start","params":{}}"#),
        );
        assert_eq!(code.unwrap(), 0, "{err}");
        assert!(out.lines().nth(1).is_some_and(|l| l.contains("remoteControl")), "{out}");
    }

    #[test]
    fn a_setup_run_exits_1_on_a_difference_and_refuses_to_start_without_a_state_folder() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/claude.setup.json");
        let (code, _, err) =
            drive(setup.clone(), Some(state.path().to_path_buf()), &[], "{\"type\":\"user\",\"message\":{\"content\":\"unexpected\"}}\n");
        assert_eq!(code.unwrap(), 1);
        assert!(err.contains("sent user \"unexpected\""), "{err}");
        let (code, _, _) = drive(setup, None, &["--version"], "");
        assert!(code.unwrap_err().to_string().contains("FABER_FAKE_STATE_DIR"));
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit replay:: fake::`
Expected: compile errors (`cannot find ... in this scope`).

- [ ] **Step 3: Write the implementation**

`testkit/src/replay.rs`, above its test block:

```rust
//! A recorded session replayed line by line: what the app writes is checked against the next recorded input, and the
//! recorded outputs that follow it are sent back. The same rule as the engine's own tests (`engine/tests/support`).

use serde_json::Value;
use std::io;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Entry {
    pub input: bool,
    pub msg: Value,
}

/// Reads a `*.jsonl` recording: one `{"dir": "in" | "out", "msg": ...}` object per line.
pub fn load(path: &Path) -> io::Result<Vec<Entry>> {
    let text = std::fs::read_to_string(path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).map_err(|e| io::Error::other(format!("{}: {e}", path.display())))?;
            Ok(Entry { input: v["dir"] == "in", msg: v["msg"].clone() })
        })
        .collect()
}

/// What must match between what the app sends and what was recorded.
pub fn key(msg: &Value) -> String {
    let method = msg["method"].as_str();
    let id = &msg["id"];
    // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
    if method == Some("turn/start") {
        return format!("turn/start {} {}", id, msg["params"]["input"]);
    }
    if let Some(m) = method {
        return if id.is_null() { m.to_string() } else { format!("{m} {id}") };
    }
    if !id.is_null() {
        return format!("answer {} {}", id, msg["result"]["decision"].as_str().unwrap_or(""));
    }
    match msg["type"].as_str().unwrap_or_default() {
        "user" => format!("user {}", msg["message"]["content"]),
        "control_request" => format!("control_request {}", msg["request"]["subtype"].as_str().unwrap_or("")),
        "control_response" => {
            let r = &msg["response"];
            format!("control_response {} {}", r["request_id"].as_str().unwrap_or(""), r["response"]["behavior"].as_str().unwrap_or("ok"))
        }
        other => other.to_string(),
    }
}

pub struct Replay {
    entries: Vec<Entry>,
    at: usize,
    pub mismatches: Vec<String>,
}

impl Replay {
    pub fn new(entries: Vec<Entry>) -> Replay {
        Replay { entries, at: 0, mismatches: Vec::new() }
    }

    /// One line the app wrote. Returns the lines to send back: the recorded outputs up to the next recorded input.
    pub fn feed(&mut self, line: &str) -> io::Result<Vec<String>> {
        let msg: Value = serde_json::from_str(line).map_err(|e| io::Error::other(format!("not JSON ({e}): {line}")))?;
        let sent = key(&msg);
        let expected = match self.entries.get(self.at) {
            Some(entry) if entry.input => key(&entry.msg),
            _ => "(no input expected)".to_string(),
        };
        if sent == expected {
            self.at += 1;
        } else {
            self.mismatches.push(format!("sent {sent}, recorded {expected}"));
        }
        let mut out = Vec::new();
        while let Some(entry) = self.entries.get(self.at).filter(|e| !e.input) {
            out.push(entry.msg.to_string());
            self.at += 1;
        }
        Ok(out)
    }
}
```

`testkit/src/fake.rs`, above its test block:

```rust
//! The fake CLI: stands in for a CLI binary and replays a recorded session over its own stdin and stdout.
//!
//! A `*.setup.json` recording also describes the commands the setup guide runs on a CLI (`--version`, the sign-in status
//! and the sign-in itself). Their state is only a marker file, never a credential, and each scenario gets its own folder.

use crate::replay::{Entry, Replay, load};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Setup {
    pub vendor: String,
    pub version: String,
    /// The conversation to replay, relative to the setup file.
    pub chat: String,
    /// The recording that answers `model/list`, relative to the setup file.
    pub models: Option<String>,
}

pub struct Env {
    /// `FABER_FAKE_RECORDING`: a `*.jsonl` recording, or a `*.setup.json` file.
    pub recording: PathBuf,
    /// `FABER_FAKE_STATE_DIR`: where a setup recording keeps its marker and its list of commands.
    pub state_dir: Option<PathBuf>,
}

/// Decides which recording a line belongs to. A short model enumeration and a chat use separate Codex app-server
/// processes, so a Codex setup run answers the handshake itself and picks the recording from the first real request.
pub struct Dispatch {
    entries: Vec<Entry>,
    models: Option<PathBuf>,
    replay: Replay,
    selected: bool,
}

impl Dispatch {
    pub fn new(entries: Vec<Entry>, models: Option<PathBuf>, wait_for_first_request: bool) -> Dispatch {
        Dispatch { replay: Replay::new(entries.clone()), entries, models, selected: !wait_for_first_request }
    }

    pub fn mismatches(&self) -> &[String] {
        &self.replay.mismatches
    }

    pub fn feed(&mut self, line: &str) -> io::Result<Vec<String>> {
        if self.selected {
            return self.replay.feed(line);
        }
        let request: Value = serde_json::from_str(line).map_err(|e| io::Error::other(format!("not JSON ({e}): {line}")))?;
        match request["method"].as_str() {
            Some("initialize") => {
                let result = json!({ "userAgent": "faber-setup-replay", "platformFamily": "windows", "platformOs": "windows" });
                return Ok(vec![json!({ "id": request["id"], "result": result }).to_string()]);
            }
            Some("initialized") => return Ok(vec![]),
            Some("model/list") => {
                if let Some(path) = &self.models {
                    self.entries = load(path)?;
                }
            }
            _ => {}
        }
        // The handshake was answered above; the recording starts after its own.
        let start = self.entries.iter().position(|e| e.input && e.msg["method"] == "initialized").map_or(0, |i| i + 1);
        self.replay = Replay::new(self.entries[start..].to_vec());
        self.selected = true;
        self.replay.feed(line)
    }
}

pub fn run(env: &Env, args: &[String], stdin: &mut dyn BufRead, stdout: &mut dyn Write, stderr: &mut dyn Write) -> io::Result<i32> {
    if !env.recording.to_string_lossy().ends_with(".setup.json") {
        return session(Dispatch::new(load(&env.recording)?, None, false), false, stdin, stdout, stderr);
    }
    let setup: Setup = serde_json::from_str(&std::fs::read_to_string(&env.recording)?).map_err(io::Error::other)?;
    let state = env.state_dir.as_deref().ok_or_else(|| io::Error::other("Setup replay requires an isolated FABER_FAKE_STATE_DIR"))?;
    let code = setup_run(&setup, env, state, args, stdin, stdout, stderr);
    // Windows reuses process ids: once this one is gone, its id must not count as a leaked CLI.
    append(state, &json!({ "exited": std::process::id() }))?;
    code
}

fn setup_run(
    setup: &Setup,
    env: &Env,
    state: &Path,
    args: &[String],
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<i32> {
    let claude = setup.vendor == "claude";
    let marker = state.join(format!("{}.signed-in", setup.vendor));
    let audit = state.join("commands.jsonl");
    let status: &[&str] = if claude { &["auth", "status"] } else { &["login", "status"] };
    let login: &[&str] = if claude { &["auth", "login"] } else { &["login"] };
    let is = |expected: &[&str]| args.iter().map(String::as_str).eq(expected.iter().copied());
    // What this command may write, so the runner can check it stayed inside the sandbox.
    let mut targets = vec![audit.to_string_lossy().into_owned()];
    if is(login) {
        targets.push(marker.to_string_lossy().into_owned());
    }
    let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    append(state, &json!({ "pid": std::process::id(), "cwd": cwd, "vendor": setup.vendor, "args": args, "targets": targets }))?;
    if args.first().map(String::as_str) == Some("--version") {
        writeln!(stdout, "{}", setup.version)?;
        return Ok(0);
    }
    if is(status) {
        let connected = marker.exists();
        if claude {
            writeln!(stdout, "{}", json!({ "loggedIn": connected }))?;
        } else {
            writeln!(stderr, "{}", if connected { "Logged in using ChatGPT" } else { "Not logged in" })?;
        }
        return Ok(if connected { 0 } else { 1 });
    }
    if is(login) {
        std::fs::write(&marker, "true")?;
        writeln!(stdout, "Fake sign-in complete")?;
        return Ok(0);
    }
    let dir = env.recording.parent().unwrap_or(Path::new("."));
    let models = setup.models.as_ref().map(|m| dir.join(m));
    session(Dispatch::new(load(&dir.join(&setup.chat))?, models, !claude), true, stdin, stdout, stderr)
}

/// Replays until stdin ends. A setup run exits 1 on a difference; a plain run only reports it on stderr.
fn session(
    mut dispatch: Dispatch,
    strict: bool,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<i32> {
    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        for out in dispatch.feed(&line)? {
            writeln!(stdout, "{out}")?;
        }
        stdout.flush()?;
    }
    let mismatches = dispatch.mismatches();
    if !mismatches.is_empty() {
        writeln!(stderr, "{}", mismatches.join("\n"))?;
    }
    Ok(if strict && !mismatches.is_empty() { 1 } else { 0 })
}

fn append(state: &Path, record: &Value) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(state.join("commands.jsonl"))?;
    writeln!(file, "{record}")
}
```

Replace `testkit/src/bin/fake-cli.rs`:

```rust
//! Stands in for a CLI binary: replays the recording named by FABER_FAKE_RECORDING (see `faber_testkit::fake`).

use faber_testkit::fake::{Env, run};
use std::io;

fn main() {
    let Some(recording) = std::env::var_os("FABER_FAKE_RECORDING") else {
        eprintln!("fake-cli: set FABER_FAKE_RECORDING to a recording under tests/fixtures");
        std::process::exit(2);
    };
    let env = Env { recording: recording.into(), state_dir: std::env::var_os("FABER_FAKE_STATE_DIR").map(Into::into) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&env, &args, &mut io::stdin().lock(), &mut io::stdout().lock(), &mut io::stderr().lock()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fake-cli: {e}");
            1
        }
    };
    std::process::exit(code);
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all pass, including `fake::tests::existing_recordings_still_replay_unchanged`, which replays every `*.jsonl` under `tests/fixtures/claude` and `tests/fixtures/codex` and must find no difference. If it fails on a recording, the port is wrong (compare with `tests/support/replay.ts`); never edit a recording.

- [ ] **Step 5: Smoke-test the binary by hand**

Run: `cargo run --quiet -p faber-testkit --bin fake-cli`
Expected: exit code 2 and one line on stderr: `fake-cli: set FABER_FAKE_RECORDING to a recording under tests/fixtures`.

---

### Task 3: `proctree`: process identity, the app's tree, kill, memory

**Model: sonnet**

**Files:**
- Create: `testkit/src/proctree.rs`
- Modify: `testkit/src/lib.rs` (`pub mod proctree;`)

**Interfaces:**
- Consumes: the `sysinfo` crate on every OS, the `wmi` crate on Windows (declared in Task 1).
- Produces (`faber_testkit::proctree`): `struct ProcRow { pid: u32, parent: u32, name: String, born: u64 }` where `born` is the creation time in microseconds since 1970 (Windows: WMI `Win32_Process.CreationDate`, finer than a second; macOS and Linux: `sysinfo` whole seconds times 1 000 000); `fn descendants(rows: &[ProcRow], pid: u32) -> Vec<ProcRow>` (a child counts only if it was born at or after its parent, the rule from commit 93e9040; pure); `fn same_process(row: &ProcRow, seen: &[ProcRow]) -> bool` (same id, same creation time and same name; on macOS and Linux the time is a whole second, so the name is the second witness against a reused id); `fn snapshot() -> Result<Vec<ProcRow>, String>`; `fn kill_recorded(seen: &[ProcRow]) -> Result<(), String>` (ends only processes that `same_process` matches to a recorded one); `fn private_mb(root: u32) -> Result<f64, String>` (Windows: private working set of the tree from `Win32_PerfRawData_PerfProc_Process`, the numbers Task Manager shows; other systems return an error).
- Why microseconds: Windows reuses process ids, and it can hand one back within the same second. Identifying a process by id and a whole-second start time would let a new program be taken for the recorded one (and be killed). `an_id_reused_within_the_same_second_is_not_the_recorded_process` and `a_parent_id_reused_within_the_same_second_does_not_pull_in_an_older_process` pin this (the first also checks a 5 ms difference, the finest the system gives), `an_equal_id_and_second_with_a_different_name_is_a_different_process` pins the macOS and Linux case, and on Windows `on_windows_creation_times_are_finer_than_a_second` checks the real process list.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod proctree;` to `testkit/src/lib.rs`. Create `testkit/src/proctree.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, parent: u32, born: u64) -> ProcRow {
        ProcRow { pid, parent, name: format!("p{pid}"), born }
    }
    fn pids(rows: &[ProcRow]) -> Vec<u32> {
        let mut pids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
        pids.sort();
        pids
    }
    // 2026-10-11 12:00:00 UTC, in microseconds.
    const SECOND: u64 = 1_791_720_000_000_000;

    #[test]
    fn the_tree_follows_children_born_after_their_parent_or_at_the_same_moment() {
        let rows = [row(10, 1, 100), row(11, 10, 110), row(12, 11, 110), row(13, 10, 130), row(99, 1, 50)];
        assert_eq!(pids(&descendants(&rows, 10)), [10, 11, 12, 13]);
    }

    // The system reuses process ids: csrss.exe still names the smss.exe that started it, though that id now belongs to a
    // process started later, so csrss.exe is not a child of it.
    #[test]
    fn a_process_older_than_the_one_that_now_owns_its_parent_id_is_not_in_the_tree() {
        let rows = [row(500, 4, 1000), row(600, 500, 2000), row(700, 500, 900)];
        assert_eq!(pids(&descendants(&rows, 500)), [500, 600]);
        let twin = [row(10, 1, 100), row(11, 10, 90)];
        assert_eq!(pids(&descendants(&twin, 10)), [10]);
    }

    #[test]
    fn a_root_that_is_gone_still_collects_what_names_it_as_parent() {
        let rows = [row(11, 10, 110), row(12, 11, 120), row(13, 7, 130)];
        assert_eq!(pids(&descendants(&rows, 10)), [11, 12]);
    }

    #[test]
    fn a_recorded_process_is_the_same_id_and_the_same_creation_time() {
        let seen = [row(10, 1, 100)];
        assert!(same_process(&row(10, 1, 100), &seen));
        assert!(!same_process(&row(10, 1, 200), &seen), "the id was reused by a younger process");
        assert!(!same_process(&row(11, 1, 100), &seen));
    }

    // Ids come back quickly on Windows: a process that died and a new one that took its id can start in the same whole
    // second. Only a finer creation time tells them apart, so the kill and the leak check must use it.
    #[test]
    fn an_id_reused_within_the_same_second_is_not_the_recorded_process() {
        let recorded = [row(4242, 100, SECOND + 120_000)];
        let reused = row(4242, 100, SECOND + 870_000);
        assert_eq!(reused.born / 1_000_000, recorded[0].born / 1_000_000, "both were created in the same whole second");
        assert!(!same_process(&reused, &recorded));
        // The finest difference the system gives is 5 ms on Windows; it is enough.
        assert!(!same_process(&row(4242, 100, SECOND + 125_000), &recorded));
        assert!(same_process(&row(4242, 7, SECOND + 120_000), &recorded), "the same process, even if the parent column differs");
    }

    // macOS and Linux times are whole seconds, so an id handed out again in the same second can only be told apart by
    // the program's name.
    #[test]
    fn an_equal_id_and_second_with_a_different_name_is_a_different_process() {
        let recorded = [ProcRow { pid: 31, parent: 1, name: "faber".into(), born: SECOND }];
        assert!(same_process(&ProcRow { pid: 31, parent: 1, name: "faber".into(), born: SECOND }, &recorded));
        assert!(!same_process(&ProcRow { pid: 31, parent: 1, name: "bash".into(), born: SECOND }, &recorded));
    }

    #[test]
    fn a_parent_id_reused_within_the_same_second_does_not_pull_in_an_older_process() {
        // 500 is the app. 800 names 500 as its parent, but was created 40 ms before 500 existed: it belongs to the
        // process that had the id earlier.
        let rows = [row(500, 4, SECOND + 500_000), row(600, 500, SECOND + 510_000), row(800, 500, SECOND + 460_000)];
        assert_eq!(pids(&descendants(&rows, 500)), [500, 600]);
    }

    #[test]
    fn the_snapshot_sees_this_very_test_process_with_its_parent_and_creation_time() {
        let me = snapshot().unwrap().into_iter().find(|r| r.pid == std::process::id()).expect("this process is in the snapshot");
        assert!(me.parent != 0 && me.born > SECOND / 2 && !me.name.is_empty(), "{me:?}");
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_creation_times_are_finer_than_a_second() {
        let rows = snapshot().unwrap();
        let fine = rows.iter().filter(|r| r.born % 1_000_000 != 0).count();
        assert!(fine > rows.len() / 2, "{fine} of {} processes have a creation time with microseconds", rows.len());
    }

    #[cfg(windows)]
    #[test]
    fn the_private_working_set_of_this_process_is_a_real_number() {
        let mb = private_mb(std::process::id()).unwrap();
        assert!(mb > 1.0 && mb < 4096.0, "{mb}");
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit proctree::`
Expected: compile errors (`cannot find ... ProcRow`).

- [ ] **Step 3: Write the implementation**

Above the test block of `testkit/src/proctree.rs`:

```rust
//! The processes of a launched app: who is in its tree, who must be killed, and (on Windows) how much memory they use.
//! The list comes from the `sysinfo` crate, so the same code runs on Windows, macOS and Linux.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// One process, identified by its id *and* the moment it was created. An id alone is not an identity: the system reuses
/// ids, and on Windows it can do so within the same second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcRow {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    /// Creation time in microseconds since 1970. Windows reads it from the system's own creation date, which is finer
    /// than a second. macOS and Linux only report whole seconds, so there it is that many seconds times a million.
    pub born: u64,
}

/// The app and everything it started. A child is born after its parent, or at the same moment: the system reuses process
/// ids, so a process whose parent id now belongs to a younger process (csrss.exe and winlogon.exe outlive the smss.exe
/// that started them) is not part of the tree. A pure function over the list.
pub fn descendants(rows: &[ProcRow], pid: u32) -> Vec<ProcRow> {
    let mut born: std::collections::BTreeMap<u32, u64> =
        std::collections::BTreeMap::from([(pid, rows.iter().find(|r| r.pid == pid).map_or(0, |r| r.born))]);
    let mut changed = true;
    while changed {
        changed = false;
        for row in rows {
            if let Some(&parent) = born.get(&row.parent)
                && !born.contains_key(&row.pid)
                && row.born >= parent
            {
                born.insert(row.pid, row.born);
                changed = true;
            }
        }
    }
    rows.iter().filter(|r| born.contains_key(&r.pid)).cloned().collect()
}

/// Whether `row` is one of the processes recorded earlier: the same id, the same creation time and the same name. On
/// macOS and Linux the creation time is a whole second, so the name is a second witness against a reused id.
pub fn same_process(row: &ProcRow, seen: &[ProcRow]) -> bool {
    seen.iter().any(|old| old.pid == row.pid && old.born == row.born && old.name == row.name)
}

/// Every process on this computer right now. Windows asks WMI (`Win32_Process`), whose creation date has microseconds.
#[cfg(windows)]
pub fn snapshot() -> Result<Vec<ProcRow>, String> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Process", rename_all = "PascalCase")]
    struct Row {
        process_id: u32,
        parent_process_id: u32,
        name: String,
        creation_date: Option<wmi::WMIDateTime>,
    }
    let wmi = wmi::WMIConnection::new().map_err(|e| format!("WMI: {e}"))?;
    let rows = wmi.query::<Row>().map_err(|e| format!("WMI process query: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|r| ProcRow {
            pid: r.process_id,
            parent: r.parent_process_id,
            name: r.name,
            // The idle process has no creation date; 0 sorts before everything, as it should.
            born: r.creation_date.map_or(0, |d| u64::try_from(d.0.timestamp_micros()).unwrap_or(0)),
        })
        .collect())
}

/// Every process on this computer right now, from the `sysinfo` crate (whole-second start times).
#[cfg(not(windows))]
pub fn snapshot() -> Result<Vec<ProcRow>, String> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    Ok(system
        .processes()
        .iter()
        .map(|(pid, p)| ProcRow {
            pid: pid.as_u32(),
            parent: p.parent().map_or(0, Pid::as_u32),
            name: p.name().to_string_lossy().into_owned(),
            born: p.start_time() * 1_000_000,
        })
        .collect())
}

/// Ends exactly these processes, and only if each is still the one that was recorded (same id, same creation time).
/// The list is read just before the kill, so the window in which an id could change hands is a few milliseconds.
pub fn kill_recorded(seen: &[ProcRow]) -> Result<(), String> {
    let doomed: Vec<Pid> = snapshot()?.iter().filter(|r| same_process(r, seen)).map(|r| Pid::from_u32(r.pid)).collect();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&doomed), true, ProcessRefreshKind::nothing());
    for pid in doomed {
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
    Ok(())
}

/// The private working set, in MiB, of the app and its whole tree, as Task Manager counts it. Windows only: the number
/// comes from `Win32_PerfRawData_PerfProc_Process`, and only the memory scenarios of `screen` read it.
#[cfg(windows)]
pub fn private_mb(root: u32) -> Result<f64, String> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_PerfRawData_PerfProc_Process", rename_all = "PascalCase")]
    struct Perf {
        #[serde(rename = "IDProcess")]
        id_process: u32,
        working_set_private: u64,
    }
    let ids: Vec<u32> = descendants(&snapshot()?, root).iter().map(|r| r.pid).collect();
    let wmi = wmi::WMIConnection::new().map_err(|e| format!("WMI: {e}"))?;
    let rows = wmi.query::<Perf>().map_err(|e| format!("WMI memory query: {e}"))?;
    let bytes: u64 = rows.iter().filter(|p| ids.contains(&p.id_process)).map(|p| p.working_set_private).sum();
    Ok(bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(not(windows))]
pub fn private_mb(_root: u32) -> Result<f64, String> {
    Err("private working sets are read through WMI, which only Windows has".into())
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit proctree:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 10 tests pass on Windows (8 elsewhere: the two Windows-only tests are skipped). Nothing here starts a process.

---

### Task 4: `sandbox`: folders, environment, file audit

**Model: sonnet**

**Files:**
- Create: `testkit/src/sandbox.rs`
- Modify: `testkit/src/lib.rs` (`pub mod sandbox;`)

**Interfaces:**
- Consumes: nothing from earlier tasks (uses `sha2`, `tempfile`, `serde_json`).
- Produces (`faber_testkit::sandbox`): `const DEAD_ORIGIN: &str = "http://127.0.0.1:1"`; `enum CliSlot { Off, Fake, Absent }` (Off is the empty string, which turns discovery off; Fake is a launch config for `fake-cli`; Absent leaves the variable unset, for the install scenarios only); `struct Launch { claude: CliSlot, codex: CliSlot, recording: Option<PathBuf>, setup_source: Option<String>, cdp_port: Option<u16>, checks_out: Option<PathBuf> }` (all `Default`); `struct Sandbox` with public folders `root, observed, enclosing, boundary, scenario, sandbox, data, state, profile, path_dir, tmp, webview, release, canary`, `Sandbox::new() -> io::Result<Sandbox>`, `audit_files(&self) -> Result<(), String>` and `dispose(&self)`; `fn child_env(parent: &[(String, String)], sb: &Sandbox, launch: &Launch, fake_cli: &Path) -> BTreeMap<String, String>`; `fn contained(base: &Path, path: &Path) -> bool`; `fn remove_tree(path: &Path) -> io::Result<()>` (removes a folder and all it holds, including a link Windows does not let the user list, `INetCache\Content.IE5`, at which `std::fs::remove_dir_all` stops and leaves about 140 files behind per launch; `Sandbox::dispose` and `app::emergency_cleanup` use it); `fn snapshot(dir: &Path, exclude: Option<&Path>) -> BTreeMap<PathBuf, String>`.
- The sandbox contract (spec B2, "Module `app`"): from the parent's environment remove every variable starting (any case) with `FABER_`, `CLAUDE`, `CODEX`, `OPENAI`, `ANTHROPIC`, `GEMINI`, `GOOGLE_API`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, and `Path`; point `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `CODEX_HOME`, `WEBVIEW2_USER_DATA_FOLDER` and `PATH` (an empty folder) into the sandbox; on macOS and Linux also `TMPDIR` and `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`; keep `DISPLAY`; set `FABER_HIDDEN=1`, `FABER_DATA_DIR`, `FABER_FAKE_STATE_DIR`, `FABER_SETUP_SOURCE` (a dead loopback address unless a scenario gives a server), `FABER_FAKE_RECORDING`, `FABER_CDP_PORT`, `FABER_CHECKS_OUT` when asked, a dead proxy and `NO_PROXY` for loopback only.
- Audit layout: `scenario` holds `sandbox` (everything the app may write) and `outside-sandbox` (a canary folder with one file). `audit_files` fails with `write outside sandbox detected: ...` if the canary changed or any file appeared in the four folders around `scenario` (`boundary`, `enclosing`, `observed`, `root`) outside it. This is the R5.5 guard.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod sandbox;` to `testkit/src/lib.rs`. Create `testkit/src/sandbox.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_child_environment_keeps_nothing_real_and_points_every_profile_variable_into_the_sandbox() {
        let sb = Sandbox::new().unwrap();
        let real = pairs(&[
            ("USERPROFILE", r"C:\Users\real"),
            ("Path", r"C:\Windows;C:\Users\real\.local\bin"),
            ("CODEX_HOME", r"C:\Users\real\.codex"),
            ("ANTHROPIC_API_KEY", "sk-ant-real"),
            ("OPENAI_API_KEY", "sk-real"),
            ("HTTPS_PROXY", "http://real-proxy:8080"),
            ("claude_oauth_token", "lowercase-secret"),
            ("FABER_CLI_CLAUDE", "stale"),
            ("NO_PROXY", "internal.example"),
            ("SystemRoot", r"C:\Windows"),
            ("DISPLAY", ":99"),
        ]);
        let launch = Launch { cdp_port: Some(9333), recording: Some(PathBuf::from("rec.jsonl")), ..Launch::default() };
        let env = child_env(&real, &sb, &launch, Path::new("fake-cli.exe"));
        for secret in ["real", "sk-ant-real", "sk-real", "lowercase-secret", "stale", "internal.example"] {
            assert!(env.values().all(|v| !v.contains(secret)), "{secret} reached the child: {env:?}");
        }
        assert!(!env.contains_key("Path") && !env.contains_key("claude_oauth_token"));
        for name in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
            "CODEX_HOME",
            "WEBVIEW2_USER_DATA_FOLDER",
            "PATH",
            "FABER_DATA_DIR",
        ] {
            assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {} is outside the sandbox", env[name]);
        }
        if cfg!(not(windows)) {
            for name in ["TMPDIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"] {
                assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {} is outside the sandbox", env[name]);
            }
        } else {
            assert!(!env.contains_key("TMPDIR") && !env.contains_key("XDG_CONFIG_HOME"), "those only exist on macOS and Linux");
        }
        assert_eq!(env["PATH"], sb.path_dir.to_string_lossy());
        assert_eq!(std::fs::read_dir(&sb.path_dir).unwrap().count(), 0, "the PATH folder must be empty, so no real CLI is on it");
        assert_eq!((env["SystemRoot"].as_str(), env["DISPLAY"].as_str()), (r"C:\Windows", ":99"));
        assert_eq!((env["FABER_HIDDEN"].as_str(), env["FABER_CDP_PORT"].as_str()), ("1", "9333"));
        assert_eq!(
            (env["HTTP_PROXY"].as_str(), env["HTTPS_PROXY"].as_str(), env["ALL_PROXY"].as_str()),
            (DEAD_ORIGIN, DEAD_ORIGIN, DEAD_ORIGIN)
        );
        assert_eq!(env["NO_PROXY"], "127.0.0.1,localhost");
        assert_eq!(env["FABER_SETUP_SOURCE"], DEAD_ORIGIN, "a scenario with no server must not be able to download from anywhere");
        sb.dispose();
    }

    #[test]
    fn each_cli_slot_is_off_a_fake_or_left_for_discovery() {
        let sb = Sandbox::new().unwrap();
        let go = |claude, codex| child_env(&[], &sb, &Launch { claude, codex, ..Launch::default() }, Path::new("fake-cli.exe"));
        let env = go(CliSlot::Off, CliSlot::Absent);
        assert_eq!(env["FABER_CLI_CLAUDE"], "");
        assert!(!env.contains_key("FABER_CLI_CODEX"), "absent means the engine may find the CLI installed in the data folder");
        let env = go(CliSlot::Fake, CliSlot::Off);
        let config: serde_json::Value = serde_json::from_str(&env["FABER_CLI_CLAUDE"]).unwrap();
        assert_eq!(config, json!({ "command": "fake-cli.exe", "prefix": [] }));
        assert_eq!(env["FABER_CLI_CODEX"], "");
        sb.dispose();
    }

    // Windows spells a variable however it likes (`Path`, `Temp`, `userprofile`) and looks it up without case. A second
    // spelling next to the sandbox value would leave the child free to read the real one.
    #[cfg(windows)]
    #[test]
    fn on_windows_no_other_spelling_of_a_sandboxed_name_survives() {
        let sb = Sandbox::new().unwrap();
        let real =
            pairs(&[("path", r"C:\real\bin"), ("Temp", r"C:\real\temp"), ("userprofile", r"C:\Users\real"), ("Windir", r"C:\Windows")]);
        let env = child_env(&real, &sb, &Launch::default(), Path::new("fake-cli.exe"));
        for name in ["PATH", "TEMP", "USERPROFILE"] {
            let spellings: Vec<&String> = env.keys().filter(|k| k.eq_ignore_ascii_case(name)).collect();
            assert_eq!(spellings, [name], "only the sandbox spelling of {name} may be left");
            assert!(contained(&sb.root, Path::new(&env[name])), "{name} = {}", env[name]);
        }
        assert_eq!(env["Windir"], r"C:\Windows", "a name the sandbox does not own is kept as it was");
        sb.dispose();
    }

    #[test]
    fn contained_judges_the_path_itself() {
        let base = Path::new("/work/sandbox");
        assert!(contained(base, Path::new("/work/sandbox")));
        assert!(contained(base, Path::new("/work/sandbox/data/x.txt")));
        assert!(contained(base, Path::new("/work/sandbox/data/../data/x.txt")));
        assert!(!contained(base, Path::new("/work/sandbox/../other")));
        assert!(!contained(base, Path::new("/work/sandbox2/x")), "a sibling that starts with the same letters is outside");
        assert!(!contained(base, Path::new("/work")));
    }

    #[test]
    fn remove_tree_removes_nested_folders_files_and_a_read_only_file() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("sandbox");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("a/one.txt"), "1").unwrap();
        std::fs::write(root.join("a/b/c/two.txt"), "2").unwrap();
        let locked = root.join("a/b/locked.txt");
        std::fs::write(&locked, "3").unwrap();
        let mut permissions = std::fs::metadata(&locked).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&locked, permissions).unwrap();
        remove_tree(&root).unwrap();
        assert!(!root.exists());
        assert!(remove_tree(&root).is_err(), "a folder that is already gone is reported, and dispose treats that as done");
    }

    // R5.5: the audit rejects a write outside the scenario's temporary folders, whatever the file is called.
    #[test]
    fn r5_5_the_audit_rejects_writes_outside_the_sandbox() {
        let sb = Sandbox::new().unwrap();
        sb.audit_files().unwrap();
        std::fs::write(sb.data.join("settings.json"), "{}").unwrap();
        sb.audit_files().expect("a write inside the sandbox is fine");
        let beyond_observed = sb.root.join("external-write.txt");
        std::fs::write(&beyond_observed, "unauthorized absolute write beyond observed root").unwrap();
        assert!(sb.audit_files().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&beyond_observed).unwrap();
        let named_like_a_test = sb.enclosing.join("unauthorized.test.ts");
        std::fs::write(&named_like_a_test, "unauthorized write named test.ts").unwrap();
        assert!(sb.audit_files().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&named_like_a_test).unwrap();
        std::fs::write(sb.canary.join("untouched.txt"), "changed").unwrap();
        assert!(sb.audit_files().unwrap_err().contains("canary"));
        sb.dispose();
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit sandbox::`
Expected: compile errors (`cannot find ... Sandbox`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! The sandbox every launched app lives in: its folders, the environment it is given and the check that nothing outside
//! it was touched. It keeps the whole contract of the setup test it replaces, for every launch.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Variables the child never inherits (matched on the start of the name, in any case): Faber's own test switches,
/// the CLIs' settings and provider keys, and any proxy.
const REMOVED_PREFIXES: [&str; 11] =
    ["FABER_", "CLAUDE", "CODEX", "OPENAI", "ANTHROPIC", "GEMINI", "GOOGLE_API", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"];

/// A loopback address nothing listens on: a download or a request that slips through goes nowhere.
pub const DEAD_ORIGIN: &str = "http://127.0.0.1:1";

/// What `FABER_CLI_CLAUDE` or `FABER_CLI_CODEX` holds in a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CliSlot {
    /// The empty string: discovery is off, so the CLI does not exist.
    #[default]
    Off,
    /// A launch config for `fake-cli`.
    Fake,
    /// The variable is not set, so the engine looks in the sandbox (the install scenarios).
    Absent,
}

/// What a scenario chooses for a launch; everything else comes from the sandbox.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    pub claude: CliSlot,
    pub codex: CliSlot,
    /// `FABER_FAKE_RECORDING`, for the fake CLIs.
    pub recording: Option<PathBuf>,
    /// `FABER_SETUP_SOURCE`: a loopback server; a dead address when `None`.
    pub setup_source: Option<String>,
    pub cdp_port: Option<u16>,
    /// `FABER_CHECKS_OUT`: the `ui` suite's report file.
    pub checks_out: Option<PathBuf>,
}

pub struct Sandbox {
    /// Four folders above `scenario` are watched for stray files: `root`, `observed`, `enclosing`, `boundary`.
    pub root: PathBuf,
    pub observed: PathBuf,
    pub enclosing: PathBuf,
    pub boundary: PathBuf,
    pub scenario: PathBuf,
    pub sandbox: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub profile: PathBuf,
    pub path_dir: PathBuf,
    pub tmp: PathBuf,
    pub webview: PathBuf,
    pub release: PathBuf,
    /// A folder beside the sandbox that nothing may write to.
    pub canary: PathBuf,
    canary_before: BTreeMap<PathBuf, String>,
}

impl Sandbox {
    pub fn new() -> io::Result<Sandbox> {
        let root = plain(tempfile::Builder::new().prefix("faber-test-").tempdir()?.keep().canonicalize()?);
        let observed = root.join("observed");
        let enclosing = observed.join("enclosing");
        let boundary = enclosing.join("boundary");
        let scenario = boundary.join("scenario");
        let sandbox = scenario.join("sandbox");
        let profile = sandbox.join("profile");
        let sb = Sandbox {
            data: sandbox.join("data"),
            state: sandbox.join("fake-state"),
            path_dir: sandbox.join("path"),
            tmp: sandbox.join("tmp"),
            webview: sandbox.join("webview"),
            release: sandbox.join("release"),
            canary: scenario.join("outside-sandbox"),
            canary_before: BTreeMap::new(),
            root,
            observed,
            enclosing,
            boundary,
            scenario,
            sandbox,
            profile,
        };
        let appdata = [sb.profile.join("AppData/Local"), sb.profile.join("AppData/Roaming")];
        for dir in [&sb.data, &sb.state, &sb.path_dir, &sb.tmp, &sb.release, &sb.canary].into_iter().chain(&appdata) {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(sb.canary.join("untouched.txt"), "No installation or login may write here.")?;
        Ok(Sandbox { canary_before: snapshot(&sb.canary, None), ..sb })
    }

    /// Everything the sandbox is allowed to hold is inside `sandbox`. This checks the rest of the disk we can see:
    /// the canary folder is unchanged and nothing appeared in the four folders around the scenario.
    pub fn audit_files(&self) -> Result<(), String> {
        if snapshot(&self.canary, None) != self.canary_before {
            return Err(format!("write outside sandbox detected: the canary folder {} changed", self.canary.display()));
        }
        let mut stray = Vec::new();
        let mut child = self.scenario.clone();
        for _ in 0..4 {
            let Some(parent) = child.parent().map(Path::to_path_buf) else { break };
            stray.extend(snapshot(&parent, Some(&child)).into_keys().map(|p| p.display().to_string()));
            child = parent;
        }
        if stray.is_empty() { Ok(()) } else { Err(format!("write outside sandbox detected: {}", stray.join(", "))) }
    }

    /// Removes every folder. A webview may hold a file for a moment after it exits, so this retries.
    pub fn dispose(&self) {
        for _ in 0..10 {
            if remove_tree(&self.root).is_ok() || !self.root.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Removes a folder and everything in it. `std::fs::remove_dir_all` gives up on the first folder it may not list, and
/// Windows makes one of those (`INetCache\Content.IE5`, a link the user may not open) in every profile folder the
/// webview touches. It can still be removed as it is, so a folder that cannot be listed is removed without looking
/// inside, and a link is removed as a folder when removing it as a file fails.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    let Ok(entries) = std::fs::read_dir(path) else { return std::fs::remove_dir(path) };
    let mut first_error = None;
    for entry in entries.flatten() {
        let child = entry.path();
        let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_tree(&child)
        } else {
            std::fs::remove_file(&child).or_else(|_| std::fs::remove_dir(&child))
        };
        if let Err(e) = removed {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => std::fs::remove_dir(path),
    }
}

/// `canonicalize` on Windows gives a `\\?\` path, which the webview and many programs reject.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// The environment of a launched app: the parent's, without anything that could reach a real account or the network,
/// with every place the app or a CLI could write pointed inside the sandbox.
pub fn child_env(parent: &[(String, String)], sb: &Sandbox, launch: &Launch, fake_cli: &Path) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = parent
        .iter()
        .filter(|(name, _)| {
            let upper = name.to_ascii_uppercase();
            name != "Path" && !REMOVED_PREFIXES.iter().any(|prefix| upper.starts_with(prefix))
        })
        .cloned()
        .collect();
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut set = |name: &str, value: String| {
        // Windows names are case-insensitive: `Temp` beside `TEMP` would leave it open which one the child sees.
        if cfg!(windows) {
            env.retain(|existing, _| !existing.eq_ignore_ascii_case(name));
        }
        env.insert(name.to_string(), value);
    };
    for name in ["HOME", "USERPROFILE"] {
        set(name, path(&sb.profile));
    }
    set("APPDATA", path(&sb.profile.join("AppData/Roaming")));
    set("LOCALAPPDATA", path(&sb.profile.join("AppData/Local")));
    let mut temp_names = vec!["TEMP", "TMP"];
    if cfg!(not(windows)) {
        // macOS and Linux have more places to write to, and `TMPDIR` is where they put temporary files.
        temp_names.push("TMPDIR");
        for (name, dir) in [
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_DATA_HOME", ".local/share"),
            ("XDG_CACHE_HOME", ".cache"),
            ("XDG_STATE_HOME", ".local/state"),
        ] {
            set(name, path(&sb.profile.join(dir)));
        }
    }
    for name in temp_names {
        set(name, path(&sb.tmp));
    }
    set("PATH", path(&sb.path_dir));
    set("CODEX_HOME", path(&sb.profile.join("fake-home")));
    set("WEBVIEW2_USER_DATA_FOLDER", path(&sb.webview));
    set("FABER_HIDDEN", "1".into());
    set("FABER_DATA_DIR", path(&sb.data));
    set("FABER_FAKE_STATE_DIR", path(&sb.state));
    set("FABER_SETUP_SOURCE", launch.setup_source.clone().unwrap_or_else(|| DEAD_ORIGIN.into()));
    if let Some(port) = launch.cdp_port {
        set("FABER_CDP_PORT", port.to_string());
    }
    if let Some(recording) = &launch.recording {
        set("FABER_FAKE_RECORDING", path(recording));
    }
    if let Some(out) = &launch.checks_out {
        set("FABER_CHECKS_OUT", path(out));
    }
    let slot = |slot: CliSlot| match slot {
        CliSlot::Off => Some(String::new()),
        CliSlot::Fake => Some(json!({ "command": path(fake_cli), "prefix": [] }).to_string()),
        CliSlot::Absent => None,
    };
    for (name, value) in [("FABER_CLI_CLAUDE", slot(launch.claude)), ("FABER_CLI_CODEX", slot(launch.codex))] {
        if let Some(value) = value {
            set(name, value);
        }
    }
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        set(name, DEAD_ORIGIN.into());
    }
    set("NO_PROXY", "127.0.0.1,localhost".into());
    env
}

/// Whether `path` is `base` or inside it, judged by the path itself (`..` included), not by what exists on disk.
pub fn contained(base: &Path, path: &Path) -> bool {
    let (base, path) = (normal(base), normal(path));
    path.len() >= base.len() && path[..base.len()] == base[..]
}

fn normal(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir => {}
            other => {
                let text = other.as_os_str().to_string_lossy().into_owned();
                parts.push(if cfg!(windows) { text.to_lowercase() } else { text });
            }
        }
    }
    parts
}

/// The SHA-256 of every file under `dir`, by path, leaving out the folder `exclude`.
pub fn snapshot(dir: &Path, exclude: Option<&Path>) -> BTreeMap<PathBuf, String> {
    let mut found = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if exclude.is_some_and(|skip| path == skip) {
            continue;
        }
        if path.is_dir() {
            found.extend(snapshot(&path, exclude));
        } else if let Ok(bytes) = std::fs::read(&path) {
            found.insert(path, format!("{:x}", Sha256::digest(bytes)));
        }
    }
    found
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit sandbox:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 5 tests pass (6 on Windows, which adds `on_windows_no_other_spelling_of_a_sandboxed_name_survives`, the Review Focus 1 test; `remove_tree_removes_nested_folders_files_and_a_read_only_file` is the first half of Review Focus 5, the second half is the real-run check in Task 14, Step 7). The tests create and delete folders under the system temp folder and nothing else.


---

### Task 5: `server`: the loopback release host

**Model: haiku**

**Files:**
- Create: `testkit/src/server.rs`
- Modify: `testkit/src/lib.rs` (`pub mod server;`)

**Interfaces:**
- Consumes: nothing (uses `zip`, `sha2`, `serde_json`, `std::net`).
- Produces (`faber_testkit::server`): `const ASSET: &str = "codex-x86_64-pc-windows-msvc.exe.zip"` (the asset name the engine looks for on Windows x86-64, `engine/src/install.rs`); `fn zip_of(name: &str, payload: &[u8]) -> io::Result<Vec<u8>>` (a zip holding one file); `struct Server { pub origin: String }` with `Server::start(archive: Vec<u8>) -> io::Result<Server>` (listens on `127.0.0.1`, any free port; `GET /` and `GET /releases/latest` answer a release description whose single asset points at `<origin>/codex.zip` with a `sha256:` digest; `GET /codex.zip` answers the archive; anything else is a 404 recorded as unexpected), `requests() -> Vec<String>` (every path asked for), `unexpected() -> Vec<String>` (unknown paths, and requests from an address other than `127.0.0.1`), `stop()`. Dropping the server stops it.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod server;` to `testkit/src/lib.rs`. Create `testkit/src/server.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn get(origin: &str, path: &str) -> (String, Vec<u8>) {
        let mut stream = TcpStream::connect(origin.trim_start_matches("http://")).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&raw[..split]).into_owned(), raw[split + 4..].to_vec())
    }

    #[test]
    fn it_serves_the_release_and_the_archive_and_writes_down_anything_else() {
        let archive = zip_of("codex-x86_64-pc-windows-msvc.exe", b"MZ fake program").unwrap();
        let mut server = Server::start(archive.clone()).unwrap();
        let (head, body) = get(&server.origin, "/releases/latest");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        let release: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(release["assets"][0]["name"], ASSET);
        assert_eq!(release["assets"][0]["browser_download_url"], format!("{}/codex.zip", server.origin));
        assert_eq!(release["assets"][0]["digest"], format!("sha256:{:x}", Sha256::digest(&archive)));
        let (head, body) = get(&server.origin, "/codex.zip");
        assert!(head.contains("application/zip"), "{head}");
        assert_eq!(body, archive);
        assert!(server.unexpected().is_empty());
        let (head, _) = get(&server.origin, "/somewhere/else");
        assert!(head.starts_with("HTTP/1.1 404"), "{head}");
        assert_eq!(server.requests(), ["/releases/latest", "/codex.zip", "/somewhere/else"]);
        assert_eq!(server.unexpected(), ["/somewhere/else"]);
        server.stop();
    }

    #[test]
    fn the_archive_holds_the_program_under_the_name_the_release_uses() {
        let archive = zip_of("codex-x86_64-pc-windows-msvc.exe", b"payload").unwrap();
        let mut zip = zip::ZipArchive::new(io::Cursor::new(archive)).unwrap();
        let mut file = zip.by_index(0).unwrap();
        assert_eq!(file.name(), "codex-x86_64-pc-windows-msvc.exe");
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "payload");
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit server::`
Expected: compile errors (`cannot find ... Server`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! A minimal loopback HTTP server that plays the part of the release host for the setup scenarios: the release
//! description and one zipped program. Anything else that is asked for is written down as unexpected.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The asset the engine looks for on Windows x86-64.
pub const ASSET: &str = "codex-x86_64-pc-windows-msvc.exe.zip";

/// A zip holding one file, like the official release asset.
pub fn zip_of(name: &str, payload: &[u8]) -> io::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    zip.start_file(name, zip::write::SimpleFileOptions::default()).map_err(io::Error::other)?;
    zip.write_all(payload)?;
    Ok(zip.finish().map_err(io::Error::other)?.into_inner())
}

#[derive(Default)]
struct Log {
    requests: Vec<String>,
    unexpected: Vec<String>,
}

pub struct Server {
    pub origin: String,
    log: Arc<Mutex<Log>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `archive` (a zip) as the latest release.
    pub fn start(archive: Vec<u8>) -> io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let digest = format!("{:x}", Sha256::digest(&archive));
        let release = json!({
            "tag_name": "setup-replay",
            "assets": [{ "name": ASSET, "browser_download_url": format!("{origin}/codex.zip"), "digest": format!("sha256:{digest}") }],
        })
        .to_string();
        let (log, stop) = (Arc::new(Mutex::new(Log::default())), Arc::new(AtomicBool::new(false)));
        let (thread_log, thread_stop) = (log.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let (archive, release) = (Arc::new(archive), Arc::new(release));
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        let (log, archive, release) = (thread_log.clone(), archive.clone(), release.clone());
                        std::thread::spawn(move || {
                            let _ = answer(stream, peer, &log, &archive, &release);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Server { origin, log, stop, thread: Some(thread) })
    }

    /// Every path asked for, in order.
    pub fn requests(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).requests.clone()
    }

    /// Paths nothing should ask for, and requests that did not come from this computer.
    pub fn unexpected(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).unexpected.clone()
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn answer(mut stream: TcpStream, peer: SocketAddr, log: &Mutex<Log>, archive: &[u8], release: &str) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    // The rest of the request is not needed; read up to the blank line so the client is not cut off mid-send.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    {
        let mut log = log.lock().unwrap_or_else(|e| e.into_inner());
        log.requests.push(path.clone());
        if !peer.ip().is_loopback() {
            log.unexpected.push(format!("{path} from {peer}"));
        }
    }
    let (status, kind, body): (&str, &str, &[u8]) = match path.as_str() {
        "/releases/latest" | "/" => ("200 OK", "application/json", release.as_bytes()),
        "/codex.zip" => ("200 OK", "application/zip", archive),
        _ => {
            log.lock().unwrap_or_else(|e| e.into_inner()).unexpected.push(path);
            ("404 Not Found", "text/plain", b"not found")
        }
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
    stream.write_all(body)?;
    stream.flush()
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit server:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 2 tests pass. They open a socket on `127.0.0.1` only.

---

### Task 6: `cdp`: the DevTools client

**Model: sonnet**

**Files:**
- Create: `testkit/src/cdp.rs`
- Modify: `testkit/src/lib.rs` (`pub mod cdp;`)

**Interfaces:**
- Consumes: the `tungstenite` crate (declared with `default-features = false, features = ["handshake"]`, so no TLS), `base64`, `serde_json`.
- Produces (`faber_testkit::cdp`): `const DEFAULT_WAIT: Duration = 15 s`; `struct Cdp` with `connect(port: u16, timeout: Duration) -> Result<Cdp, String>` (reads `GET /json/list` on `127.0.0.1:<port>` until a page target is listed, then opens its WebSocket, and turns focus emulation on so an offscreen window takes keys), `call(&mut self, method: &str, params: Value) -> Result<Value, String>` (events are skipped, answers matched by id), `eval(&mut self, expression: &str) -> Result<Value, String>` (`Runtime.evaluate` with `returnByValue` and `awaitPromise`; a thrown script is an error), `wait_for(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String>` (polls every 100 ms; on the deadline the error says `timed out after N s waiting for <what>`), `move_to`, `click_at`, `insert_text`, `press(chord)` (`Enter`, `Escape`, `Tab`, a letter, `,`, with `Control+`, `Alt+`, `Shift+`, `Meta+`), `screenshot(&mut self, path: &Path)`.
- `connect` reads the answer of `GET /json/list` by its `Content-Length`, never to the end of the stream: WebView2's server keeps the connection open after it answers, even when the request says `Connection: close` (a first version that read to the end timed out on every launch of the real app). The mock server in the tests keeps its connections open for the same reason.
- The tests start a mock DevTools server on loopback inside the test (a `TcpListener` and `tungstenite::accept`); no app and no browser is needed.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod cdp;` to `testkit/src/lib.rs`. Create `testkit/src/cdp.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A stand-in for the app's DevTools port: the target list over HTTP and a scripted WebSocket.
    fn mock() -> (u16, Arc<Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            // WebView2 does not close the connection after it answers, so neither does this stand-in.
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = stream.peek(&mut head).unwrap();
                if head[..n].starts_with(b"GET /json/list") {
                    let mut stream = stream;
                    let mut sink = [0u8; 1024];
                    let _ = stream.read(&mut sink);
                    let targets = json!([
                        { "type": "other", "url": "x", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/NOPE") },
                        { "type": "page", "url": "http://tauri.localhost/", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/P1") },
                    ])
                    .to_string();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{targets}",
                        targets.len()
                    )
                    .unwrap();
                    held.push(stream);
                    continue;
                }
                let log = log.clone();
                std::thread::spawn(move || {
                    let mut socket = tungstenite::accept(stream).unwrap();
                    while let Ok(Message::Text(text)) = socket.read() {
                        let request: Value = serde_json::from_str(text.as_str()).unwrap();
                        log.lock().unwrap().push(request.clone());
                        // An event before every answer: the client must skip it.
                        socket.send(Message::text(json!({ "method": "Page.frameNavigated", "params": {} }).to_string())).unwrap();
                        let reply = match request["method"].as_str().unwrap() {
                            "Runtime.evaluate" => match request["params"]["expression"].as_str().unwrap() {
                                "boom" => {
                                    json!({ "result": { "type": "object" }, "exceptionDetails": { "text": "Uncaught", "exception": { "description": "Error: boom" } } })
                                }
                                "1 + 1" => json!({ "result": { "type": "number", "value": 2 } }),
                                "late" => json!({ "result": { "type": "boolean", "value": request["id"].as_u64().unwrap() > 6 } }),
                                _ => json!({ "result": { "type": "undefined" } }),
                            },
                            "Page.captureScreenshot" => json!({ "data": "iVBORw0KGgo=" }),
                            _ => json!({}),
                        };
                        let answer = if request["method"] == "Nope.fail" {
                            json!({ "id": request["id"], "error": { "message": "no such method" } })
                        } else {
                            json!({ "id": request["id"], "result": reply })
                        };
                        socket.send(Message::text(answer.to_string())).unwrap();
                    }
                });
            }
        });
        (port, seen)
    }

    #[test]
    fn it_finds_the_page_target_and_matches_each_answer_to_its_call_skipping_events() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        assert_eq!(cdp.eval("1 + 1").unwrap(), json!(2));
        assert_eq!(cdp.eval("nothing").unwrap(), Value::Null);
        assert!(cdp.eval("boom").unwrap_err().contains("Error: boom"));
        assert!(cdp.call("Nope.fail", json!({})).unwrap_err().contains("no such method"));
        let methods: Vec<String> = seen.lock().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect();
        assert_eq!(methods[0], "Emulation.setFocusEmulationEnabled", "an offscreen window must be told it has focus");
        assert!(seen.lock().unwrap()[1]["params"]["awaitPromise"] == json!(true));
    }

    #[test]
    fn it_gives_up_with_a_message_when_the_port_never_lists_a_page() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let error = Cdp::connect(port, Duration::from_millis(300)).err().expect("nothing listens there");
        assert!(error.contains("did not offer a page"), "{error}");
    }

    #[test]
    fn wait_for_polls_until_the_condition_holds_and_fails_with_its_own_message() {
        let (port, _) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.wait_for("the late condition", "late", Duration::from_secs(5)).unwrap();
        let error = cdp.wait_for("the heading to appear", "nothing", Duration::from_millis(250)).unwrap_err();
        assert!(error.contains("waiting for the heading to appear"), "{error}");
    }

    #[test]
    fn click_type_and_key_calls_become_the_protocol_events_a_real_user_would_cause() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.click_at(10.0, 20.0).unwrap();
        cdp.insert_text("hello").unwrap();
        cdp.press("Enter").unwrap();
        cdp.press("Control+,").unwrap();
        let log = seen.lock().unwrap();
        let events: Vec<(&str, &Value)> = log.iter().skip(1).map(|r| (r["method"].as_str().unwrap(), &r["params"])).collect();
        let kinds: Vec<&str> = events
            .iter()
            .map(|(m, p)| if *m == "Input.dispatchMouseEvent" || *m == "Input.dispatchKeyEvent" { p["type"].as_str().unwrap() } else { *m })
            .collect();
        assert_eq!(kinds, ["mouseMoved", "mousePressed", "mouseReleased", "Input.insertText", "keyDown", "keyUp", "keyDown", "keyUp"]);
        assert_eq!(events[1].1["x"], json!(10.0));
        assert_eq!(events[3].1["text"], json!("hello"));
        assert_eq!(events[4].1["text"], json!("\r"), "Enter types a line break, which is what submits a form");
        assert_eq!(
            (events[6].1["key"].clone(), events[6].1["code"].clone(), events[6].1["windowsVirtualKeyCode"].clone()),
            (json!(","), json!("Comma"), json!(188))
        );
        assert_eq!(events[6].1["modifiers"], json!(2));
        assert!(events[6].1.get("text").is_none(), "a chord with Control types nothing");
        drop(log);
        assert!(cdp.press("Hyper+x").is_err());
        assert!(cdp.press("F13").is_err());
    }

    #[test]
    fn a_screenshot_is_decoded_and_saved_where_asked() {
        let (port, _) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shots/home.png");
        cdp.screenshot(&path).unwrap();
        assert_eq!(&std::fs::read(path).unwrap()[1..4], b"PNG");
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit cdp::`
Expected: compile errors (`cannot find ... Cdp`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! A small DevTools client: the page's target list over HTTP, then one WebSocket. It can evaluate a script, wait for a
//! condition, click, type, press keys and take a screenshot. WebView2 (Windows) is the only webview that has the port.

use base64::Engine as _;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

/// How long a wait lasts unless a scenario says otherwise.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(15);

pub struct Cdp {
    socket: WebSocket<TcpStream>,
    next: u64,
}

impl Cdp {
    /// Waits for the app's DevTools port to list a page, then connects to it.
    pub fn connect(port: u16, timeout: Duration) -> Result<Cdp, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let error = match page_url(port).and_then(|url| open(&url)) {
                Ok(mut cdp) => {
                    // An offscreen window is never focused; without this it ignores keys until someone clicks it.
                    let _ = cdp.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true }));
                    return Ok(cdp);
                }
                Err(e) => e,
            };
            if Instant::now() >= deadline {
                return Err(format!("the app's DevTools port {port} did not offer a page ({error})"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// One protocol call. Events that arrive meanwhile are ignored.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let request = json!({ "id": id, "method": method, "params": params }).to_string();
        self.socket.send(Message::text(request)).map_err(|e| format!("DevTools send: {e}"))?;
        loop {
            let message = self.socket.read().map_err(|e| format!("DevTools read ({method}): {e}"))?;
            let Message::Text(text) = message else { continue };
            let reply: Value = serde_json::from_str(text.as_str()).map_err(|e| format!("DevTools sent bad JSON: {e}"))?;
            if reply["id"] != json!(id) {
                continue;
            }
            return match reply.get("error") {
                Some(error) => Err(format!("{method}: {}", error["message"].as_str().unwrap_or("failed"))),
                None => Ok(reply["result"].clone()),
            };
        }
    }

    /// Runs a script in the page and returns its value (a promise is awaited).
    pub fn eval(&mut self, expression: &str) -> Result<Value, String> {
        let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true });
        let result = self.call("Runtime.evaluate", params)?;
        if let Some(thrown) = result.get("exceptionDetails") {
            let text = thrown["exception"]["description"].as_str().or(thrown["text"].as_str()).unwrap_or("the script threw");
            return Err(format!("page script failed: {text}"));
        }
        Ok(result["result"]["value"].clone())
    }

    /// Polls a script every 100 ms until it returns something truthy. On the deadline it fails with `what`, the
    /// condition's own message.
    pub fn wait_for(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if truthy(&self.eval(expression)?) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("timed out after {} s waiting for {what}", timeout.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.call("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })).map(drop)
    }

    pub fn click_at(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.move_to(x, y)?;
        for (kind, buttons) in [("mousePressed", 1), ("mouseReleased", 0)] {
            let params = json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 });
            self.call("Input.dispatchMouseEvent", params)?;
        }
        Ok(())
    }

    /// Types text into the focused element, as the keyboard would.
    pub fn insert_text(&mut self, text: &str) -> Result<(), String> {
        self.call("Input.insertText", json!({ "text": text })).map(drop)
    }

    /// Presses a key or a chord such as `Enter`, `Escape` or `Control+,`.
    pub fn press(&mut self, chord: &str) -> Result<(), String> {
        let mut parts: Vec<&str> = chord.split('+').collect();
        // "Control++" is the plus key itself.
        if chord.ends_with("++") {
            parts.pop();
            parts.pop();
            parts.push("+");
        }
        let key = parts.pop().ok_or("empty key")?;
        let mut modifiers = 0;
        for modifier in parts {
            modifiers |= match modifier {
                "Alt" => 1,
                "Control" | "Ctrl" => 2,
                "Meta" => 4,
                "Shift" => 8,
                other => return Err(format!("unknown modifier {other} in {chord}")),
            };
        }
        let (name, code, virtual_key, text) = key_info(key)?;
        // Text is only typed when no command key is held.
        let typed = (modifiers & 7 == 0).then_some(text).flatten();
        for kind in ["keyDown", "keyUp"] {
            let mut params =
                json!({ "type": kind, "key": name, "code": code, "windowsVirtualKeyCode": virtual_key, "modifiers": modifiers });
            if let (Some(text), "keyDown") = (typed, kind) {
                params["text"] = json!(text);
            }
            self.call("Input.dispatchKeyEvent", params)?;
        }
        Ok(())
    }

    pub fn screenshot(&mut self, path: &Path) -> Result<(), String> {
        let shot = self.call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = shot["data"].as_str().ok_or("the screenshot had no data")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// JavaScript's idea of true, for the value a waiting script returns.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// (key, code, Windows virtual key, text typed) for the keys scenarios use.
fn key_info(key: &str) -> Result<(String, String, u32, Option<&'static str>), String> {
    let fixed = |name: &str, code: &str, vk: u32, text: Option<&'static str>| Ok((name.to_string(), code.to_string(), vk, text));
    match key {
        "Enter" => fixed("Enter", "Enter", 13, Some("\r")),
        "Escape" => fixed("Escape", "Escape", 27, None),
        "Tab" => fixed("Tab", "Tab", 9, None),
        "," => fixed(",", "Comma", 188, Some(",")),
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphabetic() => {
            let upper = k.to_ascii_uppercase();
            Ok((k.to_string(), format!("Key{upper}"), u32::from(upper.as_bytes()[0]), None))
        }
        other => Err(format!("the test client does not know the key {other}")),
    }
}

/// The page's own address on the DevTools socket, from `GET /json/list`.
fn page_url(port: u16) -> Result<String, String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    write!(stream, "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    // WebView2's server keeps the connection open after it answers, whatever `Connection` says, so waiting for the end of
    // the stream would never finish: the body is read by the length the headers give.
    let mut reader = BufReader::new(stream);
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err(if first { "no HTTP answer".to_string() } else { "the HTTP answer ended inside its headers".to_string() });
        }
        first = false;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<u64>().ok();
        }
    }
    let mut body = Vec::new();
    match length {
        Some(length) => reader.take(length).read_to_end(&mut body),
        None => reader.read_to_end(&mut body),
    }
    .map_err(|e| e.to_string())?;
    let targets: Vec<Value> = serde_json::from_slice(&body).map_err(|e| format!("the target list is not JSON: {e}"))?;
    targets
        .iter()
        .filter(|t| t["type"] == "page" && !t["url"].as_str().unwrap_or_default().starts_with("devtools://"))
        .find_map(|t| t["webSocketDebuggerUrl"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "no page is listed yet".to_string())
}

fn open(url: &str) -> Result<Cdp, String> {
    let host = url.trim_start_matches("ws://").split('/').next().unwrap_or_default();
    let stream = TcpStream::connect(host).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    let (socket, _) = tungstenite::client(url, stream).map_err(|e| format!("DevTools handshake: {e}"))?;
    Ok(Cdp { socket, next: 0 })
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit cdp:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 5 tests pass.

---

### Task 7: `locator.js` and `page`: finding elements and acting on them

**Model: sonnet**

**Files:**
- Create: `testkit/src/locator.js`, `testkit/src/page.rs`
- Modify: `testkit/src/lib.rs` (`pub mod page;`)

**Interfaces:**
- Consumes: `cdp::Cdp`, `cdp::DEFAULT_WAIT`.
- Produces (`faber_testkit::page`):
  - `struct Loc` (clonable list of narrowing steps) built with `css(selector)`, `role(role, name)` (accessible role whose name contains `name`, any case), `role_any(role)`, `label(text)` (a control labelled with that text), `text(text)` (smallest elements whose text contains it); chained with `.css(sel)`, `.role(role, name)`, `.text(t)`, `.text_re(pattern)`, `.has_text(t)` (keep matches whose text contains `t`), `.last()`, `.exact()` (make the last role, label or text step match the whole name or text, same case); `describe()` for failure messages.
  - `struct Page { pub cdp: Cdp }` with `connect(port, timeout)`, `eval`, `wait_for`, `wait_for_within`, `screenshot`, `press`; texts: `text_of(key)`, `text_in(key, language)`, `text_with(key, vars, language)`, `named(role, key) -> Loc`, `named_in(role, key, language)`, `labelled(key) -> Loc` (they call `window.__faberText`, which Task 10 adds); looking: `count`, `is_visible`, `wait_visible`, `wait_gone`, `wait_count`; acting on the one element a locator names (waits up to 15 s until there is exactly one, visible and enabled when the action needs it; several matches is an error, as in Playwright's strict mode): `inner_text`, `attr`, `is_disabled`, `is_checked`, `eval_on(loc, body)`, `click`, `hover`, `fill`, `press_on(loc, chord)`, `select_option`, `check`.
- `locator.js` runs inside the page. It installs `window.__tk` once with `find(steps)`, `one(steps, need)` and `shown(el)`. Its behaviour was checked against a page in headless Chrome during planning: roles of headings, buttons, links, textboxes, comboboxes, checkboxes, dialogs and articles; names from `aria-label`, `aria-labelledby`, `<label>` and text; hidden and disabled elements; strict one-match errors (`5 elements match, but one was expected`). It is exercised for real by the `screen` suite in Task 15 and Task 16.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod page;` to `testkit/src/lib.rs`. Create `testkit/src/page.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locators_are_built_step_by_step_and_exact_applies_to_the_last_step() {
        let loc = role("dialog", "Connect your AI").exact().role("button", "Done").exact();
        assert_eq!(loc.steps[0]["exact"], json!(true));
        assert_eq!(loc.steps[1]["exact"], json!(true));
        let loc = role("button", "Settings").css(".x").last();
        assert_eq!(loc.steps[0]["exact"], json!(false));
        assert_eq!(loc.steps.last().unwrap()["k"], "last");
        assert_eq!(css(".list .item").has_text("Long one").steps[1], json!({ "k": "has", "text": "Long one" }));
        assert_eq!(text("Connected").exact().steps[0]["want"]["text"], "Connected");
        assert_eq!(text("x").text_re("Ray(leigh)").steps[1]["want"], json!({ "re": "Ray(leigh)", "flags": "" }));
        assert!(role_any("checkbox").steps[0].get("want").is_none(), "any name");
    }

    #[test]
    fn a_locator_describes_itself_for_failure_messages() {
        let loc = css(".setup-card[data-vendor=\"codex\"]").text("Connected").exact().last();
        assert_eq!(loc.describe(), "css \".setup-card[data-vendor=\\\"codex\\\"]\" > text \"Connected\" > last");
        assert_eq!(label("Message").describe(), "label \"Message\"");
    }

    #[test]
    fn the_locator_script_installs_itself_once_and_exposes_what_the_page_calls_need() {
        for needed in ["window.__tk = { find, one, shown }", "if (window.__tk) return;", "unknown locator step"] {
            assert!(LOCATOR.contains(needed), "{needed}");
        }
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit page::`
Expected: compile errors (`cannot find function css`, and `couldn't read testkit/src/locator.js`).

- [ ] **Step 3: Write the implementation**

Create `testkit/src/locator.js`:

```js
// Sent to the page by the test client before it looks for anything. It finds elements the way a person describes them:
// by role and name, by label, by visible text or by CSS, narrowing step by step. It installs `window.__tk` once.
(() => {
  if (window.__tk) return;
  const norm = (s) => String(s ?? '').replace(/\s+/g, ' ').trim();
  const hidden = (el) => {
    for (let e = el; e; e = e.parentElement) {
      if (e.getAttribute('aria-hidden') === 'true') return true;
      const style = getComputedStyle(e);
      if (style.display === 'none' || style.visibility === 'hidden') return true;
    }
    return false;
  };
  const shown = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && !hidden(el);
  };
  const INPUT_ROLES = { checkbox: 'checkbox', radio: 'radio', button: 'button', submit: 'button', reset: 'button', range: 'slider' };
  const TAG_ROLES = { button: 'button', textarea: 'textbox', select: 'combobox', article: 'article', dialog: 'dialog', nav: 'navigation', ul: 'list', li: 'listitem' };
  const roleOf = (el) => {
    const given = el.getAttribute('role');
    if (given) return given;
    const tag = el.tagName.toLowerCase();
    if (/^h[1-6]$/.test(tag)) return 'heading';
    if (tag === 'input') return INPUT_ROLES[el.type] ?? 'textbox';
    if (tag === 'a') return el.hasAttribute('href') ? 'link' : '';
    return TAG_ROLES[tag] ?? '';
  };
  const textOfIds = (ids) => ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? '').join(' ');
  const nameOf = (el) => {
    if (el.getAttribute('aria-label')) return norm(el.getAttribute('aria-label'));
    if (el.getAttribute('aria-labelledby')) return norm(textOfIds(el.getAttribute('aria-labelledby')));
    if (el.labels?.length) return norm([...el.labels].map((l) => l.textContent).join(' '));
    return norm(el.textContent) || norm(el.getAttribute('title')) || norm(el.getAttribute('alt'));
  };
  const labelsOf = (el) => [
    el.getAttribute('aria-label'),
    el.getAttribute('aria-labelledby') ? textOfIds(el.getAttribute('aria-labelledby')) : null,
    ...(el.labels ? [...el.labels].map((l) => l.textContent) : []),
  ].filter(Boolean);
  // `want` is {text} (a case-insensitive substring, or the whole text when `exact`) or {re, flags}.
  const says = (text, want, exact) => {
    const seen = norm(text);
    if (want.re !== undefined) return new RegExp(want.re, want.flags ?? '').test(seen);
    const wanted = norm(want.text);
    return exact ? seen === wanted : seen.toLowerCase().includes(wanted.toLowerCase());
  };
  const inside = (els) => els.flatMap((e) => [...e.querySelectorAll('*')]);
  const step = (els, s) => {
    let found;
    if (s.k === 'css') found = els.flatMap((e) => [...e.querySelectorAll(s.sel)]);
    else if (s.k === 'role') found = inside(els).filter((e) => roleOf(e) === s.role && !hidden(e) && (!s.want || says(nameOf(e), s.want, s.exact)));
    else if (s.k === 'label') found = inside(els).filter((e) => !hidden(e) && labelsOf(e).some((t) => says(t, s.want, s.exact)));
    else if (s.k === 'text') {
      found = inside(els).filter((e) => !hidden(e) && says(e.textContent, s.want, s.exact) && ![...e.children].some((c) => says(c.textContent, s.want, s.exact)));
    } else if (s.k === 'has') found = els.filter((e) => norm(e.textContent).toLowerCase().includes(norm(s.text).toLowerCase()));
    else if (s.k === 'last') found = els.slice(-1);
    else throw new Error(`unknown locator step ${s.k}`);
    return [...new Set(found)];
  };
  const find = (steps) => steps.reduce(step, [document]);
  // The one element the steps name, or why there is none yet. `need`: 'attached', 'visible' or 'enabled'.
  const one = (steps, need) => {
    const els = find(steps);
    if (els.length === 0) return { why: 'no element matches' };
    if (els.length > 1) return { why: `${els.length} elements match, but one was expected` };
    const el = els[0];
    if (need !== 'attached' && !shown(el)) return { why: 'the element is not visible' };
    if (need === 'enabled' && (el.disabled || el.getAttribute('aria-disabled') === 'true')) return { why: 'the element is disabled' };
    return { el };
  };
  window.__tk = { find, one, shown };
})();
```

Then put this above the test block of `testkit/src/page.rs`:

```rust
//! The page, as a scenario sees it: elements found by role, label, text or CSS, and things a person does to them.
//! Everything here goes through `cdp`; the finding itself is `locator.js`, which runs inside the page.

use crate::cdp::{Cdp, DEFAULT_WAIT};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

const LOCATOR: &str = include_str!("locator.js");

/// Where to look, as steps that narrow the matches one after another. Build one with [`css`], [`role`], [`label`],
/// [`text`] or [`text_re`], and add steps with the methods.
#[derive(Debug, Clone)]
pub struct Loc {
    steps: Vec<Value>,
}

pub fn css(selector: &str) -> Loc {
    Loc { steps: vec![] }.css(selector)
}

/// An element with this role whose name contains `name` (any case). Call `.exact()` for the whole name.
pub fn role(role: &str, name: &str) -> Loc {
    Loc { steps: vec![] }.role(role, name)
}

/// An element with this role, whatever its name.
pub fn role_any(role: &str) -> Loc {
    Loc { steps: vec![json!({ "k": "role", "role": role })] }
}

/// A control labelled with this text (its `aria-label` or its `<label>`).
pub fn label(text: &str) -> Loc {
    Loc { steps: vec![json!({ "k": "label", "want": { "text": text }, "exact": false })] }
}

pub fn text(text: &str) -> Loc {
    Loc { steps: vec![] }.text(text)
}

impl Loc {
    pub fn css(mut self, selector: &str) -> Loc {
        self.steps.push(json!({ "k": "css", "sel": selector }));
        self
    }

    pub fn role(mut self, role: &str, name: &str) -> Loc {
        self.steps.push(json!({ "k": "role", "role": role, "want": { "text": name }, "exact": false }));
        self
    }

    /// The smallest elements whose text contains this text (any case). Call `.exact()` for the whole text.
    pub fn text(mut self, text: &str) -> Loc {
        self.steps.push(json!({ "k": "text", "want": { "text": text }, "exact": false }));
        self
    }

    /// The smallest elements whose text matches this JavaScript regular expression.
    pub fn text_re(mut self, pattern: &str) -> Loc {
        self.steps.push(json!({ "k": "text", "want": { "re": pattern, "flags": "" }, "exact": false }));
        self
    }

    /// Keeps only matches whose text contains this text (any case).
    pub fn has_text(mut self, text: &str) -> Loc {
        self.steps.push(json!({ "k": "has", "text": text }));
        self
    }

    pub fn last(mut self) -> Loc {
        self.steps.push(json!({ "k": "last" }));
        self
    }

    /// Makes the last role, label or text step match the whole name or text, in the same case.
    pub fn exact(mut self) -> Loc {
        if let Some(step) = self.steps.last_mut() {
            step["exact"] = json!(true);
        }
        self
    }

    fn steps_json(&self) -> String {
        Value::Array(self.steps.clone()).to_string()
    }

    /// A line for a failure message.
    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .steps
            .iter()
            .map(|s| match s["k"].as_str().unwrap_or("?") {
                "css" => format!("css {}", s["sel"]),
                "role" => format!("role {} {}", s["role"], s["want"]["text"]),
                "label" => format!("label {}", s["want"]["text"]),
                "text" => format!("text {}", if s["want"]["re"].is_null() { s["want"]["text"].clone() } else { s["want"]["re"].clone() }),
                "has" => format!("has text {}", s["text"]),
                other => other.to_string(),
            })
            .collect();
        parts.join(" > ")
    }
}

pub struct Page {
    pub cdp: Cdp,
}

impl Page {
    pub fn connect(port: u16, timeout: Duration) -> Result<Page, String> {
        Ok(Page { cdp: Cdp::connect(port, timeout)? })
    }

    pub fn eval(&mut self, expression: &str) -> Result<Value, String> {
        self.cdp.eval(expression)
    }

    pub fn wait_for(&mut self, what: &str, expression: &str) -> Result<(), String> {
        self.cdp.wait_for(what, expression, DEFAULT_WAIT)
    }

    pub fn wait_for_within(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String> {
        self.cdp.wait_for(what, expression, timeout)
    }

    pub fn screenshot(&mut self, path: &Path) -> Result<(), String> {
        self.cdp.screenshot(path)
    }

    pub fn press(&mut self, chord: &str) -> Result<(), String> {
        self.cdp.press(chord)
    }

    // The words on screen. The screen exposes its own translations in test builds, so scenarios never write a sentence.

    /// The text for `key` in the language the screen shows now.
    pub fn text_of(&mut self, key: &str) -> Result<String, String> {
        self.text_with(key, &json!({}), None)
    }

    /// The text for `key` in a given language (`en`, `pt-BR`, `es`, `fr`, `de`), whatever the screen shows.
    pub fn text_in(&mut self, key: &str, language: &str) -> Result<String, String> {
        self.text_with(key, &json!({}), Some(language))
    }

    pub fn text_with(&mut self, key: &str, vars: &Value, language: Option<&str>) -> Result<String, String> {
        let call = format!("window.__faberText({}, {}, {})", json!(key), vars, json!(language));
        let value = self.cdp.eval(&format!("(typeof window.__faberText === 'function') ? {call} : null"))?;
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("window.__faberText is missing or has no text for {key}: is this a test build?"))
    }

    /// A role locator named by a translation key: `named("heading", "empty.chatTitle")`.
    pub fn named(&mut self, role: &str, key: &str) -> Result<Loc, String> {
        Ok(self::role(role, &self.text_of(key)?))
    }

    pub fn named_in(&mut self, role: &str, key: &str, language: &str) -> Result<Loc, String> {
        Ok(self::role(role, &self.text_in(key, language)?))
    }

    /// A control labelled by a translation key.
    pub fn labelled(&mut self, key: &str) -> Result<Loc, String> {
        Ok(label(&self.text_of(key)?))
    }

    // Looking.

    fn probe(&mut self, loc: &Loc, body: &str) -> Result<Value, String> {
        self.cdp.eval(&format!("(() => {{ {LOCATOR}\n const steps = {}; {body} }})()", loc.steps_json()))
    }

    pub fn count(&mut self, loc: &Loc) -> Result<usize, String> {
        Ok(self.probe(loc, "return window.__tk.find(steps).length;")?.as_u64().unwrap_or(0) as usize)
    }

    pub fn is_visible(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.probe(loc, "return window.__tk.find(steps).some(window.__tk.shown);")?.as_bool().unwrap_or(false))
    }

    pub fn wait_visible(&mut self, loc: &Loc) -> Result<(), String> {
        self.wait_until(&format!("{} to be visible", loc.describe()), loc, "window.__tk.find(steps).some(window.__tk.shown)", DEFAULT_WAIT)
    }

    pub fn wait_gone(&mut self, loc: &Loc) -> Result<(), String> {
        self.wait_until(&format!("{} to disappear", loc.describe()), loc, "!window.__tk.find(steps).some(window.__tk.shown)", DEFAULT_WAIT)
    }

    pub fn wait_count(&mut self, loc: &Loc, count: usize) -> Result<(), String> {
        let what = format!("{} to match {count} elements", loc.describe());
        self.wait_until(&what, loc, &format!("window.__tk.find(steps).length === {count}"), DEFAULT_WAIT)
    }

    fn wait_until(&mut self, what: &str, loc: &Loc, condition: &str, timeout: Duration) -> Result<(), String> {
        let expression = format!("(() => {{ {LOCATOR}\n const steps = {}; return {condition}; }})()", loc.steps_json());
        self.cdp.wait_for(what, &expression, timeout)
    }

    // Reading and acting on the one element a locator names. Each waits (up to 15 s) until there is exactly one, and
    // that it is visible and enabled when the action needs it; a locator that matches several is a mistake in the scenario.

    fn on(&mut self, loc: &Loc, need: &str, body: &str) -> Result<Value, String> {
        let script = format!(
            "(() => {{ {LOCATOR}\n const found = window.__tk.one({}, '{need}'); if (!found.el) return {{ why: found.why }}; const el = found.el; {body} }})()",
            loc.steps_json()
        );
        let deadline = Instant::now() + DEFAULT_WAIT;
        loop {
            let value = self.cdp.eval(&script)?;
            if value["why"].is_null() {
                return Ok(value);
            }
            if Instant::now() >= deadline {
                return Err(format!("{}: {}", loc.describe(), value["why"].as_str().unwrap_or("not ready")));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    const CENTER: &'static str = "el.scrollIntoView({ block: 'center', inline: 'center' }); const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 };";

    pub fn inner_text(&mut self, loc: &Loc) -> Result<String, String> {
        Ok(self.on(loc, "attached", "return { value: el.innerText };")?["value"].as_str().unwrap_or_default().to_string())
    }

    pub fn attr(&mut self, loc: &Loc, name: &str) -> Result<Option<String>, String> {
        let value = self.on(loc, "attached", &format!("return {{ value: el.getAttribute({}) }};", json!(name)))?;
        Ok(value["value"].as_str().map(str::to_string))
    }

    pub fn is_disabled(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.on(loc, "attached", "return { value: !!el.disabled };")?["value"].as_bool().unwrap_or(false))
    }

    pub fn is_checked(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.on(loc, "attached", "return { value: !!el.checked };")?["value"].as_bool().unwrap_or(false))
    }

    /// Runs `body` with the element as `el`. It must `return` an object, which is handed back.
    pub fn eval_on(&mut self, loc: &Loc, body: &str) -> Result<Value, String> {
        self.on(loc, "attached", body)
    }

    pub fn click(&mut self, loc: &Loc) -> Result<(), String> {
        let at = self.on(loc, "enabled", Self::CENTER)?;
        self.cdp.click_at(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())
    }

    pub fn hover(&mut self, loc: &Loc) -> Result<(), String> {
        let at = self.on(loc, "visible", Self::CENTER)?;
        self.cdp.move_to(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())
    }

    /// Replaces what is in a text field with `text`, typed the way the keyboard would.
    pub fn fill(&mut self, loc: &Loc, text: &str) -> Result<(), String> {
        self.on(loc, "enabled", "el.focus(); if (el.select) el.select(); return {};")?;
        self.cdp.insert_text(text)
    }

    /// Focuses the element, then presses a key or chord (`Enter`, `Control+,`).
    pub fn press_on(&mut self, loc: &Loc, chord: &str) -> Result<(), String> {
        self.on(loc, "visible", "el.focus(); return {};")?;
        self.cdp.press(chord)
    }

    /// Picks the option of a `<select>` whose value or text is `choice`.
    pub fn select_option(&mut self, loc: &Loc, choice: &str) -> Result<(), String> {
        let body = format!(
            "const o = [...el.options].find((o) => o.value === {c} || o.text === {c}); if (!o) return {{ why: 'no option ' + {c} }}; \
             el.value = o.value; el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); return {{}};",
            c = json!(choice)
        );
        self.on(loc, "enabled", &body).map(drop)
    }

    /// Ticks a checkbox that is not ticked yet.
    pub fn check(&mut self, loc: &Loc) -> Result<(), String> {
        let state = self.on(
            loc,
            "enabled",
            &format!("const at = (() => {{ {} }})(); return {{ checked: el.checked, x: at.x, y: at.y }};", Self::CENTER),
        )?;
        if state["checked"] == json!(true) {
            return Ok(());
        }
        self.cdp.click_at(state["x"].as_f64().unwrap_or_default(), state["y"].as_f64().unwrap_or_default())?;
        if self.is_checked(loc)? { Ok(()) } else { Err(format!("{}: still not checked after the click", loc.describe())) }
    }
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit page:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 3 tests pass.

---

### Task 8: `app`: launching the app, the processes it starts, the audit

**Model: sonnet**

**Files:**
- Create: `testkit/src/app.rs`
- Modify: `testkit/src/lib.rs` (`pub mod app;`)

**Interfaces:**
- Consumes: `page::Page`, `proctree::{ProcRow, descendants, same_process, snapshot, kill_recorded, private_mb}`, `sandbox::{Sandbox, Launch, child_env, contained}`, `server::{Server, ASSET, zip_of}`. The Ctrl+C handler itself is installed by the runner in Task 14; this task only provides the cleanup it calls.
- Produces (`faber_testkit::app`):
  - `const STARTUP: Duration = 30 s`; `struct Paths { root, app, fake_cli, fixtures, shots }` with `Paths::discover() -> Result<Paths, String>` (`root` is the parent of the testkit folder; `app` is `target/faber-test/debug/faber[.exe]`; `fake_cli` is `fake-cli[.exe]` beside the running runner; `shots` is `target/faber-test/shots`).
  - `struct Recorded { pid, cwd, vendor, args, targets }` with `line()` (args joined by spaces); `read_records(state: &Path) -> (Vec<Recorded>, Vec<u32>)` (commands, and ids that wrote they ended).
  - `fn leaked(rows, seen, roots, running, not_before) -> Vec<ProcRow>`: the processes that should be gone but are not (pure; see the tests).
  - `fn emergency_cleanup()`: ends every recorded process and removes every sandbox folder; called by the runner's Ctrl+C handler.
  - `struct Opened { pub id: usize, pub pid: u32, pub page: Page }`.
  - `struct Ctx { pub paths, pub sb: Sandbox, pub server: Option<Server>, pub payload: Vec<u8>, pub notes: Vec<String> }` with `new(&Paths)`, `serve_release()`, `write_settings(&Value)` (does nothing if the file exists, so a scenario can pre-seed it), `spawn(Launch) -> Result<usize, String>` (starts the app with the sandbox environment, no shell, no console window, stderr kept), `launch(Launch) -> Result<Opened, String>` (adds a free DevTools port, waits up to 30 s for the page and for `.app`, failing early with the app's stderr if it exits), `exited(id)`, `stderr(id)`, `record(id)`, `memory_mb(id) -> Result<(f64, bool), String>` (private working set in MiB, and whether a webview process is in the tree), `close(id)`, `records()`, `audit()`, `audit_within(Duration)`, `shot(&mut Opened, name)`, `failure_shot(path)`, `finish()` (closes every app, waits up to 15 s for the processes to go, audits, then always cleans up: kills only recorded processes, stops the server, removes the folders). `Drop` for `Ctx` does the cleanup too, so a panic leaves nothing behind.
  - The audit fails if: a process of the app or of a fake-CLI command is still alive; a recorded CLI command ran from, or wrote to, a place outside the sandbox; the canary folder changed or a file appeared in the folders around the scenario; the real process environment changed; the real user `PATH` changed (Windows registry, read with `winreg`); or a download asked for anything but the loopback source.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod app;` to `testkit/src/lib.rs`. Create `testkit/src/app.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(pid: u32, parent: u32, born: u64) -> ProcRow {
        ProcRow { pid, parent, name: format!("p{pid}"), born }
    }
    fn ids(rows: &[ProcRow]) -> Vec<u32> {
        let mut ids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
        ids.sort();
        ids
    }
    // The same whole second, told apart only by microseconds.
    const T: u64 = 1_791_720_000_000_000;

    #[test]
    fn a_recorded_process_that_is_still_running_is_a_leak_but_a_reused_id_is_not() {
        let seen = [row(50, 10, T + 100), row(51, 50, T + 200)];
        let rows = [row(50, 10, T + 100), row(51, 7, T + 900_000)];
        assert_eq!(ids(&leaked(&rows, &seen, &[], &[], 0)), [50], "51 is a different process that took the same id in the same second");
    }

    #[test]
    fn what_still_names_a_dead_app_as_its_parent_is_a_leak_unless_it_is_older_than_the_app() {
        let app = row(900, 4, T + 500_000);
        let rows = [row(901, 900, T + 600_000), row(902, 900, T + 400_000)];
        assert_eq!(ids(&leaked(&rows, &[], std::slice::from_ref(&app), &[], 0)), [901]);
    }

    #[test]
    fn a_root_id_that_now_belongs_to_another_process_collects_nothing() {
        let app = row(900, 4, T + 500_000);
        let rows = [row(900, 4, T + 800_000), row(901, 900, T + 900_000)];
        assert!(leaked(&rows, &[], std::slice::from_ref(&app), &[], 0).is_empty());
    }

    #[test]
    fn a_cli_that_never_wrote_its_end_counts_only_if_it_is_younger_than_the_app() {
        let rows = [row(70, 1, T + 10), row(71, 1, T + 999_000)];
        assert_eq!(ids(&leaked(&rows, &[], &[], &[70, 71], T + 500_000)), [71]);
    }

    #[test]
    fn the_records_list_commands_and_the_ones_that_ended() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("commands.jsonl"),
            "{\"pid\":7,\"cwd\":\"C:/s\",\"vendor\":\"claude\",\"args\":[\"auth\",\"status\"],\"targets\":[\"C:/s/x\"]}\n{\"exited\":7}\n{\"pid\":8,\"cwd\":\"C:/s\",\"vendor\":\"codex\",\"args\":[\"login\"],\"targets\":[]}\n",
        )
        .unwrap();
        let (commands, exited) = read_records(dir.path());
        assert_eq!(exited, [7]);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].line(), "auth status");
        assert_eq!(commands[1].vendor, "codex");
        assert_eq!(read_records(&dir.path().join("missing")), (vec![], vec![]));
    }

    #[test]
    fn an_interrupted_run_removes_its_sandbox_folders() {
        let dir = tempfile::tempdir().unwrap().keep();
        std::fs::write(dir.join("x.txt"), "x").unwrap();
        registry().folders.push(dir.clone());
        emergency_cleanup();
        assert!(!dir.exists());
    }

    // R5.5: the audit of a whole scenario rejects writes outside its temporary folders, whatever the file is called.
    // It needs no app, so it runs on every system.
    #[test]
    fn r5_5_the_scenario_audit_rejects_writes_outside_the_sandbox() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        ctx.audit().expect("a fresh sandbox is clean");
        std::fs::write(ctx.sb.data.join("settings.json"), "{}").unwrap();
        ctx.audit().expect("a write inside the sandbox is fine");
        let beyond_observed = ctx.sb.root.join("external-write.txt");
        std::fs::write(&beyond_observed, "unauthorized absolute write beyond observed root").unwrap();
        assert!(ctx.audit().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&beyond_observed).unwrap();
        let named_like_a_test = ctx.sb.enclosing.join("unauthorized.test.ts");
        std::fs::write(&named_like_a_test, "unauthorized write named test.ts").unwrap();
        assert!(ctx.audit().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&named_like_a_test).unwrap();
        ctx.finish().expect("with the stray files removed the scenario ends clean");
    }

    #[test]
    fn a_cli_that_ran_or_wrote_outside_the_sandbox_fails_the_audit() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        let outside = ctx.sb.root.parent().unwrap().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let inside = ctx.sb.data.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let record = |cwd: &str, target: &str| {
            let command = json!({ "pid": 1, "cwd": cwd, "vendor": "claude", "args": [], "targets": [target] });
            format!("{command}\n{}\n", json!({ "exited": 1 }))
        };
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&inside, &inside)).unwrap();
        ctx.audit().expect("a CLI that stayed inside");
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&outside, &inside)).unwrap();
        assert!(ctx.audit().unwrap_err().contains("ran from outside the sandbox"));
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&inside, &outside)).unwrap();
        assert!(ctx.audit().unwrap_err().contains("wrote outside the sandbox"));
        std::fs::remove_file(ctx.sb.state.join("commands.jsonl")).unwrap();
        ctx.finish().unwrap();
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit app::`
Expected: compile errors (`cannot find ... Paths`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! Launching the test build of Faber inside a sandbox, keeping track of every process it starts, and the audit that
//! decides, after a scenario, whether it left anything behind.

use crate::page::Page;
use crate::proctree::{ProcRow, descendants, kill_recorded, same_process, snapshot};
use crate::sandbox::{Launch, Sandbox, child_env, contained, remove_tree};
use crate::server::{ASSET, Server, zip_of};
use serde_json::Value;
use std::io::Read;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the hidden app gets to open its window (or to write its checks report).
pub const STARTUP: Duration = Duration::from_secs(30);

/// Where everything the runner needs lives.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The repository.
    pub root: PathBuf,
    /// The test build of Faber.
    pub app: PathBuf,
    /// The fake CLI, built next to the runner.
    pub fake_cli: PathBuf,
    pub fixtures: PathBuf,
    pub shots: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Paths, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or("the testkit has no parent folder")?.to_path_buf();
        let exe = std::env::current_exe().map_err(|e| format!("cannot find the runner's own path: {e}"))?;
        let bin = exe.parent().ok_or("the runner has no folder")?.to_path_buf();
        let suffix = std::env::consts::EXE_SUFFIX;
        let target = root.join("target/faber-test");
        Ok(Paths {
            app: target.join(format!("debug/faber{suffix}")),
            fake_cli: bin.join(format!("fake-cli{suffix}")),
            fixtures: root.join("tests/fixtures"),
            shots: target.join("shots"),
            root,
        })
    }
}

/// A command the fake CLI was asked to run, as it wrote it down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub pid: u32,
    pub cwd: String,
    pub vendor: String,
    pub args: Vec<String>,
    pub targets: Vec<String>,
}

impl Recorded {
    pub fn line(&self) -> String {
        self.args.join(" ")
    }
}

/// Reads `commands.jsonl`: the commands, and the ids of those that wrote down that they ended.
pub fn read_records(state: &Path) -> (Vec<Recorded>, Vec<u32>) {
    let text = std::fs::read_to_string(state.join("commands.jsonl")).unwrap_or_default();
    let (mut commands, mut exited) = (Vec::new(), Vec::new());
    for value in text.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok()) {
        if let Some(pid) = value["exited"].as_u64() {
            exited.push(pid as u32);
            continue;
        }
        let strings =
            |v: &Value| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
        commands.push(Recorded {
            pid: value["pid"].as_u64().unwrap_or(0) as u32,
            cwd: value["cwd"].as_str().unwrap_or_default().to_string(),
            vendor: value["vendor"].as_str().unwrap_or_default().to_string(),
            args: strings(&value["args"]),
            targets: strings(&value["targets"]),
        });
    }
    (commands, exited)
}

/// The processes that should be gone but are not, as a pure function over the process list.
///
/// - `seen`: every process of the app's tree recorded while it ran (id and creation time).
/// - `roots`: the app's own processes. Whatever still names one as its parent, and was created after it, counts too,
///   unless the root's id now belongs to a different process.
/// - `running`: ids of fake CLIs that never wrote that they ended. An id only counts if its process was created after
///   `not_before`, because an older process cannot be one of ours.
pub fn leaked(rows: &[ProcRow], seen: &[ProcRow], roots: &[ProcRow], running: &[u32], not_before: u64) -> Vec<ProcRow> {
    let mut all = rows.to_vec();
    let mut trees: Vec<Vec<ProcRow>> = Vec::new();
    for root in roots {
        match rows.iter().find(|r| r.pid == root.pid) {
            Some(now) if !same_process(now, std::slice::from_ref(root)) => continue,
            Some(_) => {}
            None => all.push(root.clone()),
        }
        trees.push(descendants(&all, root.pid));
    }
    rows.iter()
        .filter(|r| {
            same_process(r, seen) || trees.iter().any(|tree| same_process(r, tree)) || (running.contains(&r.pid) && r.born >= not_before)
        })
        .cloned()
        .collect()
}

#[derive(Default)]
struct Registry {
    seen: Vec<ProcRow>,
    folders: Vec<PathBuf>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry { seen: Vec::new(), folders: Vec::new() });

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called when the runner is interrupted (Ctrl+C): ends every process it recorded and removes every sandbox.
pub fn emergency_cleanup() {
    let (seen, folders) = {
        let mut r = registry();
        (std::mem::take(&mut r.seen), std::mem::take(&mut r.folders))
    };
    let _ = kill_recorded(&seen);
    for folder in folders {
        for _ in 0..5 {
            if remove_tree(&folder).is_ok() || !folder.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

struct Live {
    child: Child,
    pid: u32,
    port: Option<u16>,
    seen: Vec<ProcRow>,
    stderr: Arc<Mutex<String>>,
}

/// A launched app that a scenario drives: which one it is, and its page.
pub struct Opened {
    pub id: usize,
    pub pid: u32,
    pub page: Page,
}

/// One scenario's world: its sandbox, the fake release server if it needs one, and every app it launched.
pub struct Ctx {
    pub paths: Paths,
    pub sb: Sandbox,
    pub server: Option<Server>,
    /// The bytes the fake release server hands out as the downloaded program.
    pub payload: Vec<u8>,
    /// What the scenario wants to say about a pass, such as a memory measurement.
    pub notes: Vec<String>,
    live: Vec<Live>,
    real_env: Vec<(String, String)>,
    real_user_path: Option<String>,
    finished: bool,
}

fn env_pairs() -> Vec<(String, String)> {
    std::env::vars().collect()
}

/// The user's own `PATH` as the system stores it. Only Windows has a per-user value that an installer could change.
#[cfg(windows)]
fn user_path() -> Result<Option<String>, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment").map_err(|e| format!("cannot read the user environment: {e}"))?;
    match key.get_value::<String, _>("Path") {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read the user PATH: {e}")),
    }
}

#[cfg(not(windows))]
fn user_path() -> Result<Option<String>, String> {
    Ok(None)
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| format!("no free port: {e}"))?;
    listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

impl Ctx {
    pub fn new(paths: &Paths) -> Result<Ctx, String> {
        let sb = Sandbox::new().map_err(|e| format!("cannot create the sandbox: {e}"))?;
        registry().folders.push(sb.root.clone());
        Ok(Ctx {
            paths: paths.clone(),
            sb,
            server: None,
            payload: Vec::new(),
            notes: Vec::new(),
            live: Vec::new(),
            real_env: env_pairs(),
            real_user_path: user_path()?,
            finished: false,
        })
    }

    /// Starts the fake release server. The program it serves is a copy of the fake CLI, zipped like the real asset.
    pub fn serve_release(&mut self) -> Result<(), String> {
        let payload = std::fs::read(&self.paths.fake_cli).map_err(|e| format!("{}: {e}", self.paths.fake_cli.display()))?;
        let inner = ASSET.trim_end_matches(".zip");
        let archive = zip_of(inner, &payload).map_err(|e| e.to_string())?;
        self.server = Some(Server::start(archive).map_err(|e| format!("cannot start the release server: {e}"))?);
        self.payload = payload;
        Ok(())
    }

    /// Writes `settings.json` in the data folder, unless a scenario already put one there.
    pub fn write_settings(&self, settings: &Value) -> Result<(), String> {
        let path = self.sb.data.join("settings.json");
        if path.exists() {
            return Ok(());
        }
        std::fs::write(&path, settings.to_string()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Starts the app with the sandbox environment and returns its id. Nothing is waited for.
    pub fn spawn(&mut self, mut launch: Launch) -> Result<usize, String> {
        if launch.setup_source.is_none() {
            launch.setup_source = self.server.as_ref().map(|s| s.origin.clone());
        }
        let env = child_env(&env_pairs(), &self.sb, &launch, &self.paths.fake_cli);
        let mut command = Command::new(&self.paths.app);
        command.env_clear().envs(&env).current_dir(&self.sb.sandbox).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
        let mut child = command.spawn().map_err(|e| format!("cannot start {}: {e}", self.paths.app.display()))?;
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(mut pipe) = child.stderr.take() {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let mut text = sink.lock().unwrap_or_else(|e| e.into_inner());
                    if text.len() < 64_000 {
                        text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    }
                }
            });
        }
        let pid = child.id();
        self.live.push(Live { child, pid, port: launch.cdp_port, seen: Vec::new(), stderr });
        let id = self.live.len() - 1;
        self.record(id)?;
        Ok(id)
    }

    /// Launches the app with its DevTools port and waits for its window.
    pub fn launch(&mut self, mut launch: Launch) -> Result<Opened, String> {
        let port = free_port()?;
        launch.cdp_port = Some(port);
        let id = self.spawn(launch)?;
        let deadline = Instant::now() + STARTUP;
        let mut page = loop {
            if let Some(status) = self.exited(id) {
                return Err(format!("the app exited before its window opened ({status}). stderr: {}", self.stderr(id)));
            }
            match Page::connect(port, Duration::from_millis(500)) {
                Ok(page) => break page,
                Err(e) if Instant::now() >= deadline => return Err(format!("{e}. stderr: {}", self.stderr(id))),
                Err(_) => {}
            }
        };
        page.wait_for_within("the screen to render", "!!document.querySelector('.app')", STARTUP)
            .map_err(|e| format!("{e}. stderr: {}", self.stderr(id)))?;
        self.record(id)?;
        Ok(Opened { id, pid: self.live[id].pid, page })
    }

    /// The exit status, once the app has ended.
    pub fn exited(&mut self, id: usize) -> Option<String> {
        self.live[id].child.try_wait().ok().flatten().map(|s| s.to_string())
    }

    pub fn stderr(&self, id: usize) -> String {
        self.live[id].stderr.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string()
    }

    /// Adds the app's processes alive right now to the ones recorded, so they can be audited and ended later.
    pub fn record(&mut self, id: usize) -> Result<(), String> {
        let rows = snapshot()?;
        let live = &mut self.live[id];
        let mut fresh = descendants(&rows, live.pid);
        fresh.retain(|row| !same_process(row, &live.seen));
        registry().seen.extend(fresh.iter().cloned());
        live.seen.extend(fresh);
        Ok(())
    }

    /// The private working set of the app and its whole tree, in MiB, and whether a webview process is part of it.
    pub fn memory_mb(&mut self, id: usize) -> Result<(f64, bool), String> {
        self.record(id)?;
        let live = &self.live[id];
        let webview = descendants(&snapshot()?, live.pid).iter().any(|r| r.name.to_lowercase().contains("msedgewebview2"));
        Ok((crate::proctree::private_mb(live.pid)?, webview))
    }

    /// Ends the app and waits for it. Its children are recorded first, so the audit can check they follow.
    pub fn close(&mut self, id: usize) -> Result<(), String> {
        self.record(id)?;
        let live = &mut self.live[id];
        if live.child.try_wait().ok().flatten().is_none() {
            let _ = live.child.kill();
        }
        let _ = live.child.wait();
        Ok(())
    }

    pub fn records(&self) -> Vec<Recorded> {
        read_records(&self.sb.state).0
    }

    /// Fake CLIs that never wrote that they ended.
    fn running(&self) -> Vec<u32> {
        let (commands, exited) = read_records(&self.sb.state);
        commands.iter().map(|c| c.pid).filter(|pid| !exited.contains(pid)).collect()
    }

    fn all_seen(&self) -> Vec<ProcRow> {
        self.live.iter().flat_map(|l| l.seen.iter().cloned()).collect()
    }

    fn roots(&self) -> Vec<ProcRow> {
        self.live.iter().filter_map(|l| l.seen.iter().find(|r| r.pid == l.pid).cloned()).collect()
    }

    fn survivors(&self) -> Result<Vec<ProcRow>, String> {
        let roots = self.roots();
        let not_before = roots.iter().map(|r| r.born).min().unwrap_or(0);
        Ok(leaked(&snapshot()?, &self.all_seen(), &roots, &self.running(), not_before))
    }

    /// Checks that the scenario left nothing behind and touched nothing outside its sandbox.
    pub fn audit(&mut self) -> Result<(), String> {
        let mut problems: Vec<String> = Vec::new();
        match self.survivors() {
            Ok(rows) if rows.is_empty() => {}
            Ok(rows) => {
                let list: Vec<String> = rows.iter().map(|r| format!("{}#{}", r.name, r.pid)).collect();
                problems.push(format!("a process of the app or of a CLI is still running: {}", list.join(", ")));
            }
            Err(e) => problems.push(e),
        }
        for command in self.records() {
            if !contained(&self.sb.sandbox, Path::new(&command.cwd)) {
                problems.push(format!("a CLI ran from outside the sandbox: {}", command.cwd));
            }
            for target in &command.targets {
                if !contained(&self.sb.sandbox, Path::new(target)) {
                    problems.push(format!("a CLI wrote outside the sandbox: {target}"));
                }
            }
        }
        if let Err(e) = self.sb.audit_files() {
            problems.push(e);
        }
        if env_pairs() != self.real_env {
            problems.push("the real process environment changed".into());
        }
        match user_path() {
            Ok(now) if now == self.real_user_path => {}
            Ok(_) => problems.push("the real user PATH changed".into()),
            Err(e) => problems.push(e),
        }
        if let Some(server) = &self.server {
            let unexpected = server.unexpected();
            if !unexpected.is_empty() {
                problems.push(format!("a download asked for something other than the loopback source: {}", unexpected.join(", ")));
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) }
    }

    /// A screenshot of a checkpoint a scenario names, in `target/faber-test/shots/`.
    pub fn shot(&self, app: &mut Opened, name: &str) -> Result<(), String> {
        app.page.screenshot(&self.paths.shots.join(format!("{name}.png")))
    }

    /// The audit, tried again for up to `wait`: a process that was just told to end needs a moment to go.
    pub fn audit_within(&mut self, wait: Duration) -> Result<(), String> {
        let deadline = Instant::now() + wait;
        loop {
            match self.audit() {
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
                Ok(()) => return Ok(()),
            }
        }
    }

    /// A screenshot of the first app that has a DevTools port, taken over its own connection so a stuck page client
    /// does not block it. Best effort: it is used after a failure.
    pub fn failure_shot(&self, to: &Path) {
        for live in &self.live {
            let Some(port) = live.port else { continue };
            if let Ok(mut page) = Page::connect(port, Duration::from_secs(2))
                && page.screenshot(to).is_ok()
            {
                return;
            }
        }
    }

    /// Ends the scenario: closes every app, waits for its processes to go, audits, and removes everything.
    /// The audit's verdict is returned; the cleanup happens either way.
    pub fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        for id in 0..self.live.len() {
            let _ = self.close(id);
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.survivors().is_ok_and(|rows| !rows.is_empty()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let verdict = self.audit();
        self.cleanup();
        verdict
    }

    /// Ends what is left of the processes this scenario started, and removes its folders. Only processes whose id and
    /// creation time were recorded are touched: an id the system has handed to another program is left alone.
    fn cleanup(&mut self) {
        let _ = kill_recorded(&self.all_seen());
        if let Ok(rows) = snapshot() {
            let not_before = self.roots().iter().map(|r| r.born).min().unwrap_or(u64::MAX);
            let stragglers: Vec<ProcRow> = rows.into_iter().filter(|r| self.running().contains(&r.pid) && r.born >= not_before).collect();
            let _ = kill_recorded(&stragglers);
        }
        if let Some(server) = &mut self.server {
            server.stop();
        }
        self.sb.dispose();
        let mut r = registry();
        r.folders.retain(|f| f != &self.sb.root);
        let ours = self.all_seen();
        r.seen.retain(|row| !same_process(row, &ours));
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            for id in 0..self.live.len() {
                let _ = self.close(id);
            }
            self.cleanup();
        }
    }
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit app:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 8 tests pass, among them the two R5.5 tests (Review Focus 4 and 5: `a_recorded_process_that_is_still_running_is_a_leak_but_a_reused_id_is_not`, `an_interrupted_run_removes_its_sandbox_folders`). They create sandboxes and read the process list; they start no process.

---

### Task 9: `report` and `testbuild`

**Model: haiku**

**Files:**
- Create: `testkit/src/report.rs`, `testkit/src/testbuild.rs`
- Modify: `testkit/src/lib.rs` (`pub mod report;`, `pub mod testbuild;`)

**Interfaces:**
- Consumes: `plan::Suite`, `app::Paths`.
- Produces (`faber_testkit::report`): `enum Status { Pass, Fail, NotRun, Skipped }`; `struct Outcome { suite, name, status, time: Option<Duration>, message }` with `Outcome::pass`, `pass_with(.., note)`, `fail`, `not_run`, `skipped`; `fn line(&Outcome) -> String` (`pass  ui      name (4 ms)`; a failure adds its reason on indented lines; a pass may carry a note in brackets); `fn summary(&[Outcome]) -> String`; `fn exit_code(&[Outcome]) -> i32` (1 on any failure or any suite that could not run, 0 otherwise, so skipping `screen` on macOS is fine and a failed build is never a pass); `fn file_name(test: &str) -> String` (safe name for a screenshot).
- Produces (`faber_testkit::testbuild`): `const TEST_DIST: &str = "dist-test"`; `fn vite_args() -> Vec<String>` (`node_modules/vite/bin/vite.js build --mode test --outDir ../dist-test --emptyOutDir`); `fn tauri_args(target_dir: &Path, jobs: usize) -> Vec<String>` (`node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle --features test-hooks --config src-tauri/tauri.test.conf.json -- --target-dir <abs> -j <jobs>`); `fn fingerprint(dir: &Path) -> String`; `fn ensure_fake_cli(&Paths) -> Result<(), String>`; `fn build_test_app(&Paths, jobs: usize) -> Result<(), String>`. Both tools are started as `node <script>` with the repository as the working directory; a failure returns the tool's own output; the build fails if `dist/` changed.
- Task 11 adds three more tests to `testbuild.rs` (the override config, the feature flag, the production `dist/`), once the files they read exist.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod report;` and `pub mod testbuild;` to `testkit/src/lib.rs`. Create the two files, each containing only its tests. The `testbuild` tests below are the three that need nothing else; Task 11 adds three more.

`testkit/src/report.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_has_the_word_the_suite_the_name_and_the_time() {
        let pass = Outcome::pass(Suite::Ui, "i18n: every locale has every key", Some(Duration::from_millis(4)));
        assert_eq!(line(&pass), "pass  ui      i18n: every locale has every key (4 ms)");
        let slow = Outcome::pass(Suite::Screen, "chat", Some(Duration::from_millis(3210)));
        assert_eq!(line(&slow), "pass  screen  chat (3.2 s)");
        let measured = Outcome::pass_with(Suite::Screen, "idle memory", Some(Duration::from_millis(900)), "idle: 143 MB");
        assert_eq!(line(&measured), "pass  screen  idle memory (900 ms)  [idle: 143 MB]");
    }

    #[test]
    fn a_failure_shows_its_reason_under_the_line() {
        let fail = Outcome::fail(Suite::Screen, "chat", None, "no element matches\nstderr: boom");
        assert_eq!(line(&fail), "FAIL  screen  chat\n          no element matches\n          stderr: boom");
    }

    #[test]
    fn the_summary_counts_every_kind_and_only_failures_and_not_runs_fail_the_run() {
        let mut all = vec![Outcome::pass(Suite::Rust, "a", None), Outcome::skipped(Suite::Screen, "needs WebView2", "x")];
        assert_eq!(summary(&all), "1 passed, 0 failed, 0 not run, 1 skipped");
        assert_eq!(exit_code(&all), 0);
        all.push(Outcome::not_run(Suite::Ui, "ui", "the build failed"));
        assert_eq!(exit_code(&all), 1, "a suite that could not run is never a pass");
        all.pop();
        all.push(Outcome::fail(Suite::Ui, "b", None, ""));
        assert_eq!(summary(&all), "1 passed, 1 failed, 0 not run, 1 skipped");
        assert_eq!(exit_code(&all), 1);
    }

    #[test]
    fn a_screenshot_name_keeps_only_safe_characters() {
        assert_eq!(file_name("C5.1: installed signed-in Claude"), "C5_1__installed_signed-in_Claude");
    }
}
```

`testkit/src/testbuild.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vite_builds_in_test_mode_into_its_own_folder_without_the_shell() {
        let args = vite_args();
        assert_eq!(args[0], "node_modules/vite/bin/vite.js", "node runs Vite's own script, not npm");
        let at = |flag: &str| args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} is missing"));
        assert_eq!(args[at("--mode") + 1], "test");
        assert_eq!(args[at("--outDir") + 1], "../dist-test");
        assert!(args.contains(&"--emptyOutDir".to_string()));
        assert!(!args.iter().any(|a| a.contains("npm") || a.ends_with(".cmd")));
    }

    #[test]
    fn tauri_builds_a_debug_app_with_the_test_hooks_and_the_override_config() {
        let args = tauri_args(Path::new("/work/target/faber-test"), 4);
        assert_eq!(args[0], "node_modules/@tauri-apps/cli/tauri.js");
        let before_cargo = &args[..args.iter().position(|a| a == "--").unwrap()];
        for needed in ["build", "--debug", "--no-bundle"] {
            assert!(before_cargo.contains(&needed.to_string()), "{needed}");
        }
        let at = |flag: &str| before_cargo.iter().position(|a| a == flag).unwrap();
        assert_eq!(before_cargo[at("--features") + 1], "test-hooks");
        assert_eq!(before_cargo[at("--config") + 1], "src-tauri/tauri.test.conf.json");
        let cargo_args = &args[args.iter().position(|a| a == "--").unwrap() + 1..];
        assert_eq!(cargo_args, ["--target-dir", "/work/target/faber-test", "-j", "4"]);
    }

    #[test]
    fn the_fingerprint_changes_when_a_file_is_added_changed_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(fingerprint(&dir.path().join("missing")), "absent");
        let empty = fingerprint(dir.path());
        std::fs::write(dir.path().join("index.html"), "one").unwrap();
        let one = fingerprint(dir.path());
        assert_ne!(one, empty);
        assert_eq!(fingerprint(dir.path()), one, "the same folder gives the same fingerprint");
        std::fs::write(dir.path().join("index.html"), "three").unwrap();
        assert_ne!(fingerprint(dir.path()), one, "a file that changed size");
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/app.js"), "x").unwrap();
        let two = fingerprint(dir.path());
        std::fs::remove_file(dir.path().join("assets/app.js")).unwrap();
        assert_ne!(fingerprint(dir.path()), two);
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit report:: testbuild::`
Expected: compile errors (`cannot find ... Outcome`, `vite_args`).

- [ ] **Step 3: Write the implementation**

`testkit/src/report.rs`, above its test block:

```rust
//! What the runner prints: one line per test, and a summary. Pure functions, so the format is tested.

use crate::plan::Suite;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    /// The suite could not start (the build failed), which is never reported as a pass.
    NotRun,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub suite: Suite,
    pub name: String,
    pub status: Status,
    /// How long it took, when it is known.
    pub time: Option<Duration>,
    /// Why it failed or was skipped; for a pass, a note such as a measurement.
    pub message: String,
}

impl Outcome {
    pub fn pass(suite: Suite, name: &str, time: Option<Duration>) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Pass, time, message: String::new() }
    }

    pub fn pass_with(suite: Suite, name: &str, time: Option<Duration>, note: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Pass, time, message: note.into() }
    }

    pub fn fail(suite: Suite, name: &str, time: Option<Duration>, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Fail, time, message: message.into() }
    }

    pub fn not_run(suite: Suite, name: &str, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::NotRun, time: None, message: message.into() }
    }

    pub fn skipped(suite: Suite, name: &str, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Skipped, time: None, message: message.into() }
    }
}

fn seconds(time: Duration) -> String {
    if time < Duration::from_secs(1) { format!("{} ms", time.as_millis()) } else { format!("{:.1} s", time.as_secs_f64()) }
}

/// `pass  ui      i18n: every locale has every key (4 ms)`, and the reason on the lines after a failure.
pub fn line(outcome: &Outcome) -> String {
    let word = match outcome.status {
        Status::Pass => "pass",
        Status::Fail => "FAIL",
        Status::NotRun => "----",
        Status::Skipped => "skip",
    };
    let mut text = format!("{word}  {:<6}  {}", outcome.suite.name(), outcome.name);
    if let Some(time) = outcome.time {
        text.push_str(&format!(" ({})", seconds(time)));
    }
    if outcome.status == Status::Pass && !outcome.message.is_empty() {
        text.push_str(&format!("  [{}]", outcome.message));
    }
    if outcome.status != Status::Pass && !outcome.message.is_empty() {
        for message_line in outcome.message.lines() {
            text.push_str(&format!("\n          {message_line}"));
        }
    }
    text
}

pub fn summary(outcomes: &[Outcome]) -> String {
    let count = |status| outcomes.iter().filter(|o| o.status == status).count();
    format!(
        "{} passed, {} failed, {} not run, {} skipped",
        count(Status::Pass),
        count(Status::Fail),
        count(Status::NotRun),
        count(Status::Skipped)
    )
}

/// 1 when anything failed or could not run; skipping a suite that does not exist on this system is fine.
pub fn exit_code(outcomes: &[Outcome]) -> i32 {
    i32::from(outcomes.iter().any(|o| matches!(o.status, Status::Fail | Status::NotRun)))
}

/// A name that is safe to use as a file name, for the screenshot of a failed test.
pub fn file_name(test: &str) -> String {
    test.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
```

`testkit/src/testbuild.rs`, above its test block:

```rust
//! Building the test app: the screen with Vite in test mode, then the shell with the Tauri CLI. Both are started as
//! `node <script>`, never through `npm`, a `.cmd` shim or a shell. The production `dist/` is never touched.

use crate::app::Paths;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

/// Where Vite writes the test screen (relative to the repository). The shell embeds it.
pub const TEST_DIST: &str = "dist-test";

/// The arguments of the Vite build. `vite.config.ts` has `root: 'ui'`, so `--outDir` is relative to that folder.
pub fn vite_args() -> Vec<String> {
    ["node_modules/vite/bin/vite.js", "build", "--mode", "test", "--outDir", "../dist-test", "--emptyOutDir"].map(String::from).to_vec()
}

/// The arguments of the Tauri build. After the `--`, Cargo's: its own target folder (absolute, because Cargo reads it
/// from `src-tauri`) and the number of jobs.
pub fn tauri_args(target_dir: &Path, jobs: usize) -> Vec<String> {
    let mut args: Vec<String> = [
        "node_modules/@tauri-apps/cli/tauri.js",
        "build",
        "--debug",
        "--no-bundle",
        "--features",
        "test-hooks",
        "--config",
        "src-tauri/tauri.test.conf.json",
        "--",
        "--target-dir",
    ]
    .map(String::from)
    .to_vec();
    args.push(target_dir.to_string_lossy().into_owned());
    args.extend(["-j".to_string(), jobs.to_string()]);
    args
}

/// A fingerprint of a folder: every file's relative path, size and modification time. `absent` when it does not exist.
/// The test build must leave the production `dist/` exactly as it found it.
pub fn fingerprint(dir: &Path) -> String {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().ok().and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                out.push(format!("{} {} {modified}", path.strip_prefix(base).unwrap_or(&path).display(), meta.len()));
            }
        }
    }
    if !dir.exists() {
        return "absent".into();
    }
    let mut lines = Vec::new();
    walk(dir, dir, &mut lines);
    lines.sort();
    format!("{:x}", Sha256::digest(lines.join("\n")))
}

fn run(what: &str, mut command: Command) -> Result<(), String> {
    let output = command.output().map_err(|e| format!("cannot start the {what}: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "the {what} failed ({})\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Builds the fake CLI next to the runner. `cargo run` builds only the binary it runs.
pub fn ensure_fake_cli(paths: &Paths) -> Result<(), String> {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(&paths.root).args(["build", "--quiet", "--package", "faber-testkit", "--bin", "fake-cli"]);
    run("build of the fake CLI", command)?;
    if paths.fake_cli.exists() { Ok(()) } else { Err(format!("the fake CLI is not at {}", paths.fake_cli.display())) }
}

/// Builds the test app into `target/faber-test/`, once per run, with `jobs` cores for Cargo.
pub fn build_test_app(paths: &Paths, jobs: usize) -> Result<(), String> {
    let production = paths.root.join("dist");
    let before = fingerprint(&production);
    let mut vite = Command::new("node");
    vite.current_dir(&paths.root).args(vite_args());
    run("Vite build", vite)?;
    let mut tauri = Command::new("node");
    tauri.current_dir(&paths.root).args(tauri_args(&paths.root.join("target/faber-test"), jobs));
    run("Tauri build", tauri)?;
    if fingerprint(&production) != before {
        return Err("the test build changed dist/, which only the production build may write".into());
    }
    if paths.app.exists() { Ok(()) } else { Err(format!("the test build did not produce {}", paths.app.display())) }
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit report:: testbuild:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 4 `report` tests and 3 `testbuild` tests pass.



---

### Task 10: The screen's checks, ported: `tests/ui/`

**Model: sonnet**

**Files:**
- Create: `tests/ui/check.ts`, `tests/ui/check.check.ts`, `tests/ui/i18n.check.ts`, `tests/ui/models.check.ts`, `tests/ui/setup.check.ts`, `tests/ui/index.ts`

**Interfaces:**
- Consumes: the Node originals `tests/i18n.test.ts`, `tests/models.test.ts` and `tests/setup.test.ts` (read them; they stay in place until Task 17), and the screen's own modules `ui/i18n.ts` (`LANGUAGES`, `t`), `ui/locales/*.ts`, `ui/models.ts` (`modelOptions`) and `ui/setup.ts`.
- Produces (`tests/ui/check.ts`): `type Result = { name: string; ok: boolean; message: string; ms: number }`; `check(name, run)` registers a check; `runAll(): Promise<Result[]>` runs every registered check in registration order and reports each (a throw is a failure, never a crash); assertions `ok(value, message?)`, `equal(actual, expected, message?)`, `notEqual`, `deepEqual`, `matches(text, pattern)`, `doesNotMatch`. They replace `node:assert/strict`: nothing in this folder may import from `node:` because the checks run inside the app's webview.
- Produces (`tests/ui/index.ts`): `window.__faberChecks(): Promise<Result[]>`; `window.__faberText(key, vars?, language?)` (the screen's own translation, so screen scenarios in Rust find elements by role and translated name and never write a sentence); and, when the page was opened as `index.html?checks`, a run of all checks at load whose result goes to the shell command `report_checks` (Task 11 adds that command).
- The 9 Node tests become 9 checks with the same assertions, and a tenth check tests the assertions themselves: the old runner could not be wrong about `assert.equal`, this one can.

**Proof commands that must pass at the end:** `npm run typecheck`, `npm run lint`, and the Node smoke run in Step 7 (10 checks pass). Nothing in Rust changes in this task, so the Rust proof commands still pass as they did after Task 9.

- [ ] **Step 1: Write the failing test of the assertions**

Create `tests/ui/check.check.ts`. An assertion that never throws would turn every other check green, so this one runs first:

```ts
import { check, deepEqual, doesNotMatch, equal, matches, notEqual, ok } from './check.ts';

// An assertion that never throws would turn every other check green, so the helpers are checked first.
const throws = (run: () => void) => {
  try {
    run();
  } catch {
    return true;
  }
  return false;
};

check('check: the assertions fail when they should and pass when they should', () => {
  ok(throws(() => ok(0)), 'ok(0) must throw');
  ok(throws(() => ok('')), "ok('') must throw");
  ok(!throws(() => ok(1)), 'ok(1) must not throw');
  ok(throws(() => equal(1, 2)), 'equal(1, 2) must throw');
  ok(!throws(() => equal('a', 'a')), "equal('a', 'a') must not throw");
  ok(throws(() => notEqual(1, 1)), 'notEqual(1, 1) must throw');
  ok(!throws(() => notEqual(1, 2)), 'notEqual(1, 2) must not throw');
  ok(throws(() => deepEqual({ a: [1, 2] }, { a: [1, 3] })), 'deepEqual must see a different array item');
  ok(throws(() => deepEqual({ a: 1 }, { a: 1, b: undefined })), 'deepEqual must see an extra key');
  ok(throws(() => deepEqual([1], { 0: 1 })), 'an array is not an object');
  ok(!throws(() => deepEqual({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] })), 'deepEqual must accept equal structures');
  ok(throws(() => matches('abc', /x/)), 'matches must throw when the text differs');
  ok(!throws(() => matches('abc', /b/)), 'matches must pass when the text matches');
  ok(throws(() => doesNotMatch('abc', /b/)), 'doesNotMatch must throw when the text matches');
  ok(!throws(() => doesNotMatch('abc', /x/)), 'doesNotMatch must pass when it does not');
});
```

- [ ] **Step 2: Run the type check and see it fail**

Run: `npm run typecheck`
Expected: FAIL with `Cannot find module './check.ts'` (TS2307) for `tests/ui/check.check.ts`.

- [ ] **Step 3: Write the registry and the assertions**

Create `tests/ui/check.ts`:

```ts
// A tiny check registry for the screen's own logic. The checks run inside a hidden test build of Faber (see
// testkit/src/checks.rs), so nothing here may use Node. A check throws to fail; the assertions below do that.

export type Result = { name: string; ok: boolean; message: string; ms: number };

const registry: Array<{ name: string; run: () => void | Promise<void> }> = [];

export function check(name: string, run: () => void | Promise<void>): void {
  registry.push({ name, run });
}

/** Runs every registered check, in the order they were registered, and reports each one. */
export async function runAll(): Promise<Result[]> {
  const results: Result[] = [];
  for (const { name, run } of registry) {
    const started = performance.now();
    try {
      await run();
      results.push({ name, ok: true, message: '', ms: performance.now() - started });
    } catch (error) {
      results.push({ name, ok: false, message: error instanceof Error ? error.message : String(error), ms: performance.now() - started });
    }
  }
  return results;
}

const show = (value: unknown): string => {
  if (value === undefined) return 'undefined';
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
};

function same(actual: unknown, expected: unknown): boolean {
  if (Object.is(actual, expected)) return true;
  if (typeof actual !== 'object' || typeof expected !== 'object' || actual === null || expected === null) return false;
  if (Array.isArray(actual) !== Array.isArray(expected)) return false;
  const a = actual as Record<string, unknown>;
  const e = expected as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(e).length && keys.every((key) => key in e && same(a[key], e[key]));
}

export function ok(value: unknown, message = 'expected a true value'): asserts value {
  if (!value) throw new Error(message);
}

export function equal<T>(actual: T, expected: T, message = 'values differ'): void {
  if (!Object.is(actual, expected)) throw new Error(`${message}: expected ${show(expected)}, got ${show(actual)}`);
}

export function notEqual<T>(actual: T, unexpected: T, message = 'values should differ'): void {
  if (Object.is(actual, unexpected)) throw new Error(`${message}: both are ${show(actual)}`);
}

export function deepEqual(actual: unknown, expected: unknown, message = 'values differ'): void {
  if (!same(actual, expected)) throw new Error(`${message}: expected ${show(expected)}, got ${show(actual)}`);
}

export function matches(text: string, pattern: RegExp, message = 'text does not match'): void {
  if (!pattern.test(text)) throw new Error(`${message}: ${pattern} does not match ${show(text)}`);
}

export function doesNotMatch(text: string, pattern: RegExp, message = 'text should not match'): void {
  if (pattern.test(text)) throw new Error(`${message}: ${pattern} matches ${show(text)}`);
}
```

- [ ] **Step 4: Run the first check in plain Node**

The checks use no browser feature, so a throwaway one-liner can run them before the app exists. It is not part of the repository.

Run:

```bash
node --input-type=module -e "for (const f of ['check']) await import('./tests/ui/' + f + '.check.ts'); const { runAll } = await import('./tests/ui/check.ts'); const rs = await runAll(); for (const r of rs) console.log((r.ok ? 'pass  ' : 'FAIL  ') + r.name + (r.ok ? '' : ' -- ' + r.message)); process.exit(rs.every((r) => r.ok) ? 0 : 1)"
```

Expected: `pass  check: the assertions fail when they should and pass when they should`, exit 0.

- [ ] **Step 5: Port the three Node tests**

Port each Node file one to one. Rules: `test('title', fn)` becomes `check('<prefix>title', fn)` where the prefix is `i18n: ` for `tests/i18n.test.ts`, `models: ` for `tests/models.test.ts` and `setup ` for `tests/setup.test.ts` (those titles already start with `C4.1:` to `C4.6:`, so the check is named `setup C4.1: only a first launch ...`); `assert.ok` becomes `ok`, `assert.equal` becomes `equal`, `assert.notEqual` becomes `notEqual`, `assert.deepEqual` becomes `deepEqual`, `assert.match` becomes `matches`, `assert.doesNotMatch` becomes `doesNotMatch`; imports of `node:test` and `node:assert/strict` go away, the relative imports of `../ui/...` become `../../ui/...`. Keep every input and every expected value as it is; do not add or drop an assertion. The ported files are:

`tests/ui/i18n.check.ts` (from `tests/i18n.test.ts`, one check):

```ts
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { check, deepEqual, ok } from './check.ts';

const LOCALES = { 'pt-BR': ptBR, es, fr, de };
const slots = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
const forms = (entry: string | { one: string; other: string }) => (typeof entry === 'string' ? [entry] : [entry.one, entry.other]);

// The type checker already makes every language list every key; this checks what types cannot see.
check('i18n: every language fills every text and keeps the same placeholders as English', () => {
  for (const [code, messages] of Object.entries(LOCALES)) {
    for (const [key, source] of Object.entries(en)) {
      const entry = messages[key as keyof typeof en];
      const wanted = slots(forms(source).at(-1)!);
      for (const text of forms(entry)) {
        ok(text.trim(), `${code} ${key} is empty`);
        for (const slot of slots(text)) ok(wanted.includes(slot), `${code} ${key} has {${slot}}, which English does not`);
      }
      deepEqual(slots(forms(entry).at(-1)!), wanted, `${code} ${key} lost a placeholder`);
    }
  }
});
```

`tests/ui/models.check.ts` (from `tests/models.test.ts`, two checks):

```ts
import { modelOptions } from '../../ui/models.ts';
import { check, deepEqual, ok } from './check.ts';

const gpt = (models: Parameters<typeof modelOptions>[1]) =>
  modelOptions(['codex'], models).filter((o) => o.choice.vendor === 'codex').map((o) => o.label);

check('models: every GPT model is listed by its own name, with no generic ChatGPT entry', () => {
  const models = [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol', isDefault: true },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna', isDefault: false },
  ];
  deepEqual(gpt(models), ['GPT-6.1 Sol', 'GPT-6 Luna']);
  ok(modelOptions(['codex'], models).every((o) => o.choice.vendor === 'auto' || o.choice.model));
});

check('models: before the account lists its models, Auto stands in for them', () => {
  deepEqual(gpt([]), []);
  deepEqual(modelOptions(['codex'], []).map((o) => o.choice.vendor), ['auto']);
});
```

`tests/ui/setup.check.ts` (from `tests/setup.test.ts`, six checks C4.1 to C4.6, each keeping the Node test's own id in its name):

```ts
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { setupCard, shouldOpenSetup, toggleSubscription, type Subscription } from '../../ui/setup.ts';
import { check, deepEqual, doesNotMatch, equal, notEqual, ok } from './check.ts';

const languages: Record<string, Record<string, string | { one: string; other: string }>> = { en, 'pt-BR': ptBR, es, fr, de };
function text(messages: Record<string, unknown>, key: string): string {
  const value = messages[key];
  equal(typeof value, 'string', `${key} must have a translated sentence`);
  ok((value as string).trim(), `${key} must not be empty`);
  return value as string;
}

check('setup C4.1: only a first launch without saved conversations automatically opens the guide', () => {
  const savedChat = [{ id: 'saved-chat', title: 'A saved chat', area: 'chat', updatedAt: 1 }];
  const savedProject = [{ id: 'saved-project-chat', title: 'Project work', area: 'project', projectId: 'project', updatedAt: 1 }];
  for (const { settings, conversations, want } of [
    { settings: {}, conversations: [], want: true },
    { settings: { setup: 'done' as const }, conversations: [], want: false },
    { settings: { setup: 'skipped' as const }, conversations: [], want: false },
    { settings: {}, conversations: savedChat, want: false },
    { settings: {}, conversations: savedProject, want: false },
    { settings: { setup: 'done' as const }, conversations: savedChat, want: false },
    { settings: { setup: 'skipped' as const }, conversations: savedProject, want: false },
  ]) {
    equal(shouldOpenSetup(settings, conversations), want, JSON.stringify({ settings, conversations }));
  }
});

check('setup C4.2: choosing none clears both subscriptions and choosing either subscription clears none', () => {
  const both: readonly Subscription[] = Object.freeze(['claude', 'codex'] as const);
  const none = toggleSubscription(both, 'none');
  deepEqual(none, ['none']);
  deepEqual(both, ['claude', 'codex'], 'selection must not mutate its input');
  deepEqual(toggleSubscription(none, 'claude'), ['claude']);
  deepEqual(toggleSubscription(none, 'codex'), ['codex']);
  deepEqual(toggleSubscription(['claude'], 'codex'), ['claude', 'codex']);
  deepEqual(toggleSubscription(['claude', 'codex'], 'claude'), ['codex']);
});

check('setup C4.3: an installed signed-in subscription has a connected card without install or sign-in actions', () => {
  for (const vendor of ['claude', 'codex'] as const) {
    for (const canInstall of [true, false]) {
      const card = setupCard({ check: { vendor, installed: true, account: 'signedIn', canInstall } }, (key) => text(en, key));
      equal(card.status, 'connected');
      deepEqual(card.actions, [], 'an already connected subscription needs no setup action');
      equal(card.error, undefined);
    }
  }
});

check('setup C4.4: a failed installation translates the error key and keeps technical detail separate', () => {
  const detail = 'checksum mismatch: expected abcd, received 1234; C:\\temporary\\download.exe';
  for (const [language, messages] of Object.entries(languages)) {
    const key = 'error.setup.checksum';
    const sentence = text(messages, key);
    const card = setupCard(
      {
        check: { vendor: 'claude', installed: false, account: 'unknown', canInstall: true },
        progress: { type: 'setup', vendor: 'claude', step: 'failed', error: key, detail },
      },
      (translationKey) => text(messages, translationKey),
    );
    ok(card.error, `${language}: the failed step must expose an error`);
    equal(card.error.message, sentence, `${language}: show the translated sentence`);
    notEqual(card.error.message, key);
    equal(card.error.message.includes(detail), false, 'technical diagnostics belong in the details disclosure');
    equal(card.error.detail, detail, 'preserve the diagnostic for the details disclosure');
  }
});

check('setup C4.5: the empty state directs people to Settings subscriptions without asking them to reopen Faber', () => {
  const body = en['empty.noAiBody'];
  doesNotMatch(body, /reopen|restart|relaunch/i, 'connecting a subscription must work in the running app');
  ok(body.includes(en['settings.open']), 'the empty state must name Settings');
  ok(body.includes(text(en, 'set.subscriptions')), 'the empty state must name the Subscriptions section');
});

check('setup C4.6: all five languages fill every guide key and preserve English placeholders', () => {
  const mandatory = [
    'setup.title', 'setup.body', 'setup.claude', 'setup.chatgpt', 'setup.none',
    'setup.checking', 'setup.notInstalled', 'setup.install', 'setup.officialPage', 'setup.installing',
    'setup.step.prerequisite', 'setup.step.download', 'setup.step.verify', 'setup.step.install', 'setup.step.done',
    'setup.signedOut', 'setup.signIn', 'setup.waiting', 'setup.checkAgain', 'setup.connected',
    'setup.details', 'setup.skip', 'setup.done', 'set.subscriptions', 'set.openSetup',
    'error.setup.download', 'error.setup.checksum', 'error.setup.install',
    'error.setup.unsupported', 'error.setup.signIn', 'error.setup.check',
  ];
  const isGuideKey = (key: string) => key.startsWith('setup.') || key.startsWith('error.setup.') || key.startsWith('set.subscriptions') || key === 'set.openSetup';
  const keys = new Set([...mandatory, ...Object.values(languages).flatMap((messages) => Object.keys(messages).filter(isGuideKey))]);
  const slots = (sentence: string) => [...sentence.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
  for (const key of keys) {
    const source = en as Record<string, string | { one: string; other: string }>;
    ok(source[key] !== undefined, `English must define ${key}`);
    const forms = (entry: unknown): string[] => {
      if (typeof entry === 'string') return [entry];
      ok(entry && typeof entry === 'object', `${key} must contain text`);
      const plural = entry as { one: unknown; other: unknown };
      equal(typeof plural.one, 'string', `${key}.one must contain text`);
      equal(typeof plural.other, 'string', `${key}.other must contain text`);
      return [plural.one as string, plural.other as string];
    };
    const englishForms = forms(source[key]);
    for (const [language, messages] of Object.entries(languages)) {
      const translated = forms(messages[key]);
      equal(translated.length, englishForms.length, `${language} ${key}: preserve plural forms`);
      for (const [index, sentence] of translated.entries()) {
        ok(sentence.trim(), `${language} ${key} is empty`);
        deepEqual(slots(sentence), slots(englishForms[index]), `${language} ${key}: preserve every placeholder`);
      }
    }
  }
});
```

- [ ] **Step 6: Prove a ported check can fail**

In `tests/ui/models.check.ts` change `'GPT-6.1 Sol'` to `'GPT-6.1 SolX'` (only in the first check's `deepEqual`). Run the one-liner of Step 4 with the list `['check', 'i18n', 'models', 'setup']` in place of `['check']`:

```bash
node --input-type=module -e "for (const f of ['check', 'i18n', 'models', 'setup']) await import('./tests/ui/' + f + '.check.ts'); const { runAll } = await import('./tests/ui/check.ts'); const rs = await runAll(); for (const r of rs) console.log((r.ok ? 'pass  ' : 'FAIL  ') + r.name + (r.ok ? '' : ' -- ' + r.message)); process.exit(rs.every((r) => r.ok) ? 0 : 1)"
```

Expected: one `FAIL  models: every GPT model is listed by its own name...` line that quotes both values, the other checks pass, exit 1. Then change `'GPT-6.1 SolX'` back to `'GPT-6.1 Sol'`.

- [ ] **Step 7: Write the entry point and run everything**

Create `tests/ui/index.ts`. The page that loads it is built in Task 11; `invoke('report_checks')` is only a command name until then:

```ts
// Loaded by ui/main.tsx only in a test build (`vite build --mode test`); a production build never contains it.
//
// - `window.__faberChecks()` runs every check in this folder and returns `{ name, ok, message, ms }` for each.
// - `window.__faberText(key, vars, language)` is the screen's own translation, so screen scenarios find elements by role
//   and translated name and never write a sentence of their own.
// - When the shell opened the page as `index.html?checks` (it does that when it was started with `FABER_CHECKS_OUT`),
//   the checks run at load and their result goes to the shell's `report_checks` command, which writes it to that file and
//   ends the app. No DevTools port is involved.

import { invoke } from '@tauri-apps/api/core';
import { LANGUAGES, t } from '../../ui/i18n.ts';
import type { Key } from '../../ui/locales/en.ts';
import { runAll, type Result } from './check.ts';
import './check.check.ts';
import './i18n.check.ts';
import './models.check.ts';
import './setup.check.ts';

type Vars = Record<string, string | number>;

declare global {
  interface Window {
    __faberChecks: () => Promise<Result[]>;
    __faberText: (key: string, vars?: Vars, language?: string | null) => string;
  }
}

const fill = (text: string, vars: Vars) => text.replace(/\{(\w+)\}/g, (all, name: string) => (name in vars ? String(vars[name]) : all));

/** The text for `key`: in the language on screen, or in another one when a scenario asks for it. */
function textFor(key: string, vars: Vars = {}, language?: string | null): string {
  if (!language) return t(key as Key, vars);
  const messages = LANGUAGES.find((l) => l.code === language)?.messages;
  if (!messages) throw new Error(`no language ${language}`);
  const entry = messages[key as Key];
  if (entry === undefined) throw new Error(`no text ${key} in ${language}`);
  if (typeof entry === 'string') return fill(entry, vars);
  return fill(new Intl.PluralRules(language).select(Number(vars.n ?? 0)) === 'one' ? entry.one : entry.other, vars);
}

window.__faberChecks = runAll;
window.__faberText = textFor;

if (new URLSearchParams(location.search).has('checks')) {
  void runAll().then((results) => invoke('report_checks', { json: JSON.stringify(results) }));
}
```

Run: `npm run typecheck && npm run lint`
Expected: no error, no warning.

Run the one-liner of Step 6 again.
Expected: 10 lines starting `pass`, exit 0:
`check: the assertions ...`, `i18n: every language ...`, two `models: ...`, and `setup C4.1` to `setup C4.6`.

---

### Task 11: The shell's test hooks: `test-hooks`, `report_checks`, the override config

**Model: sonnet**

**Files:**
- Create: `src-tauri/tauri.test.conf.json`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `ui/main.tsx`, `.gitignore`, `testkit/src/testbuild.rs` (three more tests)

**Interfaces:**
- Consumes: `testbuild::{vite_args, tauri_args}` and `app::Paths` (Task 9, Task 8); `tests/ui/index.ts` (Task 10).
- Produces:
  - Cargo feature `test-hooks` in `src-tauri/Cargo.toml` (off by default; never in `default`).
  - Under that feature only: the shell command `report_checks(json: String)`, which writes `json` to the file named by the environment variable `FABER_CHECKS_OUT` (first to `<file>.part`, then renamed, so a reader never sees half a report) and then calls `app.exit(0)`; and, when `FABER_CHECKS_OUT` is set, the main window opens `index.html?checks` instead of the default page.
  - `ui/main.tsx` loads `tests/ui/index.ts` only when `import.meta.env.MODE === 'test'`. Vite drops the branch from a production build, so the production `dist/` never holds the checks. `src-tauri/tauri.test.conf.json` is the Tauri override a test build passes with `--config`: it embeds `../dist-test`, runs no `beforeBuildCommand` / `beforeDevCommand` (they run through a shell) and builds no installer.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p faber --features test-hooks -- -D warnings`, `cargo test -p faber-testkit`, `npm run lint`, `npm run typecheck`, `npm run build`.

- [ ] **Step 1: Write the failing tests**

In `testkit/src/testbuild.rs`, add these three tests and the helper `repo_file` at the end of `mod tests`, just before its closing brace:

```rust
    fn repo_file(path: &str) -> String {
        let root = Paths::discover().unwrap().root;
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn the_test_override_config_embeds_dist_test_and_runs_no_hook_and_makes_no_bundle() {
        let config: serde_json::Value = serde_json::from_str(&repo_file("src-tauri/tauri.test.conf.json")).unwrap();
        assert_eq!(config["build"]["frontendDist"], "../dist-test");
        assert!(config["build"].get("beforeBuildCommand").is_some_and(|v| v.is_null()), "the production hook runs through a shell");
        assert!(config["build"].get("beforeDevCommand").is_some_and(|v| v.is_null()));
        assert_eq!(config["bundle"]["active"], false);
    }

    #[test]
    fn the_test_hooks_feature_exists_and_is_never_a_default() {
        let manifest: toml::Table = repo_file("src-tauri/Cargo.toml").parse().unwrap();
        let features = manifest["features"].as_table().expect("src-tauri/Cargo.toml has [features]");
        assert!(features.contains_key("test-hooks"));
        let default = features.get("default").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());
        assert!(!default.unwrap_or_default().contains(&"test-hooks"));
    }

    // The checks live only in a test build. When a production `dist/` exists, nothing in it may carry them.
    #[test]
    fn the_production_dist_holds_no_checks() {
        let dist = Paths::discover().unwrap().root.join("dist");
        if !dist.exists() {
            return;
        }
        fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut all = Vec::new();
        files(&dist, &mut all);
        for file in all.iter().filter(|f| matches!(f.extension().and_then(|e| e.to_str()), Some("js" | "html" | "css"))) {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            for marker in ["__faberChecks", "__faberText", "report_checks"] {
                assert!(!text.contains(marker), "{} contains {marker}", file.display());
            }
        }
    }
```

- [ ] **Step 2: Run them and see two fail**

Run: `cargo test -p faber-testkit testbuild::`
Expected: `the_test_override_config_embeds_dist_test_and_runs_no_hook_and_makes_no_bundle` FAILS (`src-tauri/tauri.test.conf.json: ... cannot find the file`), `the_test_hooks_feature_exists_and_is_never_a_default` FAILS (`src-tauri/Cargo.toml has [features]`), and `the_production_dist_holds_no_checks` passes (it checks only an existing `dist/`). 4 pass, 2 fail.

- [ ] **Step 3: Add the override config**

Create `src-tauri/tauri.test.conf.json`:

```json
{
  "$schema": "../node_modules/@tauri-apps/cli/config.schema.json",
  "build": {
    "frontendDist": "../dist-test",
    "beforeBuildCommand": null,
    "beforeDevCommand": null
  },
  "bundle": {
    "active": false
  }
}
```

- [ ] **Step 4: Add the feature**

In `src-tauri/Cargo.toml`, add this block between `publish = false` and `[build-dependencies]`:

```toml
[features]
# Test builds only (testkit/): a command the screen's checks report through, and the page that runs them. Never default.
test-hooks = []
```

- [ ] **Step 5: Add the command and the page to the shell**

Three edits in `src-tauri/src/main.rs`. Do not touch any other line; the file already imports `AppHandle`, `PathBuf` and the `Res` alias.

1. Add the command after `set_tray` and before `/// Uses setup module to detect CLIs on this computer.`:

```rust
/// Test builds only: the screen's checks (tests/ui/) send their result here. It goes to the file named by
/// `FABER_CHECKS_OUT`, written whole before it appears, and then the app ends.
#[cfg(feature = "test-hooks")]
#[tauri::command]
async fn report_checks(app: AppHandle, json: String) -> Res<()> {
    let out = PathBuf::from(std::env::var_os("FABER_CHECKS_OUT").ok_or("FABER_CHECKS_OUT is not set")?);
    let part = out.with_extension("part");
    std::fs::write(&part, json).and_then(|()| std::fs::rename(&part, &out)).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}
```

2. In `create_window`, replace the line `let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())` with:

```rust
    // A test build started with `FABER_CHECKS_OUT` opens the page that runs the screen's checks and reports them.
    #[cfg(feature = "test-hooks")]
    let url =
        if std::env::var_os("FABER_CHECKS_OUT").is_some() { WebviewUrl::App("index.html?checks".into()) } else { WebviewUrl::default() };
    #[cfg(not(feature = "test-hooks"))]
    let url = WebviewUrl::default();
    let mut builder = WebviewWindowBuilder::new(app, "main", url)
```

3. In `tauri::generate_handler![...]`, replace the last entry `setup::setup_sign_in` with:

```rust
            setup::setup_sign_in,
            #[cfg(feature = "test-hooks")]
            report_checks
```

- [ ] **Step 6: Load the checks in test mode only**

In `ui/main.tsx`, add these lines between the `import { App } ...` line and the `render(...)` line:

```tsx
// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');
```

- [ ] **Step 7: Ignore the test build's folder**

In `.gitignore`, add the line `dist-test/` directly after the line `dist/`. Do not run `git add .gitignore`, `git checkout .gitignore` or `git stash` in this task or any later one: the file may hold a change of the maintainer that is not part of this work, and Task 18 stages exactly one line of it.

- [ ] **Step 8: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit testbuild:: && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p faber --features test-hooks -- -D warnings`
Expected: 6 `testbuild` tests pass; both clippy runs print no warning. The second one compiles `report_checks`; without it a typo inside the `cfg` block would go unseen.

Run: `npm run typecheck && npm run lint && npm run build`
Expected: no error.

- [ ] **Step 9: Check that the checks stay out of production and go into the test build**

Run: `grep -rl "__faberChecks\|__faberText\|report_checks" dist`
Expected: no output and exit code 1 (a production build holds none of them).

Run: `node node_modules/vite/bin/vite.js build --mode test --outDir ../dist-test --emptyOutDir && grep -rl "__faberChecks" dist-test`
Expected: Vite prints its build summary, then one path such as `dist-test/assets/index-XXXX.js`. `git status --short` does not list `dist-test/` (ignored).

---

### Task 12: `checks`: the `ui` suite

**Model: sonnet**

**Files:**
- Create: `testkit/src/checks.rs`
- Modify: `testkit/src/lib.rs` (`pub mod checks;`)

**Interfaces:**
- Consumes: `app::{Ctx, Paths, STARTUP}` and `Ctx::{new, spawn, exited, stderr, finish}`; `sandbox::{CliSlot, Launch}` (with `checks_out`); `report::Outcome`; `plan::Suite`.
- Produces (`faber_testkit::checks`): `struct Check { name: String, ok: bool, message: String, ms: f64 }`; `fn parse_report(&str) -> Result<Vec<Check>, String>` (a JSON array of `{ name, ok, message, ms }`; anything else, or an item without a name or a verdict, is an error, never a pass); `fn to_outcomes(Vec<Check>, filter: Option<&str>) -> Vec<Outcome>` (keeps checks whose name contains the filter); `fn run(&Paths, filter: Option<&str>) -> Vec<Outcome>`: launches the hidden test build with the checks page (`FABER_CHECKS_OUT` set to a file in the sandbox, `FABER_CLI_CLAUDE` and `FABER_CLI_CODEX` both the empty string, no DevTools port), waits up to 30 s for the report, then audits the launch. A report that does not appear, an app that exits first (the message carries its stderr), an empty report and a failed audit are each a failed outcome.
- The first real run happens in Task 14, when `faber-test` exists.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod checks;` to `testkit/src/lib.rs`. Create `testkit/src/checks.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;

    #[test]
    fn the_report_lists_each_check_with_its_verdict_message_and_time() {
        let checks =
            parse_report(r#"[{"name":"i18n: keys","ok":true,"message":"","ms":3.5},{"name":"models","ok":false,"message":"bad","ms":1}]"#)
                .unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0], Check { name: "i18n: keys".into(), ok: true, message: String::new(), ms: 3.5 });
        assert!(!checks[1].ok && checks[1].message == "bad");
    }

    #[test]
    fn a_report_that_is_not_a_list_of_checks_is_an_error_and_never_a_pass() {
        for bad in ["", "not json", "{}", r#"[{"ok":true}]"#, r#"[{"name":"a"}]"#] {
            assert!(parse_report(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_filter_keeps_only_checks_whose_name_contains_the_text() {
        let checks =
            parse_report(r#"[{"name":"i18n: keys","ok":true,"ms":1},{"name":"models: ids","ok":false,"message":"x","ms":1}]"#).unwrap();
        let all = to_outcomes(checks.clone(), None);
        assert_eq!(all.len(), 2);
        let some = to_outcomes(checks, Some("models"));
        assert_eq!((some.len(), some[0].status), (1, Status::Fail));
        assert_eq!(some[0].suite, Suite::Ui);
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit checks::`
Expected: compile errors (`cannot find function parse_report`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! The `ui` suite: the TypeScript checks in `tests/ui/`, run inside a hidden test build of Faber. The app runs them when
//! its window loads, writes the result to the file named by `FABER_CHECKS_OUT` and exits. No DevTools port is involved,
//! so this works on every system.

use crate::app::{Ctx, Paths, STARTUP};
use crate::plan::Suite;
use crate::report::Outcome;
use crate::sandbox::{CliSlot, Launch};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub message: String,
    pub ms: f64,
}

/// The page's report: a JSON array of `{ name, ok, message, ms }`.
pub fn parse_report(text: &str) -> Result<Vec<Check>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("the checks report is not JSON: {e}"))?;
    let list = value.as_array().ok_or("the checks report is not a list")?;
    list.iter()
        .map(|item| {
            Ok(Check {
                name: item["name"].as_str().ok_or("a check has no name")?.to_string(),
                ok: item["ok"].as_bool().ok_or("a check has no verdict")?,
                message: item["message"].as_str().unwrap_or_default().to_string(),
                ms: item["ms"].as_f64().unwrap_or(0.0),
            })
        })
        .collect()
}

pub fn to_outcomes(checks: Vec<Check>, filter: Option<&str>) -> Vec<Outcome> {
    checks
        .into_iter()
        .filter(|c| filter.is_none_or(|f| c.name.contains(f)))
        .map(|c| {
            let time = Some(Duration::from_secs_f64(c.ms.max(0.0) / 1000.0));
            if c.ok { Outcome::pass(Suite::Ui, &c.name, time) } else { Outcome::fail(Suite::Ui, &c.name, time, &c.message) }
        })
        .collect()
}

fn read_report(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|text| !text.trim().is_empty())
}

/// Launches the hidden app in checks mode and returns what it reports. The audit of the launch is part of the result:
/// a leaked process or an outside write fails the suite even if every check passed.
pub fn run(paths: &Paths, filter: Option<&str>) -> Vec<Outcome> {
    let started = Instant::now();
    let mut ctx = match Ctx::new(paths) {
        Ok(ctx) => ctx,
        Err(e) => return vec![Outcome::fail(Suite::Ui, "launch", None, &e)],
    };
    let out = ctx.sb.sandbox.join("checks.json");
    // The `ui` suite never lets the engine look for a CLI: it cannot redirect `/usr/local/bin` on macOS or Linux.
    let launch = Launch { claude: CliSlot::Off, codex: CliSlot::Off, checks_out: Some(out.clone()), ..Launch::default() };
    let result = ctx.spawn(launch).and_then(|id| {
        let deadline = Instant::now() + STARTUP;
        loop {
            if let Some(text) = read_report(&out) {
                return parse_report(&text);
            }
            if let Some(status) = ctx.exited(id) {
                // It may have written the report just before it exited.
                return match read_report(&out) {
                    Some(text) => parse_report(&text),
                    None => Err(format!("the app exited without a checks report ({status}). stderr: {}", ctx.stderr(id))),
                };
            }
            if Instant::now() >= deadline {
                return Err(format!("no checks report after {} s. stderr: {}", STARTUP.as_secs(), ctx.stderr(id)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let audit = ctx.finish();
    let mut outcomes = match result {
        Ok(checks) if checks.is_empty() => vec![Outcome::fail(Suite::Ui, "report", None, "the page reported no checks")],
        Ok(checks) => to_outcomes(checks, filter),
        Err(e) => vec![Outcome::fail(Suite::Ui, "launch", Some(started.elapsed()), &e)],
    };
    if let Err(e) = audit {
        outcomes.push(Outcome::fail(Suite::Ui, "audit of the launch", None, &e));
    }
    outcomes
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit checks:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 3 tests pass. None starts the app.

---

### Task 13: `screen`: the scenario framework

**Model: sonnet**

**Files:**
- Create: `testkit/src/screen/mod.rs`
- Modify: `testkit/src/lib.rs` (`pub mod screen;`)

**Interfaces:**
- Consumes: `app::{Ctx, Paths}` (`Ctx::new`, `finish`, `failure_shot`, the `notes` field), `page::Page`, `plan::Suite`, `report::{Outcome, file_name}`, `sandbox::{CliSlot, Launch}`.
- Produces (`faber_testkit::screen`): `type Result<T = ()> = std::result::Result<T, String>`; `struct Scenario { name: &'static str, run: fn(&mut Ctx) -> Result }`; `fn all() -> Vec<Scenario>`; `fn run_all(paths: &Paths, filter: Option<&str>, done: &mut dyn FnMut(&Outcome)) -> Vec<Outcome>` (runs the scenarios whose name contains the filter, each in its own sandbox; calls `done` as each one ends so the runner prints it while the next one runs; a scenario fails when its own checks fail, when it panics, or when its audit finds a leaked process, an outside write or a changed environment; a failure saves a screenshot under `target/faber-test/shots/`). Helpers the scenarios share: `claude_chat(&Ctx) -> Launch`, `no_cli() -> Launch`, `start_settings(&Ctx, language) -> Result`, `ensure(bool, FnOnce() -> String) -> Result`, `same(actual, expected, what) -> Result`, `has_version_after(text, prefix) -> bool`, `ask(&mut Page, question) -> Result`, `pause(seconds)`, `eventually(what, wait, check) -> Result`.
- In this task `all()` is empty. Task 15 adds the app scenarios and Task 16 the setup scenarios; their modules do not exist yet.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod screen;` to `testkit/src/lib.rs`. Create `testkit/src/screen/mod.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_after_a_prefix_needs_three_numbers() {
        assert!(has_version_after("Claude · v2.1.0", "Claude · v"));
        assert!(has_version_after("x Claude · v10.20.30 more", "Claude · v"));
        assert!(!has_version_after("Claude · v2.1", "Claude · v"));
        assert!(!has_version_after("Claude · vx.y.z", "Claude · v"));
        assert!(!has_version_after("Codex · v1.2.3", "Claude · v"));
    }

    #[test]
    fn ensure_and_same_carry_their_message() {
        assert!(ensure(true, || unreachable!()).is_ok());
        assert_eq!(ensure(false, || "no".into()), Err("no".into()));
        assert!(same(1, 1, "n").is_ok());
        assert_eq!(same("a", "b", "letter"), Err("letter: expected \"b\", got \"a\"".into()));
    }

    #[test]
    fn eventually_gives_up_with_what_it_waited_for() {
        let mut calls = 0;
        assert!(
            eventually("three calls", Duration::from_secs(2), || {
                calls += 1;
                calls == 3
            })
            .is_ok()
        );
        let error = eventually("never", Duration::from_millis(60), || false).unwrap_err();
        assert!(error.contains("waiting for never"), "{error}");
    }

    #[test]
    fn every_scenario_has_a_unique_name() {
        let mut names: Vec<&str> = all().iter().map(|s| s.name).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert!(names.len() == count, "{count} scenarios, {} different names", names.len());
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit screen::`
Expected: compile errors (`cannot find function has_version_after`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! The `screen` suite: scenarios that drive a hidden test build of Faber through the DevTools port. Windows only, because
//! only WebView2 has the port. Each scenario gets its own sandbox, and is audited when it ends.

pub mod app;
pub mod setup;

use crate::app::{Ctx, Paths};
use crate::page::Page;
use crate::plan::Suite;
use crate::report::{Outcome, file_name};
use crate::sandbox::{CliSlot, Launch};
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

pub type Result<T = ()> = std::result::Result<T, String>;

pub struct Scenario {
    pub name: &'static str,
    pub run: fn(&mut Ctx) -> Result,
}

pub fn all() -> Vec<Scenario> {
    app::scenarios().into_iter().chain(setup::scenarios()).collect()
}

/// Runs each scenario (those whose name contains `filter`, if there is one) and audits it. A scenario fails if its own
/// checks fail or if its audit finds a leaked process, an outside write or a changed environment. `done` hears of each
/// outcome as soon as its scenario ends, so the runner can print it while the next one runs.
pub fn run_all(paths: &Paths, filter: Option<&str>, done: &mut dyn FnMut(&Outcome)) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    for scenario in all().iter().filter(|s| filter.is_none_or(|f| s.name.contains(f))) {
        let outcome = run_one(paths, scenario);
        done(&outcome);
        outcomes.push(outcome);
    }
    outcomes
}

fn run_one(paths: &Paths, scenario: &Scenario) -> Outcome {
    let started = Instant::now();
    let mut ctx = match Ctx::new(paths) {
        Ok(ctx) => ctx,
        Err(e) => return Outcome::fail(Suite::Screen, scenario.name, None, &e),
    };
    let result = catch_unwind(AssertUnwindSafe(|| (scenario.run)(&mut ctx))).unwrap_or_else(|_| Err("the scenario panicked".to_string()));
    if result.is_err() {
        ctx.failure_shot(&paths.shots.join(format!("{}.png", file_name(scenario.name))));
    }
    let audit = ctx.finish();
    let time = Some(started.elapsed());
    match (result, audit) {
        (Ok(()), Ok(())) => Outcome::pass_with(Suite::Screen, scenario.name, time, &ctx.notes.join(", ")),
        (Err(e), Ok(())) => Outcome::fail(Suite::Screen, scenario.name, time, &e),
        (Ok(()), Err(a)) => Outcome::fail(Suite::Screen, scenario.name, time, &format!("audit: {a}")),
        (Err(e), Err(a)) => Outcome::fail(Suite::Screen, scenario.name, time, &format!("{e}\naudit: {a}")),
    }
}

// What the scenarios share.

/// A launch with the recorded Claude conversation `chat.jsonl` and no Codex.
pub fn claude_chat(ctx: &Ctx) -> Launch {
    Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/chat.jsonl")),
        ..Launch::default()
    }
}

/// A launch with no CLI at all.
pub fn no_cli() -> Launch {
    Launch::default()
}

/// The settings a scenario starts with: this language, and the first-run guide already done.
pub fn start_settings(ctx: &Ctx, language: &str) -> Result {
    ctx.write_settings(&serde_json::json!({ "language": language, "setup": "done" }))
}

pub fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result {
    if condition { Ok(()) } else { Err(message()) }
}

pub fn same<T: PartialEq + Debug>(actual: T, expected: T, what: &str) -> Result {
    if actual == expected { Ok(()) } else { Err(format!("{what}: expected {expected:?}, got {actual:?}")) }
}

/// Whether `text` holds `prefix` followed by a version such as `1.2.3`.
pub fn has_version_after(text: &str, prefix: &str) -> bool {
    text.match_indices(prefix).any(|(at, _)| {
        let rest = &text[at + prefix.len()..];
        let numbers: Vec<&str> = rest.splitn(3, '.').collect();
        numbers.len() == 3
            && numbers[0..2].iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            && numbers[2].bytes().next().is_some_and(|b| b.is_ascii_digit())
    })
}

pub fn ask(page: &mut Page, question: &str) -> Result {
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, question)?;
    page.press_on(&composer, "Enter")
}

pub fn pause(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

/// Waits until `check` is true, for up to `wait`, polling every 20 ms. For things outside the page, such as a file.
pub fn eventually(what: &str, wait: Duration, mut check: impl FnMut() -> bool) -> Result {
    let deadline = Instant::now() + wait;
    loop {
        if check() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out after {} s waiting for {what}", wait.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
```

The code above is the finished file. Two things in it do not exist yet, so change them now. Delete the two lines `pub mod app;` and `pub mod setup;`, and replace the body of `all()`:

```rust
pub fn all() -> Vec<Scenario> {
    Vec::new()
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit screen:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 4 tests pass (`a_version_after_a_prefix_needs_three_numbers`, `ensure_and_same_carry_their_message`, `eventually_gives_up_with_what_it_waited_for`, `every_scenario_has_a_unique_name`).

---

### Task 14: `runner` and the `faber-test` binary: the first real run

**Model: sonnet**

**Files:**
- Create: `testkit/src/runner.rs`
- Modify: `testkit/src/lib.rs` (`pub mod runner;`), `testkit/src/bin/faber-test.rs` (replace the stub)

**Interfaces:**
- Consumes: `plan::{Options, Suite, changed_paths, jobs, parse_args, select}`, `app::{Paths, emergency_cleanup}`, `checks::run`, `screen::run_all`, `testbuild::{build_test_app, ensure_fake_cli}`, `report::{Outcome, Status, exit_code, line, summary}`.
- Produces (`faber_testkit::runner`): `fn parse_cargo_tests(&str) -> Vec<(String, bool)>` (the `test name ... ok` and `... FAILED` lines of `cargo test`, nothing else); `fn cargo_test_args(jobs: usize, filter: Option<&str>) -> Vec<String>` (`test --workspace -j N [filter] -- --test-threads N`); `fn nothing_matched(&[Outcome], Option<&str>) -> bool`; `fn run(args: &[String]) -> i32` (the whole program: parse arguments, read `git status`, choose suites, install the Ctrl+C handler that calls `emergency_cleanup`, run `rust`, then build the test app once and run `ui` and `screen` (Windows only; elsewhere one `skip` line), print one line per test as it ends, print the summary, and return 0, 1 for any failed or not-run suite or a filter that matched nothing, and 2 for a bad command line). The `rust` suite runs `cargo test` as a child, so it must not run when this program was started by `cargo test` itself; it never is.
- Build failure: the suites that needed the build are printed as `not run`, never `pass`.
- Run it as `cargo t [--all] [--suite rust|ui|screen] [--filter <text>]`. The `faber-test` binary is a one-liner around `runner::run`.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`, and the real runs in Step 5.

- [ ] **Step 1: Write the failing tests**

Add `pub mod runner;` to `testkit/src/lib.rs`. Create `testkit/src/runner.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_that_matched_no_test_anywhere_is_a_mistake_but_no_filter_and_no_tests_is_not_this_check() {
        let one = [Outcome::pass(Suite::Ui, "i18n: keys", None)];
        assert!(nothing_matched(&[], Some("zzz")));
        assert!(!nothing_matched(&one, Some("i18n")));
        assert!(!nothing_matched(&[], None), "without a filter an empty run is another kind of problem");
    }

    #[test]
    fn cargo_test_uses_the_same_number_for_jobs_and_test_threads() {
        assert_eq!(cargo_test_args(3, None), ["test", "--workspace", "-j", "3", "--", "--test-threads", "3"]);
        assert_eq!(cargo_test_args(1, Some("fake")), ["test", "--workspace", "-j", "1", "fake", "--", "--test-threads", "1"]);
    }

    #[test]
    fn test_lines_become_pass_or_fail_and_everything_else_is_ignored() {
        let output = "running 3 tests\ntest plan::tests::a ... ok\ntest plan::tests::b ... FAILED\ntest plan::tests::c ... ignored\n\
                      test result: FAILED. 1 passed; 1 failed; 1 ignored\ntest sandbox::tests::long_name_with ... ok\n";
        assert_eq!(
            parse_cargo_tests(output),
            [
                ("plan::tests::a".to_string(), true),
                ("plan::tests::b".to_string(), false),
                ("sandbox::tests::long_name_with".to_string(), true)
            ]
        );
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-testkit runner::`
Expected: compile errors (`cannot find function nothing_matched`).

- [ ] **Step 3: Write the implementation**

Above the test block:

```rust
//! `faber-test`: reads what changed, picks the suites, runs them and prints one line per test.

use crate::app::{Paths, emergency_cleanup};
use crate::checks;
use crate::plan::{Options, Suite, changed_paths, jobs, parse_args, select};
use crate::report::{Outcome, Status, exit_code, line, summary};
use crate::screen;
use crate::testbuild::{build_test_app, ensure_fake_cli};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const USAGE: &str = "usage: cargo t [--all] [--suite rust|ui|screen] [--filter <text>]";

/// The result lines of `cargo test`: `test some::name ... ok`. Anything that is not a test line is ignored.
pub fn parse_cargo_tests(output: &str) -> Vec<(String, bool)> {
    output
        .lines()
        .filter_map(|l| l.strip_prefix("test "))
        .filter_map(|rest| {
            let (name, verdict) = rest.rsplit_once(" ... ")?;
            match verdict.trim() {
                "ok" => Some((name.to_string(), true)),
                "FAILED" => Some((name.to_string(), false)),
                _ => None,
            }
        })
        .collect()
}

/// The `cargo test` command line of the `rust` suite: `-j` and `--test-threads` both set to the same number.
pub fn cargo_test_args(jobs: usize, filter: Option<&str>) -> Vec<String> {
    let mut args = vec!["test".to_string(), "--workspace".into(), "-j".into(), jobs.to_string()];
    args.extend(filter.map(str::to_string));
    args.extend(["--".to_string(), "--test-threads".into(), jobs.to_string()]);
    args
}

fn run_rust(root: &Path, jobs: usize, filter: Option<&str>) -> Vec<Outcome> {
    let started = Instant::now();
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root).args(cargo_test_args(jobs, filter));
    let output = match command.output() {
        Ok(output) => output,
        Err(e) => return vec![Outcome::fail(Suite::Rust, "cargo test --workspace", None, &format!("cannot start cargo: {e}"))],
    };
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let mut outcomes: Vec<Outcome> = parse_cargo_tests(&text)
        .into_iter()
        .map(|(name, ok)| if ok { Outcome::pass(Suite::Rust, &name, None) } else { Outcome::fail(Suite::Rust, &name, None, "") })
        .collect();
    if !output.status.success() {
        // A build error has no test lines at all; a test failure has its panic message in the output.
        let tail: Vec<&str> = text.lines().collect();
        let shown = tail[tail.len().saturating_sub(60)..].join("\n");
        outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), &shown));
    } else if outcomes.is_empty() {
        // Nothing ran. With a filter that is the caller's mistake to hear about (see `nothing_matched`).
        if filter.is_none() {
            outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), "cargo ran no test at all"));
        }
    } else {
        outcomes.push(Outcome::pass(Suite::Rust, &format!("cargo test --workspace ({} tests)", outcomes.len()), Some(started.elapsed())));
    }
    outcomes
}

fn changed(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "core.quotepath=false", "status", "--porcelain", "-uall"])
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git status failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(changed_paths(&String::from_utf8_lossy(&output.stdout)))
}

/// With `--filter`, a run in which no suite had a test of that name is a mistake, never a pass.
pub fn nothing_matched(outcomes: &[Outcome], filter: Option<&str>) -> bool {
    filter.is_some() && outcomes.is_empty()
}

fn show(outcomes: &[Outcome]) {
    for outcome in outcomes {
        println!("{}", line(outcome));
    }
}

/// Runs the runner. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let opts: Options = match parse_args(args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let paths = match Paths::discover() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let changed = if opts.all || opts.suite.is_some() {
        Vec::new()
    } else {
        match changed(&paths.root) {
            Ok(list) => list,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        }
    };
    let suites = select(&opts, &changed);
    if suites.is_empty() {
        println!("Nothing changed that needs a test (use --all to run everything).");
        return 0;
    }
    // Ctrl+C ends the apps this run started and removes its temporary folders before the runner goes.
    let _ = ctrlc::set_handler(|| {
        emergency_cleanup();
        std::process::exit(130);
    });
    let jobs = jobs();
    let filter = opts.filter.as_deref();
    let mut outcomes: Vec<Outcome> = Vec::new();
    if suites.contains(&Suite::Rust) {
        let found = run_rust(&paths.root, jobs, filter);
        show(&found);
        outcomes.extend(found);
    }
    let (wants_ui, wants_screen) = (suites.contains(&Suite::Ui), suites.contains(&Suite::Screen));
    // Without WebView2's DevTools port, `screen` is skipped, and the app is only built if `ui` needs it.
    let screen_runs = wants_screen && cfg!(windows);
    if wants_ui || wants_screen {
        if wants_screen && !screen_runs {
            let skipped = Outcome::skipped(Suite::Screen, "screen", "needs WebView2's DevTools port, which only Windows has");
            show(std::slice::from_ref(&skipped));
            outcomes.push(skipped);
        }
        if wants_ui || screen_runs {
            match ensure_fake_cli(&paths).and_then(|()| build_test_app(&paths, jobs)) {
                Err(message) => {
                    for suite in [Suite::Ui, Suite::Screen].into_iter().filter(|s| suites.contains(s) && (*s == Suite::Ui || screen_runs)) {
                        let not_run = Outcome::not_run(suite, "the test app", &message);
                        show(std::slice::from_ref(&not_run));
                        outcomes.push(not_run);
                    }
                }
                Ok(()) => {
                    if wants_ui {
                        let found = checks::run(&paths, filter);
                        show(&found);
                        outcomes.extend(found);
                    }
                    if screen_runs {
                        // Each scenario is printed as it ends: the suite takes minutes.
                        outcomes.extend(screen::run_all(&paths, filter, &mut |outcome| println!("{}", line(outcome))));
                    }
                }
            }
        }
    }
    if nothing_matched(&outcomes, filter) {
        println!("No test name contains {:?}. Nothing ran.", filter.unwrap_or_default());
        return 1;
    }
    println!("\n{}", summary(&outcomes));
    if outcomes.iter().any(|o| o.status == Status::Fail) {
        println!("Screenshots of failures are in {}", paths.shots.display());
    }
    exit_code(&outcomes)
}
```

Replace `testkit/src/bin/faber-test.rs`:

```rust
//! The runner: `cargo t` (see `.cargo/config.toml`). All of it is in `faber_testkit::runner`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(faber_testkit::runner::run(&args));
}
```

- [ ] **Step 4: Run the tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit runner:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 3 tests pass.

- [ ] **Step 5: Run the real thing**

Each command below is run from the repository root. The first one that builds the test app takes several minutes; later ones reuse `target/faber-test/`.

1. Run: `cargo t --suite rust --filter proctree`
   Expected: one `pass  rust    proctree::tests::...` line per test (9 on Windows) and a `pass  rust    cargo test --workspace (9 tests) (N s)` line, a summary line, exit 0.
2. Run: `cargo t --suite rust --filter zzzz`
   Expected: `No test name contains "zzzz". Nothing ran.` and exit 1 (Review Focus 2). Check with `echo $?`.
3. Run: `cargo t --suite ui`
   Expected: the test app builds (`vite` then `tauri build`), then 10 lines `pass  ui      ...` (the self-check of the assertions, `i18n`, two `models`, `setup C4.1` to `C4.6`), a summary of 10 passed, exit 0. A hidden window may flash for a moment; none stays.
4. Run: `cargo t --suite ui --filter models`
   Expected: exactly the two `models:` lines, exit 0.
5. Run: `cargo t --suite ui --filter zzzz`
   Expected: `No test name contains "zzzz". Nothing ran.` and exit 1.
6. Prove the suite can fail. In `tests/ui/models.check.ts` change `'GPT-6.1 Sol'` to `'GPT-6.1 SolX'`, run `cargo t --suite ui`, expect a `FAIL  ui      models: every GPT model...` line that quotes both values and exit 1, then change it back and run `cargo t --suite ui --filter models` once more (exit 0).
7. Check that nothing was left behind. Run: `tasklist | grep -i faber` (Windows) or `pgrep -fl faber` (elsewhere): only `faber-test` itself may appear, and not once the command has returned. The system temp folder holds no folder whose name starts with `faber-test`.
8. Check what git reports for new files (Review Focus 3). Run `git -c core.quotepath=false status --porcelain -uall` and confirm that the new files are listed one by one (`?? testkit/src/plan.rs`, not `?? testkit/`). Then run `printf x > "docs/café.md" && git -c core.quotepath=false status --porcelain -uall -- docs; rm "docs/café.md"` and confirm that it prints `?? docs/café.md` without quotes or octal escapes (the form `plan::changed_paths` is tested against in Task 1).


---

### Task 15: `screen/app.rs`: the nine app scenarios

**Model: sonnet**

**Files:**
- Create: `testkit/src/screen/app.rs`
- Modify: `testkit/src/screen/mod.rs` (`pub mod app;` and `all()`)

**Interfaces:**
- Consumes: everything `screen` exports (Task 13), `app::Ctx` (`launch`, `memory_mb`, `shot`, `notes`), `page::{css, role_any}` and the `Page` operations (Task 7). The Node original is `tests/e2e/app.e2e.ts` (read it; it stays in place until Task 17).
- Produces: `screen::app::scenarios() -> Vec<Scenario>`, the nine scenarios of `app.e2e.ts` in the same order and with the same names, ported one to one:
  1. the app opens on an empty chat and fits the window;
  2. a chat streams an answer with the model and the reason, and lands in the sidebar;
  3. editing a message sends it as a branch and keeps the original conversation;
  4. deleting a conversation asks once, then removes it;
  5. with no AI installed, the app says what to do;
  6. the screen speaks the chosen language and switches at once;
  7. appearance changes apply at once and are kept after reopening (unchanged, including the literal theme name `Cockpit`: B3 rewrites it in its own commit);
  8. with no CLI running, Faber stays within its memory budget (250 MB: private working set of the app and its webview, as Task Manager counts it; the note `[idle: N MB]` is printed);
  9. a conversation with a thousand messages opens quickly (under 3 s) and stays within the budget.
- Every text on screen comes from `Page::named`, `labelled` and `text_of`, which ask the page's own `window.__faberText`; the only literal sentences are the CLI footer prefix `Claude · v` (a version label, not a translation key) and the recorded answer's words (`Rayleigh`, `Haiku 5.5`), which come from `tests/fixtures/claude/chat.jsonl`.
- Windows only at run time, but it compiles on every OS: the unit tests below run everywhere.

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`, and the real run in Step 5.

- [ ] **Step 1: Write the failing test**

Create `testkit/src/screen/app.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Port completeness: the nine tests of `tests/e2e/app.e2e.ts`, one to one, the appearance one unchanged.
    #[test]
    fn the_nine_scenarios_of_the_node_suite_are_all_ported() {
        let names: Vec<&str> = scenarios().iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "the app opens on an empty chat and fits the window",
                "a chat streams an answer with the model and the reason, and lands in the sidebar",
                "editing a message sends it as a branch and keeps the original conversation",
                "deleting a conversation asks once, then removes it",
                "with no AI installed, the app says what to do",
                "the screen speaks the chosen language and switches at once",
                "appearance changes apply at once and are kept after reopening",
                "with no CLI running, Faber stays within its memory budget",
                "a conversation with a thousand messages opens quickly and stays within the budget",
            ]
        );
    }
}
```

Add `pub mod app;` to `testkit/src/screen/mod.rs`, on its own line directly above the first `use` line, followed by a blank line.

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p faber-testkit screen::app::`
Expected: compile errors (`cannot find function scenarios in this scope`).

- [ ] **Step 3: Write the scenarios**

Above the test block of `testkit/src/screen/app.rs`:

```rust
//! The app scenarios: what a person sees and does in a chat, in settings, and how much memory the app uses.

use super::{Result, Scenario, ask, claude_chat, ensure, eventually, no_cli, pause, same, start_settings};
use crate::app::Ctx;
use crate::page::{css, role_any};
use serde_json::json;
use std::time::{Duration, Instant};

/// What Faber may use with no CLI running, as Task Manager counts it (private working set of the app and its webview).
/// The product limit is 500 MB when Faber is complete; the test keeps half of it as room for what is still to come.
const BUDGET_MB: f64 = 250.0;

const QUESTION: &str = "In one sentence: why is the sky blue?";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "the app opens on an empty chat and fits the window", run: opens_empty },
        Scenario { name: "a chat streams an answer with the model and the reason, and lands in the sidebar", run: streams_an_answer },
        Scenario { name: "editing a message sends it as a branch and keeps the original conversation", run: edits_as_a_branch },
        Scenario { name: "deleting a conversation asks once, then removes it", run: deletes_after_asking },
        Scenario { name: "with no AI installed, the app says what to do", run: says_what_to_do },
        Scenario { name: "the screen speaks the chosen language and switches at once", run: switches_language },
        Scenario { name: "appearance changes apply at once and are kept after reopening", run: keeps_appearance },
        Scenario { name: "with no CLI running, Faber stays within its memory budget", run: idle_memory },
        Scenario { name: "a conversation with a thousand messages opens quickly and stays within the budget", run: thousand_messages },
    ]
}

fn opens_empty(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&heading)?;
    let foot = page.inner_text(&css(".foot"))?;
    ensure(super::has_version_after(&foot, "Claude · v"), || format!("the footer should name the CLI version, it says {foot:?}"))?;
    let scrolls = page.eval("document.documentElement.scrollHeight > innerHeight")?;
    same(scrolls.as_bool(), Some(false), "the window should not scroll as a whole")?;
    // No system title bar: the top bar carries the window buttons and drags the window.
    for key in ["window.minimize", "window.maximize", "window.close"] {
        let button = page.named("button", key)?.exact();
        page.wait_visible(&button)?;
    }
    same(page.attr(&css("header.top"), "data-tauri-drag-region")?, Some(String::new()), "the top bar drags the window")?;
    ctx.shot(&mut app, "home")
}

fn streams_an_answer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let answer = page.named("article", "answer.label")?;
    page.wait_visible(&answer.clone().css(".actions"))?;
    let text = page.inner_text(&answer)?;
    ensure(text.contains("Rayleigh"), || format!("the answer should explain the sky: {text}"))?;
    ensure(text.contains("Haiku 5.5"), || format!("the answer should name its model: {text}"))?;
    let reason = page.text_of("reason.autoDefault")?;
    ensure(text.contains(&reason), || format!("the answer should say why Auto chose it: {text}"))?;
    ensure(text.contains("at API prices"), || format!("the answer should show the price note: {text}"))?;
    page.wait_visible(&css(".list .item").has_text(QUESTION))?;
    ctx.shot(&mut app, "chat")
}

fn edits_as_a_branch(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.hover(&css(".you"))?;
    let edit = page.named("button", "mine.editLabel")?;
    page.click(&edit)?;
    let send = page.named("button", "mine.sendBranch")?;
    page.click(&send)?;
    page.wait_visible(&actions)?;
    page.wait_for("two conversations in the list", "document.querySelectorAll('.list .item').length === 2")?;
    ctx.shot(&mut app, "branch")
}

fn deletes_after_asking(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.hover(&css(".list .item"))?;
    let x = css(".list .item .x");
    page.click(&x)?;
    let confirm = page.text_of("side.confirmDelete")?;
    same(page.inner_text(&x)?, confirm, "the first click asks")?;
    page.click(&x)?;
    page.wait_for("the list to be empty", "document.querySelectorAll('.list .item').length === 0")
}

fn says_what_to_do(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.noAiTitle")?;
    page.wait_visible(&title)?;
    let body = page.inner_text(&css(".empty p"))?;
    same(body.clone(), page.text_of("empty.noAiBody")?, "the empty state says what to do")?;
    ensure(body.contains("Settings"), || format!("it should point to Settings: {body}"))?;
    ensure(body.to_lowercase().contains("subscriptions"), || format!("it should mention Subscriptions: {body}"))?;
    let lower = body.to_lowercase();
    ensure(!["reopen", "restart", "relaunch"].iter().any(|w| lower.contains(w)), || format!("it should not ask to restart: {body}"))?;
    let send = page.named("button", "composer.send")?;
    same(page.is_disabled(&send)?, true, "Send is disabled with no AI")
}

fn switches_language(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "pt-BR")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named_in("heading", "empty.chatTitle", "pt-BR")?;
    page.wait_visible(&title)?;
    same(page.eval("document.documentElement.lang")?.as_str(), Some("pt-BR"), "the page language")?;
    ctx.shot(&mut app, "home-pt-BR")?;
    let page = &mut app.page;
    let open = page.named_in("button", "settings.open", "pt-BR")?;
    page.click(&open)?;
    let language = crate::page::label(&page.text_in("set.language", "pt-BR")?);
    page.select_option(&language, "es")?;
    let general = page.named_in("heading", "set.general", "es")?;
    page.wait_visible(&general)?;
    same(page.eval("document.documentElement.lang")?.as_str(), Some("es"), "the page language after the switch")
}

fn keeps_appearance(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let background = "getComputedStyle(document.body).backgroundColor";
    let settings = page.named("button", "settings.open")?;
    page.click(&settings)?;
    let appearance = page.named("button", "set.appearance")?;
    page.click(&appearance)?;
    let heading = page.named("heading", "set.appearance")?;
    page.wait_visible(&heading)?;
    let light = page.named("button", "set.modeLight")?;
    page.click(&light)?;
    same(page.eval(background)?.as_str(), Some("rgb(246, 247, 249)"), "light mode background")?;
    page.click(&crate::page::role("button", "Cockpit"))?;
    same(page.eval(background)?.as_str(), Some("rgb(0, 0, 0)"), "Cockpit background")?;
    let compact = page.named("button", "set.densityCompact")?;
    same(page.attr(&compact, "aria-pressed")?, Some("true".to_string()), "Cockpit is compact")?;
    ctx.shot(&mut app, "settings-cockpit")?;
    let page = &mut app.page;
    let projects = page.named("button", "set.projects")?.last();
    page.click(&projects)?;
    page.check(&role_any("checkbox"))?;
    // The file is written in the background; closing earlier could lose the last choice.
    let file = ctx.sb.data.join("settings.json");
    eventually("the choice to reach settings.json", Duration::from_secs(15), || {
        std::fs::read_to_string(&file).is_ok_and(|text| text.contains("\"userHooks\": true"))
    })?;
    ctx.close(app.id)?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    page.wait_for("the saved background", &format!("{background} === 'rgb(0, 0, 0)'"))?;
    // An unfocused offscreen window reopened on the same profile only takes keys after a click, like a real one.
    let composer = page.labelled("composer.message")?;
    page.click(&composer)?;
    page.press("Control+,")?;
    let general = page.named("heading", "set.general")?;
    page.wait_visible(&general)?;
    let projects = page.named("button", "set.projects")?.last();
    page.click(&projects)?;
    same(page.is_checked(&role_any("checkbox"))?, true, "the saved choice")
}

fn idle_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let title = app.page.named("heading", "empty.noAiTitle")?;
    app.page.wait_visible(&title)?;
    pause(2.0);
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("idle: {mb:.0} MB"));
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}

fn thousand_messages(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let at: u64 = 1_790_000_000_000;
    let paragraph = "Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ";
    let fence = "`".repeat(3);
    let id = "5f1c0000-0000-4000-8000-000000000001";
    let messages: Vec<_> = (0..1000u64)
        .map(|i| {
            let message_id = format!("5f1c{i:04x}-0000-4000-8000-0000000000{:02x}", i % 256);
            if i % 2 == 0 {
                json!({ "id": message_id, "role": "user", "text": format!("Question {}: why is the sky blue?", i / 2), "at": at + i })
            } else {
                let text = [paragraph.repeat(6), "- one\n- two".to_string(), format!("{fence}js\nconsole.log({i});\n{fence}")].join("\n\n");
                json!({ "id": message_id, "role": "assistant", "text": text, "at": at + i, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" })
            }
        })
        .collect();
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let conversation =
        json!({ "id": id, "title": "Long one", "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let started = Instant::now();
    page.click(&css(".list .item").has_text("Long one"))?;
    page.wait_visible(&css("article.ai").last())?;
    let open_ms = started.elapsed().as_millis();
    let drawn = page.count(&css("article.ai"))?;
    ensure(drawn <= 25, || format!("{drawn} answers drawn at once"))?;
    // Scrolling up draws the earlier messages, keeping the reader's place.
    page.eval_on(&css(".thread"), "el.scrollTop = 0; return {};")?;
    page.wait_for("earlier answers to be drawn", &format!("document.querySelectorAll('article.ai').length > {drawn}"))?;
    pause(2.0);
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("1000 messages: opened in {open_ms} ms, {mb:.0} MB"));
    ensure(open_ms < 3000, || format!("opening took {open_ms} ms"))?;
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}
```

In `testkit/src/screen/mod.rs` replace the body of `all()`:

```rust
pub fn all() -> Vec<Scenario> {
    app::scenarios()
}
```

- [ ] **Step 4: Run the unit tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit screen:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 5 tests pass in `screen::` (the 4 of Task 13 and `the_nine_scenarios_of_the_node_suite_are_all_ported`).

- [ ] **Step 5: Run the nine scenarios against the real app (Windows)**

Run: `cargo t --suite screen`
Expected: nine lines, each `pass  screen  <name> (N s)`, in the order above; the memory scenarios carry a note such as `[idle: 87 MB]` and `[1000 messages: opened in 113 ms, 113 MB]`; then `9 passed, 0 failed, 0 not run, 0 skipped` and exit 0. Each scenario takes 3 to 12 seconds. If one fails, the line carries the reason and `target/faber-test/shots/<name>.png` holds a screenshot of the page: fix the scenario or the locator, never the Faber screen. Common causes: a wrong role or translation key in `named(...)`, or a missing wait before a read.

- [ ] **Step 6: Prove a scenario can fail, and that a failure leaves a screenshot**

In `opens_empty`, change `Some(false)` in the line `same(scrolls.as_bool(), Some(false), "the window should not scroll as a whole")?;` to `Some(true)`. Run `cargo t --suite screen --filter "opens on an empty chat"`.
Expected: `FAIL  screen  the app opens on an empty chat and fits the window`, the reason `the window should not scroll as a whole: expected Some(true), got Some(false)`, exit 1, and the file `target/faber-test/shots/the_app_opens_on_an_empty_chat_and_fits_the_window.png`. Open the PNG once to see that it shows the app. Revert the change and run the same command again: it passes. Then check that nothing was left behind: `tasklist | grep -i faber` lists no `faber.exe` from `target\faber-test`.

---

### Task 16: `screen/setup.rs`: the six first-run scenarios

**Model: sonnet**

**Files:**
- Create: `testkit/src/screen/setup.rs`
- Modify: `testkit/src/screen/mod.rs` (`pub mod setup;` and `all()`)

**Interfaces:**
- Consumes: as Task 15, plus `app::Opened`, `page::Loc`, `sandbox::{CliSlot, Launch}`, `Ctx::{serve_release, write_settings, close, audit_within, records}` and the loopback `server::Server` (Task 5). The Node original is `tests/e2e/setup.e2e.ts` (with `tests/fixtures/setup/*.setup.json`, which stay).
- Produces: `screen::setup::scenarios() -> Vec<Scenario>`, the six scenarios C5.1 to C5.5 of `setup.e2e.ts` (C5.3 runs twice, once for None and once for Skip):
  - C5.1 installed signed-in Claude connects without reinstalling and answers the first question;
  - C5.2 missing ChatGPT installs from loopback (the fake release server), signs in and answers without reopening;
  - C5.3 None persists, shows connection guidance and never reopens setup automatically;
  - C5.3 Skip persists, shows connection guidance and never reopens setup automatically;
  - C5.4 the open guide with no conversation uses at most 250 MB (three samples are printed, for example `[open guide: 85.2, 81.7, 81.9 MB]`);
  - C5.5 closing the guide leaves no setup child or external writes (the fake CLI's `commands.jsonl` shows every command ran inside the sandbox, and the audit finds no process left).
- These scenarios leave `FABER_CLI_CODEX` absent (`CliSlot::Absent`), the one place the suite does so, so the engine finds the program the guide just installed into the data folder. `FABER_SETUP_SOURCE` is the loopback server's address, never a real host (A19 rule 1: the fake CLI does not implement `account/login/*` and stores no credential, only a marker file).
- R5.5 and the replay check of `setup.e2e.ts` are not scenarios: they are `rust` suite tests (Deviation 8).

**Proof commands that must pass at the end:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p faber-testkit`, and the real run in Step 5.

- [ ] **Step 1: Write the failing test**

Create `testkit/src/screen/setup.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Port completeness: C5.1 to C5.5 of `tests/e2e/setup.e2e.ts` (C5.3 covers None and Skip, one scenario each).
    // R5.5 and the replay check are `rust` suite tests (`sandbox`, `app` and `fake`), because they need no app.
    #[test]
    fn the_setup_scenarios_of_the_node_suite_are_all_ported() {
        let names: Vec<&str> = scenarios().iter().map(|s| s.name).collect();
        let starts: Vec<&str> = names.iter().map(|n| n.split(':').next().unwrap()).collect();
        assert_eq!(starts, ["C5.1", "C5.2", "C5.3", "C5.3", "C5.4", "C5.5"]);
        assert!(names[2].contains("None") && names[3].contains("Skip"));
    }
}
```

Add `pub mod setup;` to `testkit/src/screen/mod.rs`, on the line right after `pub mod app;`.

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p faber-testkit screen::setup::`
Expected: compile errors (`cannot find function scenarios in this scope`).

- [ ] **Step 3: Write the scenarios**

Above the test block of `testkit/src/screen/setup.rs`:

```rust
//! The first-run guide, end to end: connecting an installed CLI, installing one from a loopback release, the choice to
//! have none, memory with the guide open, and that closing it leaves no process or file behind.
//!
//! The guide opens because these scenarios start with settings that do not say the first run is done. A scenario that
//! installs leaves `FABER_CLI_CODEX` absent, so the engine finds the program the guide just put in the data folder.

use super::{Result, Scenario, ask, ensure, eventually, same};
use crate::app::{Ctx, Opened};
use crate::page::{Loc, css};
use crate::sandbox::{CliSlot, Launch};
use std::time::Duration;

const BUDGET_MB: f64 = 250.0;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "C5.1: installed signed-in Claude connects without reinstalling and answers the first question", run: c5_1 },
        Scenario { name: "C5.2: missing ChatGPT installs from loopback, signs in and answers without reopening", run: c5_2 },
        Scenario { name: "C5.3: None persists, shows connection guidance and never reopens setup automatically", run: c5_3_none },
        Scenario { name: "C5.3: Skip persists, shows connection guidance and never reopens setup automatically", run: c5_3_skip },
        Scenario { name: "C5.4: the open guide with no conversation uses at most 250 MB", run: c5_4 },
        Scenario { name: "C5.5: closing the guide leaves no setup child or external writes", run: c5_5 },
    ]
}

/// The state every setup scenario starts from: the guide is due, a loopback release server is up, and Claude is
/// signed in when `connected`.
fn start(ctx: &mut Ctx, connected: bool) -> Result {
    ctx.write_settings(&serde_json::json!({ "language": "en" }))?;
    ctx.serve_release()?;
    if connected {
        std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn recording(ctx: &Ctx, name: &str) -> Option<std::path::PathBuf> {
    Some(ctx.paths.fixtures.join("setup").join(name))
}

fn claude_installed(ctx: &Ctx) -> Launch {
    Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording: recording(ctx, "claude.setup.json"), ..Launch::default() }
}

/// ChatGPT is not installed: its program arrives from the loopback release.
fn codex_missing(ctx: &Ctx) -> Launch {
    Launch { claude: CliSlot::Off, codex: CliSlot::Absent, recording: recording(ctx, "codex.setup.json"), ..Launch::default() }
}

fn guide(app: &mut Opened) -> Result<Loc> {
    Ok(app.page.named("dialog", "setup.title")?.exact())
}

fn wait_for_guide(app: &mut Opened) -> Result {
    let guide = guide(app)?;
    app.page.wait_visible(&guide)
}

fn click_in_guide(app: &mut Opened, key: &str) -> Result {
    let button = guide(app)?.role("button", &app.page.text_of(key)?).exact();
    app.page.click(&button)
}

fn card(vendor: &str) -> Loc {
    css(&format!(".setup-card[data-vendor=\"{vendor}\"]"))
}

fn connected(app: &mut Opened, vendor: &str) -> Result {
    let shown = app.page.text_of("setup.connected")?;
    app.page.wait_visible(&card(vendor).text(&shown).exact())?;
    for key in ["setup.install", "setup.signIn"] {
        let button = card(vendor).role("button", &app.page.text_of(key)?).exact();
        same(app.page.count(&button)?, 0, &format!("{vendor} must not offer {key} once connected"))?;
    }
    Ok(())
}

fn finish(app: &mut Opened) -> Result {
    click_in_guide(app, "setup.done")?;
    let guide = guide(app)?;
    app.page.wait_gone(&guide)
}

fn ready_to_chat(app: &mut Opened) -> Result {
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    let composer = app.page.labelled("composer.message")?;
    same(app.page.is_disabled(&composer)?, false, "the composer is enabled right after setup")
}

fn answered(app: &mut Opened, word: &str) -> Result {
    ask(&mut app.page, "In one sentence: why is the sky blue?")?;
    let answer = app.page.named("article", "answer.label")?.text_re(word);
    app.page.wait_visible(&answer)
}

fn c5_1(ctx: &mut Ctx) -> Result {
    start(ctx, true)?;
    let launch = claude_installed(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.claude")?;
    connected(&mut app, "claude")?;
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.is_empty(), || format!("an existing CLI must not be downloaded again: {requests:?}"))?;
    let lines: Vec<String> = ctx.records().iter().map(|c| c.line()).collect();
    ensure(lines.iter().any(|l| l == "auth status"), || format!("the guide should check the sign-in: {lines:?}"))?;
    ensure(!lines.iter().any(|l| l == "auth login"), || format!("the guide must not sign in again: {lines:?}"))?;
    finish(&mut app)?;
    ready_to_chat(&mut app)?;
    answered(&mut app, "Rayleigh")?;
    ctx.close(app.id)
}

fn c5_2(ctx: &mut Ctx) -> Result {
    start(ctx, false)?;
    let launch = codex_missing(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.chatgpt")?;
    let absent = app.page.text_of("setup.notInstalled")?;
    app.page.wait_visible(&card("codex").text(&absent).exact())?;
    let install = card("codex").role("button", &app.page.text_of("setup.install")?).exact();
    app.page.click(&install)?;
    let sign_in = card("codex").role("button", &app.page.text_of("setup.signIn")?).exact();
    app.page.wait_visible(&sign_in)?;
    let installed = std::fs::read(ctx.sb.data.join("bin/codex.exe")).map_err(|e| format!("the installed program: {e}"))?;
    ensure(installed == ctx.payload, || "the served executable must be the installed CLI".to_string())?;
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.iter().any(|r| r == "/codex.zip"), || format!("the archive should have been downloaded: {requests:?}"))?;
    app.page.click(&sign_in)?;
    let marker = ctx.sb.state.join("codex.signed-in");
    eventually("the fake sign-in to complete", Duration::from_secs(15), || marker.exists())?;
    let again = card("codex").role("button", &app.page.text_of("setup.checkAgain")?).exact();
    if app.page.is_visible(&again)? {
        app.page.click(&again)?;
    }
    connected(&mut app, "codex")?;
    let lines: Vec<String> = ctx.records().iter().map(|c| c.line()).collect();
    ensure(lines.iter().any(|l| l == "login"), || format!("the guide should sign in: {lines:?}"))?;
    ensure(lines.iter().any(|l| l == "login status"), || format!("the guide should check the sign-in: {lines:?}"))?;
    finish(&mut app)?;
    ready_to_chat(&mut app)?;
    answered(&mut app, "scatter")?;
    same(ctx.exited(app.id), None, "the original app must still be running")?;
    ctx.close(app.id)
}

fn c5_3_none(ctx: &mut Ctx) -> Result {
    c5_3(ctx, true)
}

fn c5_3_skip(ctx: &mut Ctx) -> Result {
    c5_3(ctx, false)
}

fn c5_3(ctx: &mut Ctx, none: bool) -> Result {
    start(ctx, false)?;
    let mut app = ctx.launch(Launch::default())?;
    wait_for_guide(&mut app)?;
    if none {
        click_in_guide(&mut app, "setup.none")?;
        finish(&mut app)?;
    } else {
        click_in_guide(&mut app, "setup.skip")?;
    }
    let wanted = if none { "done" } else { "skipped" };
    for launch in 0..2 {
        let title = app.page.named("heading", "empty.noAiTitle")?;
        app.page.wait_visible(&title)?;
        let body = app.page.inner_text(&css(".empty p"))?;
        same(body.clone(), app.page.text_of("empty.noAiBody")?, "no-subscription guidance")?;
        ensure(body.contains("Settings") && body.contains("Subscriptions"), || {
            format!("it should point to Settings and Subscriptions: {body}")
        })?;
        let lower = body.to_lowercase();
        ensure(!["restart", "reopen", "relaunch"].iter().any(|w| lower.contains(w)), || format!("it should not ask to restart: {body}"))?;
        let guide = guide(&mut app)?;
        same(app.page.count(&guide)?, 0, "the saved choice must suppress the guide")?;
        let file = ctx.sb.data.join("settings.json");
        eventually("the setup decision to be durable before closing", Duration::from_secs(15), || {
            std::fs::read_to_string(&file)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|settings| settings["setup"] == wanted)
        })?;
        ctx.close(app.id)?;
        if launch == 0 {
            app = ctx.launch(Launch::default())?;
        }
    }
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.is_empty(), || format!("nothing may be downloaded: {requests:?}"))?;
    let lines: Vec<String> = ctx.records().iter().map(|c| c.line()).collect();
    ensure(lines.is_empty(), || format!("None and Skip must not start a CLI: {lines:?}"))
}

fn c5_4(ctx: &mut Ctx) -> Result {
    start(ctx, false)?;
    let mut app = ctx.launch(Launch::default())?;
    wait_for_guide(&mut app)?;
    let mut samples = Vec::new();
    for _ in 0..3 {
        let (mb, webview) = ctx.memory_mb(app.id)?;
        ensure(webview, || "measure the app and its WebView2 children together".to_string())?;
        ensure(mb.is_finite() && mb > 0.0, || "the private working set must be a real positive measurement".to_string())?;
        samples.push(mb);
    }
    let most = samples.iter().copied().fold(0.0, f64::max);
    let shown: Vec<String> = samples.iter().map(|mb| format!("{mb:.1}")).collect();
    ctx.notes.push(format!("open guide: {} MB", shown.join(", ")));
    ensure(most <= BUDGET_MB, || format!("open guide: {} MB; limit {BUDGET_MB} MB", shown.join(", ")))?;
    let lines: Vec<String> = ctx.records().iter().map(|c| c.line()).collect();
    ensure(lines.is_empty(), || format!("opening the guide must not keep an idle CLI alive: {lines:?}"))?;
    ctx.close(app.id)
}

fn c5_5(ctx: &mut Ctx) -> Result {
    start(ctx, true)?;
    let launch = claude_installed(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.claude")?;
    connected(&mut app, "claude")?;
    // Every other scenario audits the same boundaries when it ends, including after a failed flow. This one also closes
    // while the guide itself is still open, and waits for the status and login children to stop.
    ctx.close(app.id)?;
    ctx.audit_within(Duration::from_secs(5))
}
```

In `testkit/src/screen/mod.rs` replace the body of `all()`:

```rust
pub fn all() -> Vec<Scenario> {
    app::scenarios().into_iter().chain(setup::scenarios()).collect()
}
```

- [ ] **Step 4: Run the unit tests, format, lint**

Run: `cargo fmt --all && cargo test -p faber-testkit screen:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 6 tests pass in `screen::`.

- [ ] **Step 5: Run the six scenarios against the real app (Windows)**

Run: `cargo t --suite screen --filter C5`
Expected: six lines `pass  screen  C5.1: ...` to `C5.5: ...` and `6 passed, 0 failed, 0 not run, 0 skipped`, exit 0.
Then run: `cargo t --suite screen`
Expected: all 15 scenarios pass (about one minute in all).

- [ ] **Step 6: Prove the audit fails a passing scenario**

Temporarily add this line as the first statement of `c5_1` in `testkit/src/screen/setup.rs` (`ctx` is the scenario's `Ctx`):

```rust
    std::fs::write(ctx.sb.enclosing.join("stray.txt"), "outside the sandbox").map_err(|e| e.to_string())?;
```

Run: `cargo t --suite screen --filter C5.1`
Expected: `FAIL  screen  C5.1: ...` with a reason that starts `audit:` and contains `write outside sandbox detected`, even though the scenario's own checks passed. The stray file sits in the run's own temporary folder, which the run removes, so nothing needs cleaning up. Remove the line and run the same command: it passes.

---

### Task 17: Retire the Node suite; CI and documents

**Model: haiku**

**Files:**
- Delete: `tests/i18n.test.ts`, `tests/models.test.ts`, `tests/setup.test.ts`, `tests/e2e/` (`app.e2e.ts`, `setup.e2e.ts`), `tests/support/` (`fake-cli.ts`, `replay.ts`), `tests/fixtures/setup/launcher.cs`
- Modify: `package.json`, `package-lock.json` (by npm), `.github/workflows/ci.yml`, `AGENTS.md`, `README.md`, `docs/design.md`, `docs/roadmap.md`

**Interfaces:**
- Consumes: the finished runner (`cargo t --all` passes with the Node suite still present).
- Produces: a tree with no Node test and no `playwright-core`; `npm run lint` is `oxlint ui tests/ui`; CI runs `cargo t --all`; the documents describe `cargo t`.

**Proof commands that must pass at the end:** `npm run lint`, `npm run typecheck`, `npm run build`, `cargo test -p faber-testkit`, and `cargo t --suite ui`.

- [ ] **Step 1: Check that every Node test has a home**

Nothing may be lost. Confirm each row by name before deleting anything (`cargo t --all` lists them):

| Today | Becomes | Where it shows up |
| :--- | :--- | :--- |
| `tests/i18n.test.ts` | `tests/ui/i18n.check.ts` | `pass  ui      i18n: ...` |
| `tests/models.test.ts` (2 tests) | `tests/ui/models.check.ts` | two `pass  ui      models: ...` |
| `tests/setup.test.ts` (C4.1 to C4.6) | `tests/ui/setup.check.ts` | six `pass  ui      setup C4.n: ...` |
| `tests/e2e/app.e2e.ts` (9 tests) | `testkit/src/screen/app.rs` | nine `pass  screen  ...` |
| `tests/e2e/setup.e2e.ts` C5.1 to C5.5 | `testkit/src/screen/setup.rs` | six `pass  screen  C5...` |
| `tests/e2e/setup.e2e.ts` R5.5 | `sandbox::tests::r5_5_the_audit_rejects_writes_outside_the_sandbox` and `app::tests::r5_5_the_scenario_audit_rejects_writes_outside_the_sandbox` | `pass  rust ...` (inside `cargo t --suite rust --filter r5_5`) |
| `tests/e2e/setup.e2e.ts` "existing recordings still replay unchanged" | `fake::tests::existing_recordings_still_replay_unchanged` | `cargo t --suite rust --filter existing_recordings` |
| `tests/support/{fake-cli,replay}.ts` | `testkit/src/{fake,replay}.rs`, `testkit/src/bin/fake-cli.rs` | |
| `tests/fixtures/setup/launcher.cs` | nothing: the install scenarios serve a zip from `server::zip_of` | |

Run: `cargo t --suite rust --filter r5_5` and `cargo t --suite rust --filter existing_recordings`
Expected: the R5.5 tests (2) and the replay test (1) are listed and pass; exit 0. If a row is missing, stop and fix the port; do not delete.

- [ ] **Step 2: Delete the Node suite**

Run:

```bash
git rm -r tests/i18n.test.ts tests/models.test.ts tests/setup.test.ts tests/e2e tests/support tests/fixtures/setup/launcher.cs
```

Expected: git reports the removal of the files named above and nothing else. `tests/fixtures/setup/claude.setup.json`, `codex.setup.json` and every other recording stay.

- [ ] **Step 3: Change the scripts and drop the dependency**

In `package.json`: change `"lint": "oxlint ui tests"` to `"lint": "oxlint ui tests/ui"`; delete the `"test"` line and the `"test:e2e"` line from `scripts`; delete the `"playwright-core": "1.63.0"` line from `devDependencies`. Keep every other line and the commas valid.

Run: `npm install --package-lock-only --ignore-scripts`
Expected: no error; `git diff --stat package-lock.json` shows only removals (the `playwright-core` and `playwright` entries; about 14 lines). If it shows additions or version changes, run `git checkout package-lock.json` and use the same command again.

Run: `npm ls playwright-core`
Expected: `(empty)` or an error saying it is not in the tree. Either is correct.

- [ ] **Step 4: Replace the CI steps**

In `.github/workflows/ci.yml`, make four changes to the `check` job:

1. In the `webview libraries (Linux)` step, add `xvfb` at the end of the package list, so the line reads:
   `run: sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf xvfb`
2. Delete the line `- run: npm test`.
3. Replace everything from `- run: cargo test --workspace` to the end of the file with:

```yaml
      - run: cargo clippy -p faber --features test-hooks -- -D warnings
      # `rust` and `ui` run on every system; `screen` needs WebView2's DevTools port, so only Windows runs it.
      - name: tests (Linux, under a virtual display)
        if: runner.os == 'Linux'
        run: xvfb-run -a cargo t --all
      - name: tests
        if: runner.os != 'Linux'
        run: cargo t --all
      - uses: actions/upload-artifact@v4
        if: always() && runner.os == 'Windows'
        with:
          name: screenshots
          path: target/faber-test/shots/
          if-no-files-found: ignore
```

   (`cargo clippy --workspace --all-targets -- -D warnings` stays where it is, above the new lines.)
4. Check that `npm run build` is still before the Cargo steps and that the file has no tab characters.

Run: `grep -n "npm test\|test:e2e\|test-results" .github/workflows/ci.yml`
Expected: no output.

- [ ] **Step 5: Update the documents**

Three of these files, `AGENTS.md`, `docs/design.md` and `docs/roadmap.md`, are stored with CRLF line endings; keep them that way. An editor that writes LF turns the whole file into a change: after each edit, `git diff --stat <file>` must show only the lines you changed. `README.md` and `.github/workflows/ci.yml` use LF.

`AGENTS.md`: in "Stack", replace the three-line bullet that begins "- `tests/`: UI tests (`*.test.ts`, Node's runner)" with:

```markdown
- `tests/`: the screen's checks (`tests/ui/*.check.ts`, run inside a hidden test build) and the recordings the fake CLI
  replays (`tests/fixtures/`). The engine's own tests replay the same fixtures (`engine/tests/`).
- `testkit/`: Faber's own test runner (`cargo t`) and the fake CLI. It runs only the tests that depend on what changed,
  launches a hidden test build inside a sandbox and audits every run ([docs/design.md](docs/design.md#screen-tests)).
```

`AGENTS.md`: in "Proof commands", replace the code block ("All exit 0 before any commit:") and the paragraph after it (the one that starts "`npm run test:e2e` (Windows) builds the app" and ends "call a real CLI.") with the following. Leave the paragraph about the maintainer's PC and everything after it as it is:

````markdown
```bash
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p faber --features test-hooks -- -D warnings
cargo t
```

`cargo t` runs only the tests that depend on the files you changed; `cargo t --all` runs everything, as CI does (the
`rust` and `ui` suites everywhere, `screen` on Windows). Run `--all` when the screen or the shell changes. `npm run dev`
opens the app from source with live reload. Unit tests never spawn processes, touch real git or call a real CLI.
````

`README.md`: it has the same code block of commands ("Before any commit, all of these exit 0:") and, after it, the paragraph that starts "On Windows, `npm run test:e2e` also builds the app" and ends "changes." Replace that block and that paragraph with the same seven-command block as in `AGENTS.md` above, followed by this paragraph:

````markdown
`cargo t` runs only the tests that depend on the files you changed; `cargo t --all` runs everything, as CI does. On
Windows it also drives a hidden Faber through WebView2; run `cargo t --all` when the screen or the shell changes.
````

`docs/design.md`: replace the whole last section, from `## How automated runs see the app` to the end of the file, with:

```markdown
## Screen tests

Faber's own test runner (`cargo t`, the `testkit/` crate) tests the screen in two ways, both against a hidden test build
of the app. `FABER_HIDDEN=1` keeps the window offscreen with its own browser profile, so it never takes the user's screen.

- **Checks** (`tests/ui/*.check.ts`) test the screen's own logic. The test build opens `index.html?checks`, runs them when
  the page loads, writes the result to the file named by `FABER_CHECKS_OUT` and exits. No DevTools port is involved, so
  they run on Windows, macOS and Linux (Linux under `xvfb-run -a`).
- **Scenarios** (`testkit/src/screen/`, written in Rust) drive the real app like a person: they read the page, click and
  type, and save screenshots (named checkpoints, and every failure) to `target/faber-test/shots/`. On Windows the UI runs in WebView2, which
  exposes Chromium's DevTools port (`FABER_CDP_PORT`); the macOS and Linux webviews have no such port, so those systems
  skip this suite and run every other check.

Every launch is sandboxed: a temporary home, an empty `PATH`, no provider variables, a dead proxy and no real CLI (a fake
CLI replays the recordings in `tests/fixtures/`). Each scenario is audited afterwards for leaked processes, writes outside
the sandbox and a changed environment. The checks and the hooks that report them exist only in the test build
(`--features test-hooks`, `vite build --mode test`); the production app and its `dist/` contain none of them. CI runs
`cargo t --all` on all three systems and keeps the Windows screenshots as artifacts. Anything outside the page (tray,
notifications, installer, updates) is checked through the operating system's accessibility layer and file versions.
```

`docs/roadmap.md`: in the first paragraph of "Faber's own test system", replace the sentence "Until the Rust program lands, the suite is still Node's built-in runner." (it starts at the end of one line and wraps onto the next) with "The program is `faber-test`, run as `cargo t`." on a line of its own, so that no line passes 120 characters. Keep the rest of the section as it is. The sentence is untrue once this commit lands, which is why this one file outside the spec's list changes (Deviation 14).

- [ ] **Step 6: Verify**

Run: `git grep -n "npm test\|test:e2e\|playwright\|tests/e2e\|tests/support\|test-results/e2e" -- . ':!package-lock.json' ':!docs/superpowers'`
Expected: no output. (`.gitignore` still lists `test-results/`; leave it.)

Run: `git ls-files --eol AGENTS.md docs/design.md docs/roadmap.md README.md .github/workflows/ci.yml`
Expected: the first three say `i/crlf w/crlf`, the last two `i/lf w/lf`; and `git diff --stat AGENTS.md docs/design.md docs/roadmap.md README.md .github/workflows/ci.yml` shows 4 to 30 changed lines per file, never a whole file.

Run: `npm run lint && npm run typecheck && npm run build && cargo test -p faber-testkit && cargo t --suite ui`
Expected: all pass; `cargo t --suite ui` ends with `10 passed, 0 failed, 0 not run, 0 skipped`.

---

### Task 18: Final proof and the one commit

**Model: haiku**

**Files:** none are edited. This task runs the proof commands and makes the commit.

**Interfaces:**
- Consumes: the finished tree of Tasks 1 to 17.
- Produces: one commit on the branch `phase-1-base`. The branch is not pushed: it waits for the maintainer's approval (the three-system CI run is a gate for merging the branch, not for finishing B2).

- [ ] **Step 1: Run every proof command**

Export the Rust paths first (see Global Constraints), then, from the repository root:

```bash
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p faber --features test-hooks -- -D warnings
cargo t --all
```

Expected: every command exits 0. `cargo t --all` prints the `rust` lines, the 10 `ui` lines and the 15 `screen` lines, and ends with `N passed, 0 failed, 0 not run, 0 skipped` and `Screenshots of failures` is not printed. On a PC that is not Windows, `screen` prints one `skip` line instead of the 15 and the exit code is still 0.

If anything fails, fix the cause in the task that owns the file; do not weaken a test.

- [ ] **Step 2: Check what is left behind**

Run: `tasklist | grep -i faber` (Windows) or `pgrep -fl faber` (elsewhere). Expected: no `faber.exe` that lives under `target\faber-test`. (A `faber.exe` from `target\debug` is the maintainer's `npm run dev`; leave it.)

Run: `git status --short`
Expected: exactly these entries, and no others except the maintainer's own pending edits listed in the next step:
- new: `.cargo/`, `testkit/`, `tests/ui/`, `src-tauri/tauri.test.conf.json`;
- modified: `Cargo.toml`, `Cargo.lock`, `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `ui/main.tsx`, `package.json`, `package-lock.json`, `.github/workflows/ci.yml`, `AGENTS.md`, `README.md`, `docs/design.md`, `docs/roadmap.md`, `.gitignore`;
- deleted (staged by `git rm`): the Node suite of Task 17.

- [ ] **Step 3: Stage exactly this change**

`.gitignore` may hold edits that are not part of this work (when the plan was written, `git status` showed `M .gitignore` and `?? .ignore` before any task started). Stage only the `dist-test/` line of it, and never `.ignore`:

```bash
tmp="$(mktemp)"
git show HEAD:.gitignore | sed 's#^dist/$#dist/\ndist-test/#' > "$tmp"
blob="$(git hash-object -w "$tmp")"
git update-index --cacheinfo 100644,"$blob",.gitignore
rm "$tmp"
git diff --cached -- .gitignore
```

Expected: the cached diff for `.gitignore` is exactly one added line, `+dist-test/`. The working file keeps the other edits.

Then stage the rest with explicit paths:

```bash
git add .cargo Cargo.toml Cargo.lock testkit tests/ui src-tauri/Cargo.toml src-tauri/src/main.rs src-tauri/tauri.test.conf.json ui/main.tsx package.json package-lock.json .github/workflows/ci.yml AGENTS.md README.md docs/design.md docs/roadmap.md
git status --short
```

Expected: no `??` entry except `.ignore` (not ours), no `M ` or ` M` entry except `.gitignore` showing `MM` (the working file has more than what is staged), and every deletion of Task 17 under `D `. The plan file `docs/superpowers/plans/2026-10-11-b2-test-runner.md` is not part of this commit (the spec was committed on its own, `48b7165`); leave it untracked unless the maintainer asks.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(test): Faber's own test runner, in Rust (cargo t)

Replace the Node test suite and Playwright with one Rust program, faber-test.
It runs only the suites a change needs: rust (cargo test, half the cores),
ui (TypeScript checks inside a hidden test build, every OS) and screen
(Rust scenarios through WebView2's DevTools port, Windows only).

Every launch of the app is sandboxed (temporary home, empty PATH, no provider
variables, dead proxy, a fake CLI that replays tests/fixtures) and audited
for leaked processes, writes outside the sandbox and a changed environment.
The test hooks exist only behind the test-hooks feature and a Vite test mode;
the production app and its dist/ hold none of them. All tests of the old
suite are ported one to one." -m "Append the attribution trailer your session requires."
```

Replace the second `-m` text with the trailer line your session requires, or drop it if none is required.
Expected: one commit; `git show --stat HEAD` lists the files of Step 3 and no `.ignore`.

- [ ] **Step 5: Leave the branch alone**

Do not push. Report to the maintainer that B2 is done on local Windows proof and that the first run of `cargo t --all` on Linux (under `xvfb-run -a`) and macOS happens in CI after the branch is approved for a push.
