//! Manual opt-in live probe. Uses only an isolated target directory and Haiku/low.
//! Run from the worktree root with `node scripts/cargo.mjs run -p faber-engine --example autonomy_probe`.
use faber_engine::account::{claude_account, claude_limits};
use faber_engine::budget::Billing;
use faber_engine::cli::{Cli, find_cli};
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Conversation, Message, Permissions, Settings, Status, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::router::{Choice, Effort};
use faber_engine::signals::claude_env_billed;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    collections::BTreeMap,
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

#[derive(Deserialize, Serialize)]
struct SavedWait {
    id: String,
    since: u64,
    until: u64,
    pid: u32,
    model_sessions: usize,
}

struct ProbeEngine {
    engine: Engine,
    cli: Cli,
    spawn: Spawner,
}

impl ProbeEngine {
    async fn confirm_account(&self, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = root.join("data");
        let limit = Duration::from_secs(30);
        let account = claude_account(&self.cli, &self.spawn, &cwd, limit).await.ok_or("account status unavailable")?;
        if account.billing != Some(Billing::Subscription) || account.overage_active == Some(true) || account.paid_fallback == Some(true) {
            return Err("the probe requires a verified subscription with no active overage".into());
        }
        let mut events = claude_limits(&self.cli, &self.spawn, &cwd, limit).await;
        let fallback = events
            .iter()
            .filter_map(|e| match e {
                EngineEvent::Signals(s) => s.paid_fallback,
                _ => None,
            })
            .next_back();
        if fallback != Some(false) {
            return Err("the probe requires a fresh reading that paid fallback is off".into());
        }
        // Publishing subscription status can wake the wait immediately. Admit it only after the control read passes.
        events.push(EngineEvent::Signals(account));
        self.note_account_events(Vendor::Claude, events);
        Ok(())
    }
}

impl std::ops::Deref for ProbeEngine {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &self.engine
    }
}
impl Drop for ProbeEngine {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let out = command
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .output()?;
    if !out.status.success() {
        return Err("isolated repository setup failed".into());
    }
    Ok(())
}

