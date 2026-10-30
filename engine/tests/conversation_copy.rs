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
    for _ in 0..200 {
        // A copy named on the conversation but not attached yet has no repository (`root`).
        if engine.open(id).and_then(|c| c.copy).is_some_and(|c| !c.root.is_empty() && Path::new(&c.path).is_dir()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the copy of {id} was never made");
}

async fn agent_started(cwds: &Mutex<Vec<PathBuf>>, count: usize) {
    for _ in 0..200 {
        if cwds.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the agent did not start after history capture");
}

async fn agent_started_in(cwds: &Mutex<Vec<PathBuf>>, copy: &Path) {
    let expected = copy.canonicalize().unwrap();
    for _ in 0..200 {
        if cwds.lock().unwrap().iter().any(|cwd| cwd.canonicalize().is_ok_and(|cwd| cwd == expected)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the agent did not start in its copy {expected:?}; starts: {:?}", cwds.lock().unwrap());
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
async fn move_chat_to_project_prepares_the_copy_for_keep_going() {
    automatic_first_project_turn(false).await;
}

#[tokio::test]
async fn move_chat_to_project_prepares_the_copy_for_another_conversations_ask() {
    automatic_first_project_turn(true).await;
}

async fn automatic_first_project_turn(asked: bool) {
    let data = support::DataFolder::new();
    let folder = support::work_folder();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let seen = cwds.clone();
    let (inner, made) = support::pipes();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        inner(bin, args, cwd)
    });
    let engine = Engine::new(
        EngineOptions {
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
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    let git = Git(Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() })));
    engine.set_git(Arc::new(move || Some(Box::new(git.clone()) as Box<dyn Executor + Send>)));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let mut record: faber_engine::model::Conversation =
        serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    record.working_folder = None;
    record.choice = Some(ask(&project, "").choice);
    for m in &mut record.messages {
        m.attachments = None;
    }
    let id = record.id.clone();
    faber_engine::store::Store::new(data.path()).save_checked(&record).unwrap();
    engine.move_to_project(&id, &project).unwrap();
    let _asking = if asked {
        engine.send(ask(&project, "Ask the other conversation")).unwrap();
        let mut agent = support::next_pipe(&made).await;
        agent.say(serde_json::json!({"type":"system","subtype":"init","session_id":"parent","model":"claude-haiku-5-5"})).await;
        agent.say(serde_json::json!({"type":"control_request","request_id":"ask","request":{"subtype":"mcp_message","server_name":"faber",
                "message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conversation","arguments":{"action":"ask","id":id,"message":"What remains?"},"_meta":{"claudecode/toolUseId":"ask"}}}}})).await;
        Some(agent)
    } else {
        engine.keep_going(&id, true).unwrap();
        None
    };
    let target = support::next_pipe(&made).await;
    let current = engine.open(&id).unwrap();
    let copy = current.copy.expect("an automatic first project turn must prepare its separate copy");
    assert!(!copy.root.is_empty(), "the copy is made before starting its session");
    assert!(!current.project_start_pending);
    assert_eq!(cwds.lock().unwrap().last(), Some(&copy.folder()), "never in the person's checkout, asked={asked}");
    for _ in 0..200 {
        if target.lines.lock().unwrap().iter().any(|m| m["type"] == "user") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let lines = target.lines.lock().unwrap();
    let first = lines.iter().find(|m| m["type"] == "user").expect("the turn reaches its agent");
    assert!(first.to_string().contains("Earlier in this conversation"), "the first turn also carries Chat's history");
    drop(lines);
    engine.shutdown();
}

#[tokio::test]
async fn move_chat_to_project_resolves_a_stale_chat_send_before_copies_and_attachments() {
    for attached in [false, true] {
        let data = support::DataFolder::new();
        let folder = support::work_folder();
        let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
        let git = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
        let engine = start(&data, &cwds, Git(git));
        let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
        let mut record: faber_engine::model::Conversation =
            serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
        record.working_folder = None;
        for m in &mut record.messages {
            m.attachments = None;
        }
        let id = record.id.clone();
        faber_engine::store::Store::new(data.path()).save_checked(&record).unwrap();
        engine.move_to_project(&id, &project.id).unwrap();
        let mut req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Chat,
            project_id: None,
            ..ask(&project.id, "A message sent before the move finished")
        };
        if attached {
            req.files.push(faber_engine::workspace::Attachment { name: "late.txt".into(), base64: "bm90ZXM=".into() });
        }
        engine.send(req).unwrap();
        let accepted = engine.open(&id).unwrap();
        assert!(accepted.copy.is_some(), "a stale Chat area must still name the required project copy, attached={attached}");
        assert!(!accepted.project_start_pending, "the marker is consumed only with the copy already named");
        ready(&engine, &id).await;
        let current = engine.open(&id).unwrap();
        let copy = current.copy.unwrap();
        agent_started_in(&cwds, &copy.folder()).await;
        assert_eq!(current.area, Area::Project);
        assert_eq!(current.project_id.as_deref(), Some(project.id.as_str()));
        assert_eq!(*cwds.lock().unwrap(), vec![copy.folder()], "nothing starts in the person's checkout");
        assert!(!folder.path().join("attachments").exists(), "the stale area must not save attachments in the checkout");
        if attached {
            assert_eq!(std::fs::read(data.path().join("attachments").join(&id).join("late.txt")).unwrap(), b"notes");
            assert_eq!(current.messages[record.messages.len()].attachments.as_deref(), Some(["late.txt".to_string()].as_slice()));
        }
        engine.shutdown();
    }
}

