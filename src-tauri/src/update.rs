//! Keeps an installed Faber on the latest release without the owner doing anything.
//!
//! Faber opens, looks for a release before any work begins (a short timeout, so an offline start is not held up), and a
//! release it finds is downloaded and installed at once, then Faber opens again the way it was opened: the window for a
//! manual start, the tray for `--autostart`. Nothing is running yet, so nothing can be lost; a person who starts an answer
//! before the download ends gets the idle install below instead, and a running turn is never ended.
//!
//! Later, every six hours, a newer release is downloaded and kept. It installs at the first idle moment: no answer
//! running, no CLI install from the setup screen, and the window closed to the tray or minimized; or at once when the
//! person presses "Update ready, restart" (`restart_now`). It never shows a window on its own.
//!
//! On Windows the plugin's `install` starts the NSIS installer with `/S /UPDATE /R /ARGS <args>` (`updater.rs`
//! `updater_parameters`, `config.rs` `nsis_args`) and then ends the process with `exit(0)`. Silent (`/S`) shows no window
//! and, for the per-user installer (`bundle.windows.nsis.installMode = "currentUser"`, `RequestExecutionLevel user`),
//! needs no elevation. With `/R` the installer's `.onInstSuccess` starts the new `faber.exe` with the `/ARGS` value: the
//! arguments of this process, which the plugin copies by itself, then `installer_args` below. So a manual start comes
//! back with a window, and `--autostart` comes back in the tray.
//!
//! Where it lands is decided by the installer's `.onInit` (tauri-bundler's `installer.nsi`): unless the command line has
//! `/D=<dir>`, it starts from `$LOCALAPPDATA\Faber` and only moves to the folder of an earlier install if the registry
//! value `HKCU\Software\<publisher>\Faber` (written by `Section Install`) says so. `install` passes no `/D`, so an install
//! whose key the new installer does not find (as happened with E:\Apps\Faber) is replaced by a second copy in
//! `%LOCALAPPDATA%\Faber`. Faber therefore always tells the installer its own folder.
//!
//! Failures go to `update.log` in the data folder (`log`): a windowless app has nowhere else to say them.

use crate::autostart::{AUTOSTART_FLAG, is_autostart_arg};
use crate::{Faber, lock, main_window};
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::time::{Instant, sleep, timeout};

const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// After a check or a download that failed (offline, a PC that just woke up).
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often an idle moment is looked for while an update waits.
const LOOK_EVERY: Duration = Duration::from_secs(30);
/// The look for a release when Faber opens; an offline start waits this long at most.
const START_CHECK: Duration = Duration::from_secs(8);
/// The download at open: past this the line goes away and the update is tried again soon.
const START_DOWNLOAD: Duration = Duration::from_secs(150);
/// A download in the background has no one waiting, but a stalled one must not hold the six-hour loop forever.
const LATER_DOWNLOAD: Duration = Duration::from_secs(600);
/// A version whose install was tried is not installed unattended again for this long.
const TRIED_FOR: u64 = 24 * 3600;
/// The log keeps its last half when it grows past this.
const LOG_MAX: usize = 32 * 1024;

/// How Faber comes back after an install: with its window, or in the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reopen {
    Window,
    Tray,
}

impl Reopen {
    /// How this process was opened.
    pub(crate) fn of(args: impl IntoIterator<Item = String>) -> Self {
        if is_autostart_arg(args) { Self::Tray } else { Self::Window }
    }
}

/// What the installer gets after the plugin's own arguments. The plugin already copies this process's arguments after
/// `/ARGS`, so `--autostart` comes only when the update should come back in the tray; `/D=` is last, as NSIS wants.
pub(crate) fn installer_args(reopen: Reopen, install_dir: &OsString) -> Vec<OsString> {
    let mut args = Vec::new();
    if reopen == Reopen::Tray {
        args.push(AUTOSTART_FLAG.into());
    }
    args.push(install_dir.clone());
    args
}

/// A line of the log with nothing of the person's in it: a URL (its query can carry anything) or a path (it holds the user
/// name) becomes a marker, word by word.
fn clean(detail: &str) -> String {
    let word = |w: &str| {
        let bare = w.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | '<' | '>' | '[' | ']' | ',' | ';'));
        if bare.contains("://") {
            "<url>".to_string()
        } else if bare.contains('\\') || bare.contains('/') || bare.starts_with('~') {
            "<path>".to_string()
        } else {
            w.to_string()
        }
    };
    detail
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split(' ')
        .map(word)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

