//! Faber's light benchmark (never part of `cargo t`: it talks to the real CLIs with the person's own sign-in and spends real
//! quota). It runs the requests of `bench/requests/` twice, through Faber (the engine, with Auto, kits, the guide and the
//! rules) and through the plain CLI, in new folders under the run folder, checks each result with its proof script, has a
//! fresh session of the other available vendor judge the two replies blind (shown as "A" and "B", revealed after the vote) and writes the
//! results and the report tables. Everything runs without a window: each child starts through the engine's own process
//! start (no shell, no console window, its own job, below-normal priority). How to run it: `docs/dev/benchmark.md`.
//!
//!     cargo run -p faber-engine --example bench -- [--dry-run] [--plain claude|codex] [--vendors claude[,codex]]
//!         [--only id,id] [--run-id id] [--dir folder] [--cap-usd 8] [--cap-tokens 3000000] [--per-run-usd 1.5]
//!         [--plain-model m] [--plain-effort e] [--judge-model m] [--judge-effort e]
//!
//! The pure parts (request files, the A/B deal, the verdict, what a CLI reported, the cap, the tables) are
//! `bench/pure.rs`, tested by `engine/tests/all.rs`.

#[path = "../../bench/pure.rs"]
mod pure;

use faber_engine::cli::{Cli, find_cli};
use faber_engine::model::{Area, Conversation, Message, Permissions, Role, Settings, Status, Vendor};
use faber_engine::proc::{self, Proc};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use pure::{Cap, Kind, Outcome, Request, Row, Side, SideResult, Spend, Usage};
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
    judge_vendor: Vendor,
    judge_weaker: bool,
    cap: Cap,
    per_run_usd: f64,
    dry_run: bool,
}

