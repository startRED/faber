//! The install itself, on Windows: Faber hands it to a copy of itself and ends.
//!
//! The updater plugin's own `install` started the installer and only then ended Faber. The installer's first step closes every
//! running `faber.exe`, so it met a Faber still closing, could not close it, and gave up without a word: nothing was
//! installed and nothing opened again (2026-10-30: two failed installs in `update.log`, each with Windows' Restart Manager
//! event 10006 "'Faber' could not be shut down" in the same second). Now Faber keeps the installer it downloaded (its signature
//! was checked) in a new temporary folder, puts a copy of the running program (the sidecar) beside it, starts that copy with
//! `--faber-update-install`, and ends. The copy (`finish`) waits until neither the installed `faber.exe` nor its sidecar
//! runs, then runs the installer into the same folder, and opens Faber again in every case: the new version when the
//! install worked, the same version when it did not, so the person never has to guess whether to open it. The installer is
//! always silent (`/S`, `installer_line`): it shows no window, so none ever covers a game.
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
/// How long the copy waits for Faber to end by itself. Past it, what is left of the old Faber is ended (`leftovers`): the
/// installer cannot replace a program that runs, and failed every time it met one (2026-11-04: three `installer exit code 1`
/// in a row, each after a window that never closed, and the window reopened with no engine behind it).
const CLOSE_WAIT: Duration = Duration::from_secs(10);
/// How long the copy waits once more after ending what was left.
const ENDED_WAIT: Duration = Duration::from_secs(5);
/// An installer still running after this is left to finish on its own, and Faber opens anyway.
const INSTALL_WAIT: Duration = Duration::from_secs(300);
/// Windows: start the process without a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The names the installed programs are kept under, beside the copy, while the installer runs (`keep`).
const KEPT: [&str; 2] = ["faber-kept.exe", "faber-sidecar-kept.exe"];

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
    let sidecar = folder.join(SIDECAR[0]).join(SIDECAR[1]);
    leftovers(
        &data,
        [CLOSE_WAIT, ENDED_WAIT],
        || is_free(&exe) && is_free(&sidecar),
        || faber_winproc::end_programs(&[&exe, &sidecar]).unwrap_or(0),
    );
    if !installer.is_empty() {
        // The programs are kept in this copy's own folder, made new for this install.
        match std::env::current_exe().ok().and_then(|copy| copy.parent().map(Path::to_path_buf)) {
            Some(here) => {
                let sidecar = folder.join(SIDECAR[0]).join(SIDECAR[1]);
                install_or_put_back(&[&exe, &sidecar], &here, &data, || run_installer(installer, signature, version, folder, &data));
            }
            None => crate::log(&data, "install-failed", "no folder to keep Faber in"),
        }
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

/// Waits for the old Faber to end; one still running after `CLOSE_WAIT` is ended (`end` returns how many processes), since
/// both what comes next (the installer, or opening Faber again) need it gone. The person asked for the restart, or nobody
/// was looking: its work was already saved and stopped before the hand-off.
fn leftovers(data: &Path, waits: [Duration; 2], mut free: impl FnMut() -> bool, end: impl FnOnce() -> usize) {
    if wait_until(waits[0], &mut free) {
        return;
    }
    let ended = end();
    crate::log(data, "install-wait", &format!("Faber was still closing; {ended} of its processes were ended"));
    if !wait_until(waits[1], free) {
        crate::log(data, "install-wait", "Faber still runs; the installer is tried anyway");
    }
}

/// How the installer ended. Only `Failed` can have left the installed programs broken; one still running is left alone.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Installed {
    Done,
    Failed,
    /// It did not start (a bad signature, no such file): nothing was touched.
    NotRun,
    StillRunning,
}

/// Runs `install` only once the installed programs are kept in `dir`, and puts them back when it fails and left them
/// changed: an installer that ends with an error can leave `faber.exe` empty (2026-11-01: exit code 1, a 0-byte
/// `faber.exe`, and Faber could not open again until it was installed by hand). When they cannot be kept (most likely a
/// full disk) nothing is installed and the same version opens again.
fn install_or_put_back(programs: &[&Path], dir: &Path, data: &Path, install: impl FnOnce() -> Installed) -> Installed {
    let kept = match keep(programs, dir) {
        Ok(kept) => kept,
        Err(error) => {
            crate::log(data, "install-failed", &format!("Faber could not be kept before the install: {error}"));
            return Installed::NotRun;
        }
    };
    let installed = install();
    if installed == Installed::Failed {
        // Each program is tried, so one that cannot go back does not keep the other where it is.
        for (program, copy) in &kept {
            // Only a program the installer left unable to start goes back: the rest of the app (its libraries,
            // `app.asar`) may already be the new version, and an old program beside them would break a Faber that runs.
            if same(program, copy).unwrap_or(false) || is_program(program) {
                continue;
            }
            match put_back(program, copy) {
                Ok(()) => crate::log(data, "install-undone", "a program the failed install broke was put back"),
                Err(error) => crate::log(data, "put-back-failed", &error.to_string()),
            }
        }
    }
    installed
}

