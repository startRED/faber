//! The updater as an app runs it: the real world behind `flow::Release` (the release reader, the data folder, the process),
//! the loop that keeps Faber updated, and the few calls the screen reaches through the shell.

use crate::flow::{Idle, Release, fetch, idle_install, update_on_start};
use crate::gate::{Who, admit, gate};
use crate::{Meter, Phase, Status, install_dir_arg, installer_path, log, may_install, release, tried};
use std::any::Any;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::time::{Instant, sleep};

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
/// The order to end the app is a line on its way to the shell; the process ends only after this, so the line is out.
const QUIT_GRACE: Duration = Duration::from_millis(300);

/// What only the app knows, and what only it can do. The updater decides; the app answers and carries out.
pub trait App: Send + Sync + 'static {
    /// Whether the person turned previews on (the preview channel is followed).
    fn previews(&self) -> bool;
    /// Whether the window is on screen and not minimized, or about to be: nothing closes it under the person.
    fn window_visible(&self) -> bool;
    /// Closes the engine admission for the install: refused (None) while an answer runs or a message waits, and every new
    /// one after, until what is returned is dropped. For `Who::Owner` (the person own restart) work that runs does not
    /// refuse it: it is cut. The guard is held until the process ends.
    fn close_engine(&self, who: Who) -> Option<Box<dyn Any + Send>>;
    /// The clean up the install needs before the process ends, as Quit does it. `resume` notes the work that runs, so it goes
    /// on after the restart (the person own restart).
    fn shut_down(&self, resume: bool);
    /// Ends the app at once, without waiting for the updater: the installer is about to replace it.
    fn quit(&self);
    /// Tells the screen where the update is.
    fn emit(&self, status: Status);
}

/// How Faber comes back after an install: with its window, or in the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reopen {
    Window,
    Tray,
}

