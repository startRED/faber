//! S2 acceptance proofs: no real network, registry, shell, Git, or CLI process.
//!
//! References checked 2026-10-10:
//! https://code.claude.com/docs/en/setup (Git is optional; ~/.local/bin on Windows)
//! https://claude.ai/install.ps1 (manifest, win32-x64/claude.exe, `install`)
//! https://api.github.com/repos/openai/codex/releases/latest (asset `digest`)
//! https://github.com/openai/codex (release archive; rename binary to codex)
//!
//! Proposed adapter: Cli::install_windows delegates to engine::install. Parameters
//! carry the simulated platform, sources, data/destination directories, transport,
//! user-PATH writer, existing Spawner and progress sink. The JSON report is either
//! Ok({command, prefix}) or Err({key, detail}); progress is {stage, key, detail}.
//! The test-only fallback returns a missing-API sentinel, asserted before use.
//! A production inherent method supersedes it; no installer is implemented here.
//! The Node verifier currently has no bridge for this Rust target. Run only this
//! target with `cargo test -p faber-engine --test install` for the RED evidence.

use faber_engine::cli::Cli;
use faber_engine::proc::{Proc, Spawner};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Fetch = dyn Fn(&str) -> io::Result<Vec<u8>> + Send + Sync;
type Register = dyn Fn(&Path) -> io::Result<()> + Send + Sync;
type Progress = dyn Fn(Value) + Send + Sync;
type Calls = Arc<Mutex<Vec<(PathBuf, Vec<String>, Vec<u8>)>>>;

#[allow(dead_code)]
trait MissingInstallerApi {
    async fn install_windows(
        request: &Value,
        fetch: &Fetch,
        register: &Register,
        spawn: Spawner,
        progress: &Progress,
    ) -> Result<Value, Value>;
}

impl MissingInstallerApi for Cli {
    async fn install_windows(
        _request: &Value,
        _fetch: &Fetch,
        _register: &Register,
        _spawn: Spawner,
        _progress: &Progress,
    ) -> Result<Value, Value> {
        Err(Value::Null)
    }
}

const BASE: &str = "https://downloads.claude.ai/claude-code-releases";
const RELEASE: &str = "https://api.github.com/repos/openai/codex/releases/latest";
const ARCHIVE: &str = "https://github.com/openai/codex/releases/download/rust-v0.162.1/codex-x86_64-pc-windows-msvc.exe.zip";
// Independent, standard SHA-256 vector for the three bytes `abc`.
const BINARY_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
// Hand-built ZIP: one stored member codex-x86_64-pc-windows-msvc.exe, bytes `abc`.
// Its digest was checked independently with Node crypto, not the installer.
const ZIP_HEX: &str = "504b030414000000000000000000c2412435030000000300000020000000636f6465782d7838365f36342d70632d77696e646f77732d6d7376632e657865616263504b0102140014000000000000000000c24124350300000003000000200000000000000000000000000000000000636f6465782d7838365f36342d70632d77696e646f77732d6d7376632e657865504b050600000000010001004e000000410000000000";
const ZIP_SHA: &str = "9822b5254c9890463aefb0f59f99087ba31d9c64d3668fa9899f52768258b8ce";
const HOSTS: &[&str] = &["downloads.claude.ai", "api.github.com", "github.com", "release-assets.githubusercontent.com"];

