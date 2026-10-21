//! Manual measurement of the CLIs' prompt cache and of what a quota pulse costs. Never part of `cargo t`: it talks to the real
//! CLIs with the user's own sign-in and spends a little real quota.
//!
//!     cargo run -p faber-engine --example cache_probe -- <scenario> [argument]
//!
//! Scenarios (each prints one markdown table row per turn):
//!   claude-turns           three turns in one process
//!   claude-resume <secs>   a turn, the process closes, `secs` pass, a new process resumes the session
//!   claude-effort          a turn at medium effort, then a resumed turn at high
//!   claude-pulse <n>       one baseline, then `n` measured one-word questions; shared-account movement is an upper bound
//!   codex-turns            three turns on one thread
//!   codex-effort           a turn at low effort, then a turn at medium on the same thread
//!   codex-gap <mins>       a turn, `mins` pass, another turn on the same thread
//!   codex-resume           a turn, the thread is dropped, a new thread resumes it (the `total` after a resume)

use faber_engine::budget::{Billing, Purpose, Snap, Window, gate};
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

type Windows = Vec<(String, f64, f64)>;

/// One finished turn.
#[derive(Debug, Default)]
struct Turn {
    session: String,
    ok: bool,
    input: u64,
    cached: u64,
    written: u64,
    written_1h: u64,
    output: u64,
    incomplete: bool,
    windows: Windows,
    own_cost: Option<f64>,
    overage: bool,
    rejected: bool,
    paid_possible: Option<bool>,
}

async fn finish(rx: &mut UnboundedReceiver<EngineEvent>, stop: Option<&dyn Fn()>) -> Turn {
    let mut turn = Turn::default();
    loop {
        match tokio::time::timeout(TURN_TIMEOUT, rx.recv()).await.expect("no answer in time").expect("session ended") {
            EngineEvent::Started { session_id, .. } => turn.session = session_id,
            EngineEvent::Quota { window, used, resets_at } => {
                turn.windows.retain(|(name, _, _)| name != &window);
                turn.windows.push((window, used, resets_at));
            }
            EngineEvent::Signals(s) => {
                turn.overage |= s.overage_active == Some(true);
                turn.rejected |= matches!(s.reach, Some(faber_engine::budget::Reach::Out(_)))
                    || s.billing.is_some_and(|billing| billing != Billing::Subscription);
                if s.paid_fallback.is_some() {
                    turn.paid_possible = s.paid_fallback;
                }
                if (turn.overage || turn.rejected)
                    && let Some(stop) = stop
                {
                    stop();
                    return turn;
                }
            }
            e @ EngineEvent::Done {
                ok,
                input_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cache_write_1h_tokens,
                output_tokens,
                incomplete,
                ..
            } => {
                turn.ok = ok;
                turn.own_cost = faber_engine::usage::fresh_price(&e, "claude-haiku-5-5").0;
                (turn.input, turn.cached, turn.written, turn.written_1h, turn.output, turn.incomplete) =
                    (input_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens, output_tokens, incomplete);
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
        "| {scenario} | {label} | {} | {} | {} | {} | {hit:.1}% | {} | {} |",
        t.input,
        t.cached,
        t.written,
        t.written_1h,
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
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            area: Area::Chat,
            model: Some(CLAUDE_MODEL.into()),
            effort: Some(effort.into()),
            resume,
            user_hooks: false,
            instructions: None,
            group_instructions: None,
            user_name: None,
            flags: Default::default(),
            permissions: Permissions::Ask,
            tuning: Default::default(),
            tools: Default::default(),
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
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            area: Area::Chat,
            model: Some("claude-haiku-5-5".into()),
            effort: Some("low".into()),
            resume: None,
            user_hooks: false,
            instructions: Some("Reply with the single word ok.".into()),
            group_instructions: None,
            user_name: None,
            flags: Default::default(),
            permissions: Permissions::Ask,
            tuning: Default::default(),
            tools: Default::default(),
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
            permissions: Permissions::Ask,
            cli: cli.clone(),
            cwd: self.dir.clone(),
            area: Area::Chat,
            user_hooks: false,
            windows: cfg!(windows),
            spawn: spawner(&cli),
            tools: Default::default(),
        })
    }

    fn codex_thread(&self, server: &CodexServer, effort: &str, resume: Option<String>) -> (Session, UnboundedReceiver<EngineEvent>) {
        let (tx, rx) = unbounded_channel();
        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
        let thread = CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            cwd: self.dir.clone(),
            model: Some(CODEX_MODEL.into()),
            effort: Some(effort.into()),
            resume,
            instructions: None,
            group_instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
            leave_plan: false,
            tuning: Default::default(),
        };
        (server.open(thread, sink), rx)
    }
}

