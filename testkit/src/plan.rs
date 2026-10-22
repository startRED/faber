//! What a run covers: which suites a change needs, what the command line asked for, and how many cores to use.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suite {
    Rust,
    Ui,
    Screen,
    /// The two Clippy proofs. Only `--suite lint` runs them; `--all` and the changed files never pick them.
    Lint,
}

impl Suite {
    pub const ALL: [Suite; 3] = [Suite::Rust, Suite::Ui, Suite::Screen];

    pub fn name(self) -> &'static str {
        match self {
            Suite::Rust => "rust",
            Suite::Ui => "ui",
            Suite::Screen => "screen",
            Suite::Lint => "lint",
        }
    }

    pub fn parse(name: &str) -> Option<Suite> {
        Suite::ALL.into_iter().chain([Suite::Lint]).find(|s| s.name() == name)
    }
}

/// Half the logical cores on a person's PC, which games and other work share; every core on CI, where the runner is the
/// job's alone (R163). A positive Cargo job limit bounds both, and it is never fewer than one.
pub fn job_count(logical: usize, cargo_limit: Option<&str>, whole: bool) -> usize {
    let share = if whole { logical } else { logical / 2 }.max(1);
    let limit = cargo_limit.and_then(|value| value.parse::<usize>().ok()).filter(|n| *n > 0);
    share.min(limit.unwrap_or(share))
}

/// GitHub Actions, like most CI services, sets `CI=true`.
pub fn on_ci() -> bool {
    std::env::var("CI").is_ok_and(|value| value == "true")
}

pub fn jobs() -> usize {
    job_count(std::thread::available_parallelism().map_or(1, |n| n.get()), std::env::var("CARGO_BUILD_JOBS").ok().as_deref(), on_ci())
}

/// The suites that cover a set of changed paths (relative to the repository, `/` separators).
pub fn suites_for(paths: &[String]) -> BTreeSet<Suite> {
    paths.iter().flat_map(|p| suites_for_path(p)).collect()
}

/// One rule of `testkit/suites.json`, which CI's `scripts/ci-scope.mjs` reads too, so both pick the same suites (R163).
#[derive(serde::Deserialize)]
struct Rule {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    suffixes: Vec<String>,
    suites: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Rules {
    rules: Vec<Rule>,
}

fn rules() -> &'static [Rule] {
    static RULES: std::sync::OnceLock<Rules> = std::sync::OnceLock::new();
    &RULES.get_or_init(|| serde_json::from_str(include_str!("../suites.json")).expect("testkit/suites.json is valid")).rules
}

