//! The fake CLI: stands in for a CLI binary and replays a recorded session over its own stdin and stdout.
//!
//! A `*.setup.json` recording also describes the commands the setup guide runs on a CLI (`--version`, the sign-in status
//! and the sign-in itself). Their state is only a marker file, never a credential, and each scenario gets its own folder.

use crate::replay::{Entry, Replay, load};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Setup {
    pub vendor: String,
    pub version: String,
    /// The conversation to replay, relative to the setup file.
    pub chat: String,
    /// The recording that answers `model/list`, relative to the setup file.
    pub models: Option<String>,
}

pub struct Env {
    /// `FABER_FAKE_RECORDING`: a `*.jsonl` recording, or a `*.setup.json` file.
    pub recording: PathBuf,
    /// `FABER_FAKE_STATE_DIR`: where a setup recording keeps its marker and its list of commands.
    pub state_dir: Option<PathBuf>,
}

/// Decides which recording a line belongs to. A short model enumeration and a chat use separate Codex app-server
/// processes, so a Codex setup run answers the handshake itself and picks the recording from the first real request.
pub struct Dispatch {
    entries: Vec<Entry>,
    models: Option<PathBuf>,
    replay: Replay,
    selected: bool,
    connected: bool,
}

impl Dispatch {
    pub fn new(entries: Vec<Entry>, models: Option<PathBuf>, wait_for_first_request: bool, connected: bool) -> Dispatch {
        Dispatch { replay: Replay::new(entries.clone()), entries, models, selected: !wait_for_first_request, connected }
    }

    pub fn mismatches(&self) -> &[String] {
        &self.replay.mismatches
    }

    pub fn feed(&mut self, line: &str) -> io::Result<Vec<String>> {
        let request: Value = serde_json::from_str(line).map_err(|e| io::Error::other(format!("not JSON ({e}): {line}")))?;
        if matches!(request["method"].as_str(), Some("account/read" | "account/rateLimits/read")) {
            let result = if request["method"] == "account/read" {
                json!({ "account": if self.connected { json!({ "type": "chatgpt", "planType": "pro" }) } else { Value::Null } })
            } else {
                let limits = self
                    .entries
                    .iter()
                    .find(|e| !e.input && e.msg["method"] == "account/rateLimits/updated")
                    .map(|e| e.msg["params"]["rateLimits"].clone())
                    .unwrap_or(Value::Null);
                json!({ "rateLimits": if self.connected { limits } else { Value::Null } })
            };
            return Ok(vec![json!({ "id": request["id"], "result": result }).to_string()]);
        }
        if self.selected {
            return self.replay.feed(line);
        }
        match request["method"].as_str() {
            Some("initialize") => {
                let result = json!({ "userAgent": "faber-setup-replay", "platformFamily": "windows", "platformOs": "windows" });
                return Ok(vec![json!({ "id": request["id"], "result": result }).to_string()]);
            }
            Some("initialized") => return Ok(vec![]),
            Some("model/list") => {
                if let Some(path) = &self.models {
                    self.entries = load(path)?;
                }
            }
            _ => {}
        }
        // The handshake was answered above; the recording starts after its own.
        let start = self.entries.iter().position(|e| e.input && e.msg["method"] == "initialized").map_or(0, |i| i + 1);
        self.replay = Replay::new(self.entries[start..].to_vec());
        self.selected = true;
        self.replay.feed(line)
    }
}

