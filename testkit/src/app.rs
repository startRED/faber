//! Launching the test build of Faber inside a sandbox, keeping track of every process it starts, and the audit that
//! decides, after a scenario, whether it left anything behind.

use crate::contain::Owned;
use crate::page::Page;
use crate::proctree::{ProcRow, descendants, escaped, live_tree, same_process, snapshot};
use crate::sandbox::{Launch, Sandbox, child_env, contained, remove_tree};
use crate::server::{ASSET, Server, zip_of};
use serde_json::Value;
use std::io::Read;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the hidden app gets to open its window (or to write its checks report).
pub fn startup() -> Duration {
    startup_timeout(std::env::var("GITHUB_ACTIONS").ok().as_deref())
}

fn startup_timeout(actions: Option<&str>) -> Duration {
    Duration::from_secs(if actions == Some("true") { 90 } else { 30 })
}

/// Where everything the runner needs lives.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The repository.
    pub root: PathBuf,
    /// The test build of Faber.
    pub app: PathBuf,
    /// The fake CLI, built next to the runner.
    pub fake_cli: PathBuf,
    pub fixtures: PathBuf,
    pub shots: PathBuf,
    pub target: PathBuf,
}

fn test_target(root: &Path, configured: Option<&std::ffi::OsStr>) -> PathBuf {
    root.join(configured.unwrap_or_else(|| std::ffi::OsStr::new("target"))).join("faber-test")
}

impl Paths {
    pub fn discover() -> Result<Paths, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or("the testkit has no parent folder")?.to_path_buf();
        let exe = std::env::current_exe().map_err(|e| format!("cannot find the runner's own path: {e}"))?;
        let bin = exe.parent().ok_or("the runner has no folder")?.to_path_buf();
        let suffix = std::env::consts::EXE_SUFFIX;
        let target = test_target(&root, std::env::var_os("CARGO_TARGET_DIR").as_deref());
        Ok(Paths {
            app: target.join(format!("debug/faber{suffix}")),
            fake_cli: bin.join(format!("fake-cli{suffix}")),
            fixtures: root.join("tests/fixtures"),
            shots: target.join("shots"),
            target,
            root,
        })
    }
}

/// A process id and its creation time in microseconds since 1970.
pub type ProcessIdentity = (u32, u64);

/// A command the fake CLI was asked to run, as it wrote it down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub pid: u32,
    pub born: u64,
    pub cwd: String,
    pub vendor: String,
    pub args: Vec<String>,
    pub targets: Vec<String>,
}

impl Recorded {
    pub fn line(&self) -> String {
        self.args.join(" ")
    }
}

/// Reads `commands.jsonl`: the commands, and the ids of those that wrote down that they ended. No file means no CLI ran.
/// A file that cannot be read, or a line that is not a complete record, is an error: an audit that skips what it cannot
/// read would pass a CLI that ran outside the sandbox.
pub fn read_records(state: &Path) -> Result<(Vec<Recorded>, Vec<ProcessIdentity>), String> {
    let path = state.join("commands.jsonl");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let (mut commands, mut exited) = (Vec::new(), Vec::new());
    for (n, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
        let bad = |what: &str| format!("commands.jsonl line {}: {what}", n + 1);
        let value: Value = serde_json::from_str(line).map_err(|e| bad(&e.to_string()))?;
        let born = value["born"].as_u64().filter(|n| *n > 0).ok_or_else(|| bad("no creation time"))?;
        if let Some(pid) = value["exited"].as_u64() {
            exited.push((pid as u32, born));
            continue;
        }
        let text = |name: &str| value[name].as_str().map(str::to_string).ok_or_else(|| bad(&format!("no {name}")));
        let strings = |name: &str| -> Result<Vec<String>, String> {
            let list = value[name].as_array().ok_or_else(|| bad(&format!("no {name}")))?;
            list.iter().map(|s| s.as_str().map(str::to_string).ok_or_else(|| bad(&format!("{name} holds a non-text value")))).collect()
        };
        commands.push(Recorded {
            pid: value["pid"].as_u64().ok_or_else(|| bad("no pid"))? as u32,
            born,
            cwd: text("cwd")?,
            vendor: text("vendor")?,
            args: strings("args")?,
            targets: strings("targets")?,
        });
    }
    Ok((commands, exited))
}