async fn wait(label: &str, d: Duration) {
    eprintln!("waiting {label} ...");
    tokio::time::sleep(d).await;
}

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    run();
}

#[tokio::main]
async fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenario = args.first().map(String::as_str).unwrap_or("help");
    let number = |default: u64| args.get(1).and_then(|a| a.parse::<u64>().ok()).unwrap_or(default);
    let dir = std::env::temp_dir().join("faber-cache-probe");
    std::fs::create_dir_all(&dir).expect("a folder to work in");
    let probe = Probe { dir };
    println!("| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |");
    println!("|---|---|---|---|---|---|---|---|---|");
    match scenario {
        "claude-turns" => {
            let (session, mut rx) = probe.claude(None, "medium");
            for n in 1..=3 {
                session.send(UserTurn::text(question(n)));
                row(scenario, &format!("turn {n}"), &finish(&mut rx, None).await);
            }
            session.close();
        }
        "claude-resume" => {
            let (first, mut rx) = probe.claude(None, "medium");
            first.send(UserTurn::text(question(1)));
            let a = finish(&mut rx, None).await;
            row(scenario, "turn 1", &a);
            first.close();
            wait(&format!("{} s", number(360)), Duration::from_secs(number(360))).await;
            let (second, mut rx) = probe.claude(Some(a.session.clone()), "medium");
            second.send(UserTurn::text(question(2)));
            row(scenario, &format!("resumed after {} s", number(360)), &finish(&mut rx, None).await);
            second.close();
        }
        "claude-effort" => {
            let (first, mut rx) = probe.claude(None, "medium");
            first.send(UserTurn::text(question(1)));
            let a = finish(&mut rx, None).await;
            row(scenario, "medium", &a);
            first.close();
            let (second, mut rx) = probe.claude(Some(a.session.clone()), "high");
            second.send(UserTurn::text(question(2)));
            row(scenario, "high, resumed", &finish(&mut rx, None).await);
            second.close();
        }
        "claude-pulse" => {
            let n = number(200);
            assert!((1..=200).contains(&n), "use between 1 and 200 measured pulses");
            let cli = find_cli("claude").expect("claude is not installed");
            let status = faber_engine::account::claude_account(&cli, &spawner(&cli), &probe.dir, Duration::from_secs(20)).await;
            assert!(status.is_some_and(|s| s.billing == Some(Billing::Subscription)), "subscription billing must be confirmed");
            assert!(!faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)), "a billing override is present");
            let mut snap = Snap { billing: Billing::Subscription, paid_fallback: true, ..Snap::default() };
            let (mut first, mut last): (Windows, Windows) = (vec![], vec![]);
            let (mut own, mut baseline_cost) = (0.0, 0.0);
            // The first pulse establishes the reading. Only the following n pulses belong to the measured interval.
            for i in 0..=n {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
                gate(&snap, Some("haiku"), Purpose::Summary, false, now).expect("the quota gate refused a pulse");
                let (session, mut rx) = probe.pulse();
                session.send(UserTurn::text("ok"));
                let close = || session.close();
                let result = tokio::time::timeout(Duration::from_secs(60), finish(&mut rx, Some(&close))).await;
                session.close();
                let t = result.expect("pulse timed out: measurement stopped");
                assert!(t.ok && !t.overage && !t.rejected, "paid usage or a limit was reported: measurement stopped");
                assert!(!t.windows.is_empty(), "no quota reading: measurement stopped");
                let cost = t.own_cost.expect("no own cost meter: measurement stopped");
                if i == 0 {
                    first = t.windows.clone();
                    baseline_cost = cost;
                } else {
                    movement(&first, &t.windows, i).expect("reset or incomplete reading: measurement stopped");
                    own += cost;
                }
                snap.windows = t
                    .windows
                    .iter()
                    .map(|(name, used, reset)| (name.clone(), Window { used: *used, resets_at: *reset, read_at: now, length: 0.0 }))
                    .collect();
                snap.paid_fallback = t.paid_possible.unwrap_or(true);
                last = t.windows;
            }
            println!("\nmeasured pulses: {n}; one baseline pulse excluded");
            println!("own API-equivalent USD: {own:.6}; per measured pulse: {:.6}; baseline: {baseline_cost:.6}", own / n as f64);
            for (window, per_pulse) in movement(&first, &last, n).expect("invalid measurement") {
                println!("{window}: {per_pulse:.4} account points per measured pulse (upper bound; shared-account use may contribute)");
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
    let a = finish(&mut rx, None).await;
    row(scenario, "turn 1", &a);
    match scenario {
        "codex-turns" => {
            for n in 2..=3 {
                first.send(UserTurn::text(question(n)));
                row(scenario, &format!("turn {n}"), &finish(&mut rx, None).await);
            }
        }
        "codex-effort" => {
            first.close();
            let (second, mut rx) = probe.codex_thread(&server, "medium", Some(a.session.clone()));
            second.send(UserTurn::text(question(2)));
            row(scenario, "medium, same thread", &finish(&mut rx, None).await);
            second.close();
        }
        "codex-gap" => {
            wait(&format!("{minutes} min"), Duration::from_secs(minutes * 60)).await;
            first.send(UserTurn::text(question(2)));
            row(scenario, &format!("after {minutes} min"), &finish(&mut rx, None).await);
        }
        _ => {
            first.close();
            let (second, mut rx) = probe.codex_thread(&server, "low", Some(a.session.clone()));
            second.send(UserTurn::text(question(2)));
            row(scenario, "resumed thread", &finish(&mut rx, None).await);
            second.close();
        }
    }
    first.close();
    server.retire();
}

