//! Faber's built-in verifier, V1 (R190; spec `docs/superpowers/specs/2026-10-17-built-in-verifier.md`): the project's own
//! checks, run on the PC from the conversation, so an agent learns in seconds whether its change works, before any push.
//!
//! - **The checks** are the project's "Checks" (R137): the commands under "Proof commands" in AGENTS.md or CLAUDE.md, or a
//!   detected default, as the person confirmed them in Settings > Projects when they did (`crate::project::checks::view`).
//! - **Fast** runs only the checks a change can affect, found from each command (`select`): a check whose files did not
//!   change is left out, and one that passed on exactly the same content is not run again (a content hash, never a time).
//!   **Full** runs every check.
//! - **Each check** is a process with no shell and no window, in its own kill-on-close job, below normal priority. A new
//!   verify of the same folder stops the running one, and one Cargo build runs at a time across every folder. Its output
//!   streams to a log in `.faber/verify/` (the last 20 kept); only the short failures of R155 stay in memory.
//! - **Idle** costs nothing: no process, no timer, no watcher. The agent gets a short text; the screen gets keys.
//!
//! The agent calls it as Faber's `verify` tool, like the helper and the browser (`crate::helper::mcp`).

mod admit;
mod changes;
mod journal;
mod run;
mod select;

pub use changes::{Changed, changed, own_log};
pub use journal::{Entry, Journal, Known};
pub use run::{GAME_CPU, GUARD_EVERY, Guard, KEPT_LOGS, Real, Runner, Spec, Started};
pub use select::{Scripts, Watch, heavy, scripts, watch};

use crate::project::proof::output;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
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
only the checks your changes can affect and skips one that already passed on the same content; `full` runs every check. You can use \
fast after a change or full to run every check. The whole output stays in the log it names.";

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
    /// The person's project folder, which the journal files a copy's results under; `None` is `folder` itself.
    pub project: Option<PathBuf>,
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
        match crate::project::checks::command_tokens(command) {
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
    Some(crate::project::lease::hash(text.as_bytes()))
}

/// The keys of checks that passed, per folder and command, so the same content is not checked twice. Bounded: it
/// forgets everything past `MAX_REMEMBERED`.
#[derive(Default)]
struct Passed(HashMap<(PathBuf, String), String>);
const MAX_REMEMBERED: usize = 512;

/// How many `cargo metadata` a fast verify asks, how long each may take and how much of its output is read.
const METADATA_MOST: usize = 4;
const METADATA_TIME: Duration = Duration::from_secs(60);
const METADATA_BYTES: u64 = 16 << 20;
const UNKNOWN_FOLDER_LIMIT: usize = 1_024;

/// The verifier: one per engine. It holds no process and no timer while idle.
pub struct Verifier {
    runner: Arc<dyn Runner>,
    deadline: Duration,
    /// Cargo builds across every folder, admitted by free memory (`admit`).
    heavy: admit::Admission,
    /// The free memory a build is admitted against.
    free: admit::Free,
    passed: Mutex<Passed>,
    /// Current and draining runs, with the folder and conversation that own them.
    running: Mutex<HashMap<u64, Running>>,
    /// None after saturation: every folder remains blocked without retaining an unbounded list.
    unknown: Mutex<Option<HashSet<PathBuf>>>,
    next: AtomicU64,
    clock: fn() -> u64,
    /// The game guard (`run::Guard`): none where the shell cannot tell.
    guard: Option<Guard>,
    /// Past results: failed first, cheapest first, flaky told (`journal`).
    journal: Journal,
}

struct Running {
    folder: PathBuf,
    owner: String,
    task: tokio::task::AbortHandle,
}

/// Every launched process keeps the run active until its separate cancellation worker confirms completion.
struct Confirming<'a> {
    runner: &'a dyn Runner,
    pending: Arc<Mutex<Vec<tokio::sync::watch::Receiver<bool>>>>,
}

