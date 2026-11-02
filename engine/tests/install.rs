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

use crate::loopback;

use faber_engine::cli::Cli;
use faber_engine::proc::{Proc, Spawner};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Fetch<'a> = dyn Fn(&str) -> io::Result<Vec<u8>> + Send + Sync + 'a;
type Download<'a> = dyn Fn(&str, &Path) -> io::Result<String> + Send + Sync + 'a;
type Register = dyn Fn(&Path) -> io::Result<()> + Send + Sync;
type Progress = dyn Fn(Value) + Send + Sync;
type Calls = Arc<Mutex<Vec<(PathBuf, Vec<String>, Vec<u8>)>>>;

#[allow(dead_code)]
trait MissingInstallerApi {
    async fn install_windows(
        request: &Value,
        fetch: &Fetch<'_>,
        download: &Download<'_>,
        register: &Register,
        spawn: Spawner,
        progress: &Progress,
    ) -> Result<Value, Value>;
}

impl MissingInstallerApi for Cli {
    async fn install_windows(
        _request: &Value,
        _fetch: &Fetch<'_>,
        _download: &Download<'_>,
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

/// What the release host serves as a payload (written to disk); every other document is metadata (held in memory).
fn is_payload(url: &str) -> bool {
    [".exe", ".zip", ".tar.gz"].iter().any(|ext| url.ends_with(ext))
}

/// Stands in for `default_fetch`. A payload asked for here would mean the installer holds it in memory.
fn scripted_fetch(responses: &BTreeMap<String, Vec<u8>>) -> impl Fn(&str) -> io::Result<Vec<u8>> + Send + Sync + '_ {
    move |url| {
        if is_payload(url) {
            return Err(io::Error::other(format!("A payload must be downloaded to disk, not fetched: {url}")));
        }
        responses.get(url).cloned().ok_or_else(|| io::Error::other(format!("Unscripted network request: {url}")))
    }
}

/// Stands in for `default_download`: writes the scripted body to the file and returns its hash.
fn scripted_download(responses: &BTreeMap<String, Vec<u8>>) -> impl Fn(&str, &Path) -> io::Result<String> + Send + Sync + '_ {
    move |url, path| {
        use sha2::{Digest, Sha256};
        if !is_payload(url) {
            return Err(io::Error::other(format!("A metadata document must be fetched, not downloaded: {url}")));
        }
        let body = responses.get(url).ok_or_else(|| io::Error::other(format!("Unscripted network request: {url}")))?;
        std::fs::write(path, body)?;
        Ok(faber_engine::hex(&Sha256::digest(body)))
    }
}

/// Runs one network job on its own thread and runtime, like the app does, because the installer's callbacks are synchronous.
fn on_thread<T: Send + 'static>(job: impl std::future::Future<Output = io::Result<T>> + Send + 'static) -> io::Result<T> {
    std::thread::spawn(move || tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(job))
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("network thread panicked")))
}

