//! A project conversation works in a separate copy of its own (R201): a git worktree on a `faber/…` branch, made at its
//! first message, removed with the conversation, and never deleted with unsaved work unless the person says so. Git is a
//! scripted executor here: no process and no real repository.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::router::Choice;
use faber_engine::store::Store;
use faber_engine::verify::{Level, Request, Runner, Spec, Started, Verifier};
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
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
    before_branch_delete: Option<Box<dyn FnOnce() + Send>>,
    on_branch_lookup: Option<Box<dyn FnOnce() + Send>>,
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
        if matches!(args.as_slice(), ["branch", "-D", _])
            && let Some(hook) = repo.before_branch_delete.take()
        {
            drop(repo);
            hook();
            repo = self.0.lock().unwrap();
        }
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
                if let Some(hook) = repo.on_branch_lookup.take() {
                    hook();
                }
                let name = r.trim_start_matches("refs/heads/");
                let exists = repo.calls.iter().rev().find_map(|c| match c.as_slice() {
                    [worktree, add, new, branch, ..] if worktree == "worktree" && add == "add" && new == "-b" && branch == name => {
                        Some(true)
                    }
                    [cmd, delete, branch] if cmd == "branch" && delete == "-D" && branch == name => Some(false),
                    _ => None,
                });
                if exists == Some(true) { say(&format!("{HEAD}\n")) } else { no() }
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
            ["maintenance", "run", ..] => say(""),
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
    _folder: tempfile::TempDir,
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
    Setup { engine, git, cwds, project, _folder: folder, data }
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

#[tokio::test]
async fn a_merged_idle_copy_is_retired_while_another_conversation_runs() {
    let s = setup();
    let idle = sent(&s.engine, ask(&s.project, "Finished work")).await;
    let idle_copy = s.engine.open(&idle).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&idle_copy.path)).await;
    s.engine.shutdown();

    s.git.lock().unwrap().merged = true;
    std::fs::create_dir_all(s._folder.path().join(".git/objects")).unwrap();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let running = sent(&engine, ask(&s.project, "Keep working")).await;
    let running_copy = engine.open(&running).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&running_copy.path)).await;
    engine.sweep_copies();

    assert!(!Path::new(&idle_copy.path).exists(), "the idle copy has already merged");
    assert_eq!(engine.open(&idle).unwrap().copy, Some(idle_copy), "the conversation can still resume");
    assert!(Path::new(&running_copy.path).is_dir(), "the active copy stays even when merged");
    assert!(!s.git.lock().unwrap().calls.iter().any(|args| args.first().is_some_and(|arg| arg == "maintenance")));
    engine.shutdown();
}

#[tokio::test]
async fn an_archived_idle_copy_with_unmerged_work_stays() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Keep my changes")).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    s.engine.archive(&id, true).unwrap();
    s.engine.shutdown();

    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    engine.sweep_copies();

    assert!(s.git.lock().unwrap().calls.iter().any(|args| args.starts_with(&["merge-base".into(), "--is-ancestor".into()])));
    assert_eq!(engine.open(&id).unwrap().copy.as_ref(), Some(&copy));
    assert!(Path::new(&copy.path).is_dir(), "archiving does not authorize discarding unique work");
    engine.shutdown();
}

#[tokio::test]
async fn a_copy_that_gets_a_message_during_the_sweep_stays() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Finished work")).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    s.engine.shutdown();

    let marker = Path::new(&copy.path).join("target/cache");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "original copy").unwrap();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let active = engine.clone();
    let more = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "One more change") };
    {
        let mut repo = s.git.lock().unwrap();
        repo.merged = true;
        repo.on_status = Some(Box::new(move || assert_eq!(active.send(more).unwrap(), id)));
    }
    engine.sweep_copies();

    let repo = s.git.lock().unwrap();
    assert!(repo.on_status.is_none(), "the message arrived while Git was checking the copy");
    assert!(!repo.calls.iter().any(|args| args.starts_with(&["branch".into(), "-D".into()])));
    drop(repo);
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "original copy", "the message keeps the original copy in place");
    engine.shutdown();
}

#[tokio::test]
async fn a_message_during_copy_retirement_recreates_it_after_branch_deletion() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Finished work")).await;
    let expected = s.engine.open(&id).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&expected.path)).await;
    s.engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let sending = engine.clone();
    let git = s.git.clone();
    let request = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Continue after merge") };
    let (looked_up, lookup) = mpsc::channel();
    let (observed, result) = mpsc::channel();
    {
        let mut repo = s.git.lock().unwrap();
        repo.merged = true;
        repo.before_branch_delete = Some(Box::new(move || {
            git.lock().unwrap().on_branch_lookup = Some(Box::new(move || {
                let _ = looked_up.send(());
            }));
            sending.send(request).unwrap();
            observed.send(lookup.recv_timeout(Duration::from_secs(1)).is_ok()).unwrap();
        }));
    }
    engine.sweep_copies();
    assert!(!result.try_recv().unwrap(), "the old branch was consulted before it was deleted");
    ready(&engine, &id).await;
    assert_eq!(engine.open(&id).unwrap().copy, Some(expected));
    engine.shutdown();
}

