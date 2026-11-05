//! Registry pins are curated ahead of time; installation never resolves versions or runs package scripts.
use super::{ARCHIVE_LIMIT, Downloaded, SERVER_LIMIT, Server, extract_package, extract_package_capped, portable_name};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub path: String,
    pub package: String,
    pub version: String,
    pub integrity: String,
    pub url: String,
}

fn name(name: &str) -> bool {
    let component =
        |part: &str| portable_name(part) && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b));
    if let Some(scoped) = name.strip_prefix('@') {
        scoped.split_once('/').is_some_and(|(scope, package)| component(scope) && component(package))
    } else {
        component(name)
    }
}

fn registry(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "error.tools.host")?;
    if !raw.starts_with("https://registry.npmjs.org/")
        || url.scheme() != "https"
        || url.host_str() != Some("registry.npmjs.org")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("error.tools.host".into());
    }
    Ok(url)
}

fn validate(server: &Server) -> Result<(), String> {
    if server.tree.is_empty() {
        return Ok(());
    }
    if server.entry.is_empty() || !server.entry.split('/').all(portable_name) || server.entry.starts_with('/') {
        return Err("error.tools.tree".into());
    }
    if server.tree.len() > 512 {
        return Err("error.tools.size".into());
    }
    let root = &server.tree[0];
    if !root.path.is_empty() || root.package != server.package || root.version != server.version {
        return Err("error.tools.tree".into());
    }
    let mut paths = BTreeSet::new();
    for package in &server.tree {
        registry(&package.url)?;
        let integrity = package.integrity.strip_prefix("sha512-").and_then(|value| STANDARD.decode(value).ok());
        if integrity.is_none_or(|bytes| bytes.len() != 64) {
            return Err("error.tools.integrity".into());
        }
        if !name(&package.package) || !crate::install::valid_version(&package.version) || !paths.insert(package.path.to_lowercase()) {
            return Err("error.tools.tree".into());
        }
        if package.path.is_empty() {
            continue;
        }
        let Some(path) = package.path.strip_prefix("node_modules/") else {
            return Err("error.tools.tree".into());
        };
        let parts: Vec<_> = path.split("/node_modules/").collect();
        let mut parent = String::new();
        for (index, part) in parts.iter().enumerate() {
            if !name(part) {
                return Err("error.tools.tree".into());
            }
            parent.push_str("node_modules/");
            parent.push_str(part);
            if index + 1 < parts.len() {
                if !paths.contains(&parent.to_lowercase()) {
                    return Err("error.tools.tree".into());
                }
                parent.push('/');
            } else if *part != package.package {
                return Err("error.tools.tree".into());
            }
        }
    }
    Ok(())
}

fn integrity(package: &Package, archive: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > ARCHIVE_LIMIT {
        return Err("error.tools.size".into());
    }
    let mut hash = Sha512::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    if package.integrity != format!("sha512-{}", STANDARD.encode(hash.finalize())) {
        return Err("error.tools.integrity".into());
    }
    Ok(())
}

pub(super) fn check_file(path: &Path, body: &[u8]) -> Result<(), String> {
    let filename = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_ascii_lowercase();
    if filename == "binding.gyp" || filename.ends_with(".node") {
        return Err("error.tools.native".into());
    }
    if filename == "package.json" {
        let manifest: serde_json::Value = serde_json::from_slice(body).map_err(|_| "error.tools.tree")?;
        if let Some(scripts) = manifest.get("scripts") {
            let scripts = scripts.as_object().ok_or("error.tools.tree")?;
            if ["preinstall", "install", "postinstall"].iter().any(|key| scripts.contains_key(*key)) {
                return Err("error.tools.scripts".into());
            }
        }
    }
    Ok(())
}

pub(super) fn extract_tree(server: &Server, root: &Path, packages: &BTreeMap<String, PathBuf>, dest: &Path) -> Result<(), String> {
    validate(server)?;
    if server.tree.is_empty() {
        return extract_package(root, dest);
    }
    let mut budget = (0, 0);
    for package in &server.tree {
        let archive = if package.path.is_empty() { root } else { packages.get(&package.path).ok_or("error.tools.tree")? };
        integrity(package, archive)?;
        let target = dest.join(&package.path);
        extract_package_capped(archive, &target, &mut budget, true)?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(target.join("package.json")).map_err(|_| "error.tools.tree")?)
                .map_err(|_| "error.tools.tree")?;
        if manifest["name"].as_str() != Some(&package.package) || manifest["version"].as_str() != Some(&package.version) {
            return Err("error.tools.tree".into());
        }
    }
    Ok(())
}

pub(super) async fn download_tree(server: &Server, root: &Path, loopback: Option<&str>, downloaded: &mut Downloaded) -> Result<(), String> {
    validate(server)?;
    // The shared CLI downloader allows GitHub redirects. A package tree never leaves the registry, even by redirect.
    crate::tls::install();
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(crate::install::IDLE_TIMEOUT)
        .read_timeout(crate::install::IDLE_TIMEOUT);
    if loopback.is_some() {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for package in &server.tree {
        let url = registry(&package.url)?;
        let url = loopback.map_or_else(|| url.to_string(), |origin| format!("{origin}{}", url.path()));
        let path = root.join(format!(".download-{}", uuid::Uuid::new_v4()));
        downloaded.files.push(path.clone());
        let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
        if response.status().is_redirection() {
            return Err("error.tools.host".into());
        }
        if !response.status().is_success() {
            return Err(format!("HTTP error {}", response.status()));
        }
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            total = total.checked_add(chunk.len() as u64).ok_or("error.tools.size")?;
            if total > SERVER_LIMIT {
                return Err("error.tools.size".into());
            }
            file.write_all(&chunk).map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        if !package.path.is_empty() {
            downloaded.packages.insert(package.path.clone(), path);
        }
    }
    Ok(())
}