pub fn run(env: &Env, args: &[String], stdin: &mut dyn BufRead, stdout: &mut dyn Write, stderr: &mut dyn Write) -> io::Result<i32> {
    if !env.recording.to_string_lossy().ends_with(".setup.json") {
        if args.iter().map(String::as_str).eq(["auth", "status"]) {
            writeln!(stdout, "{}", json!({ "loggedIn": true, "subscriptionType": "max", "authMethod": "claude.ai" }))?;
            return Ok(0);
        }
        // The real CLI prints its help and exits. Replaying a session here would wait for input until the startup probe gave
        // up, leaving a live process (and its console host) behind for the audit to find. The help names no newer flags, so the
        // app keeps its ordinary arguments in every screen test.
        if args.iter().map(String::as_str).eq(["--help"]) {
            writeln!(stdout, "Usage: claude [options] [prompt]\n\nOptions:\n  -p, --print   Print the response and exit")?;
            return Ok(0);
        }
        return session(Dispatch::new(load(&env.recording)?, None, false, true), env.state_dir.as_deref(), false, stdin, stdout, stderr);
    }
    let setup: Setup = serde_json::from_str(&std::fs::read_to_string(&env.recording)?).map_err(io::Error::other)?;
    let state = env.state_dir.as_deref().ok_or_else(|| io::Error::other("Setup replay requires an isolated FABER_FAKE_STATE_DIR"))?;
    let code = setup_run(&setup, env, state, args, stdin, stdout, stderr);
    // Windows reuses process ids: once this one is gone, its id must not count as a leaked CLI.
    append(state, &json!({ "exited": std::process::id() }))?;
    code
}

fn setup_run(
    setup: &Setup,
    env: &Env,
    state: &Path,
    args: &[String],
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<i32> {
    let claude = setup.vendor == "claude";
    let marker = state.join(format!("{}.signed-in", setup.vendor));
    let audit = state.join("commands.jsonl");
    let status: &[&str] = if claude { &["auth", "status"] } else { &["login", "status"] };
    let login: &[&str] = if claude { &["auth", "login"] } else { &["login"] };
    let is = |expected: &[&str]| args.iter().map(String::as_str).eq(expected.iter().copied());
    // What this command may write, so the runner can check it stayed inside the sandbox.
    let mut targets = vec![audit.to_string_lossy().into_owned()];
    if is(login) {
        targets.push(marker.to_string_lossy().into_owned());
    }
    let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    append(state, &json!({ "pid": std::process::id(), "cwd": cwd, "vendor": setup.vendor, "args": args, "targets": targets }))?;
    if args.first().map(String::as_str) == Some("--version") {
        writeln!(stdout, "{}", setup.version)?;
        return Ok(0);
    }
    if is(status) {
        let connected = marker.exists();
        if claude {
            writeln!(
                stdout,
                "{}",
                json!({ "loggedIn": connected, "subscriptionType": if connected { Some("max") } else { None }, "authMethod": "claude.ai" })
            )?;
        } else {
            writeln!(stderr, "{}", if connected { "Logged in using ChatGPT" } else { "Not logged in" })?;
        }
        return Ok(if connected { 0 } else { 1 });
    }
    if is(login) {
        std::fs::write(&marker, "true")?;
        writeln!(stdout, "Fake sign-in complete")?;
        return Ok(0);
    }
    let dir = env.recording.parent().unwrap_or(Path::new("."));
    let models = setup.models.as_ref().map(|m| dir.join(m));
    session(Dispatch::new(load(&dir.join(&setup.chat))?, models, !claude, marker.exists()), Some(state), true, stdin, stdout, stderr)
}

/// A recorded line with this key is not sent: the replay stops there until the scenario creates `RELEASE` in the state
/// folder (at most a minute), so a screen check can look at the app in the middle of an answer.
const HOLD: &str = "faber_hold";
const RELEASE: &str = "release";

/// Whether a recorded line is the hold marker itself: its own field, not a mention of it inside some text.
fn is_hold(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|v| v.get(HOLD).is_some())
}

