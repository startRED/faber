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
    /// The launch policy the probes ran under (`launch_policy`). A receipt from before the field existed has none, which
    /// never matches: it asks for a new qualification run.
    #[serde(default)]
    pub launch_policy: String,
    /// What the loadout probe relied on: the launch flags that switch hooks off (Claude) and the instruction files the
    /// session declared (Codex), each by path and SHA-256.
    #[serde(default)]
    pub proof: Proof,
}

/// Where a declared instruction file came from. Anything else refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    /// `AGENTS.md` or `CLAUDE.md` at the top of the mission clone: the project's own rules.
    Clone,
    /// The user's own global instruction file.
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: String,
    pub sha256: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    /// The exact launch flags that load no hooks (Claude); empty for Codex.
    #[serde(default)]
    pub hook_flags: Vec<String>,
    #[serde(default)]
    pub instruction_sources: Vec<Source>,
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
    /// A declared instruction file that is neither the mission clone's own rules nor the user's global file, one whose
    /// bytes or set changed since the receipt was issued, or one that cannot be read safely.
    Source,
    /// A managed Claude Code policy exists, or its absence could not be proven.
    Policy,
    /// The receipt was issued under other role launch parameters than the engine uses now (or before they were
    /// recorded): the CLI must be qualified again.
    Requalify,
}

