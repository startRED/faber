//! Faber's built-in verifier, V1 (R190; spec `docs/superpowers/specs/2026-10-17-built-in-verifier.md`): the project's own
//! checks, run on the PC from the conversation, so an agent learns in seconds whether its change works, before any push.
//!
//! - **The checks** are the project's "Checks" (R137): the commands under "Proof commands" in AGENTS.md or CLAUDE.md, or a
//!   detected default, as the person confirmed them in Settings > Projects when they did (`crate::project::mission::view`).
//! - **Fast** runs only the checks a change can affect, found from each command (`select`): a check whose files did not
//!   change is left out, and one that passed on exactly the same content is not run again (a content hash, never a time).
//!   **Full** runs every check.
//! - **Each check** is a process with no shell and no window, in its own kill-on-close job, below normal priority. A new
//!   verify of the same folder stops the running one, and one Cargo build runs at a time across every folder. Its output
//!   streams to a log in `.faber/verify/` (the last 20 kept); only the short failures of R155 stay in memory.
//! - **Idle** costs nothing: no process, no timer, no watcher. The agent gets a short text; the screen gets keys.
//!
//! The agent calls it as Faber's `verify` tool, like the helper and the browser (`crate::helper::mcp`).

mod changes;
mod run;
mod select;

pub use changes::{Changed, changed, own_log};
pub use run::{GAME_CPU, GUARD_EVERY, Guard, KEPT_LOGS, Real, Runner, Spec, Started};
pub use select::{Scripts, Watch, heavy, scripts, watch};

use crate::project::proof::output;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The name both drivers give the call, whatever the CLI calls it, so the screen and the engine know one name.
pub const TOOL: &str = "Verify";
/// The tool as Codex knows it, and as Faber's MCP server lists it to Claude Code.
pub const NAME: &str = "verify";
/// The tool as Claude Code names it: `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__faber__verify";
/// How long one check may run before it is ended: a cold Cargo build of a large workspace fits.
pub const DEADLINE: Duration = Duration::from_secs(15 * 60);

/// How much a verify checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// The checks the changed files can affect.
    Fast,
    /// Every check.
    Full,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Level::Fast => "fast",
            Level::Full => "full",
        }
    }
}

const DESCRIPTION: &str = "Runs this project's own checks (its \"Proof commands\": lint, type checks, tests, builds) on this \
folder and returns what failed, each failure with its file and line and the tool's suggested fix. `fast` (the default) runs \
only the checks your changes can affect and skips one that already passed on the same content; `full` runs every check. Use \
fast after a change, and full once before you say the work is done. The whole output stays in the log it names.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "level": { "type": "string", "enum": ["fast", "full"], "description": "fast: what your changes affect; full: every check." }
        },
        "additionalProperties": false
    })
}

