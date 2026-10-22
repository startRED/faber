//! Faber's light benchmark (never part of `cargo t`: it talks to the real CLIs with the person's own sign-in and spends real
//! quota). It runs the requests of `bench/requests/` twice, through Faber (the engine, with Auto, kits, the guide and the
//! rules) and through the plain CLI, in new folders under the run folder, checks each result with its proof script, has a
//! fresh Claude Code session judge the two replies blind (shown as "A" and "B", revealed after the vote) and writes the
//! results and the report tables. Everything runs without a window: each child starts through the engine's own process
//! start (no shell, no console window, its own job, below-normal priority). How to run it: `docs/dev/benchmark.md`.
//!
//!     cargo run -p faber-engine --example bench -- [--dry-run] [--plain claude|codex] [--vendors claude[,codex]]
//!         [--only id,id] [--run-id id] [--dir folder] [--cap-usd 8] [--cap-tokens 3000000] [--per-run-usd 1.5]
//!         [--plain-model m] [--plain-effort e] [--judge-model m] [--judge-effort e]
//!
//!     cargo run -p faber-engine --example bench -- --card-pairs [--dry-run] --vendor claude|codex [--model m] [--effort e]
//!         [--conditions cold,warm,resumed] [--pairs 3] [--pair-start 0] [--only id,id] [--cap-usd ..] [--judge-model m]
//!     cargo run -p faber-engine --example bench -- --gate results.json[,results.json]
//!
//! `--card-pairs` runs only Faber, pinned to one CLI and model, every request as a Project conversation, once with the start
//! card and once without it for each pair, and reports the token gate of the work method (docs/dev/benchmark.md).
//!
//! The pure parts (request files, the A/B deal, the verdict, what a CLI reported, the cap, the tables) are
//! `bench/pure.rs`, tested by `engine/tests/all.rs`.

#[path = "../../bench/pure.rs"]
mod pure;

use faber_engine::cli::{Cli, find_cli};
use faber_engine::model::{Area, Conversation, Message, Permissions, Role, Settings, Status, Vendor};
use faber_engine::proc::{self, Proc};
use faber_engine::router::{Choice, Effort};
use faber_engine::{Engine, EngineOptions, SendRequest};
use pure::{Arm, Cap, CardRun, Condition, Kind, Outcome, Request, Row, Side, SideResult, Spend, Usage};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Failure = Box<dyn std::error::Error>;

/// One request on one side may take this long, whatever the cap says.
const RUN_LIMIT: Duration = Duration::from_secs(20 * 60);
const PROOF_LIMIT: Duration = Duration::from_secs(5 * 60);
/// How long a finished reply with no Checks row is given to get one (the same grace as `selfdev_accept`).
const ROW_GRACE: Duration = Duration::from_secs(8);

struct Options {
    run_id: String,
    dir: PathBuf,
    only: Vec<String>,
    plain: Vendor,
    vendors: Vec<Vendor>,
    plain_model: String,
    plain_effort: String,
    judge_model: String,
    judge_effort: String,
    cap: Cap,
    per_run_usd: f64,
    dry_run: bool,
    /// `--card-pairs`: Faber with the start card against Faber without it (the token gate of the work method).
    card_pairs: bool,
    /// The one CLI of a card-pairs run, and the model and effort it is pinned to.
    vendor: Vendor,
    model: String,
    effort: Effort,
    pairs: usize,
    pair_start: usize,
    conditions: Vec<Condition>,
    /// `--gate`: results.json files of card-pairs runs to read the gate from, running nothing.
    gate: Vec<PathBuf>,
}

impl Options {
    fn read() -> Result<Options, Failure> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        let vendor = |name: &str| Vendor::ALL.into_iter().find(|v| v.name() == name).ok_or_else(|| format!("{name} is claude or codex"));
        let plain = vendor(&value("--plain").unwrap_or_else(|| "claude".into()))?;
        let vendors = value("--vendors").unwrap_or_else(|| "claude".into()).split(',').map(vendor).collect::<Result<Vec<_>, _>>()?;
        let number = |name: &str, default: f64| -> Result<f64, Failure> {
            value(name).map_or(Ok(default), |v| v.parse().map_err(|_| format!("{name} takes a number").into()))
        };
        let defaults = |vendor| match vendor {
            Vendor::Claude => ("claude-sonnet-5-5", "medium"),
            Vendor::Codex => ("gpt-6.1-sol", "medium"),
        };
        let (default_model, default_effort) = defaults(plain);
        let card_vendor = vendor(&value("--vendor").unwrap_or_else(|| "claude".into()))?;
        let effort = match value("--effort").unwrap_or_else(|| defaults(card_vendor).1.into()).as_str() {
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            other => return Err(format!("--effort is low, medium or high, not {other}").into()),
        };
        let count = |name: &str, default: usize| -> Result<usize, Failure> {
            value(name).map_or(Ok(default), |v| v.parse().map_err(|_| format!("{name} takes a whole number").into()))
        };
        Ok(Options {
            run_id: value("--run-id").unwrap_or_else(|| chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()),
            dir: value("--dir").map_or_else(|| std::env::temp_dir().join("faber-bench"), PathBuf::from),
            only: value("--only").map(|v| v.split(',').map(String::from).collect()).unwrap_or_default(),
            plain,
            vendors,
            plain_model: value("--plain-model").unwrap_or_else(|| default_model.into()),
            plain_effort: value("--plain-effort").unwrap_or_else(|| default_effort.into()),
            judge_model: value("--judge-model").unwrap_or_else(|| "claude-opus-5-5".into()),
            judge_effort: value("--judge-effort").unwrap_or_else(|| "high".into()),
            cap: Cap { max_usd: number("--cap-usd", 8.0)?, max_fresh_tokens: number("--cap-tokens", 3_000_000.0)? as u64 },
            per_run_usd: number("--per-run-usd", 1.5)?,
            dry_run: args.iter().any(|a| a == "--dry-run"),
            card_pairs: args.iter().any(|a| a == "--card-pairs"),
            vendor: card_vendor,
            model: value("--model").unwrap_or_else(|| defaults(card_vendor).0.into()),
            effort,
            pairs: count("--pairs", pure::BASE_PAIRS)?,
            pair_start: count("--pair-start", 0)?,
            conditions: pure::parse_conditions(&value("--conditions").unwrap_or_else(|| "cold,warm,resumed".into()))?,
            gate: value("--gate").map(|v| v.split(',').map(PathBuf::from).collect()).unwrap_or_default(),
        })
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the engine sits in the repository").to_path_buf()
}