fn zip_bytes() -> Vec<u8> {
    ZIP_HEX.as_bytes().as_chunks::<2>().0.iter().map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

struct Rig {
    root: tempfile::TempDir,
    request: Value,
    responses: BTreeMap<String, Vec<u8>>,
    requests: Arc<Mutex<Vec<String>>>,
    calls: Calls,
    paths: Arc<Mutex<Vec<PathBuf>>>,
    events: Arc<Mutex<Vec<Value>>>,
    fail_spawn_once: Arc<Mutex<bool>>,
}

impl Rig {
    fn new(vendor: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let destination = root.path().join("user profile/.local/bin");
        std::fs::create_dir_all(data.join("install-tmp")).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let request = json!({
            "vendor": vendor, "windows": true, "arch": "x86_64",
            "data_dir": data, "destination": destination,
            "user_profile": root.path().join("user profile"), "git_available": false,
            "sources": {"claude_base": BASE, "codex_release": RELEASE,
                "git_release": "https://api.github.com/repos/git-for-windows/git/releases/latest",
                "allowed_hosts": HOSTS}
        });
        let mut responses = BTreeMap::new();
        responses.insert(format!("{BASE}/latest"), b"2.1.89\n".to_vec());
        responses.insert(
            format!("{BASE}/2.1.89/manifest.json"),
            serde_json::to_vec(&json!({
                "version": "2.1.89", "platforms": {
                    "win32-x64": {"checksum": BINARY_SHA, "size": 3},
                    "win32-arm64": {"checksum": "0".repeat(64), "size": 3}
                }
            }))
            .unwrap(),
        );
        responses.insert(format!("{BASE}/2.1.89/win32-x64/claude.exe"), b"abc".to_vec());
        responses.insert(RELEASE.into(), serde_json::to_vec(&json!({
            "tag_name": "rust-v0.162.1", "draft": false, "prerelease": false,
            "assets": [
                {"name": "codex-aarch64-pc-windows-msvc.exe.zip", "size": 3,
                    "browser_download_url": "https://github.com/openai/codex/releases/download/rust-v0.162.1/codex-aarch64-pc-windows-msvc.exe.zip",
                    "digest": format!("sha256:{}", "0".repeat(64))},
                {"name": "codex-x86_64-pc-windows-msvc.exe.zip", "size": zip_bytes().len(),
                    "browser_download_url": ARCHIVE, "digest": format!("sha256:{ZIP_SHA}")}
            ]
        })).unwrap());
        responses.insert(ARCHIVE.into(), zip_bytes());
        Self {
            root,
            request,
            responses,
            requests: Arc::default(),
            calls: Arc::default(),
            paths: Arc::default(),
            events: Arc::default(),
            fail_spawn_once: Arc::default(),
        }
    }

    fn destination(&self) -> PathBuf {
        PathBuf::from(self.request["destination"].as_str().unwrap())
    }

    fn temporary(&self) -> PathBuf {
        PathBuf::from(self.request["data_dir"].as_str().unwrap()).join("install-tmp")
    }

    async fn run(&self) -> Result<Value, Value> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        let fetch = move |url: &str| {
            requests.lock().unwrap().push(url.to_string());
            responses.get(url).cloned().ok_or_else(|| io::Error::other(format!("Unscripted network request: {url}")))
        };
        let paths = self.paths.clone();
        let register = move |dir: &Path| {
            paths.lock().unwrap().push(dir.into());
            Ok(())
        };
        let calls = self.calls.clone();
        let fail = self.fail_spawn_once.clone();
        let data = PathBuf::from(self.request["data_dir"].as_str().unwrap());
        let request_profile = self.request["user_profile"].clone();
        let spawn: Spawner = Arc::new(move |program, args, _cwd| {
            let program = PathBuf::from(program);
            // Read the actual staged bytes: wrong hash/order/path cannot satisfy this fake.
            let bytes = std::fs::read(&program)?;
            calls.lock().unwrap().push((program.clone(), args.to_vec(), bytes.clone()));
            if !program.starts_with(&data) || args != ["install"] || bytes != b"abc" {
                return Err(io::Error::other("Only the verified native installer may run, directly with install"));
            }
            let mut should_fail = fail.lock().unwrap();
            if *should_fail {
                *should_fail = false;
                return Err(io::Error::other("Injected interruption before installation"));
            }
            // External executable owns this side effect; writes to profile .local/bin as documented.
            let profile_bin = PathBuf::from(request_profile.as_str().unwrap()).join(".local").join("bin");
            std::fs::create_dir_all(&profile_bin)?;
            std::fs::write(profile_bin.join("claude.exe"), &bytes)?;
            Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(tokio::io::empty()), stderr: None, child: None })
        });
        let events = self.events.clone();
        let progress = move |event| events.lock().unwrap().push(event);
        let result = Cli::install_windows(&self.request, &fetch, &register, spawn, &progress).await;
        assert!(
            !matches!(&result, Err(error) if error.is_null()),
            "S2 installer API is missing: implement engine/src/install.rs and connect the Cli::install_windows test boundary"
        );
        result
    }

    fn assert_empty_temporary(&self) {
        fn files(path: &Path) -> usize {
            if !path.exists() {
                return 0;
            }
            std::fs::read_dir(path)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() { files(&path) } else { 1 }
                })
                .sum()
        }
        assert_eq!(files(&self.temporary()), 0, "Download staging must contain no files after completion or failure");
    }

    fn assert_stages(&self, expected: &[&str]) {
        let events = self.events.lock().unwrap();
        let stages: Vec<_> = events.iter().map(|e| e["stage"].as_str().expect("Progress must expose stage separately")).collect();
        assert_eq!(stages, expected);
        for event in events.iter() {
            assert!(
                event["key"].is_null()
                    || matches!(
                        event["key"].as_str(),
                        Some("error.setup.download" | "error.setup.checksum" | "error.setup.install" | "error.setup.unsupported")
                    )
            );
            assert!(event["detail"].is_null() || event["detail"].is_string());
        }
    }

    fn assert_detected(&self, name: &str, report: &Value) {
        let install_path = PathBuf::from(report["command"].as_str().unwrap());
        let path_var = install_path.parent().unwrap().to_string_lossy().into_owned();
        let found = Cli::find_in(name, &path_var, true, &|_| None, &|p| Path::new(p).is_file(), &|_| String::new());
        let found = found.expect("Existing CLI detection must find the installed native executable");
        assert_eq!(found.command, report["command"].as_str().unwrap());
        assert!(found.prefix.is_empty());
    }
}