/// The tool as a Codex `dynamicTools` entry.
pub fn codex_tool() -> Value {
    json!({ "type": "function", "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// The tool as Faber's MCP server lists it.
pub fn mcp_tool() -> Value {
    json!({ "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// The level the agent asked for: fast unless it says full.
pub fn level(input: &Value) -> Result<Level, &'static str> {
    match input["level"].as_str().map(str::trim) {
        None | Some("") | Some("fast") => Ok(Level::Fast),
        Some("full") => Ok(Level::Full),
        Some(_) => Err("the level is fast or full"),
    }
}

/// What a run needs, read under the engine's lock (and from git, away from it).
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// Where the commands run: the conversation's copy, or the project folder.
    pub folder: PathBuf,
    pub level: Option<Level>,
    /// The project's checks, one command line each.
    pub commands: Vec<String>,
    /// What installs the dependencies, run first when they are missing (`npm ci` in a copy without `node_modules`).
    pub setup: Vec<String>,
    /// What the PC adds to the environment (`Local`).
    pub local: Local,
    /// The changed files; `None` outside git, where fast runs every check.
    pub changed: Option<Changed>,
}

/// What one PC adds to the checks' environment, from the untracked `.faber/verify.local.json` of the project folder:
/// `{"env": {"RUSTUP_HOME": "…"}, "path": ["E:\\Apps\\rust\\cargo\\bin"]}`. `path` goes before the PATH.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Local {
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub path: Vec<String>,
}

/// The local file's place inside a project folder.
pub const LOCAL_FILE: &str = ".faber/verify.local.json";

impl Local {
    /// Reads the project's local file; a missing or unreadable one adds nothing.
    pub fn read(project: &Path) -> Local {
        std::fs::read(project.join(LOCAL_FILE))
            .ok()
            .filter(|b| b.len() < 64 * 1024)
            .and_then(|b| serde_json::from_slice::<Local>(&b).ok())
            .map(|mut l| {
                l.env.retain(|k, v| !k.is_empty() && !k.contains(['=', '\0']) && !v.contains('\0') && !k.eq_ignore_ascii_case("PATH"));
                l.path.retain(|p| !p.is_empty() && !p.contains(['\0', ';']));
                l
            })
            .unwrap_or_default()
    }

    /// The variables a check gets on top of the engine's own.
    fn vars(&self) -> Vec<(String, String)> {
        let mut vars: Vec<(String, String)> = self.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        if !self.path.is_empty() {
            let sep = if cfg!(windows) { ";" } else { ":" };
            let now = std::env::var("PATH").or_else(|_| std::env::var("Path")).unwrap_or_default();
            vars.push(("PATH".into(), self.path.iter().cloned().chain([now]).collect::<Vec<_>>().join(sep)));
        }
        vars
    }
}

/// How one check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Passed,
    Failed,
    /// It ran out of time, or could not start.
    NotFinished,
    /// It passed before on exactly the same content.
    Unchanged,
    /// No changed file is one it reads.
    LeftOut,
}

/// One check of a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub command: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub millis: u64,
    /// The short failures (R155: at most 20, of 8 lines each).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
    /// The tool's one-line tally ("failed (exit 1): 2 errors").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

/// A whole run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub level: Level,
    pub checks: Vec<CheckResult>,
    #[serde(default)]
    pub millis: u64,
    /// The log, relative to the folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// Why nothing ran, as a key: `reason.verify.noChecks`, `reason.verify.nothingChanged`, `reason.verify.stopped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Outcome {
    fn nothing(level: Level, reason: &str) -> Outcome {
        Outcome { level, checks: vec![], millis: 0, log: None, reason: Some(reason.into()) }
    }

    /// How many checks failed or did not finish.
    /// Whether the agent may read it as a pass: nothing failed, and the checks ran or had nothing to check. A stop or a
    /// project with no checks is no pass.
    pub fn ok(&self) -> bool {
        self.failed() == 0 && !matches!(self.reason.as_deref(), Some("reason.verify.stopped" | "reason.verify.noChecks"))
    }

    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|c| matches!(c.verdict, Verdict::Failed | Verdict::NotFinished)).count()
    }

    fn count(&self, verdict: Verdict) -> usize {
        self.checks.iter().filter(|c| c.verdict == verdict).count()
    }

    /// What the agent reads: a tally, each failed check with its short failures, then one line per other check.
    pub fn text(&self) -> String {
        let level = self.level.word();
        match self.reason.as_deref() {
            Some("reason.verify.noChecks") => {
                return "This project declares no checks. Faber reads the commands under a \"Proof commands\" heading in \
                        AGENTS.md or CLAUDE.md (a fenced list, one command per line), or finds `npm test` or `cargo test`."
                    .into();
            }
            Some("reason.verify.nothingChanged") => {
                return "No file changed in this folder since its base, so the fast checks had nothing to check. Ask for the \
                        full level to run every check."
                    .into();
            }
            Some("reason.verify.stopped") => {
                return "These checks were stopped: a newer verify of this folder started, or Faber is closing.".into();
            }
            _ => {}
        }
        let mut tally = vec![];
        for (verdict, words) in [
            (Verdict::Failed, "failed"),
            (Verdict::NotFinished, "did not finish"),
            (Verdict::Passed, "passed"),
            (Verdict::Unchanged, "unchanged since it passed"),
            (Verdict::LeftOut, "left out (no changed file it reads)"),
        ] {
            let n = self.count(verdict);
            if n > 0 {
                tally.push(format!("{n} {words}"));
            }
        }
        let mut out = format!("Faber ran the {level} checks of this folder in {}: {}.\n", seconds(self.millis), tally.join(", "));
        for check in self.checks.iter().filter(|c| matches!(c.verdict, Verdict::Failed | Verdict::NotFinished)) {
            out.push_str(&format!("\nFAILED {} ({}): {}\n", check.command, seconds(check.millis), check.summary));
            for failure in &check.failures {
                out.push_str(failure);
            }
        }
        let mut rest = String::new();
        for check in &self.checks {
            match check.verdict {
                Verdict::Passed => rest.push_str(&format!("passed: {} ({})\n", check.command, seconds(check.millis))),
                Verdict::Unchanged => rest.push_str(&format!("unchanged since it passed: {}\n", check.command)),
                Verdict::LeftOut => rest.push_str(&format!("left out: {}\n", check.command)),
                _ => {}
            }
        }
        if !rest.is_empty() {
            out.push('\n');
            out.push_str(&rest);
        }
        if let Some(log) = &self.log {
            out.push_str(&format!("\nThe whole output is in {log}."));
        }
        out.trim_end().to_string()
    }
}

fn seconds(millis: u64) -> String {
    if millis < 10_000 { format!("{:.1} s", millis as f64 / 1000.0) } else { format!("{} s", (millis + 500) / 1000) }
}

/// One planned check.
struct Planned {
    command: String,
    tokens: Vec<String>,
    watch: Watch,
    heavy: bool,
}

/// The project's commands as checks. A line that needs a shell or names a program by path is refused (R137's rule), and
/// so is one that names a sign-in folder or file, as every command an agent runs is (`permissions::touches_credentials`).
fn plan(commands: &[String], folder: &Path) -> (Vec<Planned>, Vec<CheckResult>) {
    let (mut planned, mut refused) = (vec![], vec![]);
    let scripts = scripts(folder);
    let refuse = |command: &str, why: &str| CheckResult {
        command: command.into(),
        verdict: Verdict::NotFinished,
        millis: 0,
        failures: vec![format!("{why}\n")],
        summary: "not run".into(),
    };
    for command in commands {
        let env = |name: &str| std::env::var(name).ok();
        let credentials = crate::permissions::touches_credentials(&json!({ "command": command }), folder.to_str(), &env);
        match crate::project::mission::command_tokens(command) {
            _ if credentials => refused.push(refuse(command, "This command names a sign-in folder or file, so Faber does not run it.")),
            Ok(tokens) => {
                planned.push(Planned { command: command.clone(), watch: watch(&tokens, &scripts), heavy: heavy(&tokens), tokens })
            }
            Err(_) => refused.push(refuse(command, "This command needs a shell or names a program by path, so Faber does not run it.")),
        }
    }
    (planned, refused)
}

