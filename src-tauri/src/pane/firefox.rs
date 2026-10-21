//! Reading the sign-ins a person has in Firefox, so the browser pane can start where they already are. Only Firefox is
//! read: Chrome and Edge lock their cookies with keys that belong to their own app, and Faber does not get around that.
//!
//! This file only reads. It finds the profiles, lists the sites that have cookies and returns the cookies of the sites
//! the person picked; putting them in the pane is `mod.rs`'s job. A cookie's value lives in the returned list and
//! nowhere else: it is never logged and never put in an error. The working copy of the database sits in the person's own
//! temporary folder (private to their Windows account) and is deleted after the read; one a crash left is swept on the next.
//!
//! Firefox keeps `cookies.sqlite` open and in WAL mode, so it is never opened in place: it is copied, with its `-wal`
//! file, and the copy is read. The screen never sends a path, only a profile id, which is found again here.

use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DB: &str = "cookies.sqlite";
/// The start of the name of the temporary folder a database is copied to.
#[cfg_attr(not(windows), allow(dead_code))]
const COPY_PREFIX: &str = "faber-firefox-";
/// The biggest database (and the biggest `-wal` file) that is copied; a real one is a few megabytes.
const MAX_DB: u64 = 64 * 1024 * 1024;
/// The biggest `profiles.ini` that is read.
const MAX_INI: u64 = 1024 * 1024;
/// At most this many sites are listed for the person to pick from.
const MAX_SITES: usize = 500;
/// At most this many cookies come back from one import.
const MAX_COOKIES: usize = 10_000;
/// Hosts per query, under SQLite's limit of 999 parameters.
const CHUNK: usize = 500;
/// A name is shown on screen, so it is cut.
const MAX_NAME: usize = 64;
/// A cookie that outlives this is cut to it: a brought-over sign-in is a loan, not a copy for life.
const MAX_AGE: i64 = 30 * 24 * 60 * 60;
/// Firefox writes some expiry times in seconds and some in milliseconds; nothing in seconds reaches this until the year 2286.
const MILLISECONDS: i64 = 10_000_000_000;

const MISSING: &str = "error.cookies.missing";
const LOCKED: &str = "error.cookies.locked";
const TOO_BIG: &str = "error.cookies.tooBig";

/// The expiry of a row in seconds, whichever unit it was stored in.
#[cfg(windows)]
const EXPIRY_SECONDS: &str = "(CASE WHEN expiry > 10000000000 THEN expiry / 1000 ELSE expiry END)";

/// A Firefox profile with a cookie database. The path stays here: callers hold the `id`.
#[derive(Clone, Debug)]
pub struct Profile {
    /// `firefox/<folder>` or `store/<folder>`: stable between calls, the folder being the profile's own.
    pub id: String,
    /// Its name in Firefox, or `default` for the ones Firefox makes by itself (the screen writes that in its language).
    pub name: String,
    /// The profile Firefox opened most recently; exactly one across every root when there is any.
    pub last_used: bool,
    db: PathBuf,
    /// Firefox names it as the default of an installation (or, with none named, marks it `Default=1`).
    candidate: bool,
    modified: SystemTime,
}

/// Why a site's sign-ins deserve a second look before they are brought over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Money,
    Email,
    SignIn,
}

/// One site with cookies: its cookies under every host it keeps them on.
#[derive(Clone, Debug)]
pub struct Site {
    pub name: String,
    pub cookies: u32,
    pub kind: Option<Kind>,
    /// The hosts as Firefox stores them, dots included, so `cookies` can pick them exactly.
    pub hosts: Vec<String>,
}

/// A cookie ready for the pane. It has no `Debug` on purpose: its value must never reach a log.
#[derive(Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub host: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<&'static str>,
    /// Unix seconds.
    pub expires: f64,
}

/// A row as the database holds it, before the rules below.
struct Raw {
    name: String,
    value: String,
    host: String,
    path: String,
    expiry: i64,
    secure: bool,
    http_only: bool,
    same_site: i64,
}

// ---------------------------------------------------------------------------------------------------------------------
// Profiles

/// The profiles of both places Firefox keeps them (the installer's and the Microsoft Store's), last used first.
pub fn profiles() -> Vec<Profile> {
    let mut all = Vec::new();
    if let Some(appdata) = env_path("APPDATA") {
        all.extend(profiles_in(&appdata.join("Mozilla").join("Firefox"), "firefox"));
    }
    if let Some(local) = env_path("LOCALAPPDATA") {
        let store =
            local.join("Packages").join("Mozilla.Firefox_n80bbvh6b1yt2").join("LocalCache").join("Roaming").join("Mozilla").join("Firefox");
        all.extend(profiles_in(&store, "store"));
    }
    mark_last_used(&mut all);
    all.sort_by(|a, b| {
        b.last_used.cmp(&a.last_used).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.id.cmp(&b.id))
    });
    all
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The profiles one `profiles.ini` names, with their cookie databases in place. `tag` starts every id. None is marked as
/// last used yet: that is decided over every root at once.
fn profiles_in(root: &Path, tag: &str) -> Vec<Profile> {
    let ini = root.join("profiles.ini");
    if !fs::metadata(&ini).is_ok_and(|m| m.is_file() && m.len() <= MAX_INI) {
        return Vec::new();
    }
    let Ok(bytes) = fs::read(&ini) else { return Vec::new() };
    let sections = parse_ini(&String::from_utf8_lossy(&bytes));
    let installed: Vec<String> = sections
        .iter()
        .filter(|(name, _)| name.to_ascii_lowercase().starts_with("install"))
        .filter_map(|(_, keys)| value(keys, "Default").map(same_path))
        .collect();

    let mut found: Vec<(Profile, bool, bool)> = Vec::new();
    for (section, keys) in &sections {
        let number = section.get(..7).filter(|p| p.eq_ignore_ascii_case("profile")).map(|_| &section[7..]);
        if !number.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let Some(path) = value(keys, "Path").filter(|p| !p.is_empty()) else { continue };
        let relative = value(keys, "IsRelative") != Some("0");
        let dir = if relative { inside(root, path) } else { absolute(path) };
        let Some(dir) = dir else { continue };
        let db = dir.join(DB);
        if !stays_local(&db) {
            continue;
        }
        let Ok(meta) = fs::metadata(&db) else { continue };
        if !meta.is_file() || meta.len() > MAX_DB {
            continue;
        }
        let Some(folder) = dir.file_name().and_then(|f| f.to_str()) else { continue };
        let id = format!("{tag}/{folder}");
        if found.iter().any(|(p, _, _)| p.id == id) {
            continue;
        }
        let name = display_name(value(keys, "Name").unwrap_or(folder));
        let modified = newest(&db);
        let named = installed.contains(&same_path(path));
        let marked = value(keys, "Default") == Some("1");
        found.push((Profile { id, name, last_used: false, db, candidate: false, modified }, named, marked));
    }
    let any_named = found.iter().any(|(_, named, _)| *named);
    found
        .into_iter()
        .map(|(mut p, named, marked)| {
            p.candidate = if any_named { named } else { marked };
            p
        })
        .collect()
}

