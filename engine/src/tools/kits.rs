//! One verified catalog snapshot; each session selects its own bounded kit in memory.

mod matching;
use matching::{item_capability, labels, matches_task, stack_labels};

use super::{Delivery, Item, Launch, Store, digest, lock, manifest_bytes, retry_busy, small_file};
use crate::model::{Area, Permissions, Vendor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// One byte per token is deliberately conservative, including the full skill instructions and framing.
pub const CONTEXT_BUDGET: usize = 16 * 1024;
/// Lazy discovery is independent of selected instruction bodies and server schemas.
pub const DISCOVERY_BUDGET: usize = 32 * 1024;
const DESCRIPTION_LIMIT: usize = 160;
const MAX_ITEMS: usize = 6;
const MAX_ALWAYS: usize = 3;
const SERVER_COST: usize = 4096;
const SNAPSHOT_LIMIT: u64 = 256 * 1024 * 1024;
const RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreparedText {
    instructions: Option<String>,
    server_identity: Option<String>,
    items: Vec<Item>,
    texts: Arc<BTreeMap<String, (String, String)>>,
    chat: bool,
    project: bool,
    description_ids: Vec<String>,
    plugin_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadyEntry {
    delivery: Delivery,
    items: Vec<Item>,
    texts: Arc<BTreeMap<String, (String, String)>>,
    generation: u64,
}

#[derive(Debug, Default)]
struct ReadyData {
    installed: BTreeMap<String, String>,
    selection: Selection,
    entry: Option<ReadyEntry>,
    pending: bool,
    error: Option<String>,
    failed_items: BTreeSet<String>,
    attempted: Option<(u64, Vec<Item>)>,
    generation: u64,
    retry_at: Option<std::time::Instant>,
    retry_delay: std::time::Duration,
}

#[derive(Debug, Default)]
pub(super) struct ReadyCache {
    data: Mutex<ReadyData>,
    building: AtomicBool,
    runtime: Mutex<Option<tokio::runtime::Handle>>,
    completed: std::sync::Condvar,
    on_ready: Mutex<Option<ReadyNotice>>,
    #[cfg(test)]
    copy_hook: Mutex<Option<(std::sync::mpsc::Sender<()>, Arc<std::sync::Barrier>)>>,
}

struct ReadyNotice(Arc<dyn Fn() + Send + Sync>);
impl std::fmt::Debug for ReadyNotice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ReadyNotice")
    }
}

impl ReadyCache {
    fn snapshot_error(&self) {
        let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let changed = data.error.as_deref() != Some("error.tools.snapshot");
        data.error = Some("error.tools.snapshot".into());
        data.attempted = None;
        if data.retry_at.is_none() {
            data.retry_at = Some(std::time::Instant::now() + data.retry_delay.max(std::time::Duration::from_secs(1)));
        }
        drop(data);
        if changed {
            self.notify();
        }
    }
    fn notify(&self) {
        let callback = self.on_ready.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|notice| notice.0.clone());
        if let Some(callback) = callback {
            callback();
        }
    }
    pub(super) fn for_root(root: &Path, items: &[Item], node: Option<&Path>) -> Arc<Self> {
        type Caches = BTreeMap<(PathBuf, [u8; 32]), Weak<ReadyCache>>;
        static ROOTS: OnceLock<Mutex<Caches>> = OnceLock::new();
        let trust: [u8; 32] = Sha256::digest(serde_json::to_vec(&(items, node)).unwrap_or_default()).into();
        let key = (root.to_owned(), trust);
        let mut roots = ROOTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        roots.retain(|_, cache| cache.strong_count() > 0);
        if let Some(cache) = roots.get(&key).and_then(Weak::upgrade) {
            return cache;
        }
        let cache = Arc::new(Self::default());
        roots.insert(key, Arc::downgrade(&cache));
        cache
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub ids: Vec<String>,
    pub jobs: Vec<String>,
    pub suggested: Option<String>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Catalog controls use the same persisted switches; uninstalling remains a separate operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(default)]
    pub disabled: BTreeSet<String>,
    #[serde(default)]
    pub always: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    ids: Vec<String>,
    pub jobs: Vec<String>,
}

/// A cached catalog state stays on disk; shared OS leases keep sweeping away from active sessions.
#[derive(Debug)]
pub struct Lease {
    root: PathBuf,
    _lock: std::fs::File,
    server_identity: String,
    ready: Weak<ReadyCache>,
    plugins: Vec<super::plugins::Definition>,
    agents: Vec<super::plugin_delivery::Agent>,
}

/// The sibling marker survives a crash without changing the payload's content digest.
pub(super) struct TemporaryCopy {
    pub(super) path: PathBuf,
    marker: PathBuf,
    file: Option<std::fs::File>,
}

impl TemporaryCopy {
    pub(super) fn new(root: &Path, prefix: &str) -> io::Result<Self> {
        let _guard = kit_guard(root)?;
        let path = root.join(format!("{prefix}{}", uuid::Uuid::new_v4()));
        let marker = path.with_extension("lease");
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&marker)?;
        file.lock_shared()?;
        Ok(Self { path, marker, file: Some(file) })
    }
}

impl Drop for TemporaryCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
        self.file.take();
        let _ = std::fs::remove_file(&self.marker);
    }
}

#[derive(Serialize, Deserialize)]
struct SnapshotState {
    items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    plugins: Vec<super::plugins::Definition>,
}

#[derive(Debug)]
struct InvalidItem(String);
impl std::fmt::Display for InvalidItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
impl std::error::Error for InvalidItem {}

impl PartialEq for Lease {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}
impl Eq for Lease {}

impl Delivery {
    pub fn tools_root(&self) -> Option<PathBuf> {
        self.catalog_root.clone().or_else(|| self.lease.as_ref().and_then(|lease| lease.root.parent()?.parent().map(Path::to_path_buf)))
    }

    /// Verify every resource the vendor will expose; ordinary I/O failure is not evidence of damaged content.
    pub fn verified_for_launch(&self, vendor: Vendor) -> (Self, Vec<String>, bool) {
        let mut checked = self.clone();
        let Some(text) = &self.prepared else {
            checked.plugin = None;
            checked.skills = None;
            checked.skill_ids = Some(vec![]);
            checked.servers.clear();
            return (checked, vec![], false);
        };
        let mut damaged = Vec::new();
        let mut excluded = Vec::new();
        let mut io_failed = false;
        let mut invalid_plugins = Vec::new();
        let Some(root) = self.lease.as_ref().map(|lease| &lease.root) else {
            checked.plugin = None;
            checked.skills = None;
            checked.servers.clear();
            checked.skill_ids = Some(vec![]);
            return (checked, vec![], false);
        };
        // Every kit folder is re-checked, with or without plugins: a file nothing placed drops the folder.
        let plugins = self.lease.as_ref().map_or(&[][..], |lease| lease.plugins.as_slice());
        let native_intact = super::plugin_delivery::intact(root, plugins);
        for plugin in plugins {
            let source_intact = self.catalog_root.as_ref().is_some_and(|store| super::plugins::verify_definition(store, plugin));
            if !native_intact || !source_intact {
                invalid_plugins.push(plugin.item.id.clone());
                checked.plugin = None;
                if self.catalog_root.as_ref().is_some_and(|store| super::plugin_delivery::source_changed(store, plugin)) {
                    damaged.push(plugin.item.id.clone());
                } else {
                    io_failed = true;
                }
            }
        }
        if checked.plugin.is_some() {
            let plain = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir());
            let names = |path: &Path| {
                super::entries(path).map(|entries| entries.into_iter().map(|entry| entry.file_name()).collect::<BTreeSet<_>>())
            };
            let expected_skills = text.items.iter().filter(|item| item.server.is_none()).map(|item| item.id.clone().into()).collect();
            let manifest = root.join(".claude-plugin");
            if !plain(root)
                || !native_intact
                || !plain(&root.join("skills"))
                || !plain(&manifest)
                || names(&root.join("skills")) != Some(expected_skills)
                || names(&manifest) != Some(BTreeSet::from(["plugin.json".into()]))
                || small_file(&manifest.join("plugin.json")).is_none_or(|bytes| bytes != manifest_bytes())
                || super::entries(root).is_none_or(|entries| {
                    entries.iter().any(|entry| {
                        !matches!(
                            entry.file_name().to_str(),
                            Some(".lease" | ".catalog.json" | ".claude-plugin" | "skills" | "servers" | "commands" | "agents")
                        )
                    })
                })
            {
                checked.plugin = None;
                io_failed = true;
            }
        }
        for item in &text.items {
            let used = if item.server.is_some() {
                self.servers.iter().any(|server| server.name == format!("faber-{}", item.id))
            } else {
                text.description_ids.contains(&item.id) || self.skill_ids.as_ref().is_none_or(|ids| ids.contains(&item.id))
            };
            let plugin_invalid = invalid_plugins.iter().any(|parent| item.id.starts_with(&format!("{parent}--")));
            if !used && !plugin_invalid {
                continue;
            }
            if plugin_invalid {
                excluded.push(item.id.clone());
                checked.servers.retain(|server| server.name != format!("faber-{}", item.id));
                if let Some(ids) = checked.skill_ids.as_mut() {
                    ids.retain(|id| id != &item.id);
                }
                continue;
            }
            let path = root.join(if item.server.is_some() { "servers" } else { "skills" }).join(&item.id);
            let actual = if [root.clone(), path.parent().unwrap_or(root).to_path_buf(), path.clone()]
                .iter()
                .all(|path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()))
            {
                digest(&path)
            } else {
                Err(io::Error::other("Not a plain resource folder"))
            };
            match actual {
                Ok(actual) if actual == item.digest && !plugin_invalid => continue,
                Ok(_) => {
                    if self.catalog_root.as_ref().is_some_and(|root| source_digest(root, item).is_ok_and(|actual| actual != item.digest)) {
                        damaged.push(item.id.clone());
                    } else {
                        io_failed = true;
                    }
                }
                Err(_) => io_failed = true,
            }
            excluded.push(item.id.clone());
            checked.servers.retain(|server| server.name != format!("faber-{}", item.id));
            if let Some(ids) = checked.skill_ids.as_mut() {
                ids.retain(|id| id != &item.id);
            }
            if item.server.is_none() {
                checked.plugin = None;
            }
        }
        if checked.skill_ids.as_ref().is_some_and(Vec::is_empty)
            && text.description_ids.iter().all(|id| excluded.contains(id))
            && text.plugin_ids.iter().all(|id| invalid_plugins.contains(id))
        {
            checked.skills = None;
            checked.plugin = None;
        }
        if let Some(prepared) = checked.prepared.as_mut() {
            let prepared = Arc::make_mut(prepared);
            prepared.description_ids.retain(|id| !excluded.contains(id));
            prepared.plugin_ids.retain(|id| !invalid_plugins.contains(id));
            let selected = checked.skill_ids.clone().unwrap_or_default();
            let instructions = selected
                .iter()
                .filter_map(|id| {
                    prepared.texts.get(id).map(|text| if prepared.chat { chat_instructions(&text.0, id) } else { text.0.clone() })
                })
                .collect::<String>();
            prepared.instructions = (!instructions.is_empty() && instructions.len() <= CONTEXT_BUDGET).then_some(instructions);
            prepared.server_identity = Some(server_key(
                &prepared
                    .items
                    .iter()
                    .filter(|item| checked.servers.iter().any(|server| server.name == format!("faber-{}", item.id)))
                    .collect::<Vec<_>>(),
            ));
        }
        let _ = vendor;
        if let Some(ready) = self.lease.as_ref().and_then(|lease| lease.ready.upgrade()) {
            let mut data = ready.data.lock().unwrap_or_else(|e| e.into_inner());
            let previous = (data.failed_items.clone(), data.error.clone());
            data.failed_items.extend(damaged.iter().cloned());
            for parent in &damaged {
                data.failed_items
                    .extend(text.items.iter().filter(|item| item.id.starts_with(&format!("{parent}--"))).map(|item| item.id.clone()));
            }
            if io_failed {
                data.error = Some("error.tools.snapshot".into());
                data.attempted = None;
                if data.retry_at.is_none() {
                    data.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
                }
            }
            let changed = previous != (data.failed_items.clone(), data.error.clone());
            drop(data);
            if changed {
                ready.notify();
            }
        }
        (checked, damaged, io_failed)
    }
    /// Compatibility helper for direct driver tests.
    #[cfg(test)]
    pub(crate) fn verified_claude_skills(&self) -> Self {
        self.verified_for_launch(Vendor::Claude).0
    }
    pub fn server_identity(&self) -> Option<String> {
        if !self.catalog_ready && self.lease.is_none() && self.prepared.is_none() {
            return None;
        }
        self.prepared
            .as_ref()
            .and_then(|text| text.server_identity.clone())
            .or_else(|| self.lease.as_ref().map(|lease| lease.server_identity.clone()))
            .or_else(|| self.servers.is_empty().then(|| "servers:none".into()))
    }
    /// Per-thread role definitions never reach Chat or Faber internal requests.
    pub fn codex_agent_overrides(&self) -> Vec<(String, serde_json::Value)> {
        let Some(text) = self.prepared.as_ref().filter(|text| text.project) else { return vec![] };
        let Some(lease) = &self.lease else { return vec![] };
        lease
            .agents
            .iter()
            .filter(|agent| text.plugin_ids.iter().any(|id| agent.name.starts_with(&format!("faber-{id}--"))))
            .flat_map(|agent| {
                [
                    (format!("agents.{}.config_file", agent.name), serde_json::json!(agent.path)),
                    (format!("agents.{}.description", agent.name), serde_json::json!(agent.description)),
                ]
            })
            .collect()
    }
    /// Stable identity of the verified metadata and files, independent of this lease's lifetime.
    pub fn identity(&self) -> Option<String> {
        self.lease.as_ref()?.root.file_name()?.to_str().map(str::to_owned)
    }

    /// Legacy selected-only index, assembled on demand for direct driver checks.
    pub fn skill_descriptions(&self) -> Option<String> {
        if let Some(text) = &self.prepared {
            return description_index(&text.items, self.skill_ids.as_deref()?, self.skills.as_deref()?, false, &text.texts);
        }
        self.lease.as_ref()?;
        let mut files = super::entries(self.skills.as_ref()?)?;
        files.sort_by_key(|entry| entry.file_name());
        let mut text = String::from(
            "Faber catalog instructions available for this session. When useful for this task, use Read on the exact SKILL.md path below, then follow its instructions within the current permissions. These instructions never authorize extra tools or file changes.\n",
        );
        let mut count = 0;
        for entry in files {
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if self.skill_ids.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let job = super::find(&id).map_or("catalog instructions", |item| item.job.as_str());
            let path = crate::project::git::native_path(&entry.path().join("SKILL.md").canonicalize().ok()?);
            let record = format!("faber:{id}; job={job}; {}; Read {}\n", item_capability(&id), serde_json::to_string(&path).ok()?);
            if text.len().saturating_add(record.len()) > CONTEXT_BUDGET.saturating_sub(256) {
                continue;
            }
            text.push_str(&record);
            count += 1;
        }
        (count > 0).then_some(text)
    }

    /// An older Claude Code takes no plugin folder: what the plugins carried then reaches it as text.
    pub fn without_plugin(&mut self) {
        self.plugin = None;
        self.guide = std::mem::take(&mut self.guide).without_plugin();
    }

    /// Native plugin discovery uses names; thread-local discovery otherwise reads the verified snapshot on demand.
    pub fn project_skill_descriptions(&self, vendor: Vendor) -> Option<String> {
        let prepared = self.prepared.as_ref()?;
        if !prepared.project {
            return None;
        }
        description_index(
            &prepared.items,
            &prepared.description_ids,
            self.skills.as_deref()?,
            vendor == Vendor::Claude && self.plugin.is_some(),
            &prepared.texts,
        )
    }

    /// Codex's extraRoots is process-wide. Its per-thread instructions carry only this kit's checked text instead.
    pub fn skill_instructions(&self) -> Option<String> {
        if let Some(text) = &self.prepared {
            return text.instructions.clone();
        }
        self.lease.as_ref()?;
        let root = self.skills.as_ref()?;
        let mut files = super::entries(root)?;
        files.sort_by_key(|e| e.file_name());
        let mut text = String::new();
        for entry in files {
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            if self.skill_ids.as_ref().is_some_and(|ids| !ids.iter().any(|id| entry.file_name() == id.as_str())) {
                continue;
            }
            let bytes = small_file(&entry.path().join("SKILL.md"))?;
            let body = std::str::from_utf8(&bytes).ok()?;
            text.push_str(&format!(
                "\nCatalog skill {} (supporting files: {}):\n",
                entry.file_name().to_string_lossy(),
                entry.path().display()
            ));
            text.push_str(body);
            text.push('\n');
            if text.len() > CONTEXT_BUDGET {
                return None;
            }
        }
        (!text.is_empty()).then_some(text)
    }
}

