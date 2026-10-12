//! S1 acceptance proofs. No real CLI, shell, git process, or credential file is used.
//!
//! Proposed boundary: Engine::account_status(vendor, timeout) returns a serializable
//! status; Engine::login_command(vendor) describes the official interactive login;
//! Engine::replace_clis replaces the live registry. Cli::find_in accepts a simulated
//! environment and filesystem so find_cli can delegate to the same detection logic.
//! Test-only fallback traits keep absent APIs compilable in RED. Inherent production
//! methods take precedence automatically; the fallbacks implement no new behavior.
//!
//! Official command references (checked 2026-10-10):
//! https://code.claude.com/docs/en/cli-reference
//! https://developers.openai.com/codex/cli/reference/
//! Native installation location: https://code.claude.com/docs/en/setup

mod support;

use faber_engine::cli::{self, Cli};
use faber_engine::model::{Area, Role, Settings, Status, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, TerminalCommand, WarmRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc::UnboundedReceiver;

#[allow(dead_code)] // Production inherent methods supersede these RED-only guards.
trait MissingAccountApi {
    async fn account_status(&self, vendor: Vendor, timeout: Duration) -> Value;
    fn login_command(&self, vendor: Vendor) -> Option<TerminalCommand>;
    fn replace_clis(&self, clis: BTreeMap<Vendor, Cli>);
}

impl MissingAccountApi for Engine {
    async fn account_status(&self, _vendor: Vendor, _timeout: Duration) -> Value {
        Value::Null // Missing status is asserted before interpreting it.
    }

    fn login_command(&self, _vendor: Vendor) -> Option<TerminalCommand> {
        None
    }

    fn replace_clis(&self, _clis: BTreeMap<Vendor, Cli>) {
        // The unchanged engine must fail the observable vendor-list assertion.
    }
}

#[allow(dead_code)]
trait MissingDiscoveryApi {
    fn find_in(
        name: &str,
        path: &str,
        windows: bool,
        env: &dyn Fn(&str) -> Option<String>,
        exists: &dyn Fn(&str) -> bool,
        read: &dyn Fn(&str) -> String,
    ) -> Option<Cli>;
}

impl MissingDiscoveryApi for Cli {
    fn find_in(
        name: &str,
        path: &str,
        windows: bool,
        _env: &dyn Fn(&str) -> Option<String>,
        exists: &dyn Fn(&str) -> bool,
        read: &dyn Fn(&str) -> String,
    ) -> Option<Cli> {
        cli::resolve_cli(name, path, windows, exists, read)
    }
}

fn engine(dir: &Path, clis: BTreeMap<Vendor, Cli>, spawn: Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis,
        spawn,
        windows: true,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    (Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))), rx)
}

fn native_cli() -> Cli {
    Cli { command: "C:\\Official tools\\account.exe".into(), prefix: vec![] }
}

type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

struct PendingReader;

struct FailedStatusReader(bool);

impl AsyncRead for FailedStatusReader {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        if self.0 {
            Poll::Ready(Err(std::io::Error::other("status stream failed")))
        } else {
            self.0 = true;
            buf.put_slice(b"Logged in using ChatGPT\n");
            Poll::Ready(Ok(()))
        }
    }
}

impl AsyncRead for PendingReader {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

struct ObservedOutput {
    inner: Box<dyn AsyncRead + Send + Unpin>,
    dropped: Arc<AtomicBool>,
}

impl AsyncRead for ObservedOutput {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.inner.as_mut()).poll_read(cx, buf)
    }
}

impl Drop for ObservedOutput {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

// Independent stdout/stderr streams exercise both vendors' real output channels.
// A pending reader models a CLI that never exits, without starting any processes.
fn status_spawner(stdout: &'static str, stderr: &'static str, hangs: bool) -> (Spawner, Calls) {
    let calls: Calls = Arc::default();
    let recorded = calls.clone();
    let spawn: Spawner = Arc::new(move |bin, args, _cwd| {
        recorded.lock().unwrap().push((bin.into(), args.to_vec()));
        if hangs {
            return Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(PendingReader), stderr: None, child: None });
        }
        let (mut out, stdout_reader) = tokio::io::duplex(4096);
        let (mut err, stderr_reader) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let _ = out.write_all(stdout.as_bytes()).await;
            let _ = err.write_all(stderr.as_bytes()).await;
        });
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(stdout_reader), stderr: Some(Box::new(stderr_reader)), child: None })
    });
    (spawn, calls)
}

