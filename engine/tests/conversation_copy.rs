//! A project conversation works in a separate copy of its own (R201): a git worktree on a `faber/…` branch, made at its
//! first message, removed with the conversation, and never deleted with unsaved work unless the person says so. Git is a
//! scripted executor here: no process and no real repository.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Vendor};
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
    data: tempfile::TempDir,
}

/// An engine on `data`, as the app starts one: the CLI is a silent fake that notes where it starts, git is `runner`.
fn start(data: &Path, cwds: &Arc<Mutex<Vec<PathBuf>>>, runner: impl Executor + Clone + Send + Sync + 'static) -> Engine {
    let inner = support::fake(vec![]).spawn;
    let seen = cwds.clone();
    let spawn: Spawner = Arc::new(move |bin, argv, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        inner(bin, argv, cwd)
    });
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();
    let opts = EngineOptions {
        data_dir: data.into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()) as Box<dyn Executor + Send>)));
    engine
}

fn setup() -> Setup {
    let data = support::canonical_tempdir().unwrap();
    let folder = support::canonical_tempdir().unwrap();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let git = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let engine = start(data.path(), &cwds, Git(git.clone()));
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

/// A copy is removed away from the engine's lock; give it a moment.
async fn gone(path: &Path) -> bool {
    for _ in 0..40 {
        if !path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn r_next_each_project_conversation_works_in_a_copy_of_its_own_on_a_faber_branch() {
    let s = setup();
    let first = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    let second = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    let (a, b) = (s.engine.open(&first).unwrap().copy.unwrap(), s.engine.open(&second).unwrap().copy.unwrap());
    assert!(a.branch.starts_with("faber/fix-the-login-page-") && a.branch != b.branch, "{a:?} {b:?}");
    assert_eq!(PathBuf::from(&a.path), faber_engine::project::copy::home(s.data.path()).join(&first));
    assert_eq!(a.base.as_deref(), Some("refs/heads/main"), "merged is measured against the person's branch");
    let made =
        s.git.lock().unwrap().calls.iter().filter(|c| c.starts_with(&["worktree".into(), "add".into()])).cloned().collect::<Vec<_>>();
    assert_eq!(made[0], ["worktree", "add", "-b", &a.branch, "--", &a.path, HEAD], "a new branch from the head the person has");
    assert!(made.iter().all(|c| !c.iter().any(|a| a == "main")), "never the person's branch: {made:?}");
    // The agent starts in its own copy, never in the person's folder.
    let cwds = s.cwds.lock().unwrap().clone();
    assert_eq!(cwds.len(), 2, "{cwds:?}");
    for (cwd, copy) in cwds.iter().zip([&a, &b]) {
        assert_eq!(cwd.canonicalize().unwrap(), PathBuf::from(&copy.path).canonicalize().unwrap());
    }
    // A branched conversation gets a copy of its own, from the source's branch.
    let message = s.engine.open(&first).unwrap().messages[0].id.clone();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let branched = s
        .engine
        .branch(BranchRequest { conversation_id: first.clone(), message_id: message, text: "Fix it again".into(), choice })
        .unwrap();
    let c = s.engine.open(&branched).unwrap().copy.unwrap();
    assert!(c.branch.starts_with("faber/fix-it-again-") && c.path.ends_with(&branched), "{c:?}");
    let last = s.git.lock().unwrap().calls.iter().rev().find(|c| c.starts_with(&["worktree".into(), "add".into()])).cloned().unwrap();
    assert_eq!(last.last(), Some(&a.branch), "{last:?}");
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_with_separate_copies_off_a_new_conversation_works_in_the_project_folder() {
    let s = setup();
    let before = s.engine.project(&s.project).unwrap();
    assert!(before.separate_copies, "on by default");
    s.engine.update_project(&before, &faber_engine::model::Project { separate_copies: false, ..before.clone() }).unwrap();
    let id = s.engine.send(ask(&s.project, "Look around")).unwrap();
    assert!(s.engine.open(&id).unwrap().copy.is_none());
    assert!(s.git.lock().unwrap().calls.is_empty(), "no git at all");
    let cwd = s.cwds.lock().unwrap()[0].clone();
    assert_eq!(cwd.canonicalize().unwrap(), s.folder.path().canonicalize().unwrap());
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_a_copy_with_unsaved_work_is_deleted_only_when_the_person_says_so() {
    let s = setup();
    let id = s.engine.send(ask(&s.project, "Change things")).unwrap();
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    // Uncommitted changes: refused, and the conversation and its copy stay.
    s.git.lock().unwrap().dirty = true;
    assert_eq!(s.engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    assert!(s.engine.open(&id).is_some() && Path::new(&copy.path).is_dir());
    // Clean, but its commits are on its branch alone: still refused.
    s.git.lock().unwrap().dirty = false;
    assert_eq!(s.engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    // The person said so: the conversation, the copy and its branch go.
    assert_eq!(s.engine.remove_with(&id, true), Ok(()));
    assert!(s.engine.open(&id).is_none());
    assert!(gone(Path::new(&copy.path)).await, "the copy is removed");
    let calls = s.git.lock().unwrap().calls.clone();
    assert!(calls.contains(&vec!["worktree".into(), "remove".into(), "--force".into(), "--".into(), copy.path.clone()]), "{calls:?}");
    assert!(calls.contains(&vec!["branch".into(), "-D".into(), copy.branch.clone()]), "{calls:?}");

    // Work held by another branch is saved, but the agent is still at work there and may write more: it asks.
    let id = s.engine.send(ask(&s.project, "Small fix")).unwrap();
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    s.git.lock().unwrap().held = true;
    assert_eq!(s.engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    // Nothing runs there now, but a message starts the agent again while git looks: the look no longer holds, so it asks.
    s.engine.shutdown();
    let engine = start(s.data.path(), &s.cwds, Git(s.git.clone()));
    let again = engine.clone();
    let more = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "One more change") };
    s.git.lock().unwrap().on_status = Some(Box::new(move || drop(again.send(more))));
    assert_eq!(engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    assert!(engine.open(&id).is_some() && Path::new(&copy.path).is_dir());
    // Once nothing runs there, the delete needs no question.
    engine.shutdown();
    let engine = start(s.data.path(), &s.cwds, Git(s.git.clone()));
    assert_eq!(engine.remove_with(&id, false), Ok(()));
    assert!(gone(Path::new(&copy.path)).await);

    // The agent left its branch and committed on a detached HEAD: that commit is on no branch, so it asks.
    let id = engine.send(ask(&s.project, "Detach")).unwrap();
    engine.shutdown();
    let engine = start(s.data.path(), &s.cwds, Git(s.git.clone()));
    s.git.lock().unwrap().detached = true;
    assert_eq!(engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    // Nor is a copy off its branch ever taken for merged at start.
    s.git.lock().unwrap().merged = true;
    engine.sweep_copies();
    assert!(engine.open(&id).unwrap().copy.is_some_and(|c| Path::new(&c.path).is_dir()));
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_new_conversation_keeps_its_copy_when_another_project_is_opened_meanwhile() {
    let s = setup();
    let other = support::canonical_tempdir().unwrap();
    let other = s.engine.add_project(&other.path().to_string_lossy()).unwrap().id;
    // While git makes the copy, the person opens the other project, and its warm-up throws the draft away.
    let engine = s.engine.clone();
    s.git.lock().unwrap().on_add = Some(Box::new(move || {
        let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
        engine.warm(WarmRequest { conversation_id: None, area: Area::Project, project_id: Some(other), choice });
    }));
    let id = s.engine.send(ask(&s.project, "Start here")).unwrap();
    let copy = s.engine.open(&id).unwrap().copy.expect("the message went to the conversation its copy was made for");
    let cwd = s.cwds.lock().unwrap().last().cloned().unwrap();
    assert_eq!(cwd.canonicalize().unwrap(), PathBuf::from(&copy.path).canonicalize().unwrap(), "never in the project folder");
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_at_start_merged_copies_and_those_of_deleted_conversations_go() {
    let s = setup();
    let kept = s.engine.send(ask(&s.project, "Keep working")).unwrap();
    let merged = s.engine.send(ask(&s.project, "Finished work")).unwrap();
    let (kept_copy, merged_copy) = (s.engine.open(&kept).unwrap().copy.unwrap(), s.engine.open(&merged).unwrap().copy.unwrap());
    s.engine.shutdown();
    // A folder no conversation names any more (its delete could not remove it then).
    let orphan = s.data.path().join("copies").join("00000000-0000-4000-8000-000000000000");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join(".branch"), "faber/old-0000").unwrap();
    // Both branches are in the person's branch, but `kept` still has uncommitted changes. Faber starts again and sweeps.
    s.git.lock().unwrap().merged = true;
    let engine = start(s.data.path(), &s.cwds, DirtyIn(Git(s.git.clone()), PathBuf::from(&kept_copy.path)));
    engine.sweep_copies();
    assert!(!orphan.exists(), "the orphan goes");
    assert!(!Path::new(&merged_copy.path).exists(), "a clean merged copy goes");
    assert!(Path::new(&kept_copy.path).is_dir(), "a copy with changes stays");
    // The merged conversation keeps its record, and its next message makes the copy again in the same folder.
    assert_eq!(engine.open(&merged).unwrap().copy.as_ref(), Some(&merged_copy));
    let again = SendRequest { conversation_id: Some(merged.clone()), ..ask(&s.project, "One more thing") };
    engine.send(again).unwrap();
    assert!(Path::new(&merged_copy.path).is_dir(), "made again at the same place, so the CLI session resumes");
    engine.shutdown();
}

/// The scripted git, with uncommitted changes in one folder only.
#[derive(Clone)]
struct DirtyIn(Git, PathBuf);
impl Executor for DirtyIn {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        if call.cwd == self.1 && call.args.iter().any(|a| a == "status") {
            return Ok(Output { success: true, stdout: b" M a.txt\0".to_vec() });
        }
        self.0.execute(call)
    }
}
