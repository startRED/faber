//! On-demand audio environments. Only original source URLs and pinned locks ship with Faber.
use super::{Capped, Downloaded, Item, Store, kits::TemporaryCopy, portable_name};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Individual catalog downloads: 400 MiB. GPU checkpoints and CUDA wheels: 3 GiB each;
/// a complete extracted environment: 12 GiB and 120,000 paths, never held in memory.
pub const FILE_LIMIT: u64 = 400 * 1024 * 1024;
const MODEL_LIMIT: u64 = 3 * 1024 * 1024 * 1024;
const ENV_LIMIT: u64 = 12 * 1024 * 1024 * 1024;
const VENV_ARGS: [&str; 10] =
    ["venv", "--python", "3.12", "--no-python-downloads", "--relocatable", "--link-mode", "copy", "--prompt", "faber-audio", ".venv"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub kind: Kind,
    pub source: String,
    pub sha256: String,
    pub size: u64,
    pub weights_license: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_vram_mb: Option<u64>,
    pub command: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Python,
    File,
}

pub(super) fn catalog_items<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Item>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let mut items = Vec::new();
    for value in values {
        let object = value.as_object().ok_or_else(|| serde::de::Error::custom("Runtime must be an item"))?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "id" | "cliName" | "job" | "tier" | "license" | "repo" | "commit" | "path" | "digest" | "server" | "chatSafe" | "runtime"
            )
        }) {
            return Err(serde::de::Error::custom("Unknown runtime item field"));
        }
        let item: Item = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        let runtime = item.runtime.as_ref().ok_or_else(|| serde::de::Error::custom("Missing runtime descriptor"))?;
        runtime.validate().map_err(serde::de::Error::custom)?;
        if item.server.is_some() || item.chat_safe || item.cli_name.is_some() || item.job != "audio" {
            return Err(serde::de::Error::custom("Invalid runtime item"));
        }
        items.push(item);
    }
    Ok(items)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    source: String,
    sha256: String,
    size: u64,
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Platform {
    lock: String,
    wheels: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    platforms: BTreeMap<String, Platform>,
    assets: Vec<Asset>,
    unpack: bool,
    entry: String,
}

fn bundles() -> &'static BTreeMap<String, Bundle> {
    static BUNDLES: std::sync::OnceLock<BTreeMap<String, Bundle>> = std::sync::OnceLock::new();
    BUNDLES.get_or_init(|| serde_json::from_str(include_str!("../runtime-bundles.json")).expect("checked runtime bundles"))
}

mod locks {
    include!("../runtime-locks/locks.rs");
}

fn wheel_index() -> &'static BTreeMap<&'static str, &'static serde_json::value::RawValue> {
    static FILES: std::sync::OnceLock<BTreeMap<&str, &serde_json::value::RawValue>> = std::sync::OnceLock::new();
    FILES.get_or_init(|| serde_json::from_str(include_str!("../runtime-wheel-files.json")).expect("checked wheel file index"))
}

fn wheel_files(hash: &str) -> Option<BTreeMap<&'static str, &'static str>> {
    serde_json::from_str(wheel_index().get(hash)?.get()).ok()
}

fn relative(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(|part| portable_name(part) && !matches!(part, "." | ".."))
}

fn pin(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Runtime {
    pub fn validate(&self) -> Result<(), String> {
        if !pin(&self.sha256)
            || self.size == 0
            || self.size > FILE_LIMIT
            || !relative(&self.command)
            || self.weights_license.is_empty()
            || !matches!(self.capability.as_str(), "tts" | "align" | "music" | "sfx" | "qa")
            || self.min_vram_mb == Some(0)
        {
            return Err("error.tools.digest".into());
        }
        crate::install::validate_url(&self.source, &[]).map_err(|_| "error.tools.host".into())
    }

    pub fn fits(&self, vram: Option<u64>) -> bool {
        self.min_vram_mb.is_none_or(|needed| vram.is_some_and(|available| available >= needed))
    }
}

pub fn available(item: &Item) -> bool {
    let architecture = item.runtime.as_ref().is_some_and(|runtime| runtime.kind == Kind::File)
        || cfg!(all(target_os = "macos", target_arch = "aarch64"))
        || cfg!(all(not(target_os = "macos"), target_arch = "x86_64"));
    architecture && bundles().get(&item.id).is_some_and(|bundle| bundle.platforms.contains_key(std::env::consts::OS))
}

pub fn uv() -> Option<crate::cli::Cli> {
    #[cfg(test)]
    if let Some(cli) =
        TEST_UV.with(|test| test.borrow().as_ref().map(|_| crate::cli::Cli { command: "C:/fake/uv.exe".into(), prefix: vec![] }))
    {
        return Some(cli);
    }
    crate::cli::find_cli("uv").filter(|cli| Path::new(&cli.command).is_absolute() && cli.prefix.is_empty())
}

pub fn download_size(item: &Item) -> u64 {
    let Some(runtime) = &item.runtime else { return 0 };
    let Some(bundle) = bundles().get(&item.id) else { return runtime.size };
    runtime.size
        + bundle.assets.iter().map(|asset| asset.size).sum::<u64>()
        + bundle.platforms.get(std::env::consts::OS).map_or(0, |platform| platform.wheels.iter().map(|wheel| wheel.size).sum())
}

pub(super) fn instructions(item: &Item) -> String {
    let runtime = item.runtime.as_ref().expect("runtime item");
    format!(
        "---\nname: {}\ndescription: Use the installed {} audio runtime.\n---\nUse the installed one first. Capability: {}. Licence: {} (weights: {}).\n",
        item.id, item.id, runtime.capability, item.license, runtime.weights_license
    )
}

pub(super) fn line(item: &Item, root: &Path) -> String {
    let runtime = item.runtime.as_ref().expect("runtime item");
    let command = command(root, runtime);
    let invocation = if runtime.kind == Kind::Python {
        format!("{} -I -B {}", quoted(&command), quoted(&root.join("run.py")))
    } else {
        quoted(&command)
    };
    let attribution = if runtime.weights_license.starts_with("CC-BY-") {
        let source = bundles().get(&item.id).and_then(|bundle| bundle.assets.first()).map_or(&runtime.source, |asset| &asset.source);
        format!("; attribution: {} ({})", item.repo, source)
    } else {
        String::new()
    };
    format!(
        "Installed audio runtime {}: {}; command {}; licence {} (weights: {}){}. Use the installed one.\n",
        item.id, runtime.capability, invocation, item.license, runtime.weights_license, attribution
    )
}

fn quoted(path: &Path) -> String {
    serde_json::to_string(&path.to_string_lossy()).expect("path string")
}

fn command(root: &Path, runtime: &Runtime) -> PathBuf {
    let path = if runtime.kind == Kind::Python && cfg!(windows) { ".venv/Scripts/python.exe" } else { &runtime.command };
    root.join(path)
}

fn checked(path: &Path, sha256: &str, size: u64) -> bool {
    check_file(path, sha256, size).unwrap_or(false)
}

fn check_file(path: &Path, sha256: &str, size: u64) -> io::Result<bool> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() != size {
        return Ok(false);
    }
    let mut file = std::fs::File::open(path)?;
    #[cfg(test)]
    if TEST_HASH_ERROR.with(|target| target.borrow().as_deref() == Some(path)) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hash.update(&buffer[..n]);
                #[cfg(test)]
                HASHED_BYTES.with(|bytes| bytes.set(bytes.get() + n as u64));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(crate::hex(&hash.finalize()) == sha256)
}