/// The first rule that matches wins; a path no rule matches needs every suite.
fn suites_for_path(path: &str) -> Vec<Suite> {
    let path = path.trim_start_matches("./");
    let matches = |rule: &&Rule| {
        rule.files.iter().any(|f| f == path)
            || rule.prefixes.iter().any(|p| path.starts_with(p.as_str()))
            || rule.suffixes.iter().any(|s| path.ends_with(s.as_str()))
    };
    match rules().iter().find(matches) {
        Some(rule) => rule.suites.iter().filter_map(|name| Suite::parse(name)).collect(),
        None => Suite::ALL.to_vec(),
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
    /// `--shard 2/3`: run only the second of three even parts of the screen suite, so CI can split it.
    pub shard: Option<(usize, usize)>,
    /// `--build-only`: build the test app, the fake CLI and their helpers, run nothing. CI builds once and hands the
    /// programs to its screen parts (R163).
    pub build_only: bool,
    /// `--prebuilt`: use the programs a `--build-only` run left instead of building them.
    pub prebuilt: bool,
    /// `--costs <file>`: the seconds of each screen scenario to deal `--shard` by, instead of the table in the repository.
    /// CI hands every part the same file (R165).
    pub costs: Option<std::path::PathBuf>,
    /// `--claims`: check the lane's claims and show a red `phase-1-base`, build and run nothing. It is the local proof
    /// that takes no build slot; the suites run in CI on the queued commit (R183).
    pub claims: bool,
    /// `--failed`: run only what failed in the last run (`target/faber-test/run.log`), see [`Failed`].
    pub failed: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--all" => opts.all = true,
            "--suite" => {
                let name = it.next().ok_or("--suite needs a name: rust, ui, screen or lint")?;
                opts.suite = Some(Suite::parse(name).ok_or_else(|| format!("unknown suite {name}: use rust, ui, screen or lint"))?);
            }
            "--filter" => opts.filter = Some(it.next().ok_or("--filter needs some text")?.clone()),
            "--shard" => opts.shard = Some(shard(it.next().ok_or("--shard needs a part such as 1/3")?)?),
            "--build-only" => opts.build_only = true,
            "--prebuilt" => opts.prebuilt = true,
            "--claims" => opts.claims = true,
            "--failed" => opts.failed = true,
            "--costs" => opts.costs = Some(it.next().ok_or("--costs needs a file such as target/screen-costs.json")?.into()),
            other => {
                return Err(format!(
                    "unknown argument {other}: use --all, --suite <name>, --filter <text>, --failed, --shard <i/n>, --costs <file>, --build-only, --prebuilt or --claims"
                ));
            }
        }
    }
    if opts.build_only && opts.prebuilt {
        return Err("--build-only and --prebuilt cannot go together".into());
    }
    if opts.failed && (opts.all || opts.filter.is_some() || opts.shard.is_some() || opts.build_only) {
        return Err("--failed picks what to run by itself: it goes with --suite and --prebuilt only".into());
    }
    Ok(opts)
}

/// What failed in the last run, read from the `FAIL  <suite>  <name>` lines of its log (`report::line`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Failed {
    /// The exact names of the `rust` tests that failed.
    pub rust: Vec<String>,
    /// `cargo test` itself failed (it did not build, or a test program died): the whole suite runs again.
    pub rust_whole: bool,
    /// A `ui` check failed, or its launch did: the suite is short, so it runs whole.
    pub ui: bool,
    pub lint: bool,
    /// The exact names of the `screen` scenarios that failed.
    pub screen: Vec<String>,
}

impl Failed {
    pub fn suites(&self) -> BTreeSet<Suite> {
        [
            (Suite::Rust, self.rust_whole || !self.rust.is_empty()),
            (Suite::Ui, self.ui),
            (Suite::Screen, !self.screen.is_empty()),
            (Suite::Lint, self.lint),
        ]
        .into_iter()
        .filter_map(|(suite, any)| any.then_some(suite))
        .collect()
    }
}

/// The name of a logged line without the time `report::line` adds (` (4.2 s)`, ` (31 ms)`). A name that ends in
/// parentheses of its own keeps them.
fn without_time(name: &str) -> &str {
    let timed = name.strip_suffix(')').and_then(|rest| rest.rsplit_once(" (")).and_then(|(name, time)| {
        let number = time.strip_suffix(" ms").or_else(|| time.strip_suffix(" s"))?;
        (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit() || b == b'.')).then_some(name)
    });
    timed.unwrap_or(name)
}

/// The tests that failed in a run, from its log: the lines that start with `FAIL`, a suite name padded to six letters, and
/// the test's name. The reasons under a failure and the output of Cargo that the log also holds are not read.
pub fn parse_failed(log: &str) -> Failed {
    let mut failed = Failed::default();
    for line in log.lines().filter_map(|l| l.strip_prefix("FAIL  ")) {
        let (Some(suite), Some(name)) = (line.get(..6).map(str::trim), line.get(6..).and_then(|rest| rest.strip_prefix("  "))) else {
            continue;
        };
        let name = without_time(name.trim_end()).trim();
        match suite {
            "rust" if name == "cargo test --workspace" => failed.rust_whole = true,
            "rust" => failed.rust.push(name.to_string()),
            "ui" => failed.ui = true,
            "lint" => failed.lint = true,
            "screen" => failed.screen.push(name.to_string()),
            _ => {}
        }
    }
    failed
}