/// The processes that should be gone but are not, as a pure function over the process list. Audit only: nothing is ended
/// because of what this finds.
///
/// - `seen`: every process of the app's tree recorded while it ran (id and creation time).
/// - `roots`: the app's own processes. Whatever still names one as its parent, and was created after it, counts too,
///   unless the root's id now belongs to a different process.
/// - `running`: identities of fake CLIs that never wrote that they ended. A process must match its recorded creation
///   time and be born after `not_before`; a later owner of its PID cannot be one of ours.
pub fn leaked(rows: &[ProcRow], seen: &[ProcRow], roots: &[ProcRow], running: &[ProcessIdentity], not_before: u64) -> Vec<ProcRow> {
    let mut all = rows.to_vec();
    let mut trees: Vec<Vec<ProcRow>> = Vec::new();
    for root in roots {
        match rows.iter().find(|r| r.pid == root.pid) {
            Some(now) if !same_process(now, std::slice::from_ref(root)) => continue,
            Some(_) => {}
            None => all.push(root.clone()),
        }
        trees.push(descendants(&all, root.pid));
    }
    rows.iter()
        .filter(|r| {
            same_process(r, seen)
                || trees.iter().any(|tree| same_process(r, tree))
                || (running.contains(&(r.pid, r.born)) && r.born >= not_before)
        })
        .cloned()
        .collect()
}

/// Everything a run owns that must be ended if the runner is interrupted: the apps (each in its own containment) and
/// the folders. Ending is done through `Owned`, so it also holds when this happens on another thread.
#[derive(Default)]
struct Registry {
    owned: Vec<Arc<Owned>>,
    folders: Vec<PathBuf>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry { owned: Vec::new(), folders: Vec::new() });

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called when the runner is interrupted (Ctrl+C): ends every app it started and removes every sandbox. On Windows the
/// jobs end the apps by themselves if the runner dies without getting here.
pub fn emergency_cleanup() {
    cleanup_registry(&REGISTRY);
}

fn cleanup_registry(registry: &Mutex<Registry>) {
    let (owned, folders) = {
        let mut r = registry.lock().unwrap_or_else(|e| e.into_inner());
        (std::mem::take(&mut r.owned), std::mem::take(&mut r.folders))
    };
    for app in owned {
        app.terminate();
    }
    for folder in folders {
        for _ in 0..5 {
            if remove_tree(&folder).is_ok() || !folder.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

struct Live {
    owned: Arc<Owned>,
    pid: u32,
    port: Option<u16>,
    seen: Vec<ProcRow>,
    /// Processes of the app's tree that the job did not hold when they were looked at.
    escaped: Vec<ProcRow>,
    stderr: Arc<Mutex<String>>,
    stdout: Arc<Mutex<String>>,
}

fn capture(pipe: Option<impl Read + Send + 'static>) -> Arc<Mutex<String>> {
    let output = Arc::new(Mutex::new(String::new()));
    if let Some(mut pipe) = pipe {
        let sink = output.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let mut text = sink.lock().unwrap_or_else(|e| e.into_inner());
                if text.len() < 64_000 {
                    text.push_str(&String::from_utf8_lossy(&chunk[..n]));
                }
            }
        });
    }
    output
}

/// A launched app that a scenario drives: which one it is, and its page.
pub struct Opened {
    pub id: usize,
    pub pid: u32,
    pub page: Page,
}

/// One scenario's world: its sandbox, the fake release server if it needs one, and every app it launched.
pub struct Ctx {
    pub paths: Paths,
    pub sb: Sandbox,
    pub server: Option<Server>,
    /// The bytes the fake release server hands out as the downloaded program.
    pub payload: Vec<u8>,
    /// What the scenario wants to say about a pass, such as a memory measurement.
    pub notes: Vec<String>,
    live: Vec<Live>,
    real_env: Vec<(String, String)>,
    real_user_path: Option<String>,
    finished: bool,
}

