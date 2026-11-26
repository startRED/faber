//! An explicitly started dev server, owned by the conversation that started it.
use super::{Core, Engine};
use crate::browser::{MAX_URL, local_app_url};
use crate::cli::Cli;
use crate::model::Area;
use crate::proc::{Containment, Proc, Spawner};
use crate::project::git::{path_within, same_path};
use crate::project::{checks, run_app};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Instant;

const STARTUP: Duration = Duration::from_secs(30);
const STOP: Duration = Duration::from_secs(5);
/// How long a deletion, project removal or shutdown waits, with the engine held, for an app it stopped: killing the process
/// tree is immediate where a job holds it, a run that is not confirmed gone stays recorded as failed so nothing is dropped
/// under it, and on Windows the job kills whatever is left when Faber exits.
pub(super) const STOP_HELD: Duration = Duration::from_millis(300);
const TAIL: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum View {
    Idle,
    Confirm { command: String, scripts: Vec<(String, String)>, seen: String },
    Starting,
    Running { url: String },
    Stopping,
    Notice { reason: &'static str, output: String },
    StopFailed { output: String },
}

impl View {
    fn notice(reason: &'static str) -> Self {
        Self::Notice { reason, output: String::new() }
    }
}

pub(super) struct Run {
    owner: String,
    pub(super) project: String,
    folder: PathBuf,
    view: Arc<Mutex<View>>,
    stop: Option<oneshot::Sender<()>>,
    tree: Option<Arc<Containment>>,
    task: JoinHandle<()>,
}

impl Run {
    fn start(owner: &str, project: &str, folder: PathBuf, spawn: &Spawner, command: &str, args: &[String]) -> std::io::Result<Self> {
        let mut proc = spawn(command, args, &folder)?;
        let tree = proc.contained.take().map(Arc::new);
        let view = Arc::new(Mutex::new(View::Starting));
        let (stop, cancel) = oneshot::channel();
        let task = tokio::spawn(monitor(proc, tree.clone(), view.clone(), cancel));
        Ok(Self { owner: owner.into(), project: project.into(), folder, view, stop: Some(stop), tree, task })
    }

    fn active(&self) -> bool {
        !self.task.is_finished() || self.tree.as_ref().and_then(|tree| tree.owned_tree()).is_some_and(|tree| !tree.is_retired())
    }

    fn view(&self) -> View {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn stop(&mut self) -> View {
        let mut view = self.view.lock().unwrap_or_else(|e| e.into_inner());
        let retry = match &*view {
            View::Idle | View::Notice { .. } => return view.clone(),
            View::StopFailed { output } => Some(output.clone()),
            _ => None,
        };
        *view = View::Stopping;
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        drop(view);
        if let Some(tree) = &self.tree {
            tree.end();
            if let Some(output) = retry {
                let (tree, view) = (tree.clone(), self.view.clone());
                self.task = tokio::spawn(async move {
                    set(&view, if tree.finish().await { View::Idle } else { View::StopFailed { output } });
                });
            }
        }
        View::Stopping
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if let Some(tree) = &self.tree {
            tree.end();
        }
    }
}

impl Engine {
    pub fn run_app_start(&self, id: &str, seen: Option<&str>) -> Result<View, String> {
        self.with(|core| core.start_app(id, seen, resolve_manager))
    }

    pub fn run_app_status(&self, id: &str) -> View {
        self.with(|core| core.apps.iter().find(|run| run.owner == id).map_or(View::Idle, Run::view))
    }

    pub fn run_app_stop(&self, id: &str) -> View {
        self.with(|core| {
            let Some(run) = core.apps.iter_mut().find(|run| run.owner == id && run.active()) else { return View::Idle };
            run.stop()
        })
    }
}

impl Core {
    fn start_app(&mut self, id: &str, seen: Option<&str>, resolve: impl FnOnce(&Path, &Path) -> Option<Cli>) -> Result<View, String> {
        if self.closing || self.projects_closed {
            return Err("reason.runApp.launch".into());
        }
        if let Some(run) = self.apps.iter().find(|run| run.owner == id && run.active()) {
            return Ok(run.view());
        }
        self.apps.retain(|run| run.owner != id);
        let conversation = self.load(id).filter(|c| c.area == Area::Project).ok_or("error.conversationGone")?;
        let project_id = conversation.project_id.clone().ok_or("error.projectGone")?;
        if conversation.copy.as_ref().is_some_and(|copy| copy.root.is_empty()) || self.copying.contains_key(id) {
            return Ok(View::notice("reason.runApp.copyPending"));
        }
        // conversation_folder canonicalizes existing folders and keeps the native spelling for the launch.
        let folder = self.conversation_folder(id).filter(|folder| folder.is_dir()).ok_or("error.workingFolder")?;
        if self.apps.iter().any(|run| same_path(&run.folder, &folder) && run.active()) {
            return Ok(View::notice("reason.runApp.busy"));
        }
        let Some(declared) = run_app::declared(&folder) else { return Ok(View::notice("reason.runApp.missing")) };
        if declared.manager != "npm" {
            return Ok(View::notice("reason.runApp.managerUnsupported"));
        }
        let mut projects = self.store.projects_checked().map_err(|err| err.key())?;
        let project = projects.iter_mut().find(|project| project.id == project_id).ok_or("error.projectGone")?;
        let stamp = declared.stamp(&folder).map_err(|_| "reason.runApp.launch")?;
        let current = run_app::seen(&stamp, &folder);
        let confirmed = run_app::confirmed(project, &stamp);
        if seen.is_some_and(|seen| seen != current) || (seen.is_none() && !confirmed) {
            return Ok(View::Confirm { command: declared.command(), scripts: declared.scripts, seen: current });
        }
        let cli = resolve(&folder, Path::new(&project.path));
        if !confirmed {
            run_app::confirm(project, stamp);
            self.store.save_projects(&projects).map_err(|_| "error.store.write")?;
        }
        let Some(cli) = cli else { return Ok(View::notice("reason.runApp.managerMissing")) };
        let args: Vec<_> = cli.prefix.into_iter().chain(["run".into(), declared.script.into()]).collect();
        // npm can start its own cmd.exe for a script, which may flash a console on Windows.
        let run = match Run::start(id, &project_id, folder, &self.opts.spawn, &cli.command, &args) {
            Ok(run) => run,
            Err(_) => return Ok(View::notice("reason.runApp.launch")),
        };
        // Keep the latest notices for reopening a pane. Running copies always keep their slots.
        let mut kept = 0;
        self.apps.reverse();
        self.apps.retain(|old| {
            if old.active() {
                return true;
            }
            kept += 1;
            !same_path(&old.folder, &run.folder) && kept <= 16
        });
        self.apps.reverse();
        self.apps.push(run);
        Ok(View::Starting)
    }

    pub(super) fn app_active(&self, id: &str, copy: &Path) -> bool {
        self.apps
            .iter()
            .any(|run| run.active() && (run.owner == id || copy.canonicalize().is_ok_and(|root| path_within(&run.folder, &root))))
    }

    pub(super) fn stop_removed_app(&mut self, id: &str) {
        for run in self.apps.iter_mut().filter(|run| run.owner == id) {
            run.stop();
        }
        self.apps.retain(|run| run.owner != id || run.active());
    }

    pub(super) fn stop_conversation_app(&mut self, id: &str) -> Result<(), String> {
        if self.apps.is_empty() {
            return Ok(());
        }
        let copy = self.load(id).and_then(|conv| Path::new(&conv.copy.as_ref()?.path).canonicalize().ok());
        self.stop_apps(STOP_HELD, |run| run.owner == id || copy.as_ref().is_some_and(|root| path_within(&run.folder, root)))
    }

    pub(super) fn stop_apps(&mut self, wait: Duration, matches: impl Fn(&Run) -> bool) -> Result<(), String> {
        let mut runs = self.apps.iter_mut().filter(|run| run.active() && matches(run)).collect::<Vec<_>>();
        for run in &mut runs {
            run.stop();
        }
        // These callers are synchronous. Confirm the tree directly while its monitor reaps the root.
        let deadline = std::time::Instant::now() + wait;
        let mut stopped = true;
        for run in runs {
            stopped &= loop {
                let empty = run.tree.as_ref().map_or_else(|| Ok(run.task.is_finished()), |tree| tree.confirm_stopped());
                match empty {
                    Ok(true) => break true,
                    Ok(false) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                    _ => break false,
                }
            };
        }
        if stopped { Ok(()) } else { Err("reason.runApp.stopFailed".into()) }
    }
}

fn resolve_manager(folder: &Path, project: &Path) -> Option<Cli> {
    let path = std::env::var("PATH").unwrap_or_default();
    npm_outside_project(&path, folder, project)
}

fn npm_outside_project(path: &str, folder: &Path, project: &Path) -> Option<Cli> {
    let project_root = project.canonicalize().ok();
    let outside = |path: &Path| {
        !path_within(path, folder)
            && !(project.is_absolute() && path_within(path, project))
            && !project_root.as_ref().is_some_and(|root| path_within(path, root))
    };
    npm_on_path(path, &|path| {
        let path = Path::new(path);
        outside(path) && path.canonicalize().is_ok_and(|real| real.is_file() && outside(&real))
    })
}

fn npm_on_path(path: &str, exists: &dyn Fn(&str) -> bool) -> Option<Cli> {
    path.split(if cfg!(windows) { ';' } else { ':' })
        .filter(|dir| Path::new(dir).is_absolute())
        .find_map(|dir| checks::node_tool("npm", dir, cfg!(windows), exists))
}

fn set(view: &Mutex<View>, next: View) {
    *view.lock().unwrap_or_else(|e| e.into_inner()) = next;
}

fn publish_url(view: &Mutex<View>, url: String) {
    let mut view = view.lock().unwrap_or_else(|e| e.into_inner());
    if matches!(*view, View::Starting) {
        *view = View::Running { url };
    }
}

fn plain(text: &str) -> String {
    let mut pieces = text.split('\x1b');
    let mut out = pieces.next().unwrap_or_default().to_owned();
    for part in pieces {
        let rest =
            part.strip_prefix('[').and_then(|csi| csi.find(|ch: char| ch.is_ascii_alphabetic()).map(|at| &csi[at + 1..])).unwrap_or(part);
        out.push_str(rest);
    }
    out.retain(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'));
    out
}

fn url_on_line(line: &[u8]) -> Option<String> {
    let clean = plain(&String::from_utf8_lossy(line));
    clean.split(|ch: char| ch.is_whitespace() || matches!(ch, '\'' | '"' | '\x60' | '<' | '>')).find_map(|word| {
        let word = word.trim_start_matches('(').trim_end_matches(['.', ',', ';', '!', ')', '}']);
        let (prefix, _) = word.split_once("://")?;
        let word = match prefix.split_once('=') {
            Some((label, _)) if !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') => {
                &word[label.len() + 1..]
            }
            _ => word,
        };
        let url = local_app_url(word)?.to_string();
        (url.chars().count() <= MAX_URL).then_some(url)
    })
}

// Keep line fragments separately so reads split inside a URL, or interleaved across the two pipes, cannot invent a URL.
fn read_url(line: &mut Vec<u8>, bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return url_on_line(line);
    }
    for part in bytes.split_inclusive(|byte| matches!(byte, b'\n' | b'\r')) {
        line.drain(..(line.len() + part.len()).saturating_sub(TAIL));
        line.extend_from_slice(part);
        if part.last().is_some_and(|byte| matches!(byte, b'\n' | b'\r')) {
            let url = url_on_line(line);
            line.clear();
            if url.is_some() {
                return url;
            }
        }
    }
    None
}

async fn monitor(proc: Proc, tree: Option<Arc<Containment>>, view: Arc<Mutex<View>>, mut cancel: oneshot::Receiver<()>) {
    let Proc { stdin, mut stdout, mut stderr, mut child, .. } = proc;
    drop(stdin);
    let (mut out, mut err) = ([0; 4096], [0; 4096]);
    let mut eof = false;
    let mut lines = [Vec::new(), Vec::new()];
    let mut tail = VecDeque::new();
    let mut ready = false;
    let mut exited = child.is_none();
    let mut deadline = Some(Instant::now() + STARTUP);
    let reason = loop {
        if exited && eof && stderr.is_none() {
            break Some("reason.runApp.exited");
        }
        let (stream, bytes) = tokio::select! {
            _ = &mut cancel => break None,
            _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                if exited && child.is_some() {
                    break Some("reason.runApp.exited");
                }
                // Some servers leave their final line open. Wait before treating a partial port as complete.
                if let Some(url) = lines.iter().find_map(|line| url_on_line(line)) {
                    ready = true;
                    deadline = None;
                    lines = [Vec::new(), Vec::new()];
                    publish_url(&view, url);
                    continue;
                }
                break Some("reason.runApp.noUrl");
            },
            _ = async { child.as_mut().unwrap().wait().await }, if !exited => {
                exited = true;
                // A descendant can keep the descriptors open after the root exits.
                deadline = Some(Instant::now() + Duration::from_millis(150));
                continue;
            },
            read = stdout.read(&mut out), if !eof => {
                let n = read.unwrap_or(0);
                eof = n == 0;
                (0, &out[..n])
            },
            read = async { stderr.as_mut().unwrap().read(&mut err).await }, if stderr.is_some() => {
                let n = read.unwrap_or(0);
                if n == 0 { stderr = None; }
                (1, &err[..n])
            },
        };
        tail.extend(bytes);
        tail.drain(..tail.len().saturating_sub(TAIL));
        if !ready && let Some(url) = read_url(&mut lines[stream], bytes) {
            ready = true;
            if !exited || child.is_none() {
                deadline = None;
                publish_url(&view, url);
            }
            lines = [Vec::new(), Vec::new()];
        }
    };
    set(&view, View::Stopping);
    let stop_by = Instant::now() + STOP;
    if let Some(tree) = &tree {
        tree.end();
    }
    if let Some(child) = &mut child {
        let _ = child.start_kill();
        let _ = tokio::time::timeout_at(stop_by, child.wait()).await;
    }
    let mut output = || {
        let text = plain(&String::from_utf8_lossy(tail.make_contiguous())).replace("\r\n", "\n").replace('\r', "\n");
        let text = text.trim_end();
        let last = text.rsplitn(21, '\n').nth(20).map_or(text, |older| &text[older.len() + 1..]);
        let start = last.ceil_char_boundary(last.len().saturating_sub(TAIL));
        last[start..].into()
    };
    let failed = if let Some(tree) = tree { !tokio::time::timeout_at(stop_by, tree.finish()).await.unwrap_or(false) } else { false };
    let mut view = view.lock().unwrap_or_else(|e| e.into_inner());
    *view = if failed {
        View::StopFailed { output: output() }
    } else {
        let reason = reason.filter(|_| cancel.try_recv().is_err());
        reason.map_or(View::Idle, |reason| View::Notice { reason, output: output() })
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineOptions;
    use crate::model::Project;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::io::{AsyncWriteExt, DuplexStream};

    fn fake(input: DuplexStream, stopped: Arc<AtomicBool>) -> Spawner {
        let ended = stopped.clone();
        fake_with_tree(
            input,
            Containment::scripted(move || Ok(ended.load(Ordering::Acquire)), move || stopped.store(true, Ordering::Release)),
        )
    }

    fn fake_with_tree(input: DuplexStream, tree: Containment) -> Spawner {
        let input = Mutex::new(Some(input));
        let tree = Mutex::new(Some(tree));
        Arc::new(move |bin, args, _| {
            assert_eq!(bin, "fake-manager");
            assert_eq!(args, ["run", "dev"]);
            Ok(Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(input.lock().unwrap().take().unwrap()),
                stderr: None,
                child: None,
                contained: Some(tree.lock().unwrap().take().unwrap()),
            })
        })
    }

    fn start_fake(engine: &Engine, id: &str, seen: Option<&str>) -> View {
        engine.with(|core| core.start_app(id, seen, |_, _| Some(Cli { command: "fake-manager".into(), prefix: Vec::new() }))).unwrap()
    }

    #[tokio::test]
    async fn run_app_opens_the_first_local_url_and_reports_exit_with_bounded_output() {
        let (reader, mut writer) = tokio::io::duplex(64);
        let spawner = fake(reader, Arc::new(AtomicBool::new(false)));
        let mut run = Run::start("one", "project", PathBuf::new(), &spawner, "fake-manager", &["run".into(), "dev".into()]).unwrap();
        writer.write_all(b"https://example.org/\n\x1b[32mURL=http://localhost:5").await.unwrap();
        tokio::task::yield_now().await;
        assert_eq!(run.view(), View::Starting, "a partial URL is not a page");
        writer.write_all(b"173/\x1b[0m\nhttp://localhost:7000/\n").await.unwrap();
        tokio::task::yield_now().await;
        assert_eq!(run.view(), View::Running { url: "http://localhost:5173/".into() });
        writer.write_all(&vec![b'a'; 12_000]).await.unwrap();
        writer.write_all(b"\n").await.unwrap();
        for line in 0..30 {
            writer.write_all(format!("line {line}\n").as_bytes()).await.unwrap();
        }
        drop(writer);
        (&mut run.task).await.unwrap();
        let expected = (10..30).map(|line| format!("line {line}")).collect::<Vec<_>>().join("\n");
        assert_eq!(run.view(), View::Notice { reason: "reason.runApp.exited", output: expected });
        assert!(!run.active());
    }

    #[tokio::test(start_paused = true)]
    async fn run_app_times_out_and_terminates_a_process_that_keeps_its_pipes_open() {
        let (reader, mut writer) = tokio::io::duplex(8192);
        let stopped = Arc::new(AtomicBool::new(false));
        let spawner = fake(reader, stopped.clone());
        let mut run = Run::start("one", "project", PathBuf::new(), &spawner, "fake-manager", &["run".into(), "dev".into()]).unwrap();
        let huge = "ä".repeat(5000) + "Still building…";
        writer.write_all(huge.as_bytes()).await.unwrap();
        (&mut run.task).await.unwrap();
        assert!(stopped.load(Ordering::Acquire));
        let View::Notice { reason, output } = run.view() else { panic!("expected startup timeout") };
        assert_eq!(reason, "reason.runApp.noUrl");
        assert!(output.len() <= TAIL && output.ends_with("Still building…"));
    }

    #[tokio::test(start_paused = true)]
    async fn run_app_accepts_a_local_address_on_an_unfinished_line() {
        let (reader, mut writer) = tokio::io::duplex(64);
        let spawner = fake(reader, Arc::new(AtomicBool::new(false)));
        let mut run = Run::start("one", "project", PathBuf::new(), &spawner, "fake-manager", &["run".into(), "dev".into()]).unwrap();
        writer.write_all(b"http://localhost:5173").await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(STARTUP).await;
        tokio::task::yield_now().await;
        assert_eq!(run.view(), View::Running { url: "http://localhost:5173/".into() });
        run.stop();
        (&mut run.task).await.unwrap();
        assert_eq!(run.view(), View::Idle);
    }

    fn engine_with_spawn(data: &Path, spawn: Spawner) -> Engine {
        Engine::begin(
            EngineOptions {
                data_dir: data.into(),
                #[cfg(windows)]
                chat_dir: data.with_extension("work"),
                clis: Default::default(),
                spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap()
    }

    fn add_project(engine: &Engine, name: &str, folder: &Path) -> String {
        let project = Project::new(name.into(), name.into(), folder.to_string_lossy().into_owned());
        engine.with(|core| {
            let mut projects = core.store.projects_checked().unwrap();
            projects.push(project);
            core.store.save_projects(&projects).unwrap();
            core.blank(Area::Project, Some(name.into()))
        })
    }

    #[tokio::test]
    async fn app_confirmation_rechecks_scripts_and_config_before_spawning() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let package = folder.path().join("package.json");
        std::fs::write(&package, r#"{"scripts":{"dev":"first"}}"#).unwrap();
        let (reader, _writer) = tokio::io::duplex(64);
        let stopped = Arc::new(AtomicBool::new(false));
        let engine = engine_with_spawn(data.path(), fake(reader, stopped.clone()));
        let id = add_project(&engine, "proj", folder.path());
        let resolved = AtomicUsize::new(0);
        let start = |id: &str, seen: Option<&str>| {
            engine
                .with(|core| {
                    core.start_app(id, seen, |_, _| {
                        resolved.fetch_add(1, Ordering::Relaxed);
                        Some(Cli { command: "fake-manager".into(), prefix: Vec::new() })
                    })
                })
                .unwrap()
        };
        let View::Confirm { seen, .. } = start(&id, None) else { panic!("expected confirmation") };
        std::fs::write(&package, r#"{"scripts":{"dev":"second"}}"#).unwrap();
        let View::Confirm { seen, .. } = start(&id, Some(&seen)) else { panic!("a script edit must ask again") };
        std::fs::write(folder.path().join(".npmrc"), "script-shell=custom-shell\n").unwrap();
        let View::Confirm { seen: fresh, .. } = start(&id, Some(&seen)) else { panic!("a config edit must ask again") };
        assert_eq!(resolved.load(Ordering::Relaxed), 0);
        assert_eq!(start(&id, Some(&fresh)), View::Starting);
        assert_eq!(resolved.load(Ordering::Relaxed), 1);
        let saved = engine.project("proj").unwrap();
        let stamp = run_app::declared(folder.path()).unwrap().stamp(folder.path()).unwrap();
        assert!(run_app::confirmed(&saved, &stamp));
        engine.shutdown();
    }

    #[tokio::test]
    async fn stop_is_visible_immediately_and_a_late_url_cannot_publish() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let (reader, _writer) = tokio::io::duplex(64);
        let engine = engine_with_spawn(data.path(), fake(reader, Arc::new(AtomicBool::new(false))));
        std::fs::write(folder.path().join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let id = add_project(&engine, "proj", folder.path());
        let View::Confirm { seen, .. } = start_fake(&engine, &id, None) else { panic!("expected confirmation") };
        assert_eq!(start_fake(&engine, &id, Some(&seen)), View::Starting);
        assert_eq!(engine.run_app_stop(&id), View::Stopping);
        assert_eq!(engine.run_app_status(&id), View::Stopping);
        // Drive the monitor's publish step directly to force the interleaving with Stop.
        engine.with(|core| publish_url(&core.apps[0].view, "http://localhost:5173/".into()));
        assert_eq!(engine.run_app_status(&id), View::Stopping);
        engine.shutdown();
    }

    #[tokio::test]
    async fn deletion_project_removal_and_shutdown_wait_for_the_tree() {
        for action in ["conversation", "project", "shutdown"] {
            let data = tempfile::tempdir().unwrap();
            let source = tempfile::tempdir().unwrap();
            let (reader, _writer) = tokio::io::duplex(64);
            let requested = Arc::new(AtomicBool::new(false));
            let ending = requested.clone();
            let probes = Arc::new(AtomicUsize::new(0));
            let count = probes.clone();
            let query_ok = Arc::new(AtomicBool::new(true));
            let can_query = query_ok.clone();
            let tree = Containment::scripted(
                move || {
                    if !can_query.load(Ordering::Acquire) {
                        return Err(std::io::Error::other("tree state unavailable"));
                    }
                    Ok(ending.load(Ordering::Acquire) && count.fetch_add(1, Ordering::AcqRel) >= 1)
                },
                move || requested.store(true, Ordering::Release),
            );
            let engine = engine_with_spawn(data.path(), fake_with_tree(reader, tree));
            let id = add_project(&engine, "proj", source.path());
            let folder = crate::project::copy::home(data.path()).join(&id);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
            engine.with(|core| {
                core.convs.get_mut(&id).unwrap().copy = Some(crate::model::WorkCopy {
                    path: folder.to_string_lossy().into_owned(),
                    root: source.path().to_string_lossy().into_owned(),
                    branch: "faber/app".into(),
                    sub: String::new(),
                    base: None,
                });
            });
            let View::Confirm { seen, .. } = start_fake(&engine, &id, None) else { panic!("expected confirmation") };
            assert_eq!(start_fake(&engine, &id, Some(&seen)), View::Starting);
            match action {
                "conversation" => {
                    query_ok.store(false, Ordering::Release);
                    assert_eq!(engine.remove_with(&id, true), Err("reason.runApp.stopFailed".into()));
                    assert!(folder.is_dir());
                    query_ok.store(true, Ordering::Release);
                    let (drop_started, drop_seen) = std::sync::mpsc::channel();
                    let probes = probes.clone();
                    engine.with(|core| {
                        // Observe the copy cleanup handoff without running Git.
                        core.git = Some(Arc::new(move || {
                            let _ = drop_started.send(probes.load(Ordering::Acquire));
                            None
                        }));
                    });
                    engine.remove_with(&id, true).unwrap();
                    assert_eq!(drop_seen.recv_timeout(Duration::from_secs(1)).unwrap(), 2);
                }
                "project" => engine.remove_project("proj").unwrap(),
                _ => engine.shutdown(),
            }
            assert_eq!(probes.load(Ordering::Acquire), 2, "{action} returned before the tree was empty");
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn a_tree_that_will_not_empty_holds_the_engine_only_briefly_on_deletion_or_removal() {
        for action in ["conversation", "project"] {
            let data = tempfile::tempdir().unwrap();
            let source = tempfile::tempdir().unwrap();
            let (reader, _writer) = tokio::io::duplex(64);
            let tree = Containment::scripted(|| Ok(false), || {});
            let engine = engine_with_spawn(data.path(), fake_with_tree(reader, tree));
            let id = add_project(&engine, "proj", source.path());
            let folder = crate::project::copy::home(data.path()).join(&id);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
            engine.with(|core| {
                core.convs.get_mut(&id).unwrap().copy = Some(crate::model::WorkCopy {
                    path: folder.to_string_lossy().into_owned(),
                    root: source.path().to_string_lossy().into_owned(),
                    branch: "faber/app".into(),
                    sub: String::new(),
                    base: None,
                });
            });
            let View::Confirm { seen, .. } = start_fake(&engine, &id, None) else { panic!("expected confirmation") };
            assert_eq!(start_fake(&engine, &id, Some(&seen)), View::Starting);
            let began = Instant::now();
            let result = if action == "conversation" { engine.remove_with(&id, true) } else { engine.remove_project("proj").map(|_| ()) };
            assert_eq!(result, Err("reason.runApp.stopFailed".into()), "{action}");
            assert!(began.elapsed() < Duration::from_secs(2), "{action} held the engine for {:?}", began.elapsed());
            assert!(folder.is_dir(), "{action} must not drop a copy under a live app");
        }
    }

    #[tokio::test]
    async fn unsupported_lockfiles_never_resolve_or_spawn() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let engine = engine_with_spawn(data.path(), Arc::new(|_, _, _| panic!("must not spawn")));
        let id = add_project(&engine, "proj", folder.path());
        for name in ["pnpm-lock.yaml", "yarn.lock", "bun.lock", "bun.lockb"] {
            let lockfile = folder.path().join(name);
            std::fs::write(&lockfile, []).unwrap();
            let found = engine.with(|core| core.start_app(&id, None, |_, _| panic!("must not resolve"))).unwrap();
            assert_eq!(found, View::notice("reason.runApp.managerUnsupported"));
            std::fs::remove_file(lockfile).unwrap();
        }
    }

    #[test]
    fn npm_resolution_skips_relative_path_entries_and_project_tools() {
        assert!(npm_on_path(".", &|_| panic!("must not inspect relative PATH entries")).is_none());
        let dir = tempfile::tempdir().unwrap();
        let install = |root: &Path| {
            let (bin, npm, node, script) = if cfg!(windows) {
                (root.to_path_buf(), "npm.cmd", "node.exe", root.join("node_modules/npm/bin/npm-cli.js"))
            } else {
                (root.join("bin"), "npm", "node", root.join("lib/node_modules/npm/bin/npm-cli.js"))
            };
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::create_dir_all(script.parent().unwrap()).unwrap();
            for file in [bin.join(npm), bin.join(node), script] {
                std::fs::write(file, []).unwrap();
            }
            bin
        };
        let project = dir.path().join("project");
        let local = install(&project);
        let external = install(&dir.path().join("tools"));
        assert!(npm_outside_project(local.to_str().unwrap(), &project, &project).is_none());
        let found = npm_outside_project(external.to_str().unwrap(), &project, &project).unwrap();
        assert_eq!(Path::new(&found.command).parent(), Some(external.as_path()));
        assert!(found.prefix[0].ends_with("npm-cli.js"));
    }

    #[tokio::test]
    async fn aliases_of_one_folder_cannot_start_two_apps() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let alias = folder.path().join("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&app, &alias).unwrap();
        #[cfg(windows)]
        junction::create(&app, &alias).unwrap();
        let (reader, _writer) = tokio::io::duplex(64);
        let engine = engine_with_spawn(data.path(), fake(reader, Arc::new(AtomicBool::new(false))));
        let id = add_project(&engine, "proj", &app);
        let other = add_project(&engine, "other", &alias);
        let View::Confirm { seen, .. } = start_fake(&engine, &id, None) else { panic!("expected confirmation") };
        assert_eq!(start_fake(&engine, &id, Some(&seen)), View::Starting);
        #[cfg(windows)]
        engine.with(|core| core.apps[0].folder.set_file_name("APP"));
        assert_eq!(start_fake(&engine, &other, None), View::notice("reason.runApp.busy"));
        engine.shutdown();
    }
}
