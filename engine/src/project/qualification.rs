//! The only thing that opens a role gate: a private, fresh receipt tied to the exact CLI version string and the
//! SHA-256 of the exact executable. A parameter, a manifest, a fixture or an observation file never opens it, and a
//! changed binary or version closes it again. The manual qualification run writes the receipt after its live probes
//! pass (`testkit/examples/project-qualification.rs`); `policy::admit` reads it.
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const SCHEMA: u32 = 1;
/// A new qualification run is needed after a day; a CLI update is caught earlier by the version/hash binding.
/// An AI default, not an owner number: the owner never fixed a lifetime.
pub const MAX_AGE_SECS: u64 = 24 * 3600;
/// Every probe the receipt must record as passed. A receipt with one missing or false never opens a gate.
pub const CHECKS: [&str; 5] = ["loadout", "native", "stop", "exposure", "billing"];
const MAX_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u32,
    pub vendor: Vendor,
    /// The exact `--version` output line.
    pub version: String,
    /// Lowercase SHA-256 of the resolved executable file.
    pub binary_hash: String,
    pub issued_at: u64,
    pub checks: BTreeMap<String, bool>,
}

/// What the installed CLI is right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub version: String,
    pub binary_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    Missing,
    /// Not a plain regular file in the private folder, too big, or readable by other users.
    Unsafe,
    Malformed,
    Vendor,
    Incomplete,
    Stale,
    Version,
    Binary,
    Unobserved,
}

static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The engine fixes the receipt folder once, from its own private data folder. No caller can point it elsewhere.
pub(crate) fn use_dir(data_dir: &Path) {
    let _ = DIR.set(data_dir.join("qualification"));
}

fn file(dir: &Path, vendor: Vendor) -> PathBuf {
    dir.join(format!("{}.json", vendor.name()))
}

pub fn fingerprint(path: &Path) -> io::Result<String> {
    // R112 permits opening exactly the resolved program file, even under AppData or a vendor folder.
    let mut file = std::fs::File::open(std::fs::canonicalize(path)?)?;
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

/// One short printable line; anything else is not a version.
pub fn parse_version(text: &str) -> io::Result<&str> {
    let text = text.trim();
    if text.is_empty() || text.len() > 128 || !text.bytes().all(|c| c.is_ascii_graphic() || c == b' ') {
        return Err(io::Error::other("invalid version output"));
    }
    Ok(text)
}

/// Hashes the program and asks it for `--version`: no shell, no window, scrubbed environment, a deadline.
/// Script launchers (`node script.js`) are not qualified: their bytes are not the program's.
pub fn observe(command: &str, prefix: &[String]) -> io::Result<Identity> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    if !prefix.is_empty() {
        return Err(io::Error::other("script launchers are not qualified"));
    }
    let binary_hash = fingerprint(Path::new(command))?;
    let mut run = Command::new(command);
    run.arg("--version").env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    for name in ["PATH", "SystemRoot", "WINDIR", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "HOME", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            run.env(name, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        run.creation_flags(0x0800_0000);
    }
    let mut child = run.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("version probe timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut text = String::new();
    child.stdout.take().ok_or_else(|| io::Error::other("no version output"))?.take(512).read_to_string(&mut text)?;
    if !status.success() {
        return Err(io::Error::other("version probe failed"));
    }
    Ok(Identity { version: parse_version(&text)?.to_string(), binary_hash })
}

fn read(dir: &Path, vendor: Vendor) -> Result<Receipt, Reject> {
    let path = file(dir, vendor);
    let meta =
        std::fs::symlink_metadata(&path).map_err(|e| if e.kind() == io::ErrorKind::NotFound { Reject::Missing } else { Reject::Unsafe })?;
    if !meta.is_file() || meta.len() > MAX_BYTES || std::fs::symlink_metadata(dir).is_ok_and(|d| !d.is_dir()) {
        return Err(Reject::Unsafe);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(Reject::Unsafe);
        }
    }
    serde_json::from_slice(&std::fs::read(&path).map_err(|_| Reject::Unsafe)?).map_err(|_| Reject::Malformed)
}

fn complete(checks: &BTreeMap<String, bool>) -> bool {
    CHECKS.iter().all(|name| checks.get(*name) == Some(&true))
}

fn hash_shaped(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Opens only when the stored receipt is private, for this vendor, complete, no older than a day, not from the future,
/// and names the version and executable bytes that `observe` reports now. `observe` runs last: an unqualified
/// vendor costs a file read, not a process.
pub fn verify(dir: &Path, vendor: Vendor, now: u64, observe: &dyn Fn() -> io::Result<Identity>) -> Result<(), Reject> {
    let receipt = read(dir, vendor)?;
    if receipt.schema != SCHEMA || receipt.vendor != vendor {
        return Err(Reject::Vendor);
    }
    if !complete(&receipt.checks) || !hash_shaped(&receipt.binary_hash) || parse_version(&receipt.version).ok() != Some(&receipt.version) {
        return Err(Reject::Incomplete);
    }
    if receipt.issued_at > now || now - receipt.issued_at > MAX_AGE_SECS {
        return Err(Reject::Stale);
    }
    let actual = observe().map_err(|_| Reject::Unobserved)?;
    if actual.version != receipt.version {
        return Err(Reject::Version);
    }
    if actual.binary_hash != receipt.binary_hash {
        return Err(Reject::Binary);
    }
    Ok(())
}

/// Writes (or atomically replaces) a vendor's receipt. Refuses an incomplete set of passed probes, so a harness
/// that skipped one cannot issue it.
pub fn issue(dir: &Path, vendor: Vendor, identity: &Identity, checks: BTreeMap<String, bool>, now: u64) -> io::Result<()> {
    use std::io::Write;
    if !complete(&checks) || !hash_shaped(&identity.binary_hash) || parse_version(&identity.version)? != identity.version {
        return Err(io::Error::other("a receipt needs every probe passed and a measured identity"));
    }
    std::fs::create_dir_all(dir)?;
    let receipt = Receipt {
        schema: SCHEMA,
        vendor,
        version: identity.version.clone(),
        binary_hash: identity.binary_hash.clone(),
        issued_at: now,
        checks,
    };
    let temporary = dir.join(format!(".{}.{}.tmp", vendor.name(), uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&temporary)?;
    out.write_all(&serde_json::to_vec(&receipt).map_err(io::Error::other)?)?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&temporary, file(dir, vendor)).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// The gate `policy` consults. Opens only on Windows, where hidden owned-job process control exists, and only
/// with the engine's own receipt folder; `bin` names the program about to be launched, looked up only if a
/// receipt is worth checking.
pub(crate) fn admitted(vendor: Vendor, bin: &dyn Fn() -> Option<String>) -> Result<(), Reject> {
    if !cfg!(windows) {
        return Err(Reject::Missing);
    }
    let dir = DIR.get().ok_or(Reject::Missing)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    verify(dir, vendor, now, &|| observe(&bin().ok_or_else(|| io::Error::other("CLI not found"))?, &[]))
}