fn env_pairs() -> Vec<(String, String)> {
    std::env::vars().collect()
}

/// The user's own `PATH` as the system stores it. Only Windows has a per-user value that an installer could change.
#[cfg(windows)]
fn user_path() -> Result<Option<String>, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment").map_err(|e| format!("cannot read the user environment: {e}"))?;
    match key.get_value::<String, _>("Path") {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read the user PATH: {e}")),
    }
}

#[cfg(not(windows))]
fn user_path() -> Result<Option<String>, String> {
    Ok(None)
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| format!("no free port: {e}"))?;
    listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

impl Ctx {
    pub fn new(paths: &Paths) -> Result<Ctx, String> {
        let sb = Sandbox::new().map_err(|e| format!("cannot create the sandbox: {e}"))?;
        registry().folders.push(sb.root.clone());
        Ok(Ctx {
            paths: paths.clone(),
            sb,
            server: None,
            payload: Vec::new(),
            notes: Vec::new(),
            live: Vec::new(),
            real_env: env_pairs(),
            real_user_path: user_path()?,
            finished: false,
        })
    }

    /// Starts the fake release server. The program it serves is a copy of the fake CLI, zipped like the real asset.
    pub fn serve_release(&mut self) -> Result<(), String> {
        let payload = std::fs::read(&self.paths.fake_cli).map_err(|e| format!("{}: {e}", self.paths.fake_cli.display()))?;
        let inner = ASSET.trim_end_matches(".zip");
        let archive = zip_of(inner, &payload).map_err(|e| e.to_string())?;
        self.server = Some(Server::start(archive).map_err(|e| format!("cannot start the release server: {e}"))?);
        self.payload = payload;
        Ok(())
    }

