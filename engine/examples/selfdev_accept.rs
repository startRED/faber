//! Manual live acceptance probe of one whole self-development task (never in CI). A project conversation on the real Claude
//! Code CLI gets "Fix the failing test, run the project's checks, and commit on your branch." in a throwaway git repository,
//! in Full access with separate copies on, on the person's own subscription. It asserts, with timings, that:
//!   1. the conversation got its own separate copy on a `faber/…` branch;
//!   2. the person's checkout stayed untouched (same branch, same head, clean tree, same files);
//!   3. the test passes in the copy and the agent did not weaken it;
//!   4. a commit exists on the copy's branch and nothing is left uncommitted;
//!   5. the agent called Faber's `verify` tool and checked its work after its last edit (see `expect_checks`);
//!   6. the spend stayed small (tokens and cost come from the replies' usage).
//!
//! With `--review` a second turn of the task asks for the helper from the other AI (Codex) and asserts that its report
//! arrives.
//!
//! The task tries Haiku (low effort) first and falls back to Sonnet (low effort) when it is not done, unless
//! `--model haiku|sonnet` picks one. `--keep` leaves the throwaway folder for a look at what happened. The throwaway
//! folders go to the system's temporary folder when this worktree's path holds a credential folder name (a worktree under
//! `.claude/worktrees/`), which Faber refuses to work in; `--dir <folder>` picks another place.
//!
//! Run from the worktree root, with cargo and rustfmt on the PATH (the agent and Faber's checks run them in the copy):
//! `cargo run -p faber-engine --example selfdev_accept [-- --review] [-- --model sonnet] [-- --keep]`
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Conversation, Message, Permissions, Role, Settings, Status, ToolUse, Vendor};
use faber_engine::proc;
use faber_engine::router::{Choice, Effort};
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TASK: &str = "Fix the failing test, run the project's checks, and commit on your branch.";
const REVIEW: &str = "Now ask the helper from the other AI for a review of your commit on this branch: call the helper tool once, \
    with a brief that names the branch and says what to look at, then end with one line saying what the helper reported.";
/// A tiny task on Haiku costs cents; anything above this is a bug worth a look.
const MAX_COST_USD: f64 = 0.50;
/// The time limit for the task and its explicit checks.
const TURN_LIMIT: Duration = Duration::from_secs(480);
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

/// Runs git in `root` with the person's global configuration out of the way, and returns stdout when it succeeded.
fn git_try(root: &Path, args: &[&str]) -> Option<String> {
    let out =
        hidden("git").args(args).current_dir(root).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git(root: &Path, args: &[&str]) -> String {
    git_try(root, args).unwrap_or_else(|| panic!("throwaway git failed: {args:?}"))
}

/// The throwaway project: a crate whose one test fails, a guide that declares its checks, and the task as a file.
fn seed(repo: &Path) -> std::io::Result<()> {
    let files: Vec<(&str, String)> = vec![
        (
            "Cargo.toml",
            "[package]\nname = \"probe-sum\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n# Its own workspace: the folder sits under another \
             crate's target/.\n[workspace]\n"
                .into(),
        ),
        ("src/lib.rs", "/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n".into()),
        ("tests/add.rs", "use probe_sum::add;\n\n#[test]\nfn adds_two_numbers() {\n    assert_eq!(add(2, 3), 5);\n}\n".into()),
        (".gitignore", "target/\n".into()),
        (
            "AGENTS.md",
            "# Probe project\n\nA tiny crate that adds two numbers.\n\n## Rules\n\n- Keep changes small.\n- Before you commit, check your work with Faber's verify tool.\n\n## Proof commands\n\nAll exit 0 \
                 before any commit:\n\n```bash\ncargo test\n```\n".into(),
        ),
        (
            "docs/tasks/010-fix.md",
            "# 010 Fix the failing test\n\n`cargo test` fails: `add(2, 3)` must be 5. Fix the code, never the test.\n".into(),
        ),
    ];
    for (name, text) in files {
        let path = repo.join(name);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, text)?;
    }
    // The lock file is committed, as in a real project.
    let lock = hidden("cargo").args(["generate-lockfile", "--offline"]).current_dir(repo).output()?;
    assert!(lock.status.success(), "cargo generate-lockfile failed: is cargo on the PATH?");
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "--", "."]);
    git(repo, &["-c", "user.name=Faber Probe", "-c", "user.email=probe@example.invalid", "commit", "-q", "-m", "first"]);
    Ok(())
}

