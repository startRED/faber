//! Manual opt-in live probe. Uses only an isolated target directory and Haiku/low.
//! Run from the worktree root with `node scripts/cargo.mjs run -p faber-engine --example autonomy_probe`.
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Permissions, Settings, Status, Vendor};
use faber_engine::router::{Choice, Effort};
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeMap, path::Path, process::Command, sync::Arc, time::Duration};
use tokio::sync::Notify;

struct ProbeEngine(Engine);
impl std::ops::Deref for ProbeEngine {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &self.0
    }
}
impl Drop for ProbeEngine {
    fn drop(&mut self) {
        self.0.shutdown();
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

fn open(root: &Path, notify: Arc<Notify>, calls: Arc<AtomicUsize>) -> Result<ProbeEngine, Box<dyn std::error::Error>> {
    let cli = find_cli("claude").ok_or("CLI not installed")?;
    let spawn = faber_engine::proc::real_spawner();
    let capped = Arc::new(move |command: &str, args: &[String], cwd: &Path| {
        let mut args = args.to_vec();
        if args.iter().any(|a| a == "-p" || a == "--print") {
            if calls.fetch_add(1, Ordering::SeqCst) >= 4 {
                return Err(std::io::Error::other("probe model-call budget exhausted"));
            }
            if !args.iter().any(|a| a == "--max-budget-usd") {
                args.extend(["--max-budget-usd".into(), "0.07".into()]);
            }
        }
        spawn(command, &args, cwd)
    });
    Ok(ProbeEngine(Engine::new(
        EngineOptions {
            data_dir: root.join("data"),
            #[cfg(windows)]
            chat_dir: root.join("chat"),
            clis: BTreeMap::from([(Vendor::Claude, cli)]),
            spawn: capped,
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: Some(Arc::new(|_| Box::pin(async { None }))), // No auxiliary title/router calls in this probe.
        },
        Arc::new(move |_| notify.notify_one()),
    )?))
}

async fn observe(
    engine: &Engine,
    id: &str,
    notify: &Notify,
    condition: impl Fn(&faber_engine::model::Conversation) -> bool,
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
    tokio::time::timeout(Duration::from_secs(180), run()).await.map_err(|_| "probe exceeded 180 seconds")?
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(std::env::current_dir()?.canonicalize()?, expected);
    let root = expected.join("target").join(format!("autonomy-probe-{}", uuid::Uuid::new_v4().simple()));
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q"])?;
    std::fs::write(repo.join("README.md"), "Synthetic autonomy probe.\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["-c", "user.name=Probe", "-c", "user.email=probe@example.invalid", "commit", "-qm", "seed"])?;
    let notify = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = open(&root, notify.clone(), calls.clone())?;
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    engine.check_plans(Duration::from_secs(30)).await;
    let project = engine.add_project(&repo.to_string_lossy())?;
    engine.set_keep_going_pace(0.1); // Existing test hook: a 30 second persisted deadline.
    let id = engine.send(SendRequest {
        conversation_id: None, area: Area::Project, project_id: Some(project.id),
        text: "Use the conversation tool action wait with only note `synthetic background result`. End this turn immediately. When Faber returns you after the deadline, reply exactly AUTONOMY_RETURNED and do not call wait again. Do not start helpers or commands.".into(),
        images: vec![], files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: Some(Effort::Low) },
    })?;
    observe(&engine, &id, &notify, |c| c.waiting_on.is_some() && c.messages.last().is_some_and(|m| m.status == Some(Status::Done))).await?;
    let until = engine.open(&id).unwrap().waiting_on.unwrap().until;
    drop(engine);
    let engine = open(&root, notify.clone(), calls.clone())?;
    assert_eq!(engine.open(&id).unwrap().waiting_on.unwrap().until, until);
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let event = notify.notified();
            if engine.open(&id).unwrap().waiting_on.as_ref().and_then(|w| w.problem.as_deref()) == Some("reason.budget.other") {
                break;
            }
            event.await;
        }
    })
    .await
    .map_err(|_| "deadline did not expose unverified account")?;
    engine.check_plans(Duration::from_secs(30)).await;
    observe(&engine, &id, &notify, |c| c.messages.iter().any(|m| m.text.trim() == "AUTONOMY_RETURNED" && m.status == Some(Status::Done)))
        .await?;
    let c = engine.open(&id).unwrap();
    assert_eq!(c.messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.wait")).count(), 1);
    let mut system = sysinfo::System::new_all();
    system.refresh_all();
    let resident = system.process(sysinfo::get_current_pid()?).map(|p| p.memory()).unwrap_or(0);
    engine.shutdown();
    let receipt = format!(
        "PASS: account confirmed after deadline across restart; one wait wake. Reported cost ${:.6}; engine resident bytes {}; model sessions {}; total budget $0.30; CLI tree peak not measured.\n",
        c.spent_usd,
        resident,
        calls.load(Ordering::SeqCst)
    );
    std::fs::write(root.join("receipt.txt"), &receipt)?;
    println!("{receipt}");
    println!("Data: {}", root.display());
    Ok(())
}