    /// Writes `settings.json` in the data folder, unless a scenario already put one there.
    pub fn write_settings(&self, settings: &Value) -> Result<(), String> {
        let path = self.sb.data.join("settings.json");
        if path.exists() {
            return Ok(());
        }
        std::fs::write(&path, settings.to_string()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Starts the app with the sandbox environment and returns its id. Nothing is waited for. The app starts inside its
    /// containment (see `contain`), and the runner can end it from then on, even if it is interrupted.
    pub fn spawn(&mut self, mut launch: Launch) -> Result<usize, String> {
        if launch.setup_source.is_none() {
            launch.setup_source = self.server.as_ref().map(|s| s.origin.clone());
        }
        let env = child_env(&env_pairs(), &self.sb, &launch, &self.paths.fake_cli);
        let (owned, stdout, stderr) = Owned::start(&self.paths.app, &launch.args, &env, &self.sb.sandbox, &self.paths.fake_cli)
            .map_err(|e| format!("cannot start {}: {e}", self.paths.app.display()))?;
        let owned = Arc::new(owned);
        registry().owned.push(owned.clone());
        let (stdout, stderr) = (capture(stdout), capture(stderr));
        let pid = owned.pid();
        self.live.push(Live { owned, pid, port: launch.cdp_port, seen: Vec::new(), escaped: Vec::new(), stderr, stdout });
        let id = self.live.len() - 1;
        self.record(id)?;
        Ok(id)
    }

    /// Launches the app with its DevTools port and waits for its window.
    pub fn launch(&mut self, mut launch: Launch) -> Result<Opened, String> {
        let port = free_port()?;
        launch.cdp_port = Some(port);
        let id = self.spawn(launch)?;
        let timeout = startup();
        let deadline = Instant::now() + timeout;
        let mut page = loop {
            self.guard_desktop(id)?;
            if let Some(status) = self.exited(id) {
                return Err(format!("the app exited before its window opened ({status}). stderr: {}", self.stderr(id)));
            }
            match Page::connect(port, Duration::from_millis(500)) {
                Ok(page) => break page,
                Err(e) if Instant::now() >= deadline => return Err(format!("{e}. stderr: {}", self.stderr(id))),
                Err(_) => {}
            }
        };
        page.cdp.wait_loaded(deadline).map_err(|e| format!("{e}. stderr: {}", self.stderr(id)))?;
        page.wait_for_within(
            "the screen to render",
            "!!document.querySelector('.app')",
            deadline.saturating_duration_since(Instant::now()),
        )
        .map_err(|e| format!("{e}. stderr: {}", self.stderr(id)))?;
        self.guard_desktop(id)?;
        self.record(id)?;
        Ok(Opened { id, pid: self.live[id].pid, page })
    }

    /// The exit status, once the app has ended.
    pub fn exited(&mut self, id: usize) -> Option<String> {
        self.live[id].owned.status()
    }

    pub fn stderr(&self, id: usize) -> String {
        self.live[id].stderr.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string()
    }

    pub fn output(&self, id: usize) -> String {
        let stdout = self.live[id].stdout.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string();
        format!("stdout: {stdout}\nstderr: {}", self.stderr(id))
    }

    /// Adds the app's processes alive right now to the ones recorded, so they can be audited. Only a running app is
    /// looked at: once it has ended its id may belong to another program, and nothing about that program is ours. On
    /// Windows the app's job is asked too, and anything of the tree that it does not hold is noted as escaped.
    pub fn record(&mut self, id: usize) -> Result<(), String> {
        if !self.live[id].owned.is_running() {
            return Ok(());
        }
        self.guard_desktop(id)?;
        let live = &mut self.live[id];
        let first = snapshot()?;
        let members = live.owned.members();
        if let Some(members) = &members {
            let later = snapshot()?;
            for row in escaped(&first, &later, live.pid, members) {
                if !same_process(&row, &live.escaped) {
                    live.escaped.push(row);
                }
            }
        }
        let mut fresh = live_tree(&first, live.pid);
        fresh.retain(|row| !same_process(row, &live.seen));
        live.seen.extend(fresh);
        Ok(())
    }

    /// The second safety net of the hidden desktop: the app must have no visible window on the desktop the person is
    /// using. If it has one, its whole job is ended at once and this fails. Only local Windows runs require isolation.
    pub fn guard_desktop(&mut self, id: usize) -> Result<(), String> {
        #[cfg(windows)]
        {
            if !crate::desktop::hidden_required(std::env::var("GITHUB_ACTIONS").ok().as_deref()) {
                return Ok(());
            }
            let live = &self.live[id];
            if !live.owned.is_running() {
                return Ok(());
            }
            let mut pids = live.owned.members().unwrap_or_default();
            pids.push(live.pid);
            let shown = crate::desktop::visible_windows_of(&pids)?;
            if !shown.is_empty() {
                live.owned.terminate();
                return Err(format!(
                    "the app has {} visible window(s) on the desktop in use ({shown:?}), so it was ended: the hidden desktop is not working",
                    shown.len()
                ));
            }
        }
        #[cfg(not(windows))]
        let _ = id;
        Ok(())
    }

    /// The private working set of the app and its whole tree, in MiB, and whether a webview process is part of it.
    pub fn memory_mb(&mut self, id: usize) -> Result<(f64, bool), String> {
        self.record(id)?;
        let live = &self.live[id];
        let webview = live_tree(&snapshot()?, live.pid).iter().any(|r| r.name.to_lowercase().contains("msedgewebview2"));
        Ok((crate::proctree::private_mb(live.pid)?, webview))
    }

    /// The processor time, in seconds, the app and its whole tree have used so far; the difference of two readings is the cost of
    /// what happened between them.
    pub fn cpu_seconds(&mut self, id: usize) -> Result<f64, String> {
        self.record(id)?;
        crate::proctree::cpu_seconds(self.live[id].pid)
    }

    /// The processor time the app and its whole tree have used so far, in milliseconds.
    pub fn cpu_ms(&mut self, id: usize) -> Result<f64, String> {
        self.record(id)?;
        crate::proctree::cpu_ms(self.live[id].pid)
    }

    /// Ends the app and waits for it, whatever else happens. Its children are recorded first, so the audit can check that
    /// they follow; if that fails the app is ended all the same, and the failure is reported.
    pub fn close(&mut self, id: usize) -> Result<(), String> {
        let recorded = self.record(id);
        self.live[id].owned.stop_app();
        recorded
    }

    pub fn records(&self) -> Result<Vec<Recorded>, String> {
        read_records(&self.sb.state).map(|(commands, _)| commands)
    }

    /// Fake CLIs that never wrote that they ended.
    fn running(&self) -> Result<Vec<ProcessIdentity>, String> {
        let (commands, exited) = read_records(&self.sb.state)?;
        Ok(commands.iter().map(|c| (c.pid, c.born)).filter(|id| !exited.contains(id)).collect())
    }

    fn all_seen(&self) -> Vec<ProcRow> {
        self.live.iter().flat_map(|l| l.seen.iter().cloned()).collect()
    }

    fn roots(&self) -> Vec<ProcRow> {
        self.live.iter().filter_map(|l| l.seen.iter().find(|r| r.pid == l.pid).cloned()).collect()
    }

    /// What is still running of what this scenario started: recorded processes that are still the same process, what the
    /// apps' jobs still hold, and CLIs that never wrote their end. It is only ever a finding; nothing is ended by it.
    fn survivors(&self) -> Result<Vec<ProcRow>, String> {
        let roots = self.roots();
        let not_before = roots.iter().map(|r| r.born).min().unwrap_or(0);
        let rows = snapshot()?;
        let mut found = leaked(&rows, &self.all_seen(), &roots, &self.running()?, not_before);
        for pid in self.live.iter().filter_map(|l| l.owned.members()).flatten() {
            if !found.iter().any(|r| r.pid == pid) {
                found.push(rows.iter().find(|r| r.pid == pid).cloned().unwrap_or(ProcRow { pid, parent: 0, name: "?".into(), born: 0 }));
            }
        }
        Ok(found)
    }

    /// Checks that the scenario left nothing behind and touched nothing outside its sandbox.
    pub fn audit(&mut self) -> Result<(), String> {
        let mut problems: Vec<String> = Vec::new();
        match self.survivors() {
            Ok(rows) if rows.is_empty() => {}
            Ok(rows) => {
                let list: Vec<String> =
                    rows.iter().map(|r| format!("{}#{} (parent {}, born {})", r.name, r.pid, r.parent, r.born)).collect();
                problems.push(format!(
                    "a process of the app or of a CLI is still running: {}; roots {:?}; recorded {:?}; running {:?}",
                    list.join(", "),
                    self.roots(),
                    self.all_seen(),
                    self.running()?
                ));
            }
            Err(e) => problems.push(e),
        }
        let escaped: Vec<String> = self
            .live
            .iter()
            .flat_map(|l| {
                l.escaped.iter().map(move |r| {
                    format!("{}#{} (parent {}, born {}, root {}, recorded tree {:?})", r.name, r.pid, r.parent, r.born, l.pid, l.seen)
                })
            })
            .collect();
        if !escaped.is_empty() {
            problems.push(format!("a process of the app is outside its job object, so it cannot be ended: {}", escaped.join(", ")));
        }
        match self.records() {
            Ok(commands) => {
                for command in commands {
                    if !contained(&self.sb.sandbox, Path::new(&command.cwd)) {
                        problems.push(format!("a CLI ran from outside the sandbox: {}", command.cwd));
                    }
                    for target in &command.targets {
                        if !contained(&self.sb.sandbox, Path::new(target)) {
                            problems.push(format!("a CLI wrote outside the sandbox: {target}"));
                        }
                    }
                }
            }
            Err(e) => problems.push(e),
        }
        if let Err(e) = self.sb.audit_files() {
            problems.push(e);
        }
        if env_pairs() != self.real_env {
            problems.push("the real process environment changed".into());
        }
        match user_path() {
            Ok(now) if now == self.real_user_path => {}
            Ok(_) => problems.push("the real user PATH changed".into()),
            Err(e) => problems.push(e),
        }
        if let Some(server) = &self.server {
            let unexpected = server.unexpected();
            if !unexpected.is_empty() {
                problems.push(format!("a download asked for something other than the loopback source: {}", unexpected.join(", ")));
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) }
    }

    /// A screenshot of a checkpoint a scenario names, under the test build's `shots/` folder.
    pub fn shot(&self, app: &mut Opened, name: &str) -> Result<(), String> {
        app.page.screenshot(&self.paths.shots.join(format!("{name}.png")))
    }

    /// The audit, tried again for up to `wait`: a process that was just told to end needs a moment to go.
    pub fn audit_within(&mut self, wait: Duration) -> Result<(), String> {
        let deadline = Instant::now() + wait;
        loop {
            match self.audit() {
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
                Ok(()) => return Ok(()),
            }
        }
    }

    /// A screenshot of the first app that has a DevTools port, taken over its own connection so a stuck page client
    /// does not block it. Best effort: it is used after a failure.
    pub fn failure_shot(&self, to: &Path) {
        for live in &self.live {
            let Some(port) = live.port else { continue };
            if let Ok(mut page) = Page::connect(port, Duration::from_secs(2))
                && page.screenshot(to).is_ok()
            {
                return;
            }
        }
    }

    /// Ends the scenario: ends every app, waits for what it started to go, audits, and removes everything. The audit's
    /// verdict is returned, together with anything that could not be cleaned up; the cleanup happens either way.
    pub fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        for id in 0..self.live.len() {
            let _ = self.close(id);
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.survivors().is_ok_and(|rows| !rows.is_empty()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let verdict = self.audit();
        let cleaned = self.cleanup();
        match (verdict, cleaned) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Ok(())) => Err(a),
            (Ok(()), Err(c)) => Err(c),
            (Err(a), Err(c)) => Err(format!("{a}; {c}")),
        }
    }

    /// Ends what is left of what this scenario started, through its containment only, then removes its folders. Whatever
    /// could not be ended or removed is an error: a passing scenario must not leave anything behind.
    fn cleanup(&mut self) -> Result<(), String> {
        let mut problems: Vec<String> = Vec::new();
        for live in &self.live {
            live.owned.terminate();
        }
        registry().owned.retain(|o| !self.live.iter().any(|l| Arc::ptr_eq(o, &l.owned)));
        if let Some(server) = &mut self.server {
            server.stop();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.survivors() {
                Ok(rows) if rows.is_empty() => break,
                Ok(rows) if Instant::now() >= deadline => {
                    let list: Vec<String> = rows.iter().map(|r| format!("{}#{}", r.name, r.pid)).collect();
                    problems.push(format!("still running after the cleanup: {}", list.join(", ")));
                    break;
                }
                Ok(_) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    problems.push(e);
                    break;
                }
            }
        }
        if let Err(e) = self.sb.dispose() {
            problems.push(e);
        }
        registry().folders.retain(|f| f != &self.sb.root);
        if problems.is_empty() { Ok(()) } else { Err(format!("cleanup failed: {}", problems.join("; "))) }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            for id in 0..self.live.len() {
                let _ = self.close(id);
            }
            let _ = self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_paths_follow_default_relative_and_absolute_cargo_targets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(test_target(root, None), root.join("target/faber-test"));
        assert_eq!(test_target(root, Some(std::ffi::OsStr::new("target/proofs"))), root.join("target/proofs/faber-test"));
        let absolute = root.join("another-target");
        assert_eq!(test_target(root, Some(absolute.as_os_str())), absolute.join("faber-test"));
    }
    use serde_json::json;