enum Outcome {
    Pass,
    Fail,
    Skip,
}

struct Row {
    name: &'static str,
    outcome: Outcome,
    detail: String,
    at: f64,
}

/// Every assertion with the seconds since the task was sent, so a slow step shows where the time went.
struct Report {
    started: Instant,
    rows: Vec<Row>,
}

impl Report {
    fn new() -> Self {
        Report { started: Instant::now(), rows: vec![] }
    }
    fn add(&mut self, name: &'static str, outcome: Outcome, detail: impl Into<String>) {
        self.rows.push(Row { name, outcome, detail: detail.into(), at: self.started.elapsed().as_secs_f64() });
    }
    fn check(&mut self, name: &'static str, ok: bool, detail: impl Into<String>) {
        self.add(name, if ok { Outcome::Pass } else { Outcome::Fail }, detail);
    }
    fn skip(&mut self, name: &'static str, why: &str) {
        self.add(name, Outcome::Skip, why);
    }
    fn ok(&self) -> bool {
        !self.rows.iter().any(|r| matches!(r.outcome, Outcome::Fail))
    }
    fn print(&self) {
        for row in &self.rows {
            let tag = match row.outcome {
                Outcome::Pass => "PASS",
                Outcome::Fail => "FAIL",
                Outcome::Skip => "SKIP",
            };
            println!("[{tag}] {:>6.1}s  {}: {}", row.at, row.name, row.detail);
        }
    }
}

fn is_edit(tool: &ToolUse) -> bool {
    matches!(tool.name.as_str(), "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
}

/// A call of Faber's `verify` tool, whatever name the CLI's driver stored it under.
fn is_verify(tool: &ToolUse) -> bool {
    tool.name == faber_engine::verify::TOOL || tool.name == faber_engine::verify::CLAUDE_NAME || tool.name == faber_engine::verify::NAME
}

/// The agent explicitly called verify and its last check passed after its last edit.
fn expect_checks(report: &mut Report, messages: &[Message]) {
    let tools: Vec<&ToolUse> = messages.iter().flat_map(|m| m.tools.iter().flatten()).collect();
    let verified = tools.iter().filter(|t| is_verify(t));
    let answered = verified.clone().any(|t| t.ok == Some(true));
    report.check(
        "the agent called Faber's verify tool, and it answered",
        answered,
        format!("{} verify call(s), ok: {:?}", verified.clone().count(), verified.map(|t| t.ok).collect::<Vec<_>>()),
    );
    let last_edit = tools.iter().rposition(|t| is_edit(t));
    let last_check = tools.iter().rposition(|t| (t.check || is_verify(t)) && !t.declined);
    let after_edit = match (last_edit, last_check) {
        (Some(e), Some(c)) => c > e,
        (None, Some(_)) => true,
        _ => false,
    };
    report.check(
        "the agent's last check came after its last edit and passed",
        after_edit && last_check.is_some_and(|c| tools[c].ok == Some(true)),
        format!(
            "{} edit(s), {} check call(s), last check ok: {:?}",
            tools.iter().filter(|t| is_edit(t)).count(),
            tools.iter().filter(|t| t.check || is_verify(t)).count(),
            last_check.map(|c| tools[c].ok)
        ),
    );
}

/// The conversation as the engine holds it now, with its settled last reply when the turn has ended.
fn reply_of(conversation: &Conversation) -> Option<&Message> {
    conversation.messages.last().filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))
}