/// Every file under `root`, relative and with `/`, sorted.
fn tree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, found);
            } else {
                found.push(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
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
    /// The files in `install-tmp` at the moment a failure was reported, before the final cleanup could hide them.
    left_at_failure: Arc<Mutex<Option<Vec<String>>>>,
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
            left_at_failure: Arc::default(),
        }
    }

    fn destination(&self) -> PathBuf {
        PathBuf::from(self.request["destination"].as_str().unwrap())
    }

    fn temporary(&self) -> PathBuf {
        PathBuf::from(self.request["data_dir"].as_str().unwrap()).join("install-tmp").join(self.request["vendor"].as_str().unwrap())
    }

    async fn run(&self) -> Result<Value, Value> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        let scripted = scripted_download(&responses);
        let download = move |url: &str, path: &Path| {
            requests.lock().unwrap().push(url.to_string());
            scripted(url, path)
        };
        self.run_with(&download).await
    }

    /// Runs the install with the given payload download; the metadata comes from the scripted responses.
    async fn run_with(&self, download: &Download<'_>) -> Result<Value, Value> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        let scripted = scripted_fetch(&responses);
        let fetch = move |url: &str| {
            requests.lock().unwrap().push(url.to_string());
            scripted(url)
        };
        self.run_all(&fetch, download).await
    }

    /// Runs the install against the loopback server named in `sources.loopback`, with the real fetch and the real download.
    async fn run_network(&self) -> Result<Value, Value> {
        let origin = self.request["sources"]["loopback"].as_str().map(String::from);
        let fetch_origin = origin.clone();
        let fetch = move |url: &str| {
            let (url, origin) = (url.to_string(), fetch_origin.clone());
            on_thread(async move { faber_engine::install::default_fetch(&url, origin.as_deref()).await })
        };
        let download = move |url: &str, dest: &Path| {
            let (url, dest, origin) = (url.to_string(), dest.to_path_buf(), origin.clone());
            on_thread(async move { faber_engine::install::default_download(&url, &dest, origin.as_deref()).await })
        };
        self.run_all(&fetch, &download).await
    }

    async fn run_all(&self, fetch: &Fetch<'_>, download: &Download<'_>) -> Result<Value, Value> {
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
            // Windows refuses to start a file that is still open for writing; this stand-in refuses the same way.
            assert_exclusive(&program)?;
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
            Ok(Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(tokio::io::empty()),
                stderr: None,
                child: None,
                contained: None,
            })
        });
        let events = self.events.clone();
        let (left, temporary) = (self.left_at_failure.clone(), self.temporary());
        let progress = move |event: Value| {
            if event["stage"] == "failed" {
                *left.lock().unwrap() = Some(tree(&temporary));
            }
            events.lock().unwrap().push(event);
        };
        let result = Cli::install_windows(&self.request, fetch, download, &register, spawn, &progress).await;
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
                        Some(
                            "error.setup.download"
                                | "reason.setup.checksum"
                                | "reason.setup.connection"
                                | "reason.setup.blocked"
                                | "reason.setup.write"
                                | "reason.setup.timeout"
                                | "error.setup.install"
                                | "error.setup.unsupported"
                        )
                    )
            );
            assert!(event["detail"].is_null() || event["detail"].is_string());
        }
    }

    fn assert_detected(&self, name: &str, report: &Value) {
        let install_path = PathBuf::from(report["command"].as_str().unwrap());
        let path_var = install_path.parent().unwrap().to_string_lossy().into_owned();
        // Windows detection joins with `\`; map it to this platform's separator so the check runs on every OS.
        let native = |p: &str| p.replace('\\', std::path::MAIN_SEPARATOR_STR);
        let found = Cli::find_in(name, &path_var, true, &|_| None, &|p| Path::new(&native(p)).is_file(), &|_| String::new());
        let found = found.expect("Existing CLI detection must find the installed native executable");
        assert_eq!(native(&found.command), report["command"].as_str().unwrap());
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
        assert_eq!(error["key"], "reason.setup.checksum");
        assert!(!error["detail"].as_str().unwrap().is_empty());
        assert_ne!(error["detail"], error["key"]);
        assert!(rig.calls.lock().unwrap().is_empty());
        assert!(rig.paths.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 0);
        rig.assert_empty_temporary();
        assert_eq!(
            *rig.left_at_failure.lock().unwrap(),
            Some(vec![]),
            "The mismatching download is deleted before the failure is reported"
        );
        rig.assert_stages(&["prerequisite", "download", "verify", "failed"]);
        let events = rig.events.lock().unwrap();
        assert_eq!(events.last().unwrap()["key"], "reason.setup.checksum");
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
    std::fs::create_dir_all(retry.temporary().join("download")).unwrap();
    std::fs::write(retry.temporary().join("download/claude-2.1.89-win32-x64.exe.part"), b"interrupted download").unwrap();
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
    // Nothing listens on this loopback port, so the request fails to connect; a client built without TLS says why instead.
    faber_engine::tls::install();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let res = reqwest::Client::builder().no_proxy().build().unwrap().get(format!("https://127.0.0.1:{port}")).send().await;
    let error = res.expect_err("Nothing answers on that port");
    let debug = format!("{error:?}");
    assert!(!debug.contains("scheme is not http"), "Official HTTPS downloads are impossible: {debug}");
}