impl Runner for Confirming<'_> {
    fn start(&self, spec: &Spec) -> std::io::Result<Started> {
        let started = self.runner.start(spec)?;
        if let Some(confirmed) = &started.confirmed {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).push(confirmed.clone());
        }
        Ok(started)
    }
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
            heavy: admit::Admission::default(),
            // No reading: one build at a time, unless a test gives one.
            free: Arc::new(|| None),
            passed: Mutex::new(Passed::default()),
            running: Mutex::new(HashMap::new()),
            unknown: Mutex::new(Some(HashSet::new())),
            next: AtomicU64::new(0),
            clock: now_millis,
            guard: None,
            journal: Journal::new(None),
        }
    }

    /// The real verifier the shell gives the engine, with its game guard and its journal (a file in the data folder).
    pub fn real(guard: Option<Guard>, journal: Option<PathBuf>) -> Arc<Verifier> {
        Arc::new(Verifier { guard, journal: Journal::new(journal), free: Arc::new(admit::free_memory), ..Verifier::new(Arc::new(Real)) })
    }

    /// The same verifier with a game guard (tests).
    pub fn with_guard(self, guard: Guard) -> Verifier {
        Verifier { guard: Some(guard), ..self }
    }

    /// Whether a game is in front now, so a check nobody is waiting for does not even ask to start (standing goals).
    pub fn game_in_front(&self) -> bool {
        self.guard.as_ref().is_some_and(|in_front| in_front())
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
        let pending = Arc::new(Mutex::new(Vec::new()));
        let confirmations = pending.clone();
        let task = runtime.spawn(async move {
            let runner = Confirming { runner: &*me.runner, pending: confirmations };
            me.run(request.await, &runner).await
        });
        let run = Running { folder: folder.clone(), owner: owner.to_string(), task: task.abort_handle() };
        {
            let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            for older in running.values().filter(|r| r.folder == folder) {
                older.task.abort();
            }
            running.insert(id, run);
        }
        let me = self.clone();
        async move {
            let outcome = task.await.unwrap_or_else(|_| Outcome::nothing(level, "reason.verify.stopped"));
            let confirmations = std::mem::take(&mut *pending.lock().unwrap_or_else(|e| e.into_inner()));
            for mut confirmed in confirmations {
                if confirmed.wait_for(|done| *done).await.is_err() {
                    // A failed confirmation never opens restore admission just because its worker ended.
                    me.remember_unknown(&folder);
                }
            }
            me.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            outcome
        }
    }

    /// Stops the verify of `folder` that `owner` started, if one runs: its processes end with their job. Another's run of
    /// the same folder (two conversations of one project without a copy share it) goes on. True when one was stopped.
    pub fn stop(&self, folder: &Path, owner: &str) -> bool {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let mut stopped = false;
        for r in running.values().filter(|r| r.folder == folder && r.owner == owner) {
            r.task.abort();
            stopped = true;
        }
        stopped
    }

    /// Whether no verify runs anywhere.
    pub fn idle(&self) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Only checks whose folders overlap can write into a restore's range.
    pub fn active_in(&self, folder: &Path) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|r| r.folder.starts_with(folder) || folder.starts_with(&r.folder))
            || self.unknown_in(folder)
    }

    /// A native check without process-tree completion proof remains a possible writer.
    pub fn unknown_in(&self, folder: &Path) -> bool {
        self.unknown
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_none_or(|folders| folders.iter().any(|p| p.starts_with(folder) || folder.starts_with(p)))
    }

    fn remember_unknown(&self, folder: &Path) {
        let mut unknown = self.unknown.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(folders) = unknown.as_mut() {
            if folders.len() >= UNKNOWN_FOLDER_LIMIT && !folders.contains(folder) {
                *unknown = None;
            } else {
                folders.insert(folder.into());
            }
        }
    }

    /// The workspace graph of the package at `manifest` (`cargo metadata --no-deps`), when Cargo gives it within
    /// `METADATA_TIME`.
    async fn metadata(&self, request: &Request, manifest: &str, runner: &dyn Runner) -> Option<String> {
        use tokio::io::AsyncReadExt;
        let args = ["metadata", "--format-version", "1", "--no-deps", "--manifest-path", manifest];
        let spec = Spec {
            program: "cargo".into(),
            args: args.map(String::from).to_vec(),
            cwd: request.folder.clone(),
            env: request.local.vars(),
            memory: None,
        };
        let started = runner.start(&spec).ok()?;
        let (mut stdout, mut stderr) = (started.stdout.take(METADATA_BYTES), started.stderr);
        let mut out = Vec::new();
        let read = async {
            let mut sink = tokio::io::sink();
            let (read, _) = tokio::join!(stdout.read_to_end(&mut out), tokio::io::copy(&mut stderr, &mut sink));
            read.ok()?;
            started.exit.await
        };
        let code = tokio::time::timeout(METADATA_TIME, read).await.ok()??;
        (code == 0).then(|| String::from_utf8_lossy(&out).into_owned())
    }

    /// Whether a verify of `folder` runs now.
    pub fn busy(&self, folder: &Path) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).values().any(|r| r.folder == folder)
    }

    async fn run(&self, request: Request, runner: &dyn Runner) -> Outcome {
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
            let mut manifests = select::rust_manifests(&request.folder, &changed.files);
            let touched = manifests.clone();
            // ponytail: one `cargo metadata` per touched package, the first `METADATA_MOST`; a change across more
            // workspaces than that checks the rest's dependents in full.
            // Asking Cargo starts a process too: never while a game is in front.
            if !touched.is_empty() {
                self.game_over().await;
            }
            for (manifest, _) in touched.iter().take(METADATA_MOST) {
                if let Some(json) = self.metadata(&request, manifest, runner).await {
                    manifests.extend(select::rust_dependents(&request.folder, &json, &touched));
                }
            }
            manifests.sort();
            manifests.dedup();
            planned.extend(rust_checks(manifests, &request.commands));
        }
        let folder_key = request.folder.clone();
        let project = request.project.as_ref().unwrap_or(&request.folder).to_string_lossy().into_owned();
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
        // Among each, what failed last time first, then the cheapest by the journal.
        let known: HashMap<String, Known> =
            todo.iter().map(|(c, _)| (c.command.clone(), self.journal.known(&project, &c.command))).collect();
        todo.sort_by_key(|(check, _)| {
            let known = known[&check.command];
            (check.heavy, !known.failed, known.cost.unwrap_or(u64::MAX))
        });
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
                memory: None,
            };
            log.line("== npm ci (the dependencies were not installed in this folder) ==");
            let ran = run::check(runner, &spec, &mut log, self.deadline, self.guard.as_ref()).await;
            if ran.exit != Some(0) {
                results.push(result("npm ci", &ran, &root));
            }
        }
        for (check, key) in todo {
            // The guard is read again once the Cargo build's turn comes: a game may have started while it waited.
            let _turn = loop {
                self.game_over().await;
                let turn = if check.heavy { Some(self.heavy.admit(known[&check.command].peak, &self.free).await) } else { None };
                if !self.guard.as_ref().is_some_and(|in_front| in_front()) {
                    break turn;
                }
            };
            log.line(&format!("== {} ==", check.command));
            let memory = admit::cap(known[&check.command].peak, (self.free)());
            let spec = Spec {
                program: check.tokens[0].clone(),
                args: check.tokens[1..].to_vec(),
                cwd: request.folder.clone(),
                env: vars.clone(),
                memory,
            };
            // A check the journal knows as flaky (it passed and failed on the same content) that fails is run once more on
            // its own: passing then, it turns nothing red (R190 V3). Any other failure stands at once.
            let mut retried = false;
            let mut out = loop {
                let ran = run::check(runner, &spec, &mut log, self.deadline, self.guard.as_ref()).await;
                log.line(&format!("== {}: exit {:?} after {} ms ==", check.command, ran.exit, ran.took.as_millis()));
                let mut out = result(&check.command, &ran, &root);
                let capped = out.verdict != Verdict::Passed && at_cap(ran.peak, memory);
                if capped {
                    out.summary.push_str(&format!(
                        "; it reached its memory cap of {} MB, half again the most its past runs held or what was free, whichever was more, at least \
                         1 GiB: it took far more memory than before; if it truly needs that much now, run it when more memory is free",
                        memory.unwrap_or_default() >> 20
                    ));
                }
                if ran.refused.is_some() {
                    break out;
                }
                let ok = out.verdict == Verdict::Passed;
                let entry = Entry {
                    project: project.clone(),
                    command: check.command.clone(),
                    key: key.clone(),
                    ok,
                    millis: out.millis,
                    at: (self.clock)(),
                    // A run stopped at its cap tells only the cap (Windows adds the refused request to the peak): kept, it
                    // would raise the next cap and the admission past what the check ever really held.
                    peak: if capped { None } else { ran.peak },
                };
                let flaky = self.journal.add(entry);
                if out.verdict == Verdict::Failed && known[&check.command].flaky && !retried {
                    retried = true;
                    log.line(&format!("== {}: known as flaky, so it runs once more on its own ==", check.command));
                    // The rerun is a new start: it waits for a game that came while the first run worked.
                    self.game_over().await;
                    continue;
                }
                if flaky && !ok && !retried {
                    out.summary.push_str("; flaky: it passed and failed before on the same content");
                }
                break out;
            };
            if retried {
                if out.verdict == Verdict::Passed {
                    out.summary = format!("{}; flaky: it failed once, then passed when run again on its own", out.summary);
                } else if out.verdict == Verdict::Failed {
                    out.summary.push_str("; it failed again when run on its own, so this failure is real");
                }
            }
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
fn rust_checks(manifests: Vec<(String, bool)>, declared: &[String]) -> Vec<Planned> {
    manifests
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

/// Whether a check's job held about as much memory as its cap allows, so a failure may be the cap's (Windows refuses the
/// allocations past it and says nothing else).
fn at_cap(peak: Option<u64>, cap: Option<u64>) -> bool {
    peak.zip(cap).is_some_and(|(peak, cap)| peak.saturating_add(64 << 20) >= cap)
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
        /// What a program does the first time only, before the table: a check that fails once.
        once: Mutex<HashMap<String, (String, Option<i32>)>>,
        dropped: Arc<AtomicUsize>,
        /// What the game guard asked of each check's processor share, in order.
        capped: Arc<Mutex<Vec<bool>>>,
        /// The memory cap each check was started with, in order.
        memories: Mutex<Vec<Option<u64>>>,
        /// The peak every check tells, when set.
        peak: Mutex<Option<u64>>,
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
            self.memories.lock().unwrap().push(spec.memory);
            let peak = self.peak.lock().unwrap().map(|p| Arc::new(std::sync::atomic::AtomicU64::new(p)));
            let first = self.once.lock().unwrap().remove(&line);
            let (out, code) = first.or_else(|| self.script.get(&line).cloned()).ok_or_else(|| std::io::Error::other("unknown program"))?;
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
            let throttle: Box<dyn Fn(bool) -> bool + Send + Sync> = Box::new(move |game| {
                capped.lock().unwrap().push(game);
                true
            });
            Ok(Started {
                confirmed: None,
                stdout: Box::new(std::io::Cursor::new(out.into_bytes())),
                stderr: Box::new(tokio::io::empty()),
                exit,
                throttle: Some(throttle),
                peak,
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
        // A package that depends on the touched one is compiled too: Cargo's own graph says which.
        std::fs::create_dir_all(dir.path().join("src-tauri")).unwrap();
        std::fs::write(dir.path().join("src-tauri/Cargo.toml"), "[package]\nname = \"faber\"\n").unwrap();
        let at = |p: &str| dir.path().join(p).to_string_lossy().into_owned();
        let metadata = serde_json::json!({"packages": [
            {"manifest_path": at("engine/Cargo.toml"), "dependencies": []},
            {"manifest_path": at("src-tauri/Cargo.toml"), "dependencies": [{"name": "faber-engine", "path": at("engine")}]},
        ]});
        let mut runner = passing();
        let asked = "cargo metadata --format-version 1 --no-deps --manifest-path engine/Cargo.toml";
        runner.script.insert(asked.into(), (metadata.to_string(), Some(0)));
        let runner = Arc::new(runner);
        let v = Arc::new(Verifier::new(runner.clone()));
        let fast = rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("engine/src/a.rs", "h3")])));
        let dependent = "cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings";
        assert!(fast.checks.iter().any(|c| c.command == clippy) && fast.checks.iter().any(|c| c.command == dependent), "{fast:?}");
        runner.started.lock().unwrap().clear();
        rt().block_on(v.verify(request(dir.path(), Level::Full, &[("engine/src/a.rs", "h1")])));
        assert!(!runner.started.lock().unwrap().iter().any(|c| c.contains("clippy")), "full runs only the project's list");
        runner.started.lock().unwrap().clear();
        rt().block_on(v.verify(request(dir.path(), Level::Fast, &[("ui/App.tsx", "h2")])));
        assert!(!runner.started.lock().unwrap().iter().any(|c| c.contains("clippy")), "no Rust change, no Rust check added");
    }

    #[test]
    fn what_failed_last_time_runs_first_and_a_flaky_failure_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(fake(&[("npm run lint", "", Some(0)), ("npm run docs", "docs: a broken link\n", Some(1))]));
        let v = Arc::new(Verifier::new(runner.clone()));
        let run = |hash: &str| {
            let mut request = request(dir.path(), Level::Full, &[("docs/a.md", hash)]);
            request.commands = vec!["npm run lint".into(), "npm run docs".into()];
            rt().block_on(v.verify(request))
        };
        run("h1");
        assert_eq!(*runner.started.lock().unwrap(), ["npm run lint", "npm run docs"], "the declared order at first");
        runner.started.lock().unwrap().clear();
        let second = run("h2");
        assert_eq!(*runner.started.lock().unwrap(), ["npm run docs", "npm run lint"], "the check that failed goes first");
        assert_eq!(second.checks[0].command, "npm run lint", "the screen still reads the declared order");
        // The same content passed once (a journal entry the fake cannot make): its next failure there reads as flaky.
        let key = v.journal.with_last_key(&dir.path().to_string_lossy(), "npm run docs");
        assert!(key.is_some(), "the failure was journaled with its content");
        v.journal.add(Entry {
            project: dir.path().to_string_lossy().into_owned(),
            command: "npm run docs".into(),
            key,
            ok: true,
            millis: 5,
            at: 0,
            peak: None,
        });
        assert!(v.journal.known(&dir.path().to_string_lossy(), "npm run docs").flaky);
        runner.started.lock().unwrap().clear();
        let third = run("h2");
        let docs = third.checks.iter().find(|c| c.command == "npm run docs").unwrap();
        assert_eq!(docs.verdict, Verdict::Failed);
        assert!(docs.summary.contains("failed again when run on its own"), "{}", docs.summary);
        assert_eq!(
            *runner.started.lock().unwrap(),
            ["npm run lint", "npm run docs", "npm run docs"],
            "a known flaky failure runs once more"
        );
    }

    #[test]
    fn a_check_is_capped_from_its_kept_peak_and_a_run_stopped_at_the_cap_keeps_none() {
        const GB: u64 = 1 << 30;
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(fake(&[("npm run docs", "docs ok", Some(0))]));
        runner.once.lock().unwrap().insert("npm run docs".into(), ("out of memory".into(), Some(1)));
        *runner.peak.lock().unwrap() = Some(3 * GB + (1 << 20));
        let v = Arc::new(Verifier::new(runner.clone()));
        let project = dir.path().to_string_lossy().into_owned();
        let kept =
            Entry { project: project.clone(), command: "npm run docs".into(), key: None, ok: true, millis: 5, at: 0, peak: Some(2 * GB) };
        v.journal.add(kept);
        let mut request = request(dir.path(), Level::Full, &[("docs/a.md", "h1")]);
        request.commands = vec!["npm run docs".into()];
        let report = rt().block_on(v.verify(request.clone()));
        assert_eq!(*runner.memories.lock().unwrap(), [Some(3 * GB)], "half again the 2 GB it held, with no free reading");
        assert!(report.checks[0].summary.contains("memory cap of 3072 MB"), "{}", report.checks[0].summary);
        assert_eq!(v.journal.known(&project, "npm run docs").peak, Some(2 * GB), "the capped run's peak is not kept");
        rt().block_on(v.verify(request));
        assert_eq!(runner.memories.lock().unwrap()[1], Some(3 * GB), "so the next cap stays where it was");
    }

    #[test]
    fn a_known_flaky_check_that_fails_once_and_then_passes_turns_nothing_red() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(fake(&[("npm run docs", "docs ok", Some(0))]));
        runner.once.lock().unwrap().insert(
            "npm run docs".into(),
            (
                "docs: a broken link
"
                .into(),
                Some(1),
            ),
        );
        let v = Arc::new(Verifier::new(runner.clone()));
        let project = dir.path().to_string_lossy().into_owned();
        for ok in [true, false] {
            v.journal.add(Entry {
                project: project.clone(),
                command: "npm run docs".into(),
                key: Some("k".into()),
                ok,
                millis: 5,
                at: 0,
                peak: None,
            });
        }
        let mut request = request(dir.path(), Level::Full, &[("docs/a.md", "h1")]);
        request.commands = vec!["npm run docs".into()];
        let report = rt().block_on(v.verify(request));
        assert_eq!(*runner.started.lock().unwrap(), ["npm run docs", "npm run docs"]);
        assert!(report.ok(), "{report:?}");
        assert!(report.checks[0].summary.contains("flaky: it failed once, then passed"), "{}", report.checks[0].summary);
        // Without the journal's word, the same single failure stands.
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(fake(&[("npm run docs", "docs ok", Some(0))]));
        runner.once.lock().unwrap().insert(
            "npm run docs".into(),
            (
                "docs: a broken link
"
                .into(),
                Some(1),
            ),
        );
        let v = Arc::new(Verifier::new(runner.clone()));
        let mut request = self::request(dir.path(), Level::Full, &[("docs/a.md", "h1")]);
        request.commands = vec!["npm run docs".into()];
        assert!(!rt().block_on(v.verify(request)).ok());
        assert_eq!(runner.started.lock().unwrap().len(), 1);
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
    fn while_a_game_is_in_front_cargo_is_not_even_asked_for_its_graph() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join(".git")).unwrap();
            std::fs::create_dir_all(dir.path().join("engine")).unwrap();
            std::fs::write(dir.path().join("engine/Cargo.toml"), "[package]\nname = \"e\"\n").unwrap();
            let runner = Arc::new(passing());
            let v = Arc::new(Verifier::new(runner.clone()).with_guard(Arc::new(|| true)));
            let run = tokio::spawn({
                let v = v.clone();
                let request = request(dir.path(), Level::Fast, &[("engine/src/a.rs", "h1")]);
                async move { v.verify(request).await }
            });
            tokio::time::sleep(Duration::from_secs(60)).await;
            assert!(runner.started.lock().unwrap().is_empty(), "{:?}", runner.started.lock().unwrap());
            run.abort();
        });
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
    fn the_rerun_of_a_known_flaky_check_waits_for_a_game_that_came_meanwhile() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let runner = Arc::new(fake(&[("npm run docs", "docs ok", Some(0))]));
            runner.once.lock().unwrap().insert(
                "npm run docs".into(),
                (
                    "docs: a broken link
"
                    .into(),
                    Some(1),
                ),
            );
            // The game comes to the front as soon as the first run starts.
            let game = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let (in_front, seen) = (game.clone(), runner.clone());
            let guard: Guard = Arc::new(move || in_front.load(Ordering::SeqCst) && !seen.started.lock().unwrap().is_empty());
            let v = Arc::new(Verifier::with_deadline(runner.clone(), Duration::from_secs(3600)).with_guard(guard));
            let project = dir.path().to_string_lossy().into_owned();
            for ok in [true, false] {
                v.journal.add(Entry {
                    project: project.clone(),
                    command: "npm run docs".into(),
                    key: Some("k".into()),
                    ok,
                    millis: 5,
                    at: 0,
                    peak: None,
                });
            }
            let mut request = request(dir.path(), Level::Full, &[("docs/a.md", "h1")]);
            request.commands = vec!["npm run docs".into()];
            let run = tokio::spawn({
                let v = v.clone();
                async move { v.verify(request).await }
            });
            tokio::time::sleep(Duration::from_secs(60)).await;
            assert_eq!(runner.started.lock().unwrap().len(), 1, "the rerun waits while the game is in front");
            game.store(false, Ordering::SeqCst);
            let report = run.await.unwrap();
            assert_eq!(runner.started.lock().unwrap().len(), 2);
            assert!(report.ok(), "{report:?}");
        });
    }

    #[test]
    fn time_held_for_a_game_does_not_count_against_a_check_s_deadline() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let runner = Arc::new(fake(&[("cargo t --claims", "", None)]));
            let game = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let in_front = game.clone();
            let guard: Guard = Arc::new(move || in_front.load(Ordering::SeqCst));
            let v = Arc::new(Verifier::with_deadline(runner.clone(), Duration::from_secs(20)).with_guard(guard));
            let mut request = request(dir.path(), Level::Full, &[("docs/a.md", "h1")]);
            request.commands = vec!["cargo t --claims".into()];
            let run = tokio::spawn({
                let v = v.clone();
                async move { v.verify(request).await }
            });
            tokio::time::sleep(Duration::from_secs(1)).await;
            game.store(true, Ordering::SeqCst);
            // Two minutes capped, six times the deadline: the check still runs.
            tokio::time::sleep(Duration::from_secs(120)).await;
            assert!(!run.is_finished(), "a check held for a game is not out of time");
            game.store(false, Ordering::SeqCst);
            let outcome = tokio::time::timeout(Duration::from_secs(60), run).await.unwrap().unwrap();
            assert_eq!(outcome.checks[0].verdict, Verdict::NotFinished, "the deadline counts again once the game goes");
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

    #[tokio::test]
    async fn rewind_stopped_and_superseded_checks_wait_for_each_process_confirmation() {
        struct Draining {
            confirmations: Mutex<std::collections::VecDeque<tokio::sync::watch::Receiver<bool>>>,
            started: tokio::sync::mpsc::UnboundedSender<()>,
        }
        impl Runner for Draining {
            fn start(&self, _: &Spec) -> std::io::Result<Started> {
                let confirmed = self.confirmations.lock().unwrap().pop_front().unwrap();
                let _ = self.started.send(());
                Ok(Started {
                    stdout: Box::new(tokio::io::empty()),
                    stderr: Box::new(tokio::io::empty()),
                    exit: Box::pin(std::future::pending()),
                    confirmed: Some(confirmed),
                    throttle: None,
                    peak: None,
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let (first, first_confirmation) = tokio::sync::watch::channel(false);
        let (second, second_confirmation) = tokio::sync::watch::channel(false);
        let (unknown, unknown_confirmation) = tokio::sync::watch::channel(false);
        drop(unknown);
        let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
        let verifier = Arc::new(Verifier::new(Arc::new(Draining {
            confirmations: Mutex::new([first_confirmation, second_confirmation, unknown_confirmation].into()),
            started,
        })));
        let request = || Request {
            folder: dir.path().into(),
            level: Some(Level::Full),
            commands: vec!["node check.js".into()],
            ..Request::default()
        };
        let first_run =
            verifier.start(&tokio::runtime::Handle::current(), dir.path().into(), "a", Level::Full, std::future::ready(request()));
        let first_run = tokio::spawn(first_run);
        starts.recv().await.unwrap();
        assert!(verifier.stop(dir.path(), "a"));
        assert!(verifier.active_in(&dir.path().join("nested")), "Stop cannot report idle before completion");
        let second_run =
            verifier.start(&tokio::runtime::Handle::current(), dir.path().into(), "b", Level::Full, std::future::ready(request()));
        let second_run = tokio::spawn(second_run);
        starts.recv().await.unwrap();
        assert!(verifier.stop(dir.path(), "b"));
        second.send(true).unwrap();
        second_run.await.unwrap();
        assert!(verifier.active_in(dir.path()), "a newer completed check cannot hide an older draining one");
        assert!(!first_run.is_finished());
        first.send(true).unwrap();
        first_run.await.unwrap();
        assert!(verifier.idle());
        assert!(!verifier.active_in(dir.path()));
        let unknown_run =
            verifier.start(&tokio::runtime::Handle::current(), dir.path().into(), "c", Level::Full, std::future::ready(request()));
        let unknown_run = tokio::spawn(unknown_run);
        starts.recv().await.unwrap();
        assert!(verifier.stop(dir.path(), "c"));
        unknown_run.await.unwrap();
        assert!(verifier.idle(), "unknown tree ownership does not hang the check's outcome");
        assert!(verifier.active_in(dir.path()), "failed native confirmation must never open restore admission");
        assert!(verifier.unknown_in(&dir.path().join("nested")));
        assert!(!verifier.active_in(&dir.path().with_file_name("unrelated")));
    }

    #[test]
    fn rewind_unknown_check_folder_saturation_blocks_all_ranges_instead_of_forgetting_writers() {
        let verifier = Verifier::new(Arc::new(passing()));
        for at in 0..=UNKNOWN_FOLDER_LIMIT {
            verifier.remember_unknown(&PathBuf::from(format!("unknown-{at}")));
        }
        assert!(verifier.unknown.lock().unwrap().is_none(), "saturation releases the bounded list");
        assert!(verifier.active_in(Path::new("elsewhere")), "an unrelated range stays blocked after saturation");
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