/// Copies each program into `dir` (as `KEPT`) and checks the copy. Returns the pairs (program, kept copy). Only a program
/// after the first (the sidecar) may be missing; one that is there but cannot be read stops the install.
fn keep(programs: &[&Path], dir: &Path) -> std::io::Result<Vec<(PathBuf, PathBuf)>> {
    let mut kept: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = (|| {
        for (at, (program, name)) in programs.iter().zip(KEPT).enumerate() {
            match std::fs::symlink_metadata(program) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && at > 0 => continue,
                Err(error) => return Err(error),
                Ok(meta) if !meta.is_file() => return Err(std::io::Error::other("the program is not a plain file")),
                Ok(_) => {}
            }
            let copy = dir.join(name);
            match copy_new(program, &copy) {
                // Someone else's file under that name is never written through, nor removed.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Err(error),
                Err(error) => {
                    let _ = std::fs::remove_file(&copy);
                    return Err(error);
                }
                Ok(()) => kept.push((program.to_path_buf(), copy.clone())),
            }
            if !same(program, &copy)? {
                return Err(std::io::Error::other("the kept copy differs from the program"));
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(kept),
        Err(error) => {
            // A copy that could not be finished (a full disk) leaves nothing of this attempt behind.
            for (_, copy) in kept {
                let _ = std::fs::remove_file(copy);
            }
            Err(error)
        }
    }
}

/// A whole Windows program: "MZ", the "PE" header it points to, and every section it lists inside the file. A program the
/// installer cut short fails the last check.
fn is_program(path: &Path) -> bool {
    whole_program(path).unwrap_or(false)
}

fn whole_program(path: &Path) -> std::io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        return Ok(false);
    }
    let pe = u64::from(u32::from_le_bytes([dos[0x3c], dos[0x3d], dos[0x3e], dos[0x3f]]));
    file.seek(SeekFrom::Start(pe))?;
    let mut head = [0u8; 24];
    file.read_exact(&mut head)?;
    if &head[..4] != b"PE\0\0" {
        return Ok(false);
    }
    let sections = u16::from_le_bytes([head[6], head[7]]);
    let optional = u64::from(u16::from_le_bytes([head[20], head[21]]));
    file.seek(SeekFrom::Start(pe + 24 + optional))?;
    let mut end = 0u64;
    for _ in 0..sections {
        let mut section = [0u8; 40];
        file.read_exact(&mut section)?;
        let size = u64::from(u32::from_le_bytes([section[16], section[17], section[18], section[19]]));
        let at = u64::from(u32::from_le_bytes([section[20], section[21], section[22], section[23]]));
        end = end.max(at + size);
    }
    Ok(len >= end)
}

/// Puts `copy` back at `program` whole or not at all: it is written beside it under another name, checked, and only then
/// renamed over it, so an error (a full disk) never leaves a half-written program. Its folder is made again if the
/// installer removed it.
fn put_back(program: &Path, copy: &Path) -> std::io::Result<()> {
    let folder = program.parent().ok_or_else(|| std::io::Error::other("no folder for the program"))?;
    std::fs::create_dir_all(folder)?;
    let staged = program.with_extension("exe.faber-put-back");
    let _ = std::fs::remove_file(&staged);
    let written = copy_new(copy, &staged).and_then(|()| {
        if same(copy, &staged)? { Ok(()) } else { Err(std::io::Error::other("the program put back differs from the kept one")) }
    });
    let renamed = written.and_then(|()| std::fs::rename(&staged, program));
    if renamed.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    renamed
}

