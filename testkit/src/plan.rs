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
    configured_jobs(std::thread::available_parallelism().map_or(1, |n| n.get()), std::env::var("CARGO_BUILD_JOBS").ok().as_deref())
}

fn configured_jobs(logical: usize, configured: Option<&str>) -> usize {
    configured.and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0).unwrap_or_else(|| half_cores(logical))
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

#[cfg(test)]
mod tests {
    #[test]
    fn configured_build_jobs_limit_the_runner_without_changing_its_default() {
        assert_eq!(super::configured_jobs(16, Some("4")), 4);
        assert_eq!(super::configured_jobs(16, Some("2")), 2);
        for configured in [None, Some("0"), Some("-1"), Some("bad")] {
            assert_eq!(super::configured_jobs(16, configured), 8);
        }
        let logical = std::thread::available_parallelism().unwrap().get();
        assert_eq!(super::jobs(), super::configured_jobs(logical, std::env::var("CARGO_BUILD_JOBS").ok().as_deref()));
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
