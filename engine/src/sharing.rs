//! The wire format for optional anonymous sharing. This module has no transport or collection side effects.
//!
//! Values taken from outside the program are accepted only through the constructors below. In particular, a crash's
//! message, thread and full path have no representation in the report.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const KEYS: &str = include_str!("sharing-errors.txt");
const SOURCES: &str = include_str!("sharing-sources.txt");
const MAX_CRASHES: usize = 16;
const MAX_FAILURE_KEYS: usize = 128;
const MAX_QUEUED: usize = 32;

pub fn build_commit() -> &'static str {
    option_env!("FABER_COMMIT").unwrap_or("unknown")
}

pub fn build_version() -> String {
    let package: serde_json::Value = serde_json::from_str(include_str!("../../package.json")).unwrap_or_default();
    package["version"].as_str().unwrap_or("other").to_owned()
}

/// Three numeric components and an optional numeric preview suffix; other shapes become `other`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Version(String);

impl Version {
    pub fn parse(value: &str) -> Self {
        let (base, preview) = value.split_once("-preview.").map_or((value, None), |(base, number)| (base, Some(number)));
        let parts: Vec<_> = base.split('.').collect();
        let number = |p: &str| !p.is_empty() && p.len() <= 4 && p.bytes().all(|c| c.is_ascii_digit());
        if parts.len() == 3 && parts.iter().all(|p| number(p)) && preview.is_none_or(number) {
            Self(value.to_owned())
        } else {
            Self("other".into())
        }
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// A commit identifies Faber's build, never a person's installation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Commit(String);

impl Commit {
    pub fn parse(value: &str) -> Self {
        if (7..=12).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_hexdigit()) {
            Self(value.to_ascii_lowercase())
        } else {
            Self("unknown".into())
        }
    }
}

impl<'de> Deserialize<'de> for Commit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// Only a name of a source file shipped in Faber's source tree can leave the computer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SourceFile(String);

impl SourceFile {
    pub fn parse(value: &str) -> Self {
        let name = value.rsplit(['/', '\\']).next().unwrap_or("");
        if name.len() <= 64 && SOURCES.lines().any(|known| known == name) { Self(name.to_owned()) } else { Self("other".into()) }
    }
}

impl<'de> Deserialize<'de> for SourceFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// Only an error or reason key compiled into Faber can be counted; values are never accepted as arbitrary text.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FailureKey(String);

impl FailureKey {
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > 80 || !(value.starts_with("error.") || value.starts_with("reason.")) {
            return None;
        }
        let known = KEYS.lines().any(|key| key == value);
        known.then(|| Self(value.to_owned()))
    }
}

impl<'de> Deserialize<'de> for FailureKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).ok_or_else(|| serde::de::Error::custom("unknown sharing failure key"))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Stable,
    Preview,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Architecture {
    X86_64,
    Aarch64,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    En,
    #[serde(rename = "pt-BR")]
    PtBr,
    Es,
    Fr,
    De,
    Other,
}

