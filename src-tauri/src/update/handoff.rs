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
/// The name the installed program is kept under, beside the copy, while the installer runs (`keep`).
const KEPT: &str = "faber-kept.exe";

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
    // The program is kept beside the installer, in the folder `start` made new for this install.
    match Path::new(installer).parent() {
        Some(here) => {
            install_or_put_back(&exe, here, &data, || run_installer(installer, folder, reopen != "tray", &data));
        }
        None => super::log(&data, "install-failed", "no folder to keep Faber in"),
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

/// How the installer ended. Only `Failed` can have left the installed program broken; one still running is left alone.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Installed {
    Done,
    Failed,
    /// It did not start: nothing was touched.
    NotRun,
    StillRunning,
}

fn run_installer(installer: &OsString, folder: &Path, shown: bool, data: &Path) -> Installed {
    let began = Instant::now();
    match Command::new(installer).raw_arg(installer_line(folder, shown)).creation_flags(CREATE_NO_WINDOW).spawn() {
        Ok(mut child) => {
            let mut status = None;
            wait_until(INSTALL_WAIT, || {
                status = child.try_wait().ok().flatten();
                status.is_some()
            });
            match status {
                Some(status) if status.success() => {
                    super::log(data, "installed", &format!("in {} ms", began.elapsed().as_millis()));
                    Installed::Done
                }
                Some(status) => {
                    super::log(data, "install-failed", &format!("installer exit code {}", status.code().unwrap_or(-1)));
                    Installed::Failed
                }
                None => {
                    super::log(data, "install-slow", "the installer still runs; Faber opens anyway");
                    Installed::StillRunning
                }
            }
        }
        Err(error) => {
            super::log(data, "install-failed", &error.to_string());
            Installed::NotRun
        }
    }
}

/// Runs `install` only once the installed program is kept in `dir`, and puts it back when the install fails and left it
/// changed: an installer that ends with an error can leave `faber.exe` empty (2026-11-01: exit code 1, a 0-byte
/// `faber.exe`, and Faber could not open again until it was installed by hand). When it cannot be kept (most likely a full
/// disk) nothing is installed and the same version opens again.
fn install_or_put_back(exe: &Path, dir: &Path, data: &Path, install: impl FnOnce() -> Installed) -> Installed {
    let kept = dir.join(KEPT);
    if let Err(error) = keep(exe, &kept) {
        super::log(data, "install-failed", &format!("Faber could not be kept before the install: {error}"));
        return Installed::NotRun;
    }
    let installed = install();
    // This build is one program, so a changed one goes back whole: nothing beside it can be of another version.
    if installed == Installed::Failed && !same(exe, &kept).unwrap_or(false) {
        match put_back(exe, &kept) {
            Ok(()) => super::log(data, "install-undone", "the failed install was undone; the same version opens"),
            Err(error) => super::log(data, "put-back-failed", &error.to_string()),
        }
    }
    installed
}

/// Copies `exe` to the new file `kept` (nothing already there is written through) and checks the copy.
/// A copy that could not be finished (a full disk) is removed; someone else's file under that name is left as it is.
fn keep(exe: &Path, kept: &Path) -> std::io::Result<()> {
    copy_new(exe, kept)?;
    let checked = same(exe, kept).and_then(|same| if same { Ok(()) } else { Err(std::io::Error::other("the kept copy differs")) });
    if checked.is_err() {
        let _ = std::fs::remove_file(kept);
    }
    checked
}

/// Copies `from` to the new file `to`. A copy cut short is removed; a file already at `to` is never touched.
fn copy_new(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut target = std::fs::OpenOptions::new().write(true).create_new(true).open(to)?;
    let written = std::fs::File::open(from).and_then(|mut source| std::io::copy(&mut source, &mut target)).and_then(|_| target.sync_all());
    if written.is_err() {
        drop(target);
        let _ = std::fs::remove_file(to);
    }
    written
}