    #[test]
    fn only_github_actions_gets_a_longer_startup_timeout() {
        assert_eq!(startup_timeout(Some("true")), Duration::from_secs(90));
        for value in [None, Some("false"), Some("1"), Some("TRUE")] {
            assert_eq!(startup_timeout(value), Duration::from_secs(30));
        }
    }

    #[test]
    fn diagnostic_capture_keeps_bounded_output() {
        let output = capture(Some(std::io::Cursor::new(vec![b'x'; 70_000])));
        let deadline = Instant::now() + Duration::from_secs(5);
        while output.lock().unwrap().len() < 64_000 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let text = output.lock().unwrap();
        assert!((64_000..68_096).contains(&text.len()));
        assert!(text.bytes().all(|b| b == b'x'));
    }

    fn row(pid: u32, parent: u32, born: u64) -> ProcRow {
        ProcRow { pid, parent, name: format!("p{pid}"), born }
    }
    fn ids(rows: &[ProcRow]) -> Vec<u32> {
        let mut ids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
        ids.sort();
        ids
    }
    // The same whole second, told apart only by microseconds.
    const T: u64 = 1_791_720_000_000_000;

    #[test]
    fn a_recorded_process_that_is_still_running_is_a_leak_but_a_reused_id_is_not() {
        let seen = [row(50, 10, T + 100), row(51, 50, T + 200)];
        let rows = [row(50, 10, T + 100), row(51, 7, T + 900_000)];
        assert_eq!(ids(&leaked(&rows, &seen, &[], &[], 0)), [50], "51 is a different process that took the same id in the same second");
    }