fn status_args(vendor: Vendor) -> Vec<String> {
    match vendor {
        Vendor::Claude => vec!["auth".into(), "status".into()],
        Vendor::Codex => vec!["login".into(), "status".into()],
    }
}

#[tokio::test]
async fn c1_1_signed_in_uses_only_official_status_arguments_through_the_spawner() {
    for (vendor, stdout, stderr, login_args) in [
        (Vendor::Claude, "{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"apiProvider\":\"firstParty\"}\n", "", vec!["auth", "login"]),
        (Vendor::Codex, "", "Logged in using ChatGPT\n", vec!["login"]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (spawn, calls) = status_spawner(stdout, stderr, false);
        // Prefix arguments are executable launch details, not a shell command.
        let cli = Cli { command: "C:\\Official tools\\node.exe".into(), prefix: vec!["C:\\Official tools\\entry.js".into()] };
        let (engine, mut events) = engine(dir.path(), BTreeMap::from([(vendor, cli.clone())]), spawn);
        let status = serde_json::to_value(engine.account_status(vendor, Duration::from_millis(100)).await).unwrap();
        assert!(!status.is_null(), "C1.1: Engine::account_status is missing");
        assert_eq!(status, json!("signedIn"));
        let mut expected_args = cli.prefix.clone();
        expected_args.extend(status_args(vendor));
        assert_eq!(*calls.lock().unwrap(), vec![(cli.command.clone(), expected_args)]);
        let login = engine.login_command(vendor);
        assert!(login.is_some(), "the official login command must be available");
        let login = login.unwrap();
        assert_eq!(login.command, cli.command);
        assert_eq!(login.args, cli.prefix.into_iter().chain(login_args.into_iter().map(String::from)).collect::<Vec<_>>());
        assert!(events.try_recv().is_err(), "account output must not reach application events");
        assert!(engine.state().conversations.is_empty());
        assert_eq!(std::fs::read_dir(dir.path().join("conversations")).unwrap().count(), 0);
        assert!(!dir.path().join("settings.json").exists(), "checking an account must not persist output");
        engine.shutdown();
    }
}

#[tokio::test]
async fn c1_2_signed_out_and_unresponsive_accounts_finish_within_the_timeout() {
    for (vendor, stdout, stderr) in [
        (Vendor::Claude, "{\"loggedIn\":false,\"authMethod\":\"none\",\"apiProvider\":\"firstParty\"}\n", ""),
        (Vendor::Codex, "", "Not logged in\n"),
    ] {
        for (hangs, expected) in [(false, "signedOut"), (true, "unknown")] {
            let dir = tempfile::tempdir().unwrap();
            let (spawn, calls) = status_spawner(if hangs { "" } else { stdout }, if hangs { "" } else { stderr }, hangs);
            let (engine, _) = engine(dir.path(), BTreeMap::from([(vendor, native_cli())]), spawn);
            let began = Instant::now();
            let result = tokio::time::timeout(Duration::from_millis(500), engine.account_status(vendor, Duration::from_millis(40))).await;
            assert!(result.is_ok(), "C1.2: account check ignored its 40 ms timeout");
            let status = serde_json::to_value(result.unwrap()).unwrap();
            assert!(!status.is_null(), "C1.2: Engine::account_status is missing");
            assert_eq!(status, json!(expected));
            assert!(began.elapsed() < Duration::from_millis(500));
            assert_eq!(*calls.lock().unwrap(), vec![(native_cli().command, status_args(vendor))]);
            engine.shutdown();
        }
    }
}

#[tokio::test]
async fn account_status_reads_a_complete_json_object_without_mixing_stderr_into_it() {
    for (stdout, expected) in [
        ("{\n  \"loggedIn\": true,\n  \"authMethod\": \"claude.ai\",\n  \"apiProvider\": \"firstParty\"\n}\n", "signedIn"),
        ("{\n  \"loggedIn\": false,\n  \"authMethod\": \"none\",\n  \"apiProvider\": \"firstParty\"\n}\n", "signedOut"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (spawn, _) = status_spawner(stdout, "Diagnostic on stderr\n", false);
        let (engine, _) = engine(dir.path(), BTreeMap::from([(Vendor::Claude, native_cli())]), spawn);
        assert_eq!(serde_json::to_value(engine.account_status(Vendor::Claude, Duration::from_millis(100)).await).unwrap(), json!(expected));
        engine.shutdown();
    }
}

#[tokio::test]
async fn account_status_rejects_failed_or_oversized_output() {
    for failed in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let spawn: Spawner = Arc::new(move |_, _, _| {
            let stdout: Box<dyn AsyncRead + Send + Unpin> = if failed {
                Box::new(FailedStatusReader(false))
            } else {
                Box::new(std::io::Cursor::new("Logged in using ChatGPT\n".repeat(5000).into_bytes()))
            };
            Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout, stderr: None, child: None })
        });
        let (engine, _) = engine(dir.path(), BTreeMap::from([(Vendor::Codex, native_cli())]), spawn);
        assert_eq!(serde_json::to_value(engine.account_status(Vendor::Codex, Duration::from_millis(100)).await).unwrap(), json!("unknown"));
        engine.shutdown();
    }
}