fn shard(text: &str) -> Result<(usize, usize), String> {
    let wrong = || format!("--shard {text}: use a part such as 1/3, from 1 to the number of parts");
    let (part, parts) = text.split_once('/').ok_or_else(wrong)?;
    let (part, parts) = (part.parse::<usize>().map_err(|_| wrong())?, parts.parse::<usize>().map_err(|_| wrong())?);
    if part == 0 || part > parts {
        return Err(wrong());
    }
    Ok((part, parts))
}

/// The part (from 0) each item goes to: the costliest first, each to the lightest part so far. The same list gives the
/// same parts on every machine, so CI's jobs together run each item exactly once.
pub fn deal(costs: &[u32], parts: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..costs.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(costs[i]));
    let (mut load, mut part) = (vec![0u32; parts.max(1)], vec![0; costs.len()]);
    for i in order {
        let lightest = (0..load.len()).min_by_key(|&p| load[p]).unwrap_or(0);
        part[i] = lightest;
        load[lightest] += costs[i];
    }
    part
}

/// `--suite` wins, then `--all`, then whatever the changed paths ask for.
pub fn select(opts: &Options, changed: &[String]) -> BTreeSet<Suite> {
    match (opts.suite, opts.all) {
        (Some(suite), _) => BTreeSet::from([suite]),
        (None, true) => Suite::ALL.into_iter().collect(),
        (None, false) => suites_for(changed),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn configured_build_jobs_limit_the_runner_without_changing_its_default() {
        assert_eq!(super::job_count(16, Some("4"), false), 4);
        assert_eq!(super::job_count(16, Some("2"), false), 2);
        for configured in [None, Some("0"), Some("-1"), Some("bad")] {
            assert_eq!(super::job_count(16, configured, false), 8);
        }
        let logical = std::thread::available_parallelism().unwrap().get();
        assert_eq!(super::jobs(), super::job_count(logical, std::env::var("CARGO_BUILD_JOBS").ok().as_deref(), super::on_ci()));
    }
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }
    fn names(set: &BTreeSet<Suite>) -> Vec<&'static str> {
        set.iter().map(|s| s.name()).collect()
    }

    #[test]
    fn half_the_cores_and_never_fewer_than_one() {
        assert_eq!(job_count(16, None, false), 8);
        assert_eq!(job_count(5, None, false), 2);
        assert_eq!(job_count(2, None, false), 1);
        assert_eq!(job_count(1, None, false), 1);
        assert_eq!(job_count(0, None, false), 1);
        assert_eq!(job_count(16, Some("4"), false), 4);
        assert_eq!(job_count(16, Some("1"), false), 1);
        assert_eq!(job_count(2, Some("4"), false), 1);
        assert_eq!(job_count(16, Some("0"), false), 8);
        assert_eq!(job_count(16, Some("invalid"), false), 8);
        assert_eq!(job_count(4, None, true), 4);
        assert_eq!(job_count(3, None, true), 3);
        assert_eq!(job_count(0, None, true), 1);
        assert_eq!(job_count(4, Some("2"), true), 2);
    }

    #[test]
    fn the_bootstrap_keeps_a_default_limit_without_overriding_the_environment() {
        let config: toml::Table = include_str!("../../.cargo/config.toml").parse().unwrap();
        assert_eq!(config["build"]["jobs"].as_integer(), Some(4));
        assert!(!config["alias"]["t"].as_str().unwrap().split_whitespace().any(|arg| arg == "-j"));
    }

    #[test]
    fn the_runner_respects_a_positive_cargo_build_limit() {
        assert_eq!(job_count(16, Some("4"), false), 4);
        assert_eq!(job_count(16, Some("2"), false), 2);
        assert_eq!(job_count(2, Some("4"), false), 1);
        for value in [None, Some("0"), Some("invalid"), Some("-2")] {
            assert_eq!(job_count(16, value, false), 8);
        }
        let default = job_count(std::thread::available_parallelism().map_or(1, |n| n.get()), None, on_ci());
        let limit = std::env::var("CARGO_BUILD_JOBS").ok().and_then(|value| value.parse::<usize>().ok()).filter(|limit| *limit > 0);
        assert_eq!(jobs(), limit.map_or(default, |limit| default.min(limit)));
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

    // The queue and `cargo t` read the same rules (R165): a small change to the screen or to the engine runs only its own
    // suites, a test file of the engine or the shell runs only the Rust suite, and the runner, the build, CI or a file no
    // rule knows runs everything.
    #[test]
    fn a_small_change_runs_only_its_suites_and_the_tools_the_build_or_an_unknown_file_run_everything() {
        let only: [(&str, &[&str]); 10] = [
            ("ui/Thread.tsx", &["ui", "screen"]),
            ("ui/locales/en.ts", &["ui", "screen"]),
            ("engine/src/codex.rs", &["rust", "screen"]),
            ("engine/examples/cache_probe.rs", &["rust", "screen"]),
            ("engine/tests/codex_session.rs", &["rust"]),
            ("engine/tests/support/mod.rs", &["rust"]),
            ("engine/examples/route_eval.rs", &["rust"]),
            ("engine/examples/bench.rs", &["rust"]),
            ("bench/pure.rs", &["rust"]),
            ("tests/ui/thread.check.ts", &["ui"]),
        ];
        for (path, want) in only {
            assert_eq!(names(&suites_for(&paths(&[path]))), want, "{path}");
        }
        for path in [
            "testkit/src/screen/app.rs",
            "testkit/suites.json",
            "Cargo.toml",
            "Cargo.lock",
            "engine/Cargo.toml",
            ".cargo/config.toml",
            ".github/workflows/ci.yml",
            "scripts/ci-scope.mjs",
            "src-tauri/src/lib.rs",
            "a-new-folder/file.txt",
        ] {
            assert_eq!(names(&suites_for(&paths(&[path]))), ["rust", "ui", "screen"], "{path}");
        }
    }

    #[test]
    fn every_suite_the_rules_name_exists_and_every_rule_matches_something() {
        for rule in rules() {
            for name in &rule.suites {
                assert!(Suite::parse(name).is_some_and(|s| s != Suite::Lint), "testkit/suites.json names {name}");
            }
            assert!(!(rule.files.is_empty() && rule.prefixes.is_empty() && rule.suffixes.is_empty()), "a rule that matches nothing");
        }
    }

    #[test]
    fn documents_pick_nothing_at_any_depth_and_nothing_means_no_suites() {
        for path in ["docs/design.md", "docs/superpowers/plans/x.md", "README.md", "engine/NOTES.md", "ui/locales/README.md"] {
            assert!(suites_for(&paths(&[path])).is_empty(), "{path}");
        }
        assert!(suites_for(&[]).is_empty());
        assert_eq!(names(&suites_for(&paths(&["docs/product-map.md"]))), ["rust"], "a test reads the map");
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
            Options { all: false, suite: Some(Suite::Ui), filter: Some("i18n".into()), ..Options::default() }
        );
        assert!(args(&["--suite"]).is_err());
        assert!(args(&["--suite", "gpu"]).is_err());
        assert_eq!(args(&["--suite", "lint"]).unwrap().suite, Some(Suite::Lint));
        assert!(args(&["--filter"]).is_err());
        assert!(args(&["--everything"]).is_err());
        assert_eq!(args(&["--shard", "2/3"]).unwrap().shard, Some((2, 3)));
        assert!(args(&["--build-only"]).unwrap().build_only);
        assert!(args(&["--suite", "screen", "--prebuilt"]).unwrap().prebuilt);
        assert!(args(&["--build-only", "--prebuilt"]).is_err());
        assert!(args(&["--claims"]).unwrap().claims);
        assert_eq!(args(&["--costs", "target/screen-costs.json"]).unwrap().costs, Some("target/screen-costs.json".into()));
        assert!(args(&["--costs"]).is_err());
        for wrong in [&["--shard"][..], &["--shard", "0/3"], &["--shard", "4/3"], &["--shard", "3"], &["--shard", "a/b"]] {
            assert!(args(wrong).is_err(), "{wrong:?}");
        }
    }

    #[test]
    fn dealing_gives_every_item_one_part_and_evens_out_the_slow_ones() {
        let costs = [5, 45, 5, 35, 27, 5, 15, 14, 5, 14, 5, 5];
        let parts = deal(&costs, 3);
        assert_eq!(parts.len(), costs.len());
        let load = |p| costs.iter().zip(&parts).filter(|(_, q)| **q == p).map(|(c, _)| c).sum::<u32>();
        let loads = [load(0), load(1), load(2)];
        assert_eq!(loads.iter().sum::<u32>(), 180);
        assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 5, "within one cheap scenario: {loads:?}");
        assert_eq!(deal(&costs, 3), parts, "the same list deals the same way every time");
        assert_eq!(deal(&costs, 1), vec![0; costs.len()]);
    }

    #[test]
    fn suite_beats_all_and_all_beats_the_changes() {
        let changed = paths(&["README.md"]);
        assert!(select(&Options::default(), &changed).is_empty());
        assert_eq!(select(&Options { all: true, ..Options::default() }, &changed).len(), 3);
        let one = Options { all: true, suite: Some(Suite::Screen), ..Options::default() };
        assert_eq!(names(&select(&one, &changed)), ["screen"]);
    }

    #[test]
    fn failed_reads_the_names_of_the_tests_that_failed_from_a_log() {
        let log = "pass  ui      i18n: keys (4 ms)\n\
                   FAIL  ui      models: ids (1 ms)\n          bad\n\
                   FAIL  rust    project::budget::tests::spends_once\n          panicked at engine/src/x.rs:1\n\
                   FAIL  rust    cargo test --workspace (1.5 s)\n\
                   FAIL  screen  a chat streams an answer with the model and the reason, and lands in the sidebar (6.2 s)\n          the reason\n\
                   FAIL  screen  a project conversation opens as a chat with its composer, and opening it starts no CLI (R160) (31 ms)\n\
                   FAIL  lint    cargo clippy --workspace --all-targets -- -D warnings (40.0 s)\n\
                   FAIL  lane    `x` overlaps something\n\
                   test result: FAILED. 1 passed; 1 failed\n";
        let failed = parse_failed(log);
        assert_eq!(failed.rust, ["project::budget::tests::spends_once"]);
        assert!(failed.rust_whole && failed.ui && failed.lint);
        assert_eq!(
            failed.screen,
            [
                "a chat streams an answer with the model and the reason, and lands in the sidebar",
                "a project conversation opens as a chat with its composer, and opening it starts no CLI (R160)"
            ],
            "the time goes, the parentheses of a name stay"
        );
        assert_eq!(names(&failed.suites()), ["rust", "ui", "screen", "lint"]);
        let none = parse_failed("pass  ui      i18n: keys (4 ms)\n0 passed, 0 failed, 0 not run, 0 skipped\n");
        assert_eq!(none, Failed::default());
        assert!(none.suites().is_empty());
    }

    #[test]
    fn failed_goes_only_with_a_suite_and_prebuilt() {
        let args = |list: &[&str]| parse_args(&paths(list));
        assert!(args(&["--failed"]).unwrap().failed);
        assert!(args(&["--failed", "--suite", "screen", "--prebuilt"]).is_ok());
        for wrong in
            [&["--failed", "--all"][..], &["--failed", "--filter", "x"], &["--failed", "--shard", "1/2"], &["--failed", "--build-only"]]
        {
            assert!(args(wrong).is_err(), "{wrong:?}");
        }
    }
}
