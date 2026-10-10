//! Installation of official AI CLIs (Claude Code, Codex) on Windows.

use crate::proc::Spawner;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};

pub type Fetch<'a> = dyn Fn(&str) -> io::Result<Vec<u8>> + Send + Sync + 'a;
pub type Register<'a> = dyn Fn(&Path) -> io::Result<()> + Send + Sync + 'a;
pub type Progress<'a> = dyn Fn(Value) + Send + Sync + 'a;

const OFFICIAL_HOSTS: &[&str] =
    &["downloads.claude.ai", "api.github.com", "github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"];

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

struct CleanupGuard<'a>(&'a Path);
impl Drop for CleanupGuard<'_> {
    fn drop(&mut self) {
        clean_temp_dir(self.0);
    }
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

fn sanitize_archive_path(path: &Path) -> Result<PathBuf, String> {
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

fn extract_zip(archive_bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(io::Cursor::new(archive_bytes)).map_err(|e| format!("Failed to read ZIP archive: {e}"))?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| format!("Failed to read ZIP entry: {e}"))?;
        let enclosed = match file.enclosed_name() {
            Some(p) => sanitize_archive_path(&p)?,
            None => return Err(format!("Unsafe path in ZIP entry: {}", file.name())),
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

fn extract_tar_gz(archive_bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let gz = flate2::read::GzDecoder::new(io::Cursor::new(archive_bytes));
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
    let tmp_dir = data_dir.join("install-tmp");

    let allowed_hosts: Vec<String> = request["sources"]["allowed_hosts"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let _ = std::fs::create_dir_all(&tmp_dir);
    clean_temp_dir(&tmp_dir);
    let _guard = CleanupGuard(&tmp_dir);

    // Stage: prerequisite
    notify(progress, "prerequisite", None);

    let claude_base = request["sources"]["claude_base"].as_str().unwrap_or("https://downloads.claude.ai/claude-code-releases");
    let codex_release = request["sources"]["codex_release"].as_str().unwrap_or("https://api.github.com/repos/openai/codex/releases/latest");

    if vendor == "claude" {
        if let Err(err) = validate_url(claude_base, &allowed_hosts) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
    } else if let Err(err) = validate_url(codex_release, &allowed_hosts) {
        notify(progress, "failed", Some(("error.setup.download", &err)));
        return Err(json!({ "key": "error.setup.download", "detail": err }));
    }

    if let Err(e) = std::fs::create_dir_all(&destination) {
        let detail = format!("Failed to create destination directory: {e}");
        notify(progress, "failed", Some(("error.setup.install", &detail)));
        return Err(json!({ "key": "error.setup.install", "detail": detail }));
    }

    // Stage: download
    notify(progress, "download", None);

    if vendor == "claude" {
        let latest_url = format!("{}/latest", claude_base.trim_end_matches('/'));
        if let Err(err) = validate_url(&latest_url, &allowed_hosts) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let latest_bytes = match fetch(&latest_url) {
            Ok(b) => b,
            Err(e) => {
                let detail = format!("Failed to fetch latest Claude version: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };
        let version = std::str::from_utf8(&latest_bytes).map_err(|e| e.to_string()).map(|s| s.trim().to_string()).unwrap_or_default();
        if version.is_empty() {
            let detail = "Empty version string returned for Claude".to_string();
            notify(progress, "failed", Some(("error.setup.download", &detail)));
            return Err(json!({ "key": "error.setup.download", "detail": detail }));
        }

        let manifest_url = format!("{}/{}/manifest.json", claude_base.trim_end_matches('/'), version);
        if let Err(err) = validate_url(&manifest_url, &allowed_hosts) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let manifest_bytes = match fetch(&manifest_url) {
            Ok(b) => b,
            Err(e) => {
                let detail = format!("Failed to fetch Claude manifest: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
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
        if let Err(err) = validate_url(&binary_url, &allowed_hosts) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }
        let binary_bytes = match fetch(&binary_url) {
            Ok(b) => b,
            Err(e) => {
                let detail = format!("Failed to download Claude executable: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        // Stage: verify
        notify(progress, "verify", None);

        let mut hasher = Sha256::new();
        hasher.update(&binary_bytes);
        let digest = format!("{:x}", hasher.finalize());

        if !digest.eq_ignore_ascii_case(&expected_checksum) {
            let detail = format!("Checksum mismatch for Claude binary: expected {expected_checksum}, got {digest}");
            notify(progress, "failed", Some(("error.setup.checksum", &detail)));
            return Err(json!({ "key": "error.setup.checksum", "detail": detail }));
        }

        // Stage: install
        notify(progress, "install", None);

        let staged_exe = tmp_dir.join(format!("claude-{version}-{platform}.exe"));
        if let Err(e) = std::fs::write(&staged_exe, &binary_bytes) {
            let detail = format!("Failed to write staged Claude executable: {e}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
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

        let registered_str = install_dir.to_string_lossy();
        let detected = crate::cli::Cli::find_in(vendor, &registered_str, true, &|_| None, &|p| Path::new(p).is_file(), &|_| String::new());
        if detected.is_none() {
            let detail = format!("CLI detection cannot find installed program {vendor} in registered PATH {install_dir:?}");
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
                let detail = format!("Failed to fetch Codex release metadata: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
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

        if let Err(err) = validate_url(download_url, &allowed_hosts) {
            notify(progress, "failed", Some(("error.setup.download", &err)));
            return Err(json!({ "key": "error.setup.download", "detail": err }));
        }

        let archive_bytes = match fetch(download_url) {
            Ok(b) => b,
            Err(e) => {
                let detail = format!("Failed to download Codex archive: {e}");
                notify(progress, "failed", Some(("error.setup.download", &detail)));
                return Err(json!({ "key": "error.setup.download", "detail": detail }));
            }
        };

        // Stage: verify
        notify(progress, "verify", None);

        let mut hasher = Sha256::new();
        hasher.update(&archive_bytes);
        let digest = format!("{:x}", hasher.finalize());

        if !digest.eq_ignore_ascii_case(expected_digest) {
            let detail = format!("Checksum mismatch for Codex archive: expected {expected_digest}, got {digest}");
            notify(progress, "failed", Some(("error.setup.checksum", &detail)));
            return Err(json!({ "key": "error.setup.checksum", "detail": detail }));
        }

        // Stage: install
        notify(progress, "install", None);

        let staging_dir = tmp_dir.join("codex-staging");
        let _ = std::fs::remove_dir_all(&staging_dir);
        if let Err(e) = std::fs::create_dir_all(&staging_dir) {
            let detail = format!("Failed to create staging directory {staging_dir:?}: {e}");
            notify(progress, "failed", Some(("error.setup.install", &detail)));
            return Err(json!({ "key": "error.setup.install", "detail": detail }));
        }

        let extract_result = if download_url.ends_with(".zip") {
            extract_zip(&archive_bytes, &staging_dir)
        } else {
            extract_tar_gz(&archive_bytes, &staging_dir)
        };

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

        let registered_str = install_dir.to_string_lossy();
        let detected = crate::cli::Cli::find_in(vendor, &registered_str, true, &|_| None, &|p| Path::new(p).is_file(), &|_| String::new());
        if detected.is_none() {
            let detail = format!("CLI detection cannot find installed program {vendor} in registered PATH {install_dir:?}");
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

/// Fallback network fetch using reqwest when called from production without fakes.
pub async fn default_fetch(url: &str) -> io::Result<Vec<u8>> {
    validate_url(url, &[]).map_err(io::Error::other)?;
    let custom_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("Too many redirects")
        } else if let Err(e) = validate_url(attempt.url().as_str(), &[]) {
            attempt.error(e)
        } else {
            attempt.follow()
        }
    });
    let client = reqwest::Client::builder().redirect(custom_policy).build().map_err(io::Error::other)?;
    let res = client.get(url).send().await.map_err(io::Error::other)?;
    if !res.status().is_success() {
        return Err(io::Error::other(format!("HTTP error {}", res.status())));
    }
    let bytes = res.bytes().await.map_err(io::Error::other)?;
    Ok(bytes.to_vec())
}

/// Registers the given directory into the user PATH via Windows registry.
pub fn default_register_path(dir: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (env, _) = hkcu.create_subkey("Environment")?;
        let current: String = env.get_value("Path").or_else(|_| env.get_value("PATH")).unwrap_or_default();
        let dir_str = dir.to_string_lossy();
        let parts: Vec<&str> = current.split(';').filter(|s| !s.is_empty()).collect();
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(&dir_str)) {
            let new_path = if current.is_empty() { dir_str.to_string() } else { format!("{current};{dir_str}") };
            env.set_value("Path", &new_path)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(())
    }
}
