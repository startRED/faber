//! Opt-in, read-only GitHub proof with synthetic conversations and no model process.
//! Run from the workspace root with `node scripts/cargo.mjs run -p faber-engine --example remote_wait_probe`.
use faber_engine::model::FollowState;
use faber_engine::{Engine, EngineOptions};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

struct Probe(Engine);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::time::timeout(Duration::from_secs(150), run()).await.map_err(|_| "real GitHub return exceeded 150 seconds")?
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(std::env::current_dir()?.canonicalize()?, workspace);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    // The real reader rejects any neutral folder under a Git repository; keep this fixture outside the worktree.
    let root = std::env::temp_dir().join(format!("faber-remote-wait-probe-{}-{now}", std::process::id()));
    std::fs::create_dir(&root)?;
    let data = root.join("data");
    std::fs::create_dir_all(data.join("conversations"))?;
    // This historical public PR is merged. No GitHub write or active user conversation is involved.
    std::fs::write(
        data.join("conversations/remote-probe.json"),
        json!({
            "id":"remote-probe", "title":"Synthetic real remote return", "area":"project", "createdAt":now,"updatedAt":now,
            "sessions":{}, "waitingOn":{"note":"Public merged PR check", "since":now,"until":now+600000,
                "pr":{"number":719,"head":"synthetic-before-read","state":"pending"}},
            "messages":[{"id":"answer","role":"assistant","text":"Synthetic wait, no model permitted","status":"done","at":now,
                "follow":{"repo":"startRED/faber","number":719,"url":"https://github.com/startRED/faber/pull/719",
                    "state":"pending","head":"synthetic-before-read","since":now}}]
        })
        .to_string(),
    )?;
    let attempted = Arc::new(AtomicUsize::new(0));
    let counter = attempted.clone();
    let changed = Arc::new(Notify::new());
    let notify = changed.clone();
    let engine = Probe(Engine::new(
        EngineOptions {
            data_dir: data,
            #[cfg(windows)]
            chat_dir: root.join("chat"),
            clis: BTreeMap::new(),
            spawn: Arc::new(move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(std::io::Error::other("model process forbidden in remote proof"))
            }),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(1),
            runtime: tokio::runtime::Handle::current(),
            ask: Some(Arc::new(|_| Box::pin(async { None }))),
        },
        Arc::new(move |_| notify.notify_one()),
    )?);
    engine.0.settle_start();
    loop {
        let event = changed.notified();
        let c = engine.0.open("remote-probe").ok_or("synthetic conversation missing")?;
        let follow = c.messages.iter().find_map(|m| m.follow.as_ref()).ok_or("synthetic follow missing")?;
        if let Some(problem) = &follow.problem {
            return Err(format!("real remote reading failed: {problem}").into());
        }
        if follow.state == FollowState::Merged {
            assert_ne!(follow.head.as_deref(), Some("synthetic-before-read"));
            assert_eq!(
                c.waiting_on.as_ref().and_then(|w| w.problem.as_deref()),
                Some("reason.budget.other"),
                "the real merge requests a return through normal admission"
            );
            assert!(c.queued.is_empty());
            assert_eq!(attempted.load(Ordering::SeqCst), 0);
            println!(
                "PASS: real GitHub merged reading requested the saved wait return; admission visibly blocked it without a model call. Model processes: 0; model cost: 0. Data: {}",
                root.display()
            );
            break;
        }
        event.await;
    }
    Ok(())
}
