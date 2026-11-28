//! Manual measurement of what a wave of reports costs the agent that started the conversations (D1/D2 of the harness study:
//! reports that wait together go out as one turn, and an idle lead's wake waits a few seconds for the ones that follow).
//! Never part of `cargo t`: it runs the real Claude Code with the person's own sign-in and spends a little quota.
//!
//!     FABER_EVAL_TMP=<a folder outside any repository> node scripts/cargo.mjs run -p faber-engine --example fanout_eval [-- runs children]
//!
//! Each run gives a fresh project's agent (Haiku, medium effort) one request: start `children` conversations at once on Haiku
//! with low effort, each writing one small file, wait, then check every file. Runs alternate between batching off (each
//! report its own turn, as before) and on, so time of day and quota drift fall on both arms alike. Per run it prints the lead's
//! turns, the wakes it got and how many reports they carried, its cache reads, first-time tokens and dollars, the seconds
//! from the last conversation's end to the lead's next answer, whether every file is right, and whether every report arrived.
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Conversation, Message, Permissions, Role, Settings, Status, Vendor, Wake};
use faber_engine::proc;
use faber_engine::router::{Choice, Effort};
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A run that has not ended by then is cut and counted as failed.
const RUN_LIMIT: Duration = Duration::from_secs(480);

const WORDS: [&str; 8] = ["amber", "birch", "cobalt", "dune", "ember", "fjord", "garnet", "harbor"];

fn request(children: usize) -> String {
    let files: Vec<String> = (0..children).map(|i| format!("- notes/{i}.txt containing exactly the line {}", WORDS[i])).collect();
    format!(
        "Start {children} conversations at once with Faber's conversation tool, one per file below, each on model haiku with \
         effort low, with the brief \"Create the file <path> in the project folder containing exactly the line <word>, and \
         nothing else. Then conclude.\" Do not create the files yourself. After you started them all, wait for them and end \
         your turn. As reports arrive, check each file. Once every file is checked, answer ALL-DONE in one line and \
         conclude.\n\n{}",
        files.join("\n")
    )
}

#[derive(Debug, Default)]
struct Run {
    lead_turns: usize,
    /// Faber's own turns into the lead that carried reports, and the reports they carried.
    wakes: usize,
    reports: usize,
    cache_read: u64,
    fresh: u64,
    lead_usd: f64,
    all_usd: f64,
    /// From the last conversation's end to the lead's next answer starting.
    react_secs: Option<f64>,
    files_ok: usize,
    /// Conversations whose report never reached the lead.
    lost: usize,
    finished: bool,
    seconds: u64,
}

fn base() -> Result<std::path::PathBuf, String> {
    let base = std::env::var_os("FABER_EVAL_TMP").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    match base.ancestors().find(|a| a.join(".git").exists()) {
        Some(repo) => Err(format!("{} is inside the repository {}: set FABER_EVAL_TMP outside any", base.display(), repo.display())),
        None => Ok(base),
    }
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.args(args).current_dir(root).output().map_err(|e| e.to_string())?;
    out.status.success().then_some(()).ok_or_else(|| String::from_utf8_lossy(&out.stderr).into_owned())
}

async fn run(cli: faber_engine::cli::Cli, children: usize, batch: bool) -> Result<Run, String> {
    let world = base()?.join(format!("faber-fanout-eval-{}", uuid::Uuid::new_v4().simple()));
    let result = measure(&world, cli, children, batch).await;
    for attempt in 0..60 {
        match std::fs::remove_dir_all(&world) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) if attempt == 59 => eprintln!("could not remove {}: {e}", world.display()),
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
    result
}

