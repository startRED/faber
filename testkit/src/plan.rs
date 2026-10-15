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

/// Half the logical cores, bounded by a positive Cargo job limit, never fewer than one.
pub fn job_count(logical: usize, cargo_limit: Option<&str>) -> usize {
    let half = (logical / 2).max(1);
    let limit = cargo_limit.and_then(|value| value.parse::<usize>().ok()).filter(|n| *n > 0);
    half.min(limit.unwrap_or(half))
}

pub fn jobs() -> usize {
    job_count(std::thread::available_parallelism().map_or(1, |n| n.get()), std::env::var("CARGO_BUILD_JOBS").ok().as_deref())
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
    /// `--shard 2/3`: run only the second of three even parts of the screen suite, so CI can split it.
    pub shard: Option<(usize, usize)>,
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
            "--shard" => opts.shard = Some(shard(it.next().ok_or("--shard needs a part such as 1/3")?)?),
            other => return Err(format!("unknown argument {other}: use --all, --suite <name>, --filter <text> or --shard <i/n>")),
        }
    }
    Ok(opts)
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
        assert_eq!(super::job_count(16, Some("4")), 4);
        assert_eq!(super::job_count(16, Some("2")), 2);
        for configured in [None, Some("0"), Some("-1"), Some("bad")] {
            assert_eq!(super::job_count(16, configured), 8);
        }
        let logical = std::thread::available_parallelism().unwrap().get();
        assert_eq!(super::jobs(), super::job_count(logical, std::env::var("CARGO_BUILD_JOBS").ok().as_deref()));
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
        assert_eq!(job_count(16, None), 8);
        assert_eq!(job_count(5, None), 2);
        assert_eq!(job_count(2, None), 1);
        assert_eq!(job_count(1, None), 1);
        assert_eq!(job_count(0, None), 1);
        assert_eq!(job_count(16, Some("4")), 4);
        assert_eq!(job_count(16, Some("1")), 1);
        assert_eq!(job_count(2, Some("4")), 1);
        assert_eq!(job_count(16, Some("0")), 8);
        assert_eq!(job_count(16, Some("invalid")), 8);
    }

    #[test]
    fn the_bootstrap_keeps_a_default_limit_without_overriding_the_environment() {
        let config: toml::Value = include_str!("../../.cargo/config.toml").parse().unwrap();
        assert_eq!(config["build"]["jobs"].as_integer(), Some(4));
        assert!(!config["alias"]["t"].as_str().unwrap().split_whitespace().any(|arg| arg == "-j"));
    }

    #[test]
    fn the_runner_respects_a_positive_cargo_build_limit() {
        assert_eq!(job_count(16, Some("4")), 4);
        assert_eq!(job_count(16, Some("2")), 2);
        assert_eq!(job_count(2, Some("4")), 1);
        for value in [None, Some("0"), Some("invalid"), Some("-2")] {
            assert_eq!(job_count(16, value), 8);
        }
        let default = job_count(std::thread::available_parallelism().map_or(1, |n| n.get()), None);
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
            Options { all: false, suite: Some(Suite::Ui), filter: Some("i18n".into()), shard: None }
        );
        assert!(args(&["--suite"]).is_err());
        assert!(args(&["--suite", "gpu"]).is_err());
        assert!(args(&["--filter"]).is_err());
        assert!(args(&["--everything"]).is_err());
        assert_eq!(args(&["--shard", "2/3"]).unwrap().shard, Some((2, 3)));
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
}