/// The content a check read: the base, the command, the PC's additions and every changed file it reads with its hash.
/// `None` when a file it reads has no hash, so nothing cuts off on a guess.
fn input_key(check: &Planned, local: &Local, changed: &Changed) -> Option<String> {
    let mut text = format!("{}\n{}\n{:?}\n", changed.base, check.command, local);
    for (path, hash) in changed.files.iter().filter(|(p, _)| check.watch.reads(p)) {
        text.push_str(&format!("{path}\0{}\n", hash.as_deref()?));
    }
    Some(crate::project::kit::digest(text.as_bytes()))
}

/// The keys of checks that passed, per folder and command, so the same content is not checked twice. Bounded: it
/// forgets everything past `MAX_REMEMBERED`.
#[derive(Default)]
struct Passed(HashMap<(PathBuf, String), String>);
const MAX_REMEMBERED: usize = 512;

/// The verifier: one per engine. It holds no process and no timer while idle.
pub struct Verifier {
    runner: Arc<dyn Runner>,
    deadline: Duration,
    /// One Cargo build at a time, across every folder.
    heavy: Arc<tokio::sync::Semaphore>,
    passed: Mutex<Passed>,
    /// The run of each folder, which a newer one stops, with who started it (a conversation's id).
    running: Mutex<HashMap<PathBuf, Running>>,
    next: AtomicU64,
    clock: fn() -> u64,
    /// The game guard (`run::Guard`): none where the shell cannot tell.
    guard: Option<Guard>,
}

