//! Keeps an installed Faber on the latest release without the owner doing anything.
//!
//! Faber stays in the tray when its window closes and is usually ended by logoff, not by Quit, so installing only at
//! Quit never happened. This checks at start and every six hours, keeps the download, and installs it at the first
//! idle moment: no answer running, no CLI install from the setup screen, and the window closed to the tray or minimized.
//! It never touches running work and never shows a window.
//!
//! On Windows the plugin's `install` starts the NSIS installer with `/S /UPDATE /R /ARGS <args>` (`updater.rs`
//! `updater_parameters`, `config.rs` `nsis_args`) and then ends the process with `exit(0)`. Silent (`/S`) shows no window
//! and, for the per-user installer (`bundle.windows.nsis.installMode = "currentUser"`, `RequestExecutionLevel user`),
//! needs no elevation. With `/R` the installer's `.onInstSuccess` starts the new `faber.exe` with the `/ARGS` value, so
//! the arguments below decide how Faber comes back.
//!
//! Where it lands is decided by the installer's `.onInit` (tauri-bundler's `installer.nsi`): unless the command line has
//! `/D=<dir>`, it starts from `$LOCALAPPDATA\Faber` and only moves to the folder of an earlier install if the registry
//! value `HKCU\Software\<publisher>\Faber` (written by `Section Install`) says so. `install` passes no `/D`, so an install
//! whose key the new installer does not find (as happened with E:\Apps\Faber) is replaced by a second copy in
//! `%LOCALAPPDATA%\Faber`. Faber therefore always tells the installer its own folder.

use crate::autostart::AUTOSTART_FLAG;
use crate::{Faber, lock, main_window};
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tokio::time::{Instant, sleep};

const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// After a check or a download that failed (offline, a PC that just woke up).
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often an idle moment is looked for while an update waits.
const LOOK_EVERY: Duration = Duration::from_secs(30);

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

/// Looks for a newer release and downloads it, signed with Faber's own key. False when the check or the download failed.
async fn fetch(app: &AppHandle, install_dir: &OsString) -> bool {
    // `--autostart` is what the installer starts the new Faber with: in the tray, no window, like the Faber it replaces.
    // `/D=` must be the last argument.
    let built = app.updater_builder().installer_arg(AUTOSTART_FLAG).installer_arg(install_dir).build();
    let Ok(updater) = built else { return false };
    let Ok(found) = updater.check().await else { return false };
    let Some(update) = found else { return fetch_succeeded(false, false) };
    let downloaded = match update.download(|_, _| {}, || {}).await {
        Ok(bytes) => {
            *lock(&app.state::<Faber>().update) = Some((update, bytes));
            true
        }
        Err(_) => false,
    };
    fetch_succeeded(true, downloaded)
}

#[derive(Debug, PartialEq)]
enum Idle {
    /// Not idle at one of the two looks: nothing was touched.
    Waited,
    Installed,
    /// The installer did not start; Faber was relaunched.
    Failed,
}

/// The idle install in order: look, look again, clean up, install, and relaunch if the installer did not start.
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
        Err(error) => {
            eprintln!("faber: update install failed: {error}");
            relaunch();
            Idle::Failed
        }
    }
}

/// Starts Faber again in the tray and ends this process. The engine was shut down for an install that did not happen, so
/// this process is no good any more; the new one starts clean with the same data.
fn relaunch_in_tray(app: &AppHandle) -> ! {
    // Releases the single-instance lock, which `RunEvent::Exit` would have done; otherwise the new process only
    // finds this one and leaves.
    tauri_plugin_single_instance::destroy(app);
    // No shell, and the program is a windowless one (`windows_subsystem`), so nothing shows.
    let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(AUTOSTART_FLAG).spawn());
    if let Err(error) = started {
        eprintln!("faber: could not start Faber again: {error}");
    }
    std::process::exit(0)
}

/// Windows only: elsewhere the installer does not restart the app, so the update waits for Quit.
fn install_when_idle(app: &AppHandle) {
    if !cfg!(windows) {
        return;
    }
    let faber = app.state::<Faber>();
    let idle = || lock(&faber.update).is_some() && may_install(true, faber.engine.is_working() || setups_running(), window_visible(app));
    let install = || {
        let (update, bytes) = lock(&faber.update).take().ok_or("no update is ready")?;
        // Only a failure comes back: the process is gone otherwise. The update is dropped; the next check fetches it again.
        update.install(bytes).map_err(|e| e.to_string())
    };
    // The installer ends this process without `RunEvent::Exit`, so the cleanup Quit does happens first.
    idle_install(idle, || crate::shut_down(app), install, || relaunch_in_tray(app));
}

pub(crate) fn keep_updated(app: &AppHandle) {
    let exe = std::env::current_exe().unwrap_or_default();
    let Some(install_dir) = install_dir_arg(&exe, &app.state::<Faber>().data_dir) else {
        eprintln!("faber: no safe folder to update into; Faber stays on this version");
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut next_check = Instant::now();
        loop {
            // A downloaded update is kept and installed first; the next one is looked for after it.
            if lock(&app.state::<Faber>().update).is_none() && Instant::now() >= next_check {
                next_check = Instant::now() + next_check_delay(fetch(&app, &install_dir).await);
            }
            install_when_idle(&app);
            sleep(LOOK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{CHECK_EVERY, RETRY_AFTER, setups_running};
    use super::{Idle, SetupRunning, fetch_succeeded, idle_install, is_dos_path, may_install, next_check_delay, plain};
    use std::cell::{Cell, RefCell};

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
