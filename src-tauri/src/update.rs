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
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Adds a line to `update.log`, keeping the file small. Only fixed stage names and the updater's own short error text go
/// in: never a path of the person's, a command or a credential.
pub(crate) fn log(data_dir: &Path, stage: &str, detail: &str) {
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let detail: String = detail.chars().filter(|c| !c.is_control()).take(200).collect();
    let file = data_dir.join("update.log");
    let before = std::fs::read_to_string(&file).unwrap_or_default();
    let _ = std::fs::write(file, with_line(&before, &format!("{at} {stage} {detail}")));
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

/// A downloaded update installs only when nothing can be lost or seen: it is ready, no answer is running, and the
/// window is not visible.
pub(crate) fn may_install(ready: bool, busy: bool, window_visible: bool) -> bool {
    ready && !busy && !window_visible
}

/// How many CLI installs from the setup screen are running. They are work the engine does not know about: an update
/// that ends the process in the middle of one leaves a half-extracted program and an unfinished PATH change.
static SETUPS_RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Held for the whole of a setup install; while any is held, no update installs.
pub(crate) struct SetupRunning;

impl SetupRunning {
    pub(crate) fn begin() -> Self {
        SETUPS_RUNNING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for SetupRunning {
    fn drop(&mut self) {
        SETUPS_RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

fn setups_running() -> bool {
    SETUPS_RUNNING.load(Ordering::SeqCst) > 0
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
    /// Looks for a newer release; true when there is one.
    async fn check(&self) -> Result<bool, String>;
    /// Downloads the release found and keeps it ready.
    async fn download(&self) -> Result<(), String>;
    /// The screen's calm line while a start download runs.
    fn downloading(&self, on: bool);
    /// An answer or a CLI install is running.
    fn working(&self) -> bool;
    fn shut_down(&self);
    /// Starts the installer; only a failure comes back.
    fn install(&self) -> Result<(), String>;
    fn relaunch(&self);
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Start {
    NothingNew,
    /// No answer from the server in time, or the download failed: tried again soon.
    Unreachable,
    Installed,
    /// A turn began during the download: the update waits for the idle install.
    Waits,
    /// The installer did not start; Faber was relaunched.
    Failed,
}

impl Start {
    fn succeeded(self) -> bool {
        matches!(self, Self::NothingNew | Self::Installed | Self::Waits)
    }
}

/// Faber just opened: look for a release, download it, and install it before any work begins. If a turn started meanwhile
/// it is left alone and the update waits for an idle moment.
async fn update_on_start(release: &impl Release, wait: Duration) -> Start {
    match timeout(wait, release.check()).await {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) => return Start::NothingNew,
        Ok(Err(_)) | Err(_) => return Start::Unreachable,
    }
    release.downloading(true);
    let downloaded = release.download().await;
    release.downloading(false);
    if downloaded.is_err() {
        return Start::Unreachable;
    }
    match idle_install(|| !release.working(), || release.shut_down(), || release.install(), || release.relaunch()) {
        Idle::Installed => Start::Installed,
        Idle::Waited => Start::Waits,
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
}

impl Live {
    fn new(app: &AppHandle, install_dir: &OsString, reopen: Reopen) -> Self {
        Self { app: app.clone(), install_dir: install_dir.clone(), reopen, found: Default::default() }
    }

    fn note(&self, stage: &str, detail: &str) {
        log(&self.app.state::<Faber>().data_dir, stage, detail);
    }
}

impl Release for Live {
    async fn check(&self) -> Result<bool, String> {
        let updater = self
            .app
            .updater_builder()
            .installer_args(installer_args(self.reopen, &self.install_dir))
            .build()
            .map_err(|e| e.to_string())
            .inspect_err(|e| self.note("check-setup-failed", e))?;
        let found = updater.check().await.map_err(|e| e.to_string()).inspect_err(|e| self.note("check-failed", e))?;
        let any = found.is_some();
        *lock(&self.found) = found;
        Ok(any)
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

    fn working(&self) -> bool {
        self.app.state::<Faber>().engine.is_working() || setups_running()
    }

    fn shut_down(&self) {
        // The installer ends this process without `RunEvent::Exit`, so the cleanup Quit does happens first.
        crate::shut_down(&self.app);
    }

    fn install(&self) -> Result<(), String> {
        let (update, bytes) = lock(&self.app.state::<Faber>().update).take().ok_or("no update is ready")?;
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
}

/// Looks for a newer release and downloads it. False when the check or the download failed.
async fn fetch(release: &impl Release) -> bool {
    match release.check().await {
        Err(_) => false,
        Ok(false) => fetch_succeeded(false, false),
        Ok(true) => fetch_succeeded(true, release.download().await.is_ok()),
    }
}

#[derive(Debug, PartialEq)]
enum Idle {
    /// Not idle at one of the two looks: nothing was touched.
    Waited,
    Installed,
    /// The installer did not start; Faber was relaunched.
    Failed,
}

/// The install in order: look, look again, clean up, install, and relaunch if the installer did not start.
///
/// ponytail: the second look narrows the gap between the decision and the end of the process to microseconds, it does not
/// close it (the person can still click the tray icon in that gap); a lock between the window and this task would.
fn idle_install(
    idle: impl Fn() -> bool,
    shut_down: impl FnOnce(),
    install: impl FnOnce() -> Result<(), String>,
    relaunch: impl FnOnce(),
) -> Idle {
    if !idle() || !idle() {
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
    // Hidden in the tray, so it comes back in the tray.
    let live = Live::new(app, install_dir, Reopen::Tray);
    let idle = || lock(&faber.update).is_some() && may_install(true, live.working(), window_visible(app));
    idle_install(idle, || live.shut_down(), || live.install(), || live.relaunch());
}

/// "Update ready, restart": installs the kept update now, with the window as it is. Refused while an answer runs.
pub(crate) async fn restart_now(app: AppHandle) -> Result<(), String> {
    if !cfg!(windows) || lock(&app.state::<Faber>().update).is_none() {
        return Err("error.update.none".into());
    }
    let dir = install_dir(&app).ok_or("error.update.failed")?;
    let reopen = Reopen::of(std::env::args());
    let live = Live::new(&app, &dir, reopen);
    // The kept update was built to come back in the tray. A process that was opened with its window comes back with it:
    // the same release is looked for again with those arguments, and the bytes already downloaded are reused.
    if reopen == Reopen::Window && matches!(timeout(START_CHECK, live.check()).await, Ok(Ok(true))) {
        let twin = lock(&live.found).take();
        if let (Some(twin), Some((held, _))) = (twin, lock(&app.state::<Faber>().update).as_mut())
            && held.version == twin.version
        {
            *held = twin;
        }
    }
    match idle_install(|| !live.working(), || live.shut_down(), || live.install(), || live.relaunch()) {
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
            let start = update_on_start(&Live::new(&app, &install_dir, Reopen::of(std::env::args())), START_CHECK).await;
            log(&app.state::<Faber>().data_dir, "start", &format!("{start:?}"));
            next_check = Instant::now() + next_check_delay(start.succeeded());
        }
        loop {
            // A downloaded update is kept and installed first; the next one is looked for after it.
            if lock(&app.state::<Faber>().update).is_none() && Instant::now() >= next_check {
                next_check = Instant::now() + next_check_delay(fetch(&Live::new(&app, &install_dir, Reopen::Tray)).await);
            }
            install_when_idle(&app, &install_dir);
            sleep(LOOK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{CHECK_EVERY, RETRY_AFTER, setups_running};
    use super::{Idle, Release, Reopen, SetupRunning, Start, fetch_succeeded, idle_install, installer_args, is_dos_path, may_install};
    use super::{LOG_MAX, next_check_delay, plain, update_on_start, with_line};
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
        let first = SetupRunning::begin();
        let second = SetupRunning::begin();
        assert!(setups_running());
        drop(first);
        assert!(setups_running(), "one install is still running");
        drop(second);
        assert!(!setups_running());
    }

    fn run(looks: &[bool], install: Result<(), String>) -> (Idle, Vec<&'static str>) {
        let (seen, step) = (RefCell::new(Vec::new()), Cell::new(0));
        let outcome = idle_install(
            || {
                let look = looks[step.get().min(looks.len() - 1)];
                step.set(step.get() + 1);
                look
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
    fn an_idle_install_cleans_up_then_installs() {
        assert_eq!(run(&[true], Ok(())), (Idle::Installed, vec!["shut down", "install"]));
    }

    #[test]
    fn a_window_opened_after_the_first_look_stops_the_install_before_anything_is_closed() {
        assert_eq!(run(&[true, false], Ok(())), (Idle::Waited, vec![]));
        assert_eq!(run(&[false], Ok(())), (Idle::Waited, vec![]));
    }

    #[test]
    fn an_installer_that_does_not_start_leaves_a_relaunched_faber_not_a_dead_one() {
        let (outcome, seen) = run(&[true], Err("blocked".into()));
        assert_eq!((outcome, seen), (Idle::Failed, vec!["shut down", "install", "relaunch"]));
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
        found: Result<bool, String>,
        /// The check never answers (a network that is down without saying so).
        hangs: bool,
        download: Result<(), String>,
        /// A turn begins while the download runs.
        turn_during_download: bool,
        install: Result<(), String>,
        busy: Cell<bool>,
        seen: RefCell<Vec<&'static str>>,
    }

    impl Fake {
        fn found() -> Self {
            Self {
                found: Ok(true),
                hangs: false,
                download: Ok(()),
                turn_during_download: false,
                install: Ok(()),
                busy: Cell::new(false),
                seen: RefCell::new(vec![]),
            }
        }
        fn saw(&self) -> Vec<&'static str> {
            self.seen.borrow().clone()
        }
    }

    impl Release for Fake {
        async fn check(&self) -> Result<bool, String> {
            self.seen.borrow_mut().push("check");
            if self.hangs {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            self.found.clone()
        }
        async fn download(&self) -> Result<(), String> {
            self.seen.borrow_mut().push("download");
            self.busy.set(self.turn_during_download);
            self.download.clone()
        }
        fn downloading(&self, on: bool) {
            self.seen.borrow_mut().push(if on { "line on" } else { "line off" });
        }
        fn working(&self) -> bool {
            self.busy.get()
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
    }

    const WAIT: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn a_release_found_at_start_is_downloaded_and_installed_before_any_work() {
        let release = Fake::found();
        assert_eq!(update_on_start(&release, WAIT).await, Start::Installed);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off", "shut down", "install"]);
    }

    #[tokio::test]
    async fn nothing_new_at_start_touches_nothing() {
        let release = Fake { found: Ok(false), ..Fake::found() };
        assert_eq!(update_on_start(&release, WAIT).await, Start::NothingNew);
        assert_eq!(release.saw(), ["check"]);
    }

    #[tokio::test]
    async fn an_offline_start_gives_up_in_time_and_tries_again_soon() {
        let refused = Fake { found: Err("offline".into()), ..Fake::found() };
        assert_eq!(update_on_start(&refused, WAIT).await, Start::Unreachable);
        assert_eq!(refused.saw(), ["check"]);
        let silent = Fake { hangs: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(update_on_start(&silent, WAIT).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5), "the start does not wait for a dead network");
        assert_eq!(silent.saw(), ["check"]);
        assert!(!Start::Unreachable.succeeded() && !Start::Failed.succeeded());
        assert!(Start::NothingNew.succeeded() && Start::Installed.succeeded() && Start::Waits.succeeded());
    }

    #[tokio::test]
    async fn a_turn_started_during_the_download_is_never_ended() {
        let release = Fake { turn_during_download: true, ..Fake::found() };
        assert_eq!(update_on_start(&release, WAIT).await, Start::Waits);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off"], "no cleanup, no installer, no relaunch");
    }

    #[tokio::test]
    async fn a_download_that_failed_installs_nothing() {
        let release = Fake { download: Err("cut".into()), ..Fake::found() };
        assert_eq!(update_on_start(&release, WAIT).await, Start::Unreachable);
        assert_eq!(release.saw(), ["check", "line on", "download", "line off"]);
    }

    #[tokio::test]
    async fn an_installer_that_does_not_start_leaves_a_relaunched_faber() {
        let release = Fake { install: Err("blocked".into()), ..Fake::found() };
        assert_eq!(update_on_start(&release, WAIT).await, Start::Failed);
        assert_eq!(release.saw().last(), Some(&"relaunch"));
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
