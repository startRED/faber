//! A local record of how Faber ended badly: `crashes.jsonl` in the data folder, one line for each panic and for each start that
//! found the last run did not exit cleanly. It never leaves this computer and nothing reads it yet.
//!
//! A line has the time, Faber's version and build commit, and for a panic the thread, the first line of the message and
//! where it came from. Never the environment or a stored conversation: the message is cut to its first line and 200
//! characters, the home folder is written `~`, and the tail that the known quoting panics print (`unwrap()` on an `Err`, a cut
//! inside a character) is dropped. A message from `expect` or `panic!` can still carry a few words of the value that failed,
//! which is why the cap is small.

use crate::toolbox::{append_capped_line, append_capped_line_without_hanging};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const CRASHES: &str = "crashes.jsonl";
/// About 500 lines of about 200 bytes; the oldest half goes at the cap (`toolbox::append_capped_line`).
const CRASHES_MAX: u64 = 100_000;
/// Exists while a run of Faber is going. A clean exit removes it; finding it at start means the last run did not end by itself.
const MARKER: &str = "running.marker";
const MESSAGE_MAX: usize = 200;
/// Where two paths that differ only in the case of a letter are the same path.
const CASE_BLIND: bool = cfg!(any(windows, target_os = "macos"));

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// `text` with the home folder written `~`, cut to `max` characters. Either slash matches either slash, the `\\?\` form of the
/// folder matches too, and with `case_blind` so does any case. A home folder under 3 characters once its trailing slashes are
/// cut (`/`, `C:`) is left alone: it would turn every slash into `~`.
fn clean(text: &str, home: &str, max: usize, case_blind: bool) -> String {
    let plain = home.strip_prefix(r"\\?\").unwrap_or(home).trim_end_matches(['/', '\\']);
    let needles: Vec<Vec<char>> =
        if plain.chars().count() < 3 { vec![] } else { vec![format!(r"\\?\{plain}").chars().collect(), plain.chars().collect()] };
    let same = |a: char, b: char| {
        a == b || (matches!(a, '/' | '\\') && matches!(b, '/' | '\\')) || (case_blind && a.to_lowercase().eq(b.to_lowercase()))
    };
    let text: Vec<char> = text.chars().collect();
    let (mut out, mut written, mut at) = (String::new(), 0, 0);
    while written < max && at < text.len() {
        let found =
            needles.iter().find(|n| text.len() - at >= n.len() && text[at..at + n.len()].iter().zip(n.iter()).all(|(a, b)| same(*a, *b)));
        out.push(found.map_or(text[at], |_| '~'));
        at += found.map_or(1, |n| n.len());
        written += 1;
    }
    out
}

/// The panics that print the value they failed on lose that tail: it can be a path or a few words of the person's text.
fn without_quoted_value(message: &str) -> &str {
    // (where the quoted part starts, how much of the marker stays)
    let cut = [("` value:", 8), (" of `", 0), (" when slicing `", 0)]
        .iter()
        .filter_map(|(mark, keep)| message.find(mark).map(|at| at + keep))
        .min();
    &message[..cut.unwrap_or(message.len())]
}

/// The line of a panic. `message` is cut to its first line, so an `assert_eq!` never writes the values it compared.
pub fn panic_line(at: u64, version: &str, commit: &str, thread: &str, message: &str, location: &str, home: &str) -> Value {
    let message = without_quoted_value(message.lines().next().unwrap_or_default());
    json!({
        "at": at, "kind": "panic", "version": version, "commit": commit, "thread": clean(thread, home, 60, CASE_BLIND),
        "message": clean(message, home, MESSAGE_MAX, CASE_BLIND), "location": clean(&location.replace('\\', "/"), home, 160, CASE_BLIND),
    })
}

/// Whether this process started a run: before that (or in a second launch, which ends before its setup) the marker on the
/// disk is the last run's or the first instance's, and a panic here must leave it.
static STARTED: AtomicBool = AtomicBool::new(false);

fn home() -> String {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default()
}

/// Writes a panic's line. When the panic ends the process (`panic = "abort"`, every release build) the marker goes too, once
/// the line is on the disk: the next start would count the same crash a second time. A panic a thread survives leaves it.
fn record_panic(dir: &Path, line: &Value, ends_the_run: bool) {
    if append_capped_line_without_hanging(&dir.join(CRASHES), &line.to_string(), CRASHES_MAX) && ends_the_run {
        end_run(dir);
    }
}

/// Makes every panic of this process leave a line in `dir`, then go on to the hook that was there (the default one prints).
/// A panic hook must not panic or hang: a failed write is dropped, and so is a line that cannot get the writer's lock.
pub fn install_hook(dir: PathBuf, version: &str, commit: &str) {
    let (version, commit, home) = (version.to_string(), commit.to_string(), home());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload.downcast_ref::<&str>().copied().or_else(|| payload.downcast_ref::<String>().map(String::as_str));
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let thread = std::thread::current();
        let line =
            panic_line(now_ms(), &version, &commit, thread.name().unwrap_or("unnamed"), message.unwrap_or_default(), &location, &home);
        record_panic(&dir, &line, cfg!(panic = "abort") && STARTED.load(Ordering::Relaxed));
        previous(info);
    }));
}