/// Waits for the task or review reply to end; checks run only inside the agent's turn.
async fn wait_settled(engine: &Engine, id: &str, assistants_before: usize, report: &mut Report) -> Option<Conversation> {
    let started = Instant::now();
    let mut seen = String::new();
    let mut announced = false;
    while started.elapsed() < TURN_LIMIT {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Some(conversation) = engine.open(id) else { continue };
        let at = report.started.elapsed().as_secs_f64();
        if !announced && assistants_before == 0 && conversation.copy.is_some() {
            announced = true;
            eprintln!("  [{at:5.1}s] copy ready");
        }
        let Some(last) = conversation.messages.last() else { continue };
        let line = format!(
            "{} {:?} wake {:?} status {:?}",
            conversation.messages.len(),
            last.role,
            last.wake.as_ref().map(|w| w.why.as_str()),
            last.status
        );
        if line != seen {
            eprintln!("  [{at:5.1}s] message {line}");
            seen = line;
        }
        let assistants = conversation.messages.iter().filter(|m| m.role == Role::Assistant).count();
        if reply_of(&conversation).filter(|_| assistants > assistants_before).is_none() {
            continue;
        }
        return Some(conversation);
    }
    None
}

fn tokens(messages: &[Message]) -> u64 {
    messages
        .iter()
        .map(|m| {
            m.input_tokens.unwrap_or(0)
                + m.output_tokens.unwrap_or(0)
                + m.cache_read_tokens.unwrap_or(0)
                + m.cache_write_tokens.unwrap_or(0)
        })
        .sum()
}

struct Options {
    review: bool,
    keep: bool,
}