#[test]
fn tls_builds_without_aws_lc() {
    // The updater compiles ring anyway; aws-lc-sys would add minutes of C to every cold build (src/tls.rs).
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock")).unwrap();
    assert!(!lock.contains("name = \"aws-lc-sys\""), "A dependency turned on aws-lc-rs again; use the ring provider");
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
    let digest = format!("sha256:{}", faber_engine::hex(&Sha256::digest(&archive)));
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
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(tokio::io::empty()), stderr: None, child: None, contained: None })
    });
    rig.request["user_profile"] = json!(actual_profile);
    let (fetch, download) = (scripted_fetch(&rig.responses), scripted_download(&rig.responses));
    let report = Cli::install_windows(&rig.request, &fetch, &download, &|_| Ok(()), native_installer, &|_| {}).await.unwrap();
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
    let app_server_digest = format!("sha256:{}", faber_engine::hex(&Sha256::digest(&app_server_zip)));
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
    let digest = format!("sha256:{}", faber_engine::hex(&Sha256::digest(&archive)));
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
    let digest = format!("sha256:{}", faber_engine::hex(&Sha256::digest(&archive)));
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
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(tokio::io::empty()), stderr: None, child: None, contained: None })
    });
    let (fetch, download) = (scripted_fetch(&rig.responses), scripted_download(&rig.responses));
    let error = Cli::install_windows(&rig.request, &fetch, &download, &|_| Ok(()), native_installer, &|_| {})
        .await
        .expect_err("Installer must fail when executable is placed in another user's profile");
    assert_eq!(error["key"], "error.setup.install");
}

#[test]
fn the_user_path_gains_the_folder_once_and_keeps_every_other_entry() {
    use faber_engine::install::path_with;
    let current = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools;";
    assert_eq!(
        path_with(current, r"C:\Users\a\.local\bin").as_deref(),
        Some(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools;C:\Users\a\.local\bin")
    );
    assert_eq!(path_with(current, r"c:\tools\"), None);
    assert_eq!(path_with("", r"C:\x").as_deref(), Some(r"C:\x"));
}

#[test]
fn a_version_that_could_leave_the_staging_folder_is_refused() {
    use faber_engine::install::valid_version;
    assert!(valid_version("2.1.80"));
    assert!(valid_version("2.1.80-beta+1"));
    for bad in ["", "..", r"..\..\Startup\x", "C:x", "2.1/../x", "a b"] {
        assert!(!valid_version(bad), "{bad:?}");
    }
}

#[test]
fn a_loopback_source_replaces_every_official_host_and_nothing_else() {
    use faber_engine::install::{allowed_url, loopback_origin};
    assert_eq!(loopback_origin("http://127.0.0.1:4100/").as_deref(), Some("http://127.0.0.1:4100"));
    assert_eq!(loopback_origin("http://[::1]:4100").as_deref(), Some("http://[::1]:4100"));
    for bad in ["http://localhost:4100", "http://192.168.1.5:4100", "https://127.0.0.1:4100", "http://127.0.0.1", "github.com"] {
        assert_eq!(loopback_origin(bad), None, "{bad}");
    }
    let origin = Some("http://127.0.0.1:4100");
    assert!(allowed_url("http://127.0.0.1:4100/releases/latest", &[], origin).is_ok());
    assert!(allowed_url("http://127.0.0.1:4101/codex.zip", &[], origin).is_err());
    assert!(allowed_url("https://github.com/openai/codex/releases/download/x.zip", &[], origin).is_err());
    assert!(allowed_url("http://127.0.0.1:4100/x", &[], None).is_err());
}

#[tokio::test]
async fn an_install_whose_source_is_not_loopback_stops_before_any_download() {
    let request = json!({
        "vendor": "codex", "windows": true, "arch": "x86_64",
        "data_dir": std::env::temp_dir().join("faber-loopback-refused"),
        "destination": std::env::temp_dir().join("faber-loopback-refused/bin"),
        "sources": { "loopback": "http://192.168.1.5:4100", "codex_release": "http://192.168.1.5:4100/releases/latest" },
    });
    let fetched = std::sync::Mutex::new(Vec::<String>::new());
    let fetch = |url: &str| {
        fetched.lock().unwrap().push(url.to_string());
        Err(std::io::Error::other("no network in tests"))
    };
    let download = |url: &str, _: &Path| {
        fetched.lock().unwrap().push(url.to_string());
        Err(std::io::Error::other("no network in tests"))
    };
    let spawn: Spawner = std::sync::Arc::new(|_, _, _| Err(std::io::Error::other("nothing runs")));
    let err = Cli::install_windows(&request, &fetch, &download, &|_| Ok(()), spawn, &|_| {}).await.unwrap_err();
    assert_eq!(err["key"], "error.setup.download");
    assert!(fetched.lock().unwrap().is_empty());
}

fn sha_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    faber_engine::hex(&Sha256::digest(bytes))
}

/// Opens the file for writing and lets nobody else share it, which fails while any other handle to it is open.
#[cfg(windows)]
fn assert_exclusive(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).share_mode(0).open(path).map(drop)
}

#[cfg(not(windows))]
fn assert_exclusive(_path: &Path) -> io::Result<()> {
    Ok(())
}

async fn loopback_download(url: String, dest: PathBuf, origin: String) -> io::Result<String> {
    faber_engine::install::default_download(&url, &dest, Some(&origin)).await
}

#[tokio::test]
async fn a_download_writes_the_file_returns_its_hash_and_lets_go_of_it() {
    let server = loopback::Server::start(|_, _, stream| loopback::send(stream, b"hello"));
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join("hello.part");
    let hash = loopback_download(format!("{}/hello", server.origin), part.clone(), server.origin.clone()).await.unwrap();
    assert_eq!(hash, sha_hex(b"hello"));
    assert_eq!(std::fs::read(&part).unwrap(), b"hello");
    // The program that starts a staged file needs it to itself; Windows refuses that while any handle to it is open.
    assert_exclusive(&part).expect("The download must have closed its handle");
    let renamed = dir.path().join("hello.exe");
    std::fs::rename(&part, &renamed).unwrap();
    std::fs::remove_file(&renamed).unwrap();
}

#[tokio::test]
async fn a_connection_cut_halfway_leaves_no_part_file() {
    let server = loopback::Server::start(|_, _, stream| {
        loopback::head(stream, Some(1024 * 1024))?;
        io::Write::write_all(stream, &[7u8; 100 * 1024])
    });
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join("cut.part");
    let error = loopback_download(format!("{}/cut", server.origin), part.clone(), server.origin.clone()).await.unwrap_err();
    assert!(!faber_engine::install::is_write_failure(&error), "a broken connection is not a write failure: {error}");
    assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    assert!(!part.exists());
}

/// A file that accepts `left` bytes and then fails, like a disk that fills up.
struct FullDisk {
    file: std::fs::File,
    left: usize,
}

impl io::Write for FullDisk {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("the disk is full"));
        }
        let wrote = self.file.write(&buf[..buf.len().min(self.left)])?;
        self.left -= wrote;
        Ok(wrote)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[tokio::test]