struct Running {
    id: u64,
    owner: String,
    task: tokio::task::AbortHandle,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Verifier {
    pub fn new(runner: Arc<dyn Runner>) -> Verifier {
        Verifier::with_deadline(runner, DEADLINE)
    }

    /// A verifier whose checks end after `deadline` (tests).
    pub fn with_deadline(runner: Arc<dyn Runner>, deadline: Duration) -> Verifier {
        Verifier {
            runner,
            deadline,
            heavy: Arc::new(tokio::sync::Semaphore::new(1)),
            passed: Mutex::new(Passed::default()),
            running: Mutex::new(HashMap::new()),
            next: AtomicU64::new(0),
            clock: now_millis,
            guard: None,
        }
    }

    /// The real verifier the shell gives the engine, with its game guard.
    pub fn real(guard: Option<Guard>) -> Arc<Verifier> {
        Arc::new(Verifier { guard, ..Verifier::new(Arc::new(Real)) })
    }

    /// The same verifier with a game guard (tests).
    pub fn with_guard(self, guard: Guard) -> Verifier {
        Verifier { guard: Some(guard), ..self }
    }

    /// While a game is in front, no new check starts: the run waits, reading the guard every `GUARD_EVERY`. A stop or a
    /// newer run ends the wait with the run.
    async fn game_over(&self) {
        while self.guard.as_ref().is_some_and(|in_front| in_front()) {
            tokio::time::sleep(GUARD_EVERY).await;
        }
    }

    /// Runs `request` to its end. A newer verify of the same folder stops this one, which then ends `reason.verify.stopped`.
    pub async fn verify(self: &Arc<Self>, request: Request) -> Outcome {
        let (folder, level) = (request.folder.clone(), request.level.unwrap_or(Level::Fast));
        self.start(&tokio::runtime::Handle::current(), folder, "", level, async move { request }).await
    }

    /// Starts a verify of `folder` for `owner` now, on `runtime`, whose request is still being made (git is asked what
    /// changed): it counts as running from this call, so a stop or a newer verify that comes while git answers stops it
    /// too, and no check of it ever starts. The future returned ends with its outcome.
    pub fn start<F: Future<Output = Request> + Send + 'static>(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        folder: PathBuf,
        owner: &str,
        level: Level,
        request: F,
    ) -> impl Future<Output = Outcome> + Send + 'static + use<F> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let me = self.clone();
        let task = runtime.spawn(async move { me.run(request.await).await });
        let run = Running { id, owner: owner.to_string(), task: task.abort_handle() };
        if let Some(older) = self.running.lock().unwrap_or_else(|e| e.into_inner()).insert(folder.clone(), run) {
            older.task.abort();
        }
        let me = self.clone();
        async move {
            let outcome = task.await.unwrap_or_else(|_| Outcome::nothing(level, "reason.verify.stopped"));
            let mut running = me.running.lock().unwrap_or_else(|e| e.into_inner());
            if running.get(&folder).is_some_and(|r| r.id == id) {
                running.remove(&folder);
            }
            outcome
        }
    }

    /// Stops the verify of `folder` that `owner` started, if one runs: its processes end with their job. Another's run of
    /// the same folder (two conversations of one project without a copy share it) goes on. True when one was stopped.
    pub fn stop(&self, folder: &Path, owner: &str) -> bool {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if running.get(folder).is_none_or(|r| r.owner != owner) {
            return false;
        }
        let Some(r) = running.remove(folder) else { return false };
        r.task.abort();
        true
    }

    /// Whether no verify runs anywhere.
    pub fn idle(&self) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Whether a verify of `folder` runs now.
    pub fn busy(&self, folder: &Path) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).get(folder).is_some_and(|r| !r.task.is_finished())
    }

    async fn run(&self, request: Request) -> Outcome {
        let level = request.level.unwrap_or(Level::Fast);
        let began = std::time::Instant::now();
        let (mut planned, refused) = plan(&request.commands, &request.folder);
        if planned.is_empty() && refused.is_empty() {
            return Outcome::nothing(level, "reason.verify.noChecks");
        }
        if level == Level::Fast && request.changed.as_ref().is_some_and(|c| c.files.is_empty()) {
            return Outcome::nothing(level, "reason.verify.nothingChanged");
        }
        if level == Level::Fast
            && let Some(changed) = &request.changed
        {
            planned.extend(rust_checks(&request.folder, changed, &request.commands));
        }
        let folder_key = request.folder.clone();
        let mut results: Vec<CheckResult> = refused;
        let mut todo: Vec<(&Planned, Option<String>)> = vec![];
        for check in &planned {
            let key = request.changed.as_ref().and_then(|c| input_key(check, &request.local, c));
            if level == Level::Fast
                && let Some(changed) = &request.changed
            {
                if !changed.files.iter().any(|(p, _)| check.watch.reads(p)) {
                    results.push(CheckResult {
                        command: check.command.clone(),
                        verdict: Verdict::LeftOut,
                        millis: 0,
                        failures: vec![],
                        summary: String::new(),
                    });
                    continue;
                }
                let before =
                    self.passed.lock().unwrap_or_else(|e| e.into_inner()).0.get(&(folder_key.clone(), check.command.clone())).cloned();
                if key.is_some() && before == key {
                    results.push(CheckResult {
                        command: check.command.clone(),
                        verdict: Verdict::Unchanged,
                        millis: 0,
                        failures: vec![],
                        summary: String::new(),
                    });
                    continue;
                }
            }
            todo.push((check, key));
        }
        // Quick checks first, so the agent hears of a type error before a build ends; a Cargo build waits for its turn.
        todo.sort_by_key(|(check, _)| check.heavy);
        let vars = request.local.vars();
        let (mut log, log_path) = if todo.is_empty() {
            (run::Log::none(), None)
        } else {
            run::Log::open(&request.folder, &format!("{}-{}.log", (self.clock)(), level.word()))
        };
        let root = request.folder.to_string_lossy().into_owned();
        // The dependencies a fresh copy lacks come first, the way a mission's setup does.
        if todo.iter().any(|(check, _)| on_node(&check.tokens))
            && request.setup.iter().any(|s| s == "npm ci")
            && !request.folder.join("node_modules").exists()
        {
            self.game_over().await;
            let spec = Spec {
                program: "npm".into(),
                args: vec!["ci".into(), "--no-audit".into(), "--no-fund".into()],
                cwd: request.folder.clone(),
                env: vars.clone(),
            };
            log.line("== npm ci (the dependencies were not installed in this folder) ==");
            let ran = run::check(&*self.runner, &spec, &mut log, self.deadline, self.guard.as_ref()).await;
            if ran.exit != Some(0) {
                results.push(result("npm ci", &ran, &root));
            }
        }
        for (check, key) in todo {
            self.game_over().await;
            let _turn = if check.heavy { self.heavy.clone().acquire_owned().await.ok() } else { None };
            log.line(&format!("== {} ==", check.command));
            let spec =
                Spec { program: check.tokens[0].clone(), args: check.tokens[1..].to_vec(), cwd: request.folder.clone(), env: vars.clone() };
            let ran = run::check(&*self.runner, &spec, &mut log, self.deadline, self.guard.as_ref()).await;
            log.line(&format!("== {}: exit {:?} after {} ms ==", check.command, ran.exit, ran.took.as_millis()));
            let out = result(&check.command, &ran, &root);
            if out.verdict == Verdict::Passed
                && let Some(key) = key
            {
                let mut passed = self.passed.lock().unwrap_or_else(|e| e.into_inner());
                if passed.0.len() >= MAX_REMEMBERED {
                    passed.0.clear();
                }
                passed.0.insert((folder_key.clone(), check.command.clone()), key);
            } else {
                self.passed.lock().unwrap_or_else(|e| e.into_inner()).0.remove(&(folder_key.clone(), check.command.clone()));
            }
            results.push(out);
        }
        log.flush();
        // The screen and the agent read the checks in the order the project declares them.
        let order = |command: &str| request.commands.iter().position(|c| c == command).unwrap_or(usize::MAX);
        results.sort_by_key(|c| order(&c.command));
        Outcome { level, checks: results, millis: began.elapsed().as_millis() as u64, log: log_path, reason: None }
    }
}

