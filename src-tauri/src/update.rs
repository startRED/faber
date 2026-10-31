//! Keeps an installed Faber on the latest release without the owner doing anything.
//!
//! Faber opens, looks for a release before any work begins (a short timeout, so an offline start is not held up), and a
//! release it finds is downloaded and installed at once, then Faber opens again the way it was opened: the window for a
//! manual start, the tray for `--autostart`. Nothing is running yet, so nothing can be lost; a person who starts an answer
//! before the download ends gets the idle install below instead, and a running turn is never ended.
//!
//! Later, every six hours, a newer release is downloaded and kept. It installs at the first idle moment: no answer
//! running, no CLI install from the setup screen, and the window closed to the tray or minimized; or
//! at once when the person presses "Update ready, restart" (`restart_now`). It never shows a window on its own.
//!
//! On Windows the install is handed to a copy of Faber (`handoff`), which waits until this process has ended, runs the NSIS
//! installer with `/S /UPDATE /D=<folder>` and opens Faber again: with its window for a manual start or the person's
//! "Update ready, restart", in the tray for `--autostart` and the idle install. Silent (`/S`) shows no window and, for the
//! per-user installer (`bundle.windows.nsis.installMode = "currentUser"`, `RequestExecutionLevel user`), needs no
//! elevation. The plugin's own `install` is used only at Quit on other systems.
//!
//! Where it lands is decided by the installer's `.onInit` (tauri-bundler's `installer.nsi`): unless the command line has
//! `/D=<dir>`, it starts from `$LOCALAPPDATA\Faber` and only moves to the folder of an earlier install if the registry
//! value `HKCU\Software\<publisher>\Faber` (written by `Section Install`) says so. `install` passes no `/D`, so an install
//! whose key the new installer does not find (as happened with E:\Apps\Faber) is replaced by a second copy in
//! `%LOCALAPPDATA%\Faber`. Faber therefore always tells the installer its own folder.
//!
//! Failures go to `update.log` in the data folder (`log`): a windowless app has nowhere else to say them.

#[cfg(windows)]
pub(crate) mod handoff;

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

/// The preview channel's manifest: a release with a fixed tag whose `latest.json` every release replaces with the newest build
/// (.github/workflows/release.yml, R156). The stable manifest is the endpoint in `tauri.conf.json`, which every install ever
/// shipped already reads; it is left alone so those installs keep working.
const PREVIEW_FEED: &str = "https://github.com/startRED/faber/releases/download/preview-channel/latest.json";

/// The manifest to read when `previews` is on, or none when the configured stable one stays.
fn preview_feed(previews: bool) -> Option<tauri::Url> {
    previews.then(|| PREVIEW_FEED.parse().expect("the preview feed is a valid address"))
}

/// A release is installed only when it is newer than the running version (R156). With previews on that is plain version
/// order. With previews off a running pre-release takes only the next stable line: the weekly stable `X.Y.Z` is rebuilt from
/// whichever preview was a day old, which can be older than the `X.Y.Z-preview.N` that runs, so the same `X.Y.Z` is not
/// proof of newer code. Only a strictly greater `X.Y.Z` is. A stable install is never held back by this.
fn is_upgrade(current: &semver::Version, remote: &semver::Version, previews: bool) -> bool {
    let same_line = (remote.major, remote.minor, remote.patch) <= (current.major, current.minor, current.patch);
    remote > current && (previews || current.pre.is_empty() || !same_line)
}

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
    log_to(data_dir, "update.log", stage, detail);
}

/// How long each start took to show its window (`main.rs` `reveal`), kept small like `update.log`.
pub(crate) const STARTUP_LOG: &str = "startup.log";