/// Marks the profile Firefox used last: among those it names as a default when there are any, else among all.
fn mark_last_used(all: &mut [Profile]) {
    let any_candidate = all.iter().any(|p| p.candidate);
    let best = all
        .iter()
        .enumerate()
        .filter(|(_, p)| p.candidate || !any_candidate)
        .max_by_key(|(i, p)| (p.modified, Reverse(*i)))
        .map(|(i, _)| i);
    for (i, p) in all.iter_mut().enumerate() {
        p.last_used = Some(i) == best;
    }
}

/// When the cookie database was last written, counting its `-wal` file, where Firefox's newest writes wait.
fn newest(db: &Path) -> SystemTime {
    let wal = wal_of(db);
    [db, wal.as_path()].iter().filter_map(|f| fs::metadata(f).and_then(|m| m.modified()).ok()).max().unwrap_or(UNIX_EPOCH)
}

fn wal_of(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push("-wal");
    PathBuf::from(name)
}

/// The profiles Firefox makes by itself all read as `default`, which the screen writes in its own language.
fn display_name(raw: &str) -> String {
    let clean: String = raw.chars().filter(|c| !c.is_control()).collect();
    let name = clean.trim();
    const OWN: [&str; 6] = ["default", "default-release", "default-esr", "default-beta", "default-nightly", "dev-edition-default"];
    if OWN.iter().any(|o| name.eq_ignore_ascii_case(o)) {
        return "default".to_string();
    }
    name.chars().take(MAX_NAME).collect()
}

/// A path as `profiles.ini` writes it, for comparing two of them: any slash, any case.
fn same_path(raw: &str) -> String {
    raw.trim().replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn value<'a>(keys: &'a [(String, String)], key: &str) -> Option<&'a str> {
    keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

/// A small INI: `[Section]` lines and `Key=Value` lines, with blank lines and `;` or `#` comments left out.
fn parse_ini(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.trim().to_string(), Vec::new()));
        } else if let (Some((key, val)), Some(section)) = (line.split_once('='), sections.last_mut()) {
            section.1.push((key.trim().to_string(), val.trim().to_string()));
        }
    }
    sections
}

/// Whether every folder on the way to `path` stays on this computer. A mapped drive, a junction or a link may lead to
/// another computer's share, which Windows would reach (and sign in to) on the first look, so the drive must be this
/// computer's own and the path is looked at from its root down, each step before anything goes through it: a link is
/// followed only to a path that passes the same look, read without opening what it points to.
#[cfg(windows)]
fn stays_local(path: &Path) -> bool {
    local_from(path, 0)
}

#[cfg(windows)]
fn local_from(path: &Path, links: u32) -> bool {
    use std::os::windows::fs::MetadataExt;
    use winsafe::co::DRIVE;
    const REPARSE_POINT: u32 = 0x400;
    // Links that lead to links: past a few, the path is refused rather than followed.
    if links > 4 || !on_a_drive(path) {
        return false;
    }
    let text = path.to_string_lossy();
    let letter = text.strip_prefix(r"\\?\").unwrap_or(&text).as_bytes()[0] as char;
    if !matches!(winsafe::GetDriveType(Some(&format!("{letter}:\\"))), DRIVE::FIXED | DRIVE::REMOVABLE) {
        return false;
    }
    // The drive's root first, never the bare `C:` (that names the drive's current folder, which may lie anywhere), then
    // each part: a step that cannot be looked at is never gone through.
    let mut step = PathBuf::new();
    for part in path.components() {
        step.push(part);
        if matches!(part, std::path::Component::Prefix(_)) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&step) else { return false };
        if meta.file_attributes() & REPARSE_POINT != 0 && !fs::read_link(&step).is_ok_and(|to| local_from(&to, links + 1)) {
            return false;
        }
    }
    true
}

#[cfg(not(windows))]
fn stays_local(_: &Path) -> bool {
    true
}

/// A plain drive path (`C:\...`, also in its `\\?\` form), never a share or a device.
#[cfg_attr(not(windows), allow(dead_code))]
fn on_a_drive(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let bytes = text.strip_prefix(r"\\?\").unwrap_or(&text).as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/')
}

/// A folder under `root`, from a relative path, or nothing when the path would climb out of `root`.
fn inside(root: &Path, rel: &str) -> Option<PathBuf> {
    let parts = lexical(rel)?;
    if parts.is_empty() {
        return None;
    }
    Some(parts.iter().fold(root.to_path_buf(), |dir, part| dir.join(part)))
}

/// A folder from a plain drive path (`C:\...`). Network, device and verbatim paths are never read.
fn absolute(path: &str) -> Option<PathBuf> {
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || !matches!(bytes[2], b'\\' | b'/') {
        return None;
    }
    let parts = lexical(&path[3..])?;
    if parts.is_empty() {
        return None;
    }
    let drive = PathBuf::from(format!("{}:\\", bytes[0].to_ascii_uppercase() as char));
    Some(parts.iter().fold(drive, |dir, part| dir.join(part)))
}

/// The parts of a path with `.` and `..` settled by reading alone, or nothing when a `..` has nothing left to climb or a
/// part could name a drive, a stream or a device.
fn lexical(path: &str) -> Option<Vec<&str>> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p if p.contains(':') || p.contains('\0') || p.ends_with('.') || p.ends_with(' ') => return None,
            p => parts.push(p),
        }
    }
    Some(parts)
}

// ---------------------------------------------------------------------------------------------------------------------
// Sites