fn open(root: &Path, notify: Arc<Notify>, calls: Arc<AtomicUsize>, allow_turn: bool) -> Result<ProbeEngine, Box<dyn std::error::Error>> {
    let cli = find_cli("claude").ok_or("CLI not installed")?;
    let spawn = real_spawner();
    let control_started = AtomicBool::new(false);
    let capped: Spawner = Arc::new(move |command: &str, args: &[String], cwd: &Path| {
        let mut args = args.to_vec();
        if args.iter().any(|a| a == "-p" || a == "--print") {
            if args.windows(2).any(|pair| pair == ["--max-budget-usd", "0.005"]) && args.windows(2).any(|pair| pair == ["--tools", ""]) {
                if control_started.swap(true, Ordering::SeqCst) {
                    return Err(std::io::Error::other("probe account-control budget exhausted"));
                }
            } else {
                let attempts = calls.fetch_add(1, Ordering::SeqCst);
                if !allow_turn || attempts >= 1 || args.iter().any(|a| a == "--max-budget-usd") {
                    return Err(std::io::Error::other("probe model-call budget exhausted"));
                }
                args.extend(["--max-budget-usd".into(), "0.07".into()]);
            }
        }
        spawn(command, &args, cwd)
    });
    let engine = Engine::new(
        EngineOptions {
            data_dir: root.join("data"),
            #[cfg(windows)]
            chat_dir: root.join("chat"),
            clis: BTreeMap::from([(Vendor::Claude, cli.clone())]),
            spawn: capped.clone(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: Some(Arc::new(|_| Box::pin(async { None }))), // No auxiliary title/router calls in this probe.
        },
        Arc::new(move |_| notify.notify_one()),
    )?;
    Ok(ProbeEngine { engine, cli, spawn: capped })
}

async fn observe(
    engine: &Engine,
    id: &str,
    notify: &Notify,
    condition: impl Fn(&Conversation) -> bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let event = notify.notified();
            if let Some(c) = engine.open(id) {
                if c.spent_usd >= 0.30 {
                    return Err("probe reached its $0.30 reported-cost limit".to_string());
                }
                if let Some(problem) = c.waiting_on.as_ref().and_then(|w| w.problem.as_ref()) {
                    return Err(format!("probe return blocked: {problem}; reported cost ${:.6}", c.spent_usd));
                }
                if c.messages.last().is_some_and(|m| m.status == Some(Status::Error)) {
                    return Err(format!("probe turn failed; reported cost ${:.6}", c.spent_usd));
                }
                if condition(&c) {
                    return Ok(());
                }
            }
            event.await;
        }
    })
    .await
    .map_err(|_| "probe timed out; inspect its isolated data")??;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This example never sends a turn through a pay-per-use environment.
    if claude_env_billed(std::env::vars_os().map(|(name, _)| name)) {
        return Err("a usage-billed environment is not supported by the live probe".into());
    }
    tokio::time::timeout(Duration::from_secs(240), run()).await.map_err(|_| "probe exceeded 240 seconds")?
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(std::env::current_dir()?.canonicalize()?, expected);
    let target = expected.join("target");
    let mut args = std::env::args_os().skip(1);
    if let Some(stage) = args.next() {
        let name = args.next().ok_or("missing probe fixture")?;
        let suffix = name.to_str().and_then(|s| s.strip_prefix("autonomy-probe-")).ok_or("invalid probe fixture")?;
        let root = target.join(&name).canonicalize()?;
        if suffix.len() != 32
            || !suffix.bytes().all(|b| b.is_ascii_hexdigit())
            || root.parent() != Some(target.canonicalize()?.as_path())
            || args.next().is_some()
        {
            return Err("invalid probe fixture".into());
        }
        return match stage.to_str() {
            Some("prepare-wait") => prepare_wait(&root).await,
            Some("restore-wait") => restore_wait(&root).await,
            Some("reopen-reply") => reopen_reply(&root).await,
            _ => Err("unknown probe stage".into()),
        };
    }
    let started = Instant::now();
    let name = format!("autonomy-probe-{}", uuid::Uuid::new_v4().simple());
    let root = target.join(&name);
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q"])?;
    std::fs::write(repo.join("README.md"), "Synthetic autonomy probe.\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["-c", "user.name=Probe", "-c", "user.email=probe@example.invalid", "commit", "-qm", "seed"])?;
    println!("Data: {}", root.display());
    let exe = std::env::current_exe()?;
    let mut pids = std::collections::BTreeSet::new();
    for stage in ["prepare-wait", "restore-wait", "reopen-reply"] {
        let mut command = tokio::process::Command::new(&exe);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.args([stage, &name]).kill_on_drop(true).spawn()?;
        assert!(pids.insert(child.id().ok_or("probe stage had no process ID")?), "the probe must use three distinct processes");
        let status = child.wait().await?;
        if !status.success() {
            return Err(format!("probe stage {stage} ended with {status}").into());
        }
    }
    let receipt = format!(
        "{}All three processes exited in {:.1}s.\n",
        std::fs::read_to_string(root.join("observed.txt"))?,
        started.elapsed().as_secs_f64()
    );
    std::fs::write(root.join("receipt.txt"), &receipt)?;
    print!("{receipt}");
    Ok(())
}