/// `log` into another small file of the data folder.
pub(crate) fn log_to(data_dir: &Path, name: &str, stage: &str, detail: &str) {
    let at = now_secs();
    let file = data_dir.join(name);
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

/// Written and flushed to the disk: an install that cannot leave this marker behind is not started unattended, because a
/// failing installer would then bring Faber back to try it again, one relaunch after another.
fn note_tried(data_dir: &Path, version: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(data_dir.join(TRIED_FILE))?;
    file.write_all(format!("{version} {}", now_secs()).as_bytes())?;
    file.sync_all()
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
    /// start: a CLI install is refused (`begin_setup`), and `other_work` is where the engine's admission is closed.
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

/// Who asks for the install.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Who {
    /// Faber just opened and the update was downloaded.
    Start,
    /// The idle install, once an update is kept.
    Later,
    /// "Update ready, restart": the person's own choice. Work that runs does not hold it back: it is cut and goes on after.
    Owner,
}

/// The one admission of an install, all inside the gate's lock. `close` shuts the engine and the shell's work (None when
/// something runs). Then, still under the lock and as late as possible, an unattended install reads whether the person has
/// an unsent draft (`editing`, which the start also reads as any input since Faber opened) and whether its tried marker
/// reached the disk (`mark`): in either case it gives way and the engine reopens. The draft is read once more after the
/// marker is flushed, the last thing before the lock opens and the shutdown starts; a refusal there takes the marker back
/// (`unmark`), so a draft does not cost the update a day. The person's own restart ignores all of it.
fn admit<G>(
    gate: &mut Gate,
    who: Who,
    close: impl FnOnce() -> Option<G>,
    mut editing: impl FnMut() -> bool,
    mark: impl FnOnce() -> std::io::Result<()>,
    unmark: impl FnOnce(),
) -> Option<G> {
    let mut closed = None;
    gate.reserve(|| {
        let Some(guard) = close() else { return true };
        if who == Who::Owner {
            let _ = mark();
        } else if editing() || mark().is_err() {
            return true;
        } else if editing() {
            unmark();
            return true;
        }
        closed = Some(guard);
        false
    });
    closed
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

/// A window still waiting for its first paint is about to be seen, so it counts as visible: nothing closes it under the person.
fn counts_as_visible(revealed: bool, visible: bool, minimized: bool) -> bool {
    !revealed || (visible && !minimized)
}

fn window_visible(app: &AppHandle) -> bool {
    let revealed = app.state::<Faber>().revealed.load(Ordering::SeqCst);
    main_window(app).is_some_and(|w| counts_as_visible(revealed, w.is_visible().unwrap_or(true), w.is_minimized().unwrap_or(false)))
}

/// Where the update is, for the screen. `Checking`, `Downloading` and `Installing` belong to the install at open (and
/// `Installing` to "Update ready, restart"); `Ready` is an update kept for an install.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Phase {
    #[default]
    None,
    Checking,
    Downloading,
    Installing,
    Ready,
}

/// What the screen shows of the update: the phase and, while downloading, how far it is: a whole percent when the server
/// said how big the file is, else the megabytes so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Status {
    phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mb: Option<u32>,
}

impl Status {
    fn of(phase: Phase) -> Self {
        Self { phase, ..Self::default() }
    }

    fn downloading(done: u64, total: Option<u64>) -> Self {
        match total.filter(|total| *total > 0) {
            Some(total) => {
                Self { percent: Some((u128::from(done) * 100 / u128::from(total)).min(100) as u8), ..Self::of(Phase::Downloading) }
            }
            None => Self { mb: Some((done / 1_000_000).min(u64::from(u32::MAX)) as u32), ..Self::of(Phase::Downloading) },
        }
    }
}

/// The screen hears of a download at most this often, and only when what it shows has changed: the chunks come many times
/// a second, and an event for each would be a flood.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Turns the download's chunk callbacks into the few updates the screen needs.
#[derive(Default)]
struct Meter {
    done: u64,
    shown: Option<Status>,
    at: Option<std::time::Instant>,
}