impl Options {
    fn read() -> Result<Options, Failure> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        let vendor = |name: &str| Vendor::ALL.into_iter().find(|v| v.name() == name).ok_or_else(|| format!("{name} is claude or codex"));
        let plain = vendor(&value("--plain").unwrap_or_else(|| "claude".into()))?;
        let vendors = value("--vendors").unwrap_or_else(|| plain.name().into()).split(',').map(vendor).collect::<Result<Vec<_>, _>>()?;
        let number = |name: &str, default: f64| -> Result<f64, Failure> {
            let number = value(name).map_or(Ok(default), |v| v.parse::<f64>().map_err(|_| format!("{name} takes a number")))?;
            if number.is_finite() && number > 0.0 { Ok(number) } else { Err(format!("{name} takes a positive finite number").into()) }
        };
        let (default_model, default_effort) = match plain {
            Vendor::Claude => ("claude-sonnet-5-5", "medium"),
            Vendor::Codex => ("gpt-6.1-sol", "medium"),
        };
        Ok(Options {
            run_id: value("--run-id").unwrap_or_else(|| chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()),
            dir: value("--dir").map_or_else(|| std::env::temp_dir().join("faber-bench"), PathBuf::from),
            only: value("--only").map(|v| v.split(',').map(String::from).collect()).unwrap_or_default(),
            plain,
            vendors,
            plain_model: value("--plain-model").unwrap_or_else(|| default_model.into()),
            plain_effort: value("--plain-effort").unwrap_or_else(|| default_effort.into()),
            judge_model: value("--judge-model").unwrap_or_default(),
            judge_effort: value("--judge-effort").unwrap_or_default(),
            judge_vendor: plain,
            judge_weaker: true,
            cap: Cap { max_usd: number("--cap-usd", 8.0)?, max_fresh_tokens: number("--cap-tokens", 3_000_000.0)? as u64 },
            per_run_usd: number("--per-run-usd", 1.5)?,
            dry_run: args.iter().any(|a| a == "--dry-run"),
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

fn prepare(request: &Request, root: &Path, node: &str) -> Result<Work, Failure> {
    let base = root.join(&request.id);
    let work = Work { pristine: base.join("pristine"), plain: base.join("plain"), repo: base.join("faber").join("repo") };
    for dir in [&work.pristine, &work.plain, &work.repo] {
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
    for dir in [&work.plain, &work.repo] {
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
        usage_incomplete: true,
        cost_usd: None,
        models: vec![],
        reply: String::new(),
        folder: None,
        proof: None,
        stopped_by_cap: false,
    }
}

fn not_run(side: Side, why: String) -> SideResult {
    let mut result = failed(side, 0.0, format!("not run: {why}"));
    result.cost_usd = Some(0.0);
    result.usage_incomplete = false;
    result
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
        Ok(mut outcome) => {
            if options.plain == Vendor::Codex {
                pure::price_codex(&mut outcome, &options.plain_model, |input, output, read, write| {
                    faber_engine::usage::api_price_for_prompt(&options.plain_model, input, output, read, write, 0, input)
                });
            }
            outcome
        }
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
        usage_incomplete: outcome.usage_incomplete,
        cost_usd: outcome.cost_usd,
        models,
        reply: outcome.text,
        folder: (request.kind == Kind::Project).then(|| work.plain.to_string_lossy().into_owned()),
        proof: None,
        stopped_by_cap: outcome.error.as_deref().is_some_and(|e| e.contains("max_budget")),
    }
}

fn message_usage(messages: &[&Message]) -> Usage {
    let mut usage = Usage::default();
    for m in messages {
        let (read, write) = (m.cache_read_tokens.unwrap_or(0), m.cache_write_tokens.unwrap_or(0));
        // A message's input is the whole prompt, cache included (`claude::result_events`).
        usage.add(&Usage {
            input: m.input_tokens.unwrap_or(0).saturating_sub(read + write),
            output: m.output_tokens.unwrap_or(0),
            cache_read: read,
            cache_write: write,
        });
    }
    usage
}

fn ledger_snapshot(ledger: &faber_engine::ledger::Ledger) -> (f64, u64) {
    [&ledger.claude, &ledger.codex].iter().fold((0.0, 0), |(cost, unknown), vendor| {
        (cost + vendor.own_spend.values().sum::<f64>(), unknown + vendor.unmetered.values().map(|n| u64::from(*n)).sum::<u64>())
    })
}

#[cfg(test)]
mod cost_tests {
    use super::*;

    #[test]
    fn both_vendors_and_auxiliary_work_count_after_the_baseline() {
        let mut ledger = faber_engine::ledger::Ledger::default();
        ledger.claude.spend(Some(10.0), false, 1.0, 0);
        let before = ledger_snapshot(&ledger);
        ledger.claude.spend(Some(0.05), false, 2.0, 0); // Router.
        ledger.codex.spend(Some(0.25), false, 3.0, 0); // Auxiliary.
        ledger.claude.spend(Some(0.5), false, 4.0, 0); // Main reply.
        assert!((pure::ledger_cost(before, ledger_snapshot(&ledger)).unwrap() - 0.8).abs() < 1e-9);
        ledger.codex.spend(None, true, 5.0, 0);
        assert_eq!(pure::ledger_cost(before, ledger_snapshot(&ledger)), None);
    }

    #[test]
    fn an_exhausted_budget_does_not_make_unstarted_work_unknown() {
        let cap = Cap { max_usd: 1.0, max_fresh_tokens: 1000 };
        let mut spent = Spend::default();
        spent.add(&Usage::default(), Some(1.0));
        let skipped = not_run(Side::Faber, cap.reached(&spent).unwrap());
        spent.add(&skipped.usage, skipped.cost_usd);
        assert_eq!(skipped.cost_usd, Some(0.0));
        assert!(!spent.unknown_cost);
        assert_eq!(spent.usd, 1.0);
    }
}

/// Waits for the reply to settle: it ended and Faber's checks on it ended (or a project has none to run), or it needs the
/// person. Ledger costs arrive at completed work boundaries; internal and auxiliary calls already in flight can overshoot.
async fn settle(engine: &Engine, id: &str, project: bool, allowance: f64, baseline: (f64, u64)) -> (Option<Conversation>, bool) {
    let started = Instant::now();
    let (mut quiet, mut capped): (Option<(String, Instant)>, bool) = (None, false);
    while started.elapsed() < RUN_LIMIT {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Some(conversation) = engine.open(id) else { continue };
        if !capped && pure::ledger_cost(baseline, ledger_snapshot(&engine.ledger())).is_none_or(|cost| cost >= allowance) {
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
    (engine.open(id), capped)
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

async fn run_faber(clis: &BTreeMap<Vendor, Cli>, request: &Request, work: &Work, root: &Path, allowance: f64, billed: bool) -> SideResult {
    let started = Instant::now();
    let state = root.join(&request.id).join("faber").join("state");
    let engine = match Engine::new(
        EngineOptions {
            data_dir: state,
            clis: clis.clone(),
            spawn: logging_spawner(root.join(&request.id).join("faber").join("spawns.log")),
            windows: cfg!(windows),
            claude_env_billed: billed,
            max_warm: 1,
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    ) {
        Ok(engine) => engine,
        Err(error) => return failed(Side::Faber, started.elapsed().as_secs_f64(), error.to_string()),
    };
    let baseline = ledger_snapshot(&engine.ledger());
    // What the app's shell gives the engine: the native git for separate copies, and the verifier for the checks.
    engine.set_git(Arc::new(|| {
        faber_engine::project::git::Process::find().ok().map(|git| Box::new(git) as Box<dyn faber_engine::project::git::Executor + Send>)
    }));
    engine.set_verifier(faber_engine::verify::Verifier::real(None, None));
    let result = async {
        // The same trust as the plain side's bypassPermissions: Full access.
        engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
        engine.check_plans(Duration::from_secs(30)).await;
        if let Some(codex) = clis.get(&Vendor::Codex) {
            // The app reads Codex's limits and model list at start (src-tauri/src/main.rs); Auto needs both.
            let real = proc::real_spawner();
            let prefix = codex.prefix.clone();
            let spawn: proc::Spawner =
                Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
            let limits =
                faber_engine::codex::read_codex_limits(&codex.command, &work.repo, cfg!(windows), spawn.clone(), Duration::from_secs(20))
                    .await;
            engine.note_account_events(Vendor::Codex, limits);
            let list =
                faber_engine::codex::list_codex_models(&codex.command, &work.repo, cfg!(windows), spawn, Duration::from_secs(20)).await;
            let recommended = list.iter().find(|m| m.is_default).map(|m| m.id.clone());
            engine.set_codex_models(list.iter().map(|m| m.id.clone()).collect(), recommended);
        }
        if pure::ledger_cost(baseline, ledger_snapshot(&engine.ledger())).is_none_or(|cost| cost >= allowance) {
            return Err("the budget is exhausted or setup cost is unknown; no request started".into());
        }
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
        Ok::<_, String>((id.clone(), settle(&engine, &id, request.kind == Kind::Project, allowance, baseline).await))
    }
    .await;
    let wall = started.elapsed().as_secs_f64();
    let mut side = match result {
        Err(error) => failed(Side::Faber, wall, error),
        Ok((_, (None, _))) => failed(Side::Faber, wall, "the engine lost the conversation"),
        Ok((_, (Some(conversation), capped))) => {
            let replies: Vec<&Message> = conversation.messages.iter().filter(|m| m.role == Role::Assistant).collect();
            let usage = message_usage(&replies);
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
                usage_incomplete: true,
                cost_usd: None, // The complete ledger delta is taken after shutdown, including late usage.
                models,
                reply: reply.to_string(),
                folder: (request.kind == Kind::Project)
                    .then(|| conversation.copy.as_ref().map_or_else(|| work.repo.clone(), |c| c.folder()).to_string_lossy().into_owned()),
                proof: None,
                stopped_by_cap: capped,
            }
        }
    };
    engine.shutdown();
    // The engine's processes end with it; give Windows a moment to release the folders before the next step reads them.
    tokio::time::sleep(Duration::from_millis(500)).await;
    side.cost_usd = pure::ledger_cost(baseline, ledger_snapshot(&engine.ledger()));
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

fn select_judge(plain: Vendor, mut find: impl FnMut(Vendor) -> Option<Cli>) -> Result<(Vendor, Cli, bool), String> {
    let other = match plain {
        Vendor::Claude => Vendor::Codex,
        Vendor::Codex => Vendor::Claude,
    };
    find(other)
        .map(|cli| (other, cli, false))
        .or_else(|| find(plain).map(|cli| (plain, cli, true)))
        .ok_or_else(|| "no judge CLI is installed".into())
}

fn strongest_judge(vendor: Vendor) -> faber_engine::route::Profile {
    faber_engine::route::PROFILES
        .iter()
        .filter(|p| p.vendor == vendor)
        .max_by_key(|p| (p.iq, p.agent))
        .copied()
        .expect("each supported vendor has measured profiles")
}

#[cfg(test)]
mod judge_tests {
    use super::*;

    #[test]
    fn fake_clis_cover_each_vendor_alone_and_both_without_requiring_the_missing_one() {
        for (plain, available, expected, weak) in [
            (Vendor::Claude, vec![Vendor::Claude], Vendor::Claude, true),
            (Vendor::Codex, vec![Vendor::Codex], Vendor::Codex, true),
            (Vendor::Claude, vec![Vendor::Claude, Vendor::Codex], Vendor::Codex, false),
            (Vendor::Codex, vec![Vendor::Claude, Vendor::Codex], Vendor::Claude, false),
        ] {
            let (vendor, _, fallback) =
                select_judge(plain, |v| available.contains(&v).then(|| Cli { command: v.name().into(), prefix: vec![] })).unwrap();
            assert_eq!((vendor, fallback), (expected, weak));
        }
    }

    #[test]
    fn fake_judge_outputs_keep_verdicts_blind_and_count_both_vendors() {
        for vendor in Vendor::ALL {
            let profile = strongest_judge(vendor);
            assert!(
                faber_engine::route::PROFILES.iter().filter(|p| p.vendor == vendor).all(|p| (p.iq, p.agent) <= (profile.iq, profile.agent))
            );
            let cmd = judge_args(vendor, profile.model, profile.effort, 0.5);
            if vendor == Vendor::Codex {
                assert!(cmd.iter().any(|arg| arg == "--ignore-user-config"));
            }
            assert!(cmd.contains(&profile.model.to_string()));
            assert!(cmd.contains(&if vendor == Vendor::Claude { "--no-session-persistence" } else { "--ephemeral" }.to_string()));
            let stdout = match vendor {
                Vendor::Claude => {
                    r#"{"type":"result","subtype":"success","result":"{\"winner\":\"B\"}","total_cost_usd":0.2,"usage":{"input_tokens":100,"output_tokens":5,"cache_read_input_tokens":20,"cache_creation_input_tokens":0}}"#
                }
                Vendor::Codex => {
                    "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"winner\\\":\\\"B\\\"}\"}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":100,\"output_tokens\":5,\"cached_input_tokens\":20,\"cache_write_input_tokens\":0}}"
                }
            };
            let mut run = Finished { stdout: stdout.into(), stderr: String::new(), code: Some(0), timed_out: false };
            let result = judge_result(vendor, profile.model, &run);
            assert_eq!(result.verdict.unwrap().pick, pure::Pick::B);
            assert!(result.cost_usd.is_some_and(|cost| cost > 0.0));
            run.code = Some(1);
            assert!(judge_result(vendor, profile.model, &run).verdict.is_none());
        }
    }

    #[tokio::test]
    async fn exhausted_budget_starts_no_judge_even_with_a_missing_cli() {
        let options = Options::read().unwrap();
        let request = pure::parse_request("---\nid: x\nkind: chat\n---\nQuestion").unwrap();
        let side = not_run(Side::Plain, "fixture".into());
        let result = judge(
            &options,
            &Cli { command: "missing-judge-must-not-start".into(), prefix: vec![] },
            &request,
            &side,
            &side,
            Path::new("."),
            0.0,
        )
        .await;
        assert_eq!(result.cost_usd, Some(0.0));
        assert!(result.verdict.is_none());
    }
}

async fn judge(options: &Options, cli: &Cli, request: &Request, a: &SideResult, b: &SideResult, dir: &Path, allowance: f64) -> Judged {
    if !allowance.is_finite() || allowance <= 0.0 {
        return Judged { verdict: None, usage: Usage::default(), cost_usd: Some(0.0), note: Some("not judged: no budget remains".into()) };
    }
    let prompt = pure::judge_prompt(&request.prompt, &entry_of(a), &entry_of(b));
    let cmd = judge_args(options.judge_vendor, &options.judge_model, &options.judge_effort, allowance);
    let none = |note: String| Judged { verdict: None, usage: Usage::default(), cost_usd: None, note: Some(note) };
    let input = if options.judge_vendor == Vendor::Codex { format!("{}\n\n{prompt}", pure::JUDGE_INSTRUCTIONS) } else { prompt };
    let run = match run_cli(cli, &cmd, dir, &input, Duration::from_secs(10 * 60)).await {
        Ok(run) => run,
        Err(error) => return none(error),
    };
    judge_result(options.judge_vendor, &options.judge_model, &run)
}

fn judge_args(vendor: Vendor, model: &str, effort: &str, allowance: f64) -> Vec<String> {
    if vendor == Vendor::Codex {
        return args(&[
            "exec",
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "--skip-git-repo-check",
            "-s",
            "read-only",
            "-m",
            model,
            "-c",
            &format!("model_reasoning_effort=\"{effort}\""),
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "mcp_servers={}",
            "-c",
            "features.shell_tool=false",
            "-c",
            "web_search=\"disabled\"",
            "-c",
            "project_doc_max_bytes=0",
            "--disable",
            "hooks",
            "--disable",
            "apps",
            "--disable",
            "plugins",
            "--disable",
            "computer_use",
            "--disable",
            "browser_use",
            "-",
        ]);
    }
    args(&[
        "-p",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--permission-mode",
        "dontAsk",
        "--model",
        model,
        "--effort",
        effort,
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
        pure::JUDGE_INSTRUCTIONS,
    ])
}

fn judge_result(vendor: Vendor, model: &str, run: &Finished) -> Judged {
    let none = |note: String| Judged { verdict: None, usage: Usage::default(), cost_usd: None, note: Some(note) };
    let parsed = match vendor {
        Vendor::Claude => pure::parse_claude_result(&run.stdout),
        Vendor::Codex => pure::parse_codex_events(&run.stdout).map(|mut out| {
            pure::price_codex(&mut out, model, |input, output, read, write| {
                faber_engine::usage::api_price_for_prompt(model, input, output, read, write, 0, input)
            });
            out
        }),
    };
    match parsed {
        Err(error) => none(format!("{error}; stderr: {}", run.stderr.chars().take(300).collect::<String>())),
        Ok(Outcome { text, usage, cost_usd, ok, .. }) => {
            let verdict = (ok && !run.timed_out && run.code == Some(0)).then(|| pure::parse_verdict(&text)).flatten();
            let note = verdict.is_none().then(|| format!("the judge gave no verdict: {}", text.chars().take(300).collect::<String>()));
            Judged { verdict, usage, cost_usd, note }
        }
    }
}

// --- The run -------------------------------------------------------------------------------------------------------

fn write_results(run_dir: &Path, options: &Options, rows: &[Row], spent: &Spend) -> Result<(), Failure> {
    let value = json!({
        "runId": options.run_id,
        "plain": { "cli": options.plain.name(), "model": options.plain_model, "effort": options.plain_effort },
        "faberVendors": options.vendors.iter().map(|v| v.name()).collect::<Vec<_>>(),
        "judge": { "vendor": options.judge_vendor.name(), "model": options.judge_model, "effort": options.judge_effort, "weakerSameVendor": options.judge_weaker },
        "cap": { "usd": options.cap.max_usd, "freshTokens": options.cap.max_fresh_tokens, "perRunUsd": options.per_run_usd },
        "spent": spent,
        "rows": rows,
    });
    std::fs::write(run_dir.join("results.json"), serde_json::to_string_pretty(&value)?)?;
    let coverage = if options.judge_weaker {
        "Weaker review: only the plain vendor's CLI was found; a fresh same-vendor judge was used."
    } else {
        "The judge uses the other vendor than plain. Faber Auto may also use that vendor, so independence from every Faber author is not guaranteed."
    };
    std::fs::write(
        run_dir.join("tables.md"),
        format!(
            "Judge: {} {} ({}). {coverage}\n\n{}",
            options.judge_model,
            options.judge_effort,
            options.judge_vendor.name(),
            pure::render_tables(rows, spent, &options.cap)
        ),
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let mut options = Options::read()?;
    let requests = load_requests(&options.only)?;
    println!(
        "run {}: {} request(s), Faber on {:?} (Auto) against plain {} ({} {}), judge {} {} (automatic vendor selection), cap US${} or {} first-time tokens",
        options.run_id,
        requests.len(),
        options.vendors.iter().map(|v| v.name()).collect::<Vec<_>>(),
        options.plain.name(),
        options.plain_model,
        options.plain_effort,
        if options.judge_model.is_empty() { "strongest measured profile" } else { &options.judge_model },
        if options.judge_effort.is_empty() { "(profile effort)" } else { &options.judge_effort },
        options.cap.max_usd,
        options.cap.max_fresh_tokens
    );
    for (i, r) in requests.iter().enumerate() {
        println!("  {}. {} [{:?}] proof {:?}, A is {:?}", i + 1, r.id, r.kind, r.proof, pure::a_side(&options.run_id, i));
    }
    if options.dry_run {
        return Ok(());
    }
    let node = find_cli("node").ok_or("node is not on the PATH (the fixtures and proofs are Node scripts)")?;
    let (judge_vendor, judge_cli, weaker) = select_judge(options.plain, |v| find_cli(v.name()))?;
    options.judge_vendor = judge_vendor;
    options.judge_weaker = weaker;
    if (options.plain == Vendor::Claude || options.vendors.contains(&Vendor::Claude) || judge_vendor == Vendor::Claude)
        && faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name))
    {
        return Err("an environment variable would bill Claude Code per use; the benchmark runs on the subscription only".into());
    }
    let strongest = strongest_judge(judge_vendor);
    if options.judge_model.is_empty() {
        options.judge_model = strongest.model.into();
    }
    if options.judge_effort.is_empty() {
        options.judge_effort = strongest.effort.into();
    }
    println!(
        "judge: {} {} ({}); {}",
        options.judge_model,
        options.judge_effort,
        judge_vendor.name(),
        if weaker { "weaker same-vendor review" } else { "other vendor than plain; Faber Auto can use either vendor" }
    );
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
            judge_attempted: false,
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
        let work = prepare(request, &root, &node.command)?;
        let mut plain = run_plain(&options, &plain_cli, request, &work, options.cap.allowance(&spent, options.per_run_usd)).await;
        spent.add(&plain.usage, plain.cost_usd);
        println!("  plain: ok {}, {:.0}s, US${:?}, {}", plain.ok, plain.secs, plain.cost_usd, plain.error.as_deref().unwrap_or(""));
        let mut faber = if let Some(why) = options.cap.reached(&spent) {
            not_run(Side::Faber, why)
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
            row.judge_attempted = true;
            let judged =
                judge(&options, &judge_cli, request, first, second, &judge_dir, options.cap.allowance(&spent, options.per_run_usd)).await;
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
