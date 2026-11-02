//! Installation of official AI CLIs (Claude Code, Codex) on Windows.

use crate::proc::Spawner;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Fetches a small metadata document into memory (the Claude version and manifest, the Codex release JSON).
pub type Fetch<'a> = dyn Fn(&str) -> io::Result<Vec<u8>> + Send + Sync + 'a;
/// Streams a payload into the file at the given path and returns its SHA-256 as lowercase hex. The file is closed
/// when the call returns, and a failed call leaves no file behind.
pub type Download<'a> = dyn Fn(&str, &Path) -> io::Result<String> + Send + Sync + 'a;
pub type Register<'a> = dyn Fn(&Path) -> io::Result<()> + Send + Sync + 'a;
pub type Progress<'a> = dyn Fn(Value) + Send + Sync + 'a;

const OFFICIAL_HOSTS: &[&str] = &[
    "downloads.claude.ai",
    "api.github.com",
    "github.com",
    "codeload.github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
    "registry.npmjs.org",
];

/// Verifies that a network URL uses HTTPS and points to an approved official host.
pub fn validate_url(url: &str, allowed_hosts: &[String]) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("URL must use HTTPS: {url}"));
    }
    let host = parsed.host_str().ok_or_else(|| format!("Empty host in URL: {url}"))?;
    let is_official = OFFICIAL_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host));
    if !is_official {
        return Err(format!("Host '{host}' is not in official hosts list"));
    }
    if !allowed_hosts.is_empty() && !allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return Err(format!("Host '{host}' is not in allowed hosts list"));
    }
    Ok(())
}

/// Empties the temporary staging folder without removing the folder itself.
pub fn clean_temp_dir(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Staging folders of the installs running now. Two installs of one CLI would delete each other's files, so the second
/// does not start.
static RUNNING: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Owns one install's staging folder: empties it when the install ends, however it ends.
struct CleanupGuard(PathBuf);
impl CleanupGuard {
    /// `None` while another install already owns this folder.
    fn claim(dir: &Path) -> Option<Self> {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        if running.iter().any(|d| d == dir) {
            return None;
        }
        running.push(dir.to_path_buf());
        Some(Self(dir.to_path_buf()))
    }
}
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        clean_temp_dir(&self.0);
        RUNNING.lock().unwrap_or_else(|e| e.into_inner()).retain(|d| d != &self.0);
    }
}

/// Reports a failed stage and builds the error that `install_windows` returns for it.
fn failure(progress: &Progress<'_>, key: &str, detail: String) -> Value {
    notify(progress, "failed", Some((key, &detail)));
    json!({ "key": key, "detail": detail })
}

fn download_reason(e: &io::Error) -> &'static str {
    if is_write_failure(e) {
        "reason.setup.write"
    } else if e.kind() == io::ErrorKind::TimedOut {
        "reason.setup.timeout"
    } else if matches!(
        e.kind(),
        io::ErrorKind::NotConnected | io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
    ) {
        "reason.setup.connection"
    } else {
        "reason.setup.blocked"
    }
}

fn network_error(e: reqwest::Error) -> io::Error {
    let kind = if e.is_timeout() {
        io::ErrorKind::TimedOut
    } else if e.is_connect() || e.is_body() {
        io::ErrorKind::NotConnected
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, e)
}

fn body_error(e: reqwest::Error) -> io::Error {
    let kind = if e.is_timeout() { io::ErrorKind::TimedOut } else { io::ErrorKind::NotConnected };
    io::Error::new(kind, e)
}

/// Downloads a payload into `part`, removing a partial file on failure.
fn download_payload(download: &Download<'_>, url: &str, part: &Path, what: &str, progress: &Progress<'_>) -> Result<String, Value> {
    download(url, part).map_err(|e| {
        let _ = std::fs::remove_file(part);
        let key = download_reason(&e);
        failure(progress, key, format!("Failed to download {what}: {e}"))
    })
}

/// Compares the hash of a downloaded payload with the published one. A payload that does not match is deleted at once.
fn verify_payload(part: &Path, got: &str, expected: &str, what: &str, progress: &Progress<'_>) -> Result<(), Value> {
    if got.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    let _ = std::fs::remove_file(part);
    Err(failure(progress, "reason.setup.checksum", format!("Checksum mismatch for {what}: expected {expected}, got {got}")))
}