impl Meter {
    /// A chunk of `chunk` bytes arrived; `total` is the file's size when the server gave one. The status to show, when it is
    /// time and it differs from what is shown.
    fn advance(&mut self, chunk: u64, total: Option<u64>, now: std::time::Instant) -> Option<Status> {
        self.done += chunk;
        let status = Status::downloading(self.done, total);
        let due = self.at.is_none_or(|at| now.saturating_duration_since(at) >= PROGRESS_EVERY);
        if !due || self.shown == Some(status) {
            return None;
        }
        self.at = Some(now);
        self.shown = Some(status);
        Some(status)
    }
}

pub(crate) fn status(app: &AppHandle) -> Status {
    let faber = app.state::<Faber>();
    let stage = *lock(&faber.stage);
    if stage.phase != Phase::None {
        stage
    } else if lock(&faber.update).is_some() {
        Status::of(Phase::Ready)
    } else {
        Status::default()
    }
}

fn set_stage(app: &AppHandle, phase: Phase) {
    *lock(&app.state::<Faber>().stage) = Status::of(phase);
    announce(app);
}

/// A download's progress, kept only while the install at open shows one: the periodic fetch downloads in silence.
fn report(app: &AppHandle, status: Status) {
    let faber = app.state::<Faber>();
    {
        let mut stage = lock(&faber.stage);
        if stage.phase != Phase::Downloading {
            return;
        }
        *stage = status;
    }
    announce(app);
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
    /// What the screen shows of the install at open (`Phase::None` when it ends).
    fn stage(&self, phase: Phase);
    /// Decides the install, all at once (`admit`): nothing is running (an answer, a CLI
    /// install), no unsent draft is on the screen, the tried marker is on the disk, and from here on nothing new may start.
    /// False when anything stops it.
    fn reserve(&self, who: Who) -> bool;
    fn shut_down(&self);
    /// Starts the installer; only a failure comes back.
    fn install(&self) -> Result<(), String>;
    fn relaunch(&self);
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
    release.stage(Phase::Checking);
    let start = start_steps(release, check_wait, download_wait).await;
    // An install ends the process; anything else gives the screen back to the person.
    if start != Start::Installed {
        release.stage(Phase::None);
    }
    start
}

async fn start_steps(release: &impl Release, check_wait: Duration, download_wait: Duration) -> Start {
    let version = match timeout(check_wait, release.check()).await {
        Ok(Ok(Some(version))) => version,
        Ok(Ok(None)) => return Start::NothingNew,
        Ok(Err(_)) | Err(_) => return Start::Unreachable,
    };
    if release.tried(&version) {
        return Start::Skipped;
    }
    release.stage(Phase::Downloading);
    if !matches!(timeout(download_wait, release.download()).await, Ok(Ok(()))) {
        return Start::Unreachable;
    }
    release.stage(Phase::Installing);
    match idle_install(|| true, || release.reserve(Who::Start), || release.shut_down(), || release.install(), || release.relaunch()) {
        Idle::Installed => Start::Installed,
        // How Faber comes back is decided when it installs (`handoff`), so the kept update waits as it is.
        Idle::Waited => Start::Waits,
        Idle::Failed => Start::Failed,
    }
}