async fn merged_idle_copy(s: &Setup) -> faber_engine::model::Conversation {
    let id = sent(&s.engine, ask(&s.project, "Finished work")).await;
    let conversation = s.engine.open(&id).unwrap();
    agent_started_in(&s.cwds, Path::new(&conversation.copy.as_ref().unwrap().path)).await;
    s.engine.shutdown();
    s.git.lock().unwrap().merged = true;
    Store::new(s.data.path()).get(&id).unwrap()
}

#[tokio::test]
async fn a_merged_copy_that_cannot_be_set_aside_keeps_its_build_folders() {
    let s = setup();
    let mut conversation = merged_idle_copy(&s).await;
    conversation.updated_at = 1;
    Store::new(s.data.path()).save_checked(&conversation).unwrap();
    let copy = conversation.copy.unwrap();
    let target = Path::new(&copy.path).join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "cache").unwrap();
    let blocked = target.parent().unwrap().with_file_name(format!(".gone-{}", conversation.id));
    std::fs::write(blocked, "rename blocked").unwrap();
    s.git.lock().unwrap().ignored = true;

    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    engine.sweep_copies();
    assert!(s.git.lock().unwrap().calls.iter().any(|args| args.get(1).is_some_and(|arg| arg == "--is-ancestor")));
    assert!(target.join("keep").is_file());
    engine.shutdown();
}

#[tokio::test]
async fn a_copy_being_made_by_branch_here_is_not_an_orphan() {
    let s = setup();
    let source = merged_idle_copy(&s).await;
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let sweeping = engine.clone();
    s.git.lock().unwrap().on_add = Some(Box::new(move || sweeping.sweep_copies()));
    let id = engine.branch_here(&source.id, &source.messages[0].id).unwrap();
    let copy = engine.open(&id).unwrap().copy.unwrap();
    assert!(Path::new(&copy.path).is_dir(), "the fork keeps the copy it just made");
    engine.shutdown();
}

#[tokio::test]
async fn a_copy_where_an_aside_starts_during_the_sweep_stays() {
    let s = setup();
    let conversation = merged_idle_copy(&s).await;
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let asking = engine.clone();
    let id = conversation.id;
    let asking_id = id.clone();
    let path = PathBuf::from(conversation.copy.unwrap().path);
    s.git.lock().unwrap().on_status = Some(Box::new(move || {
        asking.ask_aside(&asking_id, "Where did you make the change?", None).unwrap();
    }));
    engine.sweep_copies();
    assert!(engine.asides(&id).iter().any(|aside| aside.state == "running"));
    assert!(path.is_dir(), "the aside still owns this working folder");
    engine.shutdown();
}

#[tokio::test]
async fn a_copy_reserved_for_rewind_is_not_retired() {
    let s = setup();
    let conversation = merged_idle_copy(&s).await;
    let path = PathBuf::from(conversation.copy.unwrap().path);
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let restoring = engine.rewind_admission_guard(&conversation.id).unwrap();
    engine.sweep_copies();
    assert!(path.is_dir(), "restore and redo keep their working folder while admission is held");
    drop(restoring);
    engine.shutdown();
}