#[tokio::test]
async fn c2_1_verified_native_binary_runs_install_without_shell_and_cleans_staging() {
    let rig = Rig::new("claude");
    let installed = rig.run().await.expect("Verified Claude installation must succeed");
    let calls = rig.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, ["install"]);
    assert_eq!(calls[0].2, b"abc");
    assert!(calls[0].0.starts_with(rig.temporary()));
    assert_eq!(std::fs::read(rig.destination().join("claude.exe")).unwrap(), b"abc");
    rig.assert_detected("claude", &installed);
    rig.assert_empty_temporary();
    rig.assert_stages(&["prerequisite", "download", "verify", "install", "done"]);
}

#[tokio::test]
async fn c2_2_checksum_mismatch_never_executes_or_promotes_and_reports_key_separately() {
    for vendor in ["claude", "codex"] {
        let mut rig = Rig::new(vendor);
        let url = if vendor == "claude" { format!("{BASE}/2.1.89/win32-x64/claude.exe") } else { ARCHIVE.into() };
        rig.responses.insert(url, b"tampered payload".to_vec());
        let error = rig.run().await.expect_err("Corrupt payload must fail before install");
        assert_eq!(error["key"], "error.setup.checksum");
        assert!(!error["detail"].as_str().unwrap().is_empty());
        assert_ne!(error["detail"], error["key"]);
        assert!(rig.calls.lock().unwrap().is_empty());
        assert!(rig.paths.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 0);
        rig.assert_empty_temporary();
        rig.assert_stages(&["prerequisite", "download", "verify", "failed"]);
        let events = rig.events.lock().unwrap();
        assert_eq!(events.last().unwrap()["key"], "error.setup.checksum");
        assert_eq!(events.last().unwrap()["detail"], error["detail"]);
    }
}

#[tokio::test]
async fn c2_3_release_archive_is_extracted_into_requested_destination_detected_and_registered() {
    let rig = Rig::new("codex");
    let installed = rig.run().await.expect("Verified Codex archive must install");
    assert_eq!(std::fs::read(rig.destination().join("codex.exe")).unwrap(), b"abc");
    assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 1);
    rig.assert_detected("codex", &installed);
    assert_eq!(*rig.paths.lock().unwrap(), [rig.destination()]);
    assert!(rig.calls.lock().unwrap().is_empty(), "Archive extraction does not execute the downloaded program");
    assert!(rig.requests.lock().unwrap().contains(&ARCHIVE.to_string()));
    rig.assert_empty_temporary();
    rig.assert_stages(&["prerequisite", "download", "verify", "install", "done"]);
}

