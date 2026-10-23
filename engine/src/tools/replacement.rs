//! Consent-bound replacement of a held catalog copy, retaining its original bytes.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementPlan {
    item: Item,
    plugin: Option<plugins::Definition>,
    old_commit: String,
    enabled: bool,
    disabled_location: bool,
    old_digest: String,
    selection: Option<Vec<u8>>,
    previous_receipt: Option<String>,
}
impl ReplacementPlan {
    pub fn item(&self) -> &Item {
        &self.item
    }
    pub fn old_commit(&self) -> &str {
        &self.old_commit
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    plan: ReplacementPlan,
    finished: bool,
    rolled_back: bool,
}
impl Journal {
    fn write(&self, dir: &Path) -> Result<(), String> {
        let part = dir.join(format!(".state-{}", uuid::Uuid::new_v4()));
        std::fs::write(&part, serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let saved = retry_busy(|| std::fs::rename(&part, dir.join("state.json")));
        if saved.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        saved.map_err(|e| e.to_string())
    }
}

fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = meta.file_type().is_symlink();
    if linked || if directory { !meta.is_dir() } else { !meta.is_file() } {
        return Err("The stored copy contains a linked or invalid path".into());
    }
    Ok(())
}
fn exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
fn checked_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !exists(path)? {
        return Ok(None);
    }
    plain(path, false)?;
    let parent = path.parent().ok_or("Invalid metadata path")?;
    let file =
        faber_winproc::open_catalog_file(parent, Path::new(path.file_name().ok_or("Invalid metadata path")?)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Catalog metadata is too large".into());
    }
    Ok(Some(bytes))
}
fn fingerprint(path: &Path) -> Result<String, String> {
    fingerprint_bounded(path, SERVER_LIMIT, 20000, 64)
}

