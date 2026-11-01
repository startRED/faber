//! A project conversation works in a separate copy of its own (R201): a git worktree on a `faber/…` branch, made at its
//! first message, removed with the conversation, and never deleted with unsaved work unless the person says so. Git is a
//! scripted executor here: no process and no real repository.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, BranchRequest, Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HEAD: &str = "1111111111111111111111111111111111111111";
/// A commit the agent made on a detached HEAD in its copy: no branch holds it.
const LOOSE: &str = "2222222222222222222222222222222222222222";

/// What the scripted repository holds: whether the copy has uncommitted changes, and which refs hold its branch's tip.
#[derive(Default)]
struct Repo {
    root: PathBuf,
    dirty: bool,
    /// Besides the copy's own branch: a ref of the person's that holds its commits (saved), or none (unsaved).
    held: bool,
    /// The person's branch has the copy's work (merged).
    merged: bool,
    /// The agent left its branch: the copy's HEAD is a commit no branch has.
    detached: bool,
    tracked: String,
    ignored: bool,
    /// Runs once while git makes a copy, as if something else happened in the engine at that moment.
    on_add: Option<Box<dyn FnOnce() + Send>>,
    /// Runs once while git reads a copy's status.
    on_status: Option<Box<dyn FnOnce() + Send>>,
    calls: Vec<Vec<String>>,
}

#[derive(Clone)]
struct Git(Arc<Mutex<Repo>>);

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        // The fixed `-c` pairs come first; what follows is the command.
        let mut args = call.args.as_slice();
        while args.first().is_some_and(|a| a == "-c") {
            args = &args[2..];
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut repo = self.0.lock().unwrap();
        repo.calls.push(args.iter().map(|a| a.to_string()).collect());
        let say = |text: &str| Ok(Output { success: true, stdout: text.as_bytes().to_vec() });
        let no = || Ok(Output { success: false, stdout: vec![] });
        match args.as_slice() {
            ["rev-parse", "--show-toplevel", "--show-prefix"] => say(&format!("{}\n\n", repo.root.display())),
            ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => say(&format!("{HEAD}\n")),
            ["symbolic-ref", "--quiet", "HEAD"] if call.cwd == repo.root => say("refs/heads/main\n"),
            ["symbolic-ref", "--quiet", "HEAD"] if repo.detached => no(),
            ["symbolic-ref", "--quiet", "HEAD"] => {
                let branch = std::fs::read_to_string(call.cwd.join(".branch")).unwrap_or_default();
                say(&format!("refs/heads/{branch}\n"))
            }
            ["symbolic-ref", "--quiet", "--short", "HEAD"] => say(&std::fs::read_to_string(call.cwd.join(".branch")).unwrap_or_default()),
            ["rev-parse", "--path-format=absolute", "--git-common-dir"] => say(&format!("{}\n", repo.root.join(".git").display())),
            ["rev-parse", "--verify", "--quiet", r] if r.starts_with("refs/heads/faber/") => {
                let name = r.trim_start_matches("refs/heads/");
                let made = repo.calls.iter().any(|c| c.iter().any(|a| a == name) && c.first().is_some_and(|a| a == "worktree"));
                let gone = repo.calls.iter().any(|c| c.first().is_some_and(|a| a == "branch") && c.iter().any(|a| a == name));
                if made && !gone { say(&format!("{HEAD}\n")) } else { no() }
            }
            ["rev-parse", "--verify", "--quiet", "HEAD"] => say(&format!(
                "{}
",
                if repo.detached { LOOSE } else { HEAD }
            )),
            ["worktree", "add", "-b", branch, "--", path, _] | ["worktree", "add", "--", path, branch] => {
                std::fs::create_dir_all(path).unwrap();
                std::fs::write(Path::new(path).join(".branch"), branch).unwrap();
                if let Some(hook) = repo.on_add.take() {
                    drop(repo);
                    hook();
                }
                say("")
            }
            ["status", "--porcelain", "-z"] if repo.on_status.is_some() => {
                let hook = repo.on_status.take().unwrap();
                drop(repo);
                hook();
                say("")
            }
            ["status", "--porcelain", "-z"] => say(if repo.dirty { " M src/lib.rs\0" } else { "" }),
            ["for-each-ref", "--contains", tip, ..] if *tip == LOOSE => say(""),
            ["for-each-ref", "--contains", _, "--format=%(refname)", ..] => {
                let own = repo.calls.iter().rev().find_map(|c| c.iter().find(|a| a.starts_with("faber/")).cloned()).unwrap_or_default();
                say(&format!("refs/heads/{own}\n{}", if repo.held { "refs/heads/main\n" } else { "" }))
            }
            ["for-each-ref", "--format=%(upstream)", _] => say("\n"),
            ["merge-base", "--is-ancestor", ..] => {
                if repo.merged {
                    say("")
                } else {
                    no()
                }
            }
            ["merge-base", ..] | ["diff", ..] => no(),
            ["worktree", "remove", "--force", "--", path] => {
                let _ = std::fs::remove_dir_all(path);
                say("")
            }
            ["worktree", "prune"] | ["branch", "-D", _] => say(""),
            ["ls-files", ..] => say(&repo.tracked),
            ["check-ignore", ..] => {
                if repo.ignored {
                    say("")
                } else {
                    no()
                }
            }
            other => panic!("unexpected git command {other:?}"),
        }
    }
}