fn check_text(path: &Path, expected: &str) -> io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.is_file() && std::fs::read_to_string(path)? == expected)
}

pub(super) async fn download(store: &Store, item: &Item, source: Option<&str>) -> Result<Downloaded, String> {
    let runtime = item.runtime.as_ref().ok_or("error.tools.unknown")?;
    runtime.validate()?;
    let bundle = bundles().get(&item.id).ok_or("error.tools.host")?;
    if bundle.entry != runtime.command {
        return Err("error.tools.digest".into());
    }
    if !available(item) {
        return Err("error.tools.host".into());
    }
    if runtime.min_vram_mb.is_some() && !runtime.fits(if cfg!(test) { None } else { faber_winproc::available_vram_mb() }) {
        return Err("error.tools.vram".into());
    }
    if runtime.kind == Kind::Python && uv().is_none() {
        return Err("error.tools.uvMissing".into());
    }
    let platform = bundle.platforms.get(std::env::consts::OS).ok_or("error.tools.host")?;
    let origin = source.map(|url| crate::install::loopback_origin(url).ok_or("error.tools.host")).transpose()?;
    std::fs::create_dir_all(&store.root).map_err(|e| e.to_string())?;
    let needed = download_size(item).saturating_add(if runtime.kind == Kind::Python { ENV_LIMIT } else { FILE_LIMIT });
    let root = faber_winproc::rewind::Root::open(&store.root.canonicalize().map_err(|_| "error.tools.install")?)
        .map_err(|_| "error.tools.install")?;
    check_space(root.available_space().map_err(|_| "error.tools.install")?, needed)?;
    let temporary = TemporaryCopy::new(&store.root, ".staging-").map_err(|e| e.to_string())?;
    std::fs::create_dir(&temporary.path).map_err(|e| e.to_string())?;
    let mut downloaded = Downloaded { files: vec![], packages: BTreeMap::new(), slot: None, temporary: Some(temporary) };
    let primary = Asset { source: runtime.source.clone(), sha256: runtime.sha256.clone(), size: runtime.size, path: "source".into() };
    for asset in std::iter::once(&primary).chain(bundle.assets.iter()).chain(platform.wheels.iter()) {
        if !relative(&asset.path) || !pin(&asset.sha256) || asset.size == 0 || asset.size > MODEL_LIMIT {
            return Err("error.tools.size".into());
        }
        let path = downloaded.temporary.as_ref().expect("download guard").path.join(uuid::Uuid::new_v4().to_string());
        downloaded.files.push(path.clone());
        let url = match &origin {
            Some(origin) => format!("{origin}/{}", asset.path),
            None => asset.source.clone(),
        };
        let limit = asset.size;
        crate::install::download_with(&url, &path, origin.as_deref(), crate::install::IDLE_TIMEOUT, |path| {
            std::fs::File::create(path).map(|inner| Capped { inner, left: limit })
        })
        .await
        .map_err(|error| {
            if error.to_string().contains("archive is too large") {
                "error.tools.digest".to_string()
            } else {
                "error.tools.install".to_string()
            }
        })?;
        if !checked(&path, &asset.sha256, asset.size) {
            return Err("error.tools.digest".into());
        }
        downloaded.packages.insert(asset.path.clone(), path);
    }
    Ok(downloaded)
}

const UV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn uv_command(cli: &crate::cli::Cli, cwd: &Path, args: &[String]) -> std::process::Command {
    let mut process = std::process::Command::new(&cli.command);
    process.args(&cli.prefix).args(args).current_dir(cwd).env_clear();
    for key in ["PATH", "SystemRoot", "TEMP", "TMP", "USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        if let Some(value) = std::env::var_os(key) {
            process.env(key, value);
        }
    }
    process
        .env("UV_NO_PROGRESS", "1")
        .env("UV_NO_CONFIG", "1")
        .env("UV_NO_CACHE", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    process
}

fn wait_uv(child: &mut impl UvChild, timeout: std::time::Duration) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.done() {
            Ok(Some(true)) => return Ok(()),
            Ok(Some(false)) => return Err("error.tools.install".into()),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(10)),
            _ => {
                child.stop();
                return Err("error.tools.install".into());
            }
        }
    }
}

trait UvChild {
    fn done(&mut self) -> io::Result<Option<bool>>;
    fn stop(&mut self);
}
impl UvChild for std::process::Child {
    fn done(&mut self) -> io::Result<Option<bool>> {
        self.try_wait().map(|status| status.map(|status| status.success()))
    }
    fn stop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
}

fn run_uv(cli: &crate::cli::Cli, cwd: &Path, args: &[String]) -> Result<(), String> {
    // Tests intercept only spawn: argument/environment construction is always the production code.
    let mut process = uv_command(cli, cwd, args);
    #[cfg(test)]
    {
        let _ = &mut process;
        TEST_UV.with(|test| test.borrow_mut().as_mut().ok_or("No test uv runner")?(&process))
    }
    #[cfg(not(test))]
    {
        wait_uv(&mut process.spawn().map_err(|_| "error.tools.install")?, UV_TIMEOUT)
    }
}

fn uv_arguments(root: &Path, runtime: &Runtime) -> Result<[Vec<String>; 2], String> {
    let python = command(root, runtime);
    Ok([
        VENV_ARGS.map(String::from).to_vec(),
        [
            "pip",
            "install",
            "--python",
            python.to_str().ok_or("error.tools.install")?,
            "--offline",
            "--no-index",
            "--no-build",
            "--no-deps",
            "--find-links",
            "wheels",
            "--require-hashes",
            "-r",
            "requirements.txt",
        ]
        .map(String::from)
        .to_vec(),
    ])
}