/// What fast adds for a Rust change: Clippy on each package it touches, named by its manifest (`select::rust_manifests`,
/// with `--workspace` when that manifest is a workspace too), which catches a compile error in seconds on a warm folder
/// where the project's own list may compile nothing. Never in full, never in the project's list; a package the list
/// already checks that way is not checked twice. The manifest is one argument, spaces and all: no shell reads it.
// ponytail: Clippy only; the package's unit tests could join it once a run measures them at a few seconds.
fn rust_checks(folder: &Path, changed: &Changed, declared: &[String]) -> Vec<Planned> {
    select::rust_manifests(folder, &changed.files)
        .into_iter()
        .map(|(manifest, workspace)| {
            let mut tokens: Vec<String> = ["cargo", "clippy", "--manifest-path"].map(String::from).to_vec();
            tokens.push(manifest);
            tokens.extend(workspace.then(|| "--workspace".to_string()));
            tokens.extend(["--all-targets", "--", "-D", "warnings"].map(String::from));
            tokens
        })
        .map(|tokens| Planned { command: tokens.join(" "), tokens, watch: Watch::All, heavy: true })
        .filter(|check| !declared.contains(&check.command))
        .collect()
}

/// Whether a check may run on Node, so it needs the folder's `node_modules`: any program but the ones known not to.
fn on_node(tokens: &[String]) -> bool {
    let program = tokens.first().map(|p| p.trim_end_matches(".exe").to_ascii_lowercase()).unwrap_or_default();
    !["cargo", "rustfmt", "rustc", "rustup", "go", "python", "python3", "pytest", "dotnet", "make", "git"].contains(&program.as_str())
}

