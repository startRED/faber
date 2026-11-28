//! Manual live probe of compaction by the agent (R221) and Keep going to the end (R223) on the real Claude Code CLI. Part one:
//! a project's agent reads its context with Faber's conversation tool and asks for a compaction with a `next`; Faber compacts
//! after the turn and sends `next` back. Part two: Keep going takes a three-step task to the end with no one answering. It runs
//! in a throwaway git repository under this worktree's `target/` folder, on the person's own subscription, in small Haiku
//! turns. Run from the worktree root: `cargo run -p faber-engine --example keep_compact_probe`
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Conversation, Permissions, Role, Settings, Status, Vendor};
use faber_engine::proc;
use faber_engine::router::{Choice, Effort};
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn hidden(bin: &str) -> Command {
    let command = Command::new(bin);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(0x0800_0000);
        command
    };
    command
}

fn git(root: &Path, args: &[&str]) -> String {
    let out =
        hidden("git").args(args).current_dir(root).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
    assert!(out.status.success(), "throwaway git failed: {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}

const BACK: &str = "back after compact";

fn haiku() -> Choice {
    Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: Some(Effort::Low) }
}

/// The conversation's messages in one line each: who, why Faber sent it, status, the first words.
fn story(c: &Conversation) -> String {
    c.messages
        .iter()
        .map(|m| {
            let words: String = m.text.trim().chars().take(90).collect();
            format!(
                "  {:?} wake={:?} status={:?} halt={:?}: {}",
                m.role,
                m.wake.as_ref().map(|w| w.why.as_str()),
                m.status,
                m.halt,
                words.replace('\n', " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn until(engine: &Engine, id: &str, limit: Duration, done: impl Fn(&Conversation) -> bool) -> Option<Conversation> {
    let started = Instant::now();
    while started.elapsed() < limit {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Some(c) = engine.open(id).filter(|c| done(c)) {
            return Some(c);
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current = std::env::current_dir()?;
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(current.canonicalize()?, expected, "run only from this worktree's root");
    let world = expected.join("target").join(format!("keep-compact-live-{}", uuid::Uuid::new_v4().simple()));
    let repo = world.join("repo");
    std::fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "A throwaway project for Faber's compaction and Keep going probe.\n")?;
    git(&repo, &["add", "--", "README.md"]);
    git(&repo, &["-c", "user.name=Faber Probe", "-c", "user.email=probe@example.invalid", "commit", "-q", "-m", "first"]);

    let clis = BTreeMap::from([(Vendor::Claude, find_cli("claude").ok_or("Claude Code is not installed")?)]);
    let engine = Engine::new(
        EngineOptions {
            data_dir: world.join("state"),
            #[cfg(windows)]
            chat_dir: world.join("chat-work"),
            clis,
            spawn: proc::real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )?;
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    let project = engine.add_project(&repo.to_string_lossy())?;
    engine.check_plans(Duration::from_secs(30)).await;
    let started = Instant::now();

    // Part one: the agent reads its context and compacts itself, and its `next` comes back to it.
    let compact = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id.clone()),
        text: format!(
            "This topic is finished. Use Faber's conversation tool with the action `context` once, then with the action \
             `compact` and `next` set to: \"Reply with exactly: {BACK}\". Then end your turn with one short line."
        ),
        images: vec![],
        files: vec![],
        choice: haiku(),
    })?;
    let compacted = until(&engine, &compact, Duration::from_secs(300), |c| {
        c.messages.iter().any(|m| m.role == Role::Assistant && m.status == Some(Status::Done) && m.text.to_lowercase().contains(BACK))
            && c.messages.iter().any(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.compacted"))
    })
    .await;
    let c = engine.open(&compact).ok_or("the compaction conversation is gone")?;
    println!("== compaction ({} s)", started.elapsed().as_secs());
    println!("{}", story(&c));
    println!("context now: {:?}", c.context);
    let compact_ok = compacted.is_some();

    // Part two: Keep going takes a three-step task to the end by itself.
    let steps = Instant::now();
    let keep = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id.clone()),
        text: "Write the file steps.txt in the folder you work in, one line per turn, over three turns: in this turn write \
               only the line `one` and end your turn saying what is left. Later turns add `two`, then `three`. The task is done \
               when steps.txt holds the three lines."
            .into(),
        images: vec![],
        files: vec![],
        choice: haiku(),
    })?;
    // Waits of Keep going (rests, retries) run at a tenth of their real length so the probe ends in minutes.
    engine.set_keep_going_pace(0.1);
    until(&engine, &keep, Duration::from_secs(120), |c| c.messages.iter().any(|m| m.role == Role::Assistant && m.status.is_some())).await;
    engine.keep_going(&keep, true)?;
    let ended = until(&engine, &keep, Duration::from_secs(900), |c| c.keep_going.is_none()).await;
    let c = engine.open(&keep).ok_or("the Keep going conversation is gone")?;
    let files: Vec<String> = git(&repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(|w| Path::new(w).join("steps.txt"))
        .filter(|p| p.is_file())
        .map(|p| std::fs::read_to_string(&p).unwrap_or_default())
        .collect();
    println!("== keep going ({} s)", steps.elapsed().as_secs());
    println!("{}", story(&c));
    println!("ended by itself: {}, steps.txt: {files:?}", ended.is_some());
    let lines_ok = files.iter().any(|f| f.lines().map(str::trim).collect::<Vec<_>>() == ["one", "two", "three"]);
    let keep_ok = ended.is_some() && lines_ok;
    engine.shutdown();

    println!("compaction: {}", if compact_ok { "OK" } else { "FAILED" });
    println!("keep going: {}", if keep_ok { "OK" } else { "FAILED" });
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&world) {
            Ok(()) => break,
            Err(error) if attempt == 19 => eprintln!("could not remove {}: {error}", world.display()),
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    if compact_ok && keep_ok { Ok(()) } else { Err("keep going and compaction probe failed".into()) }
}