impl Language {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "en" => Self::En,
            "pt-BR" => Self::PtBr,
            "es" => Self::Es,
            "fr" => Self::Fr,
            "de" => Self::De,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashKind {
    Panic,
    UncleanExit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crash {
    pub kind: CrashKind,
    pub version: Version,
    pub commit: Commit,
    pub source_file: SourceFile,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcomes {
    pub completed: u32,
    pub interrupted: u32,
    pub failed: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerCli<T> {
    pub claude: T,
    pub codex: T,
    pub gemini: T,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub app_starts: u32,
    pub chat_conversations: PerCli<u32>,
    pub project_conversations: PerCli<u32>,
    pub turns: PerCli<Outcomes>,
    pub failures: BTreeMap<FailureKey, u32>,
    pub update_installed: u32,
    pub update_failed: u32,
}

/// The entire wire format. There is no field for arbitrary text or any installation identifier.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub version: Version,
    pub commit: Commit,
    pub channel: Channel,
    pub os: Os,
    pub os_major: Option<u16>,
    pub architecture: Architecture,
    pub language: Language,
    pub days_since_last_report: Option<u16>,
    pub days_since_last_use: Option<u16>,
    pub counters: Counters,
    pub cli_versions: PerCli<Version>,
    #[serde(deserialize_with = "read_crashes")]
    crashes: Vec<Crash>,
}

fn read_crashes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Crash>, D::Error> {
    let mut crashes = Vec::<Crash>::deserialize(deserializer)?;
    crashes.truncate(MAX_CRASHES);
    Ok(crashes)
}

/// A build without a valid fixed HTTPS endpoint cannot enable sharing.
pub fn endpoint() -> Option<&'static str> {
    static ENDPOINT: OnceLock<Option<&'static str>> = OnceLock::new();
    *ENDPOINT.get_or_init(|| checked_endpoint(option_env!("FABER_TELEMETRY_URL")?))
}

fn checked_endpoint(value: &'static str) -> Option<&'static str> {
    if !value.starts_with("https://") {
        return None;
    }
    let url = reqwest::Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(value)
}

impl Report {
    pub fn new(version: &str, commit: &str, channel: Channel, os: Os, architecture: Architecture, language: Language) -> Self {
        Self {
            version: Version::parse(version),
            commit: Commit::parse(commit),
            channel,
            os,
            os_major: None,
            architecture,
            language,
            days_since_last_report: None,
            days_since_last_use: None,
            counters: Counters::default(),
            cli_versions: PerCli { claude: Version::parse(""), codex: Version::parse(""), gemini: Version::parse("") },
            crashes: Vec::new(),
        }
    }

    pub fn add_crash(&mut self, crash: Crash) {
        if self.crashes.len() < MAX_CRASHES {
            self.crashes.push(crash);
        }
    }

    pub fn environment(language: Language) -> Self {
        let version = build_version();
        let channel = match option_env!("FABER_RELEASE_CHANNEL") {
            Some("stable") => Channel::Stable,
            Some("preview") => Channel::Preview,
            _ if Version::parse(&version).0 == "other" => Channel::Other,
            _ if version.contains("-preview.") => Channel::Preview,
            _ => Channel::Stable,
        };
        let os = match std::env::consts::OS {
            "windows" => Os::Windows,
            "macos" => Os::Macos,
            "linux" => Os::Linux,
            _ => Os::Other,
        };
        let architecture = match std::env::consts::ARCH {
            "x86_64" => Architecture::X86_64,
            "aarch64" => Architecture::Aarch64,
            _ => Architecture::Other,
        };
        let mut report = Self::new(&version, build_commit(), channel, os, architecture, language);
        report.os_major = crate::crash::os_major();
        report
    }
}

impl<T> PerCli<T> {
    fn vendor_mut(&mut self, vendor: crate::model::Vendor) -> &mut T {
        match vendor {
            crate::model::Vendor::Claude => &mut self.claude,
            crate::model::Vendor::Codex => &mut self.codex,
            crate::model::Vendor::Gemini => &mut self.gemini,
        }
    }
}

const STATE_MAX: u64 = 128 * 1024;
const REPORT_MAX: usize = 32 * 1024;
const DAY: u64 = 86_400;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stored {
    consent: Option<bool>,
    #[serde(default)]
    language: Option<Language>,
    active: Option<Report>,
    pending: Option<Report>,
    attempts: u8,
    last_report_day: Option<u64>,
    last_use_day: Option<u64>,
    last_attempt_day: Option<u64>,
    last_crash_attempt_day: Option<u64>,
    crash_cursor: u64,
}

/// A private local choice and bounded, typed reports. Disk writes use the same atomic replacement as other JSON files.
pub struct Sharing {
    path: PathBuf,
    available: bool,
    endpoint: Option<&'static str>,
    enabled: AtomicBool,
    choice: AtomicU8,
    generation: AtomicU64,
    queued: AtomicUsize,
    cancel: tokio::sync::watch::Sender<u64>,
    finished: tokio::sync::watch::Sender<u64>,
    sending: AtomicBool,
    crash_waiting: AtomicBool,
    transport: Arc<dyn Transport>,
    state: Mutex<Stored>,
    seen_turns: Mutex<VecDeque<(String, crate::model::Vendor)>>,
    language: Language,
}

impl Sharing {
    fn gate(&self) -> Option<u64> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.enabled.load(Ordering::SeqCst).then_some(generation)
    }

    fn permits(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
            && self.enabled.load(Ordering::SeqCst)
            && self.generation.load(Ordering::SeqCst) == generation
    }

    pub fn open(dir: &Path, language: Option<&str>) -> Arc<Self> {
        let language = match language {
            Some("en") => Language::En,
            Some("pt-BR") => Language::PtBr,
            Some("es") => Language::Es,
            Some("fr") => Language::Fr,
            Some("de") => Language::De,
            _ => Language::Other,
        };
        Self::open_with_endpoint(dir, language, endpoint(), Arc::new(HttpsTransport))
    }

    fn open_with_endpoint(dir: &Path, language: Language, endpoint: Option<&'static str>, transport: Arc<dyn Transport>) -> Arc<Self> {
        let available = endpoint.is_some();
        let path = dir.join("sharing.json");
        let state = if available {
            use std::io::Read as _;
            std::fs::File::open(&path)
                .ok()
                .and_then(|file| {
                    let mut bytes = Vec::new();
                    file.take(STATE_MAX + 1).read_to_end(&mut bytes).ok()?;
                    if bytes.len() as u64 > STATE_MAX {
                        return None;
                    }
                    serde_json::from_slice::<Stored>(&bytes).ok()
                })
                .unwrap_or_default()
        } else {
            Stored::default()
        };
        Arc::new(Self {
            path,
            available,
            endpoint,
            enabled: AtomicBool::new(available && state.consent == Some(true)),
            choice: AtomicU8::new(match state.consent {
                Some(true) => 1,
                Some(false) => 2,
                None => 0,
            }),
            generation: AtomicU64::new(0),
            queued: AtomicUsize::new(0),
            cancel: tokio::sync::watch::channel(0).0,
            finished: tokio::sync::watch::channel(0).0,
            sending: AtomicBool::new(false),
            crash_waiting: AtomicBool::new(false),
            transport,
            state: Mutex::new(state),
            seen_turns: Mutex::new(VecDeque::new()),
            language,
        })
    }

    fn save(&self, state: &Stored) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec(state).map_err(|_| "error.store.write")?;
        if bytes.len() as u64 > STATE_MAX {
            return Err("error.store.write");
        }
        crate::store::replace(&crate::store::RealDisk, &self.path, &bytes).map_err(|fault| fault.key())
    }

    fn new_report(&self, state: &Stored, day: u64) -> Report {
        let mut report = Report::environment(state.language.unwrap_or(self.language));
        report.days_since_last_report = state.last_report_day.map(|previous| day.saturating_sub(previous).min(u16::MAX as u64) as u16);
        report.days_since_last_use = state.last_use_day.map(|previous| day.saturating_sub(previous).min(u16::MAX as u64) as u16);
        report
    }

    pub fn choice(&self) -> Option<bool> {
        if !self.available {
            return None;
        }
        match self.choice.load(Ordering::SeqCst) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    pub fn set_language(&self, tag: &str) -> Result<(), &'static str> {
        let Some(generation) = self.gate() else { return Ok(()) };
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.consent != Some(true) || !self.permits(generation) {
            return Ok(());
        }
        let mut next = state.clone();
        let language = Language::from_tag(tag);
        if state.language == Some(language) {
            return Ok(());
        }
        next.language = Some(language);
        if let Some(report) = &mut next.active {
            report.language = language;
        }
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// The caller runs this away from the screen; a failed write leaves the previous choice and reports intact.
    pub fn set_choice(&self, yes: bool, language: Option<&str>) -> Result<(), &'static str> {
        if !self.available {
            return Err("error.host.unknownCommand");
        }
        if !yes {
            self.enabled.store(false, Ordering::SeqCst);
            self.crash_waiting.store(false, Ordering::SeqCst);
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.cancel.send_modify(|value| *value = value.wrapping_add(1));
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = if yes && state.consent == Some(true) {
            state.clone()
        } else {
            Stored { last_attempt_day: state.last_attempt_day, last_crash_attempt_day: state.last_crash_attempt_day, ..Stored::default() }
        };
        if yes && state.consent != Some(true) {
            next.crash_cursor =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "error.store.write")?.as_millis() as u64;
        }
        if yes && let Some(language) = language {
            let language = Language::from_tag(language);
            next.language = Some(language);
            if let Some(report) = &mut next.active {
                report.language = language;
            }
        }
        next.consent = Some(yes);
        if let Err(error) = self.save(&next) {
            self.enabled.store(state.consent == Some(true), Ordering::SeqCst);
            return Err(error);
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        *state = next;
        if !yes {
            self.seen_turns.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        self.choice.store(if yes { 1 } else { 2 }, Ordering::SeqCst);
        self.enabled.store(yes, Ordering::SeqCst);
        Ok(())
    }

    /// Queues work only if consent was already given when the event happened. A later choice invalidates queued work.
    pub fn record(self: &Arc<Self>, runtime: &tokio::runtime::Handle, update: impl FnOnce(&mut Report) + Send + 'static) {
        let Some(generation) = self.gate() else { return };
        self.record_for(runtime, generation, update);
    }

    fn record_for(self: &Arc<Self>, runtime: &tokio::runtime::Handle, generation: u64, update: impl FnOnce(&mut Report) + Send + 'static) {
        if !self.permits(generation)
            || self.queued.try_update(Ordering::SeqCst, Ordering::SeqCst, |count| (count < MAX_QUEUED).then_some(count + 1)).is_err()
        {
            return;
        }
        struct Queued(Arc<Sharing>);
        impl Drop for Queued {
            fn drop(&mut self) {
                self.0.queued.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let queued = Queued(Arc::clone(self));
        let sharing = Arc::clone(self);
        let runtime_for_send = runtime.clone();
        runtime.spawn_blocking(move || {
            let _queued = queued;
            let mut state = sharing.state.lock().unwrap_or_else(|e| e.into_inner());
            if !sharing.permits(generation) || state.consent != Some(true) {
                return;
            }
            let mut next = state.clone();
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let day = now / DAY;
            if next.active.is_none() {
                next.active = Some(sharing.new_report(&next, day));
            }
            update(next.active.as_mut().expect("active was just created"));
            next.last_use_day = Some(day);
            let saved = sharing.save(&next).is_ok();
            if saved {
                *state = next;
            }
            drop(state);
            if saved {
                sharing.schedule_for(generation, &runtime_for_send, now, false, Arc::clone(&sharing.transport));
            }
        });
    }

    pub fn conversation(self: &Arc<Self>, runtime: &tokio::runtime::Handle, area: crate::model::Area, vendor: crate::model::Vendor) {
        self.record(runtime, move |report| {
            let counters = match area {
                crate::model::Area::Chat => &mut report.counters.chat_conversations,
                crate::model::Area::Project => &mut report.counters.project_conversations,
            };
            let value = counters.vendor_mut(vendor);
            *value = value.saturating_add(1);
        });
    }

    pub fn turn(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        turn_id: &str,
        vendor: crate::model::Vendor,
        status: crate::model::Status,
    ) {
        let Some(generation) = self.gate() else { return };
        if turn_id.is_empty() || turn_id.len() > 128 {
            return;
        }
        let mut seen = self.seen_turns.lock().unwrap_or_else(|e| e.into_inner());
        if !self.permits(generation) || seen.iter().any(|(id, cli)| id == turn_id && *cli == vendor) {
            return;
        }
        if seen.len() == 256 {
            seen.pop_front();
        }
        seen.push_back((turn_id.to_owned(), vendor));
        drop(seen);
        self.record_for(runtime, generation, move |report| {
            let outcomes = report.counters.turns.vendor_mut(vendor);
            let counter = match status {
                crate::model::Status::Done => &mut outcomes.completed,
                crate::model::Status::Stopped => &mut outcomes.interrupted,
                crate::model::Status::Error => &mut outcomes.failed,
                crate::model::Status::Streaming => return,
            };
            *counter = counter.saturating_add(1);
        });
    }

    pub fn failure(self: &Arc<Self>, runtime: &tokio::runtime::Handle, key: &str) {
        let Some(generation) = self.gate() else { return };
        if let Some(key) = FailureKey::parse(key) {
            self.record_for(runtime, generation, move |report| {
                if !report.counters.failures.contains_key(&key) && report.counters.failures.len() >= MAX_FAILURE_KEYS {
                    return;
                }
                let value = report.counters.failures.entry(key).or_default();
                *value = value.saturating_add(1);
            });
        }
    }

    pub fn update(self: &Arc<Self>, runtime: &tokio::runtime::Handle, installed: bool) {
        self.record(runtime, move |report| {
            let count = if installed { &mut report.counters.update_installed } else { &mut report.counters.update_failed };
            *count = count.saturating_add(1);
        });
    }

    fn collect_crashes(&self, generation: u64, now_ms: u64) -> bool {
        if !self.permits(generation) {
            return false;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.consent != Some(true) || !self.permits(generation) {
            return false;
        }
        let Some(dir) = self.path.parent() else { return false };
        let Ok(records) = crate::crash::since(&dir.join(crate::crash::CRASHES), state.crash_cursor, now_ms) else { return false };
        let mut next = state.clone();
        let language = next.language.unwrap_or(self.language);
        let mut found = false;
        for record in records {
            if record.at > now_ms {
                continue;
            }
            let kind = match record.kind.as_str() {
                "panic" => CrashKind::Panic,
                "unclean-exit" => CrashKind::UncleanExit,
                _ => continue,
            };
            let mut parts = record.location.rsplitn(3, ':');
            let _column = parts.next();
            let line = parts.next().and_then(|value| value.parse::<u32>().ok());
            let file = parts.next().unwrap_or("");
            next.active.get_or_insert_with(|| Report::environment(language)).add_crash(Crash {
                kind,
                version: Version::parse(&record.version),
                commit: Commit::parse(&record.commit),
                source_file: SourceFile::parse(file),
                line,
            });
            found = true;
        }
        use std::io::Read as _;
        let update_log = std::fs::File::open(dir.join("update.log")).ok().and_then(|file| {
            let mut text = String::new();
            file.take(256 * 1024).read_to_string(&mut text).ok().map(|_| text)
        });
        if let Some(text) = update_log {
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                let Some(at) = parts.next().and_then(|value| value.parse::<u64>().ok()) else { continue };
                if at.saturating_mul(1000) <= next.crash_cursor || at.saturating_mul(1000) > now_ms {
                    continue;
                }
                let counter = match parts.next() {
                    Some("installed") => &mut next.active.get_or_insert_with(|| Report::environment(language)).counters.update_installed,
                    Some("install-failed") => &mut next.active.get_or_insert_with(|| Report::environment(language)).counters.update_failed,
                    _ => continue,
                };
                *counter = counter.saturating_add(1);
            }
        }
        next.crash_cursor = now_ms;
        if self.save(&next).is_ok() {
            *state = next;
            found
        } else {
            false
        }
    }

    /// Freezes and returns exactly the bytes of the next report. Events after this point go into the next active report.
    pub fn preview(&self) -> Result<Option<String>, &'static str> {
        let Some(generation) = self.gate() else { return Ok(None) };
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.consent != Some(true) || !self.permits(generation) {
            return Ok(None);
        }
        let mut next = state.clone();
        if next.pending.is_none() {
            let day = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) / DAY;
            let report = match next.active.take() {
                Some(report) => report,
                None => self.new_report(&next, day),
            };
            next.pending = Some(report);
        }
        let bytes = serde_json::to_string(next.pending.as_ref().expect("pending was just created")).map_err(|_| "error.store.write")?;
        if bytes.len() > REPORT_MAX {
            return Err("error.store.write");
        }
        self.save(&next)?;
        *state = next;
        Ok(Some(bytes))
    }

    /// Counts a start and remembers only the number of days since the last use and last successful report.
    pub fn started(self: &Arc<Self>, runtime: &tokio::runtime::Handle, now_ms: u64) {
        let Some(generation) = self.gate() else { return };
        let sharing = Arc::clone(self);
        let runtime_for_send = runtime.clone();
        runtime.spawn_blocking(move || {
            let mut state = sharing.state.lock().unwrap_or_else(|e| e.into_inner());
            if !sharing.permits(generation) || state.consent != Some(true) {
                return;
            }
            let mut next = state.clone();
            let day = now_ms / 1000 / DAY;
            let report_days = next.last_report_day.map(|previous| day.saturating_sub(previous).min(u16::MAX as u64) as u16);
            let use_days = next.last_use_day.map(|previous| day.saturating_sub(previous).min(u16::MAX as u64) as u16);
            let language = next.language.unwrap_or(sharing.language);
            let report = next.active.get_or_insert_with(|| Report::environment(language));
            report.days_since_last_report = report_days;
            report.days_since_last_use = use_days;
            report.counters.app_starts = report.counters.app_starts.saturating_add(1);
            next.last_use_day = Some(day);
            if sharing.save(&next).is_ok() {
                *state = next;
            }
            drop(state);
            let crashed = sharing.collect_crashes(generation, now_ms);
            sharing.schedule_for(generation, &runtime_for_send, now_ms / 1000, crashed, Arc::clone(&sharing.transport));
        });
    }

    fn prepare(&self, generation: u64, day: u64, after_crash: bool) -> Result<Option<(Vec<u8>, u64)>, &'static str> {
        if !self.permits(generation) {
            return Ok(None);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.consent != Some(true) || !self.permits(generation) {
            return Ok(None);
        }
        let crash_exception = after_crash && state.last_crash_attempt_day != Some(day);
        if state.last_attempt_day == Some(day) && !crash_exception {
            return Ok(None);
        }
        let mut next = state.clone();
        if next.pending.is_some() && next.attempts >= 2 {
            next.pending = None;
            next.attempts = 0;
            self.save(&next)?;
            *state = next;
            return Ok(None);
        }
        if next.pending.is_none() {
            let report = match next.active.take() {
                Some(report) => report,
                None => self.new_report(&next, day),
            };
            next.pending = Some(report);
            next.attempts = 0;
        }
        let body = serde_json::to_vec(next.pending.as_ref().expect("pending was just created")).map_err(|_| "error.store.write")?;
        if body.len() > REPORT_MAX {
            return Err("error.store.write");
        }
        next.attempts = next.attempts.saturating_add(1);
        next.last_attempt_day = Some(day);
        if crash_exception {
            next.last_crash_attempt_day = Some(day);
        }
        self.save(&next)?;
        *state = next;
        Ok(Some((body, generation)))
    }

    fn finish_attempt(&self, generation: u64, day: u64, accepted: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !self.permits(generation) || state.consent != Some(true) {
            return;
        }
        let mut next = state.clone();
        if accepted || next.attempts >= 2 {
            next.pending = None;
            next.attempts = 0;
        }
        if accepted {
            next.last_report_day = Some(day);
        }
        if self.save(&next).is_ok() {
            *state = next;
        }
    }

    /// Starts at most one request in the background. Failures are retried on a later eligible start at most once.
    pub fn schedule(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        now_seconds: u64,
        after_crash: bool,
        transport: Arc<dyn Transport>,
    ) {
        let Some(generation) = self.gate() else { return };
        self.schedule_for(generation, runtime, now_seconds, after_crash, transport);
    }

    fn schedule_for(
        self: &Arc<Self>,
        generation: u64,
        runtime: &tokio::runtime::Handle,
        now_seconds: u64,
        after_crash: bool,
        transport: Arc<dyn Transport>,
    ) {
        if !self.permits(generation) {
            return;
        }
        if after_crash {
            self.crash_waiting.store(true, Ordering::SeqCst);
        } else if self.crash_waiting.load(Ordering::SeqCst) {
            return;
        }
        if !self.permits(generation) {
            if after_crash {
                self.crash_waiting.store(false, Ordering::SeqCst);
            }
            return;
        }
        let mut finished = self.finished.subscribe();
        if self.sending.swap(true, Ordering::SeqCst) {
            if after_crash {
                let sharing = Arc::clone(self);
                runtime.spawn(async move {
                    while finished.changed().await.is_ok() && sharing.permits(generation) {
                        if !sharing.sending.swap(true, Ordering::SeqCst) {
                            sharing.crash_waiting.store(false, Ordering::SeqCst);
                            Arc::clone(&sharing).send_once(generation, now_seconds, true, transport).await;
                            break;
                        }
                    }
                    if !sharing.permits(generation) {
                        sharing.crash_waiting.store(false, Ordering::SeqCst);
                    }
                });
            }
            return;
        }
        if after_crash {
            self.crash_waiting.store(false, Ordering::SeqCst);
        } else if self.crash_waiting.load(Ordering::SeqCst) {
            self.sending.store(false, Ordering::SeqCst);
            self.finished.send_modify(|value| *value = value.wrapping_add(1));
            return;
        }
        runtime.spawn(Arc::clone(self).send_once(generation, now_seconds, after_crash, transport));
    }

    async fn send_once(self: Arc<Self>, generation: u64, now_seconds: u64, after_crash: bool, transport: Arc<dyn Transport>) {
        struct InFlight(Arc<Sharing>);
        impl Drop for InFlight {
            fn drop(&mut self) {
                self.0.sending.store(false, Ordering::SeqCst);
                self.0.finished.send_modify(|value| *value = value.wrapping_add(1));
            }
        }
        let _flight = InFlight(Arc::clone(&self));
        let mut cancelled = self.cancel.subscribe();
        let day = now_seconds / DAY;
        let prepare = Arc::clone(&self);
        let Ok(Ok(Some((body, generation)))) = tokio::task::spawn_blocking(move || prepare.prepare(generation, day, after_crash)).await
        else {
            return;
        };
        if !self.permits(generation) {
            return;
        }
        let Some(url) = self.endpoint else { return };
        let accepted = tokio::select! {
            biased;
            _ = cancelled.changed() => return,
            accepted = tokio::time::timeout(std::time::Duration::from_secs(10), transport.post(url, body)) => accepted.unwrap_or(false),
        };
        let finish = Arc::clone(&self);
        let _ = tokio::task::spawn_blocking(move || finish.finish_attempt(generation, day, accepted)).await;
    }
}

pub trait Transport: Send + Sync {
    fn post<'a>(&'a self, url: &'a str, body: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>>;
}

pub struct HttpsTransport;

impl Transport for HttpsTransport {
    fn post<'a>(&'a self, url: &'a str, body: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            crate::tls::install();
            let roots = rustls_native_certs::load_native_certs()
                .certs
                .into_iter()
                .filter_map(|certificate| reqwest::Certificate::from_der(certificate.as_ref()).ok());
            let Ok(client) = reqwest::Client::builder()
                .tls_certs_only(roots)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .referer(false)
                .timeout(std::time::Duration::from_secs(10))
                .build()
            else {
                return false;
            };
            client
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_values_are_bounded_and_cannot_add_fields() {
        assert_eq!(Version::parse("1.2.3").0, "1.2.3");
        assert_eq!(Version::parse("1.2.3-preview.1").0, "1.2.3-preview.1");
        assert_eq!(Version::parse("9999.9999.9999-preview.9999").0, "9999.9999.9999-preview.9999");
        for value in ["10000.2.3", "1.10000.3", "1.2.10000", "1.2.3-preview.10000"] {
            assert_eq!(Version::parse(value).0, "other");
        }
        assert_eq!(Version::parse("1.2.3-user@example.com").0, "other");
        assert_eq!(Commit::parse("secret-prompt").0, "unknown");
        assert_eq!(Commit::parse("abcdef123456").0, "abcdef123456");
        assert_eq!(Commit::parse("abcdef1234567").0, "unknown");
        assert_eq!(SourceFile::parse("C:/Users/secret/private.txt").0, "other");
        assert_eq!(SourceFile::parse("C:/Users/secret/crash.rs").0, "crash.rs");
        assert!(FailureKey::parse("error.settings.saveFailed").is_some());
        assert!(FailureKey::parse("error.secret-prompt").is_none());
        let crash = Crash {
            kind: CrashKind::Panic,
            version: Version::parse("1.2.3"),
            commit: Commit::parse("abcdef123456"),
            source_file: SourceFile::parse("crash.rs"),
            line: Some(3),
        };
        let value = serde_json::to_value(crash).unwrap();
        let keys: Vec<_> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["commit", "kind", "line", "sourceFile", "version"]);
        assert!(!value.to_string().contains("secret"));
    }

    #[test]
    fn endpoint_requires_a_literal_https_prefix_and_no_query_or_credentials() {
        assert_eq!(checked_endpoint("https://example.invalid/report"), Some("https://example.invalid/report"));
        for value in [
            "HTTPS://example.invalid",
            "http://example.invalid",
            "https://example.invalid/?x=1",
            "https://user@example.invalid/",
            "https://example.invalid/#part",
        ] {
            assert_eq!(checked_endpoint(value), None);
        }
    }

    #[test]
    fn report_serialization_has_only_the_named_fields_and_bounded_strings() {
        let mut report = Report::new("1.2.3", "abcdef123456", Channel::Stable, Os::Windows, Architecture::X86_64, Language::En);
        for _ in 0..100 {
            report.add_crash(Crash {
                kind: CrashKind::Panic,
                version: Version::parse("1.2.3"),
                commit: Commit::parse("abcdef123456"),
                source_file: SourceFile::parse("crash.rs"),
                line: Some(3),
            });
        }
        report.counters.failures.insert(FailureKey::parse("error.settings.saveFailed").unwrap(), 1);
        let value = serde_json::to_value(report).unwrap();
        let keys: Vec<_> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "architecture",
                "channel",
                "cliVersions",
                "commit",
                "counters",
                "crashes",
                "daysSinceLastReport",
                "daysSinceLastUse",
                "language",
                "os",
                "osMajor",
                "version"
            ]
        );
        assert_eq!(value["crashes"].as_array().unwrap().len(), MAX_CRASHES);
        fn strings_are_small(value: &serde_json::Value) {
            match value {
                serde_json::Value::String(s) => assert!(s.len() <= 80),
                serde_json::Value::Array(values) => values.iter().for_each(strings_are_small),
                serde_json::Value::Object(values) => {
                    for (key, value) in values {
                        assert!(key.len() <= 80);
                        strings_are_small(value);
                    }
                }
                _ => (),
            }
        }
        strings_are_small(&value);
    }