#[tokio::test]
async fn replacing_an_active_shared_server_retires_it_as_soon_as_its_answer_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let mut entries: Vec<_> = support::fixture("codex", "chat").into_iter().take_while(|e| !(e.input && e.msg["id"] == 4)).collect();
    // Closing a thread asks the existing driver to interrupt its last known turn.
    entries.push(support::Entry {
        input: true,
        msg: json!({
            "id": 4, "method": "turn/interrupt", "params": {
                "threadId": "01a12513-0e42-7b63-9a21-c8698f522c12", "turnId": "01a12513-0ee8-7061-88de-3e38fd266879"
            }
        }),
    });
    let replay = support::fake(entries);
    let dropped = Arc::new(AtomicBool::new(false));
    let spawn: Spawner = {
        let replay = replay.spawn.clone();
        let observed = dropped.clone();
        Arc::new(move |bin, args, cwd| {
            let mut proc = replay(bin, args, cwd)?;
            proc.stdout = Box::new(ObservedOutput { inner: proc.stdout, dropped: observed.clone() });
            Ok(proc)
        })
    };
    let (engine, mut events) = engine(dir.path(), BTreeMap::from([(Vendor::Codex, native_cli())]), spawn);
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "In one sentence: why is the sky blue?".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "codex".into(), model: None },
        })
        .unwrap();
    // The current-thread runtime has not polled the replay yet: the turn is in flight.
    engine.replace_clis(BTreeMap::new());
    assert!(engine.state().vendors.is_empty());
    let reply = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(event) = events.recv().await {
            if let AppEvent::Message { message, .. } = event
                && message.role == Role::Assistant
                && message.status == Some(Status::Done)
            {
                return message;
            }
        }
        panic!("the active answer ended before completion");
    })
    .await
    .expect("the active answer must finish on the old process");
    assert_eq!(
        reply.text,
        "The sky appears blue because air molecules scatter shorter blue wavelengths of sunlight more than longer red wavelengths."
    );
    let retired = tokio::time::timeout(Duration::from_millis(500), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    engine.shutdown();
    assert!(retired.is_ok(), "the old shared process remained alive after its last answer finished");
    assert!(replay.mismatches.lock().unwrap().is_empty());
}

#[test]
fn c1_3_default_installations_outside_path_resolve_to_a_program_without_a_shell() {
    let env = |key: &str| match key {
        "USERPROFILE" => Some("C:\\Users\\Example User".into()),
        "APPDATA" => Some("C:\\Users\\Example User\\AppData\\Roaming".into()),
        "HOME" => Some("/home/example".into()),
        _ => None,
    };
    let cases = [
        ("claude", true, "C:\\Users\\Example User\\.local\\bin\\claude.exe", "C:\\Users\\Example User\\.local\\bin\\claude.exe", vec![]),
        ("claude", false, "/home/example/.local/bin/claude", "/home/example/.local/bin/claude", vec![]),
        (
            "codex",
            true,
            "C:\\Users\\Example User\\AppData\\Roaming\\npm\\codex.cmd",
            "C:\\Users\\Example User\\AppData\\Roaming\\npm\\node.exe",
            vec!["C:\\Users\\Example User\\AppData\\Roaming\\npm\\node_modules\\@openai\\codex\\bin\\codex.js"],
        ),
    ];
    for (name, windows, installed, command, prefix) in cases {
        let exists = |p: &str| p == installed || p == command || prefix.contains(&p);
        let read = |p: &str| {
            assert_eq!(p, installed, "only the npm executable shim may be read");
            "\"%dp0%\\node.exe\" \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*".into()
        };
        let found = Cli::find_in(name, "", windows, &env, &exists, &read);
        assert!(found.is_some(), "C1.3: {name} in its default installation directory was not found outside PATH");
        assert_eq!(found.unwrap(), Cli { command: command.into(), prefix: prefix.into_iter().map(String::from).collect() });
    }
}

