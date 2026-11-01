//! The install itself, on Windows: Faber hands it to a copy of itself and ends.
//!
//! The updater plugin's own `install` started the installer and only then ended Faber. The installer's first step closes every
//! running `faber.exe`, so it met a Faber still closing, could not close it, and gave up without a word: nothing was
//! installed and nothing opened again (2026-10-30: two failed installs in `update.log`, each with Windows' Restart Manager
//! event 10006 "'Faber' could not be shut down" in the same second). Now Faber keeps the installer it downloaded (its signature
//! was checked) in a new temporary folder, puts a copy of the running program (the sidecar) beside it, starts that copy with
//! `--faber-update-install`, and ends. The copy (`finish`) waits until neither the installed `faber.exe` nor its sidecar
//! runs, then runs the installer into the same folder, and opens Faber again in every case: the new version when the
//! install worked, the same version when it did not, so the person never has to guess whether to open it. When Faber comes
//! back with its window the person is looking, so the installer shows its progress with nothing to press (`/P`, passive);
//! when it comes back in the tray (the idle install, an `--autostart` start) it shows nothing (`/S`), so no window ever
//! covers a game.
//!
//! No shell runs: the installer is started directly. Its `/D=<folder>` must be last and unquoted (NSIS), so the line is
//! written as is; `install_dir_arg` already refused a folder with a quote in it.

use crate::FOLDER;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The first argument of the copy that finishes an install.
pub const FLAG: &str = "--faber-update-install";
/// The copy's name in that folder.
const COPY: &str = "faber-update.exe";
/// The installed app's main program: the Electron executable keeps this name.
const APP: &str = "faber.exe";
/// The host program beside it, in the app resources folder.
const SIDECAR: [&str; 2] = ["resources", "faber-sidecar.exe"];
/// What the app is opened with to start in the tray (`faber_host::autostart::AUTOSTART_FLAG`).
const AUTOSTART: &str = "--autostart";
/// How long the copy waits for Faber to end before it runs the installer anyway (the installer then closes it itself).
const CLOSE_WAIT: Duration = Duration::from_secs(60);
/// An installer still running after this is left to finish on its own, and Faber opens anyway.
const INSTALL_WAIT: Duration = Duration::from_secs(300);
/// Windows: start the process without a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Puts a copy of the running program beside `installer` (the verified file `release::download` left in its own temporary
/// folder), starts the copy, and returns. The caller ends the process right after. `install_dir` is the installer
/// `/D=<folder>` argument (`install_dir_arg`).
pub fn start(installer: &Path, signature: &str, version: &str, install_dir: &OsString, data_dir: &Path, tray: bool) -> Result<(), String> {
    // The file and its folder are ours, made new by the download: a link in either place is not run.
    let temp = installer.parent().ok_or("no folder for the installer")?;
    if !is_plain(installer, false) || !is_plain(temp, true) {
        return Err("the update is not where it was left".into());
    }
    // A Windows program starts with "MZ"; anything else (an archive of an older format) is not run.
    let mut head = [0u8; 2];
    std::fs::File::open(installer).and_then(|mut file| file.read_exact(&mut head)).map_err(|e| e.to_string())?;
    if &head != b"MZ" {
        return Err("the update is not an installer".into());
    }
    spawn_copy(temp, installer.as_os_str(), signature, version, install_dir, data_dir, tray)
}

/// Opens Faber again without installing anything: for an install that could not start, after the app has ended. The same
/// copy waits for the app to end, since opening it while the old one still holds the single-instance lock would only make the
/// new one leave.
pub fn relaunch(install_dir: &OsString, data_dir: &Path, tray: bool) -> Result<(), String> {
    install_folder(install_dir).ok_or("no install folder")?;
    let temp = crate::staging_in(&std::env::temp_dir(), "reopen").map_err(|e| e.to_string())?;
    spawn_copy(&temp, std::ffi::OsStr::new(""), "", "", install_dir, data_dir, tray)
}

