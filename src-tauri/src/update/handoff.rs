//! The install itself, on Windows: Faber hands it to a copy of itself and ends.
//!
//! The updater plugin's own `install` starts the installer and only then ends Faber. The installer's first step closes every
//! running `faber.exe`, so it met a Faber still closing, could not close it, and gave up without a word: nothing was
//! installed and nothing opened again (2026-10-30: two failed installs in `update.log`, each with Windows' Restart Manager
//! event 10006 "'Faber' could not be shut down" in the same second). Now Faber writes the installer it downloaded (the
//! plugin checked its signature) and a copy of its own program into a new temporary folder, starts that copy with
//! `--faber-update-install`, and ends. The copy (`finish`) waits until no process runs the installed `faber.exe`, runs the
//! installer into the same folder, and opens Faber again in every case: the new version when the install worked, the same
//! version when it did not, so the person never has to guess whether to open it. When Faber comes back with its window the
//! person is looking, so the installer shows its progress with nothing to press (`/P`, passive); when it comes back in the
//! tray (the idle install, an `--autostart` start) it shows nothing (`/S`), so no window ever covers a game.
//!
//! No shell runs: the installer is started directly. Its `/D=<folder>` must be last and unquoted (NSIS), so the line is
//! written as is; `install_dir_arg` already refused a folder with a quote in it.