#[tokio::test]
async fn c1_4_replacing_clis_updates_state_warm_and_send_without_recreating_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let replay = support::fake(support::fixture("claude", "chat"));
    let codex = support::fake(support::fixture("codex", "chat"));
    let old_dropped = Arc::new(AtomicBool::new(false));
    let calls: Calls = Arc::default();
    let spawn: Spawner = {
        let claude_spawn = replay.spawn.clone();
        let codex_spawn = codex.spawn.clone();
        let observed = old_dropped.clone();
        let recorded = calls.clone();
        Arc::new(move |bin, args, cwd| {
            recorded.lock().unwrap().push((bin.into(), args.to_vec()));
            if bin.ends_with("codex.exe") {
                let mut proc = codex_spawn(bin, args, cwd)?;
                if bin == "old-codex.exe" {
                    proc.stdout = Box::new(ObservedOutput { inner: proc.stdout, dropped: observed.clone() });
                }
                Ok(proc)
            } else {
                claude_spawn(bin, args, cwd)
            }
        })
    };
    let (engine, mut events) = engine(dir.path(), BTreeMap::new(), spawn);
    assert!(engine.state().vendors.is_empty());
    engine.replace_clis(BTreeMap::from([(Vendor::Claude, native_cli())]));
    assert_eq!(engine.state().vendors, vec![Vendor::Claude], "C1.4: the existing engine still exposes the old CLI registry");
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()) };
    engine.warm(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice.clone() });
    assert_eq!(replay.args.lock().unwrap().len(), 1, "warm must use the new registry");
    let sent = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: "In one sentence: why is the sky blue?".into(),
        images: vec![],
        files: vec![],
        choice,
    });
    assert!(sent.is_ok(), "send must use the replacement subscription: {sent:?}");
    let reply = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(event) = events.recv().await {
            if let AppEvent::Message { message, .. } = event
                && message.role == Role::Assistant
                && message.status == Some(Status::Done)
            {
                return Some(message);
            }
        }
        None
    })
    .await;
    assert!(reply.is_ok(), "the recording did not finish");
    let reply = reply.unwrap().expect("engine stopped before replying");
    assert!(reply.text.contains("Rayleigh"));
    assert_eq!(reply.vendor, Some(Vendor::Claude));
    assert_eq!(replay.args.lock().unwrap().len(), 1, "send must reuse the warmed process");
    assert!(replay.mismatches.lock().unwrap().is_empty());

    // Replacing an already warmed shared server must retire its old process too.
    engine.replace_clis(BTreeMap::from([(Vendor::Codex, Cli { command: "old-codex.exe".into(), prefix: vec![] })]));
    let warm = || WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "codex".into(), model: None },
    };
    engine.warm(warm());
    assert!(!old_dropped.load(Ordering::SeqCst));
    engine.replace_clis(BTreeMap::from([(Vendor::Codex, Cli { command: "new-codex.exe".into(), prefix: vec![] })]));
    let retired = tokio::time::timeout(Duration::from_millis(500), async {
        while !old_dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(retired.is_ok(), "the old shared process stayed alive after replacing the registry");
    engine.warm(warm());
    let programs: Vec<String> = calls.lock().unwrap().iter().map(|(bin, _)| bin.clone()).collect();
    assert_eq!(programs, vec![native_cli().command, "old-codex.exe".into(), "new-codex.exe".into()]);
    assert_eq!(engine.state().vendors, vec![Vendor::Codex]);
    engine.replace_clis(BTreeMap::new());
    assert!(engine.state().vendors.is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn c1_5_saving_another_setting_preserves_skipped_setup_on_disk_and_in_app_state() {
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    std::fs::write(&settings_path, json!({"setup":"skipped", "language":"en"}).to_string()).unwrap();
    let (engine, _) = engine(dir.path(), BTreeMap::new(), Arc::new(|_, _, _| panic!("saving settings must not launch a CLI")));
    let patch: Settings = serde_json::from_value(json!({"userHooks":true})).unwrap();
    let saved = serde_json::to_value(engine.save_settings(patch).unwrap()).unwrap();
    assert_eq!(saved["setup"], "skipped", "C1.5: save_settings discarded the existing setup decision");
    assert_eq!(saved["language"], "en");
    assert_eq!(saved["userHooks"], true);
    let disk: Value = serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(disk["setup"], "skipped");
    assert_eq!(serde_json::to_value(engine.state().settings).unwrap()["setup"], "skipped");
    engine.shutdown();
}