impl Reopen {
    /// The way Faber was opened, seen from its window: one that is not on screen at open is the login start, in the tray.
    pub fn of(window_visible: bool) -> Self {
        if window_visible { Self::Window } else { Self::Tray }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A downloaded update, kept on the disk until it installs: only its place and version stay in memory.
struct Ready {
    version: String,
    installer: PathBuf,
    #[cfg_attr(not(windows), allow(dead_code))]
    signature: String,
}

struct Inner {
    app: Arc<dyn App>,
    /// The running version, which a release must beat.
    current: semver::Version,
    data_dir: PathBuf,
    /// The installer `/D=<folder>` argument; only the Windows install (`handoff`) reads it.
    #[cfg_attr(not(windows), allow(dead_code))]
    install_dir: OsString,
    /// The install at open, as the screen shows it: looking, downloading (how far), installing.
    stage: Mutex<Status>,
    ready: Mutex<Option<Ready>>,
    /// The person has typed, pasted or dropped something since Faber opened: a draft may exist only on the screen, so the
    /// install at open waits for an idle moment.
    touched: AtomicBool,
    /// Some field on the screen holds something typed or attached and not sent: no unattended install closes the window then.
    draft_dirty: AtomicBool,
}

impl Inner {
    fn note(&self, stage: &str, detail: &str) {
        log(&self.data_dir, stage, detail);
    }

    fn status(&self) -> Status {
        let stage = *lock(&self.stage);
        if stage.phase != Phase::None {
            stage
        } else if lock(&self.ready).is_some() {
            Status::of(Phase::Ready)
        } else {
            Status::default()
        }
    }

    fn announce(&self) {
        self.app.emit(self.status());
    }

    fn set_stage(&self, phase: Phase) {
        *lock(&self.stage) = Status::of(phase);
        self.announce();
    }

    /// A download progress, kept only while the install at open shows one: the periodic fetch downloads in silence.
    fn report(&self, status: Status) {
        {
            let mut stage = lock(&self.stage);
            if stage.phase != Phase::Downloading {
                return;
            }
            *stage = status;
        }
        self.announce();
    }
}

fn next_check_delay(succeeded: bool) -> Duration {
    if succeeded { CHECK_EVERY } else { RETRY_AFTER }
}

fn newer_than_ready(version: &str, ready: Option<&Ready>) -> bool {
    let Ok(candidate) = semver::Version::parse(version) else { return false };
    ready.is_none_or(|ready| semver::Version::parse(&ready.version).is_ok_and(|old| candidate > old))
}

/// The real world: the release reader, the data folder and the process.
struct Live {
    inner: Arc<Inner>,
    /// How Faber comes back after this install.
    #[cfg_attr(not(windows), allow(dead_code))]
    reopen: Reopen,
    found: Mutex<Option<release::Found>>,
    /// The engine admission closed for the install (`App::close_engine`), held until the process ends.
    closed: Mutex<Option<Box<dyn Any + Send>>>,
}

impl Live {
    fn new(inner: &Arc<Inner>, reopen: Reopen) -> Self {
        Self { inner: inner.clone(), reopen, found: Mutex::default(), closed: Mutex::default() }
    }

    /// Writes the failure to `update.log` and hands it back.
    fn failed(&self, stage: &str, error: String) -> String {
        self.inner.note(stage, &error);
        error
    }
}

impl Release for Live {
    async fn check(&self) -> Result<Option<String>, String> {
        let key = release::platform_key().ok_or_else(|| self.failed("check-setup-failed", "no installer for this system".into()))?;
        let previews = self.inner.app.previews();
        let manifest = release::manifest(previews).await.map_err(|e| self.failed("check-failed", e))?;
        let found = release::newer(&manifest, key, &self.inner.current, previews)
            .map_err(|e| self.failed("check-failed", e))?
            .filter(|found| newer_than_ready(&found.version, lock(&self.inner.ready).as_ref()));
        let version = found.as_ref().map(|found| found.version.clone());
        *lock(&self.found) = found;
        Ok(version)
    }

    fn tried(&self, version: &str) -> bool {
        tried::was_tried(&self.inner.data_dir, version)
    }

    async fn download(&self) -> Result<(), String> {
        let found = lock(&self.found).take().ok_or("no release was found")?;
        let installer = installer_path(&found.version).map_err(|e| self.failed("download-failed", e.to_string()))?;
        let (inner, mut meter) = (self.inner.clone(), Meter::default());
        let progress = move |chunk: u64, total: Option<u64>| {
            if let Some(status) = meter.advance(chunk, total, std::time::Instant::now()) {
                inner.report(status);
            }
        };
        release::download(&found.platform, &found.version, &installer, progress).await.map_err(|e| self.failed("download-failed", e))?;
        // A newer download replaces the one kept before, which would never install now.
        let before = lock(&self.inner.ready).replace(Ready {
            version: found.version,
            signature: found.platform.signature,
            installer: installer.clone(),
        });
        if let Some(before) = before.filter(|before| before.installer != installer) {
            release::discard(&before.installer);
        }
        self.inner.announce();
        Ok(())
    }

    fn stage(&self, phase: Phase) {
        self.inner.set_stage(phase);
    }

    fn reserve(&self, who: Who) -> bool {
        let inner = &self.inner;
        let guard = admit(
            &mut gate(),
            who,
            || inner.app.close_engine(who),
            || inner.draft_dirty.load(Ordering::SeqCst) || (who == Who::Start && inner.touched.load(Ordering::SeqCst)),
            || match lock(&inner.ready).as_ref() {
                Some(ready) => tried::note_tried(&inner.data_dir, &ready.version),
                None => Err(std::io::Error::other("no update is ready")),
            },
            || {
                let _ = std::fs::remove_file(inner.data_dir.join(tried::TRIED_FILE));
            },
        );
        let Some(guard) = guard else { return false };
        *lock(&self.closed) = Some(guard);
        true
    }

    fn shut_down(&self) {
        // The installer ends this process without a Quit, so the cleanup Quit does happens first.
        self.inner.app.shut_down(false);
    }

    fn install(&self) -> Result<(), String> {
        let ready = lock(&self.inner.ready).take().ok_or("no update is ready")?;
        // The tried marker was written when the install was decided (`reserve`), before anything was closed.
        // Only a failure comes back: the process is gone otherwise. The update is dropped; the next check fetches it again.
        #[cfg(windows)]
        {
            let (inner, tray) = (&self.inner, self.reopen == Reopen::Tray);
            crate::handoff::start(&ready.installer, &ready.signature, &ready.version, &inner.install_dir, &inner.data_dir, tray)
                .map_err(|e| self.failed("install-failed", e))?;
            self.end()
        }
        #[cfg(not(windows))]
        {
            let _ = ready;
            Err(self.failed("install-failed", "updates install on Windows only".into()))
        }
    }

    /// Opens Faber again when the install did not start, and ends this process. The engine was shut down for an install that
    /// did not happen, so this process is no good any more; the new one starts clean with the same data.
    fn relaunch(&self) {
        #[cfg(windows)]
        {
            if let Err(error) = crate::handoff::relaunch(&self.inner.install_dir, &self.inner.data_dir, self.reopen == Reopen::Tray) {
                self.inner.note("relaunch-failed", &error);
            }
        }
        self.end()
    }
}

impl Live {
    /// The app ends at once, then this process: the copy of the program that runs the installer waits for both.
    fn end(&self) -> ! {
        self.inner.app.quit();
        std::thread::sleep(QUIT_GRACE);
        std::process::exit(0)
    }
}

/// Windows only: elsewhere the installer does not restart the app.
fn install_when_idle(inner: &Arc<Inner>) {
    if !cfg!(windows) {
        return;
    }
    // Nobody is looking at the window: Faber comes back in the tray.
    let live = Live::new(inner, Reopen::Tray);
    let version = || lock(&inner.ready).as_ref().map(|ready| ready.version.clone());
    let idle = || version().is_some_and(|v| may_install(true, false, inner.app.window_visible()) && !live.tried(&v));
    idle_install(idle, || live.reserve(Who::Later), || live.shut_down(), || live.install(), || live.relaunch());
}

async fn keep_updated(inner: Arc<Inner>) {
    let mut next_check = Instant::now();
    // Windows only: elsewhere the installer does not restart the app, so nothing is installed here at all.
    if cfg!(windows) {
        let live = Live::new(&inner, Reopen::of(inner.app.window_visible()));
        let start = update_on_start(&live, START_CHECK, START_DOWNLOAD).await;
        inner.note("start", &format!("{start:?}"));
        next_check = Instant::now() + next_check_delay(start.succeeded());
    }
    loop {
        if Instant::now() >= next_check {
            next_check = Instant::now() + next_check_delay(fetch(&Live::new(&inner, Reopen::Tray), START_CHECK, LATER_DOWNLOAD).await);
        }
        install_when_idle(&inner);
        sleep(LOOK_EVERY).await;
    }
}

/// The updater of one running app. Cheap to clone; every clone is the same updater.
#[derive(Clone)]
pub struct Updater(Arc<Inner>);

impl Updater {
    /// `version` is the running app version (a release must beat it), `install_dir` the folder of its main program
    /// (`faber.exe`). Refused, with the reason in `update.log`, when the version is not one, when the folder cannot be handed
    /// to the installer safely (`install_dir_arg`: Faber stays on this version), or off Windows, where nothing installs yet.
    pub fn new(app: Arc<dyn App>, version: &str, data_dir: PathBuf, install_dir: &Path) -> Result<Self, String> {
        let refuse = |stage: &str, why: &str| {
            log(&data_dir, stage, why);
            Err(why.to_string())
        };
        if !cfg!(windows) {
            return refuse("not-supported", "updates install on Windows only");
        }
        let Ok(current) = semver::Version::parse(version.trim_start_matches('v')) else {
            return refuse("bad-version", "the running version is not a version");
        };
        let Some(install_dir) = install_dir_arg(&install_dir.join("faber.exe"), &data_dir) else {
            return refuse("no-safe-folder", "Faber stays on this version");
        };
        Ok(Self(Arc::new(Inner {
            app,
            current,
            data_dir,
            install_dir,
            stage: Mutex::default(),
            ready: Mutex::default(),
            touched: AtomicBool::new(false),
            draft_dirty: AtomicBool::new(false),
        })))
    }

    /// Where the update is, for the screen.
    pub fn status(&self) -> Status {
        self.0.status()
    }

    /// The screen saw the first input of the session: a draft may exist, so the install at open waits.
    pub fn input_seen(&self) {
        self.0.touched.store(true, Ordering::SeqCst);
    }

    /// Whether any field on the screen holds something typed or attached and not sent.
    pub fn draft_dirty(&self, dirty: bool) {
        self.0.draft_dirty.store(dirty, Ordering::SeqCst);
    }

    /// Starts keeping Faber updated on the current runtime: clears what earlier installs left, shows the opening screen at
    /// once (before the screen asks, so its first paint is already it), and looks for a release.
    pub fn start(&self) {
        #[cfg(windows)]
        crate::handoff::sweep();
        if cfg!(windows) {
            self.0.set_stage(Phase::Checking);
        }
        tokio::spawn(keep_updated(self.0.clone()));
    }

    /// "Update ready, restart": installs the kept update now, with the window as it is. Work that runs is cut, noted first so
    /// it goes on after the restart.
    pub async fn restart_now(&self) -> Result<(), String> {
        if !cfg!(windows) || lock(&self.0.ready).is_none() {
            return Err("error.update.none".into());
        }
        // The person pressed it in the window, so the window comes back, and the screen says so at once: nothing is looked up
        // first. A version tried a day ago does not stop it.
        let live = Live::new(&self.0, Reopen::Window);
        live.stage(Phase::Installing);
        let shut_down = || self.0.app.shut_down(true);
        match idle_install(|| true, || live.reserve(Who::Owner), shut_down, || live.install(), || live.relaunch()) {
            Idle::Installed => Ok(()),
            Idle::Waited => {
                live.stage(Phase::None);
                Err("error.update.busy".into())
            }
            Idle::Failed => {
                live.stage(Phase::None);
                Err("error.update.failed".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::fetch_succeeded;

    /// An app with nothing real behind it: it records what the screen would be told.
    #[derive(Default)]
    struct Fake {
        told: Mutex<Vec<Status>>,
        visible: AtomicBool,
    }

    impl App for Fake {
        fn previews(&self) -> bool {
            false
        }
        fn window_visible(&self) -> bool {
            self.visible.load(Ordering::SeqCst)
        }
        fn close_engine(&self, _: Who) -> Option<Box<dyn Any + Send>> {
            None
        }
        fn shut_down(&self, _: bool) {}
        fn quit(&self) {}
        fn emit(&self, status: Status) {
            lock(&self.told).push(status);
        }
    }

    fn updater(app: &Arc<Fake>) -> (Updater, tempfile::TempDir, tempfile::TempDir) {
        let (data, program) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let updater = Updater::new(app.clone(), "0.4.0", data.path().to_path_buf(), program.path()).unwrap();
        (updater, data, program)
    }

    #[test]
    fn faber_comes_back_the_way_it_was_opened() {
        assert_eq!(Reopen::of(true), Reopen::Window, "a manual start has its window");
        assert_eq!(Reopen::of(false), Reopen::Tray, "the login start has none");
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
    fn a_waiting_update_is_only_superseded_by_a_newer_version() {
        let ready = Ready { version: "1.2.3".into(), installer: PathBuf::from("old"), signature: String::new() };
        assert!(newer_than_ready("1.2.4", Some(&ready)));
        assert!(!newer_than_ready("1.2.3", Some(&ready)));
        assert!(!newer_than_ready("1.2.2", Some(&ready)));
        assert!(!newer_than_ready("invalid", Some(&ready)));
        assert!(newer_than_ready("1.2.4", None));
    }

    #[cfg(windows)]
    #[test]
    fn an_updater_needs_a_version_and_a_folder_that_is_safe_to_install_into() {
        let app = Arc::new(Fake::default());
        let (data, program) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(Updater::new(app.clone(), "0.4.0", data.path().into(), program.path()).is_ok());
        assert!(Updater::new(app.clone(), "v0.4.0-preview.2", data.path().into(), program.path()).is_ok(), "a leading v is read");
        let bad = Updater::new(app.clone(), "not a version", data.path().into(), program.path());
        assert!(bad.is_err() && std::fs::read_to_string(data.path().join("update.log")).unwrap().contains("bad-version"));
        let inside = data.path().join("app");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(Updater::new(app.clone(), "0.4.0", data.path().into(), &inside).is_err(), "never into the data folder");
        assert!(Updater::new(app.clone(), "0.4.0", data.path().into(), &data.path().join("missing")).is_err(), "not sure means no");
        assert!(std::fs::read_to_string(data.path().join("update.log")).unwrap().contains("no-safe-folder"));
    }

    #[cfg(windows)]
    #[test]
    fn the_screen_is_told_each_phase_and_a_kept_update_shows_as_ready() {
        let app = Arc::new(Fake::default());
        let (updater, _data, _program) = updater(&app);
        assert_eq!(updater.status(), Status::default());
        updater.0.set_stage(Phase::Checking);
        assert_eq!(updater.status(), Status::of(Phase::Checking));
        updater.0.set_stage(Phase::None);
        *lock(&updater.0.ready) = Some(Ready { version: "0.4.1".into(), signature: String::new(), installer: PathBuf::from("x") });
        updater.0.announce();
        assert_eq!(updater.status(), Status::of(Phase::Ready), "no stage, but an update is kept");
        updater.0.set_stage(Phase::Installing);
        assert_eq!(updater.status(), Status::of(Phase::Installing), "the stage wins while it lasts");
        let told: Vec<_> = lock(&app.told).iter().map(|status| status.phase).collect();
        assert_eq!(told, [Phase::Checking, Phase::None, Phase::Ready, Phase::Installing]);
    }

    #[cfg(windows)]
    #[test]
    fn download_progress_is_shown_only_while_the_install_at_open_is_downloading() {
        let app = Arc::new(Fake::default());
        let (updater, _data, _program) = updater(&app);
        let percent = |n| Status { percent: Some(n), ..Status::of(Phase::Downloading) };
        updater.0.report(percent(10));
        assert!(lock(&app.told).is_empty() && updater.status() == Status::default(), "the periodic fetch downloads in silence");
        updater.0.set_stage(Phase::Downloading);
        updater.0.report(percent(42));
        assert_eq!(updater.status(), percent(42));
        assert_eq!(lock(&app.told).last(), Some(&percent(42)));
    }

    #[cfg(windows)]
    #[test]
    fn typing_and_a_dirty_draft_are_remembered_for_the_admission() {
        let app = Arc::new(Fake::default());
        let (updater, _data, _program) = updater(&app);
        let (touched, dirty) = (&updater.0.touched, &updater.0.draft_dirty);
        assert!(!touched.load(Ordering::SeqCst) && !dirty.load(Ordering::SeqCst));
        updater.input_seen();
        updater.draft_dirty(true);
        assert!(touched.load(Ordering::SeqCst) && dirty.load(Ordering::SeqCst));
        updater.draft_dirty(false);
        assert!(touched.load(Ordering::SeqCst) && !dirty.load(Ordering::SeqCst), "touching is for the whole session, a draft is not");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn the_restart_button_without_a_kept_update_answers_none_and_touches_nothing() {
        let app = Arc::new(Fake::default());
        let (updater, _data, _program) = updater(&app);
        assert_eq!(updater.restart_now().await, Err("error.update.none".into()));
        assert!(lock(&app.told).is_empty(), "the screen was not told of an install that never began");
    }

    #[cfg(windows)]
    #[test]
    fn a_replaced_installer_goes_with_its_folder() {
        let folder = tempfile::tempdir().unwrap();
        let old = folder.path().join("old").join("Faber-setup.exe");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"MZ").unwrap();
        release::discard(&old);
        assert!(!old.exists() && !old.parent().unwrap().exists(), "the file and its own folder go");
        release::discard(&old);
    }
}