fn description_index(
    items: &[Item],
    ids: &[String],
    root: &Path,
    native: bool,
    texts: &BTreeMap<String, (String, String)>,
) -> Option<String> {
    let mut text = if native {
        String::from(
            "Faber catalog skills available through the native Skill tool. Use faber:name when useful, within current permissions.\n",
        )
    } else {
        format!(
            "Faber catalog skills. Read the selected SKILL.md only when useful, within current permissions. Paths below are relative to {}.\n",
            serde_json::to_string(&crate::project::git::native_path(root)).ok()?
        )
    };
    let records: Vec<_> = items
        .iter()
        .filter(|item| item.server.is_none() && ids.contains(&item.id))
        .map(|item| {
            let prefix =
                format!("faber:{}; job={};{}", item.cli_name.as_deref().unwrap_or(&item.id), item.job, if native { "" } else { " " });
            let suffix = if native { "\n".into() } else { format!("; Read {}/SKILL.md\n", item.id) };
            (item, prefix, suffix)
        })
        .collect();
    if records.is_empty() {
        return None;
    }
    let fixed = text.len() + records.iter().map(|(_, prefix, suffix)| prefix.len() + suffix.len()).sum::<usize>();
    let description_limit = (DISCOVERY_BUDGET.checked_sub(fixed)? / records.len()).min(DESCRIPTION_LIMIT);
    for (item, prefix, suffix) in records {
        let description = texts.get(&item.id).map(|(_, description)| description.as_str()).unwrap_or_else(|| item_capability(&item.id));
        text.push_str(&prefix);
        // The loaded native plugin already supplies descriptions; only fallback discovery needs them here.
        if !native {
            text.push_str(&short_description(description, description_limit));
        }
        text.push_str(&suffix);
    }
    Some(text)
}

fn short_description(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.into();
    }
    if limit < 3 {
        return String::new();
    }
    let mut end = limit - 3;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", text[..end].trim_end())
}

/// The pinned catalog uses plain/quoted scalars and indented or folded/literal continuations.
pub(super) fn checked_description(body: &str, fallback: &str) -> String {
    let mut lines = body.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim) != Some("---") {
        return short_description(fallback, DESCRIPTION_LIMIT);
    }
    let mut value = None::<String>;
    for line in lines.take_while(|line| line.trim() != "---") {
        if let Some(value) = &mut value {
            if !line.starts_with(char::is_whitespace) && !line.is_empty() {
                break;
            }
            if value.len() < 4096 {
                value.push(' ');
                value.extend(line.trim().chars().take(4096 - value.len()));
            }
        } else if let Some((key, text)) = line.split_once(':')
            && !line.starts_with(char::is_whitespace)
            && key.trim().trim_matches(['\'', '"']).eq_ignore_ascii_case("description")
        {
            let text = text.trim();
            value =
                Some(if matches!(text, ">" | ">-" | ">+" | "|" | "|-" | "|+") { String::new() } else { text.chars().take(4096).collect() });
        }
    }
    let Some(value) = value else { return short_description(fallback, DESCRIPTION_LIMIT) };
    let value = value.trim();
    let decoded = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str::<String>(value).unwrap_or_else(|_| value[1..value.len() - 1].to_string())
    } else if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        value[1..value.len() - 1].replace("''", "'")
    } else {
        value.to_string()
    };
    let normalized = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    short_description(if normalized.is_empty() { fallback } else { &normalized }, DESCRIPTION_LIMIT)
}

impl Store {
    pub(crate) fn generation(&self) -> u64 {
        self.ready.data.lock().unwrap_or_else(|e| e.into_inner()).generation
    }

    pub fn set_on_ready(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.ready.on_ready.lock().unwrap_or_else(|e| e.into_inner()) = Some(ReadyNotice(callback));
    }

    pub(crate) fn preparation_error(&self) -> Option<String> {
        self.ready.data.lock().unwrap_or_else(|e| e.into_inner()).error.clone()
    }
    pub fn set_runtime(&self, runtime: tokio::runtime::Handle) {
        *self.ready.runtime.lock().unwrap_or_else(|e| e.into_inner()) = Some(runtime);
    }

    pub(super) fn refresh_ready_inventory(&self) {
        let _store = lock();
        self.refresh_ready_inventory_locked();
    }

    fn refresh_ready_inventory_locked(&self) {
        self.refresh_ready_inventory_with_force(false);
    }

    fn refresh_ready_inventory_with_force(&self, force: bool) {
        let installed = self.installed_resources();
        let selection = self.resource_selection().unwrap_or_else(|| Selection {
            disabled: self.items.iter().chain(self.resource_catalog()).map(|item| item.id.clone()).collect(),
            ..Default::default()
        });
        let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        if force || data.installed != installed || data.selection != selection {
            data.generation += 1;
            data.entry = None;
            data.error = None;
            data.failed_items.clear();
            data.attempted = None;
            data.retry_at = None;
            data.retry_delay = std::time::Duration::ZERO;
        }
        data.installed = installed;
        data.selection = selection;
    }

    pub(super) fn refresh_ready(&self) {
        self.refresh_ready_inventory_locked();
        self.start_prepare();
    }

    fn start_prepare(&self) {
        if !self.enqueue() {
            return;
        }
        let store = self.clone();
        let runtime =
            tokio::runtime::Handle::try_current().ok().or_else(|| self.ready.runtime.lock().unwrap_or_else(|e| e.into_inner()).clone());
        if let Some(runtime) = runtime {
            runtime.spawn_blocking(move || store.drain_pending());
        } else {
            std::thread::spawn(move || store.drain_pending());
        }
    }

    pub(super) fn refresh_ready_after_install(&self) {
        self.refresh_ready_inventory_with_force(true);
        self.refresh_ready();
    }

    /// This path never waits for copying, hashing, the store lock or a worker.
    pub fn ready_kit_for(
        &self,
        vendor: Vendor,
        request: &str,
        cwd: Option<&Path>,
        area: Area,
        permissions: Permissions,
        disabled: &[String],
    ) -> (Delivery, Report) {
        let empty = || Delivery { catalog_root: Some(self.root.clone()), ..Default::default() };
        let (jobs, explicit) = labels(request, cwd);
        let stacks = stack_labels(request, cwd);
        let mut report = Report { jobs: jobs.into_iter().collect(), method: "code".into(), ..Default::default() };
        let Ok(data) = self.ready.data.try_lock() else { return (empty(), report) };
        if permissions == Permissions::Isolated {
            return (Delivery { catalog_ready: true, ..empty() }, report);
        }
        report.error = data.error.clone();
        let enabled = |item: &&Item| {
            data.installed.contains_key(&item.id)
                && !data.selection.disabled.contains(&item.id)
                && !data.failed_items.contains(&item.id)
                && eligible(item, area, permissions, disabled)
        };
        report.suggested = self
            .resource_catalog()
            .iter()
            .find(|item| {
                explicit.contains(&item.job)
                    && matches_task(item, request, &stacks)
                    && eligible(item, area, permissions, disabled)
                    && !data.installed.contains_key(&item.id)
                    && !data.selection.disabled.contains(&item.id)
                    && !self
                        .resource_catalog()
                        .iter()
                        .filter(enabled)
                        .any(|other| other.job == item.job && matches_task(other, request, &stacks))
            })
            .map(|item| item.id.clone());
        let Some(entry) = data.entry.as_ref().filter(|entry| entry.generation == data.generation) else {
            return (empty(), report);
        };
        let current = |item: &&Item| {
            enabled(item)
                && self.resource_catalog().iter().any(|trusted| trusted == *item)
                && (item.server.is_none() || entry.delivery.servers.iter().any(|server| server.name == format!("faber-{}", item.id)))
        };
        // A plugin exposes all descriptions, so stale snapshots cannot be partly filtered for Claude.
        if entry.items.iter().any(|item| {
            !self.resource_catalog().contains(item) || !data.installed.contains_key(&item.id) || data.selection.disabled.contains(&item.id)
        }) {
            return (empty(), report);
        }
        let mut items: Vec<_> = entry
            .items
            .iter()
            .filter(current)
            .filter(|item| {
                data.selection.always.contains(&item.id) || (report.jobs.contains(&item.job) && matches_task(item, request, &stacks))
            })
            .collect();
        items.sort_by_key(|item| (!data.selection.always.contains(&item.id), item.id.clone()));
        let discovery_ids: Vec<_> =
            entry.items.iter().filter(current).filter(|item| item.server.is_none()).map(|item| item.id.clone()).collect();
        let mut remaining = CONTEXT_BUDGET.saturating_sub(256);
        let mut count = 0;
        items.retain(|item| {
            let cost = if item.server.is_some() {
                SERVER_COST
            } else if vendor == Vendor::Claude {
                0
            } else {
                entry.texts.get(&item.id).map_or(usize::MAX, |text| text.0.len())
            };
            if count >= MAX_ITEMS || cost > remaining {
                return false;
            }
            count += 1;
            remaining -= cost;
            true
        });
        let mut delivery = entry.delivery.clone();
        delivery.catalog_ready = true;
        let description_items: Vec<_> = if vendor == Vendor::Claude {
            entry.items.iter().filter(|item| item.server.is_none()).collect()
        } else {
            items.iter().copied().filter(|item| item.server.is_none()).collect()
        };
        delivery.skill_ids = Some(description_items.iter().map(|item| item.id.clone()).collect());
        if delivery.skill_ids.as_ref().is_none_or(Vec::is_empty)
            && !(area == Area::Project
                && (!discovery_ids.is_empty()
                    || entry.delivery.lease.as_ref().is_some_and(|lease| super::plugin_delivery::has_native_parts(&lease.plugins))))
        {
            delivery.skills = None;
            delivery.plugin = None;
        }
        delivery.servers.retain(|server| items.iter().any(|item| server.name == format!("faber-{}", item.id)));
        delivery.disabled_servers = disabled.to_vec();
        let mut instructions = String::new();
        for item in &description_items {
            if let Some((body, _)) = entry.texts.get(&item.id)
                && vendor == Vendor::Codex
            {
                if area == Area::Chat {
                    instructions.push_str(&chat_instructions(body, &item.id));
                } else {
                    instructions.push_str(body);
                }
            }
        }
        delivery.prepared = Some(Arc::new(PreparedText {
            instructions: (!instructions.is_empty() && instructions.len() <= CONTEXT_BUDGET).then_some(instructions),
            server_identity: Some(server_key(&items)),
            items: entry.items.clone(),
            texts: entry.texts.clone(),
            chat: area == Area::Chat,
            project: area == Area::Project,
            plugin_ids: if area == Area::Project {
                delivery.lease.as_ref().map(|lease| lease.plugins.iter().map(|plugin| plugin.item.id.clone()).collect()).unwrap_or_default()
            } else {
                vec![]
            },
            description_ids: if area == Area::Project {
                discovery_ids
            } else {
                description_items.iter().map(|item| item.id.clone()).collect()
            },
        }));
        report.ids = items.iter().map(|item| item.id.clone()).collect();
        if vendor == Vendor::Claude {
            report.method = "catalog".into();
        }
        (delivery, report)
    }

    /// Prepare every switched-on resource, independently of any conversation or permission mode.
    pub fn prepare_kit_for(
        &self,
        _vendor: Vendor,
        _request: &str,
        _cwd: Option<&Path>,
        _area: Area,
        _permissions: Permissions,
        _disabled: &[String],
    ) {
        self.refresh_ready_inventory();
        if self.enqueue() {
            self.drain_pending();
        }
    }

    fn enqueue(&self) -> bool {
        let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        if data.retry_at.is_some_and(|retry| retry > std::time::Instant::now()) {
            return false;
        }
        data.pending = true;
        !self.ready.building.swap(true, Ordering::AcqRel)
    }

    fn drain_pending(&self) {
        loop {
            {
                let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
                if !data.pending {
                    self.ready.building.store(false, Ordering::Release);
                    self.ready.completed.notify_all();
                    return;
                }
                data.pending = false;
            }
            self.prepare_catalog();
            self.sweep_kits();
        }
    }

    fn wait_prepared(&self) {
        let guard = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .ready
            .completed
            .wait_timeout_while(guard, std::time::Duration::from_secs(30), |_| self.ready.building.load(Ordering::Acquire));
    }

    #[cfg(test)]
    pub(crate) fn wait_prepared_for_test(&self) {
        self.wait_prepared();
    }

    #[cfg(test)]
    pub(crate) fn clear_prepared_for_test(&self) {
        let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        data.entry = None;
        data.attempted = None;
    }

    pub fn catalog_server_ids(&self, ids: &[String]) -> Vec<String> {
        self.resource_catalog().iter().filter(|item| item.server.is_some() && ids.contains(&item.id)).map(|item| item.id.clone()).collect()
    }