    #[test]
    fn what_still_names_a_dead_app_as_its_parent_is_a_leak_unless_it_is_older_than_the_app() {
        let app = row(900, 4, T + 500_000);
        let rows = [row(901, 900, T + 600_000), row(902, 900, T + 400_000)];
        assert_eq!(ids(&leaked(&rows, &[], std::slice::from_ref(&app), &[], 0)), [901]);
    }

    #[test]
    fn a_root_id_that_now_belongs_to_another_process_collects_nothing() {
        let app = row(900, 4, T + 500_000);
        let rows = [row(900, 4, T + 800_000), row(901, 900, T + 900_000)];
        assert!(leaked(&rows, &[], std::slice::from_ref(&app), &[], 0).is_empty());
    }

    #[test]
    fn an_unfinished_cli_record_cannot_claim_a_reused_process_id() {
        let mut reused = row(71, 1, T + 999_000);
        reused.name = "wuaucltcore.exe".into();
        assert!(leaked(&[reused], &[], &[], &[(71, T + 600_000)], T + 500_000).is_empty());
    }

    #[test]
    fn a_cli_that_never_wrote_its_end_counts_only_if_it_is_younger_than_the_app() {
        let rows = [row(70, 1, T + 10), row(71, 1, T + 999_000)];
        assert_eq!(ids(&leaked(&rows, &[], &[], &[(70, T + 10), (71, T + 999_000)], T + 500_000)), [71]);
    }