async fn a_write_that_fails_after_one_mebibyte_leaves_no_part_file() {
    let server = loopback::Server::start(|_, _, stream| loopback::send(stream, &vec![1u8; 4 * 1024 * 1024]));
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join("full.part");
    let url = format!("{}/full", server.origin);
    let create = |path: &Path| std::fs::File::create(path).map(|file| FullDisk { file, left: 1024 * 1024 });
    let error = faber_engine::install::download_with(&url, &part, Some(&server.origin), faber_engine::install::IDLE_TIMEOUT, create)
        .await
        .unwrap_err();
    assert!(faber_engine::install::is_write_failure(&error), "{error}");
    assert!(!part.exists());
}

#[tokio::test]
async fn the_part_file_grows_while_the_body_is_still_arriving() {
    // A download that ends early leaves the server waiting for a signal that never comes; the limit turns that hang into a failure.
    let limit = std::time::Duration::from_secs(90);
    tokio::time::timeout(limit, part_file_grows_with_the_body()).await.expect("The progress test did not finish in time");
}

async fn part_file_grows_with_the_body() {
    const STEP: usize = 8 * 1024 * 1024;
    const STEPS: usize = 8;
    let block = |step: usize| vec![step as u8; STEP];
    let (sent_tx, mut sent_rx) = tokio::sync::mpsc::unbounded_channel::<usize>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let go_rx = Mutex::new(go_rx);
    let server = loopback::Server::start(move |_, _, stream| {
        loopback::head(stream, Some((STEP * STEPS) as u64))?;
        for step in 1..=STEPS {
            io::Write::write_all(stream, &block(step))?;
            let _ = sent_tx.send(step);
            if step < STEPS {
                go_rx.lock().unwrap().recv().map_err(io::Error::other)?;
            }
        }
        Ok(())
    });
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join("big.part");
    let download = tokio::spawn(loopback_download(format!("{}/big", server.origin), part.clone(), server.origin.clone()));
    for step in 1..STEPS {
        assert_eq!(sent_rx.recv().await, Some(step));
        // A body that is buffered until the end leaves the file empty here and fails.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) < (step * STEP) as u64 {
            let held = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            assert!(std::time::Instant::now() < deadline, "after {step} of {STEPS} steps the file holds only {held} bytes");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        go_tx.send(()).unwrap();
    }
    assert_eq!(sent_rx.recv().await, Some(STEPS));
    let hash = download.await.unwrap().unwrap();
    assert_eq!(std::fs::metadata(&part).unwrap().len(), (STEP * STEPS) as u64);
    use sha2::{Digest, Sha256};
    let mut sent = Sha256::new();
    for step in 1..=STEPS {
        sent.update(block(step));
    }
    assert_eq!(hash, faber_engine::hex(&sent.finalize()));
}