/// Puts `kept` back at `exe` whole or not at all: it is written beside it under another name, checked, and only then
/// renamed over it, so an error (a full disk) never leaves a half-written program. Its folder is made again if the
/// installer removed it.
fn put_back(exe: &Path, kept: &Path) -> std::io::Result<()> {
    let folder = exe.parent().ok_or_else(|| std::io::Error::other("no folder for the program"))?;
    std::fs::create_dir_all(folder)?;
    let staged = exe.with_extension("exe.faber-put-back");
    let _ = std::fs::remove_file(&staged);
    let written = copy_new(kept, &staged).and_then(|()| {
        if same(kept, &staged)? { Ok(()) } else { Err(std::io::Error::other("the program put back differs from the kept one")) }
    });
    let renamed = written.and_then(|()| std::fs::rename(&staged, exe));
    if renamed.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    renamed
}

/// Whether two files hold the same bytes, read in pieces so the program is never held in memory whole.
fn same(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let (mut a, mut b) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    if a.metadata()?.len() != b.metadata()?.len() {
        return Ok(false);
    }
    let (mut x, mut y) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let n = a.read(&mut x)?;
        if n == 0 {
            return Ok(true);
        }
        b.read_exact(&mut y[..n])?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
    }
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
    use super::{COPY, Installed, KEPT, install_folder, install_or_put_back, installer_line, is_free, safe, sweep_in, wait_until};
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

    /// An installed `faber.exe`, a data folder, and the copy's own folder where it is kept.
    fn installed() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let (app, data, here) = (root.path().join("Faber"), root.path().join("data"), root.path().join("faber-update-1"));
        for dir in [&app, &data, &here] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(app.join("faber.exe"), b"MZ old program").unwrap();
        (root, app.join("faber.exe"), data, here)
    }

    fn log(data: &Path) -> String {
        std::fs::read_to_string(data.join("update.log")).unwrap_or_default()
    }

    #[test]
    fn a_failed_install_that_emptied_faber_puts_the_old_program_back() {
        let (_root, exe, data, here) = installed();
        let installed = install_or_put_back(&exe, &here, &data, || {
            // 2026-11-01: the installer ended with code 1 and left `faber.exe` with no bytes.
            std::fs::write(&exe, b"").unwrap();
            Installed::Failed
        });
        assert_eq!(installed, Installed::Failed);
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program", "Faber opens again on the same version");
        assert!(log(&data).contains("install-undone"));
        assert!(!exe.with_extension("exe.faber-put-back").exists(), "nothing is left beside the program");
    }

    #[test]
    fn a_failed_install_that_removed_the_folder_gets_it_back() {
        let (_root, exe, data, here) = installed();
        install_or_put_back(&exe, &here, &data, || {
            std::fs::remove_dir_all(exe.parent().unwrap()).unwrap();
            Installed::Failed
        });
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program");
    }

    #[test]
    fn a_failed_install_that_touched_nothing_puts_nothing_back() {
        let (_root, exe, data, here) = installed();
        assert_eq!(install_or_put_back(&exe, &here, &data, || Installed::Failed), Installed::Failed);
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program");
        assert!(!log(&data).contains("install-undone"));
    }

    #[test]
    fn a_finished_or_running_install_is_never_undone() {
        for outcome in [Installed::Done, Installed::StillRunning] {
            let (_root, exe, data, here) = installed();
            install_or_put_back(&exe, &here, &data, || {
                std::fs::write(&exe, b"MZ new program").unwrap();
                outcome
            });
            assert_eq!(std::fs::read(&exe).unwrap(), b"MZ new program", "{outcome:?}");
        }
    }

    #[test]
    fn the_installer_never_runs_when_faber_cannot_be_kept_first() {
        let (_root, exe, data, here) = installed();
        // Something already holds the name the copy is kept under: nothing is written through it.
        std::fs::write(here.join(KEPT), b"planted").unwrap();
        let mut ran = false;
        let installed = install_or_put_back(&exe, &here, &data, || {
            ran = true;
            Installed::Done
        });
        assert_eq!(installed, Installed::NotRun);
        assert!(!ran, "no installer runs without a way back");
        assert_eq!(std::fs::read(here.join(KEPT)).unwrap(), b"planted");
        assert!(log(&data).contains("could not be kept"));
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