    #[test]
    fn sharing_page_lists_every_serialized_field() {
        let mut report = Report::new("1.2.3", "abcdef123456", Channel::Stable, Os::Windows, Architecture::X86_64, Language::En);
        report.add_crash(Crash {
            kind: CrashKind::Panic,
            version: Version::parse("1.2.3"),
            commit: Commit::parse("abcdef123456"),
            source_file: SourceFile::parse("crash.rs"),
            line: Some(1),
        });
        report.counters.failures.insert(FailureKey::parse("error.settings.saveFailed").unwrap(), 1);
        let page = include_str!("../../docs/product/sharing.md");
        fn fields(value: &serde_json::Value, page: &str) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, value) in map {
                        if !key.starts_with("error.") && !key.starts_with("reason.") {
                            assert!(page.contains(&format!("`{key}`")), "the sharing page omits {key}");
                        }
                        fields(value, page);
                    }
                }
                serde_json::Value::Array(values) => values.iter().for_each(|value| fields(value, page)),
                _ => (),
            }
        }
        fields(&serde_json::to_value(report).unwrap(), page);
    }

    #[test]
    fn failure_allowlist_matches_all_generated_keys() {
        use std::collections::BTreeSet;
        let locale = include_str!("../../ui/locales/en.ts");
        let mut expected: BTreeSet<String> = locale
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let quote = line.chars().next().filter(|c| *c == '\'' || *c == '"')?;
                let (key, tail) = line[1..].split_once(quote)?;
                ((key.starts_with("error.") || key.starts_with("reason.")) && key.len() <= 80 && tail.trim_start().starts_with(':'))
                    .then(|| key.to_owned())
            })
            .collect();
        fn collect(dir: &Path, expected: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    collect(&path, expected);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let text = std::fs::read_to_string(path).unwrap();
                    let mut remaining = text.as_str();
                    while let Some((_, after_quote)) = remaining.split_once('"') {
                        let Some((key, tail)) = after_quote.split_once('"') else { break };
                        let prefix = if key.starts_with("error.") {
                            "error."
                        } else if key.starts_with("reason.") {
                            "reason."
                        } else {
                            remaining = after_quote;
                            continue;
                        };
                        if key.len() > prefix.len() && key.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.')) {
                            if key.len() <= 80 {
                                expected.insert(key.to_owned());
                            }
                            remaining = tail;
                        } else {
                            remaining = after_quote;
                        }
                    }
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for dir in [
            "builds",
            "channel",
            "cli",
            "engine",
            "engine-core",
            "engine-drivers",
            "engine-projects",
            "engine-store",
            "host",
            "sidecar",
            "src-tauri",
            "update",
            "winproc",
        ] {
            collect(&root.join(dir).join("src"), &mut expected);
        }
        let actual: BTreeSet<String> = KEYS.lines().map(str::to_owned).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn source_allowlist_matches_the_program_sources() {
        use std::collections::BTreeSet;
        use std::path::Path;

        fn visit(dir: &Path, names: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    visit(&path, names);
                } else if let Some(name) = path.file_name().and_then(|name| name.to_str())
                    && name.ends_with(".rs")
                    && name.len() <= 64
                {
                    names.insert(name.to_owned());
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut expected = BTreeSet::new();
        for dir in [
            "builds",
            "channel",
            "cli",
            "engine",
            "engine-core",
            "engine-drivers",
            "engine-projects",
            "engine-store",
            "host",
            "sidecar",
            "src-tauri",
            "update",
            "winproc",
        ] {
            visit(&root.join(dir).join("src"), &mut expected);
        }
        let actual: BTreeSet<String> = SOURCES.lines().map(str::to_owned).collect();
        assert_eq!(actual, expected);
    }

    struct FakeTransport(std::sync::atomic::AtomicUsize);
    impl Transport for FakeTransport {
        fn post<'a>(&'a self, _: &'a str, _: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                false
            })
        }
    }

    struct SignalledTransport(Mutex<Option<tokio::sync::oneshot::Sender<Vec<u8>>>>);
    impl Transport for SignalledTransport {
        fn post<'a>(&'a self, _: &'a str, body: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async move {
                if let Some(sender) = self.0.lock().unwrap().take() {
                    let _ = sender.send(body);
                }
                true
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn enabled_sender_posts_the_preview_bytes_through_the_injected_transport() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let transport = Arc::new(SignalledTransport(Mutex::new(Some(tx))));
        let sharing = Sharing::open_with_endpoint(dir.path(), Language::En, Some("https://example.invalid"), transport.clone());
        sharing.set_choice(true, None).unwrap();
        let preview = sharing.preview().unwrap().unwrap();
        sharing.schedule(&tokio::runtime::Handle::current(), DAY, false, transport);
        let sent = tokio::time::timeout(std::time::Duration::from_secs(5), rx).await.unwrap().unwrap();
        assert_eq!(sent.as_slice(), preview.as_bytes());
    }

    #[test]
    fn crash_message_and_user_path_never_enter_a_report() {
        let dir = tempfile::tempdir().unwrap();
        let sharing = Sharing::open_with_endpoint(
            dir.path(),
            Language::En,
            Some("https://example.invalid"),
            Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0))),
        );
        sharing.set_choice(true, None).unwrap();
        sharing.state.lock().unwrap().crash_cursor = 1;
        let line = crate::crash::panic_line(
            2,
            "1.2.3",
            "abcdef123456",
            "thread",
            "private secret",
            "C:/Users/secret/crash.rs:42:3",
            "C:/Users/secret",
        );
        std::fs::write(dir.path().join(crate::crash::CRASHES), format!("{line}\n")).unwrap();
        assert!(sharing.collect_crashes(sharing.generation.load(Ordering::SeqCst), 3));
        let report = sharing.preview().unwrap().unwrap();
        assert!(report.contains("crash.rs") && report.contains("42"));
        assert!(!report.contains("secret") && !report.contains("thread") && !report.contains("Users"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_endpoint_or_consent_writes_no_report_and_never_calls_transport() {
        let runtime = tokio::runtime::Handle::current();
        for (url, consent) in [(None, None), (Some("https://example.invalid"), None), (Some("https://example.invalid"), Some(false))] {
            let dir = tempfile::tempdir().unwrap();
            let called = Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0)));
            let sharing = Sharing::open_with_endpoint(dir.path(), Language::En, url, called.clone());
            if consent == Some(false) {
                sharing.set_choice(false, None).unwrap();
            }
            sharing.record(&runtime, |report| report.counters.app_starts += 1);
            sharing.schedule(&runtime, 100, false, called.clone());
            assert_eq!(called.0.load(Ordering::SeqCst), 0);
            assert!(sharing.preview().unwrap().is_none());
            let state = sharing.state.lock().unwrap();
            assert!(state.active.is_none() && state.pending.is_none());
            if url.is_none() || consent.is_none() {
                assert!(!dir.path().join("sharing.json").exists());
            }
        }
    }

    #[test]
    fn switching_off_and_on_does_not_reset_either_daily_send_limit() {
        let dir = tempfile::tempdir().unwrap();
        let sharing = Sharing::open_with_endpoint(
            dir.path(),
            Language::En,
            Some("https://example.invalid"),
            Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0))),
        );
        sharing.set_choice(true, None).unwrap();
        let generation = sharing.gate().unwrap();
        let (_, generation) = sharing.prepare(generation, 42, false).unwrap().unwrap();
        sharing.finish_attempt(generation, 42, true);
        sharing.set_choice(false, None).unwrap();
        sharing.set_choice(true, None).unwrap();
        let generation = sharing.gate().unwrap();
        assert!(sharing.prepare(generation, 42, false).unwrap().is_none());
        let (_, generation) = sharing.prepare(generation, 42, true).unwrap().unwrap();
        sharing.finish_attempt(generation, 42, true);
        sharing.set_choice(false, None).unwrap();
        sharing.set_choice(true, None).unwrap();
        let generation = sharing.gate().unwrap();
        assert!(sharing.prepare(generation, 42, true).unwrap().is_none());
        let stored: Stored = crate::store::read_checked(&dir.path().join("sharing.json")).unwrap().unwrap();
        assert_eq!((stored.last_attempt_day, stored.last_crash_attempt_day), (Some(42), Some(42)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_event_from_before_refusal_cannot_be_recorded_after_a_new_yes() {
        let dir = tempfile::tempdir().unwrap();
        let transport = Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0)));
        let sharing = Sharing::open_with_endpoint(dir.path(), Language::En, Some("https://example.invalid"), transport.clone());
        sharing.set_choice(true, None).unwrap();
        let old = sharing.gate().unwrap();
        sharing.set_choice(false, None).unwrap();
        sharing.set_choice(true, None).unwrap();
        sharing.record_for(&tokio::runtime::Handle::current(), old, |report| report.counters.app_starts += 1);
        assert_eq!(sharing.queued.load(Ordering::SeqCst), 0);
        assert!(sharing.state.lock().unwrap().active.is_none());
        assert_eq!(transport.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn record_queue_drops_events_after_32_without_starting_extra_work() {
        let dir = tempfile::tempdir().unwrap();
        let transport = Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0)));
        let sharing = Sharing::open_with_endpoint(dir.path(), Language::En, Some("https://example.invalid"), transport);
        sharing.set_choice(true, None).unwrap();
        let state = sharing.state.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..(MAX_QUEUED + 8) {
            let tx = tx.clone();
            sharing.record(&tokio::runtime::Handle::current(), move |report| {
                report.counters.app_starts += 1;
                tx.send(()).unwrap();
            });
        }
        drop(tx);
        assert_eq!(sharing.queued.load(Ordering::SeqCst), MAX_QUEUED);
        drop(state);
        for _ in 0..MAX_QUEUED {
            rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        }
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn preview_is_frozen_retries_once_and_refusal_clears_pending() {
        let dir = tempfile::tempdir().unwrap();
        let sharing = Sharing::open_with_endpoint(
            dir.path(),
            Language::En,
            Some("https://example.invalid"),
            Arc::new(FakeTransport(std::sync::atomic::AtomicUsize::new(0))),
        );
        sharing.set_choice(true, None).unwrap();
        {
            let mut state = sharing.state.lock().unwrap();
            let report = state.active.get_or_insert_with(|| Report::environment(Language::En));
            report.counters.app_starts = 1;
            sharing.save(&state).unwrap();
        }
        let preview = sharing.preview().unwrap().unwrap();
        let (first, generation) = sharing.prepare(sharing.generation.load(Ordering::SeqCst), 1, false).unwrap().unwrap();
        assert_eq!(first.as_slice(), preview.as_bytes());
        sharing.finish_attempt(generation, 1, false);
        assert!(sharing.prepare(generation, 1, false).unwrap().is_none());
        let (second, generation) = sharing.prepare(generation, 2, false).unwrap().unwrap();
        assert_eq!(first, second);
        sharing.finish_attempt(generation, 2, false);
        assert!(sharing.state.lock().unwrap().pending.is_none());
        let _ = sharing.preview().unwrap();
        sharing.set_choice(false, None).unwrap();
        let state = sharing.state.lock().unwrap();
        assert_eq!(state.consent, Some(false));
        assert!(state.active.is_none() && state.pending.is_none());
        let on_disk: Stored = crate::store::read_checked(&dir.path().join("sharing.json")).unwrap().unwrap();
        assert!(on_disk.active.is_none() && on_disk.pending.is_none());
    }
}