struct Setup {
    engine: Engine,
    git: Arc<Mutex<Repo>>,
    cwds: Arc<Mutex<Vec<PathBuf>>>,
    project: String,
    folder: tempfile::TempDir,
    data: support::DataFolder,
}

/// An engine on `data`, as the app starts one: the CLI is a silent fake that notes where it starts, git is `runner`.
fn start(data: &support::DataFolder, cwds: &Arc<Mutex<Vec<PathBuf>>>, runner: impl Executor + Clone + Send + Sync + 'static) -> Engine {
    start_seen(data, cwds, runner, Arc::new(|_| {}))
}

/// The same, with every event the engine emits handed to `events`.
fn start_seen(
    data: &support::DataFolder,
    cwds: &Arc<Mutex<Vec<PathBuf>>>,
    runner: impl Executor + Clone + Send + Sync + 'static,
    events: Arc<dyn Fn(AppEvent) + Send + Sync>,
) -> Engine {
    let inner = support::fake(vec![]).spawn;
    let seen = cwds.clone();
    let spawn: Spawner = Arc::new(move |bin, argv, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        inner(bin, argv, cwd)
    });
    let opts = EngineOptions {
        data_dir: data.path().canonicalize().unwrap(),
        #[cfg(windows)]
        chat_dir: data.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, events).unwrap();
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()) as Box<dyn Executor + Send>)));
    engine
}

fn setup() -> Setup {
    let data = support::DataFolder::new();
    let folder = support::canonical_tempdir().unwrap();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let git = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let engine = start(&data, &cwds, Git(git.clone()));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    Setup { engine, git, cwds, project, folder, data }
}

fn ask(project: &str, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.into()),
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    }
}

/// Sends and waits for the copy: the message is recorded at once, but the copy is made away from the engine's lock and the
/// agent starts only in it.
async fn sent(engine: &Engine, req: SendRequest) -> String {
    let id = engine.send(req).unwrap();
    ready(engine, &id).await;
    id
}

async fn ready(engine: &Engine, id: &str) {
    // A copy named on the conversation but not attached yet has no repository (`root`).
    let made = || engine.open(id).and_then(|c| c.copy).is_some_and(|c| !c.root.is_empty() && Path::new(&c.path).is_dir());
    support::until(&format!("the copy of {id} was never made"), made).await;
}

async fn agent_started(cwds: &Mutex<Vec<PathBuf>>, count: usize) {
    support::until("the agent did not start after history capture", || cwds.lock().unwrap().len() >= count).await;
}

async fn agent_started_in(cwds: &Mutex<Vec<PathBuf>>, copy: &Path) {
    let expected = copy.canonicalize().unwrap();
    let started = || cwds.lock().unwrap().iter().any(|cwd| cwd.canonicalize().is_ok_and(|cwd| cwd == expected));
    support::until(&format!("the agent did not start in its copy {expected:?}"), started).await;
}

#[tokio::test]
async fn a_running_copy_keeps_its_rebuildable_folders_during_the_startup_sweep() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Keep working")).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&copy.path)).await;
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep"), "still in use").unwrap();
    s.engine.sweep_copies();
    assert_eq!(std::fs::read_to_string(target.join("keep")).unwrap(), "still in use");
    s.engine.shutdown();
}