/// Whether two files hold the same bytes, read in pieces so a big program is never held in memory whole.
fn same(a: &Path, b: &Path) -> std::io::Result<bool> {
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

fn run_installer(installer: &OsString, signature: &OsString, version: &OsString, folder: &Path, data: &Path) -> Installed {
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
            return Installed::NotRun;
        }
    };
    let began = Instant::now();
    let spawned = Command::new(installer).raw_arg(installer_line(folder)).creation_flags(CREATE_NO_WINDOW).spawn();
    drop(verified);
    match spawned {
        Ok(mut child) => {
            let mut status = None;
            wait_until(INSTALL_WAIT, || {
                status = child.try_wait().ok().flatten();
                status.is_some()
            });
            match status {
                Some(status) if status.success() => {
                    crate::log(data, "installed", &format!("in {} ms", began.elapsed().as_millis()));
                    Installed::Done
                }
                Some(status) => {
                    crate::log(data, "install-failed", &format!("installer exit code {}", status.code().unwrap_or(-1)));
                    Installed::Failed
                }
                None => {
                    crate::log(data, "install-slow", "the installer still runs; Faber opens anyway");
                    Installed::StillRunning
                }
            }
        }
        Err(error) => {
            crate::log(data, "install-failed", &error.to_string());
            Installed::NotRun
        }
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

/// The installer's command line: always silent; an update, into `folder`, with `/D=` last and unquoted, as NSIS wants.
/// Electron's one-click installer knows no passive mode: with `/P` it ends with exit code 1 and installs nothing, even with
/// Faber closed (2026-11-04, tried by hand; every install with the window open had failed that way). It takes a few
/// seconds, and Faber comes back with its window right after.
fn installer_line(folder: &Path) -> String {
    format!("/S /UPDATE /D={}", folder.display())
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
        COPY, Installed, KEPT, copy_new, finish, hold_folders, install_folder, install_or_put_back, installer_line, is_free, is_plain,
        is_program, leftovers, relaunch, start, sweep_in, verify_copy, wait_until,
    };
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn the_installer_lands_in_the_same_folder_with_d_last_and_unquoted() {
        let folder = install_folder(&OsString::from(r"/D=E:\Program Files\Faber")).unwrap();
        assert_eq!(folder, PathBuf::from(r"E:\Program Files\Faber"));
        // Always silent: Electron's installer fails with `/P` (exit code 1), which broke every restart with the window open.
        assert_eq!(installer_line(&folder), r"/S /UPDATE /D=E:\Program Files\Faber");
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

    /// An installed `faber.exe` and sidecar, a data folder, and the copy's own folder where they are kept.
    fn installed() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let (app, data, here) = (root.path().join("Faber"), root.path().join("data"), root.path().join("faber-update-1"));
        for dir in [&app.join("resources"), &data, &here] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(app.join("faber.exe"), b"MZ old program").unwrap();
        std::fs::write(app.join("resources").join("faber-sidecar.exe"), b"MZ old sidecar").unwrap();
        (root, app.join("faber.exe"), app.join("resources").join("faber-sidecar.exe"), data, here)
    }

    fn log(data: &Path) -> String {
        std::fs::read_to_string(data.join("update.log")).unwrap_or_default()
    }

    #[test]
    fn a_failed_install_that_emptied_faber_puts_the_old_programs_back() {
        let (_root, exe, sidecar, data, here) = installed();
        let installed = install_or_put_back(&[&exe, &sidecar], &here, &data, || {
            // 2026-11-01: the installer ended with code 1 and left `faber.exe` with no bytes.
            std::fs::write(&exe, b"").unwrap();
            std::fs::remove_dir_all(sidecar.parent().unwrap()).unwrap();
            Installed::Failed
        });
        assert_eq!(installed, Installed::Failed);
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program", "Faber opens again on the same version");
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"MZ old sidecar", "its removed folder is made again");
        assert!(log(&data).contains("install-undone"));
        assert!(!exe.with_extension("exe.faber-put-back").exists(), "nothing is left beside the program");
    }

    #[test]
    fn a_failed_install_that_left_a_program_that_starts_keeps_it() {
        let (_root, exe, sidecar, data, here) = installed();
        install_or_put_back(&[&exe, &sidecar], &here, &data, || {
            // The new program is whole; the rest of the app beside it may be new too, so the old one is not put back.
            std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
            Installed::Failed
        });
        assert_eq!(std::fs::read(&exe).unwrap(), std::fs::read(std::env::current_exe().unwrap()).unwrap());
        assert!(!log(&data).contains("install-undone"));
    }

    #[test]
    fn a_failed_install_that_cut_a_program_short_puts_it_back() {
        let (_root, exe, sidecar, data, here) = installed();
        install_or_put_back(&[&exe, &sidecar], &here, &data, || {
            // It starts like a program, but its sections run past the end of the file.
            let whole = std::fs::read(std::env::current_exe().unwrap()).unwrap();
            std::fs::write(&exe, &whole[..whole.len() / 2]).unwrap();
            Installed::Failed
        });
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program");
        assert!(log(&data).contains("install-undone"));
    }

    #[test]
    fn a_whole_program_is_told_from_a_cut_one() {
        let temp = tempfile::tempdir().unwrap();
        let whole = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let (good, cut, empty) = (temp.path().join("good.exe"), temp.path().join("cut.exe"), temp.path().join("empty.exe"));
        std::fs::write(&good, &whole).unwrap();
        std::fs::write(&cut, &whole[..whole.len() / 2]).unwrap();
        std::fs::write(&empty, b"").unwrap();
        assert!(is_program(&good));
        assert!(!is_program(&cut) && !is_program(&empty) && !is_program(&temp.path().join("missing.exe")));
    }

    #[test]
    fn a_kept_copy_that_could_not_be_finished_leaves_nothing_behind() {
        let (_root, exe, sidecar, data, here) = installed();
        // The sidecar's name is taken: the copy of `faber.exe` already made is removed, the planted file stays.
        std::fs::write(here.join(KEPT[1]), b"planted").unwrap();
        assert_eq!(install_or_put_back(&[&exe, &sidecar], &here, &data, || Installed::Done), Installed::NotRun);
        assert!(!here.join(KEPT[0]).exists());
        assert_eq!(std::fs::read(here.join(KEPT[1])).unwrap(), b"planted");
    }

    #[test]
    fn the_installer_never_runs_without_the_main_program_kept() {
        let (_root, exe, sidecar, data, here) = installed();
        std::fs::remove_file(&exe).unwrap();
        let mut ran = false;
        let installed = install_or_put_back(&[&exe, &sidecar], &here, &data, || {
            ran = true;
            Installed::Done
        });
        assert_eq!(installed, Installed::NotRun);
        assert!(!ran, "only the sidecar may be missing");
    }

    #[test]
    fn a_failed_install_that_touched_nothing_puts_nothing_back() {
        let (_root, exe, sidecar, data, here) = installed();
        assert_eq!(install_or_put_back(&[&exe, &sidecar], &here, &data, || Installed::Failed), Installed::Failed);
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ old program");
        assert!(!log(&data).contains("install-undone"));
    }

    #[test]
    fn a_finished_or_running_install_is_never_undone() {
        for outcome in [Installed::Done, Installed::StillRunning] {
            let (_root, exe, sidecar, data, here) = installed();
            install_or_put_back(&[&exe, &sidecar], &here, &data, || {
                std::fs::write(&exe, b"MZ new program").unwrap();
                outcome
            });
            assert_eq!(std::fs::read(&exe).unwrap(), b"MZ new program", "{outcome:?}");
        }
    }

    #[test]
    fn the_installer_never_runs_when_faber_cannot_be_kept_first() {
        let (_root, exe, sidecar, data, here) = installed();
        // Something already holds the name the copy is kept under: nothing is written through it.
        std::fs::write(here.join(KEPT[0]), b"planted").unwrap();
        let mut ran = false;
        let installed = install_or_put_back(&[&exe, &sidecar], &here, &data, || {
            ran = true;
            Installed::Done
        });
        assert_eq!(installed, Installed::NotRun);
        assert!(!ran, "no installer runs without a way back");
        assert_eq!(std::fs::read(here.join(KEPT[0])).unwrap(), b"planted");
        assert!(log(&data).contains("could not be kept"));
    }

    #[test]
    fn a_program_that_is_not_installed_is_simply_not_kept() {
        let (_root, exe, _sidecar, data, here) = installed();
        let missing = here.join("no-sidecar.exe");
        assert_eq!(install_or_put_back(&[&exe, &missing], &here, &data, || Installed::Done), Installed::Done);
        assert!(here.join(KEPT[0]).exists() && !here.join(KEPT[1]).exists());
    }

    #[test]
    fn an_old_faber_that_does_not_close_is_ended_before_the_installer_runs() {
        let data = tempfile::tempdir().unwrap();
        let short = [Duration::from_millis(150), Duration::from_secs(2)];
        // 2026-11-04: the window never closed, so the installer met it and failed. Now it is ended after the wait.
        let ended = std::cell::Cell::new(false);
        leftovers(
            data.path(),
            short,
            || ended.get(),
            || {
                ended.set(true);
                3
            },
        );
        assert!(ended.get());
        assert!(log(data.path()).contains("3 of its processes were ended"));
        // A Faber that closes by itself is never ended.
        let data = tempfile::tempdir().unwrap();
        leftovers(data.path(), short, || true, || panic!("nothing is ended when Faber closed in time"));
        assert_eq!(log(data.path()), "");
    }

    #[test]
    fn ending_programs_never_ends_one_that_is_not_named() {
        assert_eq!(faber_winproc::end_programs(&[Path::new(r"C:\no\such\faber.exe")]).unwrap(), 0);
    }

    #[test]
    fn the_copy_refuses_arguments_it_does_not_know() {
        assert_eq!(finish(&[]), 1);
        assert_eq!(finish(&[OsString::from("only"), OsString::from("three"), OsString::from("args")]), 1);
    }
}
