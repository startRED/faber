//! Keeps an installed Faber on the latest release without the owner doing anything.
//!
//! Faber stays in the tray when its window closes and is usually ended by logoff, not by Quit, so installing only at
//! Quit never happened. This checks at start and every six hours, keeps the download, and installs it at the first
//! idle moment: no answer running, and the window closed to the tray or minimized. It never touches running work and
//! never shows a window.
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
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tokio::time::{Instant, sleep};

const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// After a check that could not reach the server (offline, a PC that just woke up).
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often an idle moment is looked for while an update waits.
const LOOK_EVERY: Duration = Duration::from_secs(30);

/// A downloaded update installs only when nothing can be lost or seen: it is ready, no answer is running, and the
/// window is not visible.
pub(crate) fn may_install(ready: bool, busy: bool, window_visible: bool) -> bool {
    ready && !busy && !window_visible
}

/// Folder `a` is folder `b` or inside it, or the other way round. Windows paths ignore case.
fn overlaps(a: &Path, b: &Path) -> bool {
    let low = |p: &Path| Path::new(&p.to_string_lossy().to_lowercase()).to_path_buf();
    let (a, b) = (low(a), low(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// The installer argument that makes an update land in the folder of the running Faber: `/D=<folder>`. NSIS wants it
/// last, unquoted, whatever the path holds (`installer_arg` values come last on the line). None when the folder cannot be
/// written that way, or holds Faber's data: an install must never go there.
pub(crate) fn install_dir_arg(exe: &Path, data_dir: &Path) -> Option<OsString> {
    let dir = exe.parent()?.to_str()?;
    let dir = dir.strip_prefix(r"\\?\").unwrap_or(dir);
    if dir.is_empty() || dir.contains('"') || overlaps(Path::new(dir), data_dir) {
        return None;
    }
    Some(format!("/D={dir}").into())
}

fn window_visible(app: &AppHandle) -> bool {
    main_window(app).is_some_and(|w| w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false))
}

/// Looks for a newer release and downloads it, signed with Faber's own key. False when the server was not reached.
async fn fetch(app: &AppHandle, install_dir: &OsString) -> bool {
    // `--autostart` is what the installer starts the new Faber with: in the tray, no window, like the Faber it replaces.
    // `/D=` must be the last argument.
    let built = app.updater_builder().installer_arg(AUTOSTART_FLAG).installer_arg(install_dir).build();
    let Ok(updater) = built else { return false };
    let Ok(found) = updater.check().await else { return false };
    if let Some(update) = found
        && let Ok(bytes) = update.download(|_, _| {}, || {}).await
    {
        *lock(&app.state::<Faber>().update) = Some((update, bytes));
    }
    true
}

/// Windows only: elsewhere the installer does not restart the app, so the update waits for Quit.
fn install_when_idle(app: &AppHandle) {
    let faber = app.state::<Faber>();
    if !cfg!(windows) || lock(&faber.update).is_none() || !may_install(true, faber.engine.is_working(), window_visible(app)) {
        return;
    }
    let Some((update, bytes)) = lock(&faber.update).take() else { return };
    // The installer ends this process without `RunEvent::Exit`, so the cleanup Quit does happens first.
    crate::shut_down(app);
    // Only a failure comes back: the process is gone otherwise. The update is dropped; the next check fetches it again.
    if let Err(error) = update.install(bytes) {
        eprintln!("faber: update install failed: {error}");
    }
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
                next_check = Instant::now() + if fetch(&app, &install_dir).await { CHECK_EVERY } else { RETRY_AFTER };
            }
            install_when_idle(&app);
            sleep(LOOK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::may_install;
    #[cfg(windows)]
    use super::{install_dir_arg, overlaps};
    #[cfg(windows)]
    use std::path::Path;

    #[cfg(windows)]
    #[test]
    fn the_update_goes_into_the_folder_of_the_running_faber() {
        let data = Path::new(r"C:\Users\a\AppData\Roaming\Faber");
        assert_eq!(install_dir_arg(Path::new(r"E:\Apps\Faber\faber.exe"), data).unwrap(), r"/D=E:\Apps\Faber");
        let spaced = install_dir_arg(Path::new(r"C:\Program Files\Faber\faber.exe"), data).unwrap();
        assert_eq!(spaced, r"/D=C:\Program Files\Faber", "NSIS reads the rest of the line, and quotes are not allowed");
        let long = install_dir_arg(Path::new(r"\\?\E:\Apps\Faber\faber.exe"), data).unwrap();
        assert_eq!(long, r"/D=E:\Apps\Faber");
    }

    #[cfg(windows)]
    #[test]
    fn an_update_never_goes_where_faber_keeps_its_data() {
        let data = Path::new(r"C:\Users\a\AppData\Local\Faber");
        assert!(install_dir_arg(Path::new(r"c:\users\a\appdata\local\faber\faber.exe"), data).is_none(), "same folder, any case");
        assert!(install_dir_arg(Path::new(r"C:\Users\a\AppData\Local\Faber\bin\faber.exe"), data).is_none(), "inside the data");
        assert!(install_dir_arg(Path::new(r"C:\Users\a\AppData\Local\faber.exe"), data).is_none(), "holding the data");
        assert!(install_dir_arg(Path::new("faber.exe"), data).is_none(), "no folder to name");
    }

    #[cfg(windows)]
    #[test]
    fn the_installers_default_folder_is_not_the_default_data_folder() {
        // The per-user installer starts from $LOCALAPPDATA\Faber; the data lives under the config folder (Roaming on Windows).
        let data = crate::default_data_dir(Path::new(r"C:\Users\a\AppData\Roaming"));
        assert!(!overlaps(Path::new(r"C:\Users\a\AppData\Local\Faber"), &data));
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
}