fn notify(progress: &Progress<'_>, stage: &str, err: Option<(&str, &str)>) {
    let (key, detail) = match err {
        Some((k, d)) => (Value::String(k.to_string()), Value::String(d.to_string())),
        None => (Value::Null, Value::Null),
    };
    progress(json!({
        "stage": stage,
        "key": key,
        "detail": detail,
    }));
}

pub(crate) fn sanitize_archive_path(path: &Path) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(c) => clean.push(c),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => return Err(format!("Parent directory traversal in archive path: {path:?}")),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(format!("Absolute path not allowed in archive: {path:?}"));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(format!("Invalid path in archive: {path:?}"));
    }
    Ok(clean)
}

fn target_triple(arch: &str) -> Result<&'static str, String> {
    match arch {
        "x86_64" | "x64" => Ok("x86_64-pc-windows-msvc"),
        "arm64" | "aarch64" => Ok("aarch64-pc-windows-msvc"),
        _ => Err(format!("Unsupported Windows architecture: {arch}")),
    }
}

fn identify_and_promote_codex_executable(staging_dir: &Path, arch: &str) -> Result<PathBuf, String> {
    let triple = target_triple(arch)?;
    let arch_exe_name = format!("codex-{triple}.exe");

    // 1. Direct codex.exe in root
    let root_codex = staging_dir.join("codex.exe");
    if root_codex.is_file() {
        return Ok(PathBuf::from("codex.exe"));
    }

    // 2. Direct codex-{triple}.exe in root -> promote to codex.exe
    let root_arch_exe = staging_dir.join(&arch_exe_name);
    if root_arch_exe.is_file() {
        std::fs::rename(&root_arch_exe, &root_codex)
            .or_else(|_| std::fs::copy(&root_arch_exe, &root_codex).map(|_| ()))
            .map_err(|e| format!("Failed to establish codex.exe from {root_arch_exe:?}: {e}"))?;
        let _ = std::fs::remove_file(&root_arch_exe);
        return Ok(PathBuf::from("codex.exe"));
    }

    // 3. bin/codex.exe
    let bin_codex = staging_dir.join("bin").join("codex.exe");
    if bin_codex.is_file() {
        return Ok(PathBuf::from("bin").join("codex.exe"));
    }

    // 4. bin/codex-{triple}.exe -> promote to bin/codex.exe
    let bin_arch_exe = staging_dir.join("bin").join(&arch_exe_name);
    if bin_arch_exe.is_file() {
        std::fs::rename(&bin_arch_exe, &bin_codex)
            .or_else(|_| std::fs::copy(&bin_arch_exe, &bin_codex).map(|_| ()))
            .map_err(|e| format!("Failed to establish codex.exe from {bin_arch_exe:?}: {e}"))?;
        let _ = std::fs::remove_file(&bin_arch_exe);
        return Ok(PathBuf::from("bin").join("codex.exe"));
    }

    Err(format!("Official Codex binary (codex.exe or {arch_exe_name}) not found in extracted archive"))
}

fn publish_staging_to_destination(staging_dir: &Path, dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create destination directory {dest_dir:?}: {e}"))?;

    let mut to_visit = vec![staging_dir.to_path_buf()];
    while let Some(current) = to_visit.pop() {
        let entries = std::fs::read_dir(&current).map_err(|e| format!("Failed to read directory {current:?}: {e}"))?;
        for entry in entries.flatten() {
            let src_path = entry.path();
            let rel = src_path.strip_prefix(staging_dir).map_err(|e| e.to_string())?;
            let target_path = dest_dir.join(rel);
            if src_path.is_dir() {
                std::fs::create_dir_all(&target_path).map_err(|e| format!("Failed to create directory {target_path:?}: {e}"))?;
                to_visit.push(src_path);
            } else {
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory {parent:?}: {e}"))?;
                }
                if target_path.exists() {
                    let _ = std::fs::remove_file(&target_path);
                }
                std::fs::copy(&src_path, &target_path)
                    .map_err(|e| format!("Failed to copy file from {src_path:?} to {target_path:?}: {e}"))?;
            }
        }
    }
    Ok(())
}