/// The sites with cookies that have not expired, the ones that guard money, mail or sign-ins first.
pub fn sites(profile: &Profile, now: i64) -> Result<Vec<Site>, &'static str> {
    let mut sites = group(store::host_counts(profile, now)?);
    sites.sort_by(|a, b| rank(a.kind).cmp(&rank(b.kind)).then(b.cookies.cmp(&a.cookies)).then_with(|| a.name.cmp(&b.name)));
    sites.truncate(MAX_SITES);
    Ok(sites)
}

fn rank(kind: Option<Kind>) -> u8 {
    match kind {
        Some(Kind::SignIn) => 0,
        Some(Kind::Email) => 1,
        Some(Kind::Money) => 2,
        None => 3,
    }
}

/// Hosts with their cookie counts, as Firefox stores them, into sites: a host under a domain cookie belongs to that
/// domain, the others stand alone without a `www.`.
fn group(rows: Vec<(String, i64)>) -> Vec<Site> {
    let seen: Vec<(String, String, i64)> = rows.into_iter().filter_map(|(raw, n)| clean_host(&raw).map(|host| (raw, host, n))).collect();
    let domains: Vec<&str> = seen.iter().filter(|(raw, _, _)| raw.trim().starts_with('.')).map(|(_, host, _)| host.as_str()).collect();
    let mut by_name: BTreeMap<String, Site> = BTreeMap::new();
    for (raw, host, n) in &seen {
        let name = domains
            .iter()
            .filter(|d| host.strip_suffix(**d).is_some_and(|rest| rest.is_empty() || rest.ends_with('.')))
            .min_by_key(|d| d.len())
            .map_or_else(|| host.strip_prefix("www.").filter(|rest| rest.contains('.')).unwrap_or(host).to_string(), |d| (*d).to_string());
        let site = by_name.entry(name.clone()).or_insert_with(|| Site { kind: kind_of(&name), name, cookies: 0, hosts: Vec::new() });
        site.cookies = site.cookies.saturating_add(u32::try_from((*n).max(0)).unwrap_or(u32::MAX));
        if !site.hosts.contains(raw) {
            site.hosts.push(raw.clone());
        }
    }
    by_name.into_values().collect()
}

/// A host as a site could be named: lowercase, no leading dot, a real DNS name with at least one dot. Addresses and names
/// that only exist inside a network are not sites a person signs in to.
fn clean_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let host = trimmed.strip_prefix('.').unwrap_or(trimmed).to_ascii_lowercase();
    let plain = host.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    if !plain || host.len() > 253 || !host.contains('.') || host.split('.').any(str::is_empty) {
        return None;
    }
    let numeric_end = host.rsplit('.').next().is_some_and(|last| last.bytes().all(|b| b.is_ascii_digit()));
    let inner = host == "localhost" || [".local", ".internal", ".localhost"].iter().any(|s| host.ends_with(s));
    (!numeric_end && !inner && host.parse::<Ipv4Addr>().is_err()).then_some(host)
}

/// Names whose sign-in opens other doors.
const SIGN_IN_WORDS: [&str; 12] = [
    "okta",
    "onelogin",
    "auth0",
    "duosecurity",
    "duo",
    "pingidentity",
    "microsoftonline",
    "1password",
    "lastpass",
    "bitwarden",
    "dashlane",
    "keepersecurity",
];
const SIGN_IN_DOMAINS: [&str; 6] = ["login.live.com", "live.com", "microsoft.com", "apple.com", "icloud.com", "accounts.google.com"];
const EMAIL_DOMAINS: [&str; 11] = [
    "google.com",
    "gmail.com",
    "outlook.com",
    "hotmail.com",
    "office.com",
    "yahoo.com",
    "proton.me",
    "protonmail.com",
    "fastmail.com",
    "hey.com",
    "zoho.com",
];
const MONEY_WORDS: [&str; 21] = [
    "paypal",
    "stripe",
    "wise",
    "revolut",
    "nubank",
    "mercadopago",
    "picpay",
    "inter",
    "itau",
    "bradesco",
    "santander",
    "caixa",
    "chase",
    "schwab",
    "fidelity",
    "vanguard",
    "robinhood",
    "coinbase",
    "binance",
    "kraken",
    "americanexpress",
];
/// Any label holding one of these is a bank, in the languages the people who use Faber write in.
const MONEY_PARTS: [&str; 3] = ["bank", "banco", "banque"];

