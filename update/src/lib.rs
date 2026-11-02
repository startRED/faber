//! Faber's self-updater, independent of any shell (R459). It keeps an installed Faber on the latest release without the
//! owner doing anything: the Tauri shell's updater (`src-tauri/src/update.rs`) moved here, with its own release reader in
//! place of the plugin, so the Electron shell's sidecar runs it.
//!
//! Faber opens, looks for a release before any work begins (a short timeout, so an offline start is not held up), and a
//! release it finds is downloaded and installed at once, then Faber opens again the way it was opened: the window for a
//! manual start, the tray for `--autostart`. Nothing is running yet, so nothing can be lost; a person who starts an answer
//! before the download ends gets the idle install below instead, and a running turn is never ended.
//!
//! Later, every six hours, a newer release is downloaded and kept. It installs at the first idle moment: no answer
//! running, no CLI install from the setup screen, and the window closed to the tray or minimized; or
//! at once when the person presses "Update ready, restart" ([`Updater::restart_now`]). It never shows a window on its own.
//!
//! On Windows the install is handed to a copy of the running program ([`handoff`]), which waits until the app has ended,
//! runs the NSIS installer with `/S /UPDATE /D=<folder>` and opens Faber again: with its window for a manual start or the
//! person's "Update ready, restart", in the tray for `--autostart` and the idle install. Silent (`/S`) shows no window and,
//! for the per-user installer, needs no elevation.
//!
//! Where it lands is decided by the installer: unless the command line has `/D=<dir>` it picks a folder by itself, and an
//! install whose registry key it does not find (as happened with E:\Apps\Faber) would be replaced by a second copy. Faber
//! therefore always tells the installer its own folder ([`install_dir_arg`]).
//!
//! What only the app knows (is its window on screen, is the engine at work, end the program) comes through [`App`].
//! Failures go to `update.log` in the data folder ([`log`]): a windowless app has nowhere else to say them.

pub mod flow;
pub mod gate;
#[cfg(windows)]
pub mod handoff;
pub mod release;
pub mod tried;
mod updater;

pub use gate::{SetupRunning, Who};
pub use updater::{App, Reopen, Updater};

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The screen hears of a download at most this often, and only when what it shows has changed: the chunks come many times
/// a second, and an event for each would be a flood.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(250);
/// The temporary folders of an install start with this: the installer waits in one, and the copy that runs it starts there.
const FOLDER: &str = "faber-update-";

/// Where the update is, for the screen. `Checking`, `Downloading` and `Installing` belong to the install at open (and
/// `Installing` to "Update ready, restart"); `Ready` is an update kept for an install.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
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
pub struct Status {
    pub phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mb: Option<u32>,
}