fn load_requests(only: &[String]) -> Result<Vec<Request>, Failure> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo_root().join("bench/requests"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    let mut requests = Vec::new();
    for file in files {
        let request = pure::parse_request(&std::fs::read_to_string(&file)?).map_err(|e| format!("{}: {e}", file.display()))?;
        if only.is_empty() || only.contains(&request.id) {
            requests.push(request);
        }
    }
    if requests.is_empty() {
        return Err("no request matches".into());
    }
    Ok(requests)
}

// --- Starting things, without a shell and without a window ---------------------------------------------------------

struct Finished {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    timed_out: bool,
}

/// Runs one program to its end through the engine's own start (no shell, no console window, a job of its own that ends the
/// whole tree when it is dropped, below-normal priority), feeding it `input` on stdin. Past `limit` the tree is ended.
async fn run_program(program: &str, args: &[String], cwd: &Path, input: Option<&str>, limit: Duration) -> Result<Finished, String> {
    let mut command = tokio::process::Command::new(program);
    command.args(args).current_dir(cwd);
    let Proc { mut stdin, mut stdout, stderr, child, contained } =
        proc::spawn_check(command, None).map_err(|e| format!("could not start {program}: {e}"))?;
    if let Some(text) = input {
        stdin.write_all(text.as_bytes()).await.map_err(|e| format!("{program} did not take its input: {e}"))?;
    }
    drop(stdin);
    let errors = tokio::spawn(async move {
        let mut text = Vec::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_end(&mut text).await;
        }
        text
    });
    let mut child = child.ok_or("no process")?;
    let read = async {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out).await;
        let status = child.wait().await;
        (out, status)
    };
    let timed_out;
    let (out, status) = match tokio::time::timeout(limit, read).await {
        Ok(done) => {
            timed_out = false;
            done
        }
        Err(_) => {
            timed_out = true;
            drop(contained);
            let _ = child.kill().await;
            (Vec::new(), child.wait().await)
        }
    };
    let err = tokio::time::timeout(Duration::from_secs(5), errors).await.ok().and_then(Result::ok).unwrap_or_default();
    Ok(Finished {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
        code: status.ok().and_then(|s| s.code()),
        timed_out,
    })
}

/// A CLI the way the engine starts it: its program, then the prefix npm's shim needs, then the arguments.
async fn run_cli(cli: &Cli, args: &[String], cwd: &Path, input: &str, limit: Duration) -> Result<Finished, String> {
    let all: Vec<String> = cli.prefix.iter().chain(args).cloned().collect();
    run_program(&cli.command, &all, cwd, Some(input), limit).await
}

/// A quick, local command (git, a fixture script) with no window.
fn hidden(program: &str) -> std::process::Command {
    let command = std::process::Command::new(program);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(0x0800_0000);
        command
    };
    command
}