/// Starts a run: when the last one left its marker, one `unclean-exit` line says so (with that run's version and build, which
/// differ after an update), then the marker is written again for this run. Says whether the last run ended badly.
pub fn start_run(dir: &Path, version: &str, commit: &str) -> bool {
    let marker = dir.join(MARKER);
    let last = std::fs::read_to_string(&marker).ok().map(|text| serde_json::from_str::<Value>(&text).unwrap_or(Value::Null));
    if let Some(last) = &last {
        let of = |key: &str| last[key].as_str().unwrap_or("unknown").chars().take(64).collect::<String>();
        let line = json!({ "at": now_ms(), "kind": "unclean-exit", "version": of("version"), "commit": of("commit"), "started": last["at"].as_u64() });
        append_capped_line(&dir.join(CRASHES), &line.to_string(), CRASHES_MAX);
    }
    let _ = std::fs::write(&marker, json!({ "at": now_ms(), "version": version, "commit": commit }).to_string());
    STARTED.store(true, Ordering::Relaxed);
    last.is_some()
}

/// Ends a run the normal way (Quit, the end of the Windows session, or the shutdown before an update installs): the next start
/// finds no marker.
pub fn end_run(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(MARKER));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(dir: &Path) -> Vec<Value> {
        let text = std::fs::read_to_string(dir.join(CRASHES)).unwrap_or_default();
        text.lines().map(|l| serde_json::from_str(l).expect("every line is whole JSON")).collect()
    }

    #[test]
    fn a_panic_line_has_only_the_fields_of_a_crash_and_nothing_of_the_person() {
        let home = r"C:\Users\Erick";
        let message = format!("cannot open {home}\\Faber\\x\nsecond line: a prompt {}", "z".repeat(500));
        let line = panic_line(7, "0.1.0", "abc1234", "tokio-runtime-worker", &message, r"C:\Users\Erick\repo\src\a.rs:3:9", home);
        let mut keys: Vec<_> = line.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["at", "commit", "kind", "location", "message", "thread", "version"]);
        assert_eq!(
            (line["kind"].as_str(), line["commit"].as_str(), line["thread"].as_str()),
            (Some("panic"), Some("abc1234"), Some("tokio-runtime-worker"))
        );
        let (message, location) = (line["message"].as_str().unwrap(), line["location"].as_str().unwrap());
        assert!(message.contains("cannot open ~\\Faber") && !message.contains("Erick") && !message.contains("second line"), "{message}");
        assert_eq!(location, "~/repo/src/a.rs:3:9");
        let long = panic_line(1, "v", "c", "t", &"é".repeat(900), "f.rs:1:1", home);
        assert_eq!(long["message"].as_str().unwrap().chars().count(), MESSAGE_MAX, "cut by characters, never in the middle of one");
    }

    #[test]
    fn the_home_folder_is_found_in_any_case_slash_and_long_form_and_a_bare_root_is_left_alone() {
        let home = r"C:\Users\Erick";
        for (text, case_blind, want) in [
            (r"open C:\Users\Erick\x", false, r"open ~\x"),
            (r"open c:\users\ERICK\x", true, r"open ~\x"),
            (r"open c:\users\ERICK\x", false, r"open c:\users\ERICK\x"),
            ("open C:/Users/Erick/x", false, "open ~/x"),
            (r"open \\?\C:\Users\Erick\x", false, r"open ~\x"),
            (r"open //?/c:/users/erick/x", true, "open ~/x"),
        ] {
            assert_eq!(clean(text, home, 200, case_blind), want, "{text}");
        }
        assert_eq!(clean(r"open \\?\C:\Users\Erick\x", r"\\?\C:\Users\Erick", 200, false), r"open ~\x", "a home in the long form too");
        for bare in ["/", "", "C:", r"C:\", "/a"] {
            assert_eq!(clean("a/b/c", bare, 200, true), "a/b/c", "{bare:?}");
        }
        assert_eq!(clean("é/ü/ö", "/", 3, true), "é/ü");
    }

    #[test]
    fn a_panic_that_quotes_the_value_it_failed_on_loses_the_quoted_tail() {
        for (message, want) in [
            (
                "called `Result::unwrap()` on an `Err` value: Os { path: \"C:\\\\Users\\\\x\" }",
                "called `Result::unwrap()` on an `Err` value:",
            ),
            (
                "byte index 1 is not a char boundary; it is inside 'é' (bytes 0..2) of `a secret prompt`",
                "byte index 1 is not a char boundary; it is inside 'é' (bytes 0..2)",
            ),
            ("begin <= end (4 <= 2) when slicing `a secret prompt`", "begin <= end (4 <= 2)"),
            ("index out of bounds: the len is 3 but the index is 5", "index out of bounds: the len is 3 but the index is 5"),
        ] {
            let line = panic_line(1, "v", "c", "t", message, "f.rs:1:1", "");
            assert_eq!(line["message"], want);
        }
    }

    #[test]
    fn a_panic_that_ends_the_run_is_counted_once_and_one_that_does_not_keeps_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let line = panic_line(1, "v", "c", "t", "boom", "f.rs:1:1", "");
        start_run(dir.path(), "1.0.0", "aaa");
        record_panic(dir.path(), &line, false);
        assert!(dir.path().join(MARKER).exists(), "a panic a thread survives is not the end of the run");
        record_panic(dir.path(), &line, true);
        assert!(!dir.path().join(MARKER).exists());
        assert!(!start_run(dir.path(), "1.0.0", "aaa"), "the abort already left its panic line, so no second line for the same crash");
        assert_eq!(lines(dir.path()).iter().map(|l| l["kind"].as_str().unwrap().to_string()).collect::<Vec<_>>(), ["panic", "panic"]);
        // A line that could not be written must not hide the crash: the marker stays for the next start to report.
        let blocked = tempfile::tempdir().unwrap();
        start_run(blocked.path(), "1.0.0", "aaa");
        std::fs::create_dir(blocked.path().join(CRASHES)).unwrap();
        record_panic(blocked.path(), &line, true);
        assert!(blocked.path().join(MARKER).exists());
    }

    #[test]
    fn a_panic_anywhere_in_the_process_is_written_to_the_file_and_the_file_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let before = std::panic::take_hook();
        install_hook(dir.path().to_path_buf(), "9.9.9", "deadbeef");
        let thread = std::thread::Builder::new().name("worker-x".into()).spawn(|| panic!("boom {}", 42)).unwrap();
        assert!(thread.join().is_err());
        let _ = std::panic::take_hook();
        std::panic::set_hook(before);
        // Tests run side by side: another test's panic in this window is not this one's.
        let found: Vec<Value> = lines(dir.path()).into_iter().filter(|l| l["thread"] == "worker-x").collect();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0]["version"].as_str(), found[0]["thread"].as_str(), found[0]["message"].as_str()),
            (Some("9.9.9"), Some("worker-x"), Some("boom 42"))
        );
        assert!(found[0]["location"].as_str().unwrap().contains("crash.rs"), "{}", found[0]["location"]);

        let old = format!("{}\n", panic_line(1, "v", "c", "t", &"x".repeat(180), "f.rs:1:1", ""));
        std::fs::write(dir.path().join(CRASHES), old.repeat(CRASHES_MAX as usize / old.len() + 1)).unwrap();
        assert!(append_capped_line(&dir.path().join(CRASHES), &json!({ "at": 2, "kind": "panic" }).to_string(), CRASHES_MAX));
        let size = std::fs::metadata(dir.path().join(CRASHES)).unwrap().len();
        assert!(size < CRASHES_MAX, "the oldest half went: {size} bytes");
        assert_eq!(lines(dir.path()).last().unwrap()["at"], 2);
    }

    #[test]
    fn only_a_start_that_finds_the_marker_writes_an_unclean_exit() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!start_run(dir.path(), "1.0.0", "aaa"), "the first start has no last run");
        assert!(dir.path().join(MARKER).exists());
        end_run(dir.path());
        assert!(!dir.path().join(MARKER).exists(), "a clean exit leaves nothing");
        assert!(!start_run(dir.path(), "1.0.1", "bbb"), "an update restart after a clean exit is not a crash");
        assert!(lines(dir.path()).is_empty());

        // The process died: no end_run. The next start, now a newer build, says which run ended.
        assert!(start_run(dir.path(), "1.0.2", "ccc"));
        let found = lines(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0]["kind"].as_str(), found[0]["version"].as_str(), found[0]["commit"].as_str()),
            (Some("unclean-exit"), Some("1.0.1"), Some("bbb"))
        );
        assert!(found[0]["started"].as_u64().unwrap() > 0 && found[0]["at"].as_u64().unwrap() > 0);
        end_run(dir.path());
        assert!(!start_run(dir.path(), "1.0.2", "ccc") && lines(dir.path()).len() == 1, "reported once, not at every start");

        // A marker a crash cut short still counts, with unknown build.
        std::fs::write(dir.path().join(MARKER), "{\"at\":").unwrap();
        assert!(start_run(dir.path(), "1.0.3", "ddd"));
        assert_eq!(lines(dir.path()).last().unwrap()["commit"], "unknown");
    }
}