fn hold(state: Option<&Path>) {
    let Some(file) = state.map(|s| s.join(RELEASE)) else { return };
    for _ in 0..1200 {
        if file.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Replays until stdin ends. A setup run exits 1 on a difference; a plain run only reports it on stderr.
fn session(
    mut dispatch: Dispatch,
    state: Option<&Path>,
    strict: bool,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<i32> {
    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        for out in dispatch.feed(&line)? {
            if is_hold(&out) {
                stdout.flush()?;
                hold(state);
            } else {
                writeln!(stdout, "{out}")?;
            }
        }
        stdout.flush()?;
    }
    let mismatches = dispatch.mismatches();
    if !mismatches.is_empty() {
        writeln!(stderr, "{}", mismatches.join("\n"))?;
    }
    Ok(if strict && !mismatches.is_empty() { 1 } else { 0 })
}

fn append(state: &Path, record: &Value) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(state.join("commands.jsonl"))?;
    // One write for the whole line: `writeln!` on a file writes it in pieces, and the runner reading meanwhile could see
    // half a record (and, being strict about records, fail the scenario).
    let mut line = record.to_string();
    line.push('\n');
    file.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
    }

    fn drive(recording: PathBuf, state_dir: Option<PathBuf>, args: &[&str], input: &str) -> (io::Result<i32>, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let result = run(&Env { recording, state_dir }, &args, &mut input.as_bytes(), &mut out, &mut err);
        (result, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    fn records(state: &Path) -> Vec<Value> {
        std::fs::read_to_string(state.join("commands.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    // Exercise every existing recording in memory: setup-command support must not change the replay protocol.
    #[test]
    fn existing_recordings_still_replay_unchanged() {
        for vendor in ["claude", "codex"] {
            let dir = fixtures().join(vendor);
            let mut found = 0;
            for file in std::fs::read_dir(&dir).unwrap().map(|f| f.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
            {
                let entries = load(&file).unwrap();
                let mut replay = Replay::new(entries.clone());
                let mut output = Vec::new();
                for entry in entries.iter().filter(|e| e.input) {
                    output.extend(replay.feed(&entry.msg.to_string()).unwrap());
                }
                let recorded: Vec<Value> = entries.iter().filter(|e| !e.input).map(|e| e.msg.clone()).collect();
                let replayed: Vec<Value> = output.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
                assert!(replay.mismatches.is_empty(), "{}: {:?}", file.display(), replay.mismatches);
                assert_eq!(replayed, recorded, "{}", file.display());
                found += 1;
            }
            assert!(found >= 5, "{vendor}: only {found} recordings found in {}", dir.display());
        }
    }

    #[test]
    fn a_plain_recording_replays_over_stdin_and_stdout_and_reports_a_difference_without_failing() {
        let chat = fixtures().join("claude/chat.jsonl");
        let first = load(&chat).unwrap().into_iter().find(|e| e.input).unwrap().msg.to_string();
        let (code, out, err) = drive(chat.clone(), None, &["-p", "--ignored"], &format!("{first}\n"));
        assert_eq!(code.unwrap(), 0);
        assert!(out.contains("Rayleigh"), "{out}");
        assert!(err.is_empty(), "{err}");
        let (code, out, err) = drive(chat, None, &[], "{\"type\":\"user\",\"message\":{\"content\":\"something else\"}}\n");
        assert_eq!(code.unwrap(), 0);
        assert!(out.is_empty());
        assert!(err.contains("sent user \"something else\""), "{err}");
    }

    #[test]
    fn a_hold_marker_is_never_sent_and_waits_for_the_release_file() {
        let held = fixtures().join("claude/chat-held.jsonl");
        let first = load(&held).unwrap().into_iter().find(|e| e.input).unwrap().msg.to_string();
        let state = tempfile::tempdir().unwrap();
        std::fs::write(state.path().join(RELEASE), "").unwrap();
        let (code, out, err) = drive(
            held,
            Some(state.path().to_path_buf()),
            &[],
            &format!(
                "{first}
"
            ),
        );
        assert_eq!(code.unwrap(), 0);
        assert!(out.contains("Rayleigh") && !out.contains(HOLD), "{out}");
        assert!(is_hold(r#"{"faber_hold":true}"#));
        assert!(!is_hold(r#"{"type":"assistant","text":"the faber_hold marker"}"#));
        assert!(err.is_empty(), "{err}");
    }

    #[test]
    fn help_prints_and_exits_instead_of_replaying_a_session() {
        let (code, out, _) = drive(fixtures().join("claude/deny-wins.jsonl"), None, &["--help"], "");
        assert_eq!(code.unwrap(), 0);
        assert!(out.contains("Usage: claude") && !out.contains("--permission-prompts") && !out.contains("--autocompact"), "{out}");
    }

    #[test]
    fn claude_setup_commands_report_signed_out_then_signed_in_and_record_what_they_may_write() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/claude.setup.json");
        let go = |args: &[&str]| drive(setup.clone(), Some(state.path().to_path_buf()), args, "");
        let (code, out, _) = go(&["--version"]);
        assert_eq!((code.unwrap(), out.trim()), (0, "2.1.296"));
        let (code, out, _) = go(&["auth", "status"]);
        assert_eq!((code.unwrap(), out.trim()), (1, r#"{"authMethod":"claude.ai","loggedIn":false,"subscriptionType":null}"#));
        let (code, out, _) = go(&["auth", "login"]);
        assert_eq!((code.unwrap(), out.trim()), (0, "Fake sign-in complete"));
        let (code, out, _) = go(&["auth", "status"]);
        assert_eq!((code.unwrap(), out.trim()), (0, r#"{"authMethod":"claude.ai","loggedIn":true,"subscriptionType":"max"}"#));
        let log = records(state.path());
        let started: Vec<&Value> = log.iter().filter(|r| r["exited"].is_null()).collect();
        assert_eq!(started.len(), 4);
        assert_eq!(started[2]["args"], json!(["auth", "login"]));
        assert_eq!(started[2]["targets"].as_array().unwrap().len(), 2, "a login may write the marker as well as the list");
        assert_eq!(started[1]["targets"].as_array().unwrap().len(), 1);
        assert_eq!(log.iter().filter(|r| r["exited"] == json!(std::process::id())).count(), 4, "every run records its exit");
    }

    #[test]
    fn codex_setup_status_speaks_on_stderr() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/codex.setup.json");
        let (code, out, err) = drive(setup.clone(), Some(state.path().to_path_buf()), &["login", "status"], "");
        assert_eq!((code.unwrap(), out.as_str(), err.trim()), (1, "", "Not logged in"));
        drive(setup.clone(), Some(state.path().to_path_buf()), &["login"], "").0.unwrap();
        let (code, _, err) = drive(setup, Some(state.path().to_path_buf()), &["login", "status"], "");
        assert_eq!((code.unwrap(), err.trim()), (0, "Logged in using ChatGPT"));
    }

    #[test]
    fn a_codex_setup_run_answers_the_handshake_and_picks_the_recording_from_the_first_request() {
        let setup = fixtures().join("setup/codex.setup.json");
        let handshake = r#"{"id":1,"method":"initialize","params":{}}"#.to_string() + "\n" + r#"{"method":"initialized"}"# + "\n";
        let state = tempfile::tempdir().unwrap();
        let (code, out, err) = drive(
            setup.clone(),
            Some(state.path().to_path_buf()),
            &[],
            &format!("{handshake}{}\n", r#"{"id":2,"method":"model/list","params":{}}"#),
        );
        assert_eq!(code.unwrap(), 0, "{err}");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].contains("faber-setup-replay"), "{out}");
        assert!(lines[1].contains("gpt-6.1-sol"), "{out}");
        let (code, out, err) = drive(
            setup,
            Some(state.path().to_path_buf()),
            &["app-server"],
            &format!("{handshake}{}\n", r#"{"id":2,"method":"thread/start","params":{}}"#),
        );
        assert_eq!(code.unwrap(), 0, "{err}");
        assert!(out.lines().nth(1).is_some_and(|l| l.contains("remoteControl")), "{out}");
    }

    #[test]
    fn a_setup_run_exits_1_on_a_difference_and_refuses_to_start_without_a_state_folder() {
        let state = tempfile::tempdir().unwrap();
        let setup = fixtures().join("setup/claude.setup.json");
        let (code, _, err) =
            drive(setup.clone(), Some(state.path().to_path_buf()), &[], "{\"type\":\"user\",\"message\":{\"content\":\"unexpected\"}}\n");
        assert_eq!(code.unwrap(), 1);
        assert!(err.contains("sent user \"unexpected\""), "{err}");
        let (code, _, _) = drive(setup, None, &["--version"], "");
        assert!(code.unwrap_err().to_string().contains("FABER_FAKE_STATE_DIR"));
    }
}