#[tokio::test]
async fn a_metadata_document_over_four_mebibytes_is_refused() {
    const LIMIT: usize = 4 * 1024 * 1024;
    let server = loopback::Server::start(|_, path, stream| match path {
        "/exact" => loopback::send(stream, &vec![b' '; LIMIT]),
        "/declared" => loopback::send(stream, &vec![b' '; LIMIT + 1]),
        // No length: the body runs until the connection closes, so only the count of bytes received can stop it.
        _ => {
            loopback::head(stream, None)?;
            io::Write::write_all(stream, &vec![b' '; LIMIT + 1])
        }
    });
    let fetch = |path: &str| {
        let (url, origin) = (format!("{}{path}", server.origin), server.origin.clone());
        async move { faber_engine::install::default_fetch(&url, Some(&origin)).await }
    };
    assert_eq!(fetch("/exact").await.unwrap().len(), LIMIT);
    for path in ["/declared", "/undeclared"] {
        let error = fetch(path).await.expect_err("A document over the cap must be refused");
        assert!(error.to_string().contains("larger than"), "{path}: {error}");
    }
}

#[tokio::test]
async fn a_redirect_off_the_origin_or_an_error_status_ends_without_a_file() {
    let server = loopback::Server::start(|_, path, stream| match path {
        "/gone" => loopback::not_found(stream),
        _ => loopback::redirect(stream, "http://127.0.0.1:1/elsewhere"),
    });
    let dir = tempfile::tempdir().unwrap();
    for path in ["/start", "/gone"] {
        let url = format!("{}{path}", server.origin);
        assert!(faber_engine::install::default_fetch(&url, Some(&server.origin)).await.is_err(), "{path}");
        let part = dir.path().join("refused.part");
        assert!(loopback_download(url, part.clone(), server.origin.clone()).await.is_err(), "{path}");
        assert!(!part.exists(), "{path}");
    }
}

/// A ZIP of deflated members, like the official Codex release.
fn deflated_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in members {
            writer.start_file(*name, options).unwrap();
            io::Write::write_all(&mut writer, bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    buffer.into_inner()
}

#[tokio::test]
async fn only_the_release_reaches_the_destination_never_the_download() {
    let claude = Rig::new("claude");
    claude.run().await.unwrap();
    assert_eq!(tree(&claude.destination()), ["claude.exe"]);
    // The official installer was started from the renamed download, which sat in install-tmp, not in the download folder.
    assert_eq!(claude.calls.lock().unwrap()[0].0, claude.temporary().join("claude-2.1.89-win32-x64.exe"));

    let mut codex = Rig::new("codex");
    let archive = deflated_zip(&[("codex.exe", b"abc"), ("resources/config.json", b"{}")]);
    let mut release: Value = serde_json::from_slice(&codex.responses[RELEASE]).unwrap();
    release["assets"][1]["digest"] = json!(format!("sha256:{}", sha_hex(&archive)));
    codex.responses.insert(RELEASE.into(), serde_json::to_vec(&release).unwrap());
    codex.responses.insert(ARCHIVE.into(), archive);
    codex.run().await.unwrap();
    assert_eq!(tree(&codex.destination()), ["codex.exe", "resources/config.json"]);
    codex.assert_empty_temporary();
}

#[tokio::test]
async fn a_failed_write_is_an_install_error_and_a_broken_connection_a_download_error() {
    for vendor in ["claude", "codex"] {
        for write_failed in [true, false] {
            let rig = Rig::new(vendor);
            let download = move |_: &str, path: &Path| {
                std::fs::write(path, b"half of the body")?;
                let cause = io::Error::other("injected");
                Err(if write_failed { faber_engine::install::write_failure(cause) } else { cause })
            };
            let error = rig.run_with(&download).await.expect_err("A download that fails must fail the install");
            let key = if write_failed { "reason.setup.write" } else { "reason.setup.blocked" };
            assert_eq!(error["key"], key, "{vendor}, write failure: {write_failed}");
            assert_eq!(*rig.left_at_failure.lock().unwrap(), Some(vec![]), "The partial file is deleted before the failure is reported");
            assert!(rig.calls.lock().unwrap().is_empty());
            assert!(rig.paths.lock().unwrap().is_empty());
            assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 0);
            rig.assert_empty_temporary();
            rig.assert_stages(&["prerequisite", "download", "failed"]);
        }
    }
}

