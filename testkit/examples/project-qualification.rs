//! Manual hidden qualification observations. An observation never opens role admission.
use std::io;
#[cfg(any(windows, test))]
use std::path::Path;
#[cfg(any(windows, test))]
#[path = "project-qualification/live.rs"]
mod live;
#[cfg(any(windows, test))]
#[path = "project-qualification/claude.rs"]
mod live_claude;

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    #[cfg(windows)]
    let result = run();
    #[cfg(not(windows))]
    let result: io::Result<()> = Err(io::Error::other("requires the Windows hidden desktop"));
    if let Err(error) = result {
        eprintln!("Qualification failed ({error}); inspect the private receipt for requested model turns.");
        std::process::exit(1);
    }
}

#[cfg(any(windows, test))]
fn protected(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| [".claude", ".codex", ".gemini"].iter().any(|blocked| name.eq_ignore_ascii_case(blocked)))
    })
}

#[cfg(any(windows, test))]
fn random_marker() -> String {
    use std::hash::BuildHasher;
    // Independent randomly seeded hashers keep the sentinel absent from the prompt and executable bytes.
    let first = std::collections::hash_map::RandomState::new().hash_one("outside-marker");
    let second = std::collections::hash_map::RandomState::new().hash_one("outside-marker");
    format!("T13_MARKER_{first:016x}{second:016x}")
}

#[cfg(any(windows, test))]
fn fingerprint(path: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    // R112 permits only the resolved program file here; working-folder restrictions belong elsewhere.
    let path = std::fs::canonicalize(path)?;
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(any(windows, test))]
fn version(text: &str) -> io::Result<&str> {
    let text = text.trim();
    if text.is_empty() || text.len() > 128 || !text.bytes().all(|c| c.is_ascii_graphic() || c == b' ') {
        return Err(io::Error::other("invalid version output"));
    }
    Ok(text)
}

#[cfg(windows)]
fn run() -> io::Result<()> {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [mode, directory] = args.as_slice() else {
        return Err(io::Error::other("expected --preflight PRIVATE_DATA"));
    };
    let data = std::path::PathBuf::from(directory);
    if !data.is_absolute()
        || !matches!(
            mode.as_str(),
            "--preflight"
                | "--zero-turn-admission"
                | "--hidden-worker"
                | "--live-loadout"
                | "--live-native"
                | "--live-role-tools"
                | "--live-claude-loadout"
                | "--live-claude-native"
                | "--live-claude-stop"
                | "--live-claude-context"
                | "--live-claude-quota"
                | "--live-claude-output-lead"
                | "--live-claude-output-writer"
                | "--live-claude-output-reviewer"
                | "--live-claude-output-control"
                | "--live-codex-stop"
                | "--hidden-live"
        )
    {
        return Err(io::Error::other("invalid preflight arguments"));
    }
    if matches!(mode.as_str(), "--hidden-worker" | "--hidden-live") {
        if std::env::var("FABER_QUALIFICATION_HIDDEN").as_deref() != Ok("preflight") {
            return Err(io::Error::other("hidden runner required"));
        }
        return tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            if mode == "--hidden-live" {
                if std::env::var("FABER_QUALIFICATION_PROBE").is_ok_and(|mode| mode.starts_with("--live-claude")) {
                    live_claude::run(&data).await
                } else {
                    live::run(&data).await
                }
            } else {
                worker(&data).await
            }
        });
    }
    if ["CI", "GITHUB_ACTIONS"].iter().any(|name| std::env::var(name).is_ok_and(|v| v.eq_ignore_ascii_case("true"))) {
        return Err(io::Error::other("manual preflight never runs in CI"));
    }
    std::fs::create_dir_all(&data)?;
    if data.join("qualification-preflight.json").exists() {
        return Err(io::Error::other("receipt already exists; no automatic replay"));
    }
    if data.join("live-denial.json").exists() {
        return Err(io::Error::other("live receipt already exists; no automatic replay"));
    }
    let executable = std::env::current_exe()?;
    let helper = executable.parent().and_then(Path::parent).ok_or_else(|| io::Error::other("missing debug folder"))?.join("fake-cli.exe");
    let mut env = BTreeMap::new();
    // Directory/runtime names only; no API keys, tokens, loader options or provider overrides.
    for name in
        ["PATH", "SystemRoot", "WINDIR", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "ProgramFiles", "ProgramFiles(x86)"]
    {
        if let Ok(value) = std::env::var(name) {
            env.insert(name.to_string(), value);
        }
    }
    env.insert("FABER_QUALIFICATION_HIDDEN".into(), "preflight".into());
    env.insert("FABER_QUALIFICATION_PROBE".into(), mode.clone());
    env.insert(
        "FABER_QUALIFICATION_WORKSPACE".into(),
        std::env::current_dir()?
            .join("target/t13-evidence/live-qualification")
            .join(data.file_name().ok_or_else(|| io::Error::other("missing probe name"))?)
            .to_string_lossy()
            .into_owned(),
    );
    if let Some(path) = env.get("PATH") {
        let permitted: Vec<_> = std::env::split_paths(path).filter(|p| !protected(p)).collect();
        env.insert("PATH".into(), std::env::join_paths(permitted).map_err(io::Error::other)?.to_string_lossy().into_owned());
    }
    let (owned, _, stderr) = faber_testkit::contain::Owned::start(
        &executable,
        &[if mode.starts_with("--live-") { "--hidden-live".into() } else { "--hidden-worker".into() }, data.to_string_lossy().into_owned()],
        &env,
        &data,
        &helper,
    )?;
    let diagnostics = std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        if let Some(stderr) = stderr {
            let _ = stderr.take(4096).read_to_string(&mut text);
        }
        text
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    while owned.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let completed = !owned.is_running() && owned.status().as_deref() == Some("exit code: 0");
    owned.terminate();
    let diagnostics = diagnostics.join().unwrap_or_default();
    if !completed {
        return Err(io::Error::other(format!("bounded hidden worker did not complete: {diagnostics}")));
    }
    println!("Private qualification observation recorded. Admission remains blocked.");
    Ok(())
}