    /// Current trusted server metadata also detects revocation when no snapshot is ready yet.
    pub fn server_identity_for(&self, ids: &[String], area: Area, permissions: Permissions, disabled: &[String]) -> String {
        let data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        server_key(
            &self
                .resource_catalog()
                .iter()
                .filter(|item| {
                    item.server.is_some()
                        && ids.contains(&item.id)
                        && data.installed.contains_key(&item.id)
                        && !data.selection.disabled.contains(&item.id)
                        && !data.failed_items.contains(&item.id)
                        && eligible(item, area, permissions, disabled)
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn retain_servers(
        &self,
        delivery: &mut Delivery,
        ids: &[String],
        area: Area,
        permissions: Permissions,
        disabled: &[String],
    ) -> bool {
        if !delivery.catalog_ready {
            return false;
        }
        let data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = data.entry.as_ref().filter(|entry| entry.generation == data.generation).cloned() else {
            delivery.catalog_ready = false;
            return false;
        };
        if !delivery.lease.as_ref().zip(entry.delivery.lease.as_ref()).is_some_and(|(old, current)| Arc::ptr_eq(old, current)) {
            delivery.catalog_ready = false;
            return false;
        }
        let servers: Vec<_> = self.resource_catalog().iter().filter(|item| item.server.is_some() && ids.contains(&item.id)).collect();
        if !servers.iter().all(|item| {
            data.installed.contains_key(&item.id)
                && !data.selection.disabled.contains(&item.id)
                && !data.failed_items.contains(&item.id)
                && eligible(item, area, permissions, disabled)
                && entry.items.contains(item)
                && entry.delivery.servers.iter().any(|launch| launch.name == format!("faber-{}", item.id))
        }) {
            return false;
        }
        drop(data);
        let Some(root) = entry.delivery.lease.as_ref().map(|lease| &lease.root) else { return false };
        for item in &servers {
            // Retained native servers were not selected for this task, so they need their own last-boundary check.
            let path = root.join("servers").join(&item.id);
            let actual = if [root.clone(), root.join("servers"), path.clone()]
                .iter()
                .all(|path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()))
            {
                digest(&path)
            } else {
                Err(io::Error::other("Not a plain resource folder"))
            };
            match actual {
                Ok(actual) if actual == item.digest => {}
                Ok(_) => {
                    if source_digest(&self.root, item).is_ok_and(|actual| actual != item.digest) {
                        self.mark_failed_items(std::slice::from_ref(&item.id));
                    } else {
                        self.ready.snapshot_error();
                    }
                    delivery.servers.retain(|server| server.name != format!("faber-{}", item.id));
                    return false;
                }
                Err(_) => {
                    self.ready.snapshot_error();
                    return false;
                }
            }
        }
        delivery.servers = entry
            .delivery
            .servers
            .iter()
            .filter(|launch| servers.iter().any(|item| launch.name == format!("faber-{}", item.id)))
            .cloned()
            .collect();
        let mut remaining = CONTEXT_BUDGET.saturating_sub(256 + servers.len() * SERVER_COST);
        let mut count = servers.len();
        let mut instructions = String::new();
        if let Some(ids) = delivery.skill_ids.as_mut() {
            ids.retain(|id| {
                let Some((body, _)) = entry.texts.get(id) else { return false };
                let chat = (area == Area::Chat).then(|| chat_instructions(body, id));
                let body = chat.as_deref().unwrap_or(body);
                if count >= MAX_ITEMS || body.len() > remaining {
                    return false;
                }
                remaining -= body.len();
                count += 1;
                instructions.push_str(body);
                true
            });
        }
        let text = delivery.prepared.get_or_insert_with(|| Arc::new(PreparedText::default()));
        let text = Arc::make_mut(text);
        text.server_identity = Some(server_key(&servers));
        text.instructions = (!instructions.is_empty()).then_some(instructions);
        true
    }

    fn prepare_catalog(&self) {
        {
            let _store = lock();
            if self.recover_replacements().is_err() {
                let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
                data.generation += 1;
                data.entry = None;
                drop(data);
                self.ready.snapshot_error();
                return;
            }
            self.refresh_ready_inventory_locked();
        }
        let (generation, items, plugins) = {
            let data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
            let items: Vec<_> = self
                .resource_catalog()
                .iter()
                .filter(|item| data.installed.contains_key(&item.id) && !data.selection.disabled.contains(&item.id))
                .cloned()
                .collect();
            if data.attempted.as_ref().is_some_and(|(generation, previous)| *generation == data.generation && previous == &items) {
                return;
            }
            let plugins = self
                .plugins
                .iter()
                .filter(|plugin| data.installed.contains_key(&plugin.item.id) && !data.selection.disabled.contains(&plugin.item.id))
                .cloned()
                .collect::<Vec<_>>();
            (data.generation, items, plugins)
        };
        let mut failed = BTreeSet::new();
        let mut checked = Vec::new();
        let mut source_error = None;
        let plugins: Vec<_> = plugins
            .into_iter()
            .filter(|plugin| {
                if super::plugins::verify_definition(&self.root, plugin) {
                    return true;
                }
                if super::plugin_delivery::source_changed(&self.root, plugin) {
                    failed.insert(plugin.item.id.clone());
                    failed.extend(
                        self.resource_catalog()
                            .iter()
                            .filter(|item| item.id.starts_with(&format!("{}--", plugin.item.id)))
                            .map(|item| item.id.clone()),
                    );
                } else {
                    source_error = Some(io::Error::other("The plugin source could not be checked"));
                }
                false
            })
            .collect();
        for item in &items {
            if failed.contains(&item.id) {
                continue;
            }
            match source_digest(&self.root, item) {
                Ok(actual) if actual == item.digest => checked.push(item),
                Ok(_) => {
                    failed.insert(item.id.clone());
                }
                Err(error) => {
                    source_error = Some(error);
                    break;
                }
            }
        }
        let snapshot = if let Some(error) = source_error {
            Err(error)
        } else {
            loop {
                let result = self.snapshot(&checked, generation, &plugins);
                let invalid =
                    result.as_ref().err().and_then(|error| error.get_ref()?.downcast_ref::<InvalidItem>()).map(|error| error.0.clone());
                let Some(id) = invalid else { break result };
                failed.insert(id.clone());
                checked.retain(|item| item.id != id);
            }
        };
        let result = snapshot.and_then(|delivery| {
            let mut texts = BTreeMap::new();
            if let Some(root) = &delivery.skills {
                for item in checked.iter().filter(|item| item.server.is_none()) {
                    let bytes = small_file(&root.join(&item.id).join("SKILL.md"))
                        .ok_or_else(|| io::Error::other("Unreadable catalog instructions"))?;
                    let body = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
                    let path = root.join(&item.id);
                    let instructions = if bytes.len() <= CONTEXT_BUDGET {
                        format!("\nCatalog skill {} (supporting files: {}):\n{}\n", item.id, path.display(), body)
                    } else {
                        "x".repeat(CONTEXT_BUDGET + 1)
                    };
                    let description = checked_description(body, item_capability(&item.id));
                    texts.insert(item.id.clone(), (instructions, description));
                }
            }
            Ok(ReadyEntry { delivery, items: checked.iter().map(|item| (*item).clone()).collect(), texts: Arc::new(texts), generation })
        });
        let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        if data.generation != generation {
            return;
        }
        let previous = (data.error.clone(), data.failed_items.clone(), data.entry.as_ref().and_then(|entry| entry.delivery.identity()));
        data.failed_items = failed;
        match result {
            Ok(entry) => {
                data.attempted = Some((generation, items));
                data.entry = Some(entry);
                data.error = None;
                data.retry_at = None;
                data.retry_delay = std::time::Duration::ZERO;
            }
            Err(_) => {
                data.attempted = None;
                data.entry = None;
                data.error = Some("error.tools.snapshot".into());
                data.retry_delay = (data.retry_delay * 2).max(std::time::Duration::from_secs(1)).min(RETRY_MAX);
                data.retry_at = Some(std::time::Instant::now() + data.retry_delay);
            }
        }
        let changed =
            previous != (data.error.clone(), data.failed_items.clone(), data.entry.as_ref().and_then(|entry| entry.delivery.identity()));
        drop(data);
        if changed {
            self.ready.notify();
        }
    }

    /// A corrupt or outdated installation remains visible so the Catalog can offer reinstalling it.
    pub fn item_status(&self, id: &str) -> Option<String> {
        let data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        data.failed_items.contains(id).then(|| "error.tools.reinstall".into())
    }

    pub fn mark_failed_items(&self, ids: &[String]) {
        let mut data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        let previous = data.failed_items.clone();
        data.failed_items
            .extend(ids.iter().filter(|id| self.items.iter().chain(self.resource_catalog()).any(|item| item.id == **id)).cloned());
        let changed = previous != data.failed_items;
        drop(data);
        if changed {
            self.ready.notify();
        }
    }

    /// Catalog commands call this on a blocking worker, never while holding the engine lock.
    pub fn checked_statuses(&self) -> BTreeMap<String, String> {
        self.refresh_ready_inventory();
        if self.ready.data.lock().unwrap_or_else(|e| e.into_inner()).attempted.is_none() {
            self.start_prepare();
        }
        let data = self.ready.data.lock().unwrap_or_else(|e| e.into_inner());
        data.failed_items.iter().map(|id| (id.clone(), "error.tools.reinstall".into())).collect()
    }

    pub(super) fn read_selection(&self) -> Option<Selection> {
        let path = self.root.join("selection.json");
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut selection = Selection::default();
                selection
                    .disabled
                    .extend(self.items.iter().filter(|item| self.disabled().join(&item.id).exists()).map(|item| item.id.clone()));
                return Some(selection);
            }
            Ok(m) if m.is_file() => {}
            _ => return None,
        }
        let mut selection: Selection = serde_json::from_slice(&small_file(&path)?).ok()?;
        selection.disabled.extend(self.items.iter().filter(|item| self.disabled().join(&item.id).exists()).map(|item| item.id.clone()));
        Some(selection)
    }

    pub fn selection(&self) -> Selection {
        self.read_selection().unwrap_or_default()
    }

    pub(super) fn save_selection(&self, selection: &Selection) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let part = self.root.join(format!(".selection-{}", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec(selection).map_err(|e| e.to_string())?;
        let result = std::fs::write(&part, bytes).and_then(|()| retry_busy(|| std::fs::rename(&part, self.root.join("selection.json"))));
        let _ = std::fs::remove_file(part);
        result.map_err(|e| e.to_string())
    }

    pub fn set_always(&self, id: &str, always: bool) -> Result<(), String> {
        let _store = lock();
        if !self.items.iter().any(|i| i.id == id) {
            return Err("Unknown catalog item".into());
        }
        let mut selection = self.read_selection().ok_or("Could not read catalog switches")?;
        if always {
            selection.always.insert(id.into());
            if selection.always.len() > MAX_ALWAYS {
                return Err("At most three catalog items can be always on".into());
            }
        } else {
            selection.always.remove(id);
        }
        self.save_selection(&selection)?;
        self.refresh_ready();
        Ok(())
    }

    pub fn shortlist(
        &self,
        request: &str,
        cwd: Option<&Path>,
        area: Area,
        permissions: Permissions,
        disabled_servers: &[String],
    ) -> Choice {
        let Some(selection) = self.resource_selection() else { return Choice::default() };
        let installed = self.installed_resources();
        let (jobs, _) = labels(request, cwd);
        let stacks = stack_labels(request, cwd);
        let mut items: Vec<_> = self
            .resource_catalog()
            .iter()
            .filter(|i| {
                eligible(i, area, permissions, disabled_servers)
                    && installed.contains_key(&i.id)
                    && !selection.disabled.contains(&i.id)
                    && ((jobs.contains(&i.job) && matches_task(i, request, &stacks)) || selection.always.contains(&i.id))
            })
            .collect();
        items.sort_by_key(|i| (!selection.always.contains(&i.id), i.id.clone()));
        items.truncate(12);
        Choice { ids: items.iter().map(|i| i.id.clone()).collect(), jobs: items.iter().map(|i| i.job.clone()).collect() }
    }

    pub fn kit(
        &self,
        request: &str,
        cwd: Option<&Path>,
        area: Area,
        permissions: Permissions,
        disabled_servers: &[String],
    ) -> (Delivery, Report) {
        self.kit_for(Vendor::Codex, request, cwd, area, permissions, disabled_servers)
    }

    /// Synchronous compatibility path for store tests; production reads only ready_kit_for.
    pub fn kit_for(
        &self,
        vendor: Vendor,
        request: &str,
        cwd: Option<&Path>,
        area: Area,
        permissions: Permissions,
        disabled: &[String],
    ) -> (Delivery, Report) {
        self.prepare_kit_for(vendor, request, cwd, area, permissions, disabled);
        self.wait_prepared();
        self.ready_kit_for(vendor, request, cwd, area, permissions, disabled)
    }

    /// An OS lock distinguishes abandoned copies from live sessions, including other Engine instances.
    pub fn sweep_kits(&self) {
        let installed = self.installed_resources();
        let selection = self.resource_selection();
        let parent = self.root.join("kits");
        if !std::fs::symlink_metadata(&self.root).is_ok_and(|m| m.is_dir()) {
            return;
        }
        let Ok(_guard) = self.kits_guard() else { return };
        for entry in std::fs::read_dir(&self.root).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if ![".kit-staging-", ".staging-", ".trash-"].iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            let path = entry.path();
            let marker = path.with_extension("lease");
            if name.ends_with(".lease") {
                let payload = path.with_extension("");
                if !payload.exists()
                    && let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(&path)
                    && file.try_lock().is_ok()
                {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                }
                continue;
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if std::fs::symlink_metadata(&marker).is_ok_and(|metadata| !metadata.is_file()) {
                continue;
            }
            let lease = std::fs::OpenOptions::new().read(true).write(true).open(&marker);
            match lease {
                Ok(file) if file.try_lock().is_ok() => {
                    let _ = std::fs::remove_dir_all(&path);
                    drop(file);
                    let _ = std::fs::remove_file(marker);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let _ = std::fs::remove_dir_all(&path);
                }
                _ => {}
            }
        }
        for entry in std::fs::read_dir(parent).into_iter().flatten().flatten() {
            if !entry.file_type().is_ok_and(|m| m.is_dir()) || uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
                continue;
            }
            let path = entry.path();
            let current = small_file(&path.join(".catalog.json"))
                .and_then(|bytes| serde_json::from_slice::<SnapshotState>(&bytes).ok())
                .is_some_and(|state| {
                    let Some(selection) = &selection else { return false };
                    state.items.len()
                        == self
                            .resource_catalog()
                            .iter()
                            .filter(|item| installed.contains_key(&item.id) && !selection.disabled.contains(&item.id))
                            .count()
                        && state.plugins
                            == self
                                .plugins
                                .iter()
                                .filter(|plugin| installed.contains_key(&plugin.item.id) && !selection.disabled.contains(&plugin.item.id))
                                .cloned()
                                .collect::<Vec<_>>()
                        && state.items.iter().all(|item| {
                            installed.contains_key(&item.id)
                                && !selection.disabled.contains(&item.id)
                                && self.resource_catalog().contains(item)
                        })
                });
            if current {
                continue;
            }
            let marker = path.join(".lease");
            if std::fs::symlink_metadata(&marker).is_ok_and(|m| !m.is_file()) {
                continue;
            }
            match std::fs::OpenOptions::new().read(true).write(true).open(&marker) {
                Ok(file) if file.try_lock().is_ok() => {
                    drop(file);
                    let _ = std::fs::remove_dir_all(path);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let _ = std::fs::remove_dir_all(path);
                }
                _ => {}
            }
        }
    }

    fn kits_guard(&self) -> io::Result<std::fs::File> {
        kit_guard(&self.root)
    }

    fn snapshot(&self, picked: &[&Item], generation: u64, plugins: &[super::plugins::Definition]) -> io::Result<Delivery> {
        if picked.is_empty() && plugins.is_empty() {
            return Ok(Delivery { catalog_ready: true, catalog_root: Some(self.root.clone()), ..Default::default() });
        }
        for path in [self.root.clone(), self.plugin(), self.skills(), self.servers()] {
            if std::fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_dir()) {
                return Err(io::Error::other("Not a plain catalog store folder"));
            }
        }
        let mut picked = picked.to_vec();
        picked.sort_by(|a, b| a.id.cmp(&b.id));
        let state =
            serde_json::to_vec(&SnapshotState { items: picked.iter().map(|item| (*item).clone()).collect(), plugins: plugins.to_vec() })
                .map_err(io::Error::other)?;
        let mut hash = Sha256::new();
        hash.update(b"faber-catalog-snapshot-v4\0");
        hash.update(&state);
        let key: [u8; 16] = hash.finalize()[..16].try_into().map_err(io::Error::other)?;
        let parent = self.root.join("kits");
        std::fs::create_dir_all(&parent)?;
        if !std::fs::symlink_metadata(&parent)?.is_dir() {
            return Err(io::Error::other("Not a plain kit folder"));
        }
        let mut root = parent.join(uuid::Uuid::from_bytes(key).to_string());
        if std::fs::symlink_metadata(&root).is_ok_and(|metadata| !metadata.is_dir()) {
            return Err(io::Error::other("Not a plain cached snapshot folder"));
        }
        let reuse = root.exists() && snapshot_intact(&root, &picked, &state);
        let held = std::fs::OpenOptions::new().read(true).write(true).open(root.join(".lease")).is_ok_and(|file| file.try_lock().is_err());
        if root.exists() && !reuse && held {
            root = parent.join(uuid::Uuid::new_v4().to_string());
        }
        let temporary = TemporaryCopy::new(&self.root, ".kit-staging-")?;
        let staging = &temporary.path;
        if !reuse {
            std::fs::create_dir(staging)?;
            #[cfg(test)]
            if let Some((started, gate)) = self.ready.copy_hook.lock().unwrap().take() {
                started.send(()).unwrap();
                gate.wait();
            }
            let result = (|| {
                std::fs::File::create(staging.join(".lease"))?;
                std::fs::write(staging.join(".catalog.json"), &state)?;
                let mut remaining = SNAPSHOT_LIMIT;
                for item in &picked {
                    let target = staging.join(if item.server.is_some() { "servers" } else { "skills" }).join(&item.id);
                    copy_checked(&self.resource_source(item), &target, &mut remaining)?;
                    if digest(&target)? != item.digest {
                        if source_digest(&self.root, item)? != item.digest {
                            return Err(io::Error::other(InvalidItem(item.id.clone())));
                        }
                        return Err(io::Error::other("The catalog snapshot changed during preparation"));
                    }
                }
                super::plugin_delivery::copy_parts(&self.root, staging, plugins, &mut remaining)?;
                if picked.iter().any(|item| item.server.is_none()) || super::plugin_delivery::has_native_parts(plugins) {
                    std::fs::create_dir_all(staging.join("skills"))?;
                    std::fs::create_dir(staging.join(".claude-plugin"))?;
                    std::fs::write(staging.join(".claude-plugin/plugin.json"), manifest_bytes())?;
                }
                if !snapshot_intact(staging, &picked, &state) {
                    return Err(io::Error::other("The catalog snapshot failed verification"));
                }
                Ok::<_, io::Error>(())
            })();
            if let Err(error) = result {
                let _ = std::fs::remove_dir_all(staging);
                return Err(error);
            }
        }
        // Copying and hashing above hold no store, engine, cache, or cross-process lock.
        let published = (|| {
            let _store = lock();
            self.refresh_ready_inventory_locked();
            if self.ready.data.lock().unwrap_or_else(|e| e.into_inner()).generation != generation {
                return Err(io::Error::other("Catalog changed during preparation"));
            }
            let _guard = self.kits_guard()?;
            if !reuse {
                if root.exists() {
                    let file = std::fs::OpenOptions::new().read(true).write(true).open(root.join(".lease"))?;
                    if file.try_lock().is_ok() {
                        drop(file);
                        retry_busy(|| std::fs::remove_dir_all(&root))?;
                    } else {
                        root = parent.join(uuid::Uuid::new_v4().to_string());
                    }
                }
                retry_busy(|| std::fs::rename(staging, &root))?;
            }
            let file = std::fs::OpenOptions::new().read(true).write(true).open(root.join(".lease"))?;
            file.lock_shared()?;
            Ok::<_, io::Error>(file)
        })();
        let _ = std::fs::remove_dir_all(staging);
        let file = published?;
        let node = self.node.clone().or_else(|| crate::cli::find_cli("node").map(|c| PathBuf::from(c.command)).filter(|p| p.is_absolute()));
        let mut delivery = Delivery { catalog_ready: true, catalog_root: Some(self.root.clone()), ..Default::default() };
        for item in &picked {
            if let Some(server) = &item.server {
                if let Some(command) = &node {
                    delivery.servers.push(Launch {
                        name: format!("faber-{}", item.id),
                        command: command.clone(),
                        args: std::iter::once(root.join("servers").join(&item.id).join(&server.entry).to_string_lossy().into_owned())
                            .chain(server.args.iter().cloned())
                            .collect(),
                        env: server.env.clone(),
                    });
                }
            } else {
                delivery.plugin = Some(root.clone());
                delivery.skills = Some(root.join("skills"));
            }
        }
        if super::plugin_delivery::has_native_parts(plugins) {
            delivery.plugin = Some(root.clone());
        }
        let agents = super::plugin_delivery::agents(&root, plugins)?;
        delivery.lease = Some(Arc::new(Lease {
            root,
            _lock: file,
            server_identity: server_key(&picked),
            ready: Arc::downgrade(&self.ready),
            plugins: plugins.to_vec(),
            agents,
        }));
        Ok(delivery)
    }
}

