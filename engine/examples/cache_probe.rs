//! Manual measurement of the CLIs' prompt cache and of what a quota pulse costs. Never part of `cargo t`: it talks to the real
//! CLIs with the user's own sign-in and spends a little real quota.
//!
//!     cargo run -p faber-engine --example cache_probe -- <scenario> [argument]
//!
//! Scenarios (each prints one markdown table row per turn):
//!   claude-turns           three turns in one process
//!   claude-resume <secs>   a turn, the process closes, `secs` pass, a new process resumes the session
//!   claude-effort          a turn at medium effort, then a resumed turn at high
//!   claude-pulse <n>       `n` one-word questions in a row, and how far the quota windows moved
//!   codex-turns            three turns on one thread
//!   codex-effort           a turn at low effort, then a turn at medium on the same thread
//!   codex-gap <mins>       a turn, `mins` pass, another turn on the same thread
//!   codex-resume           a turn, the thread is dropped, a new thread resumes it (the `total` after a resume)

use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
use faber_engine::cli::{Cli, find_cli};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread};
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::model::{Area, Permissions};
use faber_engine::proc::{Spawner, real_spawner};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const CLAUDE_MODEL: &str = "claude-sonnet-5-5";
const CODEX_MODEL: &str = "gpt-6-luna";
/// More than a thousand tokens, so a cache has something to hold; the same text every time.
const FILLER_SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the committee reviews the quarterly report. ";
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

fn question(n: usize) -> String {
    format!("{}\n\nQuestion {n}: reply with the single word ok.", FILLER_SENTENCE.repeat(120))
}

/// The CLI the way the engine starts it: the command and, for npm shims, the prefix that goes before the arguments.
fn spawner(cli: &Cli) -> Spawner {
    let (base, prefix, command) = (real_spawner(), cli.prefix.clone(), cli.command.clone());
    Arc::new(move |_bin, args, cwd| base(&command, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd))
}

type Windows = Vec<(String, f64)>;

/// One finished turn.
#[derive(Debug, Default)]
struct Turn {
    session: String,
    input: u64,
    cached: u64,
    written: u64,
    output: u64,
    incomplete: bool,
    windows: Windows,
}

async fn finish(rx: &mut UnboundedReceiver<EngineEvent>) -> Turn {
    let mut turn = Turn::default();
    loop {
        match tokio::time::timeout(TURN_TIMEOUT, rx.recv()).await.expect("no answer in time").expect("session ended") {
            EngineEvent::Started { session_id, .. } => turn.session = session_id,
            EngineEvent::Quota { window, used, .. } => turn.windows.push((window, used)),
            EngineEvent::Done { input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, incomplete, .. } => {
                (turn.input, turn.cached, turn.written, turn.output, turn.incomplete) =
                    (input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, incomplete);
                return turn;
            }
            EngineEvent::Error { message, fatal } => panic!("the CLI said: {message} (fatal: {fatal})"),
            _ => {}
        }
    }
}

fn row(scenario: &str, label: &str, t: &Turn) {
    let hit = if t.input == 0 { 0.0 } else { t.cached as f64 * 100.0 / t.input as f64 };
    println!(
        "| {scenario} | {label} | {} | {} | {} | {hit:.1}% | {} | {} |",
        t.input,
        t.cached,
        t.written,
        t.output,
        if t.incomplete { "incomplete" } else { "" }
    );
}

struct Probe {
    dir: PathBuf,
}

impl Probe {
    fn claude(&self, resume: Option<String>, effort: &str) -> (Session, UnboundedReceiver<EngineEvent>) {
        let cli = find_cli("claude").expect("claude is not installed");
        let (tx, rx) = unbounded_channel();
        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
        let launch = ClaudeLaunch {
            area: Area::Chat,
            model: Some(CLAUDE_MODEL.into()),
            effort: Some(effort.into()),
            resume,
            user_hooks: false,
            instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
        };
        let session = start_claude(
            ClaudeOptions { bin: cli.command.clone(), cwd: self.dir.clone(), launch, spawn: spawner(&cli), sandbox: None },
            sink,
        );
        (session, rx)
    }

    /// A side question with its own instructions: no tools, no saved session, which is what a pulse is.
    fn pulse(&self) -> (Session, UnboundedReceiver<EngineEvent>) {
        let cli = find_cli("claude").expect("claude is not installed");
        let (tx, rx) = unbounded_channel();
        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
        let launch = ClaudeLaunch {
            area: Area::Chat,
            model: Some("claude-haiku-5-5".into()),
            effort: Some("low".into()),
            resume: None,
            user_hooks: false,
            instructions: Some("Reply with the single word ok.".into()),
            user_name: None,
            permissions: Permissions::Ask,
        };
        let session = start_claude(
            ClaudeOptions { bin: cli.command.clone(), cwd: self.dir.clone(), launch, spawn: spawner(&cli), sandbox: None },
            sink,
        );
        (session, rx)
    }