use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The first argument of the copy that finishes an install.
pub(crate) const FLAG: &str = "--faber-update-install";
/// The temporary folders of earlier installs start with this.
const FOLDER: &str = "faber-update-";
/// The copy's name in that folder.
const COPY: &str = "faber-update.exe";
/// How long the copy waits for Faber to end before it runs the installer anyway (the installer then closes it itself).
const CLOSE_WAIT: Duration = Duration::from_secs(60);
/// An installer still running after this is left to finish on its own, and Faber opens anyway.
const INSTALL_WAIT: Duration = Duration::from_secs(300);
/// Windows: start the process without a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Writes the installer and the copy, starts the copy, and returns. The caller ends the process right after.
/// `install_dir` is the installer's `/D=<folder>` argument (`install_dir_arg`).
pub(crate) fn start(bytes: &[u8], version: &str, install_dir: &OsString, data_dir: &Path, tray: bool) -> Result<(), String> {
    // A Windows program starts with "MZ"; anything else (an archive of an older format) is not run.
    if !bytes.starts_with(b"MZ") {
        return Err("the update is not an installer".into());
    }
    let folder = install_folder(install_dir).ok_or("no install folder")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = exe.file_name().ok_or("no program name")?;
    let temp = std::env::temp_dir().join(format!("{FOLDER}{}-{}", safe(version), std::process::id()));
    std::fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
    let installer = temp.join("Faber-setup.exe");
    std::fs::write(&installer, bytes).map_err(|e| e.to_string())?;
    let copy = temp.join(COPY);
    std::fs::copy(&exe, &copy).map_err(|e| e.to_string())?;
    Command::new(copy)
        .arg(FLAG)
        .arg(&installer)
        .arg(folder.join(name))
        .arg(data_dir)
        .arg(if tray { "tray" } else { "window" })
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The copy's work: `[installer, installed faber.exe, data folder, "window" | "tray"]`. Returns the exit code.
pub(crate) fn finish(args: &[OsString]) -> i32 {
    let [installer, exe, data, reopen] = args else { return 1 };
    let (exe, data) = (PathBuf::from(exe), PathBuf::from(data));
    let Some(folder) = exe.parent() else { return 1 };
    if !wait_until(CLOSE_WAIT, || is_free(&exe)) {
        super::log(&data, "install-wait", "Faber was still closing; the installer closes it");
    }
    let began = Instant::now();
    match Command::new(installer).raw_arg(installer_line(folder, reopen != "tray")).creation_flags(CREATE_NO_WINDOW).spawn() {
        Ok(mut child) => {
            let mut status = None;
            wait_until(INSTALL_WAIT, || {
                status = child.try_wait().ok().flatten();
                status.is_some()
            });
            match status {
                Some(status) if status.success() => super::log(&data, "installed", &format!("in {} ms", began.elapsed().as_millis())),
                Some(status) => super::log(&data, "install-failed", &format!("installer exit code {}", status.code().unwrap_or(-1))),
                None => super::log(&data, "install-slow", "the installer still runs; Faber opens anyway"),
            }
        }
        Err(error) => super::log(&data, "install-failed", &error.to_string()),
    }
    // Back in every case: the new version, or the same one when the install did not happen.
    let mut again = Command::new(&exe);
    if reopen == "tray" {
        again.arg(crate::autostart::AUTOSTART_FLAG);
    }
    if let Err(error) = again.spawn() {
        super::log(&data, "relaunch-failed", &error.to_string());
        return 1;
    }
    0
}

/// Removes the folders earlier installs left in the temporary folder. A folder whose copy still runs is an install under
/// way (Faber opened while it waits): it is left whole, so its installer is not taken from under it.
pub(crate) fn sweep() {
    sweep_in(&std::env::temp_dir());
}

fn sweep_in(temp: &Path) {
    for entry in std::fs::read_dir(temp).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(FOLDER) && is_free(&entry.path().join(COPY)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The folder in `/D=<folder>`.
fn install_folder(arg: &OsString) -> Option<PathBuf> {
    arg.to_str()?.strip_prefix("/D=").filter(|dir| !dir.is_empty()).map(PathBuf::from)
}

/// The installer's command line: passive (its progress, nothing to press) when `shown`, else silent; an update, into
/// `folder`, with `/D=` last and unquoted, as NSIS wants.
fn installer_line(folder: &Path, shown: bool) -> String {
    format!("{} /UPDATE /D={}", if shown { "/P" } else { "/S" }, folder.display())
}

/// A version as a folder name: only letters, digits, dots and dashes.
fn safe(version: &str) -> String {
    version.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

/// No process runs `exe`: Windows refuses to open a running program for writing. Nothing is written.
fn is_free(exe: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(exe) {
        Ok(_) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let began = Instant::now();
    loop {
        if done() {
            return true;
        }
        if began.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::{COPY, install_folder, installer_line, is_free, safe, sweep_in, wait_until};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn the_installer_lands_in_the_same_folder_with_d_last_and_unquoted() {
        let folder = install_folder(&OsString::from(r"/D=E:\Program Files\Faber")).unwrap();
        assert_eq!(folder, PathBuf::from(r"E:\Program Files\Faber"));
        assert_eq!(installer_line(&folder, false), r"/S /UPDATE /D=E:\Program Files\Faber", "the tray: nothing shows");
        assert_eq!(installer_line(&folder, true), r"/P /UPDATE /D=E:\Program Files\Faber", "the window: its progress shows");
        assert_eq!(install_folder(&OsString::from("/D=")), None);
        assert_eq!(install_folder(&OsString::from(r"E:\Apps\Faber")), None, "only the installer argument is read");
    }

    #[test]
    fn a_version_cannot_leave_its_temporary_folder() {
        assert_eq!(safe("0.3.18-preview.19"), "0.3.18-preview.19");
        assert_eq!(safe(r"..\..\x"), ".._.._x");
    }

    #[test]
    fn a_program_that_runs_is_not_free_and_a_missing_one_is() {
        let running = std::env::current_exe().unwrap();
        assert!(!is_free(&running), "Windows refuses to open the running test program for writing");
        assert!(is_free(Path::new(r"C:\no\such\faber.exe")));
    }

    #[test]
    fn the_wait_ends_when_the_program_is_free_or_the_limit_passes() {
        let mut calls = 0;
        assert!(wait_until(Duration::from_secs(5), || {
            calls += 1;
            calls == 3
        }));
        assert!(!wait_until(Duration::from_millis(150), || false));
    }

    #[test]
    fn the_sweep_removes_only_finished_install_folders() {
        let temp = tempfile::tempdir().unwrap();
        let done = temp.path().join("faber-update-0.3.18-1");
        std::fs::create_dir_all(&done).unwrap();
        std::fs::write(done.join(COPY), b"MZ").unwrap();
        std::fs::write(done.join("Faber-setup.exe"), b"MZ").unwrap();
        let other = temp.path().join("something-else");
        std::fs::create_dir_all(&other).unwrap();
        sweep_in(temp.path());
        assert!(!done.exists(), "a copy that no longer runs: the folder goes");
        assert!(other.exists(), "nothing else in the temporary folder is touched");
    }
}