/// The real world: the plugin, the data folder and the process.
struct Live {
    app: AppHandle,
    /// The installer's `/D=<folder>`; only the Windows install (`handoff`) reads it.
    #[cfg_attr(not(windows), allow(dead_code))]
    install_dir: OsString,
    /// How Faber comes back after this install.
    reopen: Reopen,
    found: std::sync::Mutex<Option<Update>>,
    /// The engine's admission closed for the install (`Engine::install_admission`), held until the process ends.
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
        let previews = self.app.state::<Faber>().engine.settings().previews == Some(true);
        let mut builder =
            self.app.updater_builder().version_comparator(move |current, remote| is_upgrade(&current, &remote.version, previews));
        if let Some(feed) = preview_feed(previews) {
            builder = builder.endpoints(vec![feed]).map_err(|e| e.to_string()).inspect_err(|e| self.note("check-setup-failed", e))?;
        }
        let updater = builder.build().map_err(|e| e.to_string()).inspect_err(|e| self.note("check-setup-failed", e))?;
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
        let (app, mut meter) = (self.app.clone(), Meter::default());
        let progress = move |chunk: usize, total: Option<u64>| {
            if let Some(status) = meter.advance(chunk as u64, total, std::time::Instant::now()) {
                report(&app, status);
            }
        };
        let bytes = update.download(progress, || {}).await.map_err(|e| e.to_string()).inspect_err(|e| self.note("download-failed", e))?;
        *lock(&self.app.state::<Faber>().update) = Some((update, bytes));
        announce(&self.app);
        Ok(())
    }

    fn stage(&self, phase: Phase) {
        set_stage(&self.app, phase);
    }

    fn reserve(&self, who: Who) -> bool {
        let faber = self.app.state::<Faber>();
        let guard = admit(
            &mut gate(),
            who,
            || {
                // The engine's own admission closes first (it refuses when an answer runs, and every new one after), then
                // whether it is still working is read. A CLI install is a `SetupRunning` and is read by the gate itself.
                // The person's own restart is held back only by an install the agent makes.
                if who == Who::Owner {
                    return faber.engine.install_admission_stopping().ok();
                }
                let closed = faber.engine.install_admission().ok()?;
                (!faber.engine.is_working()).then_some(closed)
            },
            || faber.draft_dirty.load(Ordering::SeqCst) || (who == Who::Start && faber.touched.load(Ordering::SeqCst)),
            || match lock(&faber.update).as_ref() {
                Some((update, _)) => note_tried(&faber.data_dir, &update.version),
                None => Err(std::io::Error::other("no update is ready")),
            },
            || {
                let _ = std::fs::remove_file(faber.data_dir.join(TRIED_FILE));
            },
        );
        let Some(guard) = guard else { return false };
        *lock(&self.closed) = Some(Box::new(guard));
        true
    }

    fn shut_down(&self) {
        // The installer ends this process without `RunEvent::Exit`, so the cleanup Quit does happens first.
        crate::shut_down(&self.app);
    }

    fn install(&self) -> Result<(), String> {
        let (update, bytes) = lock(&self.app.state::<Faber>().update).take().ok_or("no update is ready")?;
        // The tried marker was written when the install was decided (`reserve`), before anything was closed.
        // Only a failure comes back: the process is gone otherwise. The update is dropped; the next check fetches it again.
        #[cfg(windows)]
        {
            let data_dir = &self.app.state::<Faber>().data_dir;
            handoff::start(&bytes, &update.version, &self.install_dir, data_dir, self.reopen == Reopen::Tray)
                .inspect_err(|e| self.note("install-failed", e))?;
            std::process::exit(0)
        }
        #[cfg(not(windows))]
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
    // Nobody is looking at the window: Faber comes back in the tray.
    let live = Live::new(app, install_dir, Reopen::Tray);
    let version = || lock(&faber.update).as_ref().map(|(update, _)| update.version.clone());
    let idle = || version().is_some_and(|v| may_install(true, false, window_visible(app)) && !live.tried(&v));
    idle_install(idle, || live.reserve(Who::Later), || live.shut_down(), || live.install(), || live.relaunch());
}