async fn prepare_wait(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let notify = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = open(root, notify.clone(), calls.clone(), true)?;
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    engine.confirm_account(root).await?;
    let project = engine.add_project(&root.join("repo").to_string_lossy())?;
    engine.set_keep_going_pace(0.1); // Existing test hook: a 30-second persisted deadline.
    let id = engine.send(SendRequest {
        conversation_id: None, area: Area::Project, project_id: Some(project.id),
        text: "Use the conversation tool action wait with only note `synthetic background result`. End this turn immediately. When Faber returns you after the deadline, reply exactly AUTONOMY_RETURNED and do not call wait again. Do not start helpers or commands.".into(),
        images: vec![], files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: Some(Effort::Low) },
    })?;
    observe(&engine, &id, &notify, |c| c.waiting_on.is_some() && c.messages.last().is_some_and(|m| m.status == Some(Status::Done))).await?;
    let wait = engine.open(&id).unwrap().waiting_on.unwrap();
    let saved =
        SavedWait { id, since: wait.since, until: wait.until, pid: std::process::id(), model_sessions: calls.load(Ordering::SeqCst) };
    assert_eq!(saved.model_sessions, 1);
    std::fs::write(root.join("saved-wait.json"), serde_json::to_vec(&saved)?)?;
    // Skip engine shutdown: the wait and reply must already be on disk.
    std::process::exit(0);
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system time after the epoch").as_millis()
}

fn returned(message: &Message) -> bool {
    message.text.trim() == "AUTONOMY_RETURNED" && message.status == Some(Status::Done)
}

fn check_return(c: &Conversation) {
    assert_eq!(c.messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.wait")).count(), 1);
    assert_eq!(c.messages.iter().filter(|m| returned(m)).count(), 1);
    assert!(c.waiting_on.is_none());
}

async fn restore_wait(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let saved: SavedWait = serde_json::from_slice(&std::fs::read(root.join("saved-wait.json"))?)?;
    let id = saved.id.as_str();
    let pid = std::process::id();
    assert_ne!(pid, saved.pid);
    let notify = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = open(root, notify.clone(), calls.clone(), true)?;
    let restored = engine.open(id).and_then(|c| c.waiting_on).ok_or("the pending wait was not persisted before process exit")?;
    assert_eq!((restored.since, restored.until), (saved.since, saved.until));
    let reopened_at = now_ms();
    assert!(reopened_at < u128::from(saved.until), "reopened after the deadline; the run cannot verify an early return");
    assert!(restored.problem.is_none(), "reopened with a blocker before the deadline");
    let blocked_at = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let event = notify.notified();
            if engine.open(id).unwrap().waiting_on.as_ref().and_then(|w| w.problem.as_deref()) == Some("error.noSubscription") {
                break now_ms();
            }
            event.await;
        }
    })
    .await
    .map_err(|_| "deadline did not expose unverified account")?;
    assert!(blocked_at >= u128::from(saved.until), "the engine tried to return before the saved deadline");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    engine.confirm_account(root).await?;
    observe(&engine, id, &notify, |c| c.messages.iter().any(returned)).await?;
    let c = engine.open(id).unwrap();
    check_return(&c);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut system = sysinfo::System::new_all();
    system.refresh_all();
    let resident = system.process(sysinfo::get_current_pid()?).map(|p| p.memory()).unwrap_or(0);
    engine.shutdown();
    let receipt = format!(
        "PASS: processes {} and {pid} kept deadline {}; restored at {reopened_at}, blocked at {blocked_at}, then returned once after account confirmation. Conversation cost ${:.6}; engine resident bytes {}; conversation sessions {}; account-control probes excluded; CLI tree peak not measured.\n",
        saved.pid,
        saved.until,
        c.spent_usd,
        resident,
        saved.model_sessions + calls.load(Ordering::SeqCst)
    );
    std::fs::write(root.join("observed.txt"), receipt)?;
    Ok(())
}

async fn reopen_reply(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let saved: SavedWait = serde_json::from_slice(&std::fs::read(root.join("saved-wait.json"))?)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = open(root, Arc::new(Notify::new()), calls.clone(), false)?;
    check_return(&engine.open(&saved.id).ok_or("the resumed conversation did not survive restart")?);
    engine.confirm_account(root).await?;
    check_return(&engine.open(&saved.id).ok_or("the resumed conversation disappeared after account confirmation")?);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    use std::io::Write;
    writeln!(
        std::fs::OpenOptions::new().append(true).open(root.join("observed.txt"))?,
        "Process {} reopened one reply, with no pending wait or model attempt after account confirmation.",
        std::process::id()
    )?;
    Ok(())
}