    fn codex_server(&self) -> CodexServer {
        let cli = find_cli("codex").expect("codex is not installed");
        CodexServer::start(CodexServerOptions {
            bin: cli.command.clone(),
            cwd: self.dir.clone(),
            area: Area::Chat,
            user_hooks: false,
            windows: cfg!(windows),
            spawn: spawner(&cli),
        })
    }

    fn codex_thread(&self, server: &CodexServer, effort: &str, resume: Option<String>) -> (Session, UnboundedReceiver<EngineEvent>) {
        let (tx, rx) = unbounded_channel();
        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
        let thread = CodexThread {
            cwd: self.dir.clone(),
            model: Some(CODEX_MODEL.into()),
            effort: Some(effort.into()),
            resume,
            instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
        };
        (server.open(thread, sink), rx)
    }
}

async fn wait(label: &str, d: Duration) {
    eprintln!("waiting {label} ...");
    tokio::time::sleep(d).await;
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenario = args.first().map(String::as_str).unwrap_or("help");
    let number = |default: u64| args.get(1).and_then(|a| a.parse::<u64>().ok()).unwrap_or(default);
    let dir = std::env::temp_dir().join("faber-cache-probe");
    std::fs::create_dir_all(&dir).expect("a folder to work in");
    let probe = Probe { dir };
    println!("| scenario | turn | prompt tokens | cache read | cache written | hit | output | note |");
    println!("|---|---|---|---|---|---|---|---|");
    match scenario {
        "claude-turns" => {
            let (session, mut rx) = probe.claude(None, "medium");
            for n in 1..=3 {
                session.send(UserTurn::text(question(n)));
                row(scenario, &format!("turn {n}"), &finish(&mut rx).await);
            }
            session.close();
        }
        "claude-resume" => {
            let (first, mut rx) = probe.claude(None, "medium");
            first.send(UserTurn::text(question(1)));
            let a = finish(&mut rx).await;
            row(scenario, "turn 1", &a);
            first.close();
            wait(&format!("{} s", number(360)), Duration::from_secs(number(360))).await;
            let (second, mut rx) = probe.claude(Some(a.session.clone()), "medium");
            second.send(UserTurn::text(question(2)));
            row(scenario, &format!("resumed after {} s", number(360)), &finish(&mut rx).await);
            second.close();
        }
        "claude-effort" => {
            let (first, mut rx) = probe.claude(None, "medium");
            first.send(UserTurn::text(question(1)));
            let a = finish(&mut rx).await;
            row(scenario, "medium", &a);
            first.close();
            let (second, mut rx) = probe.claude(Some(a.session.clone()), "high");
            second.send(UserTurn::text(question(2)));
            row(scenario, "high, resumed", &finish(&mut rx).await);
            second.close();
        }
        "claude-pulse" => {
            let n = number(200);
            let (mut first, mut last): (Windows, Windows) = (vec![], vec![]);
            for i in 0..n {
                let (session, mut rx) = probe.pulse();
                session.send(UserTurn::text("ok"));
                let t = finish(&mut rx).await;
                session.close();
                if i == 0 {
                    first = t.windows.clone();
                }
                last = t.windows;
            }
            println!("\npulses: {n}");
            for (window, after) in &last {
                let before = first.iter().find(|(w, _)| w == window).map_or(0.0, |(_, u)| *u);
                println!(
                    "{window}: {:.0}% -> {:.0}% : {:.4} points per pulse",
                    before * 100.0,
                    after * 100.0,
                    (after - before) * 100.0 / n as f64
                );
            }
        }
        "codex-turns" | "codex-effort" | "codex-gap" | "codex-resume" => codex(&probe, scenario, number(35)).await,
        _ => eprintln!(
            "scenarios: claude-turns, claude-resume <secs>, claude-effort, claude-pulse <n>, codex-turns, codex-effort, codex-gap <mins>, codex-resume"
        ),
    }
}

async fn codex(probe: &Probe, scenario: &str, minutes: u64) {
    let server = probe.codex_server();
    let (first, mut rx) = probe.codex_thread(&server, "low", None);
    first.send(UserTurn::text(question(1)));
    let a = finish(&mut rx).await;
    row(scenario, "turn 1", &a);
    match scenario {
        "codex-turns" => {
            for n in 2..=3 {
                first.send(UserTurn::text(question(n)));
                row(scenario, &format!("turn {n}"), &finish(&mut rx).await);
            }
        }
        "codex-effort" => {
            first.close();
            let (second, mut rx) = probe.codex_thread(&server, "medium", Some(a.session.clone()));
            second.send(UserTurn::text(question(2)));
            row(scenario, "medium, same thread", &finish(&mut rx).await);
            second.close();
        }
        "codex-gap" => {
            wait(&format!("{minutes} min"), Duration::from_secs(minutes * 60)).await;
            first.send(UserTurn::text(question(2)));
            row(scenario, &format!("after {minutes} min"), &finish(&mut rx).await);
        }
        _ => {
            first.close();
            let (second, mut rx) = probe.codex_thread(&server, "low", Some(a.session.clone()));
            second.send(UserTurn::text(question(2)));
            row(scenario, "resumed thread", &finish(&mut rx).await);
            second.close();
        }
    }
    first.close();
    server.retire();
}