/// "Update ready, restart": installs the kept update now, with the window as it is. Work that runs is cut, noted first so
/// it goes on after the restart.
pub(crate) async fn restart_now(app: AppHandle) -> Result<(), String> {
    if !cfg!(windows) || lock(&app.state::<Faber>().update).is_none() {
        return Err("error.update.none".into());
    }
    let dir = install_dir(&app).ok_or("error.update.failed")?;
    // The person pressed it in the window, so the window comes back, and the screen says so at once: nothing is looked up
    // first. A version tried a day ago does not stop it.
    let live = Live::new(&app, &dir, Reopen::Window);
    live.stage(Phase::Installing);
    let shut_down = || {
        app.state::<Faber>().engine.note_resume();
        live.shut_down();
    };
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

pub(crate) fn keep_updated(app: &AppHandle) {
    let Some(install_dir) = install_dir(app) else {
        log(&app.state::<Faber>().data_dir, "no-safe-folder", "Faber stays on this version");
        return;
    };
    let app = app.clone();
    #[cfg(windows)]
    handoff::sweep();
    // Set before the screen asks, so its first paint is already the opening screen (the start check begins right after).
    if cfg!(windows) {
        set_stage(&app, Phase::Checking);
    }
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
    use super::{Gate, Idle, LOG_MAX, Release, Reopen, SetupRunning, Start, Who, admit, fetch, fetch_succeeded, idle_install, note_tried};
    use super::{Meter, PROGRESS_EVERY, Phase, Status, counts_as_visible};
    use super::{PREVIEW_FEED, is_dos_path, is_upgrade, may_install, preview_feed, setups_running};

    fn mb(n: u32) -> Status {
        Status { mb: Some(n), ..Status::of(Phase::Downloading) }
    }

    fn percent(n: u8) -> Status {
        Status { percent: Some(n), ..Status::of(Phase::Downloading) }
    }

    #[test]
    fn progress_is_a_whole_percent_of_the_size_and_never_passes_a_hundred() {
        assert_eq!(Status::downloading(0, Some(200)), percent(0));
        assert_eq!(Status::downloading(84, Some(200)), percent(42));
        assert_eq!(Status::downloading(199, Some(200)), percent(99), "rounds down: 100 means done");
        assert_eq!(Status::downloading(250, Some(200)), percent(100), "a server that undercounts does not show 125");
        assert_eq!(Status::downloading(u64::MAX, Some(u64::MAX)), percent(100), "no overflow on huge sizes");
    }

    #[test]
    fn an_unknown_size_shows_the_megabytes_so_far() {
        assert_eq!(Status::downloading(12_500_000, None), mb(12));
        assert_eq!(Status::downloading(999_999, None), mb(0));
        assert_eq!(Status::downloading(12_500_000, Some(0)), mb(12), "a size of zero is no size");
    }

    #[test]
    fn the_screen_hears_a_whole_percent_at_most_four_times_a_second() {
        let (mut meter, start) = (Meter::default(), std::time::Instant::now());
        // 1000 chunks of 1 byte in 100 ms: the first is shown at once, the rest wait for the next quarter second.
        let shown: Vec<_> =
            (0..1000u32).filter_map(|i| meter.advance(1, Some(1000), start + Duration::from_micros(u64::from(i) * 100))).collect();
        assert_eq!(shown, [percent(0)]);
        // A quarter of a second later the percent has moved, and it is shown once.
        assert_eq!(meter.advance(0, Some(1000), start + PROGRESS_EVERY + Duration::from_millis(100)), Some(percent(100)));
        assert_eq!(meter.advance(0, Some(1000), start + Duration::from_secs(5)), None, "the same percent is not said twice");
    }

    #[test]
    fn a_slow_download_says_each_new_value_once_and_a_fast_one_never_floods() {
        let (mut meter, start) = (Meter::default(), std::time::Instant::now());
        let mut told = 0;
        // 100 MB with no size, in 64 KB chunks over 10 seconds: the screen hears at most 40 times, each a new megabyte count.
        let mut last = None;
        for i in 0..1600u64 {
            if let Some(status) = meter.advance(65_536, None, start + Duration::from_micros(i * 6_250)) {
                assert_ne!(Some(status), last);
                last = Some(status);
                told += 1;
            }
        }
        assert!((30..=41).contains(&told), "{told} events for 10 seconds");
        assert!(last.is_some_and(|status| status.mb >= Some(100)), "{last:?}");
    }

    #[test]
    fn a_window_waiting_for_its_first_paint_counts_as_seen() {
        assert!(counts_as_visible(false, false, false), "not revealed yet: nothing closes it");
        assert!(counts_as_visible(true, true, false));
        assert!(!counts_as_visible(true, false, false), "closed to the tray");
        assert!(!counts_as_visible(true, true, true), "minimized");
    }

    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    fn v(text: &str) -> semver::Version {
        text.parse().unwrap()
    }

    #[test]
    fn previews_off_keeps_the_configured_stable_manifest_and_on_reads_the_preview_one() {
        assert_eq!(preview_feed(false), None, "off leaves the endpoint of tauri.conf.json, the one every install already reads");
        let feed = preview_feed(true).unwrap();
        assert_eq!(feed.as_str(), PREVIEW_FEED);
        assert_eq!(feed.scheme(), "https");
        assert!(feed.path().ends_with("/preview-channel/latest.json"), "a fixed tag, never the newest release");
    }

    #[test]
    fn the_stable_manifest_is_still_the_latest_release() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            conf["plugins"]["updater"]["endpoints"],
            serde_json::json!(["https://github.com/startRED/faber/releases/latest/download/latest.json"]),
            "installs shipped so far read this address; changing it strands them"
        );
        assert_ne!(PREVIEW_FEED, conf["plugins"]["updater"]["endpoints"][0].as_str().unwrap());
    }

    #[test]
    fn a_release_installs_only_when_it_is_newer_so_nothing_ever_goes_back() {
        // Turned off while running a preview: the stable manifest holds the older stable.
        assert!(!is_upgrade(&v("0.3.16-preview.2"), &v("0.3.15"), false), "off never downgrades");
        assert!(!is_upgrade(&v("0.3.17-preview.1"), &v("0.3.16"), false));
        assert!(!is_upgrade(&v("0.3.16"), &v("0.3.16"), false));
        assert!(!is_upgrade(&v("0.3.16"), &v("0.3.16"), true));
        // Forward moves with previews on: the next preview, the stable of the preview that runs, the next stable.
        assert!(is_upgrade(&v("0.3.16-preview.1"), &v("0.3.16-preview.2"), true));
        assert!(is_upgrade(&v("0.3.16-preview.9"), &v("0.3.16"), true), "the preview feed holds the newest build");
        assert!(is_upgrade(&v("0.3.15"), &v("0.3.16"), true));
        assert!(is_upgrade(&v("0.3.15"), &v("0.3.16-preview.1"), true), "previews on offers the next preview to a stable install");
        assert!(is_upgrade(&v("0.3.16"), &v("0.3.17-preview.1"), true));
        // Previews off: a stable install follows the stable line as ever.
        assert!(is_upgrade(&v("0.3.15"), &v("0.3.16"), false));
        assert!(is_upgrade(&v("0.3.16"), &v("0.3.17"), false));
    }

    #[test]
    fn previews_off_never_takes_the_stable_of_the_version_that_runs_as_a_preview() {
        // The stable 0.3.16 may be built from preview.2, so it is not newer code than a running preview.3 (R156).
        assert!(!is_upgrade(&v("0.3.16-preview.3"), &v("0.3.16"), false), "same X.Y.Z: not proven newer");
        assert!(!is_upgrade(&v("0.3.16-preview.1"), &v("0.3.16"), false));
        // Only the next stable line is.
        assert!(is_upgrade(&v("0.3.16-preview.3"), &v("0.3.17"), false));
        assert!(is_upgrade(&v("0.3.16-preview.3"), &v("0.4.0"), false));
    }

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

    /// `admit` with the given answers; the guard is a plain string so a dropped one shows in `dropped`.
    fn admission(gate: &mut Gate, who: Who, closes: bool, editing: bool, marks: bool, seen: &RefCell<Vec<&'static str>>) -> bool {
        admission_late(gate, who, (closes, editing, false), marks, seen)
    }

    /// Like `admission`, with the draft read once more after the marker (`late`).
    fn admission_late(
        gate: &mut Gate,
        who: Who,
        (closes, editing, late): (bool, bool, bool),
        marks: bool,
        seen: &RefCell<Vec<&'static str>>,
    ) -> bool {
        let reads = Cell::new(0);
        admit(
            gate,
            who,
            || {
                seen.borrow_mut().push("close");
                closes.then_some("closed")
            },
            || {
                seen.borrow_mut().push("read draft");
                reads.set(reads.get() + 1);
                if reads.get() == 1 { editing } else { late }
            },
            || {
                seen.borrow_mut().push("mark");
                if marks { Ok(()) } else { Err(std::io::Error::other("disk")) }
            },
            || seen.borrow_mut().push("unmark"),
        )
        .is_some()
    }

    #[test]
    fn an_unattended_install_gives_way_to_an_unsent_draft_and_the_owner_does_not() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission(&mut gate, who, true, true, true, &seen), "{who:?}");
            assert!(!gate.installing, "a refused install decides nothing, so the window and the draft stay");
            assert!(admission(&mut gate, who, true, false, true, &seen), "{who:?}: an empty draft does not stop it");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Owner, true, true, true, &seen), "\"Update ready, restart\" is the person's own choice");
    }

    #[test]
    fn the_draft_is_read_inside_the_decision_after_the_engine_closed() {
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Later, true, false, true, &seen));
        assert_eq!(
            *seen.borrow(),
            ["close", "read draft", "mark", "read draft"],
            "the draft is read again after the marker, as the last thing under the lock"
        );
        let (mut busy, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(!admission(&mut busy, Who::Later, false, false, true, &seen));
        assert_eq!(*seen.borrow(), ["close"], "work that runs stops it before the draft or the marker are touched");
    }

    #[test]
    fn input_during_the_marker_sync_is_caught_by_the_second_read_and_takes_the_marker_back() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission_late(&mut gate, who, (true, false, true), true, &seen), "{who:?}");
            assert!(!gate.installing, "nothing is decided, so nothing shuts down");
            assert_eq!(*seen.borrow(), ["close", "read draft", "mark", "read draft", "unmark"], "{who:?}");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission_late(&mut gate, Who::Owner, (true, true, true), true, &seen), "the person's own restart still goes ahead");
    }

    #[test]
    fn a_marker_that_cannot_be_written_stops_every_unattended_install_before_anything_closes() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission(&mut gate, who, true, false, false, &seen), "{who:?}");
            assert!(!gate.installing, "nothing is decided, so nothing is shut down and no restart loop can begin");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Owner, true, false, false, &seen), "the person's own restart cannot loop by itself");
        let missing = std::env::temp_dir().join("faber-no-such-folder-for-the-marker").join("deeper");
        assert!(note_tried(&missing, "1.2.3").is_err(), "a folder that cannot be written is an error, not silence");
        let here = std::env::temp_dir();
        assert!(note_tried(&here, "1.2.3").is_ok());
        let _ = std::fs::remove_file(here.join(super::TRIED_FILE));
    }

    #[test]
    fn a_cli_install_holds_the_admission_to_its_end_and_none_begins_after_the_decision() {
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(gate.begin_setup(), "a CLI install begins");
        assert!(!admission(&mut gate, Who::Owner, true, false, true, &seen), "even the owner's restart waits for it");
        gate.setups -= 1;
        assert!(admission(&mut gate, Who::Later, true, false, true, &seen));
        assert!(!gate.begin_setup(), "a CLI install asked for after the decision is refused");
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
        /// A turn begins while the download runs.
        work_during_download: bool,
        /// The person types while the download runs.
        typing_during_download: bool,
        /// This version was tried less than a day ago.
        tried: bool,
        install: Result<(), String>,
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
        fn stage(&self, phase: Phase) {
            self.seen.borrow_mut().push(match phase {
                Phase::None => "none",
                Phase::Checking => "checking",
                Phase::Downloading => "downloading",
                Phase::Installing => "installing",
                Phase::Ready => "ready",
            });
        }
        fn reserve(&self, _: Who) -> bool {
            self.seen.borrow_mut().push("reserve");
            !self.busy.get() && !self.editing.get()
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

    async fn start(release: &Fake) -> Start {
        update_on_start(release, WAIT, WAIT).await
    }

    #[tokio::test]
    async fn a_release_found_at_start_is_downloaded_and_installed_before_any_work() {
        let release = Fake::found();
        assert_eq!(start(&release).await, Start::Installed);
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "installing", "reserve", "shut down", "install"]);
    }

    #[tokio::test]
    async fn nothing_new_at_start_touches_nothing() {
        let release = Fake { found: Ok(None), ..Fake::found() };
        assert_eq!(start(&release).await, Start::NothingNew);
        assert_eq!(release.saw(), ["checking", "check", "none"]);
    }

    #[tokio::test]
    async fn an_offline_start_gives_up_in_time_and_tries_again_soon() {
        let refused = Fake { found: Err("offline".into()), ..Fake::found() };
        assert_eq!(start(&refused).await, Start::Unreachable);
        assert_eq!(refused.saw(), ["checking", "check", "none"]);
        let silent = Fake { hangs: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&silent).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5), "the start does not wait for a dead network");
        assert_eq!(silent.saw(), ["checking", "check", "none"], "the opening screen goes away with the check");
        assert!(!Start::Unreachable.succeeded() && !Start::Failed.succeeded());
        assert!(Start::NothingNew.succeeded() && Start::Installed.succeeded() && Start::Waits.succeeded() && Start::Skipped.succeeded());
    }

    #[tokio::test]
    async fn a_turn_started_during_the_download_is_never_ended() {
        let release = Fake { work_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(
            release.saw(),
            ["checking", "check", "downloading", "download", "installing", "reserve", "none"],
            "no cleanup, no installer, no relaunch; the kept update waits as it is"
        );
    }

    #[tokio::test]
    async fn typing_during_the_download_keeps_the_draft_and_defers_the_install() {
        let release = Fake { typing_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(
            release.saw(),
            ["checking", "check", "downloading", "download", "installing", "reserve", "none"],
            "the decision reads the draft inside the lock and gives way"
        );
    }

    #[tokio::test]
    async fn a_download_that_failed_installs_nothing() {
        let release = Fake { download: Err("cut".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Unreachable);
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "none"]);
    }

    #[tokio::test]
    async fn a_download_that_never_ends_is_given_up_and_the_line_goes_away() {
        let release = Fake { stalls: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&release).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "none"]);
        // A download in the background is bounded too, so the six-hour loop is never held for good.
        assert!(!fetch(&Fake { stalls: true, ..Fake::found() }, WAIT).await);
    }

    #[tokio::test]
    async fn an_installer_that_does_not_start_leaves_a_relaunched_faber() {
        let release = Fake { install: Err("blocked".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Failed);
        // The real relaunch ends the process, so the screen is never told "none" after it.
        assert!(release.saw().ends_with(&["install", "relaunch", "none"]));
    }

    #[tokio::test]
    async fn a_version_tried_a_day_ago_is_not_downloaded_or_installed_again_at_start() {
        let release = Fake { tried: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Skipped);
        assert_eq!(
            release.saw(),
            ["checking", "check", "none"],
            "the relaunch after a failed install does not start the same install again"
        );
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
        assert_eq!(Reopen::of(["faber.exe".to_string()]), Reopen::Window, "a manual start");
        assert_eq!(Reopen::of(["faber.exe".to_string(), "--autostart".to_string()]), Reopen::Tray, "the Windows login start");
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