/// The launch-policy revision: a SHA-256 of the effective launch parameters of every project role of `vendor`, as the engine
/// builds them now, with fixed stand-ins for the per-turn values. Probes prove behaviour under these exact parameters, so a
/// receipt binds them; any change to a flag, a setting or a thread parameter makes every older receipt ask to requalify.
pub fn launch_policy(vendor: Vendor) -> String {
    use crate::project::Role;
    let roles = [Role::Lead, Role::Research, Role::TestProposal, Role::Writer, Role::Reviewer];
    let parameters = match vendor {
        Vendor::Claude => crate::claude::project_launch_policy(&roles),
        Vendor::Codex => crate::codex::project_launch_policy(&roles),
    };
    format!("{:x}", Sha256::digest(parameters.to_string().as_bytes()))
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

/// The largest instruction file that is hashed.
const MAX_SOURCE: u64 = 4 * 1024 * 1024;

/// Hashes one instruction file through the very handle that was checked. The last path component is never followed
/// (a link or reparse point is refused at open and again on the handle), the file must be a plain regular file, and
/// the bytes come from that handle: there is no second resolution of the path between the check and the read.
pub fn fingerprint_source(path: &Path) -> io::Result<String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a link itself instead of what it points to.
        options.custom_flags(0x0020_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let file = options.open(path)?;
    let meta = file.metadata()?;
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = meta.file_type().is_symlink();
    if linked || !meta.is_file() || meta.len() > MAX_SOURCE {
        return Err(io::Error::other("an instruction source must be a plain regular file"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 16 * 1024];
    let mut reader = file.take(MAX_SOURCE + 1);
    let mut total = 0;
    loop {
        let size = reader.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        total += size as u64;
        digest.update(&buffer[..size]);
    }
    if total > MAX_SOURCE {
        return Err(io::Error::other("an instruction source grew while it was read"));
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

/// A program file held open so that, until it is dropped, nobody can change, rename or delete it (on Windows other
/// openers may only read it; the system can still load it as an image). Its hash was read through that same handle, so
/// the bytes hashed are the bytes a process started from `path` runs while the pin is held.
pub struct Pinned {
    _file: std::fs::File,
    pub path: PathBuf,
    pub sha256: String,
    /// The `--version` the program answered, filled only by an admission whose receipt it matched.
    pub version: String,
}

/// Opens the resolved program file, denying writes and deletes on Windows, and hashes it through that handle.
pub fn pin(path: &Path) -> io::Result<Pinned> {
    let path = std::fs::canonicalize(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ only: no writer, no rename and no delete while the pin is held.
        options.share_mode(0x1);
    }
    let mut file = options.open(&path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(Pinned { _file: file, path, sha256: format!("{:x}", digest.finalize()), version: String::new() })
}

/// Hashes the program and asks it for `--version`: no shell, no window, scrubbed environment, a deadline.
/// Script launchers (`node script.js`) are not qualified: their bytes are not the program's.
pub fn observe(command: &str, prefix: &[String]) -> io::Result<Identity> {
    if !prefix.is_empty() {
        return Err(io::Error::other("script launchers are not qualified"));
    }
    let pinned = pin(Path::new(command))?;
    Ok(Identity { version: version_of(&pinned.path)?, binary_hash: pinned.sha256 })
}

/// The admission's observation: the program is pinned and hashed first, and asked for `--version` only when its hash is
/// `expected` (the receipt's). Another program never runs; it answers with its hash and no version. The pin is handed
/// back so the launch can keep the file unchanged until the started process runs (`proc::spawn_expecting`).
pub fn observe_pinned(command: &str, expected: &str) -> io::Result<(Identity, Pinned)> {
    let mut pinned = pin(Path::new(command))?;
    let version = if pinned.sha256 == expected { version_of(&pinned.path)? } else { String::new() };
    pinned.version.clone_from(&version);
    Ok((Identity { version, binary_hash: pinned.sha256.clone() }, pinned))
}

fn version_of(command: &Path) -> io::Result<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
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
    Ok(parse_version(&text)?.to_string())
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
    verify_receipt(dir, vendor, now, observe).map(|_| ())
}

/// `verify`, handing back the receipt that matched.
pub fn verify_receipt(dir: &Path, vendor: Vendor, now: u64, observe: &dyn Fn() -> io::Result<Identity>) -> Result<Receipt, Reject> {
    let receipt = read(dir, vendor)?;
    if receipt.schema != SCHEMA || receipt.vendor != vendor {
        return Err(Reject::Vendor);
    }
    if !complete(&receipt.checks)
        || !hash_shaped(&receipt.binary_hash)
        || parse_version(&receipt.version).ok() != Some(&receipt.version)
        || !proof_valid(vendor, &receipt.proof)
    {
        return Err(Reject::Incomplete);
    }
    if receipt.issued_at > now || now - receipt.issued_at > MAX_AGE_SECS {
        return Err(Reject::Stale);
    }
    if receipt.launch_policy != launch_policy(vendor) {
        return Err(Reject::Requalify);
    }
    let actual = observe().map_err(|_| Reject::Unobserved)?;
    // The bytes first: an observer that pins the program asks another program for nothing.
    if actual.binary_hash != receipt.binary_hash {
        return Err(Reject::Binary);
    }
    if actual.version != receipt.version {
        return Err(Reject::Version);
    }
    Ok(receipt)
}

/// Whether a thing the receipt relies on exists right now. Anything that cannot be inspected is `Unproven`, which
/// never counts as absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Absent,
    Present,
    Unproven,
}

pub fn path_presence(path: &str) -> Presence {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Presence::Present,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Presence::Absent,
        Err(_) => Presence::Unproven,
    }
}

#[cfg(windows)]
pub fn registry_presence(key: &str) -> Presence {
    use winreg::{RegKey, enums::*};
    let (hive, sub) = match key.split_once('\\') {
        Some(("HKLM", sub)) => (HKEY_LOCAL_MACHINE, sub),
        Some(("HKCU", sub)) => (HKEY_CURRENT_USER, sub),
        _ => return Presence::Unproven,
    };
    match RegKey::predef(hive).open_subkey_with_flags(sub, KEY_READ) {
        Ok(_) => Presence::Present,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Presence::Absent,
        Err(_) => Presence::Unproven,
    }
}

#[cfg(not(windows))]
pub fn registry_presence(_key: &str) -> Presence {
    Presence::Unproven
}

/// The two registry keys of Claude Code managed settings.
pub const MANAGED_POLICY_KEYS: [&str; 2] = [r"HKLM\SOFTWARE\Policies\ClaudeCode", r"HKCU\SOFTWARE\Policies\ClaudeCode"];

/// Managed-policy hooks are the one hooks source the launch flags cannot switch off, so every location must be
/// confirmed absent: a location that exists, or that cannot be inspected, refuses.
pub fn managed_policy_clear(path: &dyn Fn(&str) -> Presence, registry: &dyn Fn(&str) -> Presence) -> Result<(), Reject> {
    let all_absent = MANAGED_POLICY_PATHS.iter().all(|p| path(p) == Presence::Absent)
        && MANAGED_POLICY_KEYS.iter().all(|k| registry(k) == Presence::Absent);
    if all_absent { Ok(()) } else { Err(Reject::Policy) }
}

/// Every instruction file the receipt recorded still has the recorded bytes.
pub fn sources_unchanged(recorded: &[Source]) -> Result<(), Reject> {
    for source in recorded {
        let now = fingerprint_source(Path::new(&source.path)).map_err(|_| Reject::Source)?;
        if now != source.sha256 {
            return Err(Reject::Source);
        }
    }
    Ok(())
}

fn canonical(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// The instruction files a session declares are exactly the ones the receipt recorded (same files, so none added).
pub fn session_sources(declared: &[String], recorded: &[Source]) -> Result<(), Reject> {
    let mut seen = declared.iter().map(|p| canonical(p).ok_or(Reject::Source)).collect::<Result<Vec<_>, _>>()?;
    let mut known = recorded.iter().map(|s| canonical(&s.path).ok_or(Reject::Source)).collect::<Result<Vec<_>, _>>()?;
    seen.sort();
    seen.dedup();
    known.sort();
    known.dedup();
    if seen == known { Ok(()) } else { Err(Reject::Source) }
}

/// A matching receipt is still not enough: what it relied on must be what is there now. Hands back the SHA-256 of the
/// executable the receipt names, which the launch checks again on the started process (`proc::spawn_expecting`).
pub fn verify_current(
    dir: &Path,
    vendor: Vendor,
    now: u64,
    observe: &dyn Fn() -> io::Result<Identity>,
    managed: &dyn Fn() -> Result<(), Reject>,
) -> Result<String, Reject> {
    let receipt = verify_receipt(dir, vendor, now, observe)?;
    match vendor {
        Vendor::Claude => managed()?,
        Vendor::Codex => sources_unchanged(&receipt.proof.instruction_sources)?,
    }
    Ok(receipt.binary_hash)
}

/// Unit tests of the role loops stand in for a stored receipt with this list.
#[cfg(test)]
pub(crate) static TEST_RECORDED: std::sync::Mutex<Option<Vec<Source>>> = std::sync::Mutex::new(None);

/// The instruction files the stored receipt of a vendor recorded, for the comparison with a session's own inventory.
pub(crate) fn recorded_sources(vendor: Vendor) -> Result<Vec<Source>, Reject> {
    #[cfg(test)]
    if let Some(fixed) = TEST_RECORDED.lock().unwrap().clone() {
        return Ok(fixed);
    }
    let dir = DIR.get().ok_or(Reject::Missing)?;
    Ok(read(dir, vendor)?.proof.instruction_sources)
}

/// Writes (or atomically replaces) a vendor's receipt. Refuses an incomplete set of passed probes, so a harness
/// that skipped one cannot issue it.
pub fn issue(dir: &Path, vendor: Vendor, identity: &Identity, checks: BTreeMap<String, bool>, proof: Proof, now: u64) -> io::Result<()> {
    use std::io::Write;
    if !proof_valid(vendor, &proof)
        || !complete(&checks)
        || !hash_shaped(&identity.binary_hash)
        || parse_version(&identity.version)? != identity.version
    {
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
        launch_policy: launch_policy(vendor),
        proof,
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
/// receipt is worth checking. The program is pinned and hashed before it is asked for `--version`, and the pin is handed
/// back: while the caller holds it, the file it launches is the file the receipt names.
pub(crate) fn admitted(vendor: Vendor, bin: &dyn Fn() -> Option<String>) -> Result<Pinned, Reject> {
    if !cfg!(windows) {
        return Err(Reject::Missing);
    }
    let dir = DIR.get().ok_or(Reject::Missing)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let expected = read(dir, vendor)?.binary_hash;
    let held = std::cell::RefCell::new(None);
    let observe = || {
        let (identity, pinned) = observe_pinned(&bin().ok_or_else(|| io::Error::other("CLI not found"))?, &expected)?;
        *held.borrow_mut() = Some(pinned);
        Ok(identity)
    };
    let sha256 = verify_current(dir, vendor, now, &observe, &|| managed_policy_clear(&path_presence, &registry_presence))?;
    held.into_inner().filter(|pinned| pinned.sha256 == sha256).ok_or(Reject::Binary)
}

/// Claude Code reports no effective-hooks census, so hooks are off by construction: safe mode, no settings source
/// loaded (user, project and local), and `disableAllHooks`. Returns exactly those launch flags, or `None` when the
/// launch lacks any of them. Managed-policy hooks are the one thing these flags cannot switch off; the harness proves
/// no managed policy exists on the machine (`MANAGED_POLICY_PATHS` and the registry keys) before it issues.
pub fn hook_flags(args: &[String]) -> Option<Vec<String>> {
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let settings: serde_json::Value = serde_json::from_str(value("--settings")?).ok()?;
    if !args.iter().any(|a| a == "--safe-mode") || settings["disableAllHooks"] != true || value("--setting-sources")?.as_str() != "" {
        return None;
    }
    let mut flags = vec!["--safe-mode".to_string(), "--setting-sources".into(), String::new()];
    flags.extend(["--settings".into(), value("--settings")?.clone()]);
    Some(flags)
}

/// The documented system locations of Claude Code managed settings on Windows (files and drop-in folder). The two
/// registry keys are `HKLM` and `HKCU` `SOFTWARE\Policies\ClaudeCode`.
pub const MANAGED_POLICY_PATHS: [&str; 2] =
    [r"C:\Program Files\ClaudeCode\managed-settings.json", r"C:\Program Files\ClaudeCode\managed-settings.d"];

fn proof_valid(vendor: Vendor, proof: &Proof) -> bool {
    match vendor {
        Vendor::Claude => proof.instruction_sources.is_empty() && hook_flags(&proof.hook_flags).as_ref() == Some(&proof.hook_flags),
        Vendor::Codex => {
            proof.hook_flags.is_empty() && proof.instruction_sources.iter().all(|s| hash_shaped(&s.sha256) && !s.path.is_empty())
        }
    }
}

/// Identifies every instruction file a Codex session declared. Only two kinds are accepted: `AGENTS.md`/`CLAUDE.md`
/// in the mission clone (the work folder or any folder above it up to the clone root), and the user's global
/// `AGENTS.md`/`AGENTS.override.md` in `global_dir`. Anything else, or a path that is not exactly one of those files,
/// refuses. Only those files are opened, to hash them; never anything else under the vendor folder.
pub fn identify_sources(declared: &[String], work: &Path, clone_root: &Path, global_dir: &Path) -> Result<Vec<Source>, Reject> {
    let canon = |p: &Path| std::fs::canonicalize(p).ok();
    let root = canon(clone_root).ok_or(Reject::Source)?;
    let mut allowed: Vec<(PathBuf, SourceKind)> = vec![];
    let mut dir = canon(work).ok_or(Reject::Source)?;
    while dir.starts_with(&root) {
        for name in ["AGENTS.md", "CLAUDE.md"] {
            allowed.push((dir.join(name), SourceKind::Clone));
        }
        if dir == root || !dir.pop() {
            break;
        }
    }
    if let Some(global) = canon(global_dir) {
        for name in ["AGENTS.md", "AGENTS.override.md"] {
            allowed.push((global.join(name), SourceKind::Global));
        }
    }
    declared
        .iter()
        .map(|path| {
            let real = canon(Path::new(path)).ok_or(Reject::Source)?;
            let (_, kind) = allowed.iter().find(|(candidate, _)| *candidate == real).ok_or(Reject::Source)?;
            let sha256 = fingerprint_source(&real).map_err(|_| Reject::Source)?;
            // The path must still lead to the accepted file after the read: a swap in between refuses.
            if canon(&real).as_deref() != Some(real.as_path()) {
                return Err(Reject::Source);
            }
            Ok(Source { path: path.clone(), sha256, kind: *kind })
        })
        .collect()
}