fn interpreter_pin(path: &Path) -> io::Result<String> {
    let link = std::fs::symlink_metadata(path)?.file_type().is_symlink();
    let prefix = if link { format!("link:{}", std::fs::read_link(path)?.display()) } else { String::new() };
    let mut file =
        std::fs::File::open(path).map_err(|error| if link { io::Error::new(io::ErrorKind::WouldBlock, error) } else { error })?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > 40 * 1024 * 1024 {
        return Err(io::Error::other("Invalid interpreter file"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let digest = crate::hex(&hash.finalize());
    Ok(if link { format!("{prefix}:{digest}") } else { format!("{}:{digest}", meta.len()) })
}

fn interpreter_snapshot(venv: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut pins = BTreeMap::from([("pyvenv.cfg".into(), interpreter_pin(&venv.join("pyvenv.cfg"))?)]);
    let directory = if cfg!(windows) { "Scripts" } else { "bin" };
    for entry in std::fs::read_dir(venv.join(directory))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let name = if cfg!(windows) { name.to_ascii_lowercase() } else { name };
        if name.starts_with("python") || name.ends_with(".dll") {
            pins.insert(format!("{directory}/{name}"), interpreter_pin(&entry.path())?);
        }
    }
    if pins.len() < 2 {
        return Err(io::Error::other("Missing environment interpreter"));
    }
    Ok(pins)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Integrity {
    Intact,
    Tampered,
    CannotCheck,
}

fn content_integrity(result: io::Result<bool>) -> Integrity {
    match result {
        Ok(true) => Integrity::Intact,
        Ok(false) => Integrity::Tampered,
        Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory | io::ErrorKind::InvalidData) => {
            Integrity::Tampered
        }
        Err(_) => Integrity::CannotCheck,
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InterpreterPins {
    id: String,
    commit: String,
    source: String,
    files: BTreeMap<String, String>,
}

fn interpreter_registry(dir: &Path) -> io::Result<PathBuf> {
    let store = dir
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "tools"))
        .or_else(|| dir.parent())
        .ok_or_else(|| io::Error::other("Missing runtime store"))?;
    let registry = store.join("runtime-pins");
    if std::fs::symlink_metadata(&registry).is_ok_and(|meta| !meta.is_dir() || meta.file_type().is_symlink()) {
        return Err(io::Error::other("Invalid interpreter registry"));
    }
    Ok(registry)
}

fn record_interpreter(dir: &Path, item: &Item) -> io::Result<()> {
    let snapshot = InterpreterPins {
        id: item.id.clone(),
        commit: item.commit.clone(),
        source: item.runtime.as_ref().ok_or(io::ErrorKind::InvalidInput)?.sha256.clone(),
        files: interpreter_snapshot(&dir.join(".venv"))?,
    };
    let bytes = serde_json::to_vec(&snapshot)?;
    let digest = crate::hex(&Sha256::digest(&bytes));
    let registry = interpreter_registry(dir)?;
    std::fs::create_dir_all(&registry)?;
    let target = registry.join(&digest);
    let mut temporary = tempfile::NamedTempFile::new_in(&registry)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&target) {
        Ok(_) => {}
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            if !check_file(&target, &digest, bytes.len() as u64)? {
                error.file.persist(&target).map_err(|error| error.error)?;
            }
        }
        Err(error) => return Err(error.error),
    }
    std::fs::write(dir.join("interpreter.json"), digest)
}