fn server_key(items: &[&Item]) -> String {
    let mut servers: Vec<_> = items.iter().filter(|item| item.server.is_some()).copied().collect();
    if servers.is_empty() {
        return "servers:none".into();
    }
    servers.sort_by(|a, b| a.id.cmp(&b.id));
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&servers).unwrap_or_default());
    let bytes: [u8; 16] = hash.finalize()[..16].try_into().expect("SHA-256 has sixteen bytes");
    format!("servers:{}", uuid::Uuid::from_bytes(bytes))
}

fn source_digest(root: &Path, item: &Item) -> io::Result<String> {
    if let Some(path) = super::plugins::resource_source(root, item) {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let mut parent = root.to_path_buf();
        if !std::fs::symlink_metadata(&parent)?.is_dir() {
            return Err(io::Error::other("Not a plain catalog source root"));
        }
        for component in relative.components() {
            parent.push(component);
            if !std::fs::symlink_metadata(&parent)?.is_dir() {
                return Err(io::Error::other("Not a plain plugin resource path"));
            }
        }
        return digest(&path);
    }
    let mut folders = vec![root.to_path_buf()];
    let parent = if item.server.is_some() {
        root.join("servers")
    } else {
        folders.push(root.join("plugin"));
        root.join("plugin/skills")
    };
    folders.push(parent.clone());
    folders.push(parent.join(&item.id));
    if !folders.iter().all(|path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())) {
        return Err(io::Error::other("Not a plain catalog source folder"));
    }
    digest(folders.last().expect("the item folder is present"))
}

fn chat_instructions(text: &str, id: &str) -> String {
    let body = text.split_once(":\n").map_or(text, |(_, body)| body);
    format!("\nCatalog skill {id} (provided instructions only; supporting files are unavailable in Chat):\n{body}")
}

fn tree_bytes(root: &Path) -> u64 {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => tree_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |metadata| metadata.len()),
            _ => 0,
        })
        .sum()
}

fn kit_guard(store: &Path) -> io::Result<std::fs::File> {
    if !std::fs::symlink_metadata(store)?.is_dir() {
        return Err(io::Error::other("Not a plain tool store"));
    }
    let path = store.join(".kits-lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()) {
        return Err(io::Error::other("Not a plain kit lock"));
    }
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    file.lock()?;
    Ok(file)
}

fn snapshot_intact(root: &Path, picked: &[&Item], state: &[u8]) -> bool {
    let Ok(saved) = serde_json::from_slice::<SnapshotState>(state) else { return false };
    let native = super::plugin_delivery::has_native_parts(&saved.plugins);
    let plain_dir = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    let names =
        |path: &Path| super::entries(path).map(|entries| entries.into_iter().map(|entry| entry.file_name()).collect::<BTreeSet<_>>());
    let mut expected = BTreeSet::from([std::ffi::OsString::from(".lease"), ".catalog.json".into()]);
    if !plain_dir(root)
        || !std::fs::symlink_metadata(root.join(".lease")).is_ok_and(|m| m.is_file())
        || small_file(&root.join(".catalog.json")).is_none_or(|bytes| bytes != state)
    {
        return false;
    }
    for (name, server) in [("skills", false), ("servers", true)] {
        let items: Vec<_> = picked.iter().filter(|item| item.server.is_some() == server).collect();
        if items.is_empty() && !(name == "skills" && native) {
            continue;
        }
        expected.insert(name.into());
        let parent = root.join(name);
        if !plain_dir(&parent) || names(&parent) != Some(items.iter().map(|item| std::ffi::OsString::from(&item.id)).collect()) {
            return false;
        }
        for item in items {
            let path = parent.join(&item.id);
            if !plain_dir(&path) || !digest(&path).is_ok_and(|got| got == item.digest) {
                return false;
            }
        }
        if !server {
            expected.insert(".claude-plugin".into());
            let manifest = root.join(".claude-plugin");
            if !plain_dir(&manifest)
                || names(&manifest) != Some(BTreeSet::from(["plugin.json".into()]))
                || small_file(&manifest.join("plugin.json")).is_none_or(|got| got != manifest_bytes())
            {
                return false;
            }
        }
    }
    for (kind, directory) in [(super::plugins::PartKind::Command, "commands"), (super::plugins::PartKind::Agent, "agents")] {
        if !super::plugin_delivery::names(&saved.plugins, kind).is_empty() {
            expected.insert(directory.into());
        }
    }
    names(root) == Some(expected)
        && super::plugin_delivery::intact(root, &saved.plugins)
        && ["skills", "servers", "commands", "agents"].iter().map(|name| tree_bytes(&root.join(name))).sum::<u64>() <= SNAPSHOT_LIMIT
}

fn eligible(item: &Item, area: Area, permissions: Permissions, disabled_servers: &[String]) -> bool {
    permissions != Permissions::Isolated
        && (item.server.is_none()
            || (permissions == Permissions::FullAccess
                && (area == Area::Project || item.chat_safe)
                && !disabled_servers.iter().any(|name| name == &item.id || name == &format!("faber-{}", item.id))))
}