#[cfg(windows)]
async fn worker(data: &Path) -> io::Result<()> {
    use faber_engine::model::Vendor;
    use serde_json::json;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    if std::env::var("FABER_QUALIFICATION_PROBE").as_deref() == Ok("--zero-turn-admission") {
        let clis = [(Vendor::Claude, "claude"), (Vendor::Codex, "codex")]
            .into_iter()
            .map(|(vendor, name)| {
                faber_engine::cli::find_cli(name).map(|cli| (vendor, cli)).ok_or_else(|| io::Error::other("CLI unavailable"))
            })
            .collect::<io::Result<std::collections::BTreeMap<_, _>>>()?;
        let engine = faber_engine::engine::Engine::new(
            faber_engine::engine::EngineOptions {
                data_dir: data.join("engine"),
                clis,
                spawn: faber_engine::proc::real_spawner(),
                windows: true,
                claude_env_billed: false,
                max_warm: 1,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            std::sync::Arc::new(|_| {}),
        );
        engine.check_plans(Duration::from_secs(20)).await;
        let mut rows = vec![];
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_secs_f64();
        for vendor in Vendor::ALL {
            let l = engine.ledger();
            let v = l.of(vendor);
            if !v.spans.is_empty() {
                return Err(io::Error::other("no-model account check recorded a model span"));
            }
            let model = match vendor {
                Vendor::Claude => "claude-haiku-5-5",
                Vendor::Codex => "gpt-6-luna",
            };
            let admitted = engine.mission_budget(faber_engine::project::budget::Change::AllocateModel {
                vendor,
                model: model.into(),
                cap_micros: 2_000_000,
            });
            let serialized = serde_json::to_value(v)?;
            let weekly = v.readings.get("seven_day");
            let line = weekly.and_then(|r| {
                faber_engine::budget::daily_start(r.resets_at, now).map(|start| (start + 86400.0 - (r.resets_at - 604800.0)) / 604800.0)
            });
            rows.push(json!({"vendor":vendor,"billing":format!("{:?}",v.billing),"paidFallback":v.paid_fallback,
                "readings":v.readings,"zeroTurnReadings":serialized["zero_turn_readings"],"line":line,
                "margin":line.zip(weekly).map(|(line,r)|line-r.used),"r130Admitted":admitted.is_ok(),
                "reason":admitted.err(),"modelTurns":0}));
        }
        engine.shutdown();
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(data.join("zero-turn-admission.json"))?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&json!({"schema":1,"measuredAt":now,"missionCapMicros":2_000_000,"records":rows}))?)?;
        file.sync_all()?;
        return Ok(());
    }
    let spawn = faber_engine::proc::real_spawner();
    let mut records = vec![];
    for (name, vendor) in [("claude", Vendor::Claude), ("codex", Vendor::Codex)] {
        let cli = faber_engine::cli::find_cli(name).ok_or_else(|| io::Error::other("CLI unavailable"))?;
        let binary_hash = match fingerprint(Path::new(&cli.command)) {
            Ok(hash) => hash,
            Err(error) => {
                records.push(json!({"vendor":vendor,"version":null,"binaryHash":null,
                    "fingerprintError":error.to_string(),"qualificationPassed":false,"modelTurns":0}));
                return record_stop(data, &records, "error.project.invocation");
            }
        };
        let mut files = vec![json!({"path":cli.command,"sha256":binary_hash})];
        for script in &cli.prefix {
            files.push(json!({"path":script,"sha256":fingerprint(Path::new(script))?}));
        }
        let args: Vec<_> = cli.prefix.iter().cloned().chain(["--version".to_string()]).collect();
        let p = spawn(&cli.command, &args, data)?;
        drop(p.stdin);
        drop(p.stderr);
        let mut child = p.child;
        let mut bytes = vec![];
        let status = tokio::time::timeout(Duration::from_secs(10), async {
            p.stdout.take(4097).read_to_end(&mut bytes).await?;
            child.as_mut().ok_or_else(|| io::Error::other("missing owned child"))?.wait().await
        })
        .await
        .map_err(|_| io::Error::other("version timeout"))??;
        if !status.success() || bytes.len() > 4096 {
            return Err(io::Error::other("version probe failed"));
        }
        let text = String::from_utf8(bytes).map_err(|_| io::Error::other("invalid version encoding"))?;
        let installed_version = version(&text)?;
        if vendor == Vendor::Codex {
            // The installed binary generates its own protocol; no user configuration file is inspected.
            let directory = data.join("codex-protocol");
            std::fs::create_dir(&directory)?;
            let args: Vec<_> = cli
                .prefix
                .iter()
                .cloned()
                .chain(["app-server".into(), "generate-json-schema".into(), "--out".into(), directory.to_string_lossy().into_owned()])
                .collect();
            let p = spawn(&cli.command, &args, data)?;
            drop(p.stdin);
            drop(p.stderr);
            let mut child = p.child;
            let mut output = vec![];
            let status = tokio::time::timeout(Duration::from_secs(10), async {
                p.stdout.take(4097).read_to_end(&mut output).await?;
                child.as_mut().ok_or_else(|| io::Error::other("missing owned child"))?.wait().await
            })
            .await
            .map_err(|_| io::Error::other("protocol generation timeout"))??;
            if !status.success() || output.len() > 4096 {
                return Err(io::Error::other("installed protocol generation failed"));
            }
        }
        let account = faber_engine::account::check(vendor, &cli, &spawn, data, Duration::from_secs(10)).await;
        let mut signals = if vendor == Vendor::Claude {
            faber_engine::account::claude_account(&cli, &spawn, data, Duration::from_secs(10)).await.unwrap_or_default()
        } else {
            faber_engine::signals::Signals::default()
        };
        let mut quota = vec![];
        if vendor == Vendor::Codex {
            for event in faber_engine::codex::read_codex_limits(&cli.command, data, true, spawn.clone(), Duration::from_secs(15)).await {
                match event {
                    faber_engine::events::EngineEvent::Signals(observed) => {
                        if observed.billing.is_some() {
                            signals.billing = observed.billing;
                        }
                        if observed.plan.is_some() {
                            signals.plan = observed.plan;
                        }
                        if observed.paid_fallback.is_some() {
                            signals.paid_fallback = observed.paid_fallback;
                        }
                    }
                    faber_engine::events::EngineEvent::Quota { window, used, resets_at } => {
                        quota.push(json!({"window":window,"usedPercent":used,"resetsAt":resets_at}));
                    }
                    _ => {}
                }
            }
        }
        let blockers: Vec<_> = faber_engine::project::policy::blockers(vendor).iter().map(|b| b.key()).collect();
        let billing = signals.billing.map(|b| match b {
            faber_engine::budget::Billing::Subscription => "subscription",
            faber_engine::budget::Billing::UsageBilled => "usageBilled",
            faber_engine::budget::Billing::Unknown => "unknown",
        });
        records.push(json!({"vendor":vendor,"version":installed_version,"files":files,
            "accountStatus":account,"billing":billing,"plan":signals.plan,
            "paidFallback":signals.paid_fallback,"quota":quota,"blockers":blockers,
            "restrictions":"unproven","sessionLoadout":"unobserved","invocationBoundMicros":null,
            "modelTurns":0,"qualificationPassed":false}));
    }
    record_stop(data, &records, "reason.budget.exposure")
}