fn extract_zip(archive: std::fs::File, dest_dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(io::BufReader::new(archive)).map_err(|e| format!("Failed to read ZIP archive: {e}"))?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| format!("Failed to read ZIP entry: {e}"))?;
        let enclosed = match file.enclosed_name() {
            Some(p) => sanitize_archive_path(&p)?,
            None => {
                return Err(format!(
                    "Unsafe path in ZIP entry: {}",
                    file.name().map_err(|e| format!("Failed to read ZIP entry name: {e}"))?
                ));
            }
        };
        let out_path = dest_dir.join(enclosed);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("Failed to create directory {out_path:?}: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory {parent:?}: {e}"))?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| format!("Failed to create file {out_path:?}: {e}"))?;
            io::copy(&mut file, &mut out).map_err(|e| format!("Failed to extract file {out_path:?}: {e}"))?;
        }
    }
    Ok(())
}

fn extract_tar_gz(archive: std::fs::File, dest_dir: &Path) -> Result<(), String> {
    let gz = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(gz);
    let entries = tar.entries().map_err(|e| format!("Failed to read tar entries: {e}"))?;
    for entry in entries {
        let mut file = entry.map_err(|e| format!("Failed to read tar entry: {e}"))?;
        let path = file.path().map_err(|e| format!("Failed to read tar path: {e}"))?;
        let clean = sanitize_archive_path(&path)?;
        let out_path = dest_dir.join(clean);
        if file.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("Failed to create directory {out_path:?}: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory {parent:?}: {e}"))?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| format!("Failed to create file {out_path:?}: {e}"))?;
            io::copy(&mut file, &mut out).map_err(|e| format!("Failed to extract file {out_path:?}: {e}"))?;
        }
    }
    Ok(())
}