/// One whole attempt in a fresh throwaway repository and data folder, on one model.
async fn attempt(model: &str, options: &Options, base: &Path) -> Result<Report, Box<dyn std::error::Error>> {
    let world = base.join(format!("selfdev-accept-{}", uuid::Uuid::new_v4().simple()));
    let repo = world.join("repo");
    std::fs::create_dir_all(&repo)?;
    seed(&repo)?;
    let head_before = git(&repo, &["rev-parse", "HEAD"]);
    let source_before: Vec<String> = ["src/lib.rs"].iter().map(|f| std::fs::read_to_string(repo.join(f)).unwrap_or_default()).collect();

    let mut clis = BTreeMap::from([(Vendor::Claude, find_cli("claude").ok_or("Claude Code is not installed")?)]);
    let codex = find_cli("codex");
    if let Some(cli) = codex.clone() {
        clis.insert(Vendor::Codex, cli);
    }
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
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )?;
    // The shell gives the engine the native git for separate copies and the verifier for the checks
    // (src-tauri/src/main.rs); without them every project conversation works in the project folder and has no checks.
    engine.set_git(Arc::new(|| {
        faber_engine::project::git::Process::find().ok().map(|git| Box::new(git) as Box<dyn faber_engine::project::git::Executor + Send>)
    }));
    engine.set_verifier(faber_engine::verify::Verifier::real(None, None));
    // Full access, and separate copies on (the default; the probe does not touch that setting).
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    let project = engine.add_project(&repo.to_string_lossy())?;
    // A fresh data folder does not know the plan, and Faber starts nothing on its own on an AI that may bill per use.
    engine.check_plans(Duration::from_secs(30)).await;
    // The app reads Codex's limits and model list at start (src-tauri/src/main.rs); the helper needs both to pick its model.
    if let (true, Some(codex)) = (options.review, &codex) {
        let real = proc::real_spawner();
        let prefix = codex.prefix.clone();
        let spawn: proc::Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
        let limits =
            faber_engine::codex::read_codex_limits(&codex.command, &repo, cfg!(windows), spawn.clone(), Duration::from_secs(20)).await;
        engine.note_account_events(Vendor::Codex, limits);
        let list = faber_engine::codex::list_codex_models(&codex.command, &repo, cfg!(windows), spawn, Duration::from_secs(20)).await;
        let recommended = list.iter().find(|m| m.is_default).map(|m| m.id.clone());
        engine.set_codex_models(list.iter().map(|m| m.id.clone()).collect(), recommended);
    }

    let choice = Choice { vendor: "claude".into(), model: Some(model.into()), effort: Some(Effort::Low) };
    let mut report = Report::new();
    println!("--- task on {model} (low effort), throwaway {}", world.display());
    let id = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id.clone()),
        text: TASK.into(),
        images: vec![],
        files: vec![],
        choice: choice.clone(),
    })?;
    let finished = wait_settled(&engine, &id, 0, &mut report).await;
    let mut conversation = finished.clone().or_else(|| engine.open(&id));
    report.check(
        "the task and its explicit checks settled inside the time limit",
        finished.is_some(),
        format!("limit {}s", TURN_LIMIT.as_secs()),
    );

    if let Some(conv) = &conversation {
        let reply = reply_of(conv);
        report.check(
            "the last reply finished without an error",
            reply.is_some_and(|m| m.status == Some(Status::Done) && m.error.is_none()),
            format!("status {:?}, error {:?}", reply.and_then(|m| m.status), reply.and_then(|m| m.error.clone())),
        );

        // 1. The separate copy on a faber/ branch.
        let copy = conv.copy.clone();
        report.check(
            "the conversation got a separate copy on a faber/ branch",
            copy.as_ref()
                .is_some_and(|c| c.branch.starts_with("faber/") && Path::new(&c.path).is_dir() && Path::new(&c.path) != repo.as_path()),
            format!("{:?}", copy.as_ref().map(|c| (&c.branch, &c.path))),
        );

        // 2. The person's checkout is untouched.
        let status = git(&repo, &["status", "--porcelain"]);
        let same_source = ["src/lib.rs"]
            .iter()
            .zip(&source_before)
            .all(|(f, before)| &std::fs::read_to_string(repo.join(f)).unwrap_or_default() == before);
        report.check(
            "the person's checkout is untouched",
            git(&repo, &["rev-parse", "HEAD"]) == head_before
                && git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]) == "main"
                && status.is_empty()
                && same_source,
            format!("head unchanged, tree clean: {}, sources unchanged: {same_source}", status.is_empty()),
        );

        if let Some(copy) = &copy {
            let folder: PathBuf = copy.folder();
            // 3. The test passes in the copy, and the test itself was not changed.
            let timer = Instant::now();
            let passes = hidden("cargo").args(["test", "--quiet"]).current_dir(&folder).output().is_ok_and(|o| o.status.success());
            report.check(
                "cargo test passes in the copy",
                passes,
                format!("the probe's own run took {:.1}s", timer.elapsed().as_secs_f64()),
            );
            let range = format!("main..{}", copy.branch);
            let test_changed = git_try(&repo, &["diff", "--stat", &format!("main...{}", copy.branch), "--", "tests/"]);
            report.check(
                "the test was fixed by changing the code, not the test",
                test_changed.as_deref() == Some("")
                    && git_try(&repo, &["diff", "--stat", &format!("main...{}", copy.branch), "--", "src/"]).is_some_and(|d| !d.is_empty()),
                format!("tests/ diff: {:?}", test_changed.unwrap_or_default()),
            );

            // 4. A commit on the copy's branch, and nothing left uncommitted in the copy.
            let commits = git_try(&repo, &["rev-list", "--count", &range]).and_then(|n| n.parse::<u32>().ok()).unwrap_or(0);
            let left = git_try(&folder, &["status", "--porcelain"]).unwrap_or_else(|| "unreadable".into());
            report.check("a commit exists on the copy's branch", commits >= 1, format!("{commits} commit(s) ahead of main"));
            report.check("nothing is left uncommitted in the copy", left.is_empty(), format!("status: {left:?}"));
        }

        // 5. The Checks.
        expect_checks(&mut report, &conv.messages[..]);

        // 6. Spend.
        let cost: Option<f64> = conv.messages.iter().filter_map(|m| m.cost_usd).reduce(|a, b| a + b);
        let used = tokens(&conv.messages);
        report.check(
            "spend stayed small",
            cost.is_none_or(|c| c <= MAX_COST_USD) && used > 0,
            format!("cost {} , {used} tokens (input+output+cache)", cost.map_or("n/a".into(), |c| format!("US${c:.4}"))),
        );

        // Optional: the helper from the other AI.
        if options.review {
            if codex.is_none() {
                report.skip("the helper's report arrives", "Codex is not installed");
            } else {
                let before = conv.messages.iter().filter(|m| m.role == Role::Assistant).count();
                engine.send(SendRequest {
                    conversation_id: Some(id.clone()),
                    area: Area::Project,
                    project_id: Some(project.id.clone()),
                    text: REVIEW.into(),
                    images: vec![],
                    files: vec![],
                    choice,
                })?;
                let second = wait_settled(&engine, &id, before, &mut report).await;
                let helper = second
                    .as_ref()
                    .and_then(|c| reply_of(c))
                    .into_iter()
                    .flat_map(|m| m.tools.iter().flatten())
                    .find(|t| t.name == faber_engine::helper::TOOL || t.name == faber_engine::helper::CLAUDE_NAME);
                report.check(
                    "the helper's report arrives",
                    helper.is_some_and(|t| t.ok == Some(true) && t.output.as_deref().is_some_and(|o| !o.trim().is_empty())),
                    format!(
                        "helper call ok {:?}, report: {:?}",
                        helper.map(|t| t.ok),
                        helper.and_then(|t| t.output.as_deref()).map(|o| o.trim().chars().take(300).collect::<String>())
                    ),
                );
                conversation = second.or(conversation);
            }
        }
    } else {
        report.check("the conversation exists", false, "the engine has no such conversation");
    }
    if let Some(conv) = &conversation {
        let total: Option<f64> = conv.messages.iter().filter_map(|m| m.cost_usd).reduce(|a, b| a + b);
        println!("total tokens {}, cost {}", tokens(&conv.messages), total.map_or("n/a".into(), |c| format!("US${c:.4}")));
    }
    engine.shutdown();
    if !options.keep {
        for tries in 0..20 {
            match std::fs::remove_dir_all(&world) {
                Ok(()) => break,
                Err(error) if tries == 19 => eprintln!("could not remove {}: {error}", world.display()),
                Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
    }
    Ok(report)
}

/// Where the throwaway folders go: this worktree's `target/`, unless that path holds a folder Faber refuses to work in
/// (a worktree under `.claude/worktrees/` does: `.claude` is a CLI credential folder, `error.workingFolder`), then the
/// system's temporary folder. `--dir <folder>` picks another.
fn throwaway_base(worktree: &Path, dir: Option<&String>) -> PathBuf {
    if let Some(dir) = dir {
        return PathBuf::from(dir);
    }
    let target = worktree.join("target");
    let refused = target.components().any(|c| matches!(c.as_os_str().to_str(), Some(".claude" | ".codex" | ".gemini")));
    if refused { std::env::temp_dir() } else { target }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current = std::env::current_dir()?;
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().canonicalize()?;
    assert_eq!(current.canonicalize()?, expected, "run only from this worktree's root");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let options = Options { review: flag("--review"), keep: flag("--keep") };
    let base = throwaway_base(&expected, value("--dir").as_ref());
    let models: Vec<String> = value("--model").map_or_else(|| vec!["haiku".into(), "sonnet".into()], |m| vec![m]);
    if let Some(other) = value("--scenario").filter(|s| s != "task") {
        return Err(format!("The automatic fix scenario was removed; --scenario only accepts task (got {other}).").into());
    }
    let mut ok = false;
    for (index, model) in models.iter().enumerate() {
        let report = attempt(model, &options, &base).await?;
        report.print();
        ok = report.ok();
        println!("{} (task) on {model}", if ok { "ACCEPTANCE OK" } else { "ACCEPTANCE FAILED" });
        if ok {
            break;
        }
        if index + 1 < models.len() {
            println!("falling back to {}", models[index + 1]);
        }
    }
    if ok { Ok(()) } else { Err("self-development acceptance failed".into()) }
}