/// Whether a site's sign-in guards something worth a second look. Whole labels are compared, so `wise` is a money site and
/// `otherwise` is not; the first match of sign-in, then mail, then money wins.
fn kind_of(site: &str) -> Option<Kind> {
    let under = |domain: &&str| site == *domain || site.strip_suffix(*domain).is_some_and(|rest| rest.ends_with('.'));
    let labels: Vec<&str> = site.split('.').collect();
    if labels.iter().any(|l| SIGN_IN_WORDS.contains(l)) || SIGN_IN_DOMAINS.iter().any(under) {
        Some(Kind::SignIn)
    } else if EMAIL_DOMAINS.iter().any(under) {
        Some(Kind::Email)
    } else if labels.iter().any(|l| MONEY_WORDS.contains(l) || MONEY_PARTS.iter().any(|p| l.contains(p))) {
        Some(Kind::Money)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Cookies

/// The cookies that have not expired on exactly these hosts (as `Site::hosts` gives them). Containers, private windows and
/// partitioned storage are left out.
pub fn cookies(profile: &Profile, hosts: &[String], now: i64) -> Result<Vec<Cookie>, &'static str> {
    let raw = store::rows(profile, hosts, now)?;
    Ok(raw.into_iter().filter_map(|r| finish(r, now)).take(MAX_COOKIES).collect())
}

/// The rules a row passes before it is a cookie: a lifetime in seconds, no more than thirty days, and a name and value a
/// `Cookie` header can carry.
fn finish(r: Raw, now: i64) -> Option<Cookie> {
    let seconds = if r.expiry > MILLISECONDS { r.expiry / 1000 } else { r.expiry };
    if seconds <= now {
        return None;
    }
    let odd = |s: &str| s.chars().any(|c| c.is_control() || c == ';');
    if r.name.is_empty() || odd(&r.name) || odd(&r.value) {
        return None;
    }
    let same_site = match r.same_site {
        0 => r.secure.then_some("None"),
        1 => Some("Lax"),
        2 => Some("Strict"),
        _ => None,
    };
    Some(Cookie {
        name: r.name,
        value: r.value,
        host: r.host,
        path: if r.path.is_empty() { "/".to_string() } else { r.path },
        secure: r.secure,
        http_only: r.http_only,
        same_site,
        #[allow(clippy::cast_precision_loss, reason = "a Unix time in seconds is far below 2^53")]
        expires: seconds.min(now.saturating_add(MAX_AGE)) as f64,
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// The database copy

#[cfg(windows)]
mod store {
    use super::*;
    use rusqlite::types::Value;
    use rusqlite::{Connection, OpenFlags, Row, params_from_iter};
    use std::fs::File;
    use std::io::{self, Read};

    /// Each host Firefox keeps cookies for with how many, skipping containers and what has expired.
    pub(super) fn host_counts(profile: &Profile, now: i64) -> Result<Vec<(String, i64)>, &'static str> {
        with_copy(profile, |db| {
            let sql = format!(
                "SELECT host, COUNT(*) FROM moz_cookies WHERE COALESCE(originAttributes, '') = '' AND {EXPIRY_SECONDS} > ?1 GROUP BY host"
            );
            let mut stmt = db.prepare(&sql)?;
            let mut rows = stmt.query([now])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                if let (Ok(Some(host)), Ok(n)) = (row.get::<_, Option<String>>(0), row.get::<_, i64>(1)) {
                    out.push((host, n));
                }
            }
            Ok(out)
        })
    }

    /// The rows of exactly these hosts, no more than `MAX_COOKIES` before the rules of `finish`.
    pub(super) fn rows(profile: &Profile, hosts: &[String], now: i64) -> Result<Vec<Raw>, &'static str> {
        with_copy(profile, |db| {
            let mut out = Vec::new();
            for chunk in hosts.chunks(CHUNK) {
                let marks = vec!["?"; chunk.len()].join(",");
                let sql = format!(
                    "SELECT name, value, host, path, expiry, isSecure, isHttpOnly, sameSite FROM moz_cookies \
                     WHERE COALESCE(originAttributes, '') = '' AND host IN ({marks}) AND {EXPIRY_SECONDS} > ?"
                );
                let mut stmt = db.prepare(&sql)?;
                let args = chunk.iter().map(|h| Value::Text(h.clone())).chain([Value::Integer(now)]);
                let mut rows = stmt.query(params_from_iter(args))?;
                while let Some(row) = rows.next()? {
                    if let Ok(raw) = read(row) {
                        out.push(raw);
                    }
                    if out.len() >= MAX_COOKIES * 2 {
                        return Ok(out);
                    }
                }
            }
            Ok(out)
        })
    }

    fn read(row: &Row) -> rusqlite::Result<Raw> {
        Ok(Raw {
            name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
            value: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            host: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            expiry: row.get(4)?,
            secure: row.get::<_, i64>(5)? != 0,
            http_only: row.get::<_, i64>(6)? != 0,
            same_site: row.get(7)?,
        })
    }

    /// Runs `read` on a private copy of the profile's database. The copy needs write access so SQLite can replay the `-wal`
    /// file into it; it is never the original. The folder that holds it is owner-only and gone when this returns.
    fn with_copy<T>(profile: &Profile, read: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, &'static str> {
        if !stays_local(&profile.db) || !profile.db.is_file() {
            return Err(MISSING);
        }
        sweep_left_copies();
        let dir = tempfile::Builder::new().prefix(COPY_PREFIX).tempdir().map_err(|_| LOCKED)?;
        let copy = dir.path().join(DB);
        copy_capped(&profile.db, &copy)?;
        let wal = wal_of(&profile.db);
        match File::open(&wal) {
            Ok(_) => copy_capped(&wal, &wal_of(&copy))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(LOCKED),
        }
        let db =
            Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|_| LOCKED)?;
        read(&db).map_err(|_| LOCKED)
    }

    /// The folders of earlier copies Faber could not delete (it stopped mid-read), older than a minute so a read under way
    /// keeps its own.
    fn sweep_left_copies() {
        let Ok(entries) = fs::read_dir(std::env::temp_dir()) else { return };
        for entry in entries.flatten() {
            let old =
                entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 60);
            if old && entry.file_name().to_string_lossy().starts_with(COPY_PREFIX) && entry.file_type().is_ok_and(|t| t.is_dir()) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }

    /// Copies a file that may be growing as it is read, and gives up on one over `MAX_DB`.
    fn copy_capped(from: &Path, to: &Path) -> Result<(), &'static str> {
        let mut src = File::open(from).map_err(|_| LOCKED)?;
        let mut dst = File::create(to).map_err(|_| LOCKED)?;
        let copied = io::copy(&mut (&mut src).take(MAX_DB + 1), &mut dst).map_err(|_| LOCKED)?;
        if copied > MAX_DB { Err(TOO_BIG) } else { Ok(()) }
    }
}

/// Firefox's cookies are read on Windows only, where Faber's pane runs on WebView2 and its own sign-ins are worth bringing.
#[cfg(not(windows))]
mod store {
    use super::*;