/// The release host: the Claude version, manifest and binary, and the Codex release and archive.
fn release_host(
    requests: Arc<Mutex<Vec<String>>>,
    binary: impl Fn(&mut std::net::TcpStream) -> io::Result<()> + Send + Sync + 'static,
) -> loopback::Server {
    let archive = zip_bytes();
    loopback::Server::start(move |origin, path, stream| {
        requests.lock().unwrap().push(path.to_string());
        match path {
            "/latest" => loopback::send(stream, b"2.1.89\n"),
            "/2.1.89/manifest.json" => {
                loopback::send(stream, json!({"platforms": {"win32-x64": {"checksum": BINARY_SHA}}}).to_string().as_bytes())
            }
            "/2.1.89/win32-x64/claude.exe" => binary(stream),
            "/releases/latest" => {
                let asset = json!({"name": "codex-x86_64-pc-windows-msvc.exe.zip", "browser_download_url": format!("{origin}/codex.zip"),
                    "digest": format!("sha256:{ZIP_SHA}")});
                loopback::send(stream, json!({"assets": [asset]}).to_string().as_bytes())
            }
            "/codex.zip" => loopback::send(stream, &archive),
            _ => loopback::not_found(stream),
        }
    })
}

fn use_loopback(rig: &mut Rig, origin: &str) {
    rig.request["sources"] = json!({"loopback": origin, "claude_base": origin, "codex_release": format!("{origin}/releases/latest")});
}

#[tokio::test]
async fn a_whole_install_through_the_real_download_works_for_both_vendors() {
    for vendor in ["claude", "codex"] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server = release_host(requests.clone(), |stream| loopback::send(stream, b"abc"));
        let mut rig = Rig::new(vendor);
        use_loopback(&mut rig, &server.origin);
        // On Windows the stand-in installer opens the staged file exclusively: it fails while Faber holds any handle.
        let installed = rig.run_network().await.expect("The install must succeed through the real download");
        assert_eq!(std::fs::read(rig.destination().join(format!("{vendor}.exe"))).unwrap(), b"abc");
        rig.assert_detected(vendor, &installed);
        rig.assert_empty_temporary();
        rig.assert_stages(&["prerequisite", "download", "verify", "install", "done"]);
        let expected: &[&str] = if vendor == "claude" {
            &["/latest", "/2.1.89/manifest.json", "/2.1.89/win32-x64/claude.exe"]
        } else {
            &["/releases/latest", "/codex.zip"]
        };
        assert_eq!(*requests.lock().unwrap(), expected);
    }
}

#[tokio::test]
async fn a_connection_cut_halfway_through_the_claude_download_leaves_no_file() {
    let server = release_host(Arc::default(), |stream| {
        loopback::head(stream, Some(1024 * 1024))?;
        io::Write::write_all(stream, &[7u8; 100 * 1024])
    });
    let mut rig = Rig::new("claude");
    use_loopback(&mut rig, &server.origin);
    let error = rig.run_network().await.expect_err("A body that stops short must fail the install");
    assert_eq!(error["key"], "reason.setup.connection");
    assert_eq!(*rig.left_at_failure.lock().unwrap(), Some(vec![]));
    assert!(rig.calls.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(rig.destination()).unwrap().count(), 0);
    rig.assert_empty_temporary();
    rig.assert_stages(&["prerequisite", "download", "failed"]);
}