fn git(dir: &Path, args: &[&str]) -> Result<(), Failure> {
    let out = hidden("git").args(args).current_dir(dir).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").output()?;
    if out.status.success() { Ok(()) } else { Err(format!("git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr)).into()) }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

// --- The two sides -------------------------------------------------------------------------------------------------

struct Work {
    /// The starting folder the proofs compare with (empty for a chat).
    pristine: PathBuf,
    /// The plain CLI's folder: the starting folder as a git repository, or an empty folder.
    plain: PathBuf,
    /// The project folder Faber is given: the same starting folder as a repository.
    repo: PathBuf,
}

fn prepare(request: &Request, root: &Path, node: &str, with_plain: bool) -> Result<Work, Failure> {
    let base = root.join(&request.id);
    let work = Work { pristine: base.join("pristine"), plain: base.join("plain"), repo: base.join("faber").join("repo") };
    let copies: Vec<&PathBuf> = if with_plain { vec![&work.plain, &work.repo] } else { vec![&work.repo] };
    for dir in copies.iter().copied().chain([&work.pristine]) {
        std::fs::create_dir_all(dir)?;
    }
    if request.kind == Kind::Chat {
        return Ok(work);
    }
    match &request.fixture {
        Some(name) => {
            let script = repo_root().join("bench/fixtures").join(format!("{name}.mjs"));
            let out = hidden(node).arg(&script).arg(&work.pristine).output()?;
            if !out.status.success() {
                return Err(format!("fixture {name} failed: {}", String::from_utf8_lossy(&out.stderr)).into());
            }
        }
        None => std::fs::write(work.pristine.join("README.md"), "# Project\n")?,
    }
    for dir in copies {
        copy_tree(&work.pristine, dir)?;
        git(dir, &["init", "-q", "-b", "main"])?;
        git(dir, &["add", "--", "."])?;
        git(dir, &["-c", "user.name=Bench", "-c", "user.email=bench@example.invalid", "commit", "-q", "-m", "start"])?;
    }
    Ok(work)
}

fn failed(side: Side, secs: f64, error: impl Into<String>) -> SideResult {
    SideResult {
        side,
        ok: false,
        error: Some(error.into()),
        secs,
        wall_secs: secs,
        usage: Usage::default(),
        cost_usd: None,
        models: vec![],
        reply: String::new(),
        folder: None,
        proof: None,
        stopped_by_cap: false,
    }
}

async fn run_plain(options: &Options, cli: &Cli, request: &Request, work: &Work, allowance: f64) -> SideResult {
    let started = Instant::now();
    let cmd: Vec<String> = match options.plain {
        Vendor::Claude => args(&[
            "-p",
            "--output-format",
            "json",
            "--no-session-persistence",
            "--permission-mode",
            "bypassPermissions",
            "--model",
            &options.plain_model,
            "--effort",
            &options.plain_effort,
            "--max-budget-usd",
            &format!("{allowance:.2}"),
        ]),
        Vendor::Codex => args(&[
            "exec",
            "--json",
            "--ephemeral",
            "--skip-git-repo-check",
            "-s",
            "danger-full-access",
            "-m",
            &options.plain_model,
            "-c",
            &format!("model_reasoning_effort=\"{}\"", options.plain_effort),
            "-C",
            &work.plain.to_string_lossy(),
            "-",
        ]),
    };
    let run = match run_cli(cli, &cmd, &work.plain, &request.prompt, RUN_LIMIT).await {
        Ok(run) => run,
        Err(error) => return failed(Side::Plain, started.elapsed().as_secs_f64(), error),
    };
    let secs = started.elapsed().as_secs_f64();
    let parsed = match options.plain {
        Vendor::Claude => pure::parse_claude_result(&run.stdout),
        Vendor::Codex => pure::parse_codex_events(&run.stdout),
    };
    let tail: String = run.stderr.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
    let outcome = match parsed {
        Ok(outcome) => outcome,
        Err(error) => return failed(Side::Plain, secs, format!("{error}; exit {:?}; stderr: {tail}", run.code)),
    };
    let models = if outcome.models.is_empty() { vec![options.plain_model.clone()] } else { outcome.models.clone() };
    SideResult {
        side: Side::Plain,
        ok: outcome.ok && !run.timed_out,
        error: outcome.error.clone().or(run.timed_out.then(|| "ran past the time limit".into())),
        secs,
        wall_secs: secs,
        usage: outcome.usage,
        cost_usd: outcome.cost_usd,
        models,
        reply: outcome.text,
        folder: (request.kind == Kind::Project).then(|| work.plain.to_string_lossy().into_owned()),
        proof: None,
        stopped_by_cap: outcome.error.as_deref().is_some_and(|e| e.contains("max_budget")),
    }
}

fn message_usage(messages: &[&Message]) -> (Usage, Option<f64>) {
    let mut usage = Usage::default();
    let mut cost: Option<f64> = None;
    for m in messages {
        let (read, write) = (m.cache_read_tokens.unwrap_or(0), m.cache_write_tokens.unwrap_or(0));
        // A message's input is the whole prompt, cache included (`claude::result_events`).
        usage.add(&Usage {
            input: m.input_tokens.unwrap_or(0).saturating_sub(read + write),
            output: m.output_tokens.unwrap_or(0),
            cache_read: read,
            cache_write: write,
        });
        if let Some(c) = m.cost_usd {
            *cost.get_or_insert(0.0) += c;
        }
    }
    (usage, cost)
}

fn spent_by(messages: &[Message]) -> f64 {
    messages.iter().filter_map(|m| m.cost_usd).sum()
}

/// Waits for the reply to settle (the messages before number `after` belong to earlier turns): it ended and Faber's checks on it ended (or a project has none to run), or it needs the
/// person. The cost is read only when a turn ends (the engine writes it then), so one long reply is not interrupted and a
/// project's fix loop runs until RUN_LIMIT; Codex reports no cost, so this never stops a Codex side.
async fn settle(engine: &Engine, id: &str, project: bool, allowance: f64, after: usize) -> (Option<Conversation>, bool) {
    let started = Instant::now();
    let (mut quiet, mut capped): (Option<(String, Instant)>, bool) = (None, false);
    while started.elapsed() < RUN_LIMIT {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Some(conversation) = engine.open(id).filter(|c| c.messages.len() > after) else { continue };
        if !capped && spent_by(&conversation.messages[after..]) >= allowance {
            capped = true;
            engine.interrupt(id);
        }
        let Some(reply) = conversation.messages.last().filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming)) else {
            continue;
        };
        if capped || reply.needs_you || matches!(reply.status, Some(Status::Error | Status::Stopped)) || !project {
            return (Some(conversation), capped);
        }
        match reply.checks.as_ref().map(|k| k.state) {
            Some(faber_engine::model::ChecksState::Passed | faber_engine::model::ChecksState::NeedsYou) => {
                return (Some(conversation), capped);
            }
            Some(_) => quiet = None,
            None => {
                let since = quiet.get_or_insert_with(|| (reply.id.clone(), Instant::now()));
                if since.0 != reply.id {
                    *since = (reply.id.clone(), Instant::now());
                }
                if since.1.elapsed() >= ROW_GRACE {
                    return (Some(conversation), capped);
                }
            }
        }
    }
    (engine.open(id).filter(|c| c.messages.len() > after), capped)
}

/// The engine's own start, writing what each CLI it starts was given (its arguments and folder, one JSON line each) next to the run.
fn logging_spawner(log: PathBuf) -> proc::Spawner {
    let real = proc::real_spawner();
    Arc::new(move |bin, args, cwd| {
        // A config blob (settings, MCP servers, a system prompt) may hold a token: log its length only.
        let mut hide = false;
        let shown: Vec<String> = args
            .iter()
            .map(|a| {
                let out = if hide { format!("<{} bytes>", a.len()) } else { a.clone() };
                hide = matches!(a.as_str(), "--settings" | "--mcp-config" | "--system-prompt" | "--append-system-prompt");
                out
            })
            .collect();
        let line = format!("{}\n", json!({ "bin": bin, "args": shown, "cwd": cwd }));
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
        real(bin, args, cwd)
    })
}

/// An engine as the app builds it, on `state` as its data folder, writing each CLI start to `spawns`.
fn new_engine(clis: &BTreeMap<Vendor, Cli>, state: PathBuf, spawns: PathBuf, billed: bool) -> Engine {
    let engine = Engine::new(
        EngineOptions {
            data_dir: state,
            clis: clis.clone(),
            spawn: logging_spawner(spawns),
            windows: cfg!(windows),
            claude_env_billed: billed,
            max_warm: 1,
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    // What the app's shell gives the engine: the native git for separate copies, and the verifier for the checks.
    engine.set_git(Arc::new(|| {
        faber_engine::project::git::Process::find().ok().map(|git| Box::new(git) as Box<dyn faber_engine::project::git::Executor + Send>)
    }));
    engine.set_verifier(faber_engine::verify::Verifier::real(None, None));
    engine
}

/// What the app does when it starts: Full access (the plain side's bypassPermissions), the plan check and, for Codex, its
/// limits and model list (src-tauri/src/main.rs; Auto and a pinned model need both).
async fn start_engine(engine: &Engine, clis: &BTreeMap<Vendor, Cli>, repo: &Path) -> Result<(), String> {
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    engine.check_plans(Duration::from_secs(30)).await;
    if let Some(codex) = clis.get(&Vendor::Codex) {
        let real = proc::real_spawner();
        let prefix = codex.prefix.clone();
        let spawn: proc::Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
        let limits =
            faber_engine::codex::read_codex_limits(&codex.command, repo, cfg!(windows), spawn.clone(), Duration::from_secs(20)).await;
        engine.note_account_events(Vendor::Codex, limits);
        let list = faber_engine::codex::list_codex_models(&codex.command, repo, cfg!(windows), spawn, Duration::from_secs(20)).await;
        let recommended = list.iter().find(|m| m.is_default).map(|m| m.id.clone());
        engine.set_codex_models(list.iter().map(|m| m.id.clone()).collect(), recommended);
    }
    Ok(())
}

/// A project reply as a side's result: the agent's replies from message number `from` on.
fn faber_side(conversation: &Conversation, from: usize, project: bool, work: &Work, wall: f64, capped: bool) -> SideResult {
    let replies: Vec<&Message> = conversation.messages.iter().skip(from).filter(|m| m.role == Role::Assistant).collect();
    let (usage, cost_usd) = message_usage(&replies);
    let last = replies.last();
    let reply = last.map_or("", |m| {
        let tail = pure::from_utf16(&m.text, m.activity_end.unwrap_or(0));
        if tail.trim().is_empty() { m.text.as_str() } else { tail }
    });
    let mut models: Vec<String> = Vec::new();
    for m in &replies {
        let name = format!(
            "{}@{} ({})",
            m.model.as_deref().unwrap_or("?"),
            m.effort.as_deref().unwrap_or("?"),
            m.reason.as_deref().unwrap_or("no route")
        );
        if !models.contains(&name) {
            models.push(name);
        }
    }
    let error = last.and_then(|m| m.error.clone()).or_else(|| match last.map(|m| m.status) {
        Some(Some(Status::Done)) => None,
        other => Some(format!("the last reply ended as {other:?}")),
    });
    let waiting = last.is_some_and(|m| m.needs_you);
    SideResult {
        side: Side::Faber,
        ok: error.is_none() && !reply.trim().is_empty() && !waiting,
        error: error.or(waiting.then(|| "the answer waits for the person".into())),
        secs: Some(replies.iter().filter_map(|m| m.took).sum::<u64>()).filter(|t| *t > 0).map_or(wall, |t| t as f64 / 1000.0),
        wall_secs: wall,
        usage,
        cost_usd,
        models,
        reply: reply.to_string(),
        folder: project
            .then(|| conversation.copy.as_ref().map_or_else(|| work.repo.clone(), |c| c.folder()).to_string_lossy().into_owned()),
        proof: None,
        stopped_by_cap: capped,
    }
}

async fn run_faber(clis: &BTreeMap<Vendor, Cli>, request: &Request, work: &Work, root: &Path, allowance: f64, billed: bool) -> SideResult {
    let started = Instant::now();
    let state = root.join(&request.id).join("faber").join("state");
    let engine = new_engine(clis, state, root.join(&request.id).join("faber").join("spawns.log"), billed);
    let result = async {
        start_engine(&engine, clis, &work.repo).await?;
        let (area, project_id) = match request.kind {
            Kind::Chat => (Area::Chat, None),
            Kind::Project => (Area::Project, Some(engine.add_project(&work.repo.to_string_lossy())?.id)),
        };
        let id = engine.send(SendRequest {
            conversation_id: None,
            area,
            project_id,
            text: request.prompt.clone(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "auto".into(), model: None, effort: None },
        })?;
        Ok::<_, String>((id.clone(), settle(&engine, &id, request.kind == Kind::Project, allowance, 0).await))
    }
    .await;
    let wall = started.elapsed().as_secs_f64();
    let side = match result {
        Err(error) => failed(Side::Faber, wall, error),
        Ok((_, (None, _))) => failed(Side::Faber, wall, "the engine lost the conversation"),
        Ok((_, (Some(conversation), capped))) => faber_side(&conversation, 0, request.kind == Kind::Project, work, wall, capped),
    };
    engine.shutdown();
    // The engine's processes end with it; give Windows a moment to release the folders before the next step reads them.
    tokio::time::sleep(Duration::from_millis(500)).await;
    side
}

// --- Proofs and the judge ------------------------------------------------------------------------------------------

async fn prove(node: &Cli, request: &Request, work: &Work, side: &mut SideResult, run_dir: &Path) {
    let Some(name) = &request.proof else { return };
    let answer = run_dir.join(&request.id).join(format!("answer-{:?}.txt", side.side).to_lowercase());
    if std::fs::write(&answer, &side.reply).is_err() {
        return;
    }
    let folder = side.folder.clone().map(PathBuf::from).unwrap_or_else(|| work.plain.clone());
    let script = repo_root().join("bench/proofs").join(format!("{name}.mjs"));
    let call = args(&[&script.to_string_lossy(), &folder.to_string_lossy(), &work.pristine.to_string_lossy(), &answer.to_string_lossy()]);
    side.proof = match run_cli(node, &call, run_dir, "", PROOF_LIMIT).await {
        Ok(run) => pure::parse_proof(&run.stdout).or_else(|| {
            Some(pure::Proof {
                pass: false,
                checks: vec![pure::Check {
                    name: "the proof ran".into(),
                    ok: false,
                    detail: format!("exit {:?}: {}", run.code, run.stderr.chars().take(300).collect::<String>()),
                }],
            })
        }),
        Err(error) => {
            Some(pure::Proof { pass: false, checks: vec![pure::Check { name: "the proof started".into(), ok: false, detail: error }] })
        }
    };
}

/// The files of a folder as the judge reads them: every path, the text of the ones that are text. Dot folders and
/// `node_modules` are not the work.
fn collect(dir: &Path) -> Vec<(String, Option<String>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Option<String>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with('.') || name == "node_modules" || out.len() >= 200 {
                continue;
            }
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let text = std::fs::read(&path).ok().and_then(|b| String::from_utf8(b).ok()).filter(|t| !t.contains('\0'));
                out.push((path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

fn entry_of(side: &SideResult) -> String {
    let files = side.folder.as_deref().map(|f| collect(Path::new(f))).unwrap_or_default();
    pure::render_entry(&side.reply, &files, side.proof.as_ref())
}

struct Judged {
    verdict: Option<pure::Verdict>,
    usage: Usage,
    cost_usd: Option<f64>,
    note: Option<String>,
}

/// A fresh Claude Code session with no tools, hooks or MCP servers, given `instructions` and `prompt`; its result.
async fn ask_judge(
    options: &Options,
    claude: &Cli,
    instructions: &str,
    prompt: &str,
    dir: &Path,
    allowance: f64,
) -> Result<Outcome, String> {
    let cmd = args(&[
        "-p",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--permission-mode",
        "dontAsk",
        "--model",
        &options.judge_model,
        "--effort",
        &options.judge_effort,
        "--max-budget-usd",
        &format!("{allowance:.2}"),
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--safe-mode",
        "--setting-sources",
        "",
        "--system-prompt",
        instructions,
    ]);
    let run = run_cli(claude, &cmd, dir, prompt, Duration::from_secs(10 * 60)).await?;
    pure::parse_claude_result(&run.stdout).map_err(|error| format!("{error}; stderr: {}", run.stderr.chars().take(300).collect::<String>()))
}

async fn judge(options: &Options, claude: &Cli, request: &Request, a: &SideResult, b: &SideResult, dir: &Path, allowance: f64) -> Judged {
    let prompt = pure::judge_prompt(&request.prompt, &entry_of(a), &entry_of(b));
    let none = |note: String| Judged { verdict: None, usage: Usage::default(), cost_usd: None, note: Some(note) };
    match ask_judge(options, claude, pure::JUDGE_INSTRUCTIONS, &prompt, dir, allowance).await {
        Err(error) => none(error),
        Ok(Outcome { text, usage, cost_usd, .. }) => {
            let verdict = pure::parse_verdict(&text);
            let note = verdict.is_none().then(|| format!("the judge gave no verdict: {}", text.chars().take(300).collect::<String>()));
            Judged { verdict, usage, cost_usd, note }
        }
    }
}

// --- Card pairs: the token gate of the start card ----------------------------------------------------------------------

/// The message a resumed session gets: it asks for nothing new, so what it costs is the session's own weight.
const FOLLOW_UP: &str = "Continue: check your work once more and say what you checked.";

struct CardEnv<'a> {
    options: &'a Options,
    clis: BTreeMap<Vendor, Cli>,
    node: Cli,
    claude: Cli,
    root: PathBuf,
}

/// One agent turn (with the fix loops Faber's checks add) as the runner saw it.
struct Turn {
    id: String,
    side: SideResult,
    replies: u64,
    tool_calls: u64,
}

/// Where a run sits in the plan: its pair, its arm and its condition.
type Slot = (usize, Arm, Condition);

async fn open_engine(env: &CardEnv<'_>, state: &Path, spawns: &Path, card: bool, repo: &Path) -> Result<Engine, String> {
    let engine = new_engine(&env.clis, state.to_path_buf(), spawns.to_path_buf(), false);
    engine.set_start_cards(card);
    start_engine(&engine, &env.clis, repo).await?;
    Ok(engine)
}

/// Sends `text` (a new conversation, or `conversation` goes on) and waits for the turn to settle.
#[allow(clippy::too_many_arguments)]
async fn turn(
    engine: &Engine,
    project_id: &str,
    conversation: Option<String>,
    text: &str,
    choice: &Choice,
    from: usize,
    work: &Work,
    allowance: f64,
) -> Result<Turn, String> {
    let started = Instant::now();
    let id = engine.send(SendRequest {
        conversation_id: conversation,
        area: Area::Project,
        project_id: Some(project_id.to_string()),
        text: text.to_string(),
        images: vec![],
        files: vec![],
        choice: choice.clone(),
    })?;
    let (found, capped) = settle(engine, &id, true, allowance, from).await;
    let conversation = found.ok_or("the engine gave no reply")?;
    let side = faber_side(&conversation, from, true, work, started.elapsed().as_secs_f64(), capped);
    let replies = conversation.messages.iter().skip(from).filter(|m| m.role == Role::Assistant);
    let (count, tools) = replies.fold((0, 0), |(n, t), m| (n + 1, t + m.tools.as_ref().map_or(0, |t| t.len() as u64)));
    Ok(Turn { id, side, replies: count, tool_calls: tools })
}

/// Grades one turn (the proof of its request, or the blind acceptance judge when it has none) and writes its run. A turn
/// that is not one of the wanted conditions is only counted in the spend.
#[allow(clippy::too_many_arguments)]
async fn record(
    env: &CardEnv<'_>,
    spent: &mut Spend,
    request: &Request,
    work: &Work,
    base: &Path,
    (pair, arm, condition): Slot,
    mut taken: Turn,
    (proof_answer, shown): (&str, &str),
) -> Option<CardRun> {
    spent.add(&taken.side.usage, taken.side.cost_usd);
    if !env.options.conditions.contains(&condition) {
        return None;
    }
    let mut notes: Vec<String> = taken.side.error.iter().cloned().collect();
    taken.side.reply = proof_answer.to_string();
    prove(&env.node, request, work, &mut taken.side, base).await;
    let mut accepted = None;
    if request.proof.is_none() && taken.side.ok {
        if let Some(why) = env.options.cap.reached(spent) {
            notes.push(format!("not judged, the cap was reached: {why}"));
        } else {
            let files = taken.side.folder.as_deref().map(|f| collect(Path::new(f))).unwrap_or_default();
            let prompt = pure::accept_prompt(&request.prompt, &pure::render_entry(shown, &files, None));
            let allowance = env.options.cap.allowance(spent, env.options.per_run_usd);
            match ask_judge(env.options, &env.claude, pure::ACCEPT_INSTRUCTIONS, &prompt, &env.root.join("judge"), allowance).await {
                Err(error) => notes.push(format!("the judge failed: {error}")),
                Ok(Outcome { text, usage, cost_usd, .. }) => {
                    spent.add(&usage, cost_usd);
                    match pure::parse_accept(&text) {
                        Some(verdict) => {
                            accepted = Some(verdict.acceptable);
                            notes.push(format!("judge: {}", verdict.reason));
                        }
                        None => notes.push(format!("the judge gave no verdict: {}", text.chars().take(200).collect::<String>())),
                    }
                }
            }
        }
    }
    let proof = taken.side.proof.as_ref().map(|p| p.pass);
    for check in taken.side.proof.iter().flat_map(|p| &p.checks).filter(|c| !c.ok) {
        notes.push(format!("proof: {} ({})", check.name, check.detail));
    }
    Some(CardRun {
        vendor: env.options.vendor.name().into(),
        request: request.id.clone(),
        condition,
        pair,
        arm,
        success: taken.side.ok && proof.unwrap_or(true) && (request.proof.is_some() || accepted == Some(true)),
        usage: taken.side.usage,
        replies: taken.replies,
        tool_calls: taken.tool_calls,
        secs: taken.side.wall_secs,
        cost_usd: taken.side.cost_usd,
        proof,
        accepted,
        note: (!notes.is_empty()).then(|| notes.join("; ")),
    })
}

fn failed_card_run(env: &CardEnv<'_>, request: &Request, (pair, arm, condition): Slot, why: &str) -> CardRun {
    CardRun {
        vendor: env.options.vendor.name().into(),
        request: request.id.clone(),
        condition,
        pair,
        arm,
        success: false,
        usage: Usage::default(),
        replies: 0,
        tool_calls: 0,
        secs: 0.0,
        cost_usd: None,
        proof: None,
        accepted: None,
        note: Some(why.to_string()),
    }
}

/// One arm of one pair for one request, in a project of its own: the cold turn, then the warm one (a second new conversation
/// in the project) and the resumed one (a follow-up in the first conversation, after the engine was closed and started again
/// on the same data folder), as far as the wanted conditions go. A run the cap stopped is left out, not failed.
async fn card_phases(
    env: &CardEnv<'_>,
    spent: &mut Spend,
    request: &Request,
    (pair, arm): (usize, Arm),
    runs: &mut Vec<CardRun>,
) -> Result<(), String> {
    let options = env.options;
    let cap_reached = |spent: &Spend| options.cap.reached(spent).is_some();
    let allowance = |spent: &Spend| options.cap.allowance(spent, options.per_run_usd);
    let base = env.root.join(format!("pair{}-{}", pair + 1, if arm == Arm::Card { "card" } else { "no-card" }));
    // The card exists only in projects, so a chat request runs as one here.
    let project = Request { kind: Kind::Project, ..request.clone() };
    let work = prepare(&project, &base, &env.node.command, false).map_err(|e| e.to_string())?;
    let (state, spawns) = (base.join(&request.id).join("faber").join("state"), base.join(&request.id).join("faber").join("spawns.log"));
    let choice = Choice { vendor: options.vendor.name().into(), model: Some(options.model.clone()), effort: Some(options.effort) };
    let mut engine = open_engine(env, &state, &spawns, arm == Arm::Card, &work.repo).await?;
    let result = async {
        let project_id = engine.add_project(&work.repo.to_string_lossy())?.id;
        if cap_reached(spent) {
            return Ok(());
        }
        let cold = turn(&engine, &project_id, None, &request.prompt, &choice, 0, &work, allowance(spent)).await?;
        let cold_id = cold.id.clone();
        let cold_reply = cold.side.reply.clone();
        runs.extend(record(env, spent, request, &work, &base, (pair, arm, Condition::Cold), cold, (&cold_reply, &cold_reply)).await);
        if options.conditions.contains(&Condition::Warm) && !cap_reached(spent) {
            let slot = (pair, arm, Condition::Warm);
            match turn(&engine, &project_id, None, &request.prompt, &choice, 0, &work, allowance(spent)).await {
                Ok(warm) => {
                    let reply = warm.side.reply.clone();
                    runs.extend(record(env, spent, request, &work, &base, slot, warm, (&reply, &reply)).await);
                }
                Err(why) => runs.push(failed_card_run(env, request, slot, &why)),
            }
        }
        if options.conditions.contains(&Condition::Resumed) && !cap_reached(spent) {
            let slot = (pair, arm, Condition::Resumed);
            engine.shutdown();
            // The engine's processes end with it; give Windows a moment to release the folders.
            tokio::time::sleep(Duration::from_millis(500)).await;
            engine = open_engine(env, &state, &spawns, arm == Arm::Card, &work.repo).await?;
            let before = engine.open(&cold_id).ok_or("the conversation did not come back after the restart")?.messages.len();
            match turn(&engine, &project_id, Some(cold_id.clone()), FOLLOW_UP, &choice, before, &work, allowance(spent)).await {
                Ok(again) => {
                    let shown = format!("{cold_reply}\n\n### After the person asked for one more check\n{}", again.side.reply);
                    runs.extend(record(env, spent, request, &work, &base, slot, again, (&cold_reply, &shown)).await);
                }
                Err(why) => runs.push(failed_card_run(env, request, slot, &why)),
            }
        }
        Ok::<(), String>(())
    }
    .await;
    engine.shutdown();
    tokio::time::sleep(Duration::from_millis(500)).await;
    result
}

async fn card_session(env: &CardEnv<'_>, spent: &mut Spend, request: &Request, pair: usize, arm: Arm) -> Vec<CardRun> {
    let mut runs = Vec::new();
    if let Err(why) = card_phases(env, spent, request, (pair, arm), &mut runs).await {
        for &condition in &env.options.conditions {
            if !runs.iter().any(|r| r.condition == condition) {
                runs.push(failed_card_run(env, request, (pair, arm, condition), &why));
            }
        }
    }
    runs
}

fn card_plan(options: &Options, requests: &[Request]) {
    let pairs = options.pair_start..options.pair_start + options.pairs;
    // The cold turn always runs: the warm and resumed ones follow it.
    let turns_per_session = options.conditions.len() + usize::from(!options.conditions.contains(&Condition::Cold));
    let sessions = pairs.len() * requests.len() * 2;
    let unproven = requests.iter().filter(|r| r.proof.is_none()).count();
    println!(
        "card pairs {}: Faber on {} {} {} only (no Auto, no plain side), start card on against off, conditions {:?}, pairs {}..={}, judge {} {} (a fresh Claude Code session) for requests with no proof, cap US${} or {} first-time tokens",
        options.run_id,
        options.vendor.name(),
        options.model,
        options.effort.name(),
        options.conditions.iter().map(|c| c.name()).collect::<Vec<_>>(),
        options.pair_start + 1,
        options.pair_start + options.pairs,
        options.judge_model,
        options.judge_effort,
        options.cap.max_usd,
        options.cap.max_fresh_tokens
    );
    println!("every request runs as a Project conversation in a new folder (the card exists only in projects), the chat ones too");
    for pair in pairs.clone() {
        for r in requests {
            let first = pure::first_arm(pair);
            println!("  pair {} {}: {} then {} [proof {:?}]", pair + 1, r.id, first.name(), first.other().name(), r.proof);
        }
    }
    println!(
        "{sessions} engine sessions, {} agent turns at least (more where Faber's checks send the agent back), {} judge calls",
        sessions * turns_per_session,
        pairs.len() * 2 * options.conditions.len() * unproven
    );
}

fn write_card_results(run_dir: &Path, options: &Options, runs: &[CardRun], spent: &Spend) -> Result<(), Failure> {
    let gate = pure::card_gate(runs);
    let value = json!({
        "mode": "card-pairs",
        "runId": options.run_id,
        "cli": { "name": options.vendor.name(), "model": options.model, "effort": options.effort.name() },
        "conditions": options.conditions.iter().map(|c| c.name()).collect::<Vec<_>>(),
        "pairs": { "start": options.pair_start, "count": options.pairs },
        "judge": { "model": options.judge_model, "effort": options.judge_effort },
        "cap": { "usd": options.cap.max_usd, "freshTokens": options.cap.max_fresh_tokens, "perRunUsd": options.per_run_usd },
        "spent": spent,
        "gate": gate,
        "runs": runs,
    });
    std::fs::write(run_dir.join("results.json"), serde_json::to_string_pretty(&value)?)?;
    let note = "Every request ran as a Project conversation (the start card exists only in projects), the chat ones too. The gate counts \
                input + cache writes + output; cached input (cache reads) is reported apart.\n\n";
    std::fs::write(run_dir.join("tables.md"), format!("{note}{}", pure::render_card_tables(runs, &gate)))?;
    Ok(())
}

async fn card_main(options: &Options, requests: &[Request]) -> Result<(), Failure> {
    card_plan(options, requests);
    if options.dry_run {
        return Ok(());
    }
    if faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)) {
        return Err("an environment variable would bill Claude Code per use; the benchmark runs on the subscription only".into());
    }
    let node = find_cli("node").ok_or("node is not on the PATH (the fixtures and proofs are Node scripts)")?;
    let claude = find_cli("claude").ok_or("Claude Code is not installed (the judge needs it)")?;
    let cli = find_cli(options.vendor.name()).ok_or_else(|| format!("{} is not installed", options.vendor.name()))?;
    let root = options.dir.join(&options.run_id);
    std::fs::create_dir_all(root.join("judge"))?;
    let env = CardEnv { options, clis: BTreeMap::from([(options.vendor, cli)]), node, claude, root: root.clone() };
    let (mut runs, mut spent) = (Vec::new(), Spend::default());
    'pairs: for pair in options.pair_start..options.pair_start + options.pairs {
        for request in requests {
            for arm in [pure::first_arm(pair), pure::first_arm(pair).other()] {
                if let Some(why) = options.cap.reached(&spent) {
                    println!("stopping before pair {} {} ({}): {why}", pair + 1, request.id, arm.name());
                    break 'pairs;
                }
                println!("[pair {} / {}] {} with {}", pair + 1, options.pair_start + options.pairs, request.id, arm.name());
                for run in card_session(&env, &mut spent, request, pair, arm).await {
                    println!(
                        "  {}: success {}, {} tokens ({} cached), {} replies, {:.0}s{}",
                        run.condition.name(),
                        run.success,
                        pure::gate_tokens(&run),
                        run.usage.cache_read,
                        run.replies,
                        run.secs,
                        run.note.as_deref().map_or(String::new(), |n| format!("; {}", n.chars().take(160).collect::<String>()))
                    );
                    runs.push(run);
                }
                write_card_results(&root, options, &runs, &spent)?;
            }
        }
    }
    write_card_results(&root, options, &runs, &spent)?;
    println!("\n{}", pure::render_card_tables(&runs, &pure::card_gate(&runs)));
    println!("spent US${:.3}, {} first-time tokens; results: {}", spent.usd, spent.usage.fresh(), root.join("results.json").display());
    Ok(())
}

/// `--gate a.json,b.json`: the gate over the runs of earlier card-pair results (the first three pairs and the three more).
fn gate_files(files: &[PathBuf]) -> Result<(), Failure> {
    let mut runs: Vec<CardRun> = Vec::new();
    for file in files {
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
        let part = value.get("runs").cloned().ok_or_else(|| format!("{} has no runs", file.display()))?;
        runs.extend(serde_json::from_value::<Vec<CardRun>>(part).map_err(|e| format!("{}: {e}", file.display()))?);
    }
    println!("{}", pure::render_card_tables(&runs, &pure::card_gate(&runs)));
    Ok(())
}

// --- The run -------------------------------------------------------------------------------------------------------

fn write_results(run_dir: &Path, options: &Options, rows: &[Row], spent: &Spend) -> Result<(), Failure> {
    let value = json!({
        "runId": options.run_id,
        "plain": { "cli": options.plain.name(), "model": options.plain_model, "effort": options.plain_effort },
        "faberVendors": options.vendors.iter().map(|v| v.name()).collect::<Vec<_>>(),
        "judge": { "model": options.judge_model, "effort": options.judge_effort },
        "cap": { "usd": options.cap.max_usd, "freshTokens": options.cap.max_fresh_tokens, "perRunUsd": options.per_run_usd },
        "spent": spent,
        "rows": rows,
    });
    std::fs::write(run_dir.join("results.json"), serde_json::to_string_pretty(&value)?)?;
    std::fs::write(run_dir.join("tables.md"), pure::render_tables(rows, spent, &options.cap))?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let options = Options::read()?;
    if !options.gate.is_empty() {
        return gate_files(&options.gate);
    }
    let requests = load_requests(&options.only)?;
    if options.card_pairs {
        return card_main(&options, &requests).await;
    }
    println!(
        "run {}: {} request(s), Faber on {:?} (Auto) against plain {} ({} {}), judge {} {} (a fresh Claude Code session; the other vendor is not used), cap US${} or {} first-time tokens",
        options.run_id,
        requests.len(),
        options.vendors.iter().map(|v| v.name()).collect::<Vec<_>>(),
        options.plain.name(),
        options.plain_model,
        options.plain_effort,
        options.judge_model,
        options.judge_effort,
        options.cap.max_usd,
        options.cap.max_fresh_tokens
    );
    for (i, r) in requests.iter().enumerate() {
        println!("  {}. {} [{:?}] proof {:?}, A is {:?}", i + 1, r.id, r.kind, r.proof, pure::a_side(&options.run_id, i));
    }
    if options.dry_run {
        return Ok(());
    }
    if faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)) {
        return Err("an environment variable would bill Claude Code per use; the benchmark runs on the subscription only".into());
    }
    let node = find_cli("node").ok_or("node is not on the PATH (the fixtures and proofs are Node scripts)")?;
    let claude = find_cli("claude").ok_or("Claude Code is not installed (the judge needs it)")?;
    let mut clis = BTreeMap::new();
    for vendor in &options.vendors {
        clis.insert(*vendor, find_cli(vendor.name()).ok_or_else(|| format!("{} is not installed", vendor.name()))?);
    }
    let plain_cli = find_cli(options.plain.name()).ok_or_else(|| format!("{} is not installed", options.plain.name()))?;
    let root = options.dir.join(&options.run_id);
    std::fs::create_dir_all(&root)?;
    let judge_dir = root.join("judge");
    std::fs::create_dir_all(&judge_dir)?;

    let (mut rows, mut spent) = (Vec::new(), Spend::default());
    for (index, request) in requests.iter().enumerate() {
        let a = pure::a_side(&options.run_id, index);
        let mut row = Row {
            request: request.id.clone(),
            title: request.title.clone(),
            kind: request.kind,
            a,
            verdict: None,
            winner: None,
            faber: None,
            plain: None,
            judge_cost_usd: None,
            judge_usage: Usage::default(),
            skipped: None,
        };
        if let Some(why) = options.cap.reached(&spent) {
            println!("stopping before {}: {why}", request.id);
            row.skipped = Some(format!("the cap was reached: {why}"));
            rows.push(row);
            continue;
        }
        println!("[{}/{}] {}", index + 1, requests.len(), request.id);
        let work = prepare(request, &root, &node.command, true)?;
        let mut plain = run_plain(&options, &plain_cli, request, &work, options.cap.allowance(&spent, options.per_run_usd)).await;
        spent.add(&plain.usage, plain.cost_usd);
        println!("  plain: ok {}, {:.0}s, US${:?}, {}", plain.ok, plain.secs, plain.cost_usd, plain.error.as_deref().unwrap_or(""));
        let mut faber = if let Some(why) = options.cap.reached(&spent) {
            failed(Side::Faber, 0.0, format!("not run: {why}"))
        } else {
            run_faber(&clis, request, &work, &root, options.cap.allowance(&spent, options.per_run_usd), false).await
        };
        spent.add(&faber.usage, faber.cost_usd);
        println!("  faber: ok {}, {:.0}s, US${:?}, {}", faber.ok, faber.secs, faber.cost_usd, faber.error.as_deref().unwrap_or(""));
        prove(&node, request, &work, &mut plain, &root).await;
        prove(&node, request, &work, &mut faber, &root).await;
        let proofs = |s: &SideResult| s.proof.as_ref().map(|p| p.pass);
        println!("  proofs: faber {:?}, plain {:?}", proofs(&faber), proofs(&plain));
        if faber.reply.trim().is_empty() || plain.reply.trim().is_empty() {
            println!("  no judging: a side gave no reply");
        } else if let Some(why) = options.cap.reached(&spent) {
            row.skipped = Some(format!("not judged, the cap was reached: {why}"));
        } else {
            let (first, second) = if a == Side::Faber { (&faber, &plain) } else { (&plain, &faber) };
            let judged =
                judge(&options, &claude, request, first, second, &judge_dir, options.cap.allowance(&spent, options.per_run_usd)).await;
            spent.add(&judged.usage, judged.cost_usd);
            row.judge_cost_usd = judged.cost_usd;
            row.judge_usage = judged.usage;
            row.winner = judged.verdict.as_ref().and_then(|v| pure::reveal(v.pick, a));
            println!(
                "  judge: {:?} -> {:?}; {}",
                judged.verdict.as_ref().map(|v| v.pick),
                row.winner,
                judged.verdict.as_ref().map_or("", |v| &v.reason)
            );
            row.verdict = judged.verdict;
            if let Some(note) = judged.note {
                println!("  {note}");
            }
        }
        row.faber = Some(faber);
        row.plain = Some(plain);
        rows.push(row);
        write_results(&root, &options, &rows, &spent)?;
    }
    write_results(&root, &options, &rows, &spent)?;
    println!("\n{}", pure::render_tables(&rows, &spent, &options.cap));
    println!("results: {}", root.join("results.json").display());
    Ok(())
}