#[tokio::test]
async fn c2_4_network_stays_on_fixed_official_https_hosts_and_rejects_untrusted_sources() {
    for vendor in ["claude", "codex"] {
        let rig = Rig::new(vendor);
        rig.run().await.expect("Official source must install");
        {
            let requests = rig.requests.lock().unwrap();
            assert!(!requests.is_empty(), "An empty request log does not prove a download policy");
            for url in requests.iter() {
                let authority = url.strip_prefix("https://").expect("Every request must use HTTPS").split('/').next().unwrap();
                assert!(HOSTS.contains(&authority), "Unapproved network authority: {url}");
            }
        }
        for untrusted in [
            "http://downloads.claude.ai/claude-code-releases",
            "https://downloads.claude.ai.attacker.example/releases",
            "https://downloads.claude.ai@attacker.example/releases",
        ] {
            let mut rig = Rig::new("claude");
            rig.request["sources"]["claude_base"] = json!(untrusted);
            rig.run().await.expect_err("Source must be rejected before contacting the injected network");
            assert!(rig.requests.lock().unwrap().is_empty());
            assert!(rig.calls.lock().unwrap().is_empty());
        }
    }
    let mut rig = Rig::new("codex");
    let mut release: Value = serde_json::from_slice(&rig.responses[RELEASE]).unwrap();
    release["assets"][1]["browser_download_url"] = json!("https://attacker.example/payload.zip");
    rig.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    rig.run().await.expect_err("Release metadata cannot authorize another host");
    assert_eq!(*rig.requests.lock().unwrap(), [RELEASE]);
    assert!(rig.paths.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 0);
}

#[tokio::test]
async fn c2_5_interrupted_install_with_partial_download_retries_to_clean_install_result() {
    let clean = Rig::new("claude");
    clean.run().await.expect("Clean install");
    let retry = Rig::new("claude");
    *retry.fail_spawn_once.lock().unwrap() = true;
    let failure = retry.run().await.expect_err("Injected installer interruption must be propagated");
    assert_eq!(failure["key"], "error.setup.install");
    assert_eq!(retry.events.lock().unwrap().last().unwrap()["stage"], "failed");
    assert_eq!(std::fs::read_dir(retry.destination()).unwrap().count(), 0);
    std::fs::create_dir_all(retry.temporary()).unwrap();
    std::fs::write(retry.temporary().join("claude-2.1.89-win32-x64.exe.part"), b"interrupted download").unwrap();
    let installed = retry.run().await.expect("Retry must replace stale partial data");
    assert_eq!(
        std::fs::read(retry.destination().join("claude.exe")).unwrap(),
        std::fs::read(clean.destination().join("claude.exe")).unwrap()
    );
    retry.assert_detected("claude", &installed);
    retry.assert_empty_temporary();
    assert_eq!(retry.events.lock().unwrap().last().unwrap()["stage"], "done");
    let again = retry.run().await.expect("Repeating success must stay successful");
    assert_eq!(again, installed);
    assert_eq!(std::fs::read_dir(retry.destination()).unwrap().count(), 1);
    assert_eq!(std::fs::read(retry.destination().join("claude.exe")).unwrap(), b"abc");
    retry.assert_empty_temporary();
    assert!(retry.paths.lock().unwrap().iter().all(|path| path == &retry.destination()));
}

#[tokio::test]
async fn c2_6_computer_without_git_finishes_without_manual_step_or_optional_git_download() {
    // Current official setup explicitly makes Git optional; do not install it.
    let rig = Rig::new("claude");
    assert_eq!(rig.request["git_available"], false);
    let installed = rig.run().await.expect("Native installer must finish without Git or manual intervention");
    rig.assert_detected("claude", &installed);
    assert_eq!(
        *rig.requests.lock().unwrap(),
        [format!("{BASE}/latest"), format!("{BASE}/2.1.89/manifest.json"), format!("{BASE}/2.1.89/win32-x64/claude.exe")]
    );
    assert_eq!(rig.calls.lock().unwrap().len(), 1);
    assert_eq!(*rig.paths.lock().unwrap(), [rig.destination()]);
    assert_eq!(rig.events.lock().unwrap().last().unwrap()["stage"], "done");
    rig.assert_empty_temporary();
    assert!(rig.root.path().join("user profile").is_dir());
}