fn cleanup_fixture() -> (support::DataFolder, tempfile::TempDir, faber_engine::project::copy::WorkCopy, Git) {
    let data = support::DataFolder::new();
    let source = support::canonical_tempdir().unwrap();
    let path = faber_engine::project::copy::home(&data.path().canonicalize().unwrap()).join("idle-copy");
    std::fs::create_dir_all(&path).unwrap();
    let copy = faber_engine::project::copy::WorkCopy {
        path: path.to_string_lossy().into_owned(),
        root: source.path().to_string_lossy().into_owned(),
        branch: "faber/idle-copy".into(),
        sub: String::new(),
        base: None,
    };
    let git = Git(Arc::new(Mutex::new(Repo { ignored: true, ..Repo::default() })));
    (data, source, copy, git)
}

fn clean(data: &support::DataFolder, copy: &faber_engine::project::copy::WorkCopy, git: &mut Git, days: u64, running: bool) -> (u64, u64) {
    faber_engine::project::copy::cleanup(
        git,
        &data.path().canonicalize().unwrap(),
        "idle-copy",
        copy,
        Duration::from_secs(days * 86400),
        running,
        |remove| remove(),
    )
}

#[test]
fn idle_ignored_build_folders_are_removed_but_the_copy_stays_openable() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let root = Path::new(&copy.path);
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::write(root.join("target/debug/cache"), "cache").unwrap();
    std::fs::write(root.join("source.rs"), "source").unwrap();
    assert_eq!(clean(&data, &copy, &mut git, 6, false), (0, 0));
    assert!(root.join("target").exists());
    assert_eq!(clean(&data, &copy, &mut git, 7, false), (5, 3));
    assert!(!root.join("target").exists());
    assert_eq!(std::fs::read_to_string(root.join("source.rs")).unwrap(), "source");
    assert!(git.0.lock().unwrap().calls.iter().any(|c| c == &["check-ignore", "--quiet", "--", "target/"]));
}

#[test]
fn tracked_dist_and_nonignored_build_folders_are_kept() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let dist = Path::new(&copy.path).join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("app.js"), "tracked").unwrap();
    git.0.lock().unwrap().tracked = "Dist/app.js\0".into();
    assert_eq!(clean(&data, &copy, &mut git, 7, false).0, 0);
    assert!(dist.join("app.js").exists());
    git.0.lock().unwrap().tracked.clear();
    git.0.lock().unwrap().ignored = false;
    assert_eq!(clean(&data, &copy, &mut git, 7, false).0, 0);
    assert!(dist.join("app.js").exists());
}

#[test]
fn running_old_copy_is_never_inspected() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir_all(&target).unwrap();
    assert_eq!(clean(&data, &copy, &mut git, 30, true), (0, 0));
    assert!(target.exists());
    assert!(git.0.lock().unwrap().calls.is_empty());
}

#[test]
fn paths_outside_the_copy_and_the_source_are_refused() {
    let (data, source, mut copy, mut git) = cleanup_fixture();
    std::fs::create_dir_all(source.path().join("target")).unwrap();
    std::fs::write(source.path().join("target/keep"), "source").unwrap();
    copy.path = source.path().to_string_lossy().into_owned();
    assert_eq!(clean(&data, &copy, &mut git, 30, false), (0, 0));
    assert!(source.path().join("target/keep").exists());
    assert!(git.0.lock().unwrap().calls.is_empty());
}

#[test]
fn only_known_subprojects_up_to_two_levels_are_cleaned() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let root = Path::new(&copy.path);
    git.0.lock().unwrap().tracked = "apps/web/package.json\0unknown/readme.md\0apps/deep/web/package.json\0".into();
    for sub in ["apps/web", "unknown", "apps/deep/web"] {
        std::fs::create_dir_all(root.join(sub).join("node_modules")).unwrap();
        std::fs::write(root.join(sub).join("node_modules/cache"), "cache").unwrap();
    }
    assert_eq!(clean(&data, &copy, &mut git, 7, false), (5, 2));
    assert!(!root.join("apps/web/node_modules").exists());
    assert!(root.join("unknown/node_modules/cache").exists());
    assert!(root.join("apps/deep/web/node_modules/cache").exists());
}

#[test]
fn linked_build_folder_and_nested_link_keep_their_targets() {
    let (data, source, copy, mut git) = cleanup_fixture();
    let outside = source.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep"), "outside").unwrap();
    let root = Path::new(&copy.path);
    std::fs::create_dir_all(root.join("dist")).unwrap();
    for link in [root.join("target"), root.join("dist/link")] {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, &link).unwrap();
    }
    assert_eq!(clean(&data, &copy, &mut git, 30, false).0, 0);
    assert!(root.join("target").symlink_metadata().is_ok());
    assert!(root.join(".faber-clean-dist/dist/link").symlink_metadata().is_ok());
    assert_eq!(std::fs::read_to_string(outside.join("keep")).unwrap(), "outside");
}