    pub(super) fn host_counts(_: &Profile, _: i64) -> Result<Vec<(String, i64)>, &'static str> {
        Err(MISSING)
    }

    pub(super) fn rows(_: &Profile, _: &[String], _: i64) -> Result<Vec<Raw>, &'static str> {
        Err(MISSING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const NOW: i64 = 1_800_000_000;

    fn write_ini(root: &Path, text: &str) {
        fs::write(root.join("profiles.ini"), text).unwrap();
    }

    /// A profile folder under `root` with a stand-in cookie database (these tests read no cookies).
    fn folder(root: &Path, rel: &str) -> PathBuf {
        let dir = root.join(rel);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(DB), b"x").unwrap();
        dir
    }

    fn touch(file: &Path, seconds: u64) {
        let at = UNIX_EPOCH + Duration::from_secs(seconds);
        fs::File::options().write(true).open(file).unwrap().set_times(fs::FileTimes::new().set_modified(at)).unwrap();
    }

    fn ids(list: &[Profile]) -> Vec<&str> {
        list.iter().map(|p| p.id.as_str()).collect()
    }

    #[test]
    fn a_profiles_ini_with_a_bom_and_windows_line_ends_names_its_profiles_and_the_installs_default() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "Profiles/abc.default-release");
        folder(root.path(), "Profiles/xyz.work");
        write_ini(
            root.path(),
            "\u{feff}; a comment\r\n# another\r\n[Install4F96D1932A9F858E]\r\nDefault=Profiles/xyz.work\r\nLocked=1\r\n\r\n\
             [Profile1]\r\nName = work\r\nIsRelative=1\r\nPath=Profiles/xyz.work\r\n\r\n\
             [Profile0]\r\nName=default-release\r\nIsRelative=1\r\nPath=Profiles/abc.default-release\r\nDefault=1\r\n\r\n\
             [BackgroundTasksProfiles]\r\nMozillaBackgroundTask-308046B0AF4A39CB=bg.default\r\n\r\n[General]\r\nVersion=2\r\n",
        );
        let list = profiles_in(root.path(), "firefox");
        assert_eq!(ids(&list), ["firefox/xyz.work", "firefox/abc.default-release"], "in the file's order, ids from the folder names");
        assert_eq!(list[0].name, "work");
        assert_eq!(list[1].name, "default");
        assert!(list[0].candidate, "the install names it");
        assert!(!list[1].candidate, "Default=1 only counts when no install names one");
        assert!(list.iter().all(|p| !p.last_used), "that is decided over every root at once");
    }

    #[test]
    fn without_an_install_section_the_profile_marked_default_is_the_candidate() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "a.one");
        folder(root.path(), "b.two");
        write_ini(
            root.path(),
            "[Profile0]\nName=one\nIsRelative=1\nPath=a.one\n[Profile1]\nName=two\nIsRelative=1\nPath=b.two\nDefault=1\n",
        );
        let list = profiles_in(root.path(), "store");
        assert_eq!(list.iter().map(|p| p.candidate).collect::<Vec<_>>(), [false, true]);
        assert_eq!(ids(&list), ["store/a.one", "store/b.two"]);
    }

    #[test]
    fn the_last_used_profile_is_the_installs_default_written_most_recently_across_both_roots() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let old = folder(a.path(), "old.default");
        let new = folder(a.path(), "new.default-release");
        let other = folder(b.path(), "other");
        write_ini(
            a.path(),
            "[Profile0]\nName=old\nPath=old.default\n[Profile1]\nName=new\nPath=new.default-release\n[Install1]\nDefault=new.default-release\n",
        );
        write_ini(b.path(), "[Profile0]\nName=other\nPath=other\nDefault=1\n");
        touch(&old.join(DB), 900);
        touch(&new.join(DB), 100);
        touch(&other.join(DB), 500);
        let mut all = profiles_in(a.path(), "firefox");
        all.extend(profiles_in(b.path(), "store"));
        mark_last_used(&mut all);
        let used: Vec<&str> = all.iter().filter(|p| p.last_used).map(|p| p.id.as_str()).collect();
        assert_eq!(used, ["store/other"], "candidates are the install default and the other root's Default=1: the newer of them wins");

        touch(&new.join(DB), 1_000);
        let mut all = profiles_in(a.path(), "firefox");
        all.extend(profiles_in(b.path(), "store"));
        mark_last_used(&mut all);
        let used: Vec<&str> = all.iter().filter(|p| p.last_used).map(|p| p.id.as_str()).collect();
        assert_eq!(used, ["firefox/new.default-release"]);
    }

    #[test]
    fn a_wal_file_written_after_the_database_counts_as_the_profile_being_used() {
        let root = tempfile::tempdir().unwrap();
        let one = folder(root.path(), "one");
        let two = folder(root.path(), "two");
        write_ini(root.path(), "[Profile0]\nName=one\nPath=one\n[Profile1]\nName=two\nPath=two\n");
        touch(&one.join(DB), 100);
        touch(&two.join(DB), 200);
        fs::write(one.join("cookies.sqlite-wal"), b"x").unwrap();
        touch(&one.join("cookies.sqlite-wal"), 300);
        let mut all = profiles_in(root.path(), "firefox");
        mark_last_used(&mut all);
        assert_eq!(all.iter().find(|p| p.last_used).map(|p| p.id.as_str()), Some("firefox/one"));
        assert_eq!(all.iter().filter(|p| p.last_used).count(), 1);
    }

    #[test]
    fn a_profile_path_that_climbs_out_of_firefox_s_folder_is_never_read() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("Firefox");
        fs::create_dir_all(&root).unwrap();
        folder(parent.path(), "elsewhere");
        folder(&root, "Profiles/fine");
        write_ini(
            &root,
            "[Profile0]\nName=up\nIsRelative=1\nPath=../elsewhere\n\
             [Profile1]\nName=deep\nIsRelative=1\nPath=Profiles/../../elsewhere\n\
             [Profile2]\nName=slashes\nIsRelative=1\nPath=..\\elsewhere\n\
             [Profile3]\nName=settled\nIsRelative=1\nPath=Profiles/x/../fine\n\
             [Profile4]\nName=drive\nIsRelative=1\nPath=C:\\Windows\n\
             [Profile5]\nName=stream\nIsRelative=1\nPath=Profiles/fine:stream\n",
        );
        let list = profiles_in(&root, "firefox");
        assert_eq!(ids(&list), ["firefox/fine"], "only the path that stays inside after `..` is settled");
        assert_eq!(list[0].name, "settled");
    }

    #[test]
    fn a_link_is_followed_only_to_a_local_drive() {
        assert!(on_a_drive(Path::new(r"D:\Firefox\x")));
        assert!(on_a_drive(Path::new(r"\\?\D:\Firefox")));
        for far in [r"\\server\share\x", r"\\?\UNC\server\share", r"\\.\pipe\x", "Firefox"] {
            assert!(!on_a_drive(Path::new(far)), "{far}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_profile_behind_a_local_junction_is_read_and_a_relative_or_far_path_never() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("cookies.sqlite"), b"").unwrap();
        let link = root.path().join("link");
        junction::create(&real, &link).unwrap();
        assert!(stays_local(&real.join("cookies.sqlite")));
        assert!(stays_local(&link.join("cookies.sqlite")), "a junction to this computer's own drive is followed");
        assert!(!stays_local(&root.path().join("gone").join("cookies.sqlite")), "a step that cannot be looked at is never gone through");
        assert!(!stays_local(Path::new(r"relative\cookies.sqlite")));
        assert!(!stays_local(Path::new(r"\\server\share\cookies.sqlite")));
    }

    #[test]
    fn network_device_and_verbatim_paths_are_never_read() {
        let root = tempfile::tempdir().unwrap();
        write_ini(
            root.path(),
            "[Profile0]\nIsRelative=0\nPath=\\\\server\\share\\p\n\
             [Profile1]\nIsRelative=0\nPath=//server/share/p\n\
             [Profile2]\nIsRelative=0\nPath=\\\\?\\C:\\Users\\p\n\
             [Profile3]\nIsRelative=0\nPath=\\\\.\\C:\\p\n\
             [Profile4]\nIsRelative=0\nPath=C:\\..\\p\n\
             [Profile5]\nIsRelative=0\nPath=relative\\p\n\
             [Profile6]\nIsRelative=0\nPath=C:\n",
        );
        assert!(profiles_in(root.path(), "firefox").is_empty());
        assert_eq!(absolute("C:\\Users\\me\\.\\p\\..\\q"), Some(PathBuf::from("C:\\").join("Users").join("me").join("q")));
        assert_eq!(absolute("d:/x"), Some(PathBuf::from("D:\\").join("x")));
    }

    #[test]
    fn a_profile_without_a_usable_cookie_database_is_left_out() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "has");
        fs::create_dir_all(root.path().join("none")).unwrap();
        fs::create_dir_all(root.path().join("dir").join(DB)).unwrap();
        let big = folder(root.path(), "big");
        fs::File::options().write(true).open(big.join(DB)).unwrap().set_len(MAX_DB + 1).unwrap();
        let exact = folder(root.path(), "exact");
        fs::File::options().write(true).open(exact.join(DB)).unwrap().set_len(MAX_DB).unwrap();
        write_ini(
            root.path(),
            "[Profile0]\nPath=has\n[Profile1]\nPath=none\n[Profile2]\nPath=dir\n[Profile3]\nPath=big\n[Profile4]\nPath=exact\n[Profile5]\nName=nopath\n[ProfileX]\nPath=has\n",
        );
        assert_eq!(ids(&profiles_in(root.path(), "firefox")), ["firefox/has", "firefox/exact"]);
        assert!(profiles_in(&root.path().join("missing"), "firefox").is_empty());
    }

    #[test]
    fn the_profiles_firefox_makes_by_itself_read_as_default_and_other_names_are_cut_at_a_character() {
        for own in ["default", "Default-Release", "default-esr", "default-beta", "default-nightly", "dev-edition-default"] {
            assert_eq!(display_name(own), "default", "{own}");
        }
        assert_eq!(display_name("  Work\u{7} laptop "), "Work laptop");
        assert_eq!(display_name("default-custom"), "default-custom");
        let long = "é".repeat(100);
        assert_eq!(display_name(&long).chars().count(), MAX_NAME);
        assert_eq!(display_name(&long), "é".repeat(MAX_NAME));
    }

    fn hosts(list: &[(&str, i64)]) -> Vec<(String, i64)> {
        list.iter().map(|(h, n)| ((*h).to_string(), *n)).collect()
    }

    #[test]
    fn hosts_group_into_sites_by_their_shortest_domain_cookie_and_lose_www() {
        let sites = group(hosts(&[
            (".example.com", 2),
            ("shop.example.com", 3),
            ("www.example.com", 1),
            ("www.solo.org", 4),
            (".Deep.Sub.Site.net", 1),
            ("api.deep.sub.site.net", 1),
            (".sub.site.net", 5),
        ]));
        let by = |name: &str| sites.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no site {name}"));
        assert_eq!(by("example.com").cookies, 6);
        assert_eq!(by("example.com").hosts, [".example.com", "shop.example.com", "www.example.com"], "hosts keep their raw dots");
        assert_eq!(by("solo.org").hosts, ["www.solo.org"]);
        assert_eq!(by("sub.site.net").cookies, 7, "the shortest dotted domain wins");
        assert_eq!(sites.len(), 3);
    }

    #[test]
    fn addresses_and_names_that_only_exist_on_a_network_are_not_sites() {
        let sites = group(hosts(&[
            ("localhost", 1),
            (".localhost", 1),
            ("192.168.0.1", 1),
            (".10.0.0.5", 1),
            ("printer.local", 1),
            ("db.internal", 1),
            ("app.localhost", 1),
            ("bad host.com", 1),
            ("[::1]", 1),
            ("a..b.com", 1),
            ("münchen.de", 1),
            ("good.example", 2),
        ]));
        assert_eq!(sites.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["good.example"]);
    }

    #[test]
    fn sites_that_guard_money_mail_or_sign_ins_are_named_by_whole_labels() {
        let kind = |site| kind_of(site);
        assert_eq!(kind("itau.com.br"), Some(Kind::Money));
        assert_eq!(kind("meubanco.com.br"), Some(Kind::Money));
        assert_eq!(kind("bank.example"), Some(Kind::Money));
        assert_eq!(kind("banque-exemple.fr"), Some(Kind::Money));
        assert_eq!(kind("wise.com"), Some(Kind::Money));
        assert_eq!(kind("otherwise.com"), None, "a label that only ends in a bank's name is not that bank");
        assert_eq!(kind("internet.com"), None);
        assert_eq!(kind("mail.google.com"), Some(Kind::Email));
        assert_eq!(kind("google.com"), Some(Kind::Email));
        assert_eq!(kind("notgoogle.com"), None);
        assert_eq!(kind("accounts.google.com"), Some(Kind::SignIn), "sign-in is checked before mail");
        assert_eq!(kind("mycompany.okta.com"), Some(Kind::SignIn));
        assert_eq!(kind("login.microsoftonline.com"), Some(Kind::SignIn));
        assert_eq!(kind("outlook.live.com"), Some(Kind::SignIn));
        assert_eq!(kind("bitwarden.net"), Some(Kind::SignIn));
        assert_eq!(kind("github.com"), None);
        assert_eq!(serde_json::to_string(&Kind::SignIn).unwrap(), "\"signin\"");
        assert_eq!(serde_json::to_string(&Kind::Money).unwrap(), "\"money\"");
    }

    fn raw(name: &str, value: &str) -> Raw {
        Raw {
            name: name.to_string(),
            value: value.to_string(),
            host: ".example.com".to_string(),
            path: "/".to_string(),
            expiry: NOW + 100,
            secure: false,
            http_only: false,
            same_site: 1,
        }
    }

    #[test]
    fn a_row_becomes_a_cookie_only_when_it_is_alive_and_a_header_can_carry_it() {
        let alive = finish(raw("sid", "abc"), NOW).unwrap();
        assert_eq!(
            (alive.name.as_str(), alive.value.as_str(), alive.host.as_str(), alive.path.as_str()),
            ("sid", "abc", ".example.com", "/")
        );
        assert_eq!(alive.expires, (NOW + 100) as f64);
        let mut expired = raw("sid", "abc");
        expired.expiry = NOW;
        assert!(finish(expired, NOW).is_none(), "expired at this very second");
        assert!(finish(raw("", "abc"), NOW).is_none());
        assert!(finish(raw("a;b", "abc"), NOW).is_none());
        assert!(finish(raw("sid", "a;b"), NOW).is_none());
        assert!(finish(raw("sid", "a\nb"), NOW).is_none());
        assert!(finish(raw("sid", "a\u{7f}b"), NOW).is_none());
        assert!(finish(raw("sid", ""), NOW).is_some(), "an empty value is a value");
        let mut no_path = raw("sid", "abc");
        no_path.path = String::new();
        assert_eq!(finish(no_path, NOW).unwrap().path, "/");
    }

    #[test]
    fn an_expiry_in_milliseconds_is_read_as_seconds_and_nothing_lasts_more_than_thirty_days() {
        let mut ms = raw("a", "1");
        ms.expiry = (NOW + 60) * 1000;
        assert_eq!(finish(ms, NOW).unwrap().expires, (NOW + 60) as f64);
        let mut old_ms = raw("a", "1");
        old_ms.expiry = (NOW - 60) * 1000;
        assert!(finish(old_ms, NOW).is_none());
        let mut long = raw("a", "1");
        long.expiry = NOW + 400 * 24 * 60 * 60;
        assert_eq!(finish(long, NOW).unwrap().expires, (NOW + MAX_AGE) as f64);
        let mut huge = raw("a", "1");
        huge.expiry = i64::MAX;
        assert_eq!(finish(huge, NOW).unwrap().expires, (NOW + MAX_AGE) as f64);
    }

    #[test]
    fn same_site_none_is_kept_only_on_a_secure_cookie() {
        let with = |same_site: i64, secure: bool| {
            let mut r = raw("a", "1");
            (r.same_site, r.secure) = (same_site, secure);
            finish(r, NOW).unwrap().same_site
        };
        assert_eq!(with(0, true), Some("None"));
        assert_eq!(with(0, false), None);
        assert_eq!(with(1, false), Some("Lax"));
        assert_eq!(with(2, true), Some("Strict"));
        assert_eq!(with(256, true), None);
        assert_eq!(with(-1, true), None);
    }

    #[cfg(windows)]
    mod database {
        use super::*;
        use rusqlite::{Connection, params};

        const TABLE: &str = "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, originAttributes TEXT NOT NULL DEFAULT '', name TEXT, \
            value TEXT, host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER, creationTime INTEGER, isSecure INTEGER, \
            isHttpOnly INTEGER, sameSite INTEGER)";

        /// (host, name, value, path, expiry, secure, same_site) with no container.
        type Row = (&'static str, &'static str, &'static str, &'static str, i64, bool, i64);

        fn insert(db: &Connection, origin: &str, r: Row) {
            db.execute(
                "INSERT INTO moz_cookies (originAttributes, name, value, host, path, expiry, isSecure, isHttpOnly, sameSite) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                params![origin, r.1, r.2, r.0, r.3, r.4, r.5, r.6],
            )
            .unwrap();
        }

        fn fixture(dir: &Path, rows: &[Row]) -> Profile {
            let folder = dir.join("p.default");
            fs::create_dir_all(&folder).unwrap();
            let db = Connection::open(folder.join(DB)).unwrap();
            db.execute(TABLE, []).unwrap();
            for r in rows {
                insert(&db, "", *r);
            }
            drop(db);
            profile_of(&folder)
        }

        fn profile_of(folder: &Path) -> Profile {
            Profile {
                id: "firefox/p.default".to_string(),
                name: "default".to_string(),
                last_used: true,
                db: folder.join(DB),
                candidate: true,
                modified: UNIX_EPOCH,
            }
        }

        const LATER: i64 = NOW + 1000;

        #[test]
        fn the_sites_of_a_database_are_grouped_counted_and_listed_with_the_sensitive_ones_first() {
            let dir = tempfile::tempdir().unwrap();
            let profile = fixture(
                dir.path(),
                &[
                    (".itau.com.br", "a", "1", "/", LATER, true, 1),
                    (".itau.com.br", "b", "2", "/", LATER, true, 1),
                    ("meubanco.com.br", "a", "1", "/", LATER * 1000, true, 1),
                    ("mail.google.com", "a", "1", "/", LATER, true, 0),
                    ("accounts.google.com", "a", "1", "/", LATER, true, 0),
                    ("www.example.com", "a", "1", "/", LATER, false, 0),
                    (".example.com", "a", "1", "/", LATER, false, 0),
                    ("shop.example.com", "a", "1", "/", LATER, false, 0),
                    ("shop.example.com", "b", "1", "/x", LATER, false, 0),
                    ("localhost", "a", "1", "/", LATER, false, 0),
                    ("192.168.1.1", "a", "1", "/", LATER, false, 0),
                    ("gone.example.org", "a", "1", "/", NOW - 5, false, 0),
                    ("gone-ms.example.org", "a", "1", "/", (NOW - 5) * 1000, false, 0),
                    ("plain.dev", "a", "1", "/", LATER, false, 0),
                ],
            );
            let db = Connection::open(&profile.db).unwrap();
            insert(&db, "^userContextId=2", ("work.example.net", "a", "1", "/", LATER, false, 0));
            insert(&db, "^partitionKey=%28https%2Cexample.com%29", ("part.example.net", "a", "1", "/", LATER, false, 0));
            drop(db);

            let sites = sites(&profile, NOW).unwrap();
            let summary: Vec<(&str, u32, Option<Kind>)> = sites.iter().map(|s| (s.name.as_str(), s.cookies, s.kind)).collect();
            assert_eq!(
                summary,
                [
                    ("accounts.google.com", 1, Some(Kind::SignIn)),
                    ("mail.google.com", 1, Some(Kind::Email)),
                    ("itau.com.br", 2, Some(Kind::Money)),
                    ("meubanco.com.br", 1, Some(Kind::Money)),
                    ("example.com", 4, None),
                    ("plain.dev", 1, None),
                ],
                "sign-ins, then mail, then money, then by cookie count; no containers, no expired, no addresses"
            );
            let example = sites.iter().find(|s| s.name == "example.com").unwrap();
            assert_eq!(example.hosts.len(), 3);
        }

        #[test]
        fn the_cookies_of_the_asked_hosts_come_back_with_their_flags_and_nothing_else() {
            let dir = tempfile::tempdir().unwrap();
            let profile = fixture(
                dir.path(),
                &[
                    (".example.com", "sid", "secret-1", "/", LATER, true, 2),
                    (".example.com", "lax", "v", "", LATER, false, 1),
                    (".example.com", "none", "v", "/a", LATER, true, 0),
                    (".example.com", "none-insecure", "v", "/a", LATER, false, 0),
                    (".example.com", "old", "v", "/", NOW - 1, false, 1),
                    (".example.com", "ms", "v", "/", LATER * 1000, false, 1),
                    (".example.com", "far", "v", "/", NOW + 90 * 24 * 60 * 60, false, 1),
                    (".example.com", "semi", "a;b", "/", LATER, false, 1),
                    (".example.com", "", "v", "/", LATER, false, 1),
                    ("www.example.com", "other", "v", "/", LATER, false, 1),
                    (".other.org", "elsewhere", "v", "/", LATER, false, 1),
                ],
            );
            let db = Connection::open(&profile.db).unwrap();
            insert(&db, "^userContextId=1", (".example.com", "container", "v", "/", LATER, false, 1));
            drop(db);

            let got = cookies(&profile, &[".example.com".to_string()], NOW).unwrap();
            let mut names: Vec<&str> = got.iter().map(|c| c.name.as_str()).collect();
            names.sort_unstable();
            assert_eq!(names, ["far", "lax", "ms", "none", "none-insecure", "sid"]);
            let get = |n: &str| got.iter().find(|c| c.name == n).unwrap();
            assert_eq!(get("sid").value, "secret-1");
            assert_eq!((get("sid").secure, get("sid").http_only, get("sid").same_site), (true, true, Some("Strict")));
            assert_eq!(get("lax").path, "/");
            assert_eq!(get("lax").same_site, Some("Lax"));
            assert_eq!(get("none").same_site, Some("None"));
            assert_eq!(get("none-insecure").same_site, None);
            assert_eq!(get("ms").expires, LATER as f64);
            assert_eq!(get("far").expires, (NOW + MAX_AGE) as f64);

            let two = cookies(&profile, &[".other.org".to_string(), "www.example.com".to_string()], NOW).unwrap();
            assert_eq!(two.len(), 2);
            assert!(cookies(&profile, &[], NOW).unwrap().is_empty());
            assert!(cookies(&profile, &["nothing.example".to_string()], NOW).unwrap().is_empty());
        }

        #[test]
        fn more_hosts_than_one_query_holds_are_read_in_chunks() {
            let dir = tempfile::tempdir().unwrap();
            let profile = fixture(dir.path(), &[]);
            let db = Connection::open(&profile.db).unwrap();
            let asked: Vec<String> = (0..1200).map(|n| format!("h{n}.example.com")).collect();
            for host in &asked {
                db.execute(
                    "INSERT INTO moz_cookies (name, value, host, path, expiry, isSecure, isHttpOnly, sameSite) VALUES ('a', 'v', ?1, '/', ?2, 0, 0, 1)",
                    params![host, LATER],
                )
                .unwrap();
            }
            drop(db);
            assert_eq!(cookies(&profile, &asked, NOW).unwrap().len(), 1200);
        }

        #[test]
        fn a_database_whose_newest_rows_wait_in_the_wal_file_is_read_without_touching_the_original() {
            let dir = tempfile::tempdir().unwrap();
            let folder = dir.path().join("p.default");
            fs::create_dir_all(&folder).unwrap();
            let writer = Connection::open(folder.join(DB)).unwrap();
            let mode: String = writer.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0)).unwrap();
            assert_eq!(mode, "wal");
            writer.execute_batch("PRAGMA wal_autocheckpoint=0").unwrap();
            writer.execute(TABLE, []).unwrap();
            insert(&writer, "", (".example.com", "sid", "in-the-wal", "/", LATER, false, 1));
            insert(&writer, "", ("www.news.org", "a", "1", "/", LATER, false, 1));
            let wal = folder.join("cookies.sqlite-wal");
            assert!(fs::metadata(&wal).unwrap().len() > 0, "the rows are in the -wal file");
            let main_before = fs::read(folder.join(DB)).unwrap();
            let profile = profile_of(&folder);

            // The writer is still open, as Firefox is.
            let listed = sites(&profile, NOW).unwrap();
            assert_eq!(listed.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["example.com", "news.org"]);
            let got = cookies(&profile, &[".example.com".to_string()], NOW).unwrap();
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].value, "in-the-wal");

            assert_eq!(fs::read(folder.join(DB)).unwrap(), main_before, "the original database is never written");
            drop(writer);
        }

        #[test]
        fn a_missing_broken_or_oversized_database_answers_with_its_own_key() {
            let dir = tempfile::tempdir().unwrap();
            let folder = dir.path().join("p.default");
            fs::create_dir_all(&folder).unwrap();
            let profile = profile_of(&folder);
            assert_eq!(sites(&profile, NOW).err(), Some("error.cookies.missing"));
            assert_eq!(cookies(&profile, &[".a.com".to_string()], NOW).err(), Some("error.cookies.missing"));

            fs::write(folder.join(DB), b"this is not a database at all, and it never was one").unwrap();
            assert_eq!(sites(&profile, NOW).err(), Some("error.cookies.locked"));
            assert_eq!(cookies(&profile, &[".a.com".to_string()], NOW).err(), Some("error.cookies.locked"));

            let db = Connection::open(folder.join("other.sqlite")).unwrap();
            db.execute("CREATE TABLE unrelated (x)", []).unwrap();
            drop(db);
            fs::rename(folder.join("other.sqlite"), folder.join(DB)).unwrap();
            assert_eq!(sites(&profile, NOW).err(), Some("error.cookies.locked"), "no moz_cookies table");

            let big = fixture(&dir.path().join("big"), &[]);
            fs::File::options().write(true).open(&big.db).unwrap().set_len(MAX_DB + 1).unwrap();
            assert_eq!(sites(&big, NOW).err(), Some("error.cookies.tooBig"));

            let wal_folder = dir.path().join("wal");
            let wal_profile = fixture(&wal_folder, &[]);
            let wal = wal_of(&wal_profile.db);
            fs::File::create(&wal).unwrap().set_len(MAX_DB + 1).unwrap();
            assert_eq!(sites(&wal_profile, NOW).err(), Some("error.cookies.tooBig"));
        }
    }
}