/// A finished check read into its short form; a line that looks like a secret never reaches the agent.
fn result(command: &str, ran: &run::Ran, root: &str) -> CheckResult {
    let millis = ran.took.as_millis() as u64;
    if let Some(why) = &ran.refused {
        return CheckResult {
            command: command.into(),
            verdict: Verdict::NotFinished,
            millis,
            failures: vec![format!("It could not start: {why}.\n")],
            summary: "not run".into(),
        };
    }
    let report = output::read(&output::withheld(&ran.output), ran.exit, root);
    let verdict = match ran.exit {
        Some(0) => Verdict::Passed,
        Some(_) => Verdict::Failed,
        None => Verdict::NotFinished,
    };
    let most = if report.fallback { output::LAST_LINES } else { output::FAILURE_LINES };
    let mut failures: Vec<String> = report.failures.iter().take(output::SHOWN).map(|f| f.short(most)).collect();
    if report.failures.len() > output::SHOWN {
        failures.push(format!("({} more failures in the log)\n", report.failures.len() - output::SHOWN));
    }
    let summary = if ran.timed_out { format!("did not finish in {} minutes", DEADLINE.as_secs() / 60) } else { report.summary };
    CheckResult { command: command.into(), verdict, millis, failures: if verdict == Verdict::Passed { vec![] } else { failures }, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A runner that starts nothing: each program prints what the table says and exits with its code, or never ends.
    #[derive(Default)]
    struct Fake {
        started: Mutex<Vec<String>>,
        /// Program and first argument → (output, exit code); `None` never ends.
        script: HashMap<String, (String, Option<i32>)>,
        dropped: Arc<AtomicUsize>,
        /// What the game guard asked of each check's processor share, in order.
        capped: Arc<Mutex<Vec<bool>>>,
    }

    struct Ends(Arc<AtomicUsize>);
    impl Drop for Ends {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Runner for Fake {
        fn start(&self, spec: &Spec) -> std::io::Result<Started> {
            let line = std::iter::once(spec.program.clone()).chain(spec.args.iter().cloned()).collect::<Vec<_>>().join(" ");
            self.started.lock().unwrap().push(line.clone());
            let (out, code) = self.script.get(&line).cloned().ok_or_else(|| std::io::Error::other("unknown program"))?;
            let ends = Ends(self.dropped.clone());
            let exit: std::pin::Pin<Box<dyn std::future::Future<Output = Option<i32>> + Send>> = match code {
                Some(code) => Box::pin(async move {
                    let _ends = ends;
                    Some(code)
                }),
                None => Box::pin(async move {
                    let _ends = ends;
                    std::future::pending::<()>().await;
                    None
                }),
            };
            let capped = self.capped.clone();
            let throttle: Box<dyn Fn(bool) + Send + Sync> = Box::new(move |game| capped.lock().unwrap().push(game));
            Ok(Started {
                stdout: Box::new(std::io::Cursor::new(out.into_bytes())),
                stderr: Box::new(tokio::io::empty()),
                exit,
                throttle: Some(throttle),
            })
        }
    }

    fn fake(script: &[(&str, &str, Option<i32>)]) -> Fake {
        Fake { script: script.iter().map(|(l, o, c)| (l.to_string(), (o.to_string(), *c))).collect(), ..Fake::default() }
    }

    const COMMANDS: [&str; 4] = ["npm run lint", "npm run docs", "cargo fmt --all --check", "cargo t --claims"];

    /// A folder whose `lint` script is a linter (it selects) and whose `docs` script is a program of its own (it reads
    /// everything).
    fn request(dir: &Path, level: Level, files: &[(&str, &str)]) -> Request {
        let scripts = r#"{"scripts":{"lint":"oxlint ui","docs":"node scripts/docs.mjs"}}"#;
        std::fs::write(dir.join("package.json"), scripts).unwrap();
        Request {
            folder: dir.to_path_buf(),
            level: Some(level),
            commands: COMMANDS.iter().map(|c| c.to_string()).collect(),
            changed: Some(Changed {
                base: "2".repeat(40),
                files: files.iter().map(|(p, h)| (p.to_string(), Some(h.to_string()))).collect(),
            }),
            ..Request::default()
        }
    }

    fn passing() -> Fake {
        fake(&[
            ("npm run lint", "Found 0 warnings and 0 errors.", Some(0)),
            ("npm run docs", "docs ok", Some(0)),
            ("cargo fmt --all --check", "", Some(0)),
            ("cargo t --claims", "claims ok", Some(0)),
        ])
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    #[test]
    fn fast_runs_only_what_the_changed_files_affect_and_full_runs_everything() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(passing());
        let v = Arc::new(Verifier::new(runner.clone()));
        let fast = rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("engine/src/a.rs", "h1")])));
        let verdicts: Vec<(&str, Verdict)> = fast.checks.iter().map(|c| (c.command.as_str(), c.verdict)).collect();
        assert_eq!(
            verdicts,
            [
                ("npm run lint", Verdict::LeftOut),
                ("npm run docs", Verdict::Passed),
                ("cargo fmt --all --check", Verdict::Passed),
                ("cargo t --claims", Verdict::Passed),
            ]
        );
        assert_eq!(*runner.started.lock().unwrap(), ["npm run docs", "cargo fmt --all --check", "cargo t --claims"]);
        assert_eq!(fast.failed(), 0);
        assert!(fast.ok());
        assert!(fast.log.as_deref().is_some_and(|l| l.starts_with(".faber/verify/") && l.ends_with("-fast.log")));
        assert!(fast.text().contains("3 passed, 1 left out"), "{}", fast.text());
        runner.started.lock().unwrap().clear();
        let full = rt().block_on(v.verify(request(dir.path(), Level::Full, &[("engine/src/a.rs", "h1")])));
        assert!(full.checks.iter().all(|c| c.verdict == Verdict::Passed), "{full:?}");
        assert_eq!(runner.started.lock().unwrap().len(), 4, "full never selects and never cuts off");
    }

    #[test]
    fn fast_compiles_the_rust_package_a_change_touches_and_names_the_error_s_place() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("engine/src")).unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("engine/Cargo.toml"), "[package]\nname = \"faber-engine\"\n").unwrap();
        let clippy = "cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings";
        let error = "error[E0308]: mismatched types\n --> engine/src/a.rs:3:7\n  |\n3 |     x\n  |     ^ expected `u32`, found `&str`\n\n\
                     error: could not compile `faber-engine` (lib) due to 1 previous error\n";
        let mut runner = passing();
        runner.script.insert(clippy.into(), (error.into(), Some(101)));
        let runner = Arc::new(runner);
        let v = Arc::new(Verifier::new(runner.clone()));
        let fast = rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("engine/src/a.rs", "h1")])));
        let last = fast.checks.last().unwrap();
        assert_eq!((last.command.as_str(), last.verdict), (clippy, Verdict::Failed), "{fast:?}");
        assert!(last.failures.concat().contains("engine/src/a.rs:3:7"), "{:?}", last.failures);
        assert!(fast.text().contains(&format!("FAILED {clippy}")), "{}", fast.text());
        runner.started.lock().unwrap().clear();
        rt().block_on(v.verify(request(dir.path(), Level::Full, &[("engine/src/a.rs", "h1")])));
        assert!(!runner.started.lock().unwrap().iter().any(|c| c.contains("clippy")), "full runs only the project's list");
        runner.started.lock().unwrap().clear();
        rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("ui/App.tsx", "h2")])));
        assert!(!runner.started.lock().unwrap().iter().any(|c| c.contains("clippy")), "no Rust change, no Rust check added");
    }

    #[test]
    fn a_check_that_passed_on_the_same_content_is_not_run_again_and_a_changed_file_runs_it() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(passing());
        let v = Arc::new(Verifier::new(runner.clone()));
        let rt = rt();
        rt.block_on(v.verify(request(dir.path(), Level::Fast, &[("docs/a.md", "h1")])));
        runner.started.lock().unwrap().clear();
        let again = rt.block_on(v.verify(request(dir.path(), Level::Fast, &[("docs/a.md", "h1")])));
        assert!(again.checks.iter().any(|c| c.command == "npm run docs" && c.verdict == Verdict::Unchanged), "{again:?}");
        assert!(runner.started.lock().unwrap().is_empty(), "an unchanged save triggers nothing");
        assert_eq!(again.log, None, "nothing ran, so no log");
        let edited = rt.block_on(v.verify(request(dir.path(), Level::Fast, &[("docs/a.md", "h2")])));
        assert!(edited.checks.iter().any(|c| c.command == "npm run docs" && c.verdict == Verdict::Passed));
        // A file with no hash (deleted, too big) is never trusted.
        let mut gone = request(dir.path(), Level::Fast, &[("docs/a.md", "h2")]);
        gone.changed.as_mut().unwrap().files[0].1 = None;
        runner.started.lock().unwrap().clear();
        rt.block_on(v.verify(gone));
        assert!(runner.started.lock().unwrap().contains(&"npm run docs".to_string()));
    }

    #[test]
    fn a_failure_reads_as_the_short_form_and_never_holds_more_than_r155_allows() {
        let dir = tempfile::tempdir().unwrap();
        let mut tsc = String::new();
        for n in 0..30 {
            tsc.push_str(&format!("ui/App.tsx({n},5): error TS2322: Type 'string' is not assignable to type 'number'.\n"));
        }
        let runner = Arc::new(fake(&[
            ("npm run lint", &tsc, Some(2)),
            ("npm run docs", "docs ok", Some(0)),
            ("cargo fmt --all --check", "", Some(0)),
            ("cargo t --claims", "", Some(0)),
        ]));
        let v = Arc::new(Verifier::new(runner));
        let out = rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("ui/App.tsx", "h")])));
        let lint = out.checks.iter().find(|c| c.command == "npm run lint").unwrap();
        assert_eq!(lint.verdict, Verdict::Failed);
        assert_eq!(lint.failures.len(), output::SHOWN + 1, "20 failures and one line saying how many more");
        assert!(lint.failures[0].starts_with("ui/App.tsx:0:5: error TS2322"), "{:?}", lint.failures[0]);
        assert!(lint.failures.iter().all(|f| f.lines().count() <= output::FAILURE_LINES + 2));
        assert_eq!(out.failed(), 1);
        let text = out.text();
        assert!(text.contains("1 failed") && text.contains("FAILED npm run lint") && text.contains("10 more failures"), "{text}");
        // A failing check is run again next time, even on the same content.
        assert!(!v.passed.lock().unwrap().0.contains_key(&(dir.path().to_path_buf(), "npm run lint".to_string())));
    }

    #[test]
    fn a_newer_verify_of_the_same_folder_stops_the_running_one_and_ends_its_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = passing();
        runner.script.insert("cargo t --claims".into(), (String::new(), None));
        let dropped = runner.dropped.clone();
        let runner = Arc::new(runner);
        let v = Arc::new(Verifier::new(runner.clone()));
        rt().block_on(async {
            let first = tokio::spawn({
                let v = v.clone();
                let req = request(dir.path(), Level::Full, &[("a.rs", "h")]);
                async move { v.verify(req).await }
            });
            // The light checks end; the Cargo one never does.
            while !runner.started.lock().unwrap().iter().any(|s| s == "cargo t --claims") {
                tokio::task::yield_now().await;
            }
            assert!(v.busy(dir.path()));
            let mut second = request(dir.path(), Level::Fast, &[("docs/a.md", "h")]);
            second.commands = vec!["npm run docs".into()];
            let newer = v.verify(second).await;
            assert_eq!(newer.checks[0].verdict, Verdict::Passed);
            let older = first.await.unwrap();
            assert_eq!(older.reason.as_deref(), Some("reason.verify.stopped"));
            assert!(older.text().contains("stopped"));
        });
        // Every started process ended: the never-ending one was dropped with its run, and none is left.
        assert_eq!(dropped.load(Ordering::SeqCst), 5);
        assert!(!v.busy(dir.path()));
    }

    #[test]
    fn while_a_game_is_in_front_no_check_starts_and_a_running_one_is_capped_until_it_goes() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            // `cargo t --claims` runs until it is stopped; the docs check passes at once.
            let runner = Arc::new(fake(&[("npm run docs", "docs ok", Some(0)), ("cargo t --claims", "", None)]));
            let game = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let in_front = game.clone();
            let guard: Guard = Arc::new(move || in_front.load(Ordering::SeqCst));
            let v = Arc::new(Verifier::with_deadline(runner.clone(), Duration::from_secs(3600)).with_guard(guard));
            let mut request = request(dir.path(), Level::Fast, &[("docs/a.md", "h1")]);
            request.commands = vec!["npm run docs".into(), "cargo t --claims".into()];
            let run = tokio::spawn({
                let v = v.clone();
                async move { v.verify(request).await }
            });
            tokio::time::sleep(Duration::from_secs(60)).await;
            assert!(runner.started.lock().unwrap().is_empty(), "no check starts while the game is in front");
            game.store(false, Ordering::SeqCst);
            tokio::time::sleep(GUARD_EVERY * 2).await;
            assert_eq!(*runner.started.lock().unwrap(), ["npm run docs", "cargo t --claims"]);
            // The game comes back while the long check runs: it is capped, then freed when the game goes.
            game.store(true, Ordering::SeqCst);
            tokio::time::sleep(GUARD_EVERY * 2).await;
            game.store(false, Ordering::SeqCst);
            tokio::time::sleep(GUARD_EVERY * 2).await;
            assert_eq!(*runner.capped.lock().unwrap(), [true, false]);
            run.abort();
        });
    }

    #[test]
    fn a_check_past_its_deadline_is_ended_and_said_not_to_finish() {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = passing();
        runner.script.insert("cargo t --claims".into(), (String::new(), None));
        let dropped = runner.dropped.clone();
        let v = Arc::new(Verifier::with_deadline(Arc::new(runner), Duration::from_millis(50)));
        let out = rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("a.rs", "h")])));
        let claims = out.checks.iter().find(|c| c.command == "cargo t --claims").unwrap();
        assert_eq!(claims.verdict, Verdict::NotFinished);
        assert_eq!(out.failed(), 1);
        assert_eq!(dropped.load(Ordering::SeqCst), 3, "the process that never ended was ended");
    }

    #[test]
    fn no_checks_nothing_changed_and_a_command_that_needs_a_shell() {
        let dir = tempfile::tempdir().unwrap();
        let v = Arc::new(Verifier::new(Arc::new(passing())));
        let rt = rt();
        let none = rt.block_on(v.verify(Request { folder: dir.path().into(), ..Request::default() }));
        assert_eq!(none.reason.as_deref(), Some("reason.verify.noChecks"));
        assert!(none.text().contains("Proof commands"));
        let same = rt.block_on(v.verify(request(dir.path(), Level::Fast, &[])));
        assert_eq!(same.reason.as_deref(), Some("reason.verify.nothingChanged"));
        let mut piped = request(dir.path(), Level::Full, &[]);
        piped.commands = vec!["npm test | tee out.txt".into()];
        let out = rt.block_on(v.verify(piped));
        assert_eq!(out.checks[0].verdict, Verdict::NotFinished);
        assert!(out.checks[0].failures[0].contains("needs a shell"));
        // Outside git there is nothing to select from: fast runs every check.
        let mut outside = request(dir.path(), Level::Fast, &[]);
        outside.changed = None;
        let all = rt.block_on(v.verify(outside));
        assert!(all.checks.iter().all(|c| c.verdict == Verdict::Passed) && all.checks.len() == 4);
    }

    #[test]
    fn a_fresh_copy_installs_its_dependencies_first_and_the_pc_adds_its_own_environment() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".faber")).unwrap();
        std::fs::write(
            dir.path().join(LOCAL_FILE),
            r#"{"env":{"RUSTUP_HOME":"E:\\rust","PATH":"no","A=B":"x"},"path":["E:\\Apps\\zig","bad;one"]}"#,
        )
        .unwrap();
        let local = Local::read(dir.path());
        assert_eq!(local.env.keys().collect::<Vec<_>>(), ["RUSTUP_HOME"]);
        assert_eq!(local.path, ["E:\\Apps\\zig"]);
        let vars = local.vars();
        assert!(vars.iter().any(|(k, v)| k == "PATH" && v.starts_with("E:\\Apps\\zig")));
        let mut runner = passing();
        runner.script.insert("npm ci --no-audit --no-fund".into(), (String::new(), Some(0)));
        let runner = Arc::new(runner);
        let v = Arc::new(Verifier::new(runner.clone()));
        let mut req = request(dir.path(), Level::Fast, &[("ui/a.ts", "h")]);
        req.setup = vec!["npm ci".into()];
        req.local = local;
        rt().block_on(v.verify(req));
        assert_eq!(runner.started.lock().unwrap()[0], "npm ci --no-audit --no-fund");
        assert_eq!(Local::read(&dir.path().join("missing")), Local::default());
        // Only a check that runs on Node needs the install: a Rust change that only `cargo fmt` reads installs nothing.
        runner.started.lock().unwrap().clear();
        let mut rust = request(dir.path(), Level::Fast, &[("a.rs", "h2")]);
        rust.commands = vec!["cargo fmt --all --check".into()];
        rust.setup = vec!["npm ci".into()];
        rt().block_on(v.verify(rust));
        assert_eq!(*runner.started.lock().unwrap(), ["cargo fmt --all --check"]);
    }

    #[test]
    fn a_check_that_names_a_sign_in_file_never_starts() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(passing());
        let v = Arc::new(Verifier::new(runner.clone()));
        let mut req = request(dir.path(), Level::Full, &[]);
        req.commands = vec!["python -m json.tool C:\\Users\\Alice\\.codex\\auth.json".into(), "node ~/.claude/x.js".into()];
        let out = rt().block_on(v.verify(req));
        assert!(runner.started.lock().unwrap().is_empty());
        assert!(out.checks.iter().all(|c| c.verdict == Verdict::NotFinished && c.failures[0].contains("sign-in")), "{out:?}");
        assert!(!out.ok());
    }

    #[test]
    fn a_stop_while_git_is_still_asked_starts_no_check() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(passing());
        let v = Arc::new(Verifier::new(runner.clone()));
        let rt = rt();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let req = request(dir.path(), Level::Full, &[("a.rs", "h")]);
        let pending = v.start(rt.handle(), dir.path().to_path_buf(), "a", Level::Full, async move {
            let _ = rx.await;
            req
        });
        assert!(v.busy(dir.path()), "it counts as running while its request is made");
        assert!(!v.stop(dir.path(), "b"), "another's stop leaves it running");
        assert!(v.busy(dir.path()));
        assert!(v.stop(dir.path(), "a"));
        drop(tx);
        let out = rt.block_on(pending);
        assert_eq!(out.reason.as_deref(), Some("reason.verify.stopped"));
        assert!(!out.ok());
        assert!(runner.started.lock().unwrap().is_empty());
    }

    #[test]
    fn the_agent_asks_for_fast_or_full() {
        assert_eq!(level(&json!({})), Ok(Level::Fast));
        assert_eq!(level(&json!({"level": "full"})), Ok(Level::Full));
        assert!(level(&json!({"level": "everything"})).is_err());
        assert_eq!(mcp_tool()["name"], NAME);
        assert_eq!(codex_tool()["inputSchema"]["properties"]["level"]["enum"], json!(["fast", "full"]));
    }
}