    #[test]
    fn an_exit_record_only_settles_its_exact_cli_process() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        let command = |born| json!({"pid": 7, "born": born, "cwd": "C:/s", "vendor": "codex", "args": [], "targets": []});
        std::fs::write(
            ctx.sb.state.join("commands.jsonl"),
            format!("{}\n{}\n{}\n", command(1), json!({"exited": 7, "born": 1}), command(2)),
        )
        .unwrap();
        assert_eq!(ctx.running().unwrap(), [(7, 2)]);
        std::fs::write(
            ctx.sb.state.join("commands.jsonl"),
            format!("{}\n", json!({"pid": 7, "cwd": "C:/s", "vendor": "codex", "args": [], "targets": []})),
        )
        .unwrap();
        assert!(ctx.running().unwrap_err().contains("no creation time"));
        std::fs::remove_file(ctx.sb.state.join("commands.jsonl")).unwrap();
        ctx.finish().unwrap();
    }

    #[test]
    fn the_records_list_commands_and_the_ones_that_ended() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("commands.jsonl"),
            "{\"born\":1,\"pid\":7,\"cwd\":\"C:/s\",\"vendor\":\"claude\",\"args\":[\"auth\",\"status\"],\"targets\":[\"C:/s/x\"]}\n{\"born\":1,\"exited\":7}\n{\"born\":1,\"pid\":8,\"cwd\":\"C:/s\",\"vendor\":\"codex\",\"args\":[\"login\"],\"targets\":[]}\n",
        )
        .unwrap();
        let (commands, exited) = read_records(dir.path()).unwrap();
        assert_eq!(exited, [(7, 1)]);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].line(), "auth status");
        assert_eq!(commands[1].vendor, "codex");
        assert_eq!(read_records(&dir.path().join("missing")).unwrap(), (vec![], vec![]), "no file: no CLI ran");
    }

    // An audit that skips what it cannot read would pass a CLI that ran outside the sandbox.
    #[test]
    fn a_log_that_is_cut_off_or_incomplete_is_an_error_and_never_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let write = |text: &str| std::fs::write(dir.path().join("commands.jsonl"), text).unwrap();
        write("{\"born\":1,\"pid\":7,\"cwd\":\"C:/s\",\"vendor\":\"claude\",\"args\":[],\"tar");
        assert!(read_records(dir.path()).unwrap_err().contains("line 1"));
        write(
            "{\"born\":1,\"pid\":7,\"vendor\":\"claude\",\"args\":[],\"targets\":[]}
",
        );
        assert!(read_records(dir.path()).unwrap_err().contains("no cwd"));
        write(
            "{\"born\":1,\"pid\":7,\"cwd\":\"C:/s\",\"vendor\":\"claude\",\"args\":[1],\"targets\":[]}
",
        );
        assert!(read_records(dir.path()).unwrap_err().contains("non-text"));
        std::fs::remove_file(dir.path().join("commands.jsonl")).unwrap();
        std::fs::create_dir(dir.path().join("commands.jsonl")).unwrap();
        assert!(read_records(dir.path()).is_err(), "a log that cannot be read is not an empty log");
    }

    #[test]
    fn an_interrupted_run_removes_its_sandbox_folders() {
        // A registry of its own: the global one holds the folders of the tests that run at the same time.
        let local = Mutex::new(Registry::default());
        let dir = tempfile::tempdir().unwrap().keep();
        std::fs::write(dir.join("x.txt"), "x").unwrap();
        local.lock().unwrap().folders.push(dir.clone());
        cleanup_registry(&local);
        assert!(!dir.exists());
    }

    // R5.5: the audit of a whole scenario rejects writes outside its temporary folders, whatever the file is called.
    // It needs no app, so it runs on every system.
    #[test]
    fn r5_5_the_scenario_audit_rejects_writes_outside_the_sandbox() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        ctx.audit().expect("a fresh sandbox is clean");
        std::fs::write(ctx.sb.data.join("settings.json"), "{}").unwrap();
        ctx.audit().expect("a write inside the sandbox is fine");
        let beyond_observed = ctx.sb.root.join("external-write.txt");
        std::fs::write(&beyond_observed, "unauthorized absolute write beyond observed root").unwrap();
        assert!(ctx.audit().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&beyond_observed).unwrap();
        let named_like_a_test = ctx.sb.enclosing.join("unauthorized.test.ts");
        std::fs::write(&named_like_a_test, "unauthorized write named test.ts").unwrap();
        assert!(ctx.audit().unwrap_err().to_lowercase().contains("write outside sandbox"));
        std::fs::remove_file(&named_like_a_test).unwrap();
        ctx.finish().expect("with the stray files removed the scenario ends clean");
    }

    #[test]
    fn a_cli_that_ran_or_wrote_outside_the_sandbox_fails_the_audit() {
        let paths = Paths::discover().unwrap();
        let mut ctx = Ctx::new(&paths).unwrap();
        let outside = ctx.sb.root.parent().unwrap().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let inside = ctx.sb.data.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let record = |cwd: &str, target: &str| {
            let command = json!({ "pid": 1, "born": 1, "cwd": cwd, "vendor": "claude", "args": [], "targets": [target] });
            format!("{command}\n{}\n", json!({ "exited": 1, "born": 1 }))
        };
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&inside, &inside)).unwrap();
        ctx.audit().expect("a CLI that stayed inside");
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&outside, &inside)).unwrap();
        assert!(ctx.audit().unwrap_err().contains("ran from outside the sandbox"));
        std::fs::write(ctx.sb.state.join("commands.jsonl"), record(&inside, &outside)).unwrap();
        assert!(ctx.audit().unwrap_err().contains("wrote outside the sandbox"));
        std::fs::remove_file(ctx.sb.state.join("commands.jsonl")).unwrap();
        ctx.finish().unwrap();
    }
}