impl Status {
    pub fn of(phase: Phase) -> Self {
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

/// Turns the download's chunk callbacks into the few updates the screen needs.
#[derive(Default)]
pub struct Meter {
    done: u64,
    shown: Option<Status>,
    at: Option<std::time::Instant>,
}

impl Meter {
    /// A chunk of `chunk` bytes arrived; `total` is the file's size when the server gave one. The status to show, when it is
    /// time and it differs from what is shown.
    pub fn advance(&mut self, chunk: u64, total: Option<u64>, now: std::time::Instant) -> Option<Status> {
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

/// A release is installed only when it is newer than the running version (R156). With previews on that is plain version
/// order. With previews off a running pre-release takes only the next stable line: the weekly stable `X.Y.Z` is rebuilt from
/// whichever preview was a day old, which can be older than the `X.Y.Z-preview.N` that runs, so the same `X.Y.Z` is not
/// proof of newer code. Only a strictly greater `X.Y.Z` is. A stable install is never held back by this.
pub fn is_upgrade(current: &semver::Version, remote: &semver::Version, previews: bool) -> bool {
    let same_line = (remote.major, remote.minor, remote.patch) <= (current.major, current.minor, current.patch);
    remote > current && (previews || (remote.pre.is_empty() && (current.pre.is_empty() || !same_line)))
}

/// A downloaded update installs only when it is ready, no answer is running, and the window is not visible.
pub fn may_install(ready: bool, busy: bool, window_visible: bool) -> bool {
    ready && !busy && !window_visible
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
pub fn install_dir_arg(exe: &Path, data_dir: &Path) -> Option<OsString> {
    let dir = plain(std::fs::canonicalize(exe.parent()?).ok()?.to_str()?);
    let data = plain(std::fs::canonicalize(data_dir).ok()?.to_str()?);
    if dir.is_empty() || dir.contains('"') || (cfg!(windows) && !is_dos_path(&dir)) || overlaps(Path::new(&dir), Path::new(&data)) {
        return None;
    }
    Some(format!("/D={dir}").into())
}

/// A version as a folder name: only letters, digits, dots and dashes.
fn safe(version: &str) -> String {
    version.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

/// A new, uniquely named folder in `parent`, made exclusively: `create_dir` fails when the name exists, so a folder (or a link)
/// another process planted there is never used. The name has a random part, so it cannot be guessed ahead either.
fn staging_in(parent: &Path, version: &str) -> std::io::Result<PathBuf> {
    let folder = parent.join(format!("{FOLDER}{}-{}", safe(version), uuid::Uuid::new_v4().simple()));
    std::fs::create_dir(&folder)?;
    Ok(folder)
}

/// Where the installer of `version` is kept until it runs: its own new folder in the temporary folder (made here), so the copy
/// that runs it (`handoff`) starts beside it and a finished install is swept away whole.
fn installer_path(version: &str) -> std::io::Result<PathBuf> {
    Ok(staging_in(&std::env::temp_dir(), version)?.join("Faber-setup.exe"))
}

const LOG_MAX: usize = 32 * 1024;

/// A line of the log with nothing of the person's in it: a URL (its query can carry anything) or a path (it holds the user
/// name) becomes a marker, word by word.
fn clean(detail: &str) -> String {
    let word = |w: &str| {
        let bare = w.trim_matches(|c: char| matches!(c, '"' | '\u{27}' | '(' | ')' | '<' | '>' | '[' | ']' | ',' | ';'));
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
pub fn log(data_dir: &Path, stage: &str, detail: &str) {
    log_to(data_dir, "update.log", stage, detail);
}

/// How long each start took to show its window, kept small like `update.log`.
pub const STARTUP_LOG: &str = "startup.log";

/// `log` into another small file of the data folder.
pub fn log_to(data_dir: &Path, name: &str, stage: &str, detail: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn mb(n: u32) -> Status {
        Status { mb: Some(n), ..Status::of(Phase::Downloading) }
    }

    fn percent(n: u8) -> Status {
        Status { percent: Some(n), ..Status::of(Phase::Downloading) }
    }

    fn v(text: &str) -> semver::Version {
        text.parse().unwrap()
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
    fn a_status_goes_to_the_screen_as_the_tauri_shell_sent_it() {
        let json = |status: Status| serde_json::to_value(status).unwrap();
        assert_eq!(json(Status::of(Phase::Ready)), serde_json::json!({"phase":"ready"}));
        assert_eq!(json(percent(42)), serde_json::json!({"phase":"downloading","percent":42}));
        assert_eq!(json(mb(7)), serde_json::json!({"phase":"downloading","mb":7}));
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
        assert!(!is_upgrade(&v("1.2.3"), &v("1.2.4-preview.1"), false));
        assert!(!is_upgrade(&v("1.2.3-preview.1"), &v("1.2.4-preview.1"), false));
        // The stable 0.3.16 may be built from preview.2, so it is not newer code than a running preview.3 (R156).
        assert!(!is_upgrade(&v("0.3.16-preview.3"), &v("0.3.16"), false), "same X.Y.Z: not proven newer");
        assert!(!is_upgrade(&v("0.3.16-preview.1"), &v("0.3.16"), false));
        // Only the next stable line is.
        assert!(is_upgrade(&v("0.3.16-preview.3"), &v("0.3.17"), false));
        assert!(is_upgrade(&v("0.3.16-preview.3"), &v("0.4.0"), false));
    }

    #[test]
    fn an_update_installs_only_when_ready_idle_and_out_of_sight() {
        assert!(!may_install(true, true, false), "an answer runs");
        assert!(may_install(true, false, false), "idle with the window hidden");
        assert!(!may_install(true, false, true), "the window is visible");
        assert!(!may_install(true, true, true));
        assert!(!may_install(false, false, false), "nothing was downloaded");
    }

    #[test]
    fn a_verbatim_path_is_written_as_the_installer_reads_it() {
        assert_eq!(plain(r"\\?\E:\Apps\Faber"), r"E:\Apps\Faber");
        assert_eq!(plain(r"\\?\UNC\server\share\Faber"), r"\\server\share\Faber");
        assert_eq!(plain(r"E:\Apps\Faber"), r"E:\Apps\Faber");
        assert!(is_dos_path(r"E:\Apps") && is_dos_path(r"\\server\share\x"));
        assert!(!is_dos_path(r"\\?\Volume{1234}\x"));
    }

    #[test]
    fn a_version_cannot_leave_its_temporary_folder() {
        assert_eq!(safe("0.3.18-preview.19"), "0.3.18-preview.19");
        assert_eq!(safe(r"..\..\x"), ".._.._x");
        let temp = tempfile::tempdir().unwrap();
        let folder = staging_in(temp.path(), r"..\..\x").unwrap();
        assert_eq!(folder.parent().unwrap(), temp.path());
        assert!(folder.file_name().unwrap().to_string_lossy().starts_with(FOLDER));
    }

    #[test]
    fn every_staging_folder_is_new_and_a_name_that_exists_is_never_reused() {
        let temp = tempfile::tempdir().unwrap();
        let (a, b) = (staging_in(temp.path(), "1.0.0").unwrap(), staging_in(temp.path(), "1.0.0").unwrap());
        assert_ne!(a, b, "the same version and process still get different folders");
        assert!(a.is_dir() && b.is_dir());
        // The exclusive creation itself: a folder that is already there is an error, not a folder to use.
        assert!(std::fs::create_dir(&a).is_err());
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

    #[test]
    fn a_log_line_lands_in_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        log(dir.path(), "check-failed", "offline");
        log_to(dir.path(), STARTUP_LOG, "shown", "window=10ms");
        assert!(std::fs::read_to_string(dir.path().join("update.log")).unwrap().trim_end().ends_with("check-failed offline"));
        assert!(std::fs::read_to_string(dir.path().join(STARTUP_LOG)).unwrap().contains("shown window=10ms"));
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
            assert!(arg.ends_with("Program Files\\Faber") && !arg.contains("\\\\?\\"));
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
            // The per-user installer starts from the local app data folder; the data lives under the roaming config folder.
            assert!(!overlaps(Path::new("C:\\Users\\a\\AppData\\Local\\Faber"), Path::new("C:\\Users\\a\\AppData\\Roaming\\Faber")));
        }
    }
}