/// Whole-account movement is an upper bound on a pulse's quota effect, never a meter of Faber's own money.
fn movement(before: &[(String, f64, f64)], after: &[(String, f64, f64)], measured: u64) -> Result<Vec<(String, f64)>, &'static str> {
    if measured == 0 || before.is_empty() {
        return Err("no measured interval");
    }
    before
        .iter()
        .map(|(name, used, reset)| {
            let (_, end, next_reset) = after.iter().find(|(n, _, _)| n == name).ok_or("missing window")?;
            if reset != next_reset
                || end < used
                || !faber_engine::ledger::plausible(*used, *reset)
                || !faber_engine::ledger::plausible(*end, *next_reset)
            {
                return Err("window reset or invalid reading");
            }
            Ok((name.clone(), (end - used) * 100.0 / measured as f64))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_baseline_is_excluded_and_reset_or_missing_readings_abort_measurement() {
        let baseline = vec![("five_hour".into(), 0.1, 1000.0)];
        let after = vec![("five_hour".into(), 0.3, 1000.0)];
        assert!((movement(&baseline, &after, 2).unwrap()[0].1 - 10.0).abs() < 1e-9);
        assert!(movement(&baseline, &after, 0).is_err());
        assert!(movement(&baseline, &[], 2).is_err());
        assert!(movement(&baseline, &[("five_hour".into(), 0.3, 2000.0)], 2).is_err());
        assert!(movement(&after, &baseline, 2).is_err());
        assert!(movement(&baseline, &[("five_hour".into(), f64::NAN, 1000.0)], 2).is_err());
        assert!(movement(&[("five_hour".into(), -0.1, 1000.0)], &after, 2).is_err());
    }
}

#[cfg(test)]
mod pulse_events {
    use super::*;
    use faber_engine::budget::Reach;
    use faber_engine::signals::Signals;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn paid_or_rejected_evidence_stops_immediately_without_waiting_for_done() {
        for evidence in [
            Signals { overage_active: Some(true), ..Default::default() },
            Signals { reach: Some(Reach::Out(None)), ..Default::default() },
            Signals { billing: Some(Billing::Unknown), ..Default::default() },
        ] {
            let (tx, mut rx) = unbounded_channel();
            tx.send(EngineEvent::Signals(evidence)).unwrap();
            let closed = AtomicBool::new(false);
            let close = || closed.store(true, Ordering::SeqCst);
            let turn = tokio::time::timeout(Duration::from_secs(1), finish(&mut rx, Some(&close))).await.unwrap();
            assert!(closed.load(Ordering::SeqCst));
            assert!(turn.overage || turn.rejected);
            assert!(!turn.ok);
        }
    }

    #[tokio::test]
    async fn duplicate_window_notifications_keep_the_latest_endpoint() {
        let (tx, mut rx) = unbounded_channel();
        for used in [0.1, 0.2] {
            tx.send(EngineEvent::Quota { window: "five_hour".into(), used, resets_at: 1000.0 }).unwrap();
        }
        tx.send(EngineEvent::Done {
            ok: true,
            reason: "completed".into(),
            cost_total_usd: Some(0.001),
            input_tokens: 10,
            output_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        })
        .unwrap();
        let turn = finish(&mut rx, None).await;
        assert_eq!(turn.windows, vec![("five_hour".into(), 0.2, 1000.0)]);
        assert!(turn.ok);
    }
}