#[cfg(any(windows, test))]
fn record_stop(data: &Path, records: &[serde_json::Value], reason: &str) -> io::Result<()> {
    use serde_json::json;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_secs();
    let receipt = json!({"schema":1,"measuredAt":now,"state":"stopped","stopRule":2,
        "reason":reason,"records":records,"qualificationPassed":false,"modelTurns":0,
        "note":"Preflight did not establish a version-bound qualification or invocation bound. No model prompt, canary read, or role admission attempted."});
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(data.join("qualification-preflight.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outside_marker_content_changes_without_disclosing_it_in_a_prompt() {
        let first = random_marker();
        let second = random_marker();
        assert_ne!(first, second);
        assert_eq!(first.len(), 43);
        assert!(first.starts_with("T13_MARKER_"));
    }
    #[test]
    fn version_output_is_bounded_and_single_line() {
        assert_eq!(version("2.1.296 (Claude Code)\n").unwrap(), "2.1.296 (Claude Code)");
        for text in ["", "one\ntwo", "\u{1b}secret", &"x".repeat(129)] {
            assert!(version(text).is_err());
        }
    }
    #[test]
    fn fingerprint_tracks_bytes_and_requires_an_existing_program_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.exe");
        std::fs::write(&path, b"version one").unwrap();
        let first = fingerprint(&path).unwrap();
        std::fs::write(&path, b"version two").unwrap();
        assert_ne!(fingerprint(&path).unwrap(), first);
        assert!(fingerprint(Path::new(".codex/never-opened")).is_err());
        assert!(fingerprint(Path::new(".claude/never-opened")).is_err());
        assert!(fingerprint(Path::new(".CODEX/never-opened")).is_err());
        assert!(protected(Path::new("C:/fixture/.codex/packages/bin")));
        assert!(!protected(Path::new("C:/fixture/installed/bin")));
    }
    #[test]
    fn resolved_program_bytes_are_allowed_even_under_a_vendor_root() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join(".claude");
        std::fs::create_dir(&cache).unwrap();
        let source = cache.join("cli.exe");
        std::fs::write(&source, b"disposable binary fixture, no credentials").unwrap();
        let installed = dir.path().join("cli.exe");
        std::fs::hard_link(&source, &installed).unwrap();
        assert!(fingerprint(&installed).is_ok());
        assert_eq!(fingerprint(&source).unwrap(), fingerprint(&installed).unwrap());
    }
    #[test]
    fn stopped_receipt_is_private_incomplete_and_cannot_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        record_stop(dir.path(), &[], "reason.budget.exposure").unwrap();
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("qualification-preflight.json")).unwrap()).unwrap();
        assert_eq!(receipt["state"], "stopped");
        assert_eq!(receipt["qualificationPassed"], false);
        assert_eq!(receipt["modelTurns"], 0);
        assert!(record_stop(dir.path(), &[], "replacement").is_err());
    }
}