#[tokio::test]
async fn a_copy_under_a_final_rewind_scan_is_not_retired() {
    let s = setup();
    let conversation = merged_idle_copy(&s).await;
    let path = PathBuf::from(conversation.copy.unwrap().path);
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let sealing = engine.rewind_sealing_guard(&path);
    engine.sweep_copies();
    assert!(path.is_dir(), "the closed session's final scan still reads this folder");
    drop(sealing);
    engine.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_merged_copy_with_different_windows_path_casing_is_retired() {
    let s = setup();
    let mut conversation = merged_idle_copy(&s).await;
    let copy = conversation.copy.as_mut().unwrap();
    let path = PathBuf::from(&copy.path);
    copy.path.make_ascii_uppercase();
    assert_ne!(path, Path::new(&copy.path));
    Store::new(s.data.path()).save_checked(&conversation).unwrap();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    engine.sweep_copies();
    assert!(!path.exists());

    // The next message makes the copy again at the same folder and keeps it, though the record spelled it otherwise.
    s.git.lock().unwrap().merged = false;
    let started = s.cwds.lock().unwrap().len();
    let more = SendRequest { conversation_id: Some(conversation.id.clone()), ..ask(&s.project, "One more change") };
    assert_eq!(engine.send(more).unwrap(), conversation.id);
    let again =
        || s.cwds.lock().unwrap()[started..].iter().any(|cwd| cwd.canonicalize().is_ok_and(|cwd| cwd == path.canonicalize().unwrap()));
    support::until("the follow-up never started in the recreated copy", || path.is_dir() && again()).await;
    let attached = engine.open(&conversation.id).unwrap().copy.unwrap();
    assert_eq!(Path::new(&attached.path), path, "the recreated copy is attached, not dropped as a stray");
    assert!(!s.git.lock().unwrap().calls.iter().any(|args| args.starts_with(&["worktree".into(), "remove".into()])));
    assert!(path.is_dir());
    engine.shutdown();
}

#[derive(Default)]
struct UnconfirmedCheck(tokio::sync::Notify);

impl Runner for UnconfirmedCheck {
    fn start(&self, _: &Spec) -> std::io::Result<Started> {
        let (closed, confirmed) = tokio::sync::watch::channel(false);
        drop(closed);
        self.0.notify_one();
        Ok(Started {
            stdout: Box::new(tokio::io::empty()),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(std::future::pending()),
            confirmed: Some(confirmed),
            throttle: None,
            peak: None,
        })
    }
}

#[tokio::test]
async fn leftover_retired_copy_removals_recheck_sweep_admission() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let s = setup();
    let copies = faber_engine::project::copy::home(s.data.path());
    let gone = [copies.join(".gone-one"), copies.join(".gone-two")];
    for path in &gone {
        std::fs::create_dir_all(path).unwrap();
    }
    let reads = AtomicUsize::new(0);
    let verifier =
        Verifier::new(Arc::new(UnconfirmedCheck::default())).with_guard(Arc::new(move || reads.fetch_add(1, Ordering::Relaxed) >= 3));
    s.engine.set_verifier(Arc::new(verifier));
    s.engine.sweep_copies();
    assert_eq!(gone.iter().filter(|p| p.is_dir()).count(), 1, "the second removal waits for the next sweep");
    s.engine.shutdown();
}

#[tokio::test]
async fn a_copy_with_an_unconfirmed_check_process_is_kept() {
    unconfirmed_check_keeps_the_copy(|nested| nested.to_path_buf()).await;
}

#[cfg(windows)]
#[tokio::test]
async fn a_check_folder_spelled_another_way_on_windows_keeps_the_copy() {
    let spell = |nested: &Path| PathBuf::from(format!(r"\\?\{}", faber_engine::project::git::native_path(nested)).to_uppercase());
    unconfirmed_check_keeps_the_copy(spell).await;
}

/// A check stopped without proof its processes ended, run in a folder inside the copy spelled by `spell`.
async fn unconfirmed_check_keeps_the_copy(spell: impl Fn(&Path) -> PathBuf) {
    let s = setup();
    let conversation = merged_idle_copy(&s).await;
    let path = PathBuf::from(conversation.copy.unwrap().path);
    std::fs::create_dir(path.join("nested")).unwrap();
    let nested = spell(&path.join("nested"));
    let check = Arc::new(UnconfirmedCheck::default());
    let verifier = Arc::new(Verifier::new(check.clone()));
    let request =
        Request { folder: nested.clone(), level: Some(Level::Full), commands: vec!["node check.js".into()], ..Request::default() };
    let ending = verifier.start(&tokio::runtime::Handle::current(), nested.clone(), "check", Level::Full, std::future::ready(request));
    tokio::time::timeout(support::DEADLINE, check.0.notified()).await.unwrap();
    assert!(verifier.stop(&nested, "check"));
    ending.await;
    assert!(verifier.idle() && verifier.unknown_in(&path));

    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    engine.set_verifier(verifier);
    engine.sweep_copies();
    assert!(path.is_dir(), "a stopped check may still be writing in the copy");
    engine.shutdown();
}

#[tokio::test]
async fn idle_storage_maintenance_applies_to_an_ordinary_project_without_a_conversation() {
    let s = setup();
    let root = s.git.lock().unwrap().root.clone();
    std::fs::create_dir_all(root.join(".git/objects")).unwrap();
    s.engine.sweep_copies();
    s.engine.sweep_copies();
    let calls = s.git.lock().unwrap().calls.clone();
    assert_eq!(calls.iter().filter(|args| args.first().is_some_and(|a| a == "maintenance")).count(), 1);
    assert!(calls.iter().all(|args| !args.iter().any(|a| a == "fetch" || a == "gc")), "housekeeping never downloads or prunes history");
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