#[test]
#[allow(non_snake_case)]
fn F01_url_parser_bypass() {
    let url = "https://attacker.example\\@github.com/payload.zip";
    let actual = reqwest::Url::parse(url).unwrap();
    assert_eq!(actual.host_str(), Some("attacker.example"));
    assert!(
        faber_engine::install::validate_url(url, &[]).is_err(),
        "nonofficial host accepted: {url}; reqwest sends to {}",
        actual.host_str().unwrap()
    );
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F02_tls_transport() {
    let res = reqwest::Client::builder().no_proxy().build().unwrap().get("https://github.com").send().await;
    if let Err(error) = res {
        let debug = format!("{error:?}");
        assert!(!debug.contains("scheme is not http"), "Official HTTPS downloads are impossible: {debug}");
    }
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F03_deflated_archive() {
    let mut rig = Rig::new("codex");
    let archive = if let Ok(bytes) = std::fs::read(".ade-review/deflated.zip") {
        bytes
    } else {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("codex-x86_64-pc-windows-msvc.exe", options).unwrap();
            std::io::Write::write_all(&mut writer, b"abc").unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    };
    use sha2::{Digest, Sha256};
    let digest = format!("sha256:{:x}", Sha256::digest(&archive));
    let mut release: Value = serde_json::from_slice(&rig.responses[RELEASE]).unwrap();
    release["assets"][1]["digest"] = json!(digest);
    rig.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    rig.responses.insert(ARCHIVE.into(), archive);
    let result = rig.run().await;
    assert!(result.is_ok(), "A checksum-verified standard DEFLATE ZIP must install: {result:?}");
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F04_native_destination() {
    let mut rig = Rig::new("claude");
    let actual_profile = rig.root.path().join("actual-profile");
    let profile_bin = actual_profile.join(".local/bin");
    std::fs::create_dir_all(&profile_bin).unwrap();
    let installed = profile_bin.join("claude.exe");
    let native_installer: Spawner = Arc::new(move |program, args, _| {
        assert_eq!(args, ["install"]);
        std::fs::write(&installed, std::fs::read(program)?)?;
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(tokio::io::empty()), stderr: None, child: None })
    });
    rig.request["user_profile"] = json!(actual_profile);
    let fetch = |url: &str| rig.responses.get(url).cloned().ok_or_else(|| io::Error::other(url.to_owned()));
    let report = Cli::install_windows(&rig.request, &fetch, &|_| Ok(()), native_installer, &|_| {}).await.unwrap();
    assert!(
        Path::new(report["command"].as_str().unwrap()).is_file(),
        "Installer reported done for a nonexistent executable: {report}; native install uses user-profile .local/bin, not request destination"
    );
}

#[test]
#[allow(non_snake_case)]
fn F05_fixed_hosts() {
    assert!(
        faber_engine::install::validate_url("https://attacker.example/payload", &["attacker.example".into()]).is_err(),
        "The injectable list must not authorize hosts outside the fixed official allowlist"
    );
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F06_release_asset_selection() {
    let mut rig = Rig::new("codex");
    let mut release: Value = serde_json::from_slice(&rig.responses[RELEASE]).unwrap();
    let assets = release["assets"].as_array_mut().unwrap();
    let app_server_url = "https://github.com/openai/codex/releases/download/rust-v0.162.1/codex-app-server-x86_64-pc-windows-msvc.exe.zip";
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("codex-app-server-x86_64-pc-windows-msvc.exe", options).unwrap();
        std::io::Write::write_all(&mut writer, b"app-server").unwrap();
        writer.finish().unwrap();
    }
    let app_server_zip = buffer.into_inner();
    use sha2::{Digest, Sha256};
    let app_server_digest = format!("sha256:{:x}", Sha256::digest(&app_server_zip));
    rig.responses.insert(app_server_url.into(), app_server_zip);

    assets.insert(
        0,
        json!({
            "name": "codex-app-server-x86_64-pc-windows-msvc.exe.zip",
            "browser_download_url": app_server_url,
            "digest": app_server_digest
        }),
    );
    assets.insert(1, json!({
        "name": "codex-command-runner-x86_64-pc-windows-msvc.exe.zip",
        "browser_download_url": "https://github.com/openai/codex/releases/download/rust-v0.162.1/codex-command-runner-x86_64-pc-windows-msvc.exe.zip",
        "digest": format!("sha256:{}", "0".repeat(64))
    }));
    rig.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    let result = rig.run().await.unwrap();
    assert!(!rig.requests.lock().unwrap().iter().any(|u| u.contains("app-server")), "Must not download app-server helper");
    assert!(!rig.requests.lock().unwrap().iter().any(|u| u.contains("command-runner")), "Must not download command-runner helper");
    assert_eq!(result["command"].as_str().unwrap(), rig.destination().join("codex.exe").to_string_lossy());
    assert_eq!(std::fs::read(rig.destination().join("codex.exe")).unwrap(), b"abc");
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F07_package_resources() {
    let mut rig = Rig::new("codex");
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("codex.exe", options).unwrap();
        std::io::Write::write_all(&mut writer, b"abc").unwrap();
        writer.start_file("resources/config.json", options).unwrap();
        std::io::Write::write_all(&mut writer, b"{\"version\": 1}").unwrap();
        writer.finish().unwrap();
    }
    let archive = buffer.into_inner();
    use sha2::{Digest, Sha256};
    let digest = format!("sha256:{:x}", Sha256::digest(&archive));
    let mut release: Value = serde_json::from_slice(&rig.responses[RELEASE]).unwrap();
    release["assets"][1]["digest"] = json!(digest);
    rig.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    rig.responses.insert(ARCHIVE.into(), archive);
    let result = rig.run().await.unwrap();
    assert_eq!(result["command"].as_str().unwrap(), rig.destination().join("codex.exe").to_string_lossy());
    assert!(
        rig.destination().join("resources/config.json").is_file(),
        "Reported done while dropping the packaged runtime resources required by the official Windows bundle"
    );
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F08_stale_partial_codex() {
    let rig = Rig::new("codex");
    std::fs::write(rig.destination().join("codex.exe"), b"interrupted partial executable").unwrap();
    let installed = rig.run().await.expect("Retry must succeed and promote verified binary over partial file");
    assert_eq!(
        std::fs::read(rig.destination().join("codex.exe")).unwrap(),
        b"abc",
        "Retry must replace stale partial codex.exe with verified binary"
    );
    assert_eq!(installed["command"].as_str().unwrap(), rig.destination().join("codex.exe").to_string_lossy());
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F09_bin_path_detection() {
    let mut rig = Rig::new("codex");
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("bin/codex.exe", options).unwrap();
        std::io::Write::write_all(&mut writer, b"abc").unwrap();
        writer.finish().unwrap();
    }
    let archive = buffer.into_inner();
    use sha2::{Digest, Sha256};
    let digest = format!("sha256:{:x}", Sha256::digest(&archive));
    let mut release: Value = serde_json::from_slice(&rig.responses[RELEASE]).unwrap();
    release["assets"][1]["digest"] = json!(digest);
    rig.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    rig.responses.insert(ARCHIVE.into(), archive);
    let result = rig.run().await.unwrap();
    let exe_path = rig.destination().join("bin").join("codex.exe");
    assert_eq!(result["command"].as_str().unwrap(), exe_path.to_string_lossy());
    assert!(rig.paths.lock().unwrap().contains(&rig.destination().join("bin")), "Must register destination/bin in PATH");
    rig.assert_detected("codex", &result);
}

#[tokio::test]
#[allow(non_snake_case)]
async fn F10_other_profile() {
    let rig = Rig::new("claude");
    let other_profile = rig.root.path().join("another-user/.local/bin");
    std::fs::create_dir_all(&other_profile).unwrap();
    let other_exe = other_profile.join("claude.exe");
    let native_installer: Spawner = Arc::new(move |program, args, _| {
        assert_eq!(args, ["install"]);
        std::fs::write(&other_exe, std::fs::read(program)?)?;
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(tokio::io::empty()), stderr: None, child: None })
    });
    let fetch = |url: &str| rig.responses.get(url).cloned().ok_or_else(|| io::Error::other(url.to_owned()));
    let error = Cli::install_windows(&rig.request, &fetch, &|_| Ok(()), native_installer, &|_| {})
        .await
        .expect_err("Installer must fail when executable is placed in another user's profile");
    assert_eq!(error["key"], "error.setup.install");
}
