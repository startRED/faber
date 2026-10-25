//! Manual live probe of self-development (R198): a project's agent on the real Claude Code CLI starts a conversation of its
//! own with Faber's conversation tool, reads it back, and the new conversation does its piece of the work. It runs in a
//! throwaway git repository under this worktree's `target/` folder, on the person's own subscription, and spends one small
//! Haiku turn for each of the two conversations. Run from the worktree root:
//! `cargo run -p faber-engine --example selfdev_probe`
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Permissions, Role, Settings, Status, Vendor};
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

const BRIEF: &str = "Create the file NOTES.md in the folder you work in, containing the single line `probe ok`, then stop.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current = std::env::current_dir()?;
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(current.canonicalize()?, expected, "run only from this worktree's root");
    let world = expected.join("target").join(format!("selfdev-live-{}", uuid::Uuid::new_v4().simple()));
    let repo = world.join("repo");
    std::fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "A throwaway project for Faber's self-development probe.\n")?;
    git(&repo, &["add", "--", "README.md"]);
    git(&repo, &["-c", "user.name=Faber Probe", "-c", "user.email=probe@example.invalid", "commit", "-q", "-m", "first"]);

    let clis = BTreeMap::from([(Vendor::Claude, find_cli("claude").ok_or("Claude Code is not installed")?)]);
    let engine = Engine::new(
        EngineOptions {
            data_dir: world.join("state"),
            clis,
            spawn: proc::real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )?;
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    let project = engine.add_project(&repo.to_string_lossy())?;
    // A fresh data folder does not know the plan yet, and Faber starts nothing on its own on an AI that may bill per use.
    // The installed app checks the plan at start through the CLI's own account command; the probe does the same.
    engine.check_plans(Duration::from_secs(30)).await;
    let haiku = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: Some(Effort::Low) };
    let prompt = format!(
        "Use Faber's conversation tool exactly once to start a conversation titled \"Notes\" with this brief: \"{BRIEF}\" Do not \
         create that file yourself. Then call the conversation tool again with only the new conversation's id, and end with one \
         line saying what it answered."
    );
    let started = Instant::now();
    let parent = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id),
        text: prompt,
        images: vec![],
        files: vec![],
        choice: haiku,
    })?;

    let settled = |id: &str| {
        engine
            .open(id)
            .and_then(|c| c.messages.last().cloned())
            .filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))
    };
    let mut child = None;
    let mut child_done = None;
    let mut parent_done = None;
    while started.elapsed() < Duration::from_secs(240) && (parent_done.is_none() || child_done.is_none()) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if parent_done.is_none() {
            parent_done = settled(&parent);
        }
        if child.is_none() {
            child = engine.state().conversations.into_iter().find(|c| c.title == "Notes").map(|c| c.id);
        }
        if let (Some(id), None) = (&child, &child_done) {
            child_done = settled(id);
        }
    }
    engine.shutdown();

    let reply = parent_done.or_else(|| engine.open(&parent).and_then(|c| c.messages.last().cloned()));
    let calls: Vec<String> = reply
        .iter()
        .flat_map(|m| m.tools.iter().flatten())
        .filter(|t| t.name == faber_engine::sibling::TOOL)
        .map(|t| format!("{}:{}", t.ok.map_or("open", |ok| if ok { "ok" } else { "failed" }), t.input))
        .collect();
    let notes: Vec<String> = git(&repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(|w| Path::new(w).join("NOTES.md"))
        .filter(|p| p.is_file())
        .map(|p| format!("{} = {:?}", p.display(), std::fs::read_to_string(&p).unwrap_or_default().trim()))
        .collect();
    println!("seconds: {}", started.elapsed().as_secs());
    println!("parent status: {:?}, error: {:?}", reply.as_ref().and_then(|m| m.status), reply.as_ref().and_then(|m| m.error.clone()));
    println!("parent conversation-tool calls: {calls:#?}");
    println!("parent answer: {}", reply.as_ref().map_or("", |m| m.text.trim()));
    println!("child: {child:?}, status: {:?}", child_done.as_ref().and_then(|m| m.status));
    println!("child answer: {}", child_done.as_ref().map_or("", |m| m.text.trim()));
    println!("NOTES.md found in: {notes:#?}");
    let ok = calls.iter().any(|c| c.starts_with("ok:")) && child_done.is_some_and(|m| m.status == Some(Status::Done)) && !notes.is_empty();
    println!("{}", if ok { "PROBE OK" } else { "PROBE FAILED" });
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&world) {
            Ok(()) => break,
            Err(error) if attempt == 19 => eprintln!("could not remove {}: {error}", world.display()),
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    if ok { Ok(()) } else { Err("self-development probe failed".into()) }
}