/// Installs an official CLI on Windows according to the specification.
pub async fn install_windows(
    request: &Value,
    fetch: &Fetch<'_>,
    download: &Download<'_>,
    register: &Register<'_>,
    spawn: Spawner,
    progress: &Progress<'_>,
) -> Result<Value, Value> {
    let vendor = request["vendor"].as_str().unwrap_or_default();
    let is_windows = request["windows"].as_bool().unwrap_or(false);
    let arch = request["arch"].as_str().unwrap_or("x86_64");

    if !is_windows {
        let detail = "Only Windows installation is currently supported".to_string();
        notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
        return Err(json!({
            "key": "error.setup.unsupported",
            "detail": detail
        }));
    }

    if vendor != "claude" && vendor != "codex" {
        let detail = format!("Unsupported vendor: {vendor}");
        notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
        return Err(json!({
            "key": "error.setup.unsupported",
            "detail": detail
        }));
    }

    let data_dir = PathBuf::from(request["data_dir"].as_str().unwrap_or("."));
    let destination = PathBuf::from(request["destination"].as_str().unwrap_or("."));
    // One staging folder per CLI, so installing both at once cannot clean up the other's download or extraction.
    let tmp_dir = data_dir.join("install-tmp").join(vendor);
    // Downloads never share a folder with an extraction tree: an archive must not be published as release content.
    let download_dir = tmp_dir.join("download");

    let allowed_hosts: Vec<String> = request["sources"]["allowed_hosts"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    // Tests point every download at one loopback server; then nothing else, official hosts included, is reachable.
    let loopback = match request["sources"]["loopback"].as_str() {
        Some(raw) => match loopback_origin(raw) {
            Some(origin) => Some(origin),
            None => {
                let detail = format!("Only a loopback address may replace the official sources: {raw}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        },
        None => None,
    };
    let check_url = |url: &str| allowed_url(url, &allowed_hosts, loopback.as_deref());

    // The same CLI is already being installed (the account cards were closed and opened again): that install reports
    // its own progress, so this request has nothing to add.
    let Some(_guard) = CleanupGuard::claim(&tmp_dir) else {
        return Ok(json!({ "alreadyRunning": true }));
    };
    let _ = std::fs::create_dir_all(&tmp_dir);
    clean_temp_dir(&tmp_dir);

    // Stage: prerequisite
    notify(progress, "prerequisite", None);

    let claude_base = request["sources"]["claude_base"].as_str().unwrap_or("https://downloads.claude.ai/claude-code-releases");
    let codex_release = request["sources"]["codex_release"].as_str().unwrap_or("https://api.github.com/repos/openai/codex/releases/latest");

    if vendor == "claude" {
        if let Err(err) = check_url(claude_base) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
    } else if let Err(err) = check_url(codex_release) {
        notify(progress, "failed", Some(("error.setup.download", &err)));
        return Err(json!({ "key": "error.setup.download", "detail": err }));
    }

    if let Err(e) = std::fs::create_dir_all(&destination) {
        return Err(failure(progress, "reason.setup.write", format!("Failed to create destination directory: {e}")));
    }
    if let Err(e) = std::fs::create_dir_all(&download_dir) {
        return Err(failure(progress, "reason.setup.write", format!("Failed to create download directory: {e}")));
    }

    // Stage: download
    notify(progress, "download", None);

    if vendor == "claude" {
        let latest_url = format!("{}/latest", claude_base.trim_end_matches('/'));
        if let Err(err) = check_url(&latest_url) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let latest_bytes = match fetch(&latest_url) {
            Ok(b) => b,
            Err(e) => {
                return Err(failure(progress, download_reason(&e), format!("Failed to fetch latest Claude version: {e}")));
            }
        };
        let version = std::str::from_utf8(&latest_bytes).map_err(|e| e.to_string()).map(|s| s.trim().to_string()).unwrap_or_default();
        // the version names a file in the staging folder: `..\` or a drive letter there would write outside it
        if !valid_version(&version) {
            let detail = format!("Unexpected version string returned for Claude: {version:?}");
            notify(progress, "failed", Some(("error.setup.download", &detail)));
            return Err(json!({ "key": "error.setup.download", "detail": detail }));
        }

        let manifest_url = format!("{}/{}/manifest.json", claude_base.trim_end_matches('/'), version);
        if let Err(err) = check_url(&manifest_url) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let manifest_bytes = match fetch(&manifest_url) {
            Ok(b) => b,
            Err(e) => {
                return Err(failure(progress, download_reason(&e), format!("Failed to fetch Claude manifest: {e}")));
            }
        };
        let manifest: Value = match serde_json::from_slice(&manifest_bytes) {
            Ok(v) => v,
            Err(e) => {
                let detail = format!("Failed to parse Claude manifest JSON: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        let platform = match arch {
            "x86_64" | "x64" => "win32-x64",
            "arm64" | "aarch64" => "win32-arm64",
            _ => {
                let detail = format!("Unsupported Windows architecture: {arch}");
                notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
                return Err(json!({ "key": "error.setup.unsupported", "detail": detail }));
            }
        };

        let expected_checksum = match manifest["platforms"][platform]["checksum"].as_str() {
            Some(c) => c.to_string(),
            None => {
                let detail = format!("Platform {platform} missing checksum in manifest");
                notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
                return Err(json!({ "key": "error.setup.unsupported", "detail": detail }));
            }
        };

        let binary_url = format!("{}/{}/{}/claude.exe", claude_base.trim_end_matches('/'), version, platform);
        if let Err(err) = check_url(&binary_url) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let part = download_dir.join(format!("claude-{version}-{platform}.exe.part"));
        let digest = download_payload(download, &binary_url, &part, "Claude executable", progress)?;

        // Stage: verify
        notify(progress, "verify", None);
        verify_payload(&part, &digest, &expected_checksum, "Claude binary", progress)?;

        // Stage: install
        notify(progress, "install", None);

        // The download's handle is already closed, so Windows lets the renamed file start.
        let staged_exe = tmp_dir.join(format!("claude-{version}-{platform}.exe"));
        if let Err(e) = std::fs::rename(&part, &staged_exe) {
            return Err(failure(progress, "error.setup.install", format!("Failed to stage Claude executable: {e}")));
        }

        let staged_str = match staged_exe.to_str() {
            Some(s) => s,
            None => {
                let detail = "Invalid path encoding for staged executable".to_string();
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
        };

        let mut proc = match spawn(staged_str, &["install".to_string()], &data_dir) {
            Ok(p) => p,
            Err(e) => {
                let detail = format!("Failed to spawn Claude installer: {e}");
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
        };

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let out_task = tokio::io::copy(&mut proc.stdout, &mut stdout_buf);
        let err_task = async {
            if let Some(ref mut err) = proc.stderr {
                let _ = tokio::io::copy(err, &mut stderr_buf).await;
            }
        };
        let wait_task = async { if let Some(ref mut child) = proc.child { child.wait().await.map(Some) } else { Ok(None) } };
        let (_, _, wait_res) = tokio::join!(out_task, err_task, wait_task);

        match wait_res {
            Ok(Some(status)) if !status.success() => {
                let err_text = String::from_utf8_lossy(&stderr_buf);
                let out_text = String::from_utf8_lossy(&stdout_buf);
                let detail = if !err_text.trim().is_empty() {
                    format!("Claude installer failed with status {status}: {}", err_text.trim())
                } else if !out_text.trim().is_empty() {
                    format!("Claude installer failed with status {status}: {}", out_text.trim())
                } else {
                    format!("Claude installer exited with status {status}")
                };
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
            Err(e) => {
                let detail = format!("Failed waiting for Claude installer: {e}");
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
            _ => {}
        }

        // Determine where claude.exe was actually installed by the native installer
        let user_profile = request["user_profile"]
            .as_str()
            .map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
            .or_else(|| std::env::var("HOME").ok().map(PathBuf::from));

        let mut candidates = Vec::new();
        if let Some(ref prof) = user_profile {
            candidates.push(prof.join(".local").join("bin").join("claude.exe"));
        }
        let dest_exe = destination.join("claude.exe");
        if !candidates.contains(&dest_exe) {
            candidates.push(dest_exe);
        }

        let installed_exe = candidates.into_iter().find(|p| p.is_file());
        let final_exe = match installed_exe {
            Some(exe) => exe,
            None => {
                let detail = "Claude installer completed but claude.exe was not found in installed location".to_string();
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
        };

        let install_dir = final_exe.parent().unwrap_or(&destination);
        if let Err(e) = register(install_dir) {
            let detail = format!("Failed to register PATH for destination: {e}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        notify(progress, "done", None);

        Ok(json!({
            "command": final_exe.to_string_lossy(),
            "prefix": []
        }))
    } else {
        let release_bytes = match fetch(codex_release) {
            Ok(b) => b,
            Err(e) => {
                return Err(failure(progress, download_reason(&e), format!("Failed to fetch Codex release metadata: {e}")));
            }
        };
        let release: Value = match serde_json::from_slice(&release_bytes) {
            Ok(v) => v,
            Err(e) => {
                let detail = format!("Failed to parse Codex release metadata: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        let assets = match release["assets"].as_array() {
            Some(a) => a,
            None => {
                let detail = "Codex release metadata has no assets array".to_string();
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        let triple = match target_triple(arch) {
            Ok(t) => t,
            Err(detail) => {
                notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
                return Err(json!({ "key": "error.setup.unsupported", "detail": detail }));
            }
        };

        let expected_names = [format!("codex-{triple}.exe.zip"), format!("codex-{triple}.zip"), format!("codex-{triple}.tar.gz")];

        let matched_asset = assets.iter().find(|asset| {
            let name = asset["name"].as_str().unwrap_or("");
            expected_names.iter().any(|exp| exp.eq_ignore_ascii_case(name))
        });

        let asset = match matched_asset {
            Some(a) => a,
            None => {
                let detail = format!("No matching official Codex release asset found for Windows {arch}");
                notify(progress, "failed", Some(("error.setup.unsupported", &detail)));
                return Err(json!({ "key": "error.setup.unsupported", "detail": detail }));
            }
        };

        let download_url = match asset["browser_download_url"].as_str() {
            Some(u) => u,
            None => {
                let detail = "Missing browser_download_url in Codex release asset".to_string();
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        let raw_digest = match asset["digest"].as_str() {
            Some(d) => d,
            None => {
                let detail = "Missing digest in Codex release asset".to_string();
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };
        let expected_digest = raw_digest.strip_prefix("sha256:").unwrap_or(raw_digest);

        if let Err(err) = check_url(download_url) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }

        let part = download_dir.join(format!("codex-{triple}.part"));
        let digest = download_payload(download, download_url, &part, "Codex archive", progress)?;

        // Stage: verify
        notify(progress, "verify", None);
        verify_payload(&part, &digest, expected_digest, "Codex archive", progress)?;

        // Stage: install
        notify(progress, "install", None);

        let staging_dir = tmp_dir.join("codex-staging");
        let _ = std::fs::remove_dir_all(&staging_dir);
        if let Err(e) = std::fs::create_dir_all(&staging_dir) {
            let detail = format!("Failed to create staging directory {staging_dir:?}: {e}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        let archive = match std::fs::File::open(&part) {
            Ok(file) => file,
            Err(e) => return Err(failure(progress, "error.setup.install", format!("Failed to open the downloaded archive: {e}"))),
        };
        let extract_result =
            if download_url.ends_with(".zip") { extract_zip(archive, &staging_dir) } else { extract_tar_gz(archive, &staging_dir) };

        if let Err(detail) = extract_result {
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        let rel_exe = match identify_and_promote_codex_executable(&staging_dir, arch) {
            Ok(p) => p,
            Err(detail) => {
                notify(progress, "failed", Some(("error.setup.install", &detail)));
                return Err(json!({ "key": "error.setup.install", "detail": detail }));
            }
        };

        if let Err(detail) = publish_staging_to_destination(&staging_dir, &destination) {
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        let dest_file = destination.join(&rel_exe);
        if !dest_file.is_file() {
            let detail = format!("Codex binary not found in destination at {dest_file:?}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        let install_dir = dest_file.parent().unwrap_or(&destination);
        if let Err(e) = register(install_dir) {
            let detail = format!("Failed to register PATH for destination: {e}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        notify(progress, "done", None);

        Ok(json!({
            "command": dest_file.to_string_lossy(),
            "prefix": []
        }))
    }
}

/// `http://127.0.0.1:<port>` (or `[::1]`) as an origin, for tests that serve every download themselves.
pub fn loopback_origin(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw).ok()?;
    let ip: std::net::IpAddr = url.host_str()?.trim_start_matches('[').trim_end_matches(']').parse().ok()?;
    (url.scheme() == "http" && ip.is_loopback() && url.port().is_some()).then(|| url.origin().ascii_serialization())
}

/// An official HTTPS host, or, when a loopback origin replaces the sources, that exact origin and nothing else.
pub fn allowed_url(url: &str, allowed_hosts: &[String], loopback: Option<&str>) -> Result<(), String> {
    match loopback {
        None => validate_url(url, allowed_hosts),
        Some(origin) => match reqwest::Url::parse(url) {
            Ok(u) if u.origin().ascii_serialization() == origin => Ok(()),
            _ => Err(format!("URL is outside the loopback source {origin}: {url}")),
        },
    }
}

/// The most a metadata document may hold. A server that sends more is refused, so it cannot fill memory.
pub const METADATA_LIMIT: usize = 4 * 1024 * 1024;

/// A write that failed on this computer, as opposed to a network that failed. `install_windows` tells them apart.
#[derive(Debug)]
struct WriteFailure(io::Error);

impl std::fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not write the file: {}", self.0)
    }
}

impl std::error::Error for WriteFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// Marks an error as a failed write (a full disk, a locked file), which a `Download` reports separately from a network failure.
pub fn write_failure(e: io::Error) -> io::Error {
    io::Error::new(e.kind(), WriteFailure(e))
}

/// Whether a `Download` failed on this computer instead of on the network.
pub fn is_write_failure(e: &io::Error) -> bool {
    e.get_ref().is_some_and(|inner| inner.is::<WriteFailure>())
}

/// How long a connection may stay silent, while connecting or between two received chunks, before a fetch or a
/// download fails. A stalled server must not hold the install (and the screen's thread) forever.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Starts a GET: official hosts only, or only the given loopback origin, redirects included. Silence for longer than
/// `idle` fails the request; a loopback origin never goes through a proxy.
async fn get(url: &str, loopback: Option<&str>, idle: Duration) -> io::Result<reqwest::Response> {
    allowed_url(url, &[], loopback).map_err(io::Error::other)?;
    crate::tls::install();
    let origin = loopback.map(String::from);
    let custom_policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("Too many redirects")
        } else if let Err(e) = allowed_url(attempt.url().as_str(), &[], origin.as_deref()) {
            attempt.error(e)
        } else {
            attempt.follow()
        }
    });
    let mut builder = reqwest::Client::builder().redirect(custom_policy).connect_timeout(idle).read_timeout(idle);
    if loopback.is_some() {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(io::Error::other)?;
    let res = client.get(url).send().await.map_err(network_error)?;
    if !res.status().is_success() {
        return Err(io::Error::other(format!("HTTP error {}", res.status())));
    }
    Ok(res)
}

/// Network fetch of a metadata document: at most `METADATA_LIMIT` bytes, held in memory.
pub async fn default_fetch(url: &str, loopback: Option<&str>) -> io::Result<Vec<u8>> {
    metadata_body(get(url, loopback, IDLE_TIMEOUT).await?, METADATA_LIMIT, url).await
}

/// Shared bounded response reader; callers own destination policy and deadlines.
pub(crate) async fn metadata_body(mut res: reqwest::Response, limit: usize, url: &str) -> io::Result<Vec<u8>> {
    let too_big = || io::Error::other(format!("The document is larger than {limit} bytes: {url}"));
    if res.content_length().is_some_and(|n| n > limit as u64) {
        return Err(too_big());
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(body_error)? {
        if body.len() + chunk.len() > limit {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Network download of a payload into the file at `dest`, one received chunk at a time, hashing as it goes. Returns the
/// SHA-256 in lowercase hex. The file is closed before this returns, and a failure deletes it.
pub async fn default_download(url: &str, dest: &Path, loopback: Option<&str>) -> io::Result<String> {
    download_with(url, dest, loopback, IDLE_TIMEOUT, |path| std::fs::File::create(path)).await
}

/// `default_download` with the silence limit and the destination's writer supplied by the caller, so a test can make
/// the disk fail or the server stall.
pub async fn download_with<W: Write>(
    url: &str,
    dest: &Path,
    loopback: Option<&str>,
    idle: Duration,
    create: impl FnOnce(&Path) -> io::Result<W>,
) -> io::Result<String> {
    let result = stream_to(url, dest, loopback, idle, create).await;
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

async fn stream_to<W: Write>(
    url: &str,
    dest: &Path,
    loopback: Option<&str>,
    idle: Duration,
    create: impl FnOnce(&Path) -> io::Result<W>,
) -> io::Result<String> {
    let mut res = get(url, loopback, idle).await?;
    let mut out = create(dest).map_err(write_failure)?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = res.chunk().await.map_err(body_error)? {
        hasher.update(&chunk);
        out.write_all(&chunk).map_err(write_failure)?;
    }
    out.flush().map_err(write_failure)?;
    // `out` is dropped here, before the caller sees the hash, so Windows lets the file be renamed, run or deleted.
    Ok(crate::hex(&hasher.finalize()))
}

/// A release version such as `2.1.80` or `2.1.80-beta+1`: nothing that could leave the folder it names a file in.
pub fn valid_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c)) && !v.contains("..")
}

/// The user's PATH with `dir` appended, or `None` when it is already there.
pub fn path_with(current: &str, dir: &str) -> Option<String> {
    let present = current.split(';').any(|p| p.trim_end_matches('\\').eq_ignore_ascii_case(dir.trim_end_matches('\\')));
    match (present, current.trim_end_matches(';')) {
        (true, _) => None,
        (false, "") => Some(dir.to_string()),
        (false, kept) => Some(format!("{kept};{dir}")),
    }
}

/// Registers the given directory into the user PATH via Windows registry. The value keeps its type: Windows stores it as
/// REG_EXPAND_SZ so entries like `%USERPROFILE%\...` expand, and rewriting it as REG_SZ would break them. A value that
/// cannot be read is an error, never an empty PATH to overwrite.
pub fn default_register_path(dir: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, RegType};
        use winreg::{RegKey, RegValue};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (env, _) = hkcu.create_subkey("Environment")?;
        let (current, vtype) = match env.get_raw_value("Path") {
            Ok(v) if v.vtype == RegType::REG_SZ || v.vtype == RegType::REG_EXPAND_SZ => {
                let units: Vec<u16> = v.bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                let text = String::from_utf16(&units).map_err(|_| io::Error::other("the user PATH is not valid text"))?;
                (text.trim_end_matches('\0').to_string(), v.vtype)
            }
            Ok(_) => return Err(io::Error::other("the user PATH has an unexpected registry type")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (String::new(), RegType::REG_EXPAND_SZ),
            Err(e) => return Err(e),
        };
        let dir = dir.to_str().ok_or_else(|| io::Error::other("the folder name is not valid text"))?;
        if let Some(new) = path_with(&current, dir) {
            let bytes: Vec<u8> = new.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
            env.set_raw_value("Path", &RegValue { bytes: bytes.into(), vtype })?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(())
    }
}