fn spawn_copy(
    temp: &Path,
    installer: &std::ffi::OsStr,
    signature: &str,
    version: &str,
    install_dir: &OsString,
    data_dir: &Path,
    tray: bool,
) -> Result<(), String> {
    let folder = install_folder(install_dir).ok_or("no install folder")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let temp = std::fs::canonicalize(temp).map_err(|e| e.to_string())?;
    let _folders = hold_folders(&temp).map_err(|e| e.to_string())?;
    let copy = temp.join(COPY);
    copy_new(&exe, &copy).map_err(|e| e.to_string())?;
    let _verified = verify_copy(&exe, &copy).map_err(|e| e.to_string())?;
    Command::new(copy)
        .arg(FLAG)
        .arg(installer)
        .arg(signature)
        .arg(version)
        .arg(folder.join(APP))
        .arg(data_dir)
        .arg(if tray { "tray" } else { "window" })
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The copy's work: `[installer, signature, version, installed faber.exe, data folder, "window" | "tray"]`, the installer empty to only open Faber
/// again (`relaunch`). Returns the exit code.
pub fn finish(args: &[OsString]) -> i32 {
    let [installer, signature, version, exe, data, reopen] = args else { return 1 };
    let (exe, data) = (PathBuf::from(exe), PathBuf::from(data));
    let Some(folder) = exe.parent() else { return 1 };
    if !wait_until(CLOSE_WAIT, || is_free(&exe) && is_free(&folder.join(SIDECAR[0]).join(SIDECAR[1]))) {
        crate::log(&data, "install-wait", "Faber was still closing; the installer closes it");
    }
    if !installer.is_empty() {
        run_installer(installer, signature, version, folder, reopen != "tray", &data);
    }
    // Back in every case: the new version, or the same one when the install did not happen.
    let mut again = Command::new(&exe);
    if reopen == "tray" {
        again.arg(AUTOSTART);
    }
    if let Err(error) = again.spawn() {
        crate::log(&data, "relaunch-failed", &error.to_string());
        return 1;
    }
    0
}

fn run_installer(installer: &OsString, signature: &OsString, version: &OsString, folder: &Path, shown: bool, data: &Path) {
    let checked = || -> Result<std::fs::File, String> {
        // Deny write and delete sharing until the verified executable has started.
        let mut file = std::fs::OpenOptions::new().read(true).share_mode(1).open(installer).map_err(|e| e.to_string())?;
        crate::release::verify_file(&mut file, signature.to_str().ok_or("invalid signature")?, version.to_str().ok_or("invalid version")?)?;
        Ok(file)
    };
    let verified = match checked() {
        Ok(file) => file,
        Err(error) => {
            crate::log(data, "install-failed", &error);
            return;
        }
    };
    let began = Instant::now();
    let spawned = Command::new(installer).raw_arg(installer_line(folder, shown)).creation_flags(CREATE_NO_WINDOW).spawn();
    drop(verified);
    match spawned {
        Ok(mut child) => {
            let mut status = None;
            wait_until(INSTALL_WAIT, || {
                status = child.try_wait().ok().flatten();
                status.is_some()
            });
            match status {
                Some(status) if status.success() => crate::log(data, "installed", &format!("in {} ms", began.elapsed().as_millis())),
                Some(status) => crate::log(data, "install-failed", &format!("installer exit code {}", status.code().unwrap_or(-1))),
                None => crate::log(data, "install-slow", "the installer still runs; Faber opens anyway"),
            }
        }
        Err(error) => crate::log(data, "install-failed", &error.to_string()),
    }
}

/// Removes the folders earlier installs left in the temporary folder. A folder whose copy still runs is an install under
/// way (Faber opened while it waits): it is left whole, so its installer is not taken from under it.
pub fn sweep() {
    sweep_in(&std::env::temp_dir());
}

fn sweep_in(temp: &Path) {
    for entry in std::fs::read_dir(temp).into_iter().flatten().flatten() {
        let path = entry.path();
        // A link (symlink or junction) named like our folders is not one of ours: it is left alone, never followed.
        if entry.file_name().to_string_lossy().starts_with(FOLDER) && is_plain(&path, true) && is_free(&path.join(COPY)) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// `path` is what it says: a folder (`dir`) or a file, and not a symlink, a junction or any other reparse point. Not there, or
/// not readable, is no.
fn is_plain(path: &Path, dir: bool) -> bool {
    use std::os::windows::fs::MetadataExt;
    const REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path).is_ok_and(|meta| {
        !meta.is_symlink() && meta.file_attributes() & REPARSE_POINT == 0 && if dir { meta.is_dir() } else { meta.is_file() }
    })
}

/// Copies `from` to the new file `to`: the copy must not exist, so nothing planted there is written through.
fn copy_new(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut source = std::fs::File::open(from)?;
    let mut target = std::fs::OpenOptions::new().write(true).create_new(true).open(to)?;
    std::io::copy(&mut source, &mut target).map(drop)
}

// Hold every path component so none can be rebound between checking the copy and starting it.
fn hold_folders(path: &Path) -> std::io::Result<Vec<std::fs::File>> {
    use std::os::windows::fs::MetadataExt;
    let mut folders: Vec<_> = path.ancestors().collect();
    folders.reverse();
    folders
        .into_iter()
        .map(|folder| {
            let held = std::fs::OpenOptions::new().read(true).share_mode(3).custom_flags(0x0200_0000 | 0x0020_0000).open(folder)?;
            let meta = held.metadata()?;
            if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                return Err(std::io::Error::other("update folder is a reparse point"));
            }
            Ok(held)
        })
        .collect()
}

fn verify_copy(source: &Path, copy: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::MetadataExt;
    let mut original = std::fs::File::open(source)?;
    let mut held = std::fs::OpenOptions::new().read(true).share_mode(1).custom_flags(0x0020_0000).open(copy)?;
    if held.metadata()?.file_attributes() & 0x400 != 0 {
        return Err(std::io::Error::other("update helper is a reparse point"));
    }
    let (mut a, mut b) = ([0u8; 65536], [0u8; 65536]);
    loop {
        let n = original.read(&mut a)?;
        held.read_exact(&mut b[..n])?;
        if a[..n] != b[..n] {
            return Err(std::io::Error::other("update helper differs from the running program"));
        }
        if n == 0 {
            break;
        }
    }
    if held.read(&mut b[..1])? != 0 {
        return Err(std::io::Error::other("update helper differs from the running program"));
    }
    held.seek(SeekFrom::Start(0))?;
    Ok(held)
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
    use super::{
        COPY, copy_new, finish, hold_folders, install_folder, installer_line, is_free, is_plain, relaunch, start, sweep_in, verify_copy,
        wait_until,
    };
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

    #[test]
    fn the_sweep_leaves_a_junction_named_like_ours_alone_and_never_follows_it() {
        let temp = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("precious.txt"), b"keep").unwrap();
        let link = temp.path().join("faber-update-planted");
        junction::create(elsewhere.path(), &link).unwrap();
        sweep_in(temp.path());
        assert!(link.exists(), "a link is not one of our folders");
        assert!(elsewhere.path().join("precious.txt").exists(), "nothing behind it was touched");
        assert!(!is_plain(&link, true), "a junction is not a plain folder");
        assert!(is_plain(temp.path(), true) && !is_plain(temp.path(), false) && !is_plain(&temp.path().join("none"), true));
    }

    #[test]
    fn an_installer_in_a_linked_folder_is_never_run() {
        let real = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(real.path().join("Faber-setup.exe"), b"MZ").unwrap();
        let holder = tempfile::tempdir().unwrap();
        let link = holder.path().join("linked");
        junction::create(real.path(), &link).unwrap();
        let dir = OsString::from(r"/D=E:\Apps\Faber");
        assert_eq!(
            start(&link.join("Faber-setup.exe"), "", "", &dir, data.path(), false),
            Err("the update is not where it was left".into())
        );
        assert!(!real.path().join(COPY).exists(), "no copy of the program was made");
    }

    #[test]
    fn the_copy_of_the_program_is_a_new_file() {
        let temp = tempfile::tempdir().unwrap();
        let from = temp.path().join("from.exe");
        std::fs::write(&from, b"MZ program").unwrap();
        let to = temp.path().join(COPY);
        copy_new(&from, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"MZ program");
        assert!(copy_new(&from, &to).is_err(), "a file that is already there is never written through");
    }

    #[test]
    fn the_helper_is_compared_and_locked_along_with_its_folder() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("stage");
        std::fs::create_dir(&folder).unwrap();
        let source = temp.path().join("source");
        let copy = folder.join(COPY);
        std::fs::write(&source, b"original").unwrap();
        std::fs::write(&copy, b"tampered").unwrap();
        assert!(verify_copy(&source, &copy).is_err());
        std::fs::write(&copy, b"original").unwrap();
        let folders = hold_folders(&std::fs::canonicalize(&folder).unwrap()).unwrap();
        let held = verify_copy(&source, &copy).unwrap();
        assert!(std::fs::write(&copy, b"changed").is_err());
        assert!(std::fs::remove_file(&copy).is_err());
        assert!(std::fs::rename(&folder, temp.path().join("moved")).is_err());
        drop((held, folders));
    }

    #[test]
    fn a_locked_installer_cannot_be_changed_or_replaced() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("setup.exe");
        std::fs::write(&path, b"MZ").unwrap();
        let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
        assert!(std::fs::write(&path, b"changed").is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(held);
        assert!(std::fs::remove_file(&path).is_ok());
    }

    // These stop before any program is started: a file that is not an installer, and arguments the copy refuses.
    #[test]
    fn a_file_that_is_not_an_installer_is_never_run() {
        let temp = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let archive = temp.path().join("Faber-setup.exe");
        std::fs::write(&archive, b"PK\x03\x04").unwrap();
        let dir = OsString::from(r"/D=E:\Apps\Faber");
        assert_eq!(start(&archive, "", "", &dir, data.path(), false), Err("the update is not an installer".into()));
        assert!(start(&temp.path().join("missing.exe"), "", "", &dir, data.path(), false).is_err(), "no file, nothing to run");
        assert!(!temp.path().join(COPY).exists(), "no copy of the program was made for it");
    }

    #[test]
    fn a_hand_off_without_an_install_folder_starts_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let installer = temp.path().join("Faber-setup.exe");
        std::fs::write(&installer, b"MZ").unwrap();
        assert!(start(&installer, "", "", &OsString::from(r"E:\Apps\Faber"), data.path(), true).is_err(), "not a /D= argument");
        assert!(!temp.path().join(COPY).exists());
        assert!(relaunch(&OsString::from("/D="), data.path(), true).is_err());
    }

    #[test]
    fn the_copy_refuses_arguments_it_does_not_know() {
        assert_eq!(finish(&[]), 1);
        assert_eq!(finish(&[OsString::from("only"), OsString::from("three"), OsString::from("args")]), 1);
    }
}