#[tokio::test]
async fn move_chat_to_project_uses_the_current_copy_rule_at_its_first_project_turn_and_warmup() {
    for enabled_at_move in [false, true] {
        let s = setup();
        let store = faber_engine::store::Store::new(s.data.path());
        let mut projects = store.projects();
        projects[0].separate_copies = enabled_at_move;
        store.save_projects(&projects).unwrap();
        let mut record: faber_engine::model::Conversation =
            serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
        record.working_folder = None;
        for m in &mut record.messages {
            m.attachments = None;
        }
        let id = record.id.clone();
        store.save_checked(&record).unwrap();
        s.engine.move_to_project(&id, &s.project).unwrap();
        s.engine.shutdown();
        projects[0].separate_copies = !enabled_at_move;
        store.save_projects(&projects).unwrap();
        let reopened = start(&s.data, &s.cwds, Git(s.git.clone()));
        reopened.warm(WarmRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(s.project.clone()),
            choice: ask(&s.project, "").choice,
        });
        if !enabled_at_move {
            assert!(s.cwds.lock().unwrap().is_empty(), "enabling copies after a move must block checkout warmup");
            sent(&reopened, SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Continue") }).await;
            let copy = reopened.open(&id).unwrap().copy.unwrap();
            for _ in 0..200 {
                if !s.cwds.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(*s.cwds.lock().unwrap(), vec![copy.folder()]);
        } else {
            assert!(s.cwds.lock().unwrap().is_empty(), "project warmup must wait for the first rewind baseline even without copies");
            reopened.send(SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Continue") }).unwrap();
            agent_started(&s.cwds, 1).await;
            assert_eq!(s.cwds.lock().unwrap()[0].canonicalize().unwrap(), s.folder.path());
            assert!(reopened.open(&id).unwrap().copy.is_none());
            assert!(!s.git.lock().unwrap().calls.iter().any(|args| args.starts_with(&["worktree".into(), "add".into()])));
        }
        reopened.shutdown();
    }
}

#[tokio::test]
async fn move_chat_to_project_makes_its_separate_copy_only_at_the_next_turn_even_after_restart() {
    let s = setup();
    // A settled Chat record, with earlier messages, is enough: the next turn must still get its first project copy.
    let mut record: faber_engine::model::Conversation =
        serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    record.area = Area::Chat;
    record.project_id = None;
    record.working_folder = None;
    record.copy = None;
    record.queued.clear();
    record.keep_going = None;
    record.archived_at = None;
    for m in &mut record.messages {
        m.attachments = None;
        m.permissions = None;
    }
    let id = record.id.clone();
    faber_engine::store::Store::new(s.data.path()).save_checked(&record).unwrap();
    s.engine.open(&id).unwrap();
    let moved = s.engine.move_to_project(&id, &s.project).unwrap();
    assert!(moved.copy.is_none() && moved.project_start_pending, "the copy rule waits for the first project turn");
    assert!(s.cwds.lock().unwrap().is_empty(), "moving starts neither git nor a CLI");
    s.engine.warm(WarmRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: Some(s.project.clone()),
        choice: ask(&s.project, "").choice,
    });
    assert!(s.cwds.lock().unwrap().is_empty(), "warming never starts in the person's checkout");
    s.engine.shutdown();
    let reopened = start(&s.data, &s.cwds, Git(s.git.clone()));
    sent(&reopened, SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Continue the work") }).await;
    let copy = reopened.open(&id).unwrap().copy.unwrap();
    assert!(!copy.root.is_empty());
    assert_eq!(reopened.open(&id).unwrap().messages.len(), record.messages.len() + 2);
    for _ in 0..200 {
        if !s.cwds.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*s.cwds.lock().unwrap(), vec![copy.folder()]);
    reopened.shutdown();
}

#[tokio::test]
async fn r_next_each_project_conversation_works_in_a_copy_of_its_own_on_a_faber_branch() {
    let s = setup();
    let first = sent(&s.engine, ask(&s.project, "Fix the login page")).await;
    let a = s.engine.open(&first).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&a.path)).await;
    let second = sent(&s.engine, ask(&s.project, "Fix the login page")).await;
    let b = s.engine.open(&second).unwrap().copy.unwrap();
    agent_started_in(&s.cwds, Path::new(&b.path)).await;
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
async fn r_next_with_auto_the_copy_is_named_when_the_message_is_sent_not_when_the_route_is_back() {
    let s = setup();
    let mut req = ask(&s.project, "Fix the login page");
    req.choice = Choice { vendor: "auto".into(), model: None, effort: None };
    let id = s.engine.send(req).unwrap();
    // The screen opens the conversation as soon as send returns and has no later event for the copy.
    let copy = s.engine.open(&id).unwrap().copy.expect("the crumb needs the copy at once");
    assert!(copy.branch.starts_with("faber/fix-the-login-page-"), "{copy:?}");
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
    agent_started(&s.cwds, 1).await;
    assert!(s.git.lock().unwrap().calls.iter().all(|args| args.first().is_some_and(|a| a == "ls-files")), "only history name reads");
    let cwd = s.cwds.lock().unwrap()[0].clone();
    assert_eq!(cwd.canonicalize().unwrap(), s.folder.path().canonicalize().unwrap());
    s.engine.shutdown();
}

/// Parity F5: a session picked up from a terminal goes on in the folder it ran in, even while the project keeps copies.
#[tokio::test]
async fn a_session_picked_up_from_a_terminal_works_in_the_project_folder() {
    let s = setup();
    let session = faber_engine::model::CliSession {
        vendor: Vendor::Claude,
        id: "terminal-1".into(),
        title: "Look around".into(),
        model: Some("claude-haiku-5-5".into()),
        updated_at: 1,
    };
    let id = s.engine.pick_up(&s.project, session).unwrap();
    let model = s.engine.open(&id).unwrap().choice.and_then(|c| c.model);
    assert_eq!(model.as_deref(), Some("haiku"), "the picker names Claude's models by family");
    s.engine.send(SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Go on") }).unwrap();
    assert!(s.engine.open(&id).unwrap().copy.is_none());
    agent_started(&s.cwds, 1).await;
    // No copy is made: only the file lists the turn reads before it starts.
    assert!(s.git.lock().unwrap().calls.iter().all(|args| args.first().is_some_and(|a| a == "ls-files")), "no copy");
    let cwd = s.cwds.lock().unwrap()[0].clone();
    assert_eq!(cwd.canonicalize().unwrap(), s.folder.path().canonicalize().unwrap());
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_a_copy_with_unsaved_work_is_deleted_only_when_the_person_says_so() {
    let s = setup();
    let id = sent(&s.engine, ask(&s.project, "Change things")).await;
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
    // The folder goes first; the prune and the branch follow it, on the same background step.
    let branch_gone = vec!["branch".to_string(), "-D".into(), copy.branch.clone()];
    for _ in 0..40 {
        if s.git.lock().unwrap().calls.contains(&branch_gone) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let calls = s.git.lock().unwrap().calls.clone();
    assert!(calls.contains(&vec!["worktree".into(), "remove".into(), "--force".into(), "--".into(), copy.path.clone()]), "{calls:?}");
    assert!(calls.contains(&branch_gone), "{calls:?}");

    // Work held by another branch is saved, but the agent is still at work there and may write more: it asks.
    let id = sent(&s.engine, ask(&s.project, "Small fix")).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    s.git.lock().unwrap().held = true;
    assert_eq!(s.engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    // Nothing runs there now, but a message starts the agent again while git looks: the look no longer holds, so it asks.
    s.engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let again = engine.clone();
    let more = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "One more change") };
    s.git.lock().unwrap().on_status = Some(Box::new(move || drop(again.send(more))));
    assert_eq!(engine.remove_with(&id, false), Err("error.copy.unsaved".into()));
    assert!(engine.open(&id).is_some() && Path::new(&copy.path).is_dir());
    // Once nothing runs there, the delete needs no question.
    engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    assert_eq!(engine.remove_with(&id, false), Ok(()));
    assert!(gone(Path::new(&copy.path)).await);

    // The agent left its branch and committed on a detached HEAD: that commit is on no branch, so it asks.
    let id = sent(&engine, ask(&s.project, "Detach")).await;
    engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
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
    let id = sent(&s.engine, ask(&s.project, "Start here")).await;
    let copy = s.engine.open(&id).unwrap().copy.expect("the message went to the conversation its copy was made for");
    agent_started_in(&s.cwds, Path::new(&copy.path)).await;
    let cwd = s.cwds.lock().unwrap().last().cloned().unwrap();
    assert_eq!(cwd.canonicalize().unwrap(), PathBuf::from(&copy.path).canonicalize().unwrap(), "never in the project folder");
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_at_start_merged_copies_and_those_of_deleted_conversations_go() {
    let s = setup();
    let kept = sent(&s.engine, ask(&s.project, "Keep working")).await;
    let merged = sent(&s.engine, ask(&s.project, "Finished work")).await;
    let (kept_copy, merged_copy) = (s.engine.open(&kept).unwrap().copy.unwrap(), s.engine.open(&merged).unwrap().copy.unwrap());
    // The startup sweep needs idle folders, including the off-lock history capture that follows attachment.
    agent_started_in(&s.cwds, Path::new(&kept_copy.path)).await;
    agent_started_in(&s.cwds, Path::new(&merged_copy.path)).await;
    s.engine.shutdown();
    // A folder no conversation names any more (its delete could not remove it then).
    let orphan = s.data.path().join("copies").join("00000000-0000-4000-8000-000000000000");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join(".branch"), "faber/old-0000").unwrap();
    // Both branches are in the person's branch, but `kept` still has uncommitted changes. Faber starts again and sweeps.
    s.git.lock().unwrap().merged = true;
    let engine = start(&s.data, &s.cwds, DirtyIn(Git(s.git.clone()), PathBuf::from(&kept_copy.path)));
    engine.sweep_copies();
    assert!(!orphan.exists(), "the orphan goes");
    assert!(!Path::new(&merged_copy.path).exists(), "a clean merged copy goes");
    assert!(Path::new(&kept_copy.path).is_dir(), "a copy with changes stays");
    // The merged conversation keeps its record, and its next message makes the copy again in the same folder.
    assert_eq!(engine.open(&merged).unwrap().copy.as_ref(), Some(&merged_copy));
    let again = SendRequest { conversation_id: Some(merged.clone()), ..ask(&s.project, "One more thing") };
    sent(&engine, again).await;
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

/// A conversation the project's agent starts by itself (R198) works in a copy of its own too, named after its title, and
/// its brief reaches it only there: three agents' pieces and the person's checkout never share a folder.
#[tokio::test]
async fn r_next_a_conversation_the_agent_starts_works_in_a_copy_of_its_own() {
    let data = support::DataFolder::new();
    let folder = support::canonical_tempdir().unwrap();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let git = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let (inner, made) = support::pipes();
    let seen = cwds.clone();
    let spawn: Spawner = Arc::new(move |bin, argv, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        inner(bin, argv, cwd)
    });
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();
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
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    // Faber starts a conversation on its own only on a known subscription.
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    let runner = Git(git.clone());
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()) as Box<dyn Executor + Send>)));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let parent = engine.send(ask(&project, "Split the work")).unwrap();
    let mut agent = support::next_pipe(&made).await;
    agent.say(serde_json::json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    let input = serde_json::json!({"title":"Docs page","brief":"Write the docs page for the parser."});
    agent.say(serde_json::json!({"type":"control_request","request_id":"r1","request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conversation","arguments":input,"_meta":{"claudecode/toolUseId":"toolu_c"}}}}})).await;
    let mut sibling = support::next_pipe(&made).await;
    for _ in 0..300 {
        if sibling.lines.lock().unwrap().iter().any(|m| m["type"] == "user") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let brief = sibling.lines.lock().unwrap().iter().find(|m| m["type"] == "user").cloned().expect("the brief is sent");
    assert!(brief.to_string().contains("Write the docs page for the parser."), "{brief}");
    let child = engine.state().conversations.into_iter().find(|c| c.title == "Docs page").expect("the new conversation is listed").id;
    let (own, theirs) = (engine.open(&parent).unwrap().copy.unwrap(), engine.open(&child).unwrap().copy.expect("a copy of its own"));
    assert!(theirs.branch.starts_with("faber/docs-page-") && theirs.path != own.path, "{own:?} {theirs:?}");
    let started = cwds.lock().unwrap().clone();
    assert_eq!(started.len(), 2, "{started:?}");
    assert_eq!(started[1].canonicalize().unwrap(), PathBuf::from(&theirs.path).canonicalize().unwrap(), "never the project folder");
    sibling.say(serde_json::json!({"type":"system","subtype":"init","session_id":"s2","model":"claude-haiku-5-5"})).await;
    // A slow child checkout cannot hold the engine lock or leave a stray copy after its conversation is removed.
    let (release, gate) = std::sync::mpsc::channel();
    let copying = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let entered = copying.clone();
    git.lock().unwrap().on_add = Some(Box::new(move || {
        entered.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = gate.recv_timeout(Duration::from_secs(10));
    }));
    agent.say(serde_json::json!({"type":"control_request","request_id":"r2","request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"conversation","arguments":{"title":"Discarded docs","brief":"Check the docs."},"_meta":{"claudecode/toolUseId":"toolu_discard"}}}}})).await;
    for _ in 0..300 {
        if copying.load(std::sync::atomic::Ordering::SeqCst)
            && agent.lines.lock().unwrap().iter().any(|m| m["type"] == "control_response" && m["response"]["request_id"] == "r2")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(copying.load(std::sync::atomic::Ordering::SeqCst), "the child copy started");
    assert!(agent.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "r2"), "the call answers while git still waits");
    let discarded = engine.state().conversations.into_iter().find(|c| c.title == "Discarded docs").unwrap().id;
    let stray = engine.open(&discarded).unwrap().copy.unwrap();
    engine.remove_with(&discarded, true).unwrap();
    release.send(()).unwrap();
    assert!(gone(Path::new(&stray.path)).await, "the common copy callback removes the discarded child's checkout");
    assert_eq!(cwds.lock().unwrap().len(), 2, "the discarded brief starts no agent");
    engine.shutdown();
}

/// What the screen sees of a first message while the copy is still being made: the scripted `worktree add` waits for
/// `release` (at most ten seconds, so a send that waits for it still ends).
struct Slow {
    setup: Setup,
    release: std::sync::mpsc::Sender<()>,
    /// Whether the copy's folder already existed when each message event went out.
    early: Arc<Mutex<Vec<bool>>>,
}

fn slow() -> Slow {
    let data = support::DataFolder::new();
    let folder = support::canonical_tempdir().unwrap();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let git = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let (release, gate) = std::sync::mpsc::channel::<()>();
    git.lock().unwrap().on_add = Some(Box::new(move || {
        let _ = gate.recv_timeout(Duration::from_secs(10));
    }));
    let early: Arc<Mutex<Vec<bool>>> = Arc::default();
    let home = faber_engine::project::copy::home(data.path());
    let note = early.clone();
    let events: Arc<dyn Fn(AppEvent) + Send + Sync> = Arc::new(move |e| {
        if let AppEvent::Message { conversation_id, .. } = e {
            note.lock().unwrap().push(home.join(conversation_id).exists());
        }
    });
    let engine = start_seen(&data, &cwds, Git(git.clone()), events);
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    Slow { setup: Setup { engine, git, cwds, project, folder, data }, release, early }
}

#[tokio::test]
async fn move_chat_to_project_paid_consent_waits_for_its_copy_and_cancel_revokes_it() {
    use faber_engine::{
        budget::{Billing, Reach},
        events::EngineEvent,
        signals::Signals,
    };
    for (cancel, wait_first) in [(false, false), (false, true), (true, false), (true, true)] {
        let Slow { setup: s, release, .. } = slow();
        s.engine.set_time_source(Arc::new(|| 1_791_620_000.0));
        s.engine.note_account_events(
            Vendor::Claude,
            vec![
                EngineEvent::Signals(Signals {
                    billing: Some(Billing::Subscription),
                    paid_fallback: Some(true),
                    reach: Some(Reach::Out(Some(1_791_620_600.0))),
                    ..Default::default()
                }),
                EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: 1_791_620_600.0 },
            ],
        );
        let mut record: faber_engine::model::Conversation =
            serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
        record.working_folder = None;
        for m in &mut record.messages {
            m.attachments = None;
        }
        faber_engine::store::Store::new(s.data.path()).save_checked(&record).unwrap();
        let id = record.id.clone();
        s.engine.move_to_project(&id, &s.project).unwrap();
        s.engine.send(SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Continue in the project") }).unwrap();
        let blocked = s.engine.open(&id).unwrap().messages.last().unwrap().clone();
        assert_eq!(blocked.error.as_deref(), Some("error.quotaOut"));
        if wait_first {
            s.engine.wait_turn(&id, &blocked.id).unwrap();
        }
        s.engine.approve_credits(&id, &blocked.id, Vendor::Claude).unwrap();
        assert!(s.cwds.lock().unwrap().is_empty(), "no agent before its copy");
        if cancel {
            s.engine.interrupt(&id);
        }
        release.send(()).unwrap();
        ready(&s.engine, &id).await;
        for _ in 0..200 {
            let current = s.engine.open(&id).unwrap();
            if cancel || current.messages.last().unwrap().status == Some(Status::Error) || !s.cwds.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let current = s.engine.open(&id).unwrap();
        assert_eq!(s.cwds.lock().unwrap().len(), usize::from(!cancel), "consent must survive until actual dispatch, cancel={cancel}");
        if !cancel {
            assert!(only_in_copy(&s.cwds.lock().unwrap(), s.data.path(), &id));
        }
        assert_eq!(current.messages.last().unwrap().status, Some(if cancel { Status::Stopped } else { Status::Streaming }));
        assert!(s.engine.approve_credits(&id, &blocked.id, Vendor::Claude).is_err(), "approval cannot be replayed");
        s.engine.interrupt(&id);
        let next =
            s.engine.send(SendRequest { conversation_id: cancel.then(|| id.clone()), ..ask(&s.project, "A different turn") }).unwrap();
        assert_eq!(
            s.engine.open(&next).unwrap().messages.last().unwrap().error.as_deref(),
            Some("error.quotaOut"),
            "the next reply got no consent"
        );
        s.engine.shutdown();
    }
}

#[tokio::test]
async fn r_next_a_first_message_shows_at_once_and_its_agent_starts_in_the_copy_when_the_copy_is_made() {
    let Slow { setup: s, release, early } = slow();
    let (capturing, capture_started) = tokio::sync::oneshot::channel();
    let (finish_capture, capturing_gate) = std::sync::mpsc::channel();
    let runner = s.git.clone();
    let capture_gate = Mutex::new(Some((capturing, capturing_gate)));
    // The first runner makes the copy; the next prepares its capture, even when storage later rejects it.
    s.engine.set_git(Arc::new(move || {
        let copied = runner.lock().unwrap().calls.iter().any(|call| call.starts_with(&["worktree".into(), "add".into()]));
        let gate = if copied { capture_gate.lock().unwrap().take() } else { None };
        if let Some((capturing, capturing_gate)) = gate {
            let _ = capturing.send(());
            capturing_gate.recv_timeout(Duration::from_secs(10)).expect("the test releases history capture");
        }
        Some(Box::new(Git(runner.clone())))
    }));
    let started = std::time::Instant::now();
    let id = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "send waited for the copy: {:?}", started.elapsed());
    // The question and the reply that is typing are on screen, and nothing runs yet: no copy, no agent, nothing in the folder.
    let shown = s.engine.open(&id).unwrap();
    assert_eq!(shown.messages.len(), 2, "{:?}", shown.messages);
    assert_eq!(shown.messages[1].status, Some(Status::Streaming));
    assert!(shown.copy.as_ref().is_some_and(|c| c.root.is_empty()), "the copy is named on the conversation, not made: {:?}", shown.copy);
    assert!(s.cwds.lock().unwrap().is_empty(), "the agent must not start before its copy is there");
    assert_eq!(early.lock().unwrap().clone(), [false, false], "the question and the typing reply go out before the copy exists");
    // The copy attaches first; its history capture must then finish before the agent starts in it.
    let _ = release.send(());
    ready(&s.engine, &id).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    tokio::time::timeout(Duration::from_secs(10), capture_started).await.expect("history capture starts after attachment").unwrap();
    assert!(s.cwds.lock().unwrap().is_empty(), "an attached copy must finish its history capture before the agent starts");
    assert!(s.engine.open(&id).unwrap().messages[0].rewind.is_none(), "the held capture has not published its baseline yet");
    finish_capture.send(()).unwrap();
    agent_started_in(&s.cwds, Path::new(&copy.path)).await;
    let cwds = s.cwds.lock().unwrap().clone();
    assert_eq!(cwds.len(), 1, "{cwds:?}");
    assert_eq!(cwds[0].canonicalize().unwrap(), PathBuf::from(&copy.path).canonicalize().unwrap());
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_a_stopped_first_message_does_not_start_its_agent_when_the_copy_comes() {
    let Slow { setup: s, release, .. } = slow();
    let id = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    s.engine.interrupt(&id);
    assert_eq!(s.engine.open(&id).unwrap().messages[1].status, Some(Status::Stopped));
    let _ = release.send(());
    ready(&s.engine, &id).await;
    assert!(s.cwds.lock().unwrap().is_empty(), "stopped before its copy: no agent");
    // The copy is kept on the conversation's record, so after a restart the next message still goes on in it.
    s.engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    assert!(engine.open(&id).unwrap().copy.is_some_and(|c| !c.root.is_empty()), "the made copy was saved");
    let again = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Go on") };
    engine.send(again).unwrap();
    agent_started(&s.cwds, 1).await;
    assert_eq!(s.cwds.lock().unwrap().len(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_message_written_after_stopping_a_first_message_waits_for_the_same_copy() {
    let Slow { setup: s, release, .. } = slow();
    let id = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    s.engine.interrupt(&id);
    let again = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Go on") };
    assert_eq!(s.engine.send(again).unwrap(), id);
    assert!(s.cwds.lock().unwrap().is_empty(), "the copy is not there yet: nothing may start in the project folder");
    let _ = release.send(());
    ready(&s.engine, &id).await;
    let copy = s.engine.open(&id).unwrap().copy.unwrap();
    for _ in 0..200 {
        if !s.cwds.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let cwds = s.cwds.lock().unwrap().clone();
    assert_eq!(cwds.len(), 1, "{cwds:?}");
    assert_eq!(cwds[0].canonicalize().unwrap(), PathBuf::from(&copy.path).canonicalize().unwrap());
    let adds = s.git.lock().unwrap().calls.iter().filter(|c| c.starts_with(&["worktree".into(), "add".into()])).count();
    assert_eq!(adds, 1, "one copy for both messages");
    s.engine.shutdown();
}

/// A repository that refuses to add a work tree.
#[derive(Clone)]
struct NoAdd(Git, Arc<std::sync::atomic::AtomicUsize>);
impl Executor for NoAdd {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        if call.args.iter().any(|a| a == "worktree") && call.args.iter().any(|a| a == "add") {
            self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Ok(Output { success: false, stdout: vec![] });
        }
        self.0.execute(call)
    }
}

#[tokio::test]
async fn r_next_a_copy_that_cannot_be_made_ends_the_reply_and_the_next_message_tries_again() {
    let data = support::DataFolder::new();
    let folder = support::canonical_tempdir().unwrap();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let repo = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let adds: Arc<std::sync::atomic::AtomicUsize> = Arc::default();
    let engine = start(&data, &cwds, NoAdd(Git(repo.clone()), adds.clone()));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let id = engine.send(ask(&project, "Fix the login page")).unwrap();
    let mut reply = engine.open(&id).unwrap().messages[1].clone();
    for _ in 0..200 {
        if reply.status != Some(Status::Streaming) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        reply = engine.open(&id).unwrap().messages[1].clone();
    }
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.copy.failed")));
    assert!(cwds.lock().unwrap().is_empty(), "no agent without its copy");
    // The person writes again in the same conversation: it asks for a copy again instead of working in the folder.
    let tries = || adds.load(std::sync::atomic::Ordering::SeqCst);
    let before = tries();
    let again = SendRequest { conversation_id: Some(id.clone()), ..ask(&project, "Try again") };
    engine.send(again).unwrap();
    for _ in 0..200 {
        if tries() > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(tries() > before, "the copy was tried again");
    assert!(cwds.lock().unwrap().is_empty(), "still never in the project folder");
    engine.shutdown();
}

/// Where an agent may start while a copy is pending: only inside its conversation's copy folder, never the project folder.
fn only_in_copy(cwds: &[PathBuf], data: &Path, id: &str) -> bool {
    // A folder git has not made yet cannot be resolved: it is compared as it is named.
    let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let copy = real(&faber_engine::project::copy::home(data).join(id));
    cwds.iter().all(|cwd| real(cwd).starts_with(&copy))
}

#[tokio::test]
async fn r_next_a_queued_message_sent_now_after_stopping_never_starts_in_the_project_folder() {
    let Slow { setup: s, release, .. } = slow();
    let id = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    // Written while the first answer waits for its copy: queued. Then the first answer is stopped, and "Send now" is pressed.
    let queued = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "And the footer") };
    s.engine.send(queued).unwrap();
    let item = s.engine.open(&id).unwrap().queued[0].id.clone();
    s.engine.interrupt(&id);
    let _ = s.engine.send_queued(&id, &item);
    assert!(s.cwds.lock().unwrap().is_empty(), "nothing starts before the copy is there");
    // The copy comes: the queued message is the one that starts, in the copy.
    let _ = release.send(());
    ready(&s.engine, &id).await;
    for _ in 0..200 {
        if !s.cwds.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let cwds = s.cwds.lock().unwrap().clone();
    assert_eq!(cwds.len(), 1, "{cwds:?}");
    assert!(only_in_copy(&cwds, s.data.path(), &id), "started outside the copy: {cwds:?}");
    s.engine.shutdown();
}

#[tokio::test]
async fn r_next_a_restart_before_the_copy_is_attached_still_works_in_the_copy_afterwards() {
    let Slow { setup: s, release, .. } = slow();
    let id = s.engine.send(ask(&s.project, "Fix the login page")).unwrap();
    // Faber closes while git is still making the copy; the next start finds the conversation with its copy named only.
    s.engine.shutdown();
    let engine = start(&s.data, &s.cwds, Git(s.git.clone()));
    let named = engine.open(&id).unwrap().copy.expect("the copy was named before git started");
    assert!(named.root.is_empty(), "{named:?}");
    // Typing in it, or a message, never starts the agent in the project folder; the message makes the copy and runs there.
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    engine.warm(WarmRequest { conversation_id: Some(id.clone()), area: Area::Project, project_id: Some(s.project.clone()), choice });
    assert!(only_in_copy(&s.cwds.lock().unwrap(), s.data.path(), &id));
    let again = SendRequest { conversation_id: Some(id.clone()), ..ask(&s.project, "Go on") };
    engine.send(again).unwrap();
    ready(&engine, &id).await;
    let copy = engine.open(&id).unwrap().copy.unwrap();
    assert_eq!(copy.branch, named.branch, "the same branch, so nothing is left behind");
    assert!(only_in_copy(&s.cwds.lock().unwrap(), s.data.path(), &id));
    let _ = release.send(());
    tokio::time::sleep(Duration::from_millis(200)).await;
    engine.shutdown();
}
