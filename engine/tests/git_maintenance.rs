//! Portable housekeeping uses a scripted Git, never a process or the user's repositories.
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::project::maintenance::{self, Sweeps};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Git {
    common: HashMap<PathBuf, PathBuf>,
    maintained: Vec<PathBuf>,
    fail: bool,
}

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let mut args = call.args.as_slice();
        while args.first().is_some_and(|a| a == "-c") {
            args = &args[2..];
        }
        if args.first().is_some_and(|a| a == "rev-parse") {
            assert!(call.deadline.is_some(), "repository discovery is time-bounded");
            let stdout = self.common.get(&call.cwd).map(|p| format!("{}\n", p.display()).into_bytes());
            return Ok(Output { success: stdout.is_some(), stdout: stdout.unwrap_or_default() });
        }
        assert_eq!(
            args.iter().map(String::as_str).collect::<Vec<_>>(),
            ["maintenance", "run", "--auto", "--quiet", "--task=loose-objects", "--task=incremental-repack", "--task=commit-graph",]
        );
        for pin in ["protocol.allow=never", "core.fsmonitor=false", "pack.threads=1", "pack.windowMemory=16m"] {
            assert!(call.args.iter().any(|a| a == pin), "missing safety/resource pin: {pin}");
        }
        self.maintained.push(call.cwd.clone());
        Ok(Output { success: !self.fail, stdout: vec![] })
    }
}

#[test]
fn maintenance_visits_every_project_but_shares_work_between_its_copies() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let roots: Vec<_> = ["project", "copy", "other"].iter().map(|name| root.join(name)).collect();
    for folder in &roots {
        std::fs::create_dir(folder).unwrap();
    }
    let common = root.join("project/.git");
    let other = root.join("other/.git");
    for git in [&common, &other] {
        std::fs::create_dir_all(git.join("objects")).unwrap();
    }
    let mut git = Git {
        common: HashMap::from([(roots[0].clone(), common.clone()), (roots[1].clone(), common), (roots[2].clone(), other)]),
        ..Git::default()
    };
    assert_eq!(maintenance::run(&mut git, &roots, || true), 2);
    assert_eq!(git.maintained, [roots[0].clone(), roots[2].clone()]);
    git.fail = true;
    assert_eq!(maintenance::run(&mut git, &roots, || true), 0, "unsupported maintenance does not break a project");
}

#[test]
fn maintenance_does_nothing_while_work_is_active_or_git_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let roots = [dir.path().canonicalize().unwrap()];
    let mut git = Git::default();
    assert_eq!(maintenance::run(&mut git, &roots, || false), 0);
    assert_eq!(maintenance::run(&mut git, &roots, || true), 0);
    assert!(git.maintained.is_empty());
}

#[test]
fn maintenance_rechecks_idle_after_repository_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let common = root.join(".git");
    std::fs::create_dir_all(common.join("objects")).unwrap();
    let mut git = Git { common: HashMap::from([(root.clone(), common)]), ..Git::default() };
    let mut calls = 0;
    assert_eq!(
        maintenance::run(&mut git, &[root], || {
            calls += 1;
            calls == 1
        }),
        0
    );
    assert!(git.maintained.is_empty());
}

#[test]
fn maintenance_rejects_relative_git_directories_and_linked_object_stores() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut git = Git { common: HashMap::from([(root.clone(), PathBuf::from("../elsewhere"))]), ..Git::default() };
    assert_eq!(maintenance::run(&mut git, std::slice::from_ref(&root), || true), 0);
    let common = root.join(".git");
    let outside = root.join("outside");
    std::fs::create_dir(&common).unwrap();
    std::fs::create_dir(&outside).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, common.join("objects")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, common.join("objects")).unwrap();
    git.common.insert(root.clone(), common);
    assert_eq!(maintenance::run(&mut git, &[root], || true), 0);
    assert!(git.maintained.is_empty());
}

#[test]
fn maintenance_rejects_links_in_the_pack_and_commit_graph_directories() {
    for name in ["pack", "info"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let common = root.join(".git");
        let outside = root.join("outside");
        std::fs::create_dir_all(common.join("objects")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let link = common.join("objects").join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, &link).unwrap();
        let mut git = Git { common: HashMap::from([(root.clone(), common)]), ..Git::default() };
        assert_eq!(maintenance::run(&mut git, &[root], || true), 0);
        assert!(git.maintained.is_empty(), "housekeeping must not write through objects/{name}");
    }
}

#[test]
fn idle_sweeps_are_single_flight_and_rate_limited_without_a_timer() {
    let now = Instant::now();
    let mut sweeps = Sweeps::default();
    assert!(sweeps.begin(now));
    assert!(!sweeps.begin(now + Duration::from_secs(7200)), "a live sweep is never duplicated");
    sweeps.finish();
    assert!(!sweeps.begin(now + Duration::from_secs(3599)));
    assert!(sweeps.begin(now + Duration::from_secs(3600)));
}