#[tokio::test]
async fn startup_sweep_reclaims_old_copy_but_keeps_a_recent_copy() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Build later")).await;
    let mut conv = s.engine.open(&id).unwrap();
    let copy = conv.copy.clone().unwrap();
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("cache"), "cache").unwrap();
    s.engine.shutdown();
    s.git.lock().unwrap().ignored = true;
    let restarted = start(&s.data, &s.cwds, Git(s.git.clone()));
    restarted.sweep_copies();
    assert!(target.join("cache").exists(), "a recent copy must stay untouched");
    restarted.shutdown();
    conv.updated_at = 1;
    std::fs::write(s.data.path().join("conversations").join(format!("{id}.json")), serde_json::to_vec(&conv).unwrap()).unwrap();
    let restarted = start(&s.data, &s.cwds, Git(s.git.clone()));
    restarted.sweep_copies();
    assert!(!target.exists(), "the startup sweep must clean an old idle copy");
    assert!(Path::new(&copy.path).is_dir());
    assert!(restarted.open(&id).unwrap().copy.is_some());
    restarted.shutdown();
}

#[test]
fn cleanup_rechecks_idle_before_removing_any_file() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep"), "running").unwrap();
    assert_eq!(
        faber_engine::project::copy::cleanup(
            &mut git,
            &data.path().canonicalize().unwrap(),
            "idle-copy",
            &copy,
            Duration::from_secs(30 * 86400),
            false,
            |_| {}
        ),
        (0, 0)
    );
    assert!(target.join("keep").exists());
}

#[test]
fn a_reply_starting_after_the_rename_keeps_its_new_build_folder() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("old"), "old").unwrap();
    let mut calls = 0;
    let result =
        faber_engine::project::copy::cleanup(&mut git, data.path(), "idle-copy", &copy, Duration::from_secs(7 * 86400), false, |rename| {
            calls += 1;
            rename();
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("new"), "running reply").unwrap();
        });
    assert_eq!(result, (3, 2));
    assert_eq!(calls, 1);
    assert_eq!(std::fs::read_to_string(target.join("new")).unwrap(), "running reply");
    assert!(!Path::new(&copy.path).join(".faber-clean-target").exists());
}

#[test]
fn a_link_at_the_copy_root_is_refused_without_touching_its_destination() {
    let (data, source, copy, mut git) = cleanup_fixture();
    let root = Path::new(&copy.path);
    std::fs::remove_dir(root).unwrap();
    std::fs::create_dir_all(source.path().join("target")).unwrap();
    std::fs::write(source.path().join("target/keep"), "source").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(source.path(), root).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(source.path(), root).unwrap();
    assert_eq!(clean(&data, &copy, &mut git, 30, false), (0, 0));
    assert!(source.path().join("target/keep").exists());
    assert!(git.0.lock().unwrap().calls.is_empty());
}

#[test]
fn the_copy_itself_cannot_be_used_as_the_source_of_a_cleanup() {
    let (data, _source, mut copy, mut git) = cleanup_fixture();
    copy.root = copy.path.clone();
    std::fs::create_dir_all(Path::new(&copy.path).join("target")).unwrap();
    assert_eq!(clean(&data, &copy, &mut git, 30, false), (0, 0));
    assert!(Path::new(&copy.path).join("target").exists());
    assert!(git.0.lock().unwrap().calls.is_empty());
}

#[test]
fn the_exact_five_rebuildable_names_are_cleaned_in_one_walk_each() {
    let (data, _source, copy, mut git) = cleanup_fixture();
    let root = Path::new(&copy.path);
    for name in ["target", "node_modules", ".next", "dist", "__pycache__", "__pitch", ".cache"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(root.join(name).join("cache"), "cache").unwrap();
    }
    assert_eq!(clean(&data, &copy, &mut git, 7, false), (25, 10));
    for name in ["target", "node_modules", ".next", "dist", "__pycache__"] {
        assert!(!root.join(name).exists());
        assert!(!root.join(format!(".faber-clean-{name}")).exists());
    }
    for name in ["__pitch", ".cache"] {
        assert_eq!(std::fs::read_to_string(root.join(name).join("cache")).unwrap(), "cache");
    }
}