fn interpreter_intact(dir: &Path, item: &Item) -> Integrity {
    let load = || -> io::Result<InterpreterPins> {
        // Runtime contents may name a snapshot but never create its authority. Trusted install metadata stays outside it.
        let mut receipt = Vec::new();
        faber_winproc::open_catalog_file(dir, Path::new("interpreter.json"))?.take(65).read_to_end(&mut receipt)?;
        let digest = String::from_utf8(receipt).map_err(|_| io::ErrorKind::InvalidData)?;
        if !pin(&digest) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let registry = interpreter_registry(dir)?;
        let mut bytes = Vec::new();
        faber_winproc::open_catalog_file(&registry, Path::new(&digest))?.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 || crate::hex(&Sha256::digest(&bytes)) != digest {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let pins: InterpreterPins = serde_json::from_slice(&bytes)?;
        if pins.id != item.id
            || pins.commit != item.commit
            || item.runtime.as_ref().is_none_or(|runtime| pins.source != runtime.sha256)
            || pins.files.keys().any(|path| !relative(path))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(pins)
    };
    let pins = match load() {
        Ok(pins) => pins.files,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => return Integrity::Tampered,
        Err(_) => return Integrity::CannotCheck,
    };
    match interpreter_snapshot(&dir.join(".venv")) {
        Ok(actual) if actual == pins => {
            let directory = dir.join(if cfg!(windows) { ".venv/Scripts" } else { ".venv/bin" });
            let Ok(entries) = std::fs::read_dir(directory) else { return Integrity::CannotCheck };
            for entry in entries {
                let Ok(entry) = entry else { return Integrity::CannotCheck };
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                let sensitive = [".dll", ".pyd", ".so", ".zip", "._pth", ".pth"].iter().any(|suffix| name.ends_with(suffix));
                if sensitive && !pins.keys().any(|path| path.to_ascii_lowercase().ends_with(&format!("/{name}"))) {
                    return Integrity::Tampered;
                }
            }
            Integrity::Intact
        }
        Ok(actual) => {
            if actual.len() == pins.len()
                && actual.iter().all(|(path, value)| {
                    pins.get(path).is_some_and(|expected| {
                        value == expected
                            || (value.starts_with("link:")
                                && value.rsplit_once(':').map(|(link, _)| link) == expected.rsplit_once(':').map(|(link, _)| link))
                    })
                })
            {
                Integrity::CannotCheck
            } else {
                Integrity::Tampered
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Integrity::Tampered,
        Err(_) => Integrity::CannotCheck,
    }
}

fn check_space(free: u64, needed: u64) -> Result<(), String> {
    if free < needed { Err("error.tools.space".into()) } else { Ok(()) }
}

fn stage_download(source: &Path, target: &Path, staging: &Path) -> io::Result<()> {
    let owned = source.parent().is_some_and(|parent| {
        parent.parent() == staging.parent() && parent.file_name().is_some_and(|name| name.to_string_lossy().starts_with(".staging-"))
    });
    if owned { std::fs::rename(source, target) } else { std::fs::copy(source, target).map(|_| ()) }
}

/// `packages` contains only already checked downloads. No network or third-party build backend runs here.
pub(super) fn extract(item: &Item, source: &Path, packages: &BTreeMap<String, PathBuf>, dest: &Path) -> Result<(), String> {
    let runtime = item.runtime.as_ref().ok_or("error.tools.unknown")?;
    runtime.validate()?;
    if !checked(source, &runtime.sha256, runtime.size) {
        return Err("error.tools.digest".into());
    }
    std::fs::create_dir(dest).map_err(|e| e.to_string())?;
    stage_download(source, &dest.join("source"), dest).map_err(|e| e.to_string())?;
    let source = &dest.join("source");
    let bundle = bundles().get(&item.id);
    if let Some(bundle) = bundle {
        for asset in &bundle.assets {
            let path = packages.get(&asset.path).ok_or("error.tools.digest")?;
            if !checked(path, &asset.sha256, asset.size) {
                return Err("error.tools.digest".into());
            }
            let target = dest.join(&asset.path);
            std::fs::create_dir_all(target.parent().ok_or("error.tools.digest")?).map_err(|e| e.to_string())?;
            stage_download(path, &target, dest).map_err(|e| e.to_string())?;
            if asset.path == "library/antlr4.tar.gz" {
                unpack_tar(&target, &dest.join("library/antlr4"))?;
            }
            if let Some(folder) = asset.path.strip_suffix(".zip") {
                unpack(&target, &dest.join(folder))?;
            }
        }
        if bundle.unpack {
            unpack(source, &dest.join("payload"))?;
        }
    }
    if runtime.kind == Kind::Python {
        let cli = uv().ok_or("error.tools.uvMissing")?;
        let platform = bundle.and_then(|bundle| bundle.platforms.get(std::env::consts::OS)).ok_or("error.tools.host")?;
        let lock = locks::get(&platform.lock).ok_or("error.tools.digest")?;
        let wheelhouse = dest.join("wheels");
        std::fs::create_dir(&wheelhouse).map_err(|e| e.to_string())?;
        for wheel in &platform.wheels {
            let file = packages.get(&wheel.path).ok_or("error.tools.digest")?;
            if !checked(file, &wheel.sha256, wheel.size) {
                return Err("error.tools.digest".into());
            }
            stage_download(file, &wheelhouse.join(&wheel.path), dest).map_err(|e| e.to_string())?;
        }
        std::fs::write(dest.join("requirements.txt"), lock).map_err(|e| e.to_string())?;
        for args in uv_arguments(dest, runtime)? {
            run_uv(&cli, dest, &args)?;
        }
        // These uv-created distutils hooks serve package builds; runtimes never build packages or need startup hooks.
        let site = dest.join(if cfg!(windows) { ".venv/Lib/site-packages" } else { ".venv/lib/python3.12/site-packages" });
        for name in ["_virtualenv.py", "_virtualenv.pth"] {
            match std::fs::remove_file(site.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        if let Ok(entries) = std::fs::read_dir(site.join("__pycache__")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let name = if cfg!(windows) { name.to_ascii_lowercase() } else { name };
                if name.starts_with("_virtualenv.") && name.ends_with(".pyc") {
                    std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                }
            }
        }
        std::fs::remove_dir_all(wheelhouse).map_err(|e| e.to_string())?;
        record_interpreter(dest, item).map_err(|e| e.to_string())?;
        std::fs::write(dest.join("run.py"), include_str!("../runtime-launch.py")).map_err(|e| e.to_string())?;
    } else if !bundle.is_some_and(|bundle| bundle.unpack) {
        let target = command(dest, runtime);
        std::fs::create_dir_all(target.parent().ok_or("error.tools.digest")?).map_err(|e| e.to_string())?;
        std::fs::copy(source, target).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir(dest.join("instructions")).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("instructions/SKILL.md"), instructions(item)).map_err(|e| e.to_string())?;
    if !verify(dest, item) {
        return Err("error.tools.digest".into());
    }
    Ok(())
}

fn unpack(source: &Path, dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(source).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if archive.len() > 20_000 {
        return Err("error.tools.size".into());
    }
    let mut remaining = FILE_LIMIT;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = file.name().map_err(|e| e.to_string())?.trim_end_matches('/').to_string();
        if !relative(&name) || !names.insert(name.to_lowercase()) || file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("error.tools.digest".into());
        }
        remaining = remaining.checked_sub(file.size()).ok_or("error.tools.size")?;
        let target = dest.join(name);
        if file.is_dir() {
            std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
            continue;
        }
        std::fs::create_dir_all(target.parent().ok_or("error.tools.digest")?).map_err(|e| e.to_string())?;
        let mut output = std::fs::File::create(target).map_err(|e| e.to_string())?;
        let limit = file.size();
        let copied = io::copy(&mut file.by_ref().take(limit + 1), &mut output).map_err(|e| e.to_string())?;
        if copied != limit {
            return Err("error.tools.digest".into());
        }
    }
    Ok(())
}

fn unpack_tar(source: &Path, dest: &Path) -> Result<(), String> {
    let input = flate2::read::GzDecoder::new(std::fs::File::open(source).map_err(|e| e.to_string())?);
    let mut archive = tar::Archive::new(input);
    let mut remaining = FILE_LIMIT;
    let mut names = BTreeSet::new();
    for file in archive.entries().map_err(|e| e.to_string())? {
        let mut file = file.map_err(|e| e.to_string())?;
        let path = file.path().map_err(|e| e.to_string())?.to_string_lossy().trim_end_matches('/').to_string();
        if !relative(&path) || !names.insert(path.to_lowercase()) || names.len() > 20_000 {
            return Err("error.tools.digest".into());
        }
        let target = dest.join(path);
        if file.header().entry_type().is_dir() {
            std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
            continue;
        }
        if !file.header().entry_type().is_file() {
            return Err("error.tools.digest".into());
        }
        remaining = remaining.checked_sub(file.size()).ok_or("error.tools.size")?;
        std::fs::create_dir_all(target.parent().ok_or("error.tools.digest")?).map_err(|e| e.to_string())?;
        let size = file.size();
        let mut output = std::fs::File::create(target).map_err(|e| e.to_string())?;
        if io::copy(&mut file.by_ref().take(size + 1), &mut output).map_err(|e| e.to_string())? != size {
            return Err("error.tools.digest".into());
        }
    }
    Ok(())
}

fn runtime_cache_key(item: &Item) -> String {
    crate::hex(&Sha256::digest(serde_json::to_vec(item).expect("catalog item")))
}

type VerifiedCache = BTreeMap<PathBuf, (String, String)>;
fn verified_cache() -> &'static std::sync::Mutex<VerifiedCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<VerifiedCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn runtime_stamp(dir: &Path) -> io::Result<String> {
    let mut hash = Sha256::new();
    for path in plain_paths(dir)? {
        hash.update(path.strip_prefix(dir).map_err(io::Error::other)?.to_string_lossy().as_bytes());
        let meta = std::fs::symlink_metadata(&path)?;
        hash.update(meta.len().to_le_bytes());
        if meta.is_file() || (meta.file_type().is_symlink() && path.is_file()) {
            let file = std::fs::File::open(&path)?;
            // Includes file identity, size, modification AND change time, including restored timestamps.
            hash.update(faber_winproc::rewind::capture_metadata(&file)?.0);
        }
        if meta.file_type().is_symlink() {
            hash.update(std::fs::read_link(path)?.to_string_lossy().as_bytes());
        }
    }
    Ok(crate::hex(&hash.finalize()))
}

pub(super) fn verify_state(dir: &Path, item: &Item) -> Integrity {
    let before = match runtime_stamp(dir) {
        Ok(stamp) => stamp,
        Err(error) => return content_integrity(Err(error)),
    };
    if item.runtime.as_ref().is_some_and(|runtime| runtime.kind == Kind::Python) && interpreter_intact(dir, item) == Integrity::CannotCheck
    {
        return Integrity::CannotCheck;
    }
    let contents = content_integrity(verify_contents(dir, item));
    if contents != Integrity::Intact {
        return contents;
    }
    match runtime_stamp(dir) {
        Ok(after) if before == after => {
            let mut cache = verified_cache().lock().unwrap_or_else(|error| error.into_inner());
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(dir.to_owned(), (runtime_cache_key(item), after));
            Integrity::Intact
        }
        _ => Integrity::CannotCheck,
    }
}

pub(super) fn verify(dir: &Path, item: &Item) -> bool {
    verify_state(dir, item) == Integrity::Intact
}

pub(super) fn verify_cached(dir: &Path, item: &Item) -> bool {
    let Some(runtime) = &item.runtime else { return false };
    let stamp = runtime_stamp(dir).ok();
    let cached = stamp.as_ref().is_some_and(|stamp| {
        verified_cache()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(dir)
            .is_some_and(|(pin, previous)| pin == &runtime_cache_key(item) && stamp == previous)
    });
    if !cached {
        return verify(dir, item);
    }
    if runtime.kind == Kind::Python {
        interpreter_intact(dir, item) == Integrity::Intact
            && std::fs::read_to_string(dir.join("run.py")).ok().as_deref() == Some(include_str!("../runtime-launch.py"))
    } else if command(dir, runtime).extension().is_some_and(|ext| ext == "exe") {
        // Executable payloads were authenticated against the checked archive; a changed stamp invalidates the cache.
        root_paths_intact(dir, item).unwrap_or(false)
    } else {
        true
    }
}

fn root_paths_intact(dir: &Path, item: &Item) -> io::Result<bool> {
    let Some(runtime) = &item.runtime else { return Ok(false) };
    let mut files = BTreeSet::from(["source".to_owned(), "instructions/SKILL.md".to_owned()]);
    let mut trees = Vec::new();
    if runtime.kind == Kind::Python {
        files.extend(["run.py".into(), "requirements.txt".into(), "interpreter.json".into()]);
        trees.push(".venv".to_owned());
    } else if !bundles().get(&item.id).is_some_and(|bundle| bundle.unpack) {
        files.insert(runtime.command.clone());
    }
    if let Some(bundle) = bundles().get(&item.id) {
        for asset in &bundle.assets {
            files.insert(asset.path.clone());
            if asset.path == "library/antlr4.tar.gz" {
                trees.push("library/antlr4".into());
            }
            if let Some(folder) = asset.path.strip_suffix(".zip") {
                trees.push(folder.to_owned());
            }
        }
        if bundle.unpack {
            trees.push("payload".into());
        }
    }
    for path in plain_paths(dir)? {
        if path == dir {
            continue;
        }
        let relative = path.strip_prefix(dir).map_err(io::Error::other)?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if !files.contains(&name)
            && !trees.iter().any(|tree| name == *tree || name.starts_with(&format!("{tree}/")))
            && !(std::fs::symlink_metadata(&path)?.is_dir()
                && files.iter().chain(trees.iter()).any(|file| file.starts_with(&format!("{name}/"))))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn verify_contents(dir: &Path, item: &Item) -> io::Result<bool> {
    let reject = |part: &str| {
        eprintln!("tools: runtime {} failed integrity: {part}", item.id);
        Ok(false)
    };
    let Some(runtime) = &item.runtime else { return Ok(false) };
    if !root_paths_intact(dir, item)? {
        return reject("unpinned root");
    }
    if !std::fs::symlink_metadata(dir)?.is_dir()
        || !check_file(&dir.join("source"), &runtime.sha256, runtime.size)?
        || !check_text(&dir.join("instructions/SKILL.md"), &instructions(item))?
        || !std::fs::metadata(command(dir, runtime))?.is_file()
    {
        return reject("source, instructions or command");
    }
    if let Some(bundle) = bundles().get(&item.id) {
        if bundle.entry != runtime.command {
            return reject("command descriptor");
        }
        for asset in &bundle.assets {
            if !check_file(&dir.join(&asset.path), &asset.sha256, asset.size)? {
                return reject("model assets");
            }
            if asset.path == "library/antlr4.tar.gz" && !tar_intact(&dir.join(&asset.path), &dir.join("library/antlr4"))? {
                return reject("import library");
            }
            if let Some(folder) = asset.path.strip_suffix(".zip")
                && !unpacked_intact(&dir.join(&asset.path), &dir.join(folder))?
            {
                return reject("model archive");
            }
        }
        if bundle.unpack && !unpacked_intact(&dir.join("source"), &dir.join("payload"))? {
            return reject("extracted source");
        }
        if runtime.kind == Kind::Python {
            match interpreter_intact(dir, item) {
                Integrity::Intact => {}
                Integrity::Tampered => return reject("interpreter"),
                Integrity::CannotCheck => return Err(io::ErrorKind::WouldBlock.into()),
            }
            let Some(platform) = bundle.platforms.get(std::env::consts::OS) else { return Ok(false) };
            let Some(lock) = locks::get(&platform.lock) else { return Ok(false) };
            if !check_text(&dir.join("requirements.txt"), lock)? || !check_text(&dir.join("run.py"), include_str!("../runtime-launch.py"))?
            {
                return reject("launcher or lock");
            }
            if !site_packages_intact(dir, platform)? {
                return reject("installed package files");
            }
            let mut size = 0u64;
            for path in plain_paths(&dir.join(".venv"))? {
                size = size.saturating_add(std::fs::symlink_metadata(path)?.len());
            }
            return Ok(size <= ENV_LIMIT);
        }
    }
    if runtime.kind == Kind::File && !bundles().get(&item.id).is_some_and(|bundle| bundle.unpack) {
        return check_file(&command(dir, runtime), &runtime.sha256, runtime.size);
    }
    Ok(true)
}

fn environment_digest(root: &Path) -> io::Result<String> {
    fn walk(path: &Path, root: &Path, files: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
        if depth > 64 || !std::fs::symlink_metadata(path)?.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid environment folder"));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.file_type()?;
            if entry.path().extension().is_some_and(|ext| ext == "pyc") {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Unexpected bytecode cache"));
            }
            if meta.is_symlink()
                && root.file_name().is_some_and(|name| name == ".venv")
                && entry.path() == root.join("lib64")
                && std::fs::read_link(entry.path())? == Path::new("lib")
            {
                continue;
            }
            if meta.is_dir() {
                walk(&entry.path(), root, files, depth + 1)?;
            } else if meta.is_file() {
                files.push(entry.path().strip_prefix(root).map_err(io::Error::other)?.to_path_buf());
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Linked environment file"));
            }
            if files.len() > 100_000 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Environment has too many files"));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files, 0)?;
    let mut files: Vec<_> = files.into_iter().map(|path| path.to_string_lossy().replace('\\', "/")).collect();
    files.sort();
    let mut hash = Sha256::new();
    let mut remaining = ENV_LIMIT;
    for relative in files {
        hash.update(relative.as_bytes());
        let mut file = std::fs::File::open(root.join(&relative))?;
        hash.update([0]);
        hash.update(file.metadata()?.len().to_le_bytes());
        remaining = remaining
            .checked_sub(file.metadata()?.len())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Environment too large"))?;
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
    }
    Ok(crate::hex(&hash.finalize()))
}

fn site_packages_intact(root: &Path, platform: &Platform) -> io::Result<bool> {
    let site = root.join(if cfg!(windows) { ".venv/Lib/site-packages" } else { ".venv/lib/python3.12/site-packages" });
    let mut expected = BTreeMap::new();
    for wheel in &platform.wheels {
        let Some(files) = wheel_files(&wheel.sha256).filter(|files| !files.is_empty()) else {
            return Ok(false);
        };
        for (path, hash) in files {
            if !relative(path) || !pin(hash) || expected.insert(path, hash).is_some_and(|prior| prior != hash) {
                return Ok(false);
            }
        }
    }
    package_files_intact(&site, &expected)
}

fn package_files_intact(site: &Path, expected: &BTreeMap<&str, &str>) -> io::Result<bool> {
    for (path, hash) in expected {
        let file = site.join(path);
        let meta = std::fs::symlink_metadata(&file)?;
        if !check_file(&file, hash, meta.len())? {
            eprintln!("tools: package file differs: {path}");
            return Ok(false);
        }
    }
    fn extras(path: &Path, root: &Path, expected: &BTreeMap<&str, &str>, depth: usize) -> io::Result<()> {
        if depth > 64 || !std::fs::symlink_metadata(path)?.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid package directory"));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                extras(&entry.path(), root, expected, depth + 1)?;
            } else if entry.file_type()?.is_file() {
                let relative = entry.path().strip_prefix(root).map_err(io::Error::other)?.to_string_lossy().replace('\\', "/");
                let generated = [
                    ".dist-info/RECORD",
                    ".dist-info/INSTALLER",
                    ".dist-info/REQUESTED",
                    ".dist-info/direct_url.json",
                    ".dist-info/uv_cache.json",
                ]
                .iter()
                .any(|suffix| relative.ends_with(suffix))
                    || relative == ".gitignore";
                if !generated && !expected.contains_key(relative.as_str()) {
                    eprintln!("tools: unpinned package file: {relative}");
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Unpinned executable package file"));
                }
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Linked package file"));
            }
        }
        Ok(())
    }
    extras(site, site, expected, 0)?;
    Ok(true)
}

fn plain_paths(root: &Path) -> io::Result<Vec<PathBuf>> {
    fn walk(path: &Path, root: &Path, files: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
        if depth > 64 || files.len() > 120_000 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Runtime has too many paths"));
        }
        let meta = std::fs::symlink_metadata(path)?;
        let venv = if root.file_name().is_some_and(|name| name == ".venv") { root.to_path_buf() } else { root.join(".venv") };
        if meta.file_type().is_symlink()
            && ((path == venv.join("lib64") && std::fs::read_link(path)? == Path::new("lib"))
                || (path.parent() == Some(venv.join("bin").as_path())
                    && path.file_name().is_some_and(|name| matches!(name.to_str(), Some("python" | "python3" | "python3.12")))))
        {
            files.push(path.to_owned());
            return Ok(());
        }
        if meta.file_type().is_symlink() || path.extension().is_some_and(|ext| ext == "pyc") {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Unexpected runtime link or bytecode"));
        }
        if !meta.is_dir() && !meta.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid runtime path"));
        }
        files.push(path.to_owned());
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(&entry?.path(), root, files, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files, 0)?;
    files.sort();
    Ok(files)
}

fn unpacked_intact(source: &Path, payload: &Path) -> io::Result<bool> {
    let calculate = || -> io::Result<String> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(source)?)?;
        let mut names =
            zip.file_names().map(|name| name.map(|name| name.into_owned()).map_err(io::Error::from)).collect::<io::Result<Vec<_>>>()?;
        names.retain(|name| !name.ends_with('/'));
        names.sort();
        let mut hash = Sha256::new();
        for name in names {
            let mut file = zip.by_name(&name)?;
            hash.update(name.as_bytes());
            hash.update([0]);
            hash.update(file.size().to_le_bytes());
            let mut buffer = [0; 64 * 1024];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
        }
        Ok(crate::hex(&hash.finalize()))
    };
    Ok(calculate()? == environment_digest(payload)?)
}