fn fingerprint_bounded(path: &Path, byte_limit: u64, path_limit: usize, depth_limit: usize) -> Result<String, String> {
    fn walk(root: &Path, path: &Path, remaining_depth: usize, remaining_paths: &mut usize, files: &mut Vec<String>) -> Result<(), String> {
        plain(path, true)?;
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            *remaining_paths = remaining_paths.checked_sub(1).ok_or("Catalog copy has too many paths")?;
            let entry = entry.map_err(|e| e.to_string())?;
            let is_dir = entry.file_type().map_err(|e| e.to_string())?.is_dir();
            plain(&entry.path(), is_dir)?;
            if is_dir {
                walk(
                    root,
                    &entry.path(),
                    remaining_depth.checked_sub(1).ok_or("Catalog copy is too deeply nested")?,
                    remaining_paths,
                    files,
                )?;
            } else {
                files.push(entry.path().strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    let mut remaining_paths = path_limit;
    walk(path, path, depth_limit, &mut remaining_paths, &mut files)?;
    files.sort();
    let mut total = 0u64;
    let mut hash = Sha256::new();
    for relative in files {
        let file = faber_winproc::open_catalog_file(path, Path::new(&relative)).map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        total = total.checked_add(length).ok_or("Catalog copy is too large")?;
        if total > byte_limit {
            return Err("Catalog copy is too large".into());
        }
        hash.update(relative.as_bytes());
        hash.update([0]);
        hash.update(length.to_string().as_bytes());
        hash.update([0]);
        let mut input = file.take(length + 1);
        let mut read = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            read += count as u64;
            hash.update(&buffer[..count]);
        }
        if read != length {
            return Err("The catalog copy changed during confirmation".into());
        }
    }
    Ok(crate::hex(&hash.finalize()))
}

fn copy_original(source: &Path, dest: &Path, remaining: &mut u64, paths: &mut usize, depth: usize) -> Result<(), String> {
    plain(source, true)?;
    if depth > 64 {
        return Err("The retained copy is too deeply nested".into());
    }
    std::fs::create_dir(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        *paths = paths.checked_sub(1).ok_or("The retained copy has too many paths")?;
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_original(&entry.path(), &target, remaining, paths, depth + 1)?;
        } else {
            let source = faber_winproc::open_catalog_file(source, Path::new(&entry.file_name())).map_err(|e| e.to_string())?;
            let permissions = source.metadata().map_err(|e| e.to_string())?.permissions();
            let mut target = std::fs::OpenOptions::new().write(true).create_new(true).open(target).map_err(|e| e.to_string())?;
            let copied = io::copy(&mut source.take(*remaining + 1), &mut target).map_err(|e| e.to_string())?;
            *remaining = remaining.checked_sub(copied).ok_or("The retained copy grew during recovery")?;
            target.set_permissions(permissions).map_err(|e| e.to_string())?;
            #[cfg(test)]
            if FAIL_RECOVERY_COPY.with(|fail| fail.replace(false)) {
                return Err("Interrupted recovery copy".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAIL_RECOVERY_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_INITIAL_JOURNAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Store {
    pub(super) fn replacement_roots(&self) -> Result<(), String> {
        for path in [
            self.root.clone(),
            self.plugin(),
            self.skills(),
            self.servers(),
            self.root.join("plugins"),
            self.disabled(),
            self.removed(),
            self.root.join("replacements"),
        ] {
            if exists(&path)? {
                plain(&path, true)?;
            }
        }
        Ok(())
    }
    pub(super) fn initial_ledger(&self) -> Result<BTreeMap<String, String>, String> {
        let Some(bytes) = checked_bytes(&self.ledger())? else { return Ok(BTreeMap::new()) };
        let ledger: BTreeMap<String, String> = serde_json::from_slice(&bytes).map_err(|_| "Could not read the catalog ledger")?;
        if ledger.iter().any(|(id, commit)| !valid_id(id) || commit.is_empty()) {
            return Err("Invalid catalog ledger".into());
        }
        Ok(ledger)
    }
    fn replacement_ledger(&self) -> Result<BTreeMap<String, String>, String> {
        checked_bytes(&self.ledger())?.ok_or("The tool is not installed")?;
        self.initial_ledger()
    }
    pub(super) fn physical_copy(&self, item: &Item) -> Result<bool, String> {
        Ok(exists(&self.dir(item))? || exists(&self.disabled().join(&item.id))?)
    }
    pub(super) fn occupied_install(&self, item: &Item) -> Result<bool, String> {
        Ok(self.physical_copy(item)? || exists(&self.removed().join(&item.id))?)
    }
    pub fn has_copy(&self, id: &str) -> Result<bool, String> {
        let _store = lock();
        self.replacement_roots()?;
        let item = self.items.iter().find(|item| item.id == id).ok_or("Unknown tool")?;
        self.physical_copy(item)
    }
    pub(super) fn direct_replacement(&self, item: &Item, zip: &Path, packages: &BTreeMap<String, PathBuf>) -> Result<bool, String> {
        let _store = lock();
        self.recover_replacements()?;
        if !self.physical_copy(item)? {
            return Ok(false);
        }
        let plan = self.replacement_snapshot(&item.id)?;
        if plan.item != *item {
            return Err("The replacement does not match the catalog".into());
        }
        let copy = if plan.disabled_location { self.disabled().join(&item.id) } else { self.dir(item) };
        // Trusted direct callers can repeat an already complete installation without replacing its files or receipt.
        if plan.old_commit == item.commit && self.verify_item(&copy, item) {
            self.refresh_ready_after_install();
            return Ok(true);
        }
        self.replace_sources_locked(&plan, zip, packages, || true)?;
        Ok(true)
    }
    fn replacement_snapshot(&self, id: &str) -> Result<ReplacementPlan, String> {
        self.replacement_roots()?;
        if !valid_id(id) {
            return Err("Unknown tool".into());
        }
        let item = self.items.iter().find(|item| item.id == id).ok_or("Unknown tool")?;
        if exists(&self.removed().join(id))? {
            return Err("A removed copy needs attention in Catalog".into());
        }
        let old_commit = self.replacement_ledger()?.get(id).cloned().ok_or("The tool is not installed")?;
        let selection = checked_bytes(&self.root.join("selection.json"))?;
        let switches = self.read_selection().ok_or("Could not read catalog switches")?;
        let active = exists(&self.dir(item))?;
        let disabled_location = exists(&self.disabled().join(id))?;
        if active == disabled_location {
            return Err("The stored copy needs attention in Catalog".into());
        }
        let enabled = active && !switches.disabled.contains(id);
        let copy = if disabled_location { self.disabled().join(id) } else { self.dir(item) };
        let old_digest = fingerprint(&copy)?;
        Ok(ReplacementPlan {
            item: item.clone(),
            plugin: self.plugin_definition(id).cloned(),
            old_commit,
            enabled,
            disabled_location,
            old_digest,
            selection,
            // A recovery receipt can hold both old and rejected copies, plus its atomic metadata.
            previous_receipt: if exists(&self.root.join("replacements").join(id))? {
                Some(fingerprint_bounded(&self.root.join("replacements").join(id), 2 * SERVER_LIMIT + 2 * 1024 * 1024, 40064, 65)?)
            } else {
                None
            },
        })
    }
    pub fn capture_replacement(&self, id: &str) -> Result<ReplacementPlan, String> {
        let _store = lock();
        self.recover_replacements()?;
        self.replacement_snapshot(id)
    }
    pub fn check_replacement(&self, plan: &ReplacementPlan) -> Result<(), String> {
        let _store = lock();
        self.recover_replacements()?;
        self.check_replacement_locked(plan)
    }
    fn check_replacement_locked(&self, plan: &ReplacementPlan) -> Result<(), String> {
        self.recover_replacements()?;
        if self.replacement_snapshot(&plan.item.id)? != *plan {
            return Err("The catalog copy changed; request confirmation again".into());
        }
        Ok(())
    }
    pub fn replace_sources(&self, plan: &ReplacementPlan, zip: &Path, packages: &BTreeMap<String, PathBuf>) -> Result<(), String> {
        self.replace_sources_if(plan, zip, packages, || true)
    }

    pub(crate) fn replace_sources_if(
        &self,
        plan: &ReplacementPlan,
        zip: &Path,
        packages: &BTreeMap<String, PathBuf>,
        authorized: impl FnOnce() -> bool,
    ) -> Result<(), String> {
        let _store = lock();
        self.replace_sources_locked(plan, zip, packages, authorized)
    }

    fn replace_sources_locked(
        &self,
        plan: &ReplacementPlan,
        zip: &Path,
        packages: &BTreeMap<String, PathBuf>,
        authorized: impl FnOnce() -> bool,
    ) -> Result<(), String> {
        self.recover_replacements()?;
        self.check_replacement_locked(plan)?;
        let temporary = kits::TemporaryCopy::new(&self.root, ".staging-").map_err(|e| e.to_string())?;
        let staging = &temporary.path;
        if let Some(plugin) = &plan.plugin {
            self.validate_plugin_aliases(plugin)?;
            plugins::extract(plugin, zip, packages, staging)?;
        } else if plan.item.server.is_some() {
            extract_package(zip, staging)?;
        } else {
            extract_skill(zip, &plan.item.path, staging)?;
            let text = std::fs::read_to_string(staging.join("SKILL.md")).map_err(|e| e.to_string())?;
            standalone_skill(&text)?;
        }
        if !self.verify_item(staging, &plan.item) {
            return Err("The replacement files do not match the catalog".into());
        }
        self.check_replacement_locked(plan)?;
        if !authorized() {
            return Err("The catalog request is no longer active".into());
        }
        let root = self.root.join("replacements");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let journal_dir = root.join(&plan.item.id);
        let pending = kits::TemporaryCopy::new(&self.root, ".staging-").map_err(|e| e.to_string())?;
        std::fs::create_dir(&pending.path).map_err(|e| e.to_string())?;
        #[cfg(test)]
        if FAIL_INITIAL_JOURNAL.with(|fail| fail.replace(false)) {
            return Err("Interrupted initial journal write".into());
        }
        let mut journal = Journal { plan: plan.clone(), finished: false, rolled_back: false };
        journal.write(&pending.path)?;
        if exists(&journal_dir)? {
            self.discard(&journal_dir).map_err(|e| e.to_string())?;
        }
        retry_busy(|| std::fs::rename(&pending.path, &journal_dir)).map_err(|e| e.to_string())?;
        let dest = if plan.disabled_location { self.disabled().join(&plan.item.id) } else { self.dir(&plan.item) };
        let published = (|| {
            retry_busy(|| std::fs::rename(&dest, journal_dir.join("old"))).map_err(|e| e.to_string())?;
            if fingerprint(&journal_dir.join("old"))? != plan.old_digest {
                return Err("The original copy changed during replacement".into());
            }
            retry_busy(|| std::fs::rename(staging, &dest)).map_err(|e| e.to_string())?;
            let mut ledger = self.replacement_ledger()?;
            ledger.insert(plan.item.id.clone(), plan.item.commit.clone());
            self.write_ledger(&ledger).map_err(|e| e.to_string())?;
            journal.finished = true;
            journal.write(&journal_dir)
        })();
        if let Err(error) = published {
            return match self.recover_replacements() {
                Ok(()) => Err(error),
                Err(recovery) => Err(format!("{error}; catalog recovery needs attention: {recovery}")),
            };
        }
        self.refresh_ready_after_install();
        Ok(())
    }
    /// Called under the store lock before any pruning or further mutation.
    pub(super) fn recover_replacements(&self) -> Result<(), String> {
        self.replacement_roots()?;
        let root = self.root.join("replacements");
        if !exists(&root)? {
            return Ok(());
        }
        for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
            let dir = entry.map_err(|e| e.to_string())?.path();
            plain(&dir, true)?;
            let bytes = checked_bytes(&dir.join("state.json"))?.ok_or("Missing replacement journal")?;
            let mut journal: Journal = serde_json::from_slice(&bytes).map_err(|_| "Invalid replacement journal")?;
            let plan = &journal.plan;
            if !valid_id(&plan.item.id)
                || dir.file_name().and_then(|name| name.to_str()) != Some(plan.item.id.as_str())
                || journal.finished && journal.rolled_back
                || plan.old_commit.is_empty()
                || plan.old_digest.len() != 64
                || !plan.old_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("Invalid replacement journal".into());
            }
            let old = dir.join("old");
            if journal.rolled_back {
                if exists(&old)? && fingerprint(&old)? != plan.old_digest {
                    return Err("The retained catalog copy needs attention".into());
                }
                continue;
            }
            if journal.finished {
                if fingerprint(&old)? != plan.old_digest {
                    return Err("The retained catalog copy needs attention".into());
                }
                continue;
            }
            if self.items.iter().find(|item| item.id == plan.item.id) != Some(&plan.item)
                || self.plugin_definition(&plan.item.id) != plan.plugin.as_ref()
            {
                return Err("The interrupted replacement needs attention in Catalog".into());
            }
            if exists(&self.removed().join(&plan.item.id))? || checked_bytes(&self.root.join("selection.json"))? != plan.selection {
                return Err("The interrupted catalog state changed".into());
            }
            let dest = if plan.disabled_location { self.disabled().join(&plan.item.id) } else { self.dir(&plan.item) };
            let mut ledger = self.replacement_ledger()?;
            if !ledger.get(&plan.item.id).is_some_and(|commit| commit == &plan.old_commit || commit == &plan.item.commit) {
                return Err("The interrupted catalog ledger changed".into());
            }
            if exists(&old)? {
                if fingerprint(&old)? != plan.old_digest {
                    return Err("The retained copy changed".into());
                }
                if exists(&dest)? && fingerprint(&dest).ok().as_deref() != Some(&plan.old_digest) {
                    plain(&dest, true)?;
                    if exists(&dir.join("rejected"))? {
                        return Err("Interrupted catalog recovery needs attention".into());
                    }
                    retry_busy(|| std::fs::rename(&dest, dir.join("rejected"))).map_err(|e| e.to_string())?;
                }
                if !exists(&dest)? {
                    let recovery = kits::TemporaryCopy::new(&self.root, ".staging-").map_err(|e| e.to_string())?;
                    let mut remaining = SERVER_LIMIT;
                    let mut paths = 20000;
                    copy_original(&old, &recovery.path, &mut remaining, &mut paths, 0)?;
                    if fingerprint(&recovery.path)? != plan.old_digest {
                        return Err("The retained copy changed during recovery".into());
                    }
                    retry_busy(|| std::fs::rename(&recovery.path, &dest)).map_err(|e| e.to_string())?;
                }
                if fingerprint(&dest)? != plan.old_digest {
                    return Err("Could not restore the retained catalog copy".into());
                }
            } else if fingerprint(&dest)? != plan.old_digest {
                return Err("The original catalog copy is missing".into());
            }
            ledger.insert(plan.item.id.clone(), plan.old_commit.clone());
            self.write_ledger(&ledger).map_err(|e| e.to_string())?;
            journal.rolled_back = true;
            journal.write(&dir)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn replacement_detects_changed_copy_before_consent_is_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let item = catalog()[0].clone();
        let store = Store::with_catalog(tmp.path(), vec![item.clone()]);
        let dir = store.dir(&item);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), b"old modified copy").unwrap();
        store.write_ledger(&BTreeMap::from([(item.id.clone(), "old".into())])).unwrap();
        let plan = store.capture_replacement(&item.id).unwrap();
        std::fs::write(dir.join("SKILL.md"), b"changed after consent").unwrap();
        assert!(store.check_replacement(&plan).is_err());
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::super::*;
    use super::Journal;

    fn setup() -> (tempfile::TempDir, Store, Item, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let mut item = catalog()[0].clone();
        item.server = None;
        item.path = "skill".into();
        let source = tmp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let text = format!("---\nname: {}\n---\nNew curated instructions.", item.cli_name.as_deref().unwrap_or(&item.id));
        std::fs::write(source.join("SKILL.md"), &text).unwrap();
        item.digest = digest(&source).unwrap();
        let archive = tmp.path().join("source.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        zip.start_file("repo/skill/SKILL.md", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
        zip.finish().unwrap();
        let store = Store::with_catalog(&tmp.path().join("data"), vec![item.clone()]);
        let dir = store.dir(&item);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), b"old user modified instructions").unwrap();
        store.write_ledger(&BTreeMap::from([(item.id.clone(), "older-pin".into())])).unwrap();
        (tmp, store, item, archive)
    }

    #[test]
    fn an_agent_install_never_overwrites_files_missing_from_the_ledger() {
        let (_tmp, store, item, archive) = setup();
        std::fs::remove_file(store.ledger()).unwrap();
        let original = store.dir(&item).join("SKILL.md");
        let before = std::fs::read(&original).unwrap();
        assert!(store.install_sources_as(&item, &archive, &BTreeMap::new(), true).is_err(), "unrecorded files need explicit recovery");
        assert_eq!(std::fs::read(original).unwrap(), before);
    }

    #[test]
    fn manual_reinstall_failure_preserves_disabled_original_files() {
        let (_tmp, store, item, archive) = setup();
        store.set_enabled(&item.id, false).unwrap();
        store.wait_prepared_for_test();
        let original = store.disabled().join(&item.id).join("SKILL.md");
        let old_bytes = std::fs::read(&original).unwrap();
        let old_selection = std::fs::read(store.root.join("selection.json")).unwrap();
        std::fs::remove_file(store.ledger()).unwrap();
        std::fs::create_dir(store.ledger()).unwrap();
        assert!(store.install_sources(&item, &archive, &BTreeMap::new()).is_err());
        assert_eq!(std::fs::read(&original).ok(), Some(old_bytes), "a failed manual reinstall must retain the disabled original");
        assert_eq!(std::fs::read(store.root.join("selection.json")).unwrap(), old_selection);
        assert!(!store.dir(&item).exists(), "a failed reinstall must not enable its replacement");
    }

    #[test]
    fn initial_install_preserves_missing_entries_and_the_off_switch() {
        let (_tmp, store, item, archive) = setup();
        store.set_enabled(&item.id, false).unwrap();
        store.wait_prepared_for_test();
        std::fs::remove_dir_all(store.disabled().join(&item.id)).unwrap();
        let unrelated = "another-missing-tool".to_string();
        store.write_ledger(&BTreeMap::from([(item.id.clone(), "older-pin".into()), (unrelated.clone(), "other-pin".into())])).unwrap();
        let switches = std::fs::read(store.root.join("selection.json")).unwrap();
        store.install_sources(&item, &archive, &BTreeMap::new()).unwrap();
        assert_eq!(store.initial_ledger().unwrap().get(&unrelated).map(String::as_str), Some("other-pin"));
        assert_eq!(std::fs::read(store.root.join("selection.json")).unwrap(), switches);
        assert!(store.disabled().join(&item.id).join("SKILL.md").is_file());
        assert!(!store.dir(&item).exists());
    }

    #[test]
    fn initial_install_refuses_a_corrupt_ledger_without_publishing_files() {
        let (_tmp, store, item, archive) = setup();
        std::fs::remove_dir_all(store.dir(&item)).unwrap();
        std::fs::write(store.ledger(), b"not a ledger").unwrap();
        assert!(store.install_sources(&item, &archive, &BTreeMap::new()).is_err());
        assert_eq!(std::fs::read(store.ledger()).unwrap(), b"not a ledger");
        assert!(!store.dir(&item).exists());
    }

    #[test]
    fn replacement_keeps_old_bytes_and_disabled_location() {
        let (_tmp, store, item, archive) = setup();
        let active = store.dir(&item);
        let disabled = store.disabled().join(&item.id);
        std::fs::create_dir_all(store.disabled()).unwrap();
        std::fs::rename(&active, &disabled).unwrap();
        let plan = store.capture_replacement(&item.id).unwrap();
        assert!(!plan.enabled());
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        assert!(!active.exists());
        assert!(store.verify_item(&disabled, &item));
        assert_eq!(store.installed()[&item.id], item.commit);
        assert!(!store.enabled(&item.id));
        let backup = store.root.join("replacements").join(&item.id).join("old");
        assert_eq!(std::fs::read(backup.join("SKILL.md")).unwrap(), b"old user modified instructions");
        store.recover_replacements().unwrap();
        assert!(backup.exists());
    }

    #[test]
    fn replacement_refuses_corrupt_metadata_and_source_without_removing_old_copy() {
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        std::fs::write(&archive, b"not an archive").unwrap();
        assert!(store.replace_sources(&plan, &archive, &BTreeMap::new()).is_err());
        assert_eq!(std::fs::read(store.dir(&item).join("SKILL.md")).unwrap(), b"old user modified instructions");
        std::fs::write(store.ledger(), b"{").unwrap();
        assert!(store.capture_replacement(&item.id).is_err());
    }

    #[test]
    fn replacement_recovers_each_interrupted_publication_with_original_backup_intact() {
        for phase in 0..=3 {
            let (_tmp, store, item, _) = setup();
            let plan = store.capture_replacement(&item.id).unwrap();
            let journal_dir = store.root.join("replacements").join(&item.id);
            std::fs::create_dir_all(&journal_dir).unwrap();
            Journal { plan, finished: false, rolled_back: false }.write(&journal_dir).unwrap();
            let dest = store.dir(&item);
            if phase > 0 {
                std::fs::rename(&dest, journal_dir.join("old")).unwrap();
            }
            if phase > 1 {
                std::fs::create_dir_all(&dest).unwrap();
                std::fs::write(dest.join("SKILL.md"), b"partially published replacement").unwrap();
            }
            if phase > 2 {
                store.write_ledger(&BTreeMap::from([(item.id.clone(), item.commit.clone())])).unwrap();
            }
            store.recover_replacements().unwrap();
            assert_eq!(std::fs::read(dest.join("SKILL.md")).unwrap(), b"old user modified instructions", "phase {phase}");
            assert_eq!(store.installed()[&item.id], "older-pin");
            if phase > 0 {
                assert_eq!(std::fs::read(journal_dir.join("old/SKILL.md")).unwrap(), b"old user modified instructions");
            }
            store.recover_replacements().unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn replacement_rolls_back_immediately_when_ledger_rename_is_denied() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        let held = std::fs::OpenOptions::new().read(true).share_mode(3).open(store.ledger()).unwrap();
        assert!(store.replace_sources(&plan, &archive, &BTreeMap::new()).is_err());
        assert_eq!(std::fs::read(store.dir(&item).join("SKILL.md")).unwrap(), b"old user modified instructions");
        assert_eq!(store.installed()[&item.id], "older-pin");
        assert_eq!(
            std::fs::read(store.root.join("replacements").join(&item.id).join("old/SKILL.md")).unwrap(),
            b"old user modified instructions"
        );
        drop(held);
        store.recover_replacements().unwrap();
    }

    #[test]
    fn replacement_cannot_discard_a_changed_backup_or_corrupt_receipt() {
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        store.wait_prepared_for_test();
        let next = store.capture_replacement(&item.id).unwrap();
        let receipt = store.root.join("replacements").join(&item.id);
        std::fs::write(receipt.join("old/SKILL.md"), b"changed retained copy").unwrap();
        assert!(store.replace_sources(&next, &archive, &BTreeMap::new()).is_err());
        assert_eq!(std::fs::read(receipt.join("old/SKILL.md")).unwrap(), b"changed retained copy");
        std::fs::write(receipt.join("state.json"), b"{").unwrap();
        assert!(store.capture_replacement(&item.id).is_err());
        assert!(store.verify_item(&store.dir(&item), &item));
    }

    #[test]
    fn replacement_corrupt_journal_revokes_a_previously_ready_catalog() {
        use crate::model::{Area, Permissions, Vendor};

        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        store.prepare_kit_for(Vendor::Claude, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        assert!(store.ready_kit_for(Vendor::Claude, "debug", None, Area::Project, Permissions::FullAccess, &[]).0.catalog_ready);
        std::fs::write(store.root.join("replacements").join(&item.id).join("state.json"), b"{").unwrap();
        store.prepare_kit_for(Vendor::Claude, "debug", None, Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        assert!(!store.ready_kit_for(Vendor::Claude, "debug", None, Area::Project, Permissions::FullAccess, &[]).0.catalog_ready);
    }

    #[test]
    fn replacement_uses_checked_package_extraction_for_servers() {
        let tmp = tempfile::tempdir().unwrap();
        let mut item = catalog().iter().find(|item| item.server.is_some()).unwrap().clone();
        item.server.as_mut().unwrap().entry = "run.js".into();
        let source = tmp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("run.js"), b"console.log('new');").unwrap();
        item.digest = digest(&source).unwrap();
        let archive = tmp.path().join("package.tgz");
        let encoder = flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        tar.append_path_with_name(source.join("run.js"), "package/run.js").unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let store = Store::with_catalog(&tmp.path().join("data"), vec![item.clone()]).with_node("C:/node.exe".into());
        let dest = store.dir(&item);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("run.js"), b"old server").unwrap();
        store.write_ledger(&BTreeMap::from([(item.id.clone(), "older-pin".into())])).unwrap();
        let plan = store.capture_replacement(&item.id).unwrap();
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        assert!(store.verify_item(&dest, &item));
        assert_eq!(std::fs::read(store.root.join("replacements").join(&item.id).join("old/run.js")).unwrap(), b"old server");
    }

    #[test]
    fn replacement_uses_checked_composite_extraction_for_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let mut item = catalog()[0].clone();
        item.cli_name = None;
        item.chat_safe = false;
        item.commit = "a".repeat(40);
        item.path = "plugin".into();
        let source = tmp.path().join("source");
        std::fs::create_dir_all(source.join("commands")).unwrap();
        let text = b"Explain the selected code clearly.";
        std::fs::write(source.join("commands/help.md"), text).unwrap();
        item.digest = digest(&source).unwrap();
        let definition = plugins::Definition {
            item: item.clone(),
            parts: vec![plugins::Part {
                id: "help".into(),
                kind: plugins::PartKind::Command,
                cli_name: None,
                path: "commands/help.md".into(),
                digest: crate::hex(&Sha256::digest(text)),
                server: None,
            }],
        };
        let archive = tmp.path().join("plugin.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        zip.start_file("repo/plugin/commands/help.md", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text).unwrap();
        zip.finish().unwrap();
        let store = Store::with_plugins(&tmp.path().join("data"), vec![item.clone()], vec![definition]);
        let dest = store.dir(&item);
        std::fs::create_dir_all(dest.join("source")).unwrap();
        std::fs::write(dest.join("source/old.md"), b"old plugin").unwrap();
        store.write_ledger(&BTreeMap::from([(item.id.clone(), "older-pin".into())])).unwrap();
        let plan = store.capture_replacement(&item.id).unwrap();
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        assert!(store.verify_item(&dest, &item));
        assert_eq!(std::fs::read(store.root.join("replacements").join(&item.id).join("old/source/old.md")).unwrap(), b"old plugin");
    }

    #[test]
    fn replacement_crash_recovery_copy_never_publishes_partial_original() {
        let (_tmp, store, item, _) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        let receipt = store.root.join("replacements").join(&item.id);
        std::fs::create_dir_all(&receipt).unwrap();
        Journal { plan, finished: false, rolled_back: false }.write(&receipt).unwrap();
        let dest = store.dir(&item);
        std::fs::rename(&dest, receipt.join("old")).unwrap();
        std::fs::create_dir(receipt.join("rejected")).unwrap();
        std::fs::write(receipt.join("rejected/new.md"), b"retained new evidence").unwrap();
        super::FAIL_RECOVERY_COPY.with(|fail| fail.set(true));
        assert!(store.recover_replacements().is_err());
        assert!(!dest.exists(), "an interrupted recovery must leave the original outside launch roots");
        store.recover_replacements().unwrap();
        assert_eq!(std::fs::read(dest.join("SKILL.md")).unwrap(), b"old user modified instructions");
        assert_eq!(std::fs::read(receipt.join("rejected/new.md")).unwrap(), b"retained new evidence");
        assert!(receipt.join("old/SKILL.md").exists());
    }

    #[test]
    fn replacement_crash_before_first_journal_write_leaves_live_copy_recoverable() {
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        super::FAIL_INITIAL_JOURNAL.with(|fail| fail.set(true));
        assert!(store.replace_sources(&plan, &archive, &BTreeMap::new()).is_err());
        store.recover_replacements().unwrap();
        assert!(store.capture_replacement(&item.id).is_ok());
        assert_eq!(std::fs::read(store.dir(&item).join("SKILL.md")).unwrap(), b"old user modified instructions");
    }

    #[test]
    fn replacement_rollback_preserves_original_file_permissions() {
        let (_tmp, store, item, _) = setup();
        let dest = store.dir(&item);
        let original = dest.join("SKILL.md");
        let mut permissions = std::fs::metadata(&original).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&original, permissions.clone()).unwrap();
        let plan = store.capture_replacement(&item.id).unwrap();
        let receipt = store.root.join("replacements").join(&item.id);
        std::fs::create_dir_all(&receipt).unwrap();
        Journal { plan, finished: false, rolled_back: false }.write(&receipt).unwrap();
        std::fs::rename(&dest, receipt.join("old")).unwrap();
        store.recover_replacements().unwrap();
        assert_eq!(std::fs::metadata(&original).unwrap().permissions(), permissions);
    }

    #[test]
    fn replacement_revoked_at_final_publication_admission_keeps_original_and_ledger() {
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        let ledger = std::fs::read(store.ledger()).unwrap();
        let checked = std::cell::Cell::new(false);
        assert!(
            store
                .replace_sources_if(&plan, &archive, &BTreeMap::new(), || {
                    checked.set(true);
                    false
                })
                .is_err()
        );
        assert!(checked.get(), "the callback runs after staging and final copy verification");
        assert_eq!(std::fs::read(store.dir(&item).join("SKILL.md")).unwrap(), b"old user modified instructions");
        assert_eq!(std::fs::read(store.ledger()).unwrap(), ledger);
        assert!(!store.root.join("replacements").exists(), "revocation publishes no backup or journal");
    }

    #[test]
    fn replacement_preserves_quarantined_files_changed_after_the_next_consent() {
        let (_tmp, store, item, archive) = setup();
        let plan = store.capture_replacement(&item.id).unwrap();
        store.replace_sources(&plan, &archive, &BTreeMap::new()).unwrap();
        store.wait_prepared_for_test();
        let receipt = store.root.join("replacements").join(&item.id);
        let mut journal: Journal = serde_json::from_slice(&std::fs::read(receipt.join("state.json")).unwrap()).unwrap();
        journal.finished = false;
        journal.write(&receipt).unwrap();
        store.recover_replacements().unwrap();
        let next = store.capture_replacement(&item.id).unwrap();
        let retained = receipt.join("rejected/SKILL.md");
        std::fs::write(&retained, b"changed quarantined files after consent").unwrap();
        assert!(
            store.replace_sources(&next, &archive, &BTreeMap::new()).is_err(),
            "the complete retained receipt must still match consent"
        );
        assert_eq!(std::fs::read(retained).unwrap(), b"changed quarantined files after consent");
    }
}