async fn measure(world: &Path, cli: faber_engine::cli::Cli, children: usize, batch: bool) -> Result<Run, String> {
    let repo = world.join("repo");
    std::fs::create_dir_all(repo.join("notes")).map_err(|e| e.to_string())?;
    std::fs::write(repo.join("README.md"), "A folder of notes.\n").map_err(|e| e.to_string())?;
    git(&repo, &["init", "-q", "-b", "main"])?;
    git(&repo, &["add", "--", "."])?;
    git(&repo, &["-c", "user.name=Faber Eval", "-c", "user.email=eval@example.invalid", "commit", "-q", "-m", "first"])?;
    let engine = Engine::new(
        EngineOptions {
            data_dir: world.join("state"),
            #[cfg(windows)]
            chat_dir: world.join("chat-work"),
            clis: BTreeMap::from([(Vendor::Claude, cli)]),
            spawn: proc::real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .map_err(|e| e.to_string())?;
    engine.set_batch_reports(batch);
    let result = drive(&engine, &repo, children).await;
    engine.shutdown();
    result
}

async fn drive(engine: &Engine, repo: &Path, children: usize) -> Result<Run, String> {
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    let project = engine.add_project(&repo.to_string_lossy())?;
    engine.check_plans(Duration::from_secs(30)).await;
    let started = Instant::now();
    let lead = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id),
        text: request(children),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("claude-haiku-5-5".into()), effort: Some(Effort::Medium) },
    })?;
    let streaming = |m: &Message| m.status == Some(Status::Streaming);
    let kids = || -> Vec<Conversation> {
        engine.state().conversations.iter().filter(|c| c.id != lead).filter_map(|c| engine.open(&c.id)).collect()
    };
    let mut finished = false;
    while started.elapsed() < RUN_LIMIT {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(c) = engine.open(&lead) else { continue };
        let done = c.messages.last().is_some_and(|m| m.role == Role::Assistant && !streaming(m) && m.text.contains("ALL-DONE"));
        let quiet = kids().iter().all(|k| k.messages.last().is_none_or(|m| !streaming(m)) && k.queued.is_empty());
        if done && quiet && c.queued.is_empty() {
            finished = true;
            break;
        }
    }
    let seconds = started.elapsed().as_secs();
    // Whatever still runs stops, and its usage settles before it is counted.
    let all = engine.state().conversations;
    all.iter().for_each(|c| engine.interrupt(&c.id));
    let settled = Instant::now();
    while settled.elapsed() < Duration::from_secs(30)
        && all.iter().filter_map(|c| engine.open(&c.id)).any(|c| c.messages.last().is_some_and(streaming))
    {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let lead_conv = engine.open(&lead).ok_or("the lead is gone")?;
    let kids = kids();
    let replies = |c: &Conversation| c.messages.iter().filter(|m| m.role == Role::Assistant).cloned().collect::<Vec<_>>();
    let lead_replies = replies(&lead_conv);
    let mut r = Run { lead_turns: lead_replies.len(), finished, seconds, ..Run::default() };
    for m in &lead_replies {
        r.cache_read += m.cache_read_tokens.unwrap_or(0);
        r.fresh += m.input_tokens.unwrap_or(0) + m.output_tokens.unwrap_or(0) + m.cache_write_tokens.unwrap_or(0);
        r.lead_usd += m.cost_usd.unwrap_or(0.0);
    }
    r.all_usd = r.lead_usd + kids.iter().flat_map(replies).filter_map(|m| m.cost_usd).sum::<f64>();
    let report_wakes: Vec<&Message> =
        lead_conv.messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.childDone")).collect();
    r.wakes = report_wakes.len();
    r.reports = report_wakes.iter().map(|m| 1 + m.wake.as_ref().map_or(0, |w| w.ids.len())).sum();
    r.lost = kids
        .iter()
        .filter(|k| {
            let Some(reply) = k.messages.iter().rev().find(|m| m.role == Role::Assistant) else { return false };
            let id = format!("report-{}-{}", k.id, reply.id);
            !lead_conv.messages.iter().any(|m| Wake::carries(&m.id, m.wake.as_ref(), &id))
        })
        .count();
    let last_end =
        kids.iter().filter_map(|k| k.messages.iter().rev().find(|m| m.role == Role::Assistant)).map(|m| m.at + m.took.unwrap_or(0)).max();
    r.react_secs =
        last_end.and_then(|end| lead_replies.iter().find(|m| m.at >= end)).zip(last_end).map(|(m, end)| (m.at - end) as f64 / 1000.0);
    for (i, word) in WORDS.iter().enumerate().take(children) {
        let path = format!("notes/{i}.txt");
        let right = |folder: &Path| std::fs::read_to_string(folder.join(&path)).is_ok_and(|t| t.trim() == *word);
        r.files_ok += usize::from(right(repo) || kids.iter().filter_map(|k| k.copy.as_ref()).any(|c| right(&c.folder())));
    }
    Ok(r)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(6);
    let children: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(4).clamp(2, WORDS.len());
    let cli = find_cli("claude").ok_or("Claude Code is not installed")?;
    println!(
        "| run | batching | lead turns | report wakes | reports carried | cache reads | first-time tokens | lead US$ | all US$ | s to react | files right | lost | finished | s |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    let mut arms: BTreeMap<bool, Vec<Run>> = BTreeMap::new();
    for n in 0..runs * 2 {
        // Off, on, on, off, off, on...: neither arm always goes first.
        let batch = (n % 4 == 1) || (n % 4 == 2);
        let r = match run(cli.clone(), children, batch).await {
            Ok(r) => r,
            Err(e) => {
                println!("| {n} | {batch} | could not run: {e} |||||||||||");
                continue;
            }
        };
        println!(
            "| {n} | {batch} | {} | {} | {} | {} | {} | {:.3} | {:.3} | {} | {}/{children} | {} | {} | {} |",
            r.lead_turns,
            r.wakes,
            r.reports,
            r.cache_read,
            r.fresh,
            r.lead_usd,
            r.all_usd,
            r.react_secs.map_or("-".into(), |s| format!("{s:.0}")),
            r.files_ok,
            r.lost,
            r.finished,
            r.seconds
        );
        arms.entry(batch).or_default().push(r);
    }
    println!(
        "\n| batching | runs | lead turns (mean ± sd) | cache reads (mean) | lead US$ (mean) | all US$ (sum) | s to react (mean) | all files right | lost reports | finished |\n|---|---|---|---|---|---|---|---|---|---|"
    );
    for (batch, rs) in &arms {
        let n = rs.len() as f64;
        let mean = |f: &dyn Fn(&Run) -> f64| rs.iter().map(f).sum::<f64>() / n;
        let turns = mean(&|r| r.lead_turns as f64);
        let sd = (rs.iter().map(|r| (r.lead_turns as f64 - turns).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
        let reacts: Vec<f64> = rs.iter().filter_map(|r| r.react_secs).collect();
        println!(
            "| {batch} | {} | {turns:.2} ± {sd:.2} | {:.0} | {:.3} | {:.3} | {} | {}/{} | {} | {}/{} |",
            rs.len(),
            mean(&|r| r.cache_read as f64),
            mean(&|r| r.lead_usd),
            rs.iter().map(|r| r.all_usd).sum::<f64>(),
            if reacts.is_empty() { "-".into() } else { format!("{:.1}", reacts.iter().sum::<f64>() / reacts.len() as f64) },
            rs.iter().filter(|r| r.files_ok == children).count(),
            rs.len(),
            rs.iter().map(|r| r.lost).sum::<usize>(),
            rs.iter().filter(|r| r.finished).count(),
            rs.len(),
        );
    }
    Ok(())
}