fn tar_intact(source: &Path, payload: &Path) -> io::Result<bool> {
    let check = || -> io::Result<bool> {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(source)?));
        let mut expected = BTreeMap::new();
        for file in archive.entries()? {
            let mut file = file?;
            if file.header().entry_type().is_dir() {
                continue;
            }
            if !file.header().entry_type().is_file() {
                return Ok(false);
            }
            let relative = file.path()?.to_string_lossy().to_string();
            if !self::relative(&relative) {
                return Ok(false);
            }
            let mut hash = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            expected.insert(payload.join(relative), (crate::hex(&hash.finalize()), file.size()));
        }
        let files = plain_paths(payload)?;
        let mut count = 0;
        for path in files {
            count += usize::from(std::fs::symlink_metadata(path)?.is_file());
        }
        if count != expected.len() {
            return Ok(false);
        }
        for (path, (hash, size)) in expected {
            if !check_file(&path, &hash, size)? {
                return Ok(false);
            }
        }
        Ok(true)
    };
    check()
}

#[cfg(test)]
type TestUv = Box<dyn FnMut(&std::process::Command) -> Result<(), String>>;
#[cfg(test)]
thread_local! {
    static HASHED_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TEST_UV: std::cell::RefCell<Option<TestUv>> = const { std::cell::RefCell::new(None) };
    static TEST_HASH_ERROR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn fixture(id: &str, payload: &[u8], vram: Option<u64>) -> Item {
    let mut item: Item = serde_json::from_value(serde_json::json!({
        "id":id,"job":"audio","tier":"recommended","license":"MIT","repo":"fixture/audio",
        "commit":"a".repeat(40),"path":"","digest":"",
        "runtime":{"kind":"file","source":"https://github.com/fixture/audio/releases/download/v1/audio.dat",
        "sha256":crate::hex(&Sha256::digest(payload)),"size":payload.len(),"weights_license":"MIT",
        "capability":"qa","command":"payload/audio.dat","min_vram_mb":vram}
    }))
    .unwrap();
    let text = instructions(&item);
    let mut hash = Sha256::new();
    hash.update(b"SKILL.md\0");
    hash.update(text.len().to_string().as_bytes());
    hash.update([0]);
    hash.update(text.as_bytes());
    item.digest = crate::hex(&hash.finalize());
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_uv_deadline_kills_and_reaps_the_spawned_child() {
        struct Stuck {
            stopped: bool,
        }
        impl UvChild for Stuck {
            fn done(&mut self) -> io::Result<Option<bool>> {
                Ok(None)
            }
            fn stop(&mut self) {
                self.stopped = true;
            }
        }
        assert_eq!(UV_TIMEOUT.as_secs(), 300);
        let mut child = Stuck { stopped: false };
        assert!(wait_uv(&mut child, std::time::Duration::ZERO).is_err());
        assert!(child.stopped);
    }

    #[test]
    fn runtime_root_and_weight_extras_are_rejected_and_launch_cache_detects_same_size_changes() {
        let temp = tempfile::tempdir().unwrap();
        let item = fixture("review-audio", b"audio", None);
        let source = temp.path().join("download");
        std::fs::write(&source, b"audio").unwrap();
        let root = temp.path().join("runtime");
        extract(&item, &source, &BTreeMap::new(), &root).unwrap();
        assert!(verify(&root, &item));
        HASHED_BYTES.with(|bytes| bytes.set(0));
        assert!(verify_cached(&root, &item));
        assert_eq!(HASHED_BYTES.with(|bytes| bytes.get()), 0, "cached launch must not rehash data");
        let payload = root.join("payload/audio.dat");
        let time = std::fs::metadata(&payload).unwrap().modified().unwrap();
        std::fs::write(&payload, b"evil!").unwrap();
        std::fs::File::options().write(true).open(&payload).unwrap().set_times(std::fs::FileTimes::new().set_modified(time)).unwrap();
        assert!(!verify_cached(&root, &item));
        std::fs::write(&payload, b"audio").unwrap();
        for path in ["extra.py", "weights/extra.bin", "library/antlr4/antlr4-python3-runtime-evil/src/faster_whisper/__init__.py"] {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "hostile").unwrap();
            assert!(!verify(&root, &item), "{path}");
            std::fs::remove_file(file).unwrap();
        }
    }

    #[test]
    fn runtime_downloads_move_only_from_owned_staging_and_free_space_is_checked() {
        let temp = tempfile::tempdir().unwrap();
        let downloads = temp.path().join(".staging-download");
        let staging = temp.path().join(".staging-install");
        std::fs::create_dir(&downloads).unwrap();
        std::fs::create_dir(&staging).unwrap();
        let source = downloads.join("wheel");
        std::fs::write(&source, "wheel").unwrap();
        stage_download(&source, &staging.join("wheel"), &staging).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(staging.join("wheel")).unwrap(), b"wheel");
        let external = temp.path().join("source");
        std::fs::write(&external, "source").unwrap();
        stage_download(&external, &staging.join("source"), &staging).unwrap();
        assert!(external.exists());
        assert_eq!(check_space(9, 10), Err("error.tools.space".into()));
        assert!(check_space(10, 10).is_ok());
    }

    #[test]
    fn runtime_zip_verification_sorts_portable_names_not_path_components() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
        for name in ["a.txt", "a/b.txt"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"fixture").unwrap();
        }
        zip.finish().unwrap();
        let payload = temp.path().join("payload");
        unpack(&source, &payload).unwrap();
        assert!(unpacked_intact(&source, &payload).unwrap());
        std::fs::write(payload.join("a.txt"), "changed").unwrap();
        assert!(!unpacked_intact(&source, &payload).unwrap());
    }

    #[test]
    fn runtime_interpreter_uses_install_pins_without_uv() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("tools/runtimes/local-python");
        let directory = if cfg!(windows) { "Scripts" } else { "bin" };
        let name = if cfg!(windows) { "python.exe" } else { "python" };
        let create = |root: &Path| {
            std::fs::create_dir_all(root.join(".venv").join(directory)).unwrap();
            std::fs::write(root.join(".venv/pyvenv.cfg"), "trusted uv configuration").unwrap();
            std::fs::write(root.join(".venv").join(directory).join(name), "trusted interpreter").unwrap();
        };
        create(&root);
        let mut item = fixture("local-python", b"python", None);
        item.runtime.as_mut().unwrap().kind = Kind::Python;
        record_interpreter(&root, &item).unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Intact);
        std::fs::write(root.join(".venv").join(directory).join(name), "tampered interpreter").unwrap();
        std::fs::write(root.join("environment.sha256"), "forged receipt").unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered);
        create(&root);
        std::fs::write(root.join(".venv/pyvenv.cfg"), "tampered configuration").unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered);
        create(&root);
        for extra in ["python._pth", "PYTHON._PTH", "PYTHON312.ZIP"] {
            let path = root.join(".venv").join(directory).join(extra);
            std::fs::write(&path, "untrusted startup file").unwrap();
            assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered, "{extra}");
            std::fs::remove_file(path).unwrap();
        }
        std::fs::write(root.join(".venv").join(directory).join(name), "forged interpreter").unwrap();
        let forged = interpreter_snapshot(&root.join(".venv")).unwrap();
        std::fs::write(root.join("interpreter.json"), serde_json::to_vec(&forged).unwrap()).unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered);
        let bytes = serde_json::to_vec(&InterpreterPins {
            id: item.id.clone(),
            commit: item.commit.clone(),
            source: item.runtime.as_ref().unwrap().sha256.clone(),
            files: forged,
        })
        .unwrap();
        std::fs::write(root.join("interpreter.json"), crate::hex(&Sha256::digest(&bytes))).unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::CannotCheck);
        std::fs::write(root.join("interpreter.json"), vec![b'a'; 4096]).unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered);
        TEST_UV.with(|test| test.borrow_mut().take());
    }

    #[test]
    fn runtime_package_pins_reject_forged_seals_and_bytecode() {
        let temp = tempfile::tempdir().unwrap();
        let site = temp.path().join("site");
        std::fs::create_dir(&site).unwrap();
        let code = site.join("module.py");
        std::fs::write(&code, b"original").unwrap();
        let hash = crate::hex(&Sha256::digest(b"original"));
        let expected = BTreeMap::from([("module.py", hash.as_str())]);
        assert_eq!(content_integrity(package_files_intact(&site, &expected)), Integrity::Intact);
        std::fs::write(&code, b"tampered").unwrap();
        std::fs::write(temp.path().join("module.py.sha256"), "forged local receipt").unwrap();
        assert_eq!(content_integrity(package_files_intact(&site, &expected)), Integrity::Tampered);
        std::fs::write(&code, b"original").unwrap();
        std::fs::remove_file(temp.path().join("module.py.sha256")).unwrap();
        std::fs::write(site.join("module.pyc"), b"untrusted bytecode").unwrap();
        assert_eq!(content_integrity(package_files_intact(&site, &expected)), Integrity::Tampered);
        assert!(plain_paths(temp.path()).is_err());
        std::fs::remove_file(site.join("module.pyc")).unwrap();
        std::fs::write(site.join("startup.pth"), b"import something").unwrap();
        assert_eq!(content_integrity(package_files_intact(&site, &expected)), Integrity::Tampered);
    }

    #[test]
    fn runtime_digest_failure_is_atomic_and_the_journal_round_trip_keeps_switches() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"local fixture audio";
        let item = fixture("local-audio", bytes, None);
        let source = temp.path().join("download");
        std::fs::write(&source, vec![0; bytes.len()]).unwrap();
        let store = Store::with_catalog(temp.path(), vec![item.clone()]);
        assert_eq!(store.install_from(&item, &source).unwrap_err(), "error.tools.digest");
        assert!(store.installed().is_empty());
        assert!(!store.dir(&item).exists());
        assert!(
            !std::fs::read_dir(&store.root).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().starts_with(".staging-"))
        );
        std::fs::write(&source, bytes).unwrap();
        store.install_from(&item, &source).unwrap();
        assert!(store.enabled(&item.id));
        store.set_enabled(&item.id, false).unwrap();
        assert!(!store.enabled(&item.id));
        store.set_enabled(&item.id, true).unwrap();
        assert!(store.enabled(&item.id));
        store.remove(&item.id).unwrap();
        assert!(store.installed().is_empty() && !store.dir(&item).exists());
        store.restore(&item.id).unwrap();
        assert!(store.enabled(&item.id));
        store.set_enabled(&item.id, false).unwrap();
        store.remove(&item.id).unwrap();
        store.restore(&item.id).unwrap();
        assert!(!store.enabled(&item.id));
        store.remove(&item.id).unwrap();
        store.clear_removed().unwrap();
        assert!(!store.dir(&item).exists() && !store.disabled().join(&item.id).exists() && !store.has_removed_copy(&item.id));
    }

    #[test]
    fn runtime_agent_mutations_refuse_a_linked_runtime_parent() {
        let temp = tempfile::tempdir().unwrap();
        let item = fixture("local-audio", b"audio", None);
        let source = temp.path().join("download");
        std::fs::write(&source, b"audio").unwrap();
        let store = Store::with_catalog(temp.path(), vec![item.clone()]);
        store.install_from(&item, &source).unwrap();
        store.wait_prepared_for_test();
        let parent = store.root.join("runtimes");
        let outside = temp.path().join("outside");
        super::super::retry_busy(|| std::fs::rename(&parent, &outside)).unwrap();
        #[cfg(windows)]
        junction::create(&outside, &parent).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &parent).unwrap();
        assert!(store.set_enabled_for_agent(&item.id, false).is_err());
        assert!(store.remove_for_agent(&item.id).is_err());
        assert!(outside.join(&item.id).join("payload/audio.dat").is_file());
        assert!(!store.disabled().join(&item.id).exists());
        assert!(!store.removed().join(&item.id).exists());
        #[cfg(windows)]
        junction::delete(&parent).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(&parent).unwrap();
    }

    #[test]
    fn runtime_metadata_refuses_unknown_missing_and_unbounded_fields() {
        let item = fixture("local-audio", b"audio", None);
        let runtime = item.runtime.unwrap();
        let original = serde_json::to_value(&runtime).unwrap();
        for missing in ["sha256", "size"] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(missing);
            assert!(serde_json::from_value::<Runtime>(value).is_err());
        }
        let mut unknown = original;
        unknown["extra"] = true.into();
        assert!(serde_json::from_value::<Runtime>(unknown).is_err());
        let mut invalid = runtime.clone();
        invalid.size = FILE_LIMIT + 1;
        assert!(invalid.validate().is_err());
        invalid = runtime;
        invalid.command = "../escape".into();
        assert!(invalid.validate().is_err());
        let item = serde_json::to_value(fixture("local-audio", b"audio", None)).unwrap();
        for field in ["sha256", "size", "unexpected"] {
            let mut value = item.clone();
            let descriptor = value["runtime"].as_object_mut().unwrap();
            if field == "unexpected" {
                descriptor.insert(field.into(), true.into());
            } else {
                descriptor.remove(field);
            }
            let catalog = serde_json::json!({"skills":[],"runtimes":[value]});
            assert!(serde_json::from_value::<super::super::CatalogFile>(catalog).is_err(), "{field}");
        }
        let mut unknown = item;
        unknown["unexpected"] = true.into();
        assert!(serde_json::from_value::<super::super::CatalogFile>(serde_json::json!({"skills":[],"runtimes":[unknown]})).is_err());
    }

    #[test]
    fn runtime_uv_arguments_are_passed_to_the_fake_executable_without_a_shell() {
        let temp = tempfile::tempdir().unwrap();
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let received = calls.clone();
        TEST_UV.with(|test| {
            *test.borrow_mut() = Some(Box::new(move |process| {
                assert_eq!(process.get_program(), std::ffi::OsStr::new("C:/fake/uv.exe"));
                received.borrow_mut().push((
                    process.get_current_dir().unwrap().to_path_buf(),
                    process.get_args().map(|arg| arg.to_str().unwrap().to_owned()).collect(),
                ));
                Ok(())
            }))
        });
        let cli = uv().unwrap();
        let root = temp.path().join("an item with spaces");
        let mut runtime = fixture("local-python", b"python", None).runtime.unwrap();
        runtime.kind = Kind::Python;
        let arguments = uv_arguments(&root, &runtime).unwrap();
        let hostile = vec!["pip".to_owned(), "C:/spaces & $(injected)/input".to_owned()];
        let process = uv_command(&cli, &root, &hostile);
        assert_eq!(process.get_program(), std::ffi::OsStr::new(&cli.command));
        assert_eq!(process.get_args().collect::<Vec<_>>(), hostile.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>());
        assert_eq!(process.get_current_dir(), Some(root.as_path()));
        assert!(process.get_envs().all(|(key, _)| {
            let key = key.to_string_lossy();
            [
                "PATH",
                "SystemRoot",
                "TEMP",
                "TMP",
                "USERPROFILE",
                "HOME",
                "LOCALAPPDATA",
                "APPDATA",
                "UV_NO_PROGRESS",
                "UV_NO_CONFIG",
                "UV_NO_CACHE",
            ]
            .contains(&key.as_ref())
        }));
        for args in &arguments {
            run_uv(&cli, &root, args).unwrap();
        }
        assert_eq!(cli.command, "C:/fake/uv.exe");
        assert!(cli.prefix.is_empty());
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(calls.borrow()[0], (root.clone(), arguments[0].clone()));
        assert_eq!(calls.borrow()[1], (root, arguments[1].clone()));
        assert_eq!(arguments[0][0], "venv");
        for required in ["--offline", "--no-build", "--require-hashes", "-r", "requirements.txt"] {
            assert!(arguments[1].iter().any(|arg| arg == required));
        }
        TEST_UV.with(|test| test.borrow_mut().take());
    }

    #[test]
    fn every_runtime_bundle_is_pinned_and_uses_a_hashed_offline_lock() {
        assert_eq!(super::super::catalog().iter().filter(|item| item.runtime.is_some()).count(), 11);
        for item in super::super::catalog().iter().filter(|item| item.runtime.is_some()) {
            let runtime = item.runtime.as_ref().unwrap();
            runtime.validate().unwrap();
            let bundle = &bundles()[&item.id];
            assert!(!bundle.platforms.is_empty());
            assert!(relative(&bundle.entry));
            for asset in bundle.assets.iter().chain(bundle.platforms.values().flat_map(|platform| &platform.wheels)) {
                assert!(relative(&asset.path) && pin(&asset.sha256) && asset.size > 0 && asset.size <= MODEL_LIMIT);
                crate::install::validate_url(&asset.source, &[]).unwrap();
            }
            if runtime.kind == Kind::Python {
                for (os, platform) in &bundle.platforms {
                    let lock = locks::get(&platform.lock).unwrap();
                    assert!(!platform.wheels.is_empty());
                    for wheel in &platform.wheels {
                        assert!(!(os != "windows" && wheel.path.contains("win_amd64")));
                        assert!(!(os != "linux" && wheel.path.contains("manylinux")));
                        assert!(!(os != "macos" && wheel.path.contains("macosx")));
                        assert!(lock.contains(&format!("--hash=sha256:{}", wheel.sha256)), "{} {}", item.id, wheel.path);
                        assert!(wheel_index().contains_key(wheel.sha256.as_str()), "{} has immutable installed-file pins", wheel.path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod review_tests;