/// Adds a line to `update.log`, keeping the file small. Only fixed stage names and the updater's own short error text go
/// in, with every URL and path taken out (`clean`): never a path of the person's, a command or a credential.
pub(crate) fn log(data_dir: &Path, stage: &str, detail: &str) {
    let at = now_secs();
    let file = data_dir.join("update.log");
    let before = std::fs::read_to_string(&file).unwrap_or_default();
    let _ = std::fs::write(file, with_line(&before, &format!("{at} {stage} {}", clean(detail))));
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `log` with `line` added; past `LOG_MAX` only the newest lines stay.
fn with_line(log: &str, line: &str) -> String {
    let mut text = format!("{log}{line}\n");
    if text.len() > LOG_MAX {
        let mut from = text.len() - LOG_MAX / 2;
        while !text.is_char_boundary(from) {
            from += 1;
        }
        // Starts at a whole line.
        from += text[from..].find('\n').map_or(0, |n| n + 1);
        text = text[from..].to_string();
    }
    text
}

/// The install of a version is written down before it starts: if the installer fails (or starts and dies), Faber comes back
/// on the same version and must not try the same install again at once, one relaunch after another.
const TRIED_FILE: &str = "update-tried";

fn note_tried(data_dir: &Path, version: &str) {
    let _ = std::fs::write(data_dir.join(TRIED_FILE), format!("{version} {}", now_secs()));
}

/// Whether `text` (the tried file) says `version` was tried less than `TRIED_FOR` seconds before `now`.
fn tried_recently(text: &str, version: &str, now: u64) -> bool {
    text.split_once(' ').is_some_and(|(v, at)| v == version && at.trim().parse::<u64>().is_ok_and(|at| now.saturating_sub(at) < TRIED_FOR))
}

fn was_tried(data_dir: &Path, version: &str) -> bool {
    std::fs::read_to_string(data_dir.join(TRIED_FILE)).is_ok_and(|text| tried_recently(&text, version, now_secs()))
}

/// A downloaded update installs only when it is ready, no answer is running, and the window is not visible.
pub(crate) fn may_install(ready: bool, busy: bool, window_visible: bool) -> bool {
    ready && !busy && !window_visible
}

/// What decides, in one place, whether an install may begin: the CLI installs of the setup screen that are running, and
/// whether an install has already been decided. Starting one and deciding the other take the same lock, so a setup cannot
/// begin after the decision and an install cannot be decided in the middle of a setup.
#[derive(Debug, Default, PartialEq)]
struct Gate {
    setups: usize,
    installing: bool,
}

impl Gate {
    /// A CLI install begins; refused once an update install is decided.
    fn begin_setup(&mut self) -> bool {
        if self.installing {
            return false;
        }
        self.setups += 1;
        true
    }

    /// The install is decided unless a setup runs or `other_work` says something else does. From then on nothing new may
    /// start: the shell's own admission points read `installing`, and `other_work` is where the engine's are closed.
    fn reserve(&mut self, other_work: impl FnOnce() -> bool) -> bool {
        if self.installing || self.setups > 0 || other_work() {
            return false;
        }
        self.installing = true;
        true
    }
}

static GATE: Mutex<Gate> = Mutex::new(Gate { setups: 0, installing: false });

fn gate() -> MutexGuard<'static, Gate> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Held for the whole of a setup install (CLI downloads from the setup screen): they are work the engine does not know
/// about, and an update that ends the process in the middle of one leaves a half-extracted program and an unfinished PATH
/// change. While any is held no update installs.
pub(crate) struct SetupRunning;

impl SetupRunning {
    /// Refused with `error.update.closing` once an update install is decided.
    pub(crate) fn begin() -> Result<Self, String> {
        if gate().begin_setup() { Ok(Self) } else { Err("error.update.closing".into()) }
    }
}

impl Drop for SetupRunning {
    fn drop(&mut self) {
        let mut gate = gate();
        gate.setups = gate.setups.saturating_sub(1);
    }
}

/// An update install has been decided: the shell's missions and CLI installs start nothing from here on.
pub(crate) fn installing() -> bool {
    gate().installing
}

#[cfg(test)]
fn setups_running() -> bool {
    gate().setups > 0
}

/// The server answered, and a found update is also downloaded. A found update that did not download is a failure: it
/// is tried again soon, not in six hours.
fn fetch_succeeded(found: bool, downloaded: bool) -> bool {
    !found || downloaded
}

fn next_check_delay(succeeded: bool) -> Duration {
    if succeeded { CHECK_EVERY } else { RETRY_AFTER }
}

/// Folder `a` is folder `b` or inside it, or the other way round. Windows paths ignore case.
fn overlaps(a: &Path, b: &Path) -> bool {
    let low = |p: &Path| Path::new(&p.to_string_lossy().to_lowercase()).to_path_buf();
    let (a, b) = (low(a), low(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// A canonical Windows path (`\\?\C:\x`, `\\?\UNC\server\share\x`) as the installer and people write it (`C:\x`,
/// `\\server\share\x`).
fn plain(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

/// `C:\x` or `\\server\share`: the two shapes the installer can take after `/D=`.
fn is_dos_path(path: &str) -> bool {
    (path.starts_with(r"\\") && !path.starts_with(r"\\?\")) || path.as_bytes().get(1) == Some(&b':')
}

/// The installer argument that makes an update land in the folder of the running Faber: `/D=<folder>`. NSIS wants it
/// last, unquoted, whatever the path holds (`installer_arg` values come last on the line). None when the folder cannot be
/// written that way, or holds Faber's data: an install must never go there.
///
/// Both folders are resolved first, so a junction, a symlink, a short name (`ERICK~1`) or a `..` cannot hide that they
/// are the same place. When either cannot be resolved the answer is None: not sure means no.
pub(crate) fn install_dir_arg(exe: &Path, data_dir: &Path) -> Option<OsString> {
    let dir = plain(std::fs::canonicalize(exe.parent()?).ok()?.to_str()?);
    let data = plain(std::fs::canonicalize(data_dir).ok()?.to_str()?);
    if dir.is_empty() || dir.contains('"') || (cfg!(windows) && !is_dos_path(&dir)) || overlaps(Path::new(&dir), Path::new(&data)) {
        return None;
    }
    Some(format!("/D={dir}").into())
}

fn window_visible(app: &AppHandle) -> bool {
    main_window(app).is_some_and(|w| w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false))
}

/// What the screen shows of the update: `downloading` while Faber opens and fetches one, `ready` once one is kept for
/// an install, else `none`.
pub(crate) fn status(app: &AppHandle) -> &'static str {
    let faber = app.state::<Faber>();
    if faber.updating.load(Ordering::SeqCst) {
        "downloading"
    } else if lock(&faber.update).is_some() {
        "ready"
    } else {
        "none"
    }
}

fn announce(app: &AppHandle) {
    let _ = app.emit("faber:update", status(app));
}

/// Everything the start and the idle install need from the world, so the order of the steps is tested without a network, a
/// window or an installer.
trait Release {
    /// Looks for a newer release: its version when there is one.
    async fn check(&self) -> Result<Option<String>, String>;
    /// This version was already tried less than a day ago: it is not installed unattended again yet.
    fn tried(&self, version: &str) -> bool;
    /// Downloads the release found and keeps it ready.
    async fn download(&self) -> Result<(), String>;
    /// The screen's calm line while a start download runs.
    fn downloading(&self, on: bool);
    /// The person has typed, pasted or dropped something since Faber opened: a draft may exist only on the screen.
    fn editing(&self) -> bool;
    /// Decides the install, all at once: nothing is running (an answer, a mission, a CLI install), and from here on nothing
    /// new may start. False when anything is.
    fn reserve(&self) -> bool;
    fn shut_down(&self);
    /// Starts the installer; only a failure comes back.
    fn install(&self) -> Result<(), String>;
    fn relaunch(&self);
    /// Rebuilds the kept update to come back in the tray, which is where it will be when it installs later.
    async fn retarget(&self) -> bool;
    /// Forgets the kept update, so the next check fetches it again.
    fn discard(&self);
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Start {
    NothingNew,
    /// This version was tried less than a day ago: nothing is downloaded or installed unattended.
    Skipped,
    /// No answer from the server in time, or the download failed or took too long: tried again soon.
    Unreachable,
    Installed,
    /// Work began, or the person started typing, during the download: the update waits for the idle install.
    Waits,
    /// The installer did not start; Faber was relaunched.
    Failed,
}

impl Start {
    fn succeeded(self) -> bool {
        matches!(self, Self::NothingNew | Self::Skipped | Self::Installed | Self::Waits)
    }
}

/// Faber just opened: look for a release, download it, and install it before any work begins. If work started meanwhile,
/// or the person has typed anything (a draft lives only on the screen), nothing is ended: the update waits for an idle
/// moment, or for the person's "Update ready, restart".
async fn update_on_start(release: &impl Release, check_wait: Duration, download_wait: Duration) -> Start {
    let version = match timeout(check_wait, release.check()).await {
        Ok(Ok(Some(version))) => version,
        Ok(Ok(None)) => return Start::NothingNew,
        Ok(Err(_)) | Err(_) => return Start::Unreachable,
    };
    if release.tried(&version) {
        return Start::Skipped;
    }
    release.downloading(true);
    let downloaded = timeout(download_wait, release.download()).await;
    release.downloading(false);
    if !matches!(downloaded, Ok(Ok(()))) {
        return Start::Unreachable;
    }
    match idle_install(|| !release.editing(), || release.reserve(), || release.shut_down(), || release.install(), || release.relaunch()) {
        Idle::Installed => Start::Installed,
        Idle::Waited if release.retarget().await => Start::Waits,
        Idle::Waited => {
            release.discard();
            Start::Unreachable
        }
        Idle::Failed => Start::Failed,
    }
}

/// The real world: the plugin, the data folder and the process.
struct Live {
    app: AppHandle,
    install_dir: OsString,
    /// How Faber comes back after this install.
    reopen: Reopen,
    found: std::sync::Mutex<Option<Update>>,
    /// The engine's admission closed for the install (`Engine::local_build_admission`), held until the process ends.
    closed: std::sync::Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

impl Live {
    fn new(app: &AppHandle, install_dir: &OsString, reopen: Reopen) -> Self {
        Self { app: app.clone(), install_dir: install_dir.clone(), reopen, found: Default::default(), closed: Default::default() }
    }

    fn note(&self, stage: &str, detail: &str) {
        log(&self.app.state::<Faber>().data_dir, stage, detail);
    }
}

impl Release for Live {
    async fn check(&self) -> Result<Option<String>, String> {
        let updater = self
            .app
            .updater_builder()
            .installer_args(installer_args(self.reopen, &self.install_dir))
            .build()
            .map_err(|e| e.to_string())
            .inspect_err(|e| self.note("check-setup-failed", e))?;
        let found = updater.check().await.map_err(|e| e.to_string()).inspect_err(|e| self.note("check-failed", e))?;
        let version = found.as_ref().map(|update| update.version.clone());
        *lock(&self.found) = found;
        Ok(version)
    }

    fn tried(&self, version: &str) -> bool {
        was_tried(&self.app.state::<Faber>().data_dir, version)
    }

    async fn download(&self) -> Result<(), String> {
        let update = lock(&self.found).take().ok_or("no release was found")?;
        let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string()).inspect_err(|e| self.note("download-failed", e))?;
        *lock(&self.app.state::<Faber>().update) = Some((update, bytes));
        announce(&self.app);
        Ok(())
    }

    fn downloading(&self, on: bool) {
        self.app.state::<Faber>().updating.store(on, Ordering::SeqCst);
        announce(&self.app);
    }

    fn editing(&self) -> bool {
        self.app.state::<Faber>().touched.load(Ordering::SeqCst)
    }

    fn reserve(&self) -> bool {
        let faber = self.app.state::<Faber>();
        gate().reserve(|| {
            // The engine's own admission closes first (it refuses when an answer or a mission worker runs, and refuses every
            // new one after), then the shell's work is read: a mission being driven, the check of the agents. A mission that
            // starts after this finds the engine closed and pauses cleanly.
            let Ok(closed) = faber.engine.local_build_admission() else { return true };
            if !lock(&faber.driving).is_empty() || faber.qualifying.try_lock().is_err() {
                return true;
            }
            *lock(&self.closed) = Some(Box::new(closed));
            false
        })
    }

    fn shut_down(&self) {
        // The installer ends this process without `RunEvent::Exit`, so the cleanup Quit does happens first.
        crate::shut_down(&self.app);
    }

    fn install(&self) -> Result<(), String> {
        let (update, bytes) = lock(&self.app.state::<Faber>().update).take().ok_or("no update is ready")?;
        // Written before the installer starts: whatever happens next, this version is not tried again at once.
        note_tried(&self.app.state::<Faber>().data_dir, &update.version);
        // Only a failure comes back: the process is gone otherwise. The update is dropped; the next check fetches it again.
        update.install(bytes).map_err(|e| e.to_string()).inspect_err(|e| self.note("install-failed", e))
    }

    /// Starts Faber again and ends this process. The engine was shut down for an install that did not happen, so this
    /// process is no good any more; the new one starts clean with the same data.
    fn relaunch(&self) {
        // Releases the single-instance lock, which `RunEvent::Exit` would have done; otherwise the new process only
        // finds this one and leaves.
        tauri_plugin_single_instance::destroy(&self.app);
        // No shell, and the program is a windowless one (`windows_subsystem`), so nothing shows.
        let started = std::env::current_exe().and_then(|exe| {
            let mut command = std::process::Command::new(exe);
            if self.reopen == Reopen::Tray {
                command.arg(AUTOSTART_FLAG);
            }
            command.spawn()
        });
        if let Err(error) = started {
            self.note("relaunch-failed", &error.to_string());
        }
        std::process::exit(0)
    }

    async fn retarget(&self) -> bool {
        // A process that was opened in the tray already built the update that way.
        self.reopen == Reopen::Tray || swap_args(&self.app, &self.install_dir, Reopen::Tray).await
    }

    fn discard(&self) {
        *lock(&self.app.state::<Faber>().update) = None;
        announce(&self.app);
    }
}

/// Builds the kept update again with the arguments for `reopen`, from a fresh look at the same release; the bytes already
/// downloaded (and verified) stay. False when the release cannot be looked at again or is no longer the one kept.
async fn swap_args(app: &AppHandle, install_dir: &OsString, reopen: Reopen) -> bool {
    let live = Live::new(app, install_dir, reopen);
    let Ok(Ok(Some(version))) = timeout(START_CHECK, live.check()).await else { return false };
    let Some(twin) = lock(&live.found).take() else { return false };
    match lock(&app.state::<Faber>().update).as_mut() {
        Some((held, _)) if held.version == version => {
            *held = twin;
            true
        }
        _ => false,
    }
}

/// Looks for a newer release and downloads it. False when the check or the download failed.
async fn fetch(release: &impl Release, download_wait: Duration) -> bool {
    match release.check().await {
        Err(_) => false,
        Ok(None) => fetch_succeeded(false, false),
        // A version tried a day ago is left alone: it would only be refused again.
        Ok(Some(version)) if release.tried(&version) => true,
        Ok(Some(_)) => fetch_succeeded(true, matches!(timeout(download_wait, release.download()).await, Ok(Ok(())))),
    }
}

#[derive(Debug, PartialEq)]
enum Idle {
    /// Not idle, or something is running: nothing was touched.
    Waited,
    Installed,
    /// The installer did not start; Faber was relaunched.
    Failed,
}

/// The install in order: the cheap look, the one decision that reserves it against everything that could start, clean up,
/// install, and relaunch if the installer did not start.
fn idle_install(
    idle: impl Fn() -> bool,
    reserve: impl FnOnce() -> bool,
    shut_down: impl FnOnce(),
    install: impl FnOnce() -> Result<(), String>,
    relaunch: impl FnOnce(),
) -> Idle {
    if !idle() || !reserve() {
        return Idle::Waited;
    }
    shut_down();
    match install() {
        Ok(()) => Idle::Installed,
        Err(_) => {
            relaunch();
            Idle::Failed
        }
    }
}

fn install_dir(app: &AppHandle) -> Option<OsString> {
    let exe = std::env::current_exe().unwrap_or_default();
    install_dir_arg(&exe, &app.state::<Faber>().data_dir)
}

/// Windows only: elsewhere the installer does not restart the app, so the update waits for Quit.
fn install_when_idle(app: &AppHandle, install_dir: &OsString) {
    if !cfg!(windows) {
        return;
    }
    let faber = app.state::<Faber>();
    // `Reopen::Tray` only decides how a failed install relaunches. What the installer reopens with is in the kept update
    // itself, built by `Live::check`: the periodic fetch builds it for the tray, and an update kept from the start is
    // rebuilt that way (`Release::retarget`) before it waits here.
    let live = Live::new(app, install_dir, Reopen::Tray);
    let version = || lock(&faber.update).as_ref().map(|(update, _)| update.version.clone());
    let idle = || version().is_some_and(|v| may_install(true, false, window_visible(app)) && !live.tried(&v));
    idle_install(idle, || live.reserve(), || live.shut_down(), || live.install(), || live.relaunch());
}

/// "Update ready, restart": installs the kept update now, with the window as it is. Refused while an answer runs.
pub(crate) async fn restart_now(app: AppHandle) -> Result<(), String> {
    if !cfg!(windows) || lock(&app.state::<Faber>().update).is_none() {
        return Err("error.update.none".into());
    }
    let dir = install_dir(&app).ok_or("error.update.failed")?;
    let reopen = Reopen::of(std::env::args());
    let live = Live::new(&app, &dir, reopen);
    // The kept update was built to come back in the tray. A process that was opened with its window comes back with it.
    if reopen == Reopen::Window {
        swap_args(&app, &dir, Reopen::Window).await;
    }
    // The person chose this, so a version tried a day ago does not stop it.
    match idle_install(|| true, || live.reserve(), || live.shut_down(), || live.install(), || live.relaunch()) {
        Idle::Installed => Ok(()),
        Idle::Waited => Err("error.update.busy".into()),
        Idle::Failed => Err("error.update.failed".into()),
    }
}

pub(crate) fn keep_updated(app: &AppHandle) {
    let Some(install_dir) = install_dir(app) else {
        log(&app.state::<Faber>().data_dir, "no-safe-folder", "Faber stays on this version");
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut next_check = Instant::now();
        // Elsewhere the installer does not restart the app, so an update waits for Quit and is only fetched below.
        if cfg!(windows) {
            let live = Live::new(&app, &install_dir, Reopen::of(std::env::args()));
            let start = update_on_start(&live, START_CHECK, START_DOWNLOAD).await;
            log(&app.state::<Faber>().data_dir, "start", &format!("{start:?}"));
            next_check = Instant::now() + next_check_delay(start.succeeded());
        }
        loop {
            // A downloaded update is kept and installed first; the next one is looked for after it.
            if lock(&app.state::<Faber>().update).is_none() && Instant::now() >= next_check {
                next_check = Instant::now() + next_check_delay(fetch(&Live::new(&app, &install_dir, Reopen::Tray), LATER_DOWNLOAD).await);
            }
            install_when_idle(&app, &install_dir);
            sleep(LOOK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{CHECK_EVERY, RETRY_AFTER, TRIED_FOR, clean, next_check_delay, plain, tried_recently, update_on_start, with_line};
    use super::{Gate, Idle, LOG_MAX, Release, Reopen, SetupRunning, Start, fetch, fetch_succeeded, idle_install, installer_args};
    use super::{is_dos_path, may_install, setups_running};
    use std::cell::{Cell, RefCell};
    use std::ffi::OsString;
    use std::time::Duration;

    #[test]
    fn a_ready_update_waits_while_an_answer_runs() {
        assert!(!may_install(true, true, false));
    }

    #[test]
    fn a_ready_update_installs_when_idle_with_the_window_hidden() {
        assert!(may_install(true, false, false));
    }

    #[test]
    fn a_ready_update_waits_while_the_window_is_visible() {
        assert!(!may_install(true, false, true));
        assert!(!may_install(true, true, true));
    }

    #[test]
    fn nothing_installs_without_a_downloaded_update() {
        assert!(!may_install(false, false, false));
    }

    #[test]
    fn a_setup_install_counts_as_work_until_it_ends() {
        assert!(!setups_running());
        let first = SetupRunning::begin().unwrap();
        let second = SetupRunning::begin().unwrap();
        assert!(setups_running());
        drop(first);
        assert!(setups_running(), "one install is still running");
        drop(second);
        assert!(!setups_running());
    }

    #[test]
    fn the_install_is_decided_once_and_nothing_starts_after_it() {
        let mut gate = Gate::default();
        assert!(gate.begin_setup(), "a setup may start before the decision");
        assert!(!gate.reserve(|| false), "a setup that runs stops the install");
        gate.setups = 0;
        assert!(!gate.reserve(|| true), "work the shell or the engine reports stops it too");
        assert!(!gate.installing, "a refused decision reserves nothing");
        assert!(gate.reserve(|| false));
        assert!(!gate.begin_setup(), "no CLI install may begin once the install is decided");
        assert!(!gate.reserve(|| false), "and it is decided only once");
        assert_eq!(gate.setups, 0);
    }

    /// The install in order, with `reserve` answering as given.
    fn run(idle: bool, reserve: bool, install: Result<(), String>) -> (Idle, Vec<&'static str>) {
        let seen = RefCell::new(Vec::new());
        let outcome = idle_install(
            || idle,
            || {
                seen.borrow_mut().push("reserve");
                reserve
            },
            || seen.borrow_mut().push("shut down"),
            || {
                seen.borrow_mut().push("install");
                install
            },
            || seen.borrow_mut().push("relaunch"),
        );
        (outcome, seen.into_inner())
    }

    #[test]
    fn an_idle_install_reserves_cleans_up_then_installs() {
        assert_eq!(run(true, true, Ok(())), (Idle::Installed, vec!["reserve", "shut down", "install"]));
    }

    #[test]
    fn nothing_is_closed_unless_the_one_decision_is_made() {
        assert_eq!(run(false, true, Ok(())), (Idle::Waited, vec![]), "a visible window never gets as far as the decision");
        assert_eq!(run(true, false, Ok(())), (Idle::Waited, vec!["reserve"]), "work that began stops it before anything is closed");
    }

    #[test]
    fn an_installer_that_does_not_start_leaves_a_relaunched_faber_not_a_dead_one() {
        let (outcome, seen) = run(true, true, Err("blocked".into()));
        assert_eq!((outcome, seen), (Idle::Failed, vec!["reserve", "shut down", "install", "relaunch"]));
    }

    #[test]
    fn a_found_update_that_did_not_download_is_tried_again_soon() {
        assert!(fetch_succeeded(false, false), "nothing new is a success");
        assert!(fetch_succeeded(true, true));
        assert!(!fetch_succeeded(true, false));
        assert_eq!(next_check_delay(false), RETRY_AFTER);
        assert_eq!(next_check_delay(true), CHECK_EVERY);
    }

    #[test]
    fn a_verbatim_path_is_written_as_the_installer_reads_it() {
        assert_eq!(plain(r"\\?\E:\Apps\Faber"), r"E:\Apps\Faber");
        assert_eq!(plain(r"\\?\UNC\server\share\Faber"), r"\\server\share\Faber");
        assert_eq!(plain(r"E:\Apps\Faber"), r"E:\Apps\Faber");
        assert!(is_dos_path(r"E:\Apps") && is_dos_path(r"\\server\share\x"));
        assert!(!is_dos_path(r"\\?\Volume{1234}\x"));
    }

    /// A release with nothing real behind it: it records what the start did, in order.
    struct Fake {
        found: Result<Option<String>, String>,
        /// The check never answers (a network that is down without saying so).
        hangs: bool,
        /// The download never ends.
        stalls: bool,
        download: Result<(), String>,
        /// A turn or a mission begins while the download runs.
        work_during_download: bool,
        /// The person types while the download runs.
        typing_during_download: bool,
        /// This version was tried less than a day ago.
        tried: bool,
        install: Result<(), String>,
        retarget: bool,
        busy: Cell<bool>,
        editing: Cell<bool>,
        seen: RefCell<Vec<&'static str>>,
    }

    impl Fake {
        fn found() -> Self {
            Self {
                found: Ok(Some("1.2.3".into())),
                hangs: false,
                stalls: false,
                download: Ok(()),
                work_during_download: false,
                typing_during_download: false,
                tried: false,
                install: Ok(()),
                retarget: true,
                busy: Cell::new(false),
                editing: Cell::new(false),
                seen: RefCell::new(vec![]),
            }
        }
        fn saw(&self) -> Vec<&'static str> {
            self.seen.borrow().clone()
        }
    }

    impl Release for Fake {
        async fn check(&self) -> Result<Option<String>, String> {
            self.seen.borrow_mut().push("check");
            if self.hangs {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            self.found.clone()
        }
        fn tried(&self, _: &str) -> bool {
            self.tried
        }
        async fn download(&self) -> Result<(), String> {
            self.seen.borrow_mut().push("download");
            if self.stalls {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            self.busy.set(self.work_during_download);
            self.editing.set(self.typing_during_download);
            self.download.clone()
        }
        fn downloading(&self, on: bool) {
            self.seen.borrow_mut().push(if on { "line on" } else { "line off" });
        }
        fn editing(&self) -> bool {
            self.editing.get()
        }
        fn reserve(&self) -> bool {
            self.seen.borrow_mut().push("reserve");
            !self.busy.get()
        }
        fn shut_down(&self) {
            self.seen.borrow_mut().push("shut down");
        }
        fn install(&self) -> Result<(), String> {
            self.seen.borrow_mut().push("install");
            self.install.clone()
        }
        fn relaunch(&self) {
            self.seen.borrow_mut().push("relaunch");
        }
        async fn retarget(&self) -> bool {
            self.seen.borrow_mut().push("retarget");
            self.retarget
        }
        fn discard(&self) {
            self.seen.borrow_mut().push("discard");
        }
    }

    const WAIT: Duration = Duration::from_millis(50);

    async fn start(release: &Fake) -> Start {
        update_on_start(release, WAIT, WAIT).await
    }

    #[tokio::test]
    async fn a_release_found_at_start_is_downloaded_and_installed_before_any_work() {
        let release = Fake::found();
        assert_eq!(start(&release).await, Start::Installed);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off", "reserve", "shut down", "install"]);
    }

    #[tokio::test]
    async fn nothing_new_at_start_touches_nothing() {
        let release = Fake { found: Ok(None), ..Fake::found() };
        assert_eq!(start(&release).await, Start::NothingNew);
        assert_eq!(release.saw(), ["check"]);
    }

    #[tokio::test]
    async fn an_offline_start_gives_up_in_time_and_tries_again_soon() {
        let refused = Fake { found: Err("offline".into()), ..Fake::found() };
        assert_eq!(start(&refused).await, Start::Unreachable);
        assert_eq!(refused.saw(), ["check"]);
        let silent = Fake { hangs: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&silent).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5), "the start does not wait for a dead network");
        assert_eq!(silent.saw(), ["check"]);
        assert!(!Start::Unreachable.succeeded() && !Start::Failed.succeeded());
        assert!(Start::NothingNew.succeeded() && Start::Installed.succeeded() && Start::Waits.succeeded() && Start::Skipped.succeeded());
    }

    #[tokio::test]
    async fn a_turn_or_a_mission_started_during_the_download_is_never_ended() {
        let release = Fake { work_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(
            release.saw(),
            ["check", "line on", "download", "line off", "reserve", "retarget"],
            "no cleanup, no installer, no relaunch; the kept update is rebuilt for the tray"
        );
    }

    #[tokio::test]
    async fn typing_during_the_download_keeps_the_draft_and_defers_the_install() {
        let release = Fake { typing_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off", "retarget"], "not even the decision is made");
    }

    #[tokio::test]
    async fn an_update_that_waits_is_rebuilt_for_the_tray_or_dropped() {
        let release = Fake { work_during_download: true, retarget: false, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Unreachable);
        assert_eq!(release.saw().last(), Some(&"discard"), "an update that would open a window from the tray is not kept");
    }

    #[tokio::test]
    async fn a_download_that_failed_installs_nothing() {
        let release = Fake { download: Err("cut".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Unreachable);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off"]);
    }

    #[tokio::test]
    async fn a_download_that_never_ends_is_given_up_and_the_line_goes_away() {
        let release = Fake { stalls: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&release).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(release.saw(), ["check", "line on", "download", "line off"]);
        // A download in the background is bounded too, so the six-hour loop is never held for good.
        assert!(!fetch(&Fake { stalls: true, ..Fake::found() }, WAIT).await);
    }

    #[tokio::test]
    async fn an_installer_that_does_not_start_leaves_a_relaunched_faber() {
        let release = Fake { install: Err("blocked".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Failed);
        assert_eq!(release.saw().last(), Some(&"relaunch"));
    }

    #[tokio::test]
    async fn a_version_tried_a_day_ago_is_not_downloaded_or_installed_again_at_start() {
        let release = Fake { tried: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Skipped);
        assert_eq!(release.saw(), ["check"], "the relaunch after a failed install does not start the same install again");
        let later = Fake { tried: true, ..Fake::found() };
        assert!(fetch(&later, WAIT).await);
        assert_eq!(later.saw(), ["check"], "nor does the periodic check download it");
    }

    #[test]
    fn a_tried_version_is_remembered_for_a_day_and_only_that_version() {
        let now = 1_000_000;
        assert!(tried_recently(&format!("0.3.15 {}", now - 60), "0.3.15", now));
        assert!(tried_recently(&format!("0.3.15 {}", now - TRIED_FOR + 1), "0.3.15", now));
        assert!(!tried_recently(&format!("0.3.15 {}", now - TRIED_FOR), "0.3.15", now), "a day later it is tried again");
        assert!(!tried_recently(&format!("0.3.14 {}", now - 60), "0.3.15", now), "a newer version is a new chance");
        assert!(!tried_recently("garbage", "0.3.15", now) && !tried_recently("", "0.3.15", now));
    }

    #[test]
    fn faber_comes_back_the_way_it_was_opened() {
        let dir: OsString = r"/D=E:\Apps\Faber".into();
        assert_eq!(Reopen::of(["faber.exe".to_string()]), Reopen::Window, "a manual start");
        assert_eq!(Reopen::of(["faber.exe".to_string(), "--autostart".to_string()]), Reopen::Tray, "the Windows login start");
        assert_eq!(installer_args(Reopen::Window, &dir), std::slice::from_ref(&dir), "no autostart flag for a window");
        assert_eq!(installer_args(Reopen::Tray, &dir), [OsString::from("--autostart"), dir.clone()]);
        assert_eq!(installer_args(Reopen::Tray, &dir).last(), Some(&dir), "NSIS wants /D= last");
    }

    #[test]
    fn the_log_stays_small_and_keeps_the_newest_lines_whole() {
        let mut text = String::new();
        for n in 0..5000 {
            text = with_line(&text, &format!("{n} check-failed some error text"));
        }
        assert!(text.len() <= LOG_MAX, "bounded");
        assert!(text.ends_with("4999 check-failed some error text\n"), "the newest line is kept");
        assert!(text.starts_with(|c: char| c.is_ascii_digit()) && !text.starts_with("heck"), "starts at a whole line");
    }

    #[test]
    fn the_log_holds_no_url_query_and_no_path_of_the_person() {
        let url = clean("error sending request for url (https://example.com/latest.json?token=abc123&u=erick): timed out");
        assert!(!url.contains("token") && !url.contains("example.com") && url.contains("<url>") && url.contains("timed out"), "{url}");
        let temp = clean(r#"The system cannot find "C:\Users\Erick Silva\AppData\Local\Temp\faber-setup.exe" (os error 2)"#);
        assert!(!temp.contains("Erick") && !temp.contains("Silva") && !temp.contains("Temp") && temp.contains("<path>"), "{temp}");
        assert!(!clean("failed at /home/erick/.cache/x").contains("erick") && !clean(r"\\server\share\x").contains("share"));
        assert_eq!(clean("Access is denied. (os error 5)"), "Access is denied. (os error 5)", "plain text is kept");
    }

    // The folder tests need real folders, because the guard resolves them; they are Windows-only like the feature.
    #[cfg(windows)]
    mod folders {
        use super::super::{install_dir_arg, overlaps, plain};
        use std::path::Path;

        #[test]
        fn the_update_goes_into_the_folder_of_the_running_faber() {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("data");
            let spaced = root.path().join("Program Files").join("Faber");
            std::fs::create_dir_all(&data).unwrap();
            std::fs::create_dir_all(&spaced).unwrap();
            let arg = install_dir_arg(&spaced.join("faber.exe"), &data).unwrap().into_string().unwrap();
            let want = plain(std::fs::canonicalize(&spaced).unwrap().to_str().unwrap());
            assert_eq!(arg, format!("/D={want}"), "NSIS reads the rest of the line, and quotes are not allowed");
            assert!(arg.ends_with(r"Program Files\Faber") && !arg.contains(r"\\?\"));
        }

        #[test]
        fn an_update_never_goes_where_faber_keeps_its_data() {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("Faber");
            let inside = data.join("bin");
            std::fs::create_dir_all(&inside).unwrap();
            assert!(install_dir_arg(&data.join("faber.exe"), &data).is_none(), "same folder");
            assert!(install_dir_arg(&inside.join("faber.exe"), &data).is_none(), "inside the data");
            assert!(install_dir_arg(&root.path().join("faber.exe"), &data).is_none(), "holding the data");
        }

        #[test]
        fn a_dotdot_or_a_junction_does_not_hide_the_data_folder() {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("Faber");
            std::fs::create_dir_all(data.join("sub")).unwrap();
            let roundabout = data.join("sub").join("..");
            assert!(install_dir_arg(&roundabout.join("faber.exe"), &data).is_none(), "..");
            let link = root.path().join("shortcut");
            junction::create(&data, &link).unwrap();
            assert!(install_dir_arg(&link.join("faber.exe"), &data).is_none(), "a junction to the data");
            assert!(install_dir_arg(&data.join("faber.exe"), &link).is_none(), "the data reached through a junction");
        }

        #[test]
        fn a_folder_that_cannot_be_resolved_is_refused() {
            let root = tempfile::tempdir().unwrap();
            let there = root.path().join("here");
            std::fs::create_dir_all(&there).unwrap();
            assert!(install_dir_arg(&root.path().join("missing").join("faber.exe"), &there).is_none(), "no program folder");
            assert!(install_dir_arg(&there.join("faber.exe"), &root.path().join("missing")).is_none(), "no data folder");
        }

        #[test]
        fn the_installers_default_folder_is_not_the_default_data_folder() {
            // The per-user installer starts from $LOCALAPPDATA\Faber; the data lives under the config folder (Roaming).
            let data = crate::default_data_dir(Path::new(r"C:\Users\a\AppData\Roaming"));
            assert!(!overlaps(Path::new(r"C:\Users\a\AppData\Local\Faber"), &data));
        }
    }
}