#[tokio::test]
async fn a_server_that_stalls_halfway_fails_the_download_and_leaves_no_part_file() {
    let (hold_tx, hold_rx) = std::sync::mpsc::channel::<()>();
    let hold_rx = Mutex::new(hold_rx);
    let server = loopback::Server::start(move |_, _, stream| {
        loopback::head(stream, Some(1024 * 1024))?;
        io::Write::write_all(stream, &[7u8; 100 * 1024])?;
        // Keep the connection open and say nothing until the test is over.
        let _ = hold_rx.lock().unwrap().recv();
        Ok(())
    });
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join("stalled.part");
    let url = format!("{}/stalled", server.origin);
    let create = |path: &Path| std::fs::File::create(path);
    let idle = std::time::Duration::from_millis(500);
    let started = std::time::Instant::now();
    let run = faber_engine::install::download_with(&url, &part, Some(&server.origin), idle, create);
    let error =
        tokio::time::timeout(std::time::Duration::from_secs(30), run).await.expect("A stalled body must fail, not hang").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(!faber_engine::install::is_write_failure(&error), "a stall is a network failure: {error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
    assert!(!part.exists());
    drop(hold_tx);
}

#[tokio::test]
async fn installing_one_cli_never_touches_the_staging_of_the_other() {
    let claude = Rig::new("claude");
    let codex_staging = claude.temporary().with_file_name("codex");
    std::fs::create_dir_all(codex_staging.join("download")).unwrap();
    std::fs::write(codex_staging.join("download/codex.zip.part"), b"half a download").unwrap();
    claude.run().await.expect("Claude installs while Codex is still downloading");
    assert_eq!(std::fs::read(codex_staging.join("download/codex.zip.part")).unwrap(), b"half a download");
}

#[tokio::test]
async fn a_second_request_for_the_same_cli_leaves_the_first_one_alone() {
    let rig = Rig::new("claude");
    let scripted = scripted_download(&rig.responses);
    let (request, second) = (rig.request.clone(), Arc::new(Mutex::new(None)));
    let heard = second.clone();
    // The download of the first install asks for a second install of the same CLI, as a reopened account card would.
    let download = move |url: &str, path: &Path| {
        let request = request.clone();
        let again = std::thread::spawn(move || {
            let idle: Spawner = Arc::new(|_, _, _| Err(io::Error::other("the second request must not start anything")));
            let (fetch, download) =
                (|_: &str| Err(io::Error::other("no network")), |_: &str, _: &Path| Err(io::Error::other("no network")));
            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(Cli::install_windows(
                &request,
                &fetch,
                &download,
                &|_| Ok(()),
                idle,
                &|_| {},
            ))
        })
        .join()
        .unwrap();
        *heard.lock().unwrap() = Some(again);
        scripted(url, path)
    };
    rig.run_with(&download).await.expect("The first install finishes although a second was asked for");
    let again = second.lock().unwrap().take().expect("the download ran");
    assert_eq!(again.expect("a repeated request is not an error")["alreadyRunning"], true);
    assert_eq!(rig.calls.lock().unwrap().len(), 1, "only the first install started the installer");
    assert_eq!(rig.events.lock().unwrap().last().unwrap()["stage"], "done");
    rig.assert_empty_temporary();
}

#[tokio::test]
async fn install_failure_reasons_are_specific_and_clean_up() {
    for (kind, key) in [
        (io::ErrorKind::NotConnected, "reason.setup.connection"),
        (io::ErrorKind::TimedOut, "reason.setup.timeout"),
        (io::ErrorKind::PermissionDenied, "reason.setup.write"),
    ] {
        let rig = Rig::new("claude");
        let download = move |_: &str, path: &Path| {
            std::fs::write(path, b"partial")?;
            let err = io::Error::new(kind, "injected");
            Err(if kind == io::ErrorKind::PermissionDenied { faber_engine::install::write_failure(err) } else { err })
        };
        let error = rig.run_with(&download).await.unwrap_err();
        assert_eq!(error["key"], key);
        rig.assert_empty_temporary();
    }
}

#[tokio::test]
async fn refused_connection_and_wrong_checksum_report_reasons_and_clean_up() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let mut rig = Rig::new("claude");
    use_loopback(&mut rig, &origin);
    let error = rig.run_network().await.unwrap_err();
    assert_eq!(error["key"], "reason.setup.connection");
    rig.assert_empty_temporary();

    let server = release_host(Arc::default(), |stream| loopback::send(stream, b"wrong"));
    let mut rig = Rig::new("claude");
    use_loopback(&mut rig, &server.origin);
    let error = rig.run_network().await.unwrap_err();
    assert_eq!(error["key"], "reason.setup.checksum");
    rig.assert_empty_temporary();
}
