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
            ["ls-files", ..] => say(""),
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
        data_dir: data.path().into(),
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