fn copy_checked(source: &Path, dest: &Path, remaining: &mut u64) -> io::Result<()> {
    if !std::fs::symlink_metadata(source)?.is_dir() {
        return Err(io::Error::other("Not a plain tool folder"));
    }
    std::fs::create_dir_all(dest)?;
    if !std::fs::symlink_metadata(dest)?.is_dir() {
        return Err(io::Error::other("Not a plain snapshot folder"));
    }
    // Close directory enumeration before copying: Windows otherwise prevents replacing the source folder.
    let entries = std::fs::read_dir(source)?
        .map(|entry| entry.and_then(|entry| Ok((entry.path(), entry.file_name(), entry.file_type()?))))
        .collect::<io::Result<Vec<_>>>()?;
    for (path, name, kind) in entries {
        let target = dest.join(name);
        if kind.is_dir() {
            copy_checked(&path, &target, remaining)?;
        } else if kind.is_file() {
            let length = std::fs::metadata(&path)?.len();
            *remaining = remaining.checked_sub(length).ok_or_else(|| io::Error::other("Catalog snapshot exceeds its size limit"))?;
            if std::fs::hard_link(&path, &target).is_ok() {
                continue;
            }
            copy_file(&path, &target)?;
        } else {
            return Err(io::Error::other("A link cannot enter a kit"));
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> io::Result<()> {
    // Rust handles permit source replacement on Windows; create_new refuses planted destinations, including links.
    let mut source = std::fs::File::open(source)?;
    let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
    std::io::copy(&mut source, &mut output)?;
    output.set_permissions(source.metadata()?.permissions())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_hardlinks_keep_verified_bytes_and_enforce_the_aggregate_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let dest = tmp.path().join("copy");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("file"), "verified bytes").unwrap();
        let mut remaining = 14;
        copy_checked(&source, &dest, &mut remaining).unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(digest(&source).unwrap(), digest(&dest).unwrap());
        // Both are on this temporary volume, where the standard hard-link operation must succeed.
        std::fs::write(source.join("file"), "same inode").unwrap();
        assert_eq!(std::fs::read(dest.join("file")).unwrap(), b"same inode");
        assert!(copy_checked(&source, &tmp.path().join("too-large"), &mut 2).is_err());
        let fallback = tmp.path().join("fallback");
        std::fs::create_dir(&fallback).unwrap();
        copy_file(&source.join("file"), &fallback.join("file")).unwrap();
        assert_eq!(digest(&source).unwrap(), digest(&fallback).unwrap());
        std::fs::write(source.join("file"), "changed source").unwrap();
        assert_eq!(std::fs::read(fallback.join("file")).unwrap(), b"same inode");
        assert!(copy_checked(&source, &fallback, &mut 100).is_err(), "a planted existing destination is never overwritten");
        assert_eq!(std::fs::read(fallback.join("file")).unwrap(), b"same inode");
        let planted = tmp.path().join("planted-file");
        std::fs::hard_link(fallback.join("file"), &planted).unwrap();
        assert!(copy_file(&source.join("file"), &planted).is_err());
        assert_eq!(std::fs::read(fallback.join("file")).unwrap(), b"same inode");
        #[cfg(windows)]
        {
            let linked = tmp.path().join("linked-directory");
            junction::create(&fallback, &linked).unwrap();
            assert!(copy_checked(&source, &linked, &mut 100).is_err());
            assert_eq!(std::fs::read(fallback.join("file")).unwrap(), b"same inode");
            junction::delete(linked).unwrap();
        }
        #[cfg(unix)]
        {
            let linked = tmp.path().join("linked");
            std::os::unix::fs::symlink(fallback.join("file"), &linked).unwrap();
            assert!(copy_file(&source.join("file"), &linked).is_err());
            assert_eq!(std::fs::read(fallback.join("file")).unwrap(), b"same inode");
        }
    }

    #[test]
    fn catalog_statuses_never_wait_for_a_build_and_publish_notifies_only_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let notices = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = notices.clone();
        store.set_on_ready(Arc::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
        }));
        let (started, ready) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        *store.ready.copy_hook.lock().unwrap() = Some((started, gate.clone()));
        assert!(store.checked_statuses().is_empty());
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(store.checked_statuses().is_empty(), "the blocked builder must not block Catalog rows");
        gate.wait();
        store.wait_prepared_for_test();
        assert_eq!(notices.load(Ordering::Relaxed), 1);
        store.checked_statuses();
        store.wait_prepared_for_test();
        assert_eq!(notices.load(Ordering::Relaxed), 1, "a Catalog refresh must not create a notification loop");
        let (kit, _) = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::Ask, &[]);
        std::fs::write(kit.skills.as_ref().unwrap().join("debug/SKILL.md"), "damaged source and snapshot").unwrap();
        kit.verified_for_launch(Vendor::Codex);
        assert_eq!(notices.load(Ordering::Relaxed), 2, "a launch failure refreshes an already mounted Catalog");
        kit.verified_for_launch(Vendor::Codex);
        assert_eq!(notices.load(Ordering::Relaxed), 2, "unchanged launch failures do not create a notification loop");
    }

    #[test]
    fn project_discovery_keeps_large_skills_and_chat_chooses_the_next_fitting_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("a-large", "design", CONTEXT_BUDGET + 1), ("b-small", "design", 100), ("off", "design", 100)]);
        store.set_enabled("off", false).unwrap();
        for permissions in [Permissions::Ask, Permissions::AcceptEdits, Permissions::FullAccess] {
            let (project, report) = store.kit_for(Vendor::Codex, "design", None, Area::Project, permissions, &[]);
            assert_eq!(report.ids, ["b-small"]);
            let index = project.project_skill_descriptions(Vendor::Codex).unwrap();
            assert!(index.contains("a-large/SKILL.md") && index.contains("b-small/SKILL.md"));
            assert!(!index.contains("off/SKILL.md"));
            assert!(index.len() <= DISCOVERY_BUDGET);
            assert!(project.skill_instructions().unwrap().len() + 256 <= CONTEXT_BUDGET);
            let (chat, report) = store.kit_for(Vendor::Codex, "design", None, Area::Chat, permissions, &[]);
            assert_eq!(report.ids, ["b-small"]);
            assert!(chat.project_skill_descriptions(Vendor::Codex).is_none());
            let (claude, _) = store.kit_for(Vendor::Claude, "design", None, Area::Project, permissions, &[]);
            let native = claude.project_skill_descriptions(Vendor::Claude).unwrap();
            assert!(native.contains("faber:a-large") && !native.contains("Read") && !native.contains("SKILL.md"));
            let mut fallback = claude.clone();
            fallback.plugin = None;
            assert!(fallback.project_skill_descriptions(Vendor::Claude).unwrap().contains("a-large/SKILL.md"));
        }
        let (kit, _) = store.kit_for(Vendor::Codex, "unrelated", None, Area::Project, Permissions::Ask, &[]);
        std::fs::write(kit.skills.as_ref().unwrap().join("a-large/SKILL.md"), "tampered").unwrap();
        let (checked, damaged, _) = kit.verified_for_launch(Vendor::Codex);
        assert_eq!(damaged, ["a-large"]);
        assert!(!checked.project_skill_descriptions(Vendor::Codex).unwrap().contains("a-large"));
        assert!(checked.project_skill_descriptions(Vendor::Codex).unwrap().contains("b-small"));
    }

    #[test]
    fn every_shipped_description_fits_its_own_cap_without_instruction_bodies() {
        let items = super::super::catalog();
        let ids = items.iter().filter(|item| item.server.is_none()).map(|item| item.id.clone()).collect::<Vec<_>>();
        let texts = items
            .iter()
            .map(|item| {
                (item.id.clone(), (String::new(), format!("Checked description for {}: {}", item.id, "meaningful detail ".repeat(30))))
            })
            .collect();
        let index = description_index(
            items,
            &ids,
            Path::new(
                "E:/Documentos/ProjetosIA/faber/data/tools/kits/0123456789012345678901234567890123456789012345678901234567890123/skills",
            ),
            false,
            &texts,
        )
        .unwrap();
        assert!(index.len() <= DISCOVERY_BUDGET, "114 descriptions need {} bytes", index.len());
        assert!(index.len() > CONTEXT_BUDGET, "discovery does not consume selected instructions or server budget");
        for item in items.iter().filter(|item| item.server.is_none()) {
            assert!(index.contains(&format!("{}/SKILL.md", item.id)), "{}", item.id);
            assert!(index.contains(&format!("Checked description for {}:", item.id)));
        }
    }

    #[test]
    fn checked_descriptions_handle_shipped_scalar_forms_and_stay_short_utf8() {
        for (header, expected) in [
            ("description: Explain code\n  before editing it.\nmetadata:\n  description: Wrong field", "Explain code before editing it."),
            ("description: >-\n  Explain code\n  before editing it.\nname: sample", "Explain code before editing it."),
            ("description: |\n  Explain code\n  before editing it.\nname: sample", "Explain code before editing it."),
            ("description: \"Explain \\\"quoted\\\" code.\"", "Explain \"quoted\" code."),
            ("description: 'Explain someone''s code.'", "Explain someone's code."),
            ("name: sample", "fallback"),
            ("description: \"", "\""),
        ] {
            assert_eq!(checked_description(&format!("---\r\n{header}\r\n---\r\nBody description: not metadata"), "fallback"), expected);
        }
        let text = checked_description(&format!("---\ndescription: {}\n---\n", "Éclairage sûr. ".repeat(100)), "fallback");
        assert!(text.len() <= DESCRIPTION_LIMIT && text.ends_with("..."));
        assert!(!text.contains('\n'));
    }

    #[test]
    fn all_shipped_skills_leave_budget_for_selected_pinned_and_retained_servers() {
        let tmp = tempfile::tempdir().unwrap();
        let mut items = super::super::catalog().to_vec();
        let root = tmp.path().join("tools");
        for item in &mut items {
            let folder = root.join(if item.server.is_some() { "servers" } else { "plugin/skills" }).join(&item.id);
            std::fs::create_dir_all(&folder).unwrap();
            if let Some(server) = &item.server {
                let entry = folder.join(&server.entry);
                std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
                std::fs::write(entry, "checked server fixture").unwrap();
            } else {
                let instructions = if item.id == "systematic-debugging" { 11_000 } else { 100 };
                let body = format!(
                    "---\nname: {}\ndescription: >\n  Checked capability for {}. {}\n---\n{}",
                    item.cli_name.as_deref().unwrap_or(&item.id),
                    item.id,
                    "Useful original description. ".repeat(20),
                    "x".repeat(instructions)
                );
                std::fs::write(folder.join("SKILL.md"), body).unwrap();
            }
            item.digest = digest(&folder).unwrap();
        }
        let store = Store::with_catalog(tmp.path(), items).with_node("C:/fixture/node.exe".into());
        store.write_manifest().unwrap();
        store.write_ledger(&store.items.iter().map(|item| (item.id.clone(), item.commit.clone())).collect()).unwrap();
        let check = |delivery: &Delivery| {
            let index = delivery.project_skill_descriptions(Vendor::Codex).unwrap();
            assert_eq!(index.lines().filter(|line| line.starts_with("faber:")).count(), 114);
            assert!(index.len() > CONTEXT_BUDGET && index.len() <= DISCOVERY_BUDGET);
            assert!(index.contains("Checked capability for systematic-debugging."));
            assert!(delivery.servers.iter().any(|server| server.name == "faber-chrome-devtools"));
            assert!(delivery.skill_instructions().unwrap().len() + SERVER_COST + 256 <= CONTEXT_BUDGET);
            assert!(delivery.skill_instructions().unwrap().contains(&"x".repeat(11_000)));
        };
        let (selected, report) = store.kit_for(
            Vendor::Codex,
            "Measure my page with Lighthouse and debug its crash",
            None,
            Area::Project,
            Permissions::FullAccess,
            &[],
        );
        assert!(report.ids.iter().any(|id| id == "chrome-devtools"));
        check(&selected);
        let (mut retained, _) = store.kit_for(Vendor::Codex, "Debug the crash", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(store.retain_servers(&mut retained, &["chrome-devtools".into()], Area::Project, Permissions::FullAccess, &[]));
        check(&retained);
        store.set_always("chrome-devtools", true).unwrap();
        let (pinned, report) = store.kit_for(Vendor::Codex, "Debug the crash", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(report.ids.iter().any(|id| id == "chrome-devtools"));
        check(&pinned);
        let snapshot = pinned.skills.unwrap().join("systematic-debugging/SKILL.md");
        std::fs::write(&snapshot, "---\ndescription: Changed after preparation\n---").unwrap();
        let (cached, _) = store.ready_kit_for(Vendor::Codex, "Debug the crash", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(cached.project_skill_descriptions(Vendor::Codex).unwrap().contains("Checked capability for systematic-debugging."));
        let (verified, damaged, _) = cached.verified_for_launch(Vendor::Codex);
        assert_eq!(damaged, ["systematic-debugging"]);
        assert!(!verified.project_skill_descriptions(Vendor::Codex).unwrap().contains("faber:systematic-debugging;"));
    }

    #[test]
    fn transient_preparation_backoff_is_bounded_and_catalog_changes_retry_immediately() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        {
            let mut data = store.ready.data.lock().unwrap();
            data.retry_at = Some(std::time::Instant::now() + RETRY_MAX);
            data.retry_delay = RETRY_MAX;
            data.error = Some("error.tools.snapshot".into());
        }
        for _ in 0..100 {
            assert!(!store.enqueue(), "launches cannot repeatedly rebuild a locked or over-cap catalog");
        }
        assert!(store.item_status("debug").is_none());
        store.set_enabled("debug", false).unwrap();
        store.wait_prepared_for_test();
        assert!(store.ready.data.lock().unwrap().retry_at.is_none());
        store.set_enabled("debug", true).unwrap();
        let (kit, report) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(kit.catalog_ready && report.error.is_none());
        store.ready.snapshot_error();
        store.ready.data.lock().unwrap().retry_at = Some(std::time::Instant::now());
        let (kit, report) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(kit.catalog_ready && report.error.is_none(), "transient errors are retried after their deadline");
    }

    #[test]
    fn copy_io_failure_is_uncached_and_never_marks_a_healthy_item_for_reinstall() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let (started, ready) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        *store.ready.copy_hook.lock().unwrap() = Some((started, gate.clone()));
        let worker_store = store.clone();
        let worker = std::thread::spawn(move || worker_store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]));
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let staging = std::fs::read_dir(&store.root)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir()) && entry.file_name().to_string_lossy().starts_with(".kit-staging-")
            })
            .unwrap()
            .path();
        std::fs::write(staging.join("skills"), "cannot create a directory here").unwrap();
        gate.wait();
        worker.join().unwrap();
        let (unavailable, report) = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(!unavailable.catalog_ready);
        assert_eq!(report.error.as_deref(), Some("error.tools.snapshot"));
        assert!(store.item_status("debug").is_none());
        assert!(!store.enqueue(), "a repeated transient failure is not rebuilt for every launch");
        store.ready.data.lock().unwrap().retry_at = Some(std::time::Instant::now());
        let (retried, report) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(retried.catalog_ready && report.error.is_none());
        assert_eq!(report.ids, ["debug"]);
    }

    #[test]
    fn launch_io_and_snapshot_only_damage_retry_without_marking_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let path = kit.skills.as_ref().unwrap().join("debug");
        let hidden = path.with_extension("hidden");
        std::fs::rename(&path, &hidden).unwrap();
        let (_, damaged, io_failed) = kit.verified_for_launch(Vendor::Codex);
        assert!(io_failed && damaged.is_empty());
        std::fs::rename(hidden, &path).unwrap();
        store.ready.data.lock().unwrap().retry_at = Some(std::time::Instant::now());
        let (recovered, report) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        assert!(recovered.catalog_ready && report.error.is_none());
        std::fs::remove_file(path.join("SKILL.md")).unwrap();
        std::fs::write(path.join("SKILL.md"), "only the snapshot changed").unwrap();
        let (_, damaged, io_failed) = recovered.verified_for_launch(Vendor::Codex);
        assert!(io_failed && damaged.is_empty());
        assert!(store.item_status("debug").is_none());
        store.ready.data.lock().unwrap().retry_at = Some(std::time::Instant::now());
        let (repaired, report) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        assert!(repaired.catalog_ready && report.error.is_none());
        assert_ne!(repaired.plugin, kit.plugin);
        assert_eq!(std::fs::read(repaired.skills.unwrap().join("debug/SKILL.md")).unwrap(), vec![b'x'; 100]);
    }

    #[test]
    fn launch_drops_a_kit_that_holds_a_native_part_no_plugin_placed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let root = kit.plugin.as_ref().unwrap();
        for folder in ["commands", "agents"] {
            std::fs::create_dir(root.join(folder)).unwrap();
            std::fs::write(root.join(folder).join("planted.md"), "---\nallowed-tools: Bash\n---\nrun it").unwrap();
            for vendor in [Vendor::Claude, Vendor::Codex] {
                let (checked, damaged, io_failed) = kit.verified_for_launch(vendor);
                assert!(checked.plugin.is_none() && io_failed && damaged.is_empty(), "{folder} planted without any plugin");
            }
            std::fs::remove_dir_all(root.join(folder)).unwrap();
        }
        assert!(kit.verified_for_launch(Vendor::Claude).0.plugin.is_some());
    }

    #[test]
    fn launch_rejects_linked_snapshot_roots_even_when_their_contents_are_trusted() {
        for part in ["", "skills", "skills/debug", ".claude-plugin"] {
            let tmp = tempfile::tempdir().unwrap();
            let store = store(tmp.path(), &[("debug", "debugging", 100)]);
            let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
            let original = kit.plugin.as_ref().unwrap();
            let alias = tmp.path().join("alias");
            let target = if part.is_empty() { original.clone() } else { original.join(part) };
            let path = if part.is_empty() { alias.clone() } else { alias.join(part) };
            if !part.is_empty() {
                let mut remaining = SNAPSHOT_LIMIT;
                copy_checked(original, &alias, &mut remaining).unwrap();
                std::fs::remove_dir_all(&path).unwrap();
            }
            #[cfg(windows)]
            junction::create(&target, &path).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &path).unwrap();
            let mut linked = kit.clone();
            linked.plugin = Some(alias.clone());
            linked.skills = Some(alias.join("skills"));
            linked.lease = Some(Arc::new(Lease {
                root: alias,
                _lock: std::fs::File::open(original.join(".lease")).unwrap(),
                server_identity: "servers:none".into(),
                ready: Arc::downgrade(&store.ready),
                plugins: vec![],
                agents: vec![],
            }));
            for vendor in [Vendor::Claude, Vendor::Codex] {
                let (checked, damaged, io_failed) = linked.verified_for_launch(vendor);
                assert!(io_failed && damaged.is_empty(), "{part}: a root link is rejected without blaming unchanged bytes");
                assert!(checked.plugin.is_none());
            }
            #[cfg(windows)]
            junction::delete(&path).unwrap();
            #[cfg(unix)]
            std::fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn snapshot_mismatch_never_hashes_a_linked_catalog_source_to_classify_damage() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let snapshot = kit.skills.as_ref().unwrap().join("debug/SKILL.md");
        std::fs::remove_file(&snapshot).unwrap();
        std::fs::write(&snapshot, "only the snapshot changed").unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(outside.join("debug")).unwrap();
        std::fs::write(outside.join("SKILL.md"), "different external bytes").unwrap();
        std::fs::write(outside.join("debug/SKILL.md"), "different external bytes").unwrap();
        for leaf in [false, true] {
            let source = tmp.path().join(if leaf { "leaf-source" } else { "parent-source" });
            let parent = source.join("plugin/skills");
            let linked = if leaf {
                std::fs::create_dir_all(&parent).unwrap();
                parent.join("debug")
            } else {
                std::fs::create_dir_all(source.join("plugin")).unwrap();
                parent
            };
            #[cfg(windows)]
            junction::create(&outside, &linked).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&outside, &linked).unwrap();
            let mut changed = kit.clone();
            changed.catalog_root = Some(source);
            let (checked, damaged, io_failed) = changed.verified_for_launch(Vendor::Codex);
            assert!(io_failed && damaged.is_empty());
            assert!(checked.skill_ids.unwrap().is_empty());
            assert!(store.item_status("debug").is_none());
            assert_eq!(std::fs::read(outside.join("SKILL.md")).unwrap(), b"different external bytes");
            #[cfg(windows)]
            junction::delete(linked).unwrap();
            #[cfg(unix)]
            std::fs::remove_file(linked).unwrap();
        }
    }

    #[test]
    fn catalog_launch_rechecks_support_files_and_servers_without_blaming_io_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store =
            store(tmp.path(), &[("debug", "debugging", 100), ("design", "design", 100)]).with_node("C:/fixture/node.exe".into());
        std::fs::create_dir(store.skills().join("debug/scripts")).unwrap();
        std::fs::write(store.skills().join("debug/scripts/run.py"), "trusted script").unwrap();
        store.items[0].digest = digest(&store.dir(&store.items[0])).unwrap();
        let server_dir = store.servers().join("measure");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(server_dir.join("run.js"), "trusted server").unwrap();
        store.items.push(Item {
            id: "measure".into(),
            job: "measure".into(),
            digest: digest(&server_dir).unwrap(),
            server: Some(super::super::Server {
                package: "fixture".into(),
                version: "1".into(),
                entry: "run.js".into(),
                args: vec![],
                env: Default::default(),
            }),
            ..store.items[0].clone()
        });
        store.write_ledger(&store.items.iter().map(|item| (item.id.clone(), item.commit.clone())).collect()).unwrap();
        let (kit, _) = store.kit("debug devtools", None, Area::Project, Permissions::FullAccess, &[]);
        std::fs::write(kit.skills.as_ref().unwrap().join("debug/scripts/run.py"), "injected script").unwrap();
        std::fs::write(&kit.servers[0].args[0], "injected server").unwrap();
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let (checked, damaged, io_failed) = kit.verified_for_launch(vendor);
            assert_eq!(damaged, ["debug", "measure"]);
            assert!(!io_failed && checked.servers.is_empty());
            assert!(!checked.skill_ids.unwrap().contains(&"debug".into()));
        }
        assert_eq!(store.item_status("measure").as_deref(), Some("error.tools.reinstall"));
        let (mut continuation, _) = store.ready_kit_for(Vendor::Codex, "go on", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(!store.retain_servers(&mut continuation, &["measure".into()], Area::Project, Permissions::FullAccess, &[]));
        assert!(continuation.servers.is_empty(), "retaining a saved server cannot bypass its launch digest check");
        let healthy = kit.skills.as_ref().unwrap().join("design");
        std::fs::rename(&healthy, healthy.with_extension("hidden")).unwrap();
        let (_, damaged, io_failed) = kit.verified_for_launch(Vendor::Claude);
        assert!(io_failed && !damaged.contains(&"design".into()));
        assert!(store.item_status("design").is_none(), "an unreadable path is an I/O error, not a digest mismatch");
    }

    #[test]
    fn snapshot_copy_holds_no_store_lock_and_stale_generation_never_publishes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 1000)]);
        let (started, ready) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        *store.ready.copy_hook.lock().unwrap() = Some((started, gate.clone()));
        let worker_store = store.clone();
        let worker =
            std::thread::spawn(move || worker_store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]));
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        // The worker cannot continue until this thread mutates the store and releases the barrier.
        store.set_enabled("debug", false).unwrap();
        assert!(!store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]).0.catalog_ready);
        gate.wait();
        worker.join().unwrap();
        store.wait_prepared_for_test();
        let (kit, report) = store.ready_kit_for(Vendor::Codex, "design", None, Area::Chat, Permissions::Ask, &[]);
        assert!(kit.catalog_ready);
        assert_eq!(report.ids, ["design"]);
        assert!(!kit.skills.unwrap().join("debug").exists());
        assert_eq!(std::fs::read_dir(store.root.join("kits")).unwrap().count(), 1);
    }

    #[test]
    fn animation_performance_requires_animation_or_interface_evidence_in_five_languages() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("animation-performance", "performance", 1000)]);
        store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
        for (positive, negative) in [
            ("Optimize memory and FPS of my web animations", "Optimize memory usage in my Rust backend"),
            ("Otimize a memória e o desempenho das animações da minha interface", "Otimize o uso de memória do meu servidor Rust"),
            ("Optimiza el rendimiento de las animaciones de mi interfaz", "Optimiza el uso de memoria de mi servidor Rust"),
            ("Optimise la mémoire des animations de mon interface", "Optimise la mémoire de mon serveur Rust"),
            ("Optimiere die Leistung der Animationen meiner Oberfläche", "Optimiere den Speicherverbrauch meines Rust Servers"),
        ] {
            assert_eq!(
                store.ready_kit_for(Vendor::Codex, positive, None, Area::Chat, Permissions::Ask, &[]).1.ids,
                ["animation-performance"],
                "{positive}"
            );
            assert!(store.ready_kit_for(Vendor::Codex, negative, None, Area::Chat, Permissions::Ask, &[]).1.ids.is_empty(), "{negative}");
        }
    }

    #[test]
    fn unavailable_failed_and_genuinely_empty_catalogs_are_distinct() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        let cold = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]).0;
        assert!(!cold.catalog_ready);
        assert_eq!(cold.server_identity(), None);
        std::fs::write(store.root.join("kits"), "not a snapshot directory").unwrap();
        store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
        let (failed, report) = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(!failed.catalog_ready);
        assert_eq!(report.error.as_deref(), Some("error.tools.snapshot"));
        std::fs::remove_file(store.root.join("kits")).unwrap();
        store.ready.data.lock().unwrap().retry_at = Some(std::time::Instant::now());
        store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
        assert!(
            store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]).0.catalog_ready,
            "ordinary I/O failures retry after their bounded backoff"
        );
        store.set_enabled("debug", false).unwrap();
        store.wait_prepared_for_test();
        let (empty, report) = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(empty.catalog_ready && empty.servers.is_empty() && empty.plugin.is_none());
        assert!(report.error.is_none());
        assert_eq!(empty.server_identity().as_deref(), Some("servers:none"));
    }

    #[test]
    fn corrupt_items_need_reinstall_without_blocking_healthy_items_or_rebuilding_each_send() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100), ("design", "design", 100)]);
        let source = store.skills().join("debug/SKILL.md");
        std::fs::write(&source, "corrupt").unwrap();
        store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
        store.wait_prepared_for_test();
        let statuses = store.checked_statuses();
        assert_eq!(statuses.get("debug").map(String::as_str), Some("error.tools.reinstall"));
        assert!(!statuses.contains_key("design"));
        let (healthy, report) = store.ready_kit_for(Vendor::Codex, "design and debug", None, Area::Chat, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["design"]);
        assert!(report.error.is_none() && healthy.catalog_ready);
        std::fs::write(source, "x".repeat(100)).unwrap();
        for _ in 0..3 {
            store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]);
            assert!(store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]).1.ids.is_empty());
            assert_eq!(store.item_status("debug").as_deref(), Some("error.tools.reinstall"));
        }
        store.set_enabled("design", false).unwrap();
        store.wait_prepared_for_test();
        assert_eq!(store.kit("debug", None, Area::Chat, Permissions::Ask, &[]).1.ids, ["debug"]);
        assert!(store.item_status("debug").is_none());
    }

    #[test]
    fn a_held_tampered_snapshot_is_never_retrusted_after_switching_back_to_its_state() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100), ("design", "design", 100)]);
        let (held, _) = store.kit("debug and design", None, Area::Project, Permissions::Ask, &[]);
        std::fs::create_dir(held.plugin.as_ref().unwrap().join("unexpected")).unwrap();
        store.set_enabled("design", false).unwrap();
        store.wait_prepared_for_test();
        store.set_enabled("design", true).unwrap();
        store.wait_prepared_for_test();
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let (kit, report) = store.ready_kit_for(vendor, "debug and design", None, Area::Project, Permissions::Ask, &[]);
            assert!(kit.catalog_ready && report.error.is_none());
            assert_eq!(report.ids, ["debug", "design"], "a held malformed cache never damages healthy source items");
            assert_ne!(kit.plugin, held.plugin, "publish beside the held non-intact root");
            assert!(!kit.skill_instructions().unwrap_or_default().contains("Injected instructions"));
            let checked = kit.verified_claude_skills();
            assert!(checked.skill_descriptions().unwrap_or_default().contains("faber:debug;"));
        }
        assert!(store.item_status("debug").is_none());
        assert!(held.skills.as_ref().unwrap().join("debug/SKILL.md").is_file(), "another session's live copy is never deleted");
    }

    #[test]
    fn an_item_changed_during_copy_is_omitted_without_losing_healthy_resources() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100), ("design", "design", 100)]);
        let (started, ready) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        *store.ready.copy_hook.lock().unwrap() = Some((started, gate.clone()));
        let worker_store = store.clone();
        let worker =
            std::thread::spawn(move || worker_store.prepare_kit_for(Vendor::Claude, "", None, Area::Project, Permissions::Ask, &[]));
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        std::fs::write(store.skills().join("debug/SKILL.md"), "changed after the source check").unwrap();
        gate.wait();
        worker.join().unwrap();
        let (kit, report) = store.ready_kit_for(Vendor::Claude, "debug and design", None, Area::Project, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["design"]);
        assert!(kit.catalog_ready && report.error.is_none());
        assert!(!kit.skills.unwrap().join("debug").exists());
        assert_eq!(store.item_status("debug").as_deref(), Some("error.tools.reinstall"));
    }

    #[test]
    fn cleanup_sweeps_actual_interrupted_copy_prefixes_but_keeps_active_builders() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 100)]);
        for prefix in [".kit-staging-", ".staging-", ".trash-"] {
            let orphan = store.root.join(format!("{prefix}{}.lease", uuid::Uuid::new_v4()));
            std::fs::write(&orphan, []).unwrap();
            let abandoned = store.root.join(format!("{prefix}{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&abandoned).unwrap();
            std::fs::write(abandoned.join("payload"), "interrupted").unwrap();
            let active = TemporaryCopy::new(&store.root, prefix).unwrap();
            std::fs::create_dir(&active.path).unwrap();
            std::fs::write(active.path.join("payload"), "copying").unwrap();
            store.sweep_kits();
            assert!(!orphan.exists(), "orphaned zero-byte markers are swept");
            assert!(!abandoned.exists(), "{prefix}");
            assert!(active.path.join("payload").is_file(), "{prefix}");
        }
        let (started, ready) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        *store.ready.copy_hook.lock().unwrap() = Some((started, gate.clone()));
        let worker_store = store.clone();
        let worker = std::thread::spawn(move || worker_store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]));
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let staging = std::fs::read_dir(&store.root)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir()) && entry.file_name().to_string_lossy().starts_with(".kit-staging-")
            })
            .unwrap()
            .path();
        store.sweep_kits();
        assert!(staging.is_dir(), "the real active copy is leased before any bytes are copied");
        gate.wait();
        worker.join().unwrap();
        assert!(!staging.exists());
        assert!(store.ready_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]).0.catalog_ready);
    }

    #[test]
    fn pages_plants_and_arithmetic_do_not_select_software_tools_in_any_shipped_language() {
        for (positive, negative) in [
            ("Create a web page for my shop", "Create a summary of page 3"),
            ("Crie uma página web pra minha loja", "Crie um resumo da página 5"),
            ("Crea una página web para mi tienda", "Crea un resumen de la página 3"),
            ("Crée une page web pour ma boutique", "Fais un résumé de cette page"),
            ("Erstelle eine Webseite für meinen Laden", "Erstelle eine Zusammenfassung von Seite 3"),
        ] {
            assert!(labels(positive, None).0.contains("design"), "{positive}");
            assert!(!labels(negative, None).0.contains("design"), "{negative}");
        }
        for (positive, negative) in [
            ("Fix the error in my app", "What is the error in this calculation?"),
            ("Corrige o erro no meu aplicativo", "Qual o erro nesta conta?"),
            ("Arregla el error en mi aplicación", "Cuál es el error en esta cuenta?"),
            ("Mon application plante, répare-la", "Quelle plante pour mon salon ?"),
            ("Behebe den Fehler in meiner Anwendung", "Welcher Fehler steckt in dieser Rechnung?"),
        ] {
            assert!(labels(positive, None).0.contains("debugging"), "{positive}");
            assert!(!labels(negative, None).0.contains("debugging"), "{negative}");
        }
    }

    #[test]
    fn ready_selection_never_waits_on_store_preparation_and_switches_filter_old_kits() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 1000)]);
        store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        let _gate = lock();
        let (kit, report) = store.ready_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        assert_eq!(report.ids, ["debug"]);
        assert!(kit.skill_instructions().unwrap().contains("Catalog skill debug"));
        drop(_gate);
        store.set_enabled("debug", false).unwrap();
        assert!(store.ready_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]).1.ids.is_empty());
        store.prepare_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        store.wait_prepared_for_test();
        assert!(store.ready_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]).1.ids.is_empty());
        assert!(store.ready_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Isolated, &[]).1.ids.is_empty());
    }

    #[test]
    fn startup_prepares_every_resource_and_alternating_tasks_and_helpers_pick_immediately() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 1000)]);
        store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
        for (task, expected) in [
            ("debug the failing test", "debug"),
            ("Create a website", "design"),
            ("Crie um site", "design"),
            ("debug helper task", "debug"),
        ] {
            let (kit, report) = store.ready_kit_for(Vendor::Codex, task, None, Area::Project, Permissions::FullAccess, &[]);
            assert!(kit.catalog_ready);
            assert_eq!(report.ids, [expected]);
            assert!(kit.skill_instructions().unwrap().contains(&format!("Catalog skill {expected}")));
        }
        assert!(store.ready_kit_for(Vendor::Codex, "hello", None, Area::Chat, Permissions::Ask, &[]).1.ids.is_empty());
        let (claude, report) = store.ready_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["debug"], "visible line describes only task matches");
        assert!(claude.skill_descriptions().unwrap().contains("faber:design;"));
    }

    #[test]
    fn cached_verified_resources_are_reused_without_rehashing_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 1000)]);
        let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let text = kit.skill_instructions();
        assert!(kit.skills.as_ref().unwrap().join("design").exists());
        std::fs::write(store.skills().join("debug/SKILL.md"), "source modified after verified snapshot").unwrap();
        assert_eq!(store.kit("debug", None, Area::Project, Permissions::Ask, &[]).0.skill_instructions(), text);
    }

    #[test]
    fn preparation_coalesces_scopes_and_keeps_distinct_trusted_catalogs_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000)]);
        store.ready.building.store(true, Ordering::Release);
        store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Chat, Permissions::Ask, &[]);
        store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        store.prepare_kit_for(Vendor::Codex, "debug latest", None, Area::Chat, Permissions::Ask, &[]);
        assert!(store.ready.data.lock().unwrap().pending);
        store.drain_pending();
        for (area, permissions) in [(Area::Chat, Permissions::Ask), (Area::Project, Permissions::FullAccess)] {
            assert_eq!(store.ready_kit_for(Vendor::Codex, "debug", None, area, permissions, &[]).1.ids, ["debug"]);
        }
        let mut changed = store.items.clone();
        changed[0].digest = "different trusted pin".into();
        let other = Store::with_catalog(tmp.path(), changed);
        assert!(!Arc::ptr_eq(&store.ready, &other.ready));
        assert!(other.ready_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]).1.ids.is_empty());
    }

    #[test]
    fn a_worker_inventory_refresh_cannot_overwrite_a_newer_disabled_switch() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000)]);
        store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        let gate = lock();
        let worker_store = store.clone();
        let (started, waiting) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            worker_store.refresh_ready_inventory();
        });
        waiting.recv().unwrap();
        let mut selection = store.selection();
        selection.disabled.insert("debug".into());
        store.save_selection(&selection).unwrap();
        store.refresh_ready_inventory_locked();
        drop(gate);
        worker.join().unwrap();
        assert!(store.ready_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]).1.ids.is_empty());
    }

    #[test]
    fn catalog_changes_from_a_plain_thread_prepare_the_next_session_without_send() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 1000)]);
        store.prepare_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        store.ready_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        let changed = store.clone();
        std::thread::spawn(move || changed.set_enabled("debug", false).unwrap()).join().unwrap();
        store.wait_prepared_for_test();
        let (kit, report) = store.ready_kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(report.ids.is_empty());
        assert!(kit.skill_descriptions().unwrap().contains("faber:design;"));
        assert!(!kit.skill_descriptions().unwrap().contains("faber:debug;"));
    }

    #[test]
    fn effective_server_identity_and_paths_ignore_skill_and_unselected_catalog_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store =
            store(tmp.path(), &[("debug", "debugging", 1000), ("design", "design", 14_000)]).with_node("C:/fixture/node.exe".into());
        let dir = store.servers().join("measure");
        std::fs::create_dir_all(dir.join("dist")).unwrap();
        std::fs::write(dir.join("dist/run.js"), "trusted server").unwrap();
        store.items.push(Item {
            id: "measure".into(),
            job: "measure".into(),
            digest: digest(&dir).unwrap(),
            server: Some(super::super::Server {
                package: "fixture".into(),
                version: "1".into(),
                entry: "dist/run.js".into(),
                args: vec![],
                env: Default::default(),
            }),
            ..store.items[0].clone()
        });
        store.write_ledger(&store.items.iter().map(|item| (item.id.clone(), item.commit.clone())).collect()).unwrap();
        let (first, _) = store.kit("debug devtools", None, Area::Project, Permissions::FullAccess, &[]);
        let key = first.server_identity();
        let script = first.servers[0].args[0].clone();
        let mut old_delivery = first.clone();
        let (mut old_continuation, _) = store.kit("Go on", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(Path::new(&script).is_file());
        let (mut design, report) = store.kit("Design a landing page", None, Area::Project, Permissions::FullAccess, &[]);
        assert_eq!(report.ids, ["design"]);
        assert!(design.skill_instructions().unwrap().len() > CONTEXT_BUDGET - SERVER_COST);
        assert!(store.retain_servers(&mut design, &["measure".into()], Area::Project, Permissions::FullAccess, &[]));
        assert!(design.skill_ids.as_ref().unwrap().is_empty(), "retained native server schemas reserve budget before new instructions");
        assert!(design.skill_instructions().is_none());
        drop(design);
        store.set_enabled("design", false).unwrap();
        let (same, _) = store.kit("debug devtools", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(
            !store.retain_servers(&mut old_delivery, &["measure".into()], Area::Project, Permissions::FullAccess, &[]),
            "publication cannot attach current paths to a delivery that only leases the previous snapshot"
        );
        assert_eq!(old_delivery.servers[0].args[0], script);
        drop(old_delivery);
        assert!(!store.retain_servers(&mut old_continuation, &["measure".into()], Area::Project, Permissions::FullAccess, &[]));
        assert!(!old_continuation.catalog_ready, "publication is unavailable retention, never an authoritative empty-server pick");
        assert!(old_continuation.servers.is_empty());
        assert_eq!(store.server_identity_for(&["measure".into()], Area::Project, Permissions::FullAccess, &[]), key.clone().unwrap());
        drop(old_continuation);
        assert_eq!(same.server_identity(), key);
        assert_ne!(same.servers[0].args[0], script, "a new catalog state has one new shared folder");
        let (mut continuation, report) = store.kit("go on", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(report.ids.is_empty() && continuation.servers.is_empty());
        assert!(store.retain_servers(&mut continuation, &["measure".into()], Area::Project, Permissions::FullAccess, &[]));
        assert_eq!(continuation.server_identity(), key);
        assert_eq!(
            continuation.servers[0].args[0], same.servers[0].args[0],
            "resume refreshes script paths from the current shared snapshot"
        );
        assert!(!store.retain_servers(&mut continuation, &["measure".into()], Area::Project, Permissions::Ask, &[]));
        store.set_enabled("debug", false).unwrap();
        let (changed_skills, _) = store.kit("devtools", None, Area::Project, Permissions::FullAccess, &[]);
        assert_eq!(changed_skills.server_identity(), key);
        assert_ne!(changed_skills.servers[0].args[0], script);
        drop(first);
        drop(same);
        drop(continuation);
        store.sweep_kits();
        assert!(!Path::new(&script).is_file(), "released obsolete snapshots are swept");
        store.set_enabled("measure", false).unwrap();
        assert_ne!(store.kit("devtools", None, Area::Project, Permissions::FullAccess, &[]).0.server_identity(), key);
    }

    #[test]
    #[ignore = "manual realistic-size disk benchmark"]
    fn realistic_catalog_snapshot_benchmark() {
        if let Some(data_dir) = std::env::var_os("FABER_KIT_BENCH_DATA") {
            let store = Store::new(Path::new(&data_dir));
            let task = "Design a website and measure devtools performance";
            let start = std::time::Instant::now();
            let pending = store.ready_kit_for(Vendor::Codex, task, None, Area::Project, Permissions::FullAccess, &[]);
            eprintln!("shipped catalog pending Send: {} us; ready={}", start.elapsed().as_micros(), pending.0.catalog_ready);
            let start = std::time::Instant::now();
            store.prepare_kit_for(Vendor::Codex, "", None, Area::Chat, Permissions::Ask, &[]);
            store.wait_prepared();
            eprintln!("shipped catalog worker prepare: {} ms", start.elapsed().as_millis());
            let mut roots = BTreeSet::new();
            for vendor in [Vendor::Claude, Vendor::Codex] {
                for area in [Area::Chat, Area::Project] {
                    for permissions in [Permissions::FullAccess, Permissions::Ask] {
                        let start = std::time::Instant::now();
                        let (kit, report) = store.ready_kit_for(vendor, task, None, area, permissions, &[]);
                        assert!(kit.catalog_ready && report.error.is_none(), "{report:?}");
                        if let Some(root) = kit.identity() {
                            roots.insert(root);
                        }
                        let text = if vendor == Vendor::Claude { kit.skill_descriptions() } else { kit.skill_instructions() };
                        eprintln!(
                            "shipped ready Send {vendor:?}/{area:?}/{permissions:?}: {} us; {} ids; {} text bytes",
                            start.elapsed().as_micros(),
                            report.ids.len(),
                            text.as_ref().map_or(0, String::len)
                        );
                    }
                }
            }
            assert_eq!(roots.len(), 1);
            let start = std::time::Instant::now();
            store.prepare_kit_for(Vendor::Claude, "unrelated helper", None, Area::Project, Permissions::FullAccess, &[]);
            eprintln!("shipped catalog warm worker prepare: {} us", start.elapsed().as_micros());
            eprintln!(
                "shipped catalog snapshot bytes: {}; installed items: {}",
                tree_bytes(&store.root.join("kits")),
                store.installed().len()
            );
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let names: Vec<_> = (0..114).map(|n| format!("skill-{n:03}")).collect();
        let specs: Vec<_> = names.iter().map(|name| (name.as_str(), "design", 4000)).collect();
        let mut store = store(tmp.path(), &specs);
        for item in &mut store.items {
            let dir = tmp.path().join("tools/plugin/skills").join(&item.id);
            for n in 0..12 {
                std::fs::write(dir.join(format!("support-{n}.txt")), vec![b'x'; 24 * 1024]).unwrap();
            }
            item.digest = digest(&dir).unwrap();
        }
        let server_dir = store.servers().join("measure");
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(server_dir.join("run.js"), vec![b'x'; 15 * 1024 * 1024]).unwrap();
        store.items.push(Item {
            id: "measure".into(),
            job: "measure".into(),
            digest: digest(&server_dir).unwrap(),
            server: Some(super::super::Server {
                package: "measure".into(),
                version: "1".into(),
                entry: "run.js".into(),
                args: vec![],
                env: Default::default(),
            }),
            ..store.items[0].clone()
        });
        store.write_ledger(&store.items.iter().map(|item| (item.id.clone(), "pin".into())).collect()).unwrap();
        let task = "Design a website and measure devtools performance";
        let cold = std::time::Instant::now();
        assert!(store.ready_kit_for(Vendor::Claude, task, None, Area::Project, Permissions::FullAccess, &[]).1.ids.is_empty());
        eprintln!("realistic catalog pending Send: {} us", cold.elapsed().as_micros());
        for area in [Area::Project, Area::Chat] {
            for permissions in [Permissions::FullAccess, Permissions::Ask] {
                let start = std::time::Instant::now();
                store.prepare_kit_for(Vendor::Claude, task, None, area, permissions, &[]);
                eprintln!("realistic catalog off-lock prepare {area:?}/{permissions:?}: {} ms", start.elapsed().as_millis());
                for vendor in [Vendor::Claude, Vendor::Codex] {
                    if vendor == Vendor::Codex {
                        store.prepare_kit_for(vendor, task, None, area, permissions, &[]);
                    }
                    let _gate = lock();
                    let start = std::time::Instant::now();
                    let (kit, report) = store.ready_kit_for(vendor, task, None, area, permissions, &[]);
                    let text = if vendor == Vendor::Claude { kit.skill_descriptions() } else { kit.skill_instructions() };
                    eprintln!(
                        "realistic catalog ready Send {vendor:?}/{area:?}/{permissions:?}: {} us, ids={}, text={} bytes",
                        start.elapsed().as_micros(),
                        report.ids.len(),
                        text.as_ref().map_or(0, String::len)
                    );
                    assert!(!report.ids.is_empty());
                }
            }
        }
        eprintln!("realistic catalog total snapshot bytes: {}", tree_bytes(&store.root.join("kits")));
        let data = store.ready.data.lock().unwrap();
        eprintln!(
            "realistic catalog retained text bytes: {}",
            data.entry
                .iter()
                .flat_map(|entry| entry.texts.values())
                .map(|(body, description)| body.len() + description.len())
                .sum::<usize>()
        );
    }

    fn store(dir: &Path, skills: &[(&str, &str, usize)]) -> Store {
        let root = dir.join("tools");
        let mut ledger = std::collections::BTreeMap::new();
        let items = skills
            .iter()
            .map(|(id, job, len)| {
                let skill = root.join("plugin/skills").join(id);
                std::fs::create_dir_all(&skill).unwrap();
                std::fs::write(skill.join("SKILL.md"), "x".repeat(*len)).unwrap();
                ledger.insert((*id).to_string(), "pin".to_string());
                Item {
                    id: (*id).into(),
                    cli_name: None,
                    job: (*job).into(),
                    tier: Default::default(),
                    license: "MIT".into(),
                    repo: "test/fixture".into(),
                    commit: "pin".into(),
                    path: "skills".into(),
                    digest: digest(&skill).unwrap(),
                    server: None,
                    chat_safe: false,
                }
            })
            .collect();
        let store = Store::with_catalog(dir, items);
        store.write_manifest().unwrap();
        store.write_ledger(&ledger).unwrap();
        store
    }

    #[test]
    fn full_skill_text_has_a_conservative_context_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("one", "design", 9_000), ("two", "design", 9_000), ("huge", "design", CONTEXT_BUDGET + 1)]);
        let (kit, report) = store.kit("design a website", None, Area::Chat, Permissions::FullAccess, &[]);
        assert_eq!(report.ids.len(), 1);
        let text = kit.skill_instructions().unwrap();
        assert!(text.len() <= CONTEXT_BUDGET);
        assert!(text.contains(&"x".repeat(9_000)), "the chosen skill is whole, never truncated instructions");
    }

    #[test]
    fn aliased_cli_skill_names_keep_catalog_ids_for_selection_and_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = store(tmp.path(), &[("catalog-debug", "debugging", 10)]);
        store.items[0].cli_name = Some("native-debug".into());
        let (kit, report) = store.kit_for(Vendor::Claude, "debug my app crash", None, Area::Project, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["catalog-debug"]);
        assert_eq!(kit.skill_ids.as_deref(), Some(["catalog-debug".to_string()].as_slice()));
        let (checked, damaged, io_failed) = kit.verified_for_launch(Vendor::Claude);
        assert!(damaged.is_empty() && !io_failed);
        let index = checked.skill_descriptions().unwrap();
        assert!(index.contains("faber:native-debug; job=debugging;"));
        assert!(index.contains("catalog-debug"), "the verified file remains stored under its catalog id");
    }

    #[test]
    fn code_choices_stay_task_specific() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10), ("design", "design", 10), ("test", "testing", 10)]);
        let args = ("debug and design", None, Area::Chat, Permissions::FullAccess, &[][..]);
        let choice = store.shortlist(args.0, args.1, args.2, args.3, args.4);
        assert_eq!(choice.jobs, ["debugging", "design"]);
        let (_, report) = store.kit(args.0, args.1, args.2, args.3, args.4);
        assert_eq!(report.ids, ["debug", "design"]);
        assert_eq!(report.method, "code");
    }

    #[test]
    fn switched_off_or_uninstalled_items_never_reach_a_new_kit() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10), ("test", "testing", 10)]);
        store.set_enabled("debug", false).unwrap();
        store.remove("test").unwrap();
        let (kit, report) = store.kit("debug and test", None, Area::Project, Permissions::FullAccess, &[]);
        assert!(kit.plugin.is_none() && kit.servers.is_empty());
        assert!(report.ids.is_empty());
        store.set_always("debug", true).unwrap();
        assert!(store.kit("debug", None, Area::Project, Permissions::Ask, &[]).1.ids.is_empty(), "off wins over always");
    }

    #[test]
    fn a_session_snapshot_stays_whole_until_a_stale_cache_is_swept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10)]);
        let (kit, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let root = kit.plugin.clone().unwrap();
        let copy = kit.clone();
        store.remove("debug").unwrap();
        assert_eq!(std::fs::read(root.join("skills/debug/SKILL.md")).unwrap(), b"xxxxxxxxxx");
        drop(kit);
        assert!(root.is_dir());
        drop(copy);
        store.sweep_kits();
        assert!(!root.exists());
    }

    #[test]
    fn identical_sessions_reuse_one_cached_state_after_the_last_lease_ends() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10)]);
        let (first, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let (second, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let path = first.plugin.clone().unwrap();
        assert_eq!(second.plugin.as_ref(), Some(&path));
        assert!(
            Arc::ptr_eq(first.lease.as_ref().unwrap(), second.lease.as_ref().unwrap()),
            "the prepared snapshot shares its OS lease across sessions"
        );
        Store::new(tmp.path()).sweep_kits();
        assert!(path.is_dir());
        drop(first);
        assert!(path.is_dir(), "closing one session leaves the other session's files");
        drop(second);
        assert!(path.exists(), "closing the last session retains the current catalog cache");
        let (resumed, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        assert_eq!(resumed.plugin.as_ref(), Some(&path), "native resumed threads may retain the earlier server script paths");
    }

    #[test]
    fn a_mutated_shared_snapshot_fails_closed_without_deleting_other_sessions_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10)]);
        let (active, _) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        let path = active.plugin.clone().unwrap();
        std::fs::write(path.join("skills/debug/SKILL.md"), "mutated").unwrap();
        {
            let mut data = store.ready.data.lock().unwrap();
            data.entry = None;
            data.attempted = None;
        } // Simulate application restart.
        let (excluded, report) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert!(excluded.catalog_ready && excluded.plugin.is_none() && report.ids.is_empty());
        assert_eq!(store.item_status("debug").as_deref(), Some("error.tools.reinstall"));
        assert!(path.is_dir(), "failed reuse cannot delete a live session's copy");
        drop(active);
        assert!(path.exists());
        std::fs::remove_file(store.skills().join("debug/SKILL.md")).unwrap();
        std::fs::write(store.skills().join("debug/SKILL.md"), b"xxxxxxxxxx").unwrap();
        store.set_enabled("debug", false).unwrap();
        store.set_enabled("debug", true).unwrap();
        let (recreated, _) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert_eq!(recreated.plugin, Some(path.clone()));
        assert_eq!(std::fs::read(path.join("skills/debug/SKILL.md")).unwrap(), b"xxxxxxxxxx");
    }

    #[test]
    fn a_changed_catalog_digest_gets_a_distinct_snapshot_path() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = store(tmp.path(), &[("debug", "debugging", 10)]);
        let (old, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        let installed = store.dir(&store.items[0]);
        std::fs::remove_file(installed.join("SKILL.md")).unwrap();
        std::fs::write(installed.join("SKILL.md"), "new pinned instructions").unwrap();
        store.items[0].digest = digest(&installed).unwrap();
        let (new, _) = store.kit("debug", None, Area::Project, Permissions::Ask, &[]);
        assert_ne!(old.plugin, new.plugin);
        assert_eq!(std::fs::read(old.skills.as_ref().unwrap().join("debug/SKILL.md")).unwrap(), b"xxxxxxxxxx");
        let previous = old.plugin.clone().unwrap();
        store.sweep_kits();
        assert!(previous.is_dir(), "a changed catalog cannot sweep an active session's instructions");
        drop(old);
        store.sweep_kits();
        assert!(!previous.exists());
        assert!(new.plugin.unwrap().is_dir(), "sweeping retains the current catalog state");
    }

    #[test]
    fn all_vendors_areas_and_modes_share_one_snapshot_with_independent_task_picks() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 20), ("design", "design", 20)]);
        store.prepare_kit_for(Vendor::Codex, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        let mut roots = BTreeSet::new();
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for area in [Area::Chat, Area::Project] {
                for permissions in
                    [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan, Permissions::FullAccess]
                {
                    for (task, expected) in [("debug", "debug"), ("design", "design")] {
                        let (kit, report) = store.ready_kit_for(vendor, task, None, area, permissions, &[]);
                        assert_eq!(report.ids, [expected]);
                        roots.insert(kit.plugin.unwrap());
                    }
                }
            }
        }
        assert_eq!(roots.len(), 1);
        assert_eq!(std::fs::read_dir(store.root.join("kits")).unwrap().count(), 1);
    }

    #[test]
    fn cached_instruction_bytes_decide_the_budget_even_if_the_source_is_later_shrunk() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("one", "design", 9000), ("two", "design", 9000)]);
        let (first, report) = store.kit("design", None, Area::Project, Permissions::Ask, &[]);
        assert_eq!(report.ids.len(), 1);
        for item in &store.items {
            std::fs::write(store.dir(item).join("SKILL.md"), b"x").unwrap();
        }
        drop(first);
        let (next, report) = store.kit("design", None, Area::Project, Permissions::Ask, &[]);
        assert_eq!(report.ids.len(), 1, "the verified snapshot, not changed source bytes, determines the budget");
        assert!(next.skill_instructions().unwrap().len() <= CONTEXT_BUDGET);
        store.set_enabled("two", false).unwrap();
        assert!(
            store.kit("design", None, Area::Project, Permissions::Ask, &[]).1.ids.is_empty(),
            "a changed state must validate source digests again"
        );
    }

    #[test]
    fn always_on_is_small_and_never_enters_an_internal_request() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("one", "design", 10), ("two", "testing", 10), ("three", "debugging", 10), ("four", "plan", 10)]);
        for id in ["one", "two", "three"] {
            store.set_always(id, true).unwrap();
        }
        assert!(store.set_always("four", true).is_err());
        assert_eq!(store.kit("hello", None, Area::Chat, Permissions::Ask, &[]).1.ids.len(), 3);
        assert!(store.kit("design", None, Area::Project, Permissions::Isolated, &[]).0.catalog_ready);
    }

    #[test]
    fn broken_switches_fail_closed_and_stack_fingerprints_read_only_fixed_file_names() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("lean", "lean", 10)]);
        std::fs::write(tmp.path().join("Cargo.toml"), "not parsed or sent to any model").unwrap();
        assert_eq!(store.kit("hello", Some(tmp.path()), Area::Project, Permissions::Ask, &[]).1.ids, ["lean"]);
        std::fs::write(store.root.join("selection.json"), b"broken").unwrap();
        assert!(store.kit("code", None, Area::Chat, Permissions::FullAccess, &[]).1.ids.is_empty());
        assert!(store.set_enabled("lean", true).is_err());
    }

    #[test]
    fn shipped_stack_identities_prevent_cross_framework_and_consumer_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let ids = [
            ("swift-concurrency", "mobile", 20),
            ("expo-router", "mobile", 20),
            ("flutter-add-widget-test", "mobile", 20),
            ("jetpack-compose-performance", "mobile", 20),
            ("react-native-best-practices", "mobile", 20),
            ("prisma-client-api", "database", 20),
            ("firebase-firestore", "database", 20),
            ("supabase-postgres-best-practices", "database", 20),
            ("grill-me", "plan", 20),
            ("systematic-debugging", "debugging", 20),
            ("design-taste-frontend", "design", 20),
            ("mcp-builder", "mcp", 20),
            ("skill-creator", "skills", 20),
        ];
        for (id, job, _) in &ids {
            assert_eq!(super::super::find(id).unwrap().job, *job, "test realistic shipped identities");
        }
        let store = store(tmp.path(), &ids);
        for task in ["Fix Swift concurrency in my iOS app", "Corrige a concorrência Swift no meu aplicativo iOS"] {
            let choice = store.shortlist(task, None, Area::Chat, Permissions::Ask, &[]);
            assert!(choice.ids.iter().any(|id| id == "swift-concurrency"));
            assert!(!choice.ids.iter().any(|id| {
                ["expo-router", "flutter-add-widget-test", "jetpack-compose-performance", "react-native-best-practices"]
                    .contains(&id.as_str())
            }));
        }
        for task in [
            "Qual o melhor plano de celular?",
            "Analise os dados desta planilha",
            "Meu banco tem um aplicativo e site",
            "Como publicar a tela de login no servidor?",
            "What are your skills?",
        ] {
            assert!(store.shortlist(task, None, Area::Chat, Permissions::Ask, &[]).ids.is_empty(), "{task}");
        }
        for task in ["corrige meu app", "conserta meu site", "não funciona", "travando"] {
            assert_eq!(store.shortlist(task, None, Area::Chat, Permissions::Ask, &[]).ids, ["systematic-debugging"]);
        }
        std::fs::write(tmp.path().join("schema.prisma"), "ignored contents").unwrap();
        assert_eq!(store.shortlist("hello", Some(tmp.path()), Area::Project, Permissions::Ask, &[]).ids, ["prisma-client-api"]);
        std::fs::remove_file(tmp.path().join("schema.prisma")).unwrap();
        std::fs::write(tmp.path().join("pubspec.yaml"), "ignored contents").unwrap();
        assert_eq!(store.shortlist("hello", Some(tmp.path()), Area::Project, Permissions::Ask, &[]).ids, ["flutter-add-widget-test"]);
    }

    #[test]
    fn realistic_creation_design_and_failure_requests_work_in_every_shipped_language() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("systematic-debugging", "debugging", 20), ("design-taste-frontend", "design", 20)]);
        for (language, design, debug, unrelated) in [
            (
                "en",
                vec!["Create a website for my bakery", "Make a sales page", "Create a landing page", "Improve the look of my website"],
                "My app crashes when opening",
                vec!["What's the best mobile plan?", "My bank has an app and a website", "Fix the typo in README"],
            ),
            (
                "pt",
                vec![
                    "Crie um site pra minha padaria",
                    "Faça uma página de vendas",
                    "Crie uma landing page",
                    "Melhore o visual do meu site",
                ],
                "Meu app trava ao abrir",
                vec!["Qual o melhor plano de celular?", "Meu banco tem um aplicativo e site", "Corrija o erro de digitação no README"],
            ),
            (
                "es",
                vec![
                    "Crea un sitio para mi panadería",
                    "Haz una página de ventas",
                    "Crea una landing page",
                    "Mejora el aspecto de mi sitio",
                ],
                "Mi app se bloquea al abrir",
                vec!["¿Cuál es el mejor plan de móvil?", "Mi banco tiene una aplicación y un sitio", "Corrige la errata en README"],
            ),
            (
                "fr",
                vec![
                    "Crée un site pour ma boulangerie",
                    "Fais une page de vente",
                    "Crée une landing page",
                    "Améliore l'apparence de mon site",
                ],
                "Mon application plante au démarrage",
                vec![
                    "Quel est le meilleur forfait mobile ?",
                    "Ma banque a une application et un site",
                    "Corrige la faute de frappe dans README",
                ],
            ),
            (
                "de",
                vec![
                    "Erstelle eine Website für meine Bäckerei",
                    "Baue eine Verkaufsseite",
                    "Erstelle eine Landing Page",
                    "Verbessere das Aussehen meiner Webseite",
                ],
                "Meine App stürzt beim Öffnen ab",
                vec![
                    "Welcher Handytarif ist am besten?",
                    "Meine Bank hat eine App und eine Website",
                    "Korrigiere den Tippfehler in README",
                ],
            ),
        ] {
            for task in design {
                assert_eq!(
                    store.shortlist(task, None, Area::Chat, Permissions::Ask, &[]).ids,
                    ["design-taste-frontend"],
                    "{language}: {task}"
                );
            }
            assert_eq!(
                store.shortlist(debug, None, Area::Chat, Permissions::Ask, &[]).ids,
                ["systematic-debugging"],
                "{language}: {debug}"
            );
            for task in unrelated {
                assert!(store.shortlist(task, None, Area::Chat, Permissions::Ask, &[]).ids.is_empty(), "{language}: {task}");
            }
        }
    }

    #[test]
    fn claude_descriptions_keep_enabled_skills_while_codex_instructions_remain_task_specific() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10), ("design", "design", CONTEXT_BUDGET + 1), ("off", "design", 10)]);
        store.set_enabled("off", false).unwrap();
        let (_kit, report) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["debug"]);
        let (claude, report) = store.kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Ask, &[]);
        assert_eq!(report.ids, ["debug"]);
        assert!(
            claude.skills.as_ref().unwrap().join("design/SKILL.md").is_file(),
            "Claude indexes descriptions and reads full skills only on invocation"
        );
        let index = claude.skill_descriptions().unwrap();
        assert!(index.contains("faber:debug;") && index.contains("faber:design;"));
        assert!(!index.contains("faber:off;") && !index.contains(&"x".repeat(10)));
        assert!(index.len() <= CONTEXT_BUDGET);
        let root: PathBuf =
            serde_json::from_str(index.lines().next().unwrap().split_once("relative to ").unwrap().1.trim_end_matches('.')).unwrap();
        assert!(root.is_absolute());
        for line in index.lines().skip(1) {
            let path = PathBuf::from(line.split_once("; Read ").unwrap().1);
            assert!(path.is_relative() && root.join(path).is_file());
        }
        assert!(store.kit_for(Vendor::Claude, "debug", None, Area::Chat, Permissions::Isolated, &[]).0.catalog_ready);
    }

    #[test]
    fn startup_removes_abandoned_copies_but_never_live_other_store_leases() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("debug", "debugging", 10)]);
        let (kit, _) = store.kit("debug", None, Area::Chat, Permissions::Ask, &[]);
        let live = kit.plugin.clone().unwrap();
        let parent = store.root.join("kits");
        let orphan = parent.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&orphan).unwrap();
        std::fs::write(orphan.join(".lease"), "").unwrap();
        let interrupted = parent.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&interrupted).unwrap();
        let other = Store::new(tmp.path());
        other.sweep_kits();
        assert!(!orphan.exists() && !interrupted.exists());
        assert!(live.join("skills/debug/SKILL.md").is_file(), "another Engine cannot remove a live kit");
        other.sweep_kits();
        assert!(live.exists());
        drop(kit);
        assert!(live.exists());
    }

    #[test]
    fn project_switch_names_and_chat_server_review_are_enforced_before_the_model() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path(), &[("measure", "measure", 10)]);
        let mut item = store.items[0].clone();
        item.server = Some(super::super::Server {
            package: "measure".into(),
            version: "1".into(),
            entry: "run.js".into(),
            args: vec![],
            env: Default::default(),
        });
        assert!(!eligible(&item, Area::Chat, Permissions::FullAccess, &[]), "Chat servers default to off");
        item.chat_safe = true;
        assert!(eligible(&item, Area::Chat, Permissions::FullAccess, &[]));
        for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Plan, Permissions::Auto, Permissions::Isolated] {
            for area in [Area::Chat, Area::Project] {
                assert!(!eligible(&item, area, mode, &[]));
            }
        }
        for name in ["measure", "faber-measure"] {
            assert!(!eligible(&item, Area::Project, Permissions::FullAccess, &[name.into()]));
        }
    }
}
