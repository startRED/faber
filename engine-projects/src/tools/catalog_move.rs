//! Move one installed catalog payload at a time. The location record is the commit point.
use super::{Store, lock, replacement, small_file, valid_id};
use crate::project::lease::real_path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const UNREACHABLE: &str = "error.storage.unreachable";
const ITEM_FAILED: &str = "error.storage.moveItem";
const PAYLOADS: [&str; 5] = ["disabled", "plugin/skills", "plugins", "runtimes", "servers"];
const CHUNK: usize = 64 * 1024;

pub enum MoveEvent<'a> {
    Item { id: &'a str, number: usize, total: usize },
    Size(u64),
    Copied(u64),
    Moved,
    Failed { id: &'a str, error: &'static str },
}

/// What the caller wants next, asked before each item and again before an item is published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveControl {
    Go,
    /// Finish the item being moved, then stop.
    Stop,
    /// The destination is no longer the chosen one: discard the item being moved and stop.
    Abandon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveEnd {
    Finished,
    Stopped,
    Abandoned,
}

enum Outcome {
    Moved,
    Skipped,
    Abandoned,
}

/// What one item's move reads under the store lock, and checks again under it before publishing: the copy and its checks
/// run without the lock, so a change in between (an update, a removal, a switch) leaves the item where it was.
#[derive(PartialEq, Eq)]
struct Plan {
    commit: String,
    folders: Vec<PathBuf>,
    parts: Vec<PathBuf>,
    signature: [u8; 32],
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    target: PathBuf,
    commit: String,
    folders: Vec<PathBuf>,
    proofs: BTreeMap<PathBuf, [u8; 32]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Copied,
    Verified,
    Published,
    Switched,
}

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

fn checked(path: &Path) -> io::Result<()> {
    real_path(path).map(|_| ()).map_err(|_| invalid())
}

fn present(path: &Path) -> io::Result<bool> {
    checked(path)?;
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_owned(root: &faber_winproc::rewind::Root, relative: &Path, directory: bool) -> io::Result<()> {
    match root.remove_storage_path(relative, directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

impl Store {
    pub fn movable_catalog_ids(&self) -> Result<Vec<String>, &'static str> {
        self.initial_ledger()
            .map(|ledger| ledger.into_keys().filter(|id| valid_id(id) && self.source_root(id) == self.root).collect())
            .map_err(|_| "error.storage.move")
    }

    fn receipt_path(&self, id: &str) -> PathBuf {
        self.root.join("storage-moves").join(format!("{id}.json"))
    }

    fn moved_receipt(&self, id: &str) -> Option<Receipt> {
        if !valid_id(id) {
            return None;
        }
        let path = self.receipt_path(id);
        checked(&path).ok()?;
        let receipt: Receipt = serde_json::from_slice(&small_file(&path)?).ok()?;
        let root = self.recorded_root(id).ok().flatten()?;
        let payload = |path: &PathBuf, parents: &[&str]| parents.iter().any(|parent| *path == Path::new(parent).join(id));
        let pin = |path: &PathBuf| {
            path.parent() == Some(Path::new("runtime-pins"))
                && path.file_name().and_then(|name| name.to_str()).is_some_and(super::runtime::pin)
        };
        (root == receipt.target
            && root != self.root
            && !receipt.commit.is_empty()
            && root.file_name().is_some_and(|name| name == "tools")
            && root.parent().is_some_and(crate::storage::external_root_name)
            && receipt.folders.first().is_some_and(|path| payload(path, &PAYLOADS))
            && (receipt.folders.len() == 1 || (receipt.folders.len() == 2 && payload(&receipt.folders[1], &["replacements"])))
            && receipt.folders.iter().all(|path| receipt.proofs.contains_key(path))
            && receipt.proofs.keys().all(|path| receipt.folders.contains(path) || pin(path)))
        .then_some(receipt)
    }

    pub(super) fn retains_old_move(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|id| id.to_str())
            .and_then(|id| self.moved_receipt(id))
            .is_some_and(|receipt| receipt.folders.iter().any(|relative| self.root.join(relative) == path))
    }

    pub(super) fn has_old_move(&self, id: &str) -> bool {
        self.moved_receipt(id).is_some()
    }

    pub fn has_old_catalog_copies(&self) -> bool {
        !self.old_move_ids().is_empty()
    }

    fn old_move_ids(&self) -> Vec<String> {
        let dir = self.root.join("storage-moves");
        if checked(&dir).is_err() {
            return vec![];
        }
        fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let id = path.file_stem()?.to_str()?;
                (path.extension()? == "json" && self.moved_receipt(id).is_some()).then(|| id.to_owned())
            })
            .collect()
    }

    /// Removes the retained originals of moved items, except those `in_use` says a live session may still read: a runtime
    /// is handed to sessions by its path in the store, not copied into their kit. Returns the ids kept for that reason.
    pub fn remove_old_catalog_copies(
        &self,
        authorized: impl Fn() -> bool,
        in_use: impl Fn(&str) -> bool,
    ) -> Result<Vec<String>, &'static str> {
        let mut failed = false;
        let mut kept = Vec::new();
        for id in self.old_move_ids() {
            let _store = lock();
            if !authorized() {
                return Err("error.storage.removeOld");
            }
            if in_use(&id) {
                kept.push(id);
                continue;
            }
            failed |= self.remove_old_move(&id).is_err();
        }
        if failed { Err("error.storage.removeOld") } else { Ok(kept) }
    }

    fn remove_old_move(&self, id: &str) -> io::Result<()> {
        let receipt = self.moved_receipt(id).ok_or_else(invalid)?;
        let ledger = self.replacement_ledger().map_err(io::Error::other)?;
        if let Some(commit) = ledger.get(id) {
            if !self.move_target_reachable(&receipt.target) {
                return Err(io::ErrorKind::NotFound.into());
            }
            let item = self.items.iter().find(|item| item.id == id).ok_or_else(invalid)?;
            let disabled = self.disabled_copy(id);
            let live = if present(&disabled)? { disabled } else { self.dir(item) };
            checked(&live)?;
            let mut buffer = vec![0; CHUNK];
            let unchanged = |path: &Path, expected: &[u8; 32], buffer: &mut [u8]| -> io::Result<()> {
                checked(path)?;
                if verify(path, None, buffer)? != *expected {
                    return Err(invalid());
                }
                Ok(())
            };
            let live_verified = self.verify_item(&live, item);
            if !live_verified {
                unchanged(&live, &receipt.proofs[&receipt.folders[0]], &mut buffer)?;
            }
            let has_replacement = receipt.folders.len() > 1;
            let mut superseded = false;
            if let Some(relative) = receipt.folders.get(1) {
                let recovery = receipt.target.join(relative);
                if let Err(error) = unchanged(&recovery, &receipt.proofs[relative], &mut buffer) {
                    checked(&recovery)?;
                    if !live_verified || *commit == receipt.commit || !self.verified_replacement(&recovery, item, commit) {
                        return Err(error);
                    }
                    if item.runtime.is_some() {
                        pins(&receipt.target, &recovery.join("old"))?;
                    }
                    superseded = true;
                }
            }
            for (relative, digest) in &receipt.proofs {
                if relative.parent() == Some(Path::new("runtime-pins")) && (!live_verified || (has_replacement && !superseded)) {
                    unchanged(&receipt.target.join(relative), digest, &mut buffer)?;
                }
            }
        }
        // The platform's recursive removal unlinks descendants without following links.
        for relative in &receipt.folders {
            let path = self.root.join(relative);
            if present(&path)? {
                replacement::plain(&path, true).map_err(io::Error::other)?;
            }
        }
        let root = faber_winproc::rewind::Root::open_no_links(&self.root)?;
        for relative in receipt.folders.iter().rev() {
            remove_owned(&root, relative, true)?;
        }
        remove_owned(&root, &Path::new("storage-moves").join(format!("{id}.json")), false)
    }

    pub fn move_target_reachable(&self, target: &Path) -> bool {
        super::location::external(&self.root, target)
    }

    pub fn move_catalog(
        &self,
        target: &Path,
        control: impl Fn() -> MoveControl,
        event: impl FnMut(MoveEvent<'_>),
    ) -> Result<MoveEnd, &'static str> {
        self.move_catalog_with(target, control, event, |_, _| Ok(()))
    }

    fn move_catalog_with(
        &self,
        target: &Path,
        control: impl Fn() -> MoveControl,
        mut event: impl FnMut(MoveEvent<'_>),
        mut checkpoint: impl FnMut(&str, Stage) -> io::Result<()>,
    ) -> Result<MoveEnd, &'static str> {
        if !self.move_target_reachable(target) {
            return Err(UNREACHABLE);
        }
        let ids = {
            let _store = lock();
            self.movable_catalog_ids()?
        };
        for (index, id) in ids.iter().enumerate() {
            match control() {
                MoveControl::Go => {}
                MoveControl::Stop => return Ok(MoveEnd::Stopped),
                MoveControl::Abandon => return Ok(MoveEnd::Abandoned),
            }
            if !self.move_target_reachable(target) {
                return Err(UNREACHABLE);
            }
            event(MoveEvent::Item { id, number: index + 1, total: ids.len() });
            match self.move_one(target, id, &control, &mut event, |stage| checkpoint(id, stage)) {
                Ok(Outcome::Moved) => event(MoveEvent::Moved),
                Ok(Outcome::Skipped) => {}
                Ok(Outcome::Abandoned) => return Ok(MoveEnd::Abandoned),
                Err(_) if !self.move_target_reachable(target) => return Err(UNREACHABLE),
                Err(_) => event(MoveEvent::Failed { id, error: ITEM_FAILED }),
            }
        }
        Ok(MoveEnd::Finished)
    }

    /// Reads what moving `id` takes, under the store lock: its commit, its folders and the runtime pins it needs, with a
    /// signature of their metadata. None when it is not installed in the main data folder.
    fn plan_move(&self, id: &str) -> io::Result<Option<Plan>> {
        if self.source_root(id) != self.root {
            return Ok(None);
        }
        checked(&self.root.join("replacements"))?;
        self.recover_replacements_in(&self.root, Some(id)).map_err(io::Error::other)?;
        let Some(commit) = self.initial_ledger().map_err(io::Error::other)?.remove(id) else {
            return Ok(None);
        };
        let item = self.items.iter().find(|item| item.id == id).ok_or_else(invalid)?;
        let disabled = self.disabled_copy(id);
        let source = if present(&disabled)? { disabled } else { self.dir(item) };
        checked(&source)?;
        replacement::plain(&source, true).map_err(io::Error::other)?;
        let relative = |path: &Path| path.strip_prefix(&self.root).map(Path::to_path_buf).map_err(io::Error::other);
        let mut folders = vec![relative(&source)?];
        let replacement = self.root.join("replacements").join(id);
        if present(&replacement)? {
            folders.push(relative(&replacement)?);
        }
        let mut parts = Vec::new();
        if let Some(runtime) = &item.runtime {
            parts.extend(pins(&self.root, &source)?);
            if runtime.kind == super::runtime::Kind::Python && parts.is_empty() {
                return Err(invalid());
            }
            if folders.len() > 1 {
                for name in ["old", "rejected"] {
                    let dir = replacement.join(name);
                    if present(&dir)? {
                        parts.extend(pins(&self.root, &dir)?);
                    }
                }
            }
        }
        parts.sort_unstable();
        parts.dedup();
        parts.extend(folders.iter().cloned());
        let mut digest = Sha256::new();
        for part in &parts {
            digest.update(part.as_os_str().as_encoded_bytes());
            signature(&self.root.join(part), &mut digest)?;
        }
        Ok(Some(Plan { commit, folders, parts, signature: digest.finalize().into() }))
    }

    /// Moves one item. Only reading the plan and publishing hold the store lock; staging, verifying and flushing a payload
    /// of several gigabytes run without it, so the catalog, installs and kits are never held up behind a copy.
    fn move_one(
        &self,
        target: &Path,
        id: &str,
        control: &impl Fn() -> MoveControl,
        event: &mut impl FnMut(MoveEvent<'_>),
        mut checkpoint: impl FnMut(Stage) -> io::Result<()>,
    ) -> io::Result<Outcome> {
        let Some(plan) = ({
            let _store = lock();
            self.plan_move(id)?
        }) else {
            return Ok(Outcome::Skipped);
        };
        let parts = &plan.parts;
        let staging_relative = Path::new(".storage-move").join(id);
        let staging = target.join(&staging_relative);
        // Only a retry for this item clears debris from an interrupted run on this drive.
        checked(target)?;
        fs::create_dir_all(target)?;
        let target_handle = faber_winproc::rewind::Root::open_no_links(target)?;
        remove_owned(&target_handle, &staging_relative, true)?;
        fs::create_dir_all(&staging)?;
        let result: io::Result<Outcome> = (|| {
            let mut bytes = 0;
            for part in parts {
                bytes += size(&self.root.join(part))?;
            }
            event(MoveEvent::Size(bytes));
            let mut copied = 0;
            let mut buffer = vec![0; CHUNK];
            for part in parts {
                let from = self.root.join(part);
                let to = staging.join(part);
                fs::create_dir_all(to.parent().ok_or_else(invalid)?)?;
                copy(&from, &from, &to, &mut buffer, &mut |count| {
                    copied += count;
                    event(MoveEvent::Copied(copied));
                })?;
            }
            checkpoint(Stage::Copied)?;
            if !self.move_target_reachable(target) {
                return Err(io::ErrorKind::NotFound.into());
            }
            let mut proofs = BTreeMap::new();
            for part in parts {
                let proof = verify(&self.root.join(part), Some(&staging.join(part)), &mut buffer)?;
                proofs.insert(part.clone(), proof);
            }
            checkpoint(Stage::Verified)?;
            faber_winproc::sync_storage_item(&staging, parts)?;
            // Asked before the store lock: the caller's answer may take its own locks, never under this one.
            if control() == MoveControl::Abandon {
                return Ok(Outcome::Abandoned);
            }
            let _store = lock();
            match self.plan_move(id) {
                Ok(Some(now)) if now == plan => {}
                // Removed while it was being copied: nothing is left to move.
                Ok(None) => return Ok(Outcome::Skipped),
                // Updated, switched or damaged while it was being copied: the copy is not what the store holds now.
                _ => return Err(invalid()),
            }
            if !self.move_target_reachable(target) {
                return Err(io::ErrorKind::NotFound.into());
            }
            self.publish_move(target, &target_handle, &staging, id, &plan, proofs, &mut buffer, &mut checkpoint)?;
            self.refresh_ready_after_install();
            Ok(Outcome::Moved)
        })();
        if self.move_target_reachable(target) {
            let _ = remove_owned(&target_handle, &staging_relative, true);
        }
        let outcome = match result {
            Err(error) => {
                let _store = lock();
                // A copy that failed because the item was removed meanwhile is not an error of the move.
                if self.initial_ledger().is_ok_and(|ledger| !ledger.contains_key(id)) {
                    return Ok(Outcome::Skipped);
                }
                return Err(error);
            }
            Ok(outcome) => outcome,
        };
        if matches!(outcome, Outcome::Moved) {
            checkpoint(Stage::Switched)?;
        }
        Ok(outcome)
    }

    /// Under the store lock: rename the verified parts into place, write the receipt that keeps the originals for the
    /// person to remove, then switch the location record. A failure before the switch takes the published parts back.
    #[allow(clippy::too_many_arguments)]
    fn publish_move(
        &self,
        target: &Path,
        target_handle: &faber_winproc::rewind::Root,
        staging: &Path,
        id: &str,
        plan: &Plan,
        proofs: BTreeMap<PathBuf, [u8; 32]>,
        buffer: &mut [u8],
        checkpoint: &mut impl FnMut(Stage) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut published = Vec::new();
        let result: io::Result<()> = (|| {
            for part in &plan.parts {
                let dest = target.join(part);
                if present(&dest)? {
                    // A previous interruption or an identical registry pin may have left this path.
                    if verify(&dest, None, buffer)? != proofs[part] {
                        return Err(invalid());
                    }
                    continue;
                }
                let parent = dest.parent().ok_or_else(invalid)?;
                fs::create_dir_all(parent)?;
                faber_winproc::publish_storage_path(&staging.join(part), &dest)?;
                published.push(part);
            }
            faber_winproc::sync_storage_publication(target, &plan.parts)?;
            checkpoint(Stage::Published)?;
            let path = self.receipt_path(id);
            checked(&path)?;
            let parent = path.parent().ok_or_else(invalid)?;
            fs::create_dir_all(parent)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            let receipt = Receipt { target: target.to_path_buf(), commit: plan.commit.clone(), folders: plan.folders.clone(), proofs };
            serde_json::to_writer(&mut temp, &receipt)?;
            temp.as_file().sync_all()?;
            faber_winproc::publish_storage_record(temp.path(), &path)?;
            let relative = path.strip_prefix(&self.root).map_err(io::Error::other)?;
            faber_winproc::sync_storage_publication(&self.root, &[relative.to_path_buf()])?;
            self.record_location(id, target)
        })();
        if result.is_err() && self.move_target_reachable(target) {
            for part in published.into_iter().rev() {
                let _ = remove_owned(target_handle, part, part.parent() != Some(Path::new("runtime-pins")));
            }
        }
        result
    }
}

/// Names, kinds, sizes and modification times under `path`, never file contents: cheap enough to take again under the
/// store lock, and any install, update or removal of the item changes it.
fn signature(path: &Path, digest: &mut Sha256) -> io::Result<()> {
    let meta = metadata(path)?;
    let modified = meta.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).unwrap_or_default();
    digest.update([u8::from(meta.is_dir()), u8::from(meta.is_symlink())]);
    digest.update(meta.len().to_le_bytes());
    digest.update(modified.as_nanos().to_le_bytes());
    if meta.is_dir() {
        let mut entries = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_unstable_by_key(fs::DirEntry::file_name);
        digest.update((entries.len() as u64).to_le_bytes());
        for entry in entries {
            let name = entry.file_name();
            digest.update((name.len() as u64).to_le_bytes());
            digest.update(name.as_encoded_bytes());
            signature(&entry.path(), digest)?;
        }
    }
    Ok(())
}

fn pins(store: &Path, dir: &Path) -> io::Result<Vec<PathBuf>> {
    let reference = dir.join("interpreter.json");
    if !present(&reference)? {
        return Ok(vec![]);
    }
    let mut digest = String::new();
    faber_winproc::open_catalog_file(dir, Path::new("interpreter.json"))?.take(65).read_to_string(&mut digest)?;
    if !super::runtime::pin(&digest) {
        return Err(invalid());
    }
    let path = Path::new("runtime-pins").join(&digest);
    let file = store.join(&path);
    checked(&file)?;
    replacement::plain(&file, false).map_err(io::Error::other)?;
    let (hash, _) = hash(&file, &mut vec![0; CHUNK])?;
    if crate::hex(&hash) != digest {
        return Err(invalid());
    }
    Ok(vec![path])
}

fn metadata(path: &Path) -> io::Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
    }
    if meta.is_file() || meta.is_dir() || meta.is_symlink() { Ok(meta) } else { Err(invalid()) }
}

fn size(path: &Path) -> io::Result<u64> {
    let meta = metadata(path)?;
    if meta.is_dir() {
        fs::read_dir(path)?.try_fold(0, |sum, entry| Ok(sum + size(&entry?.path())?))
    } else {
        Ok(if meta.is_file() { meta.len() } else { 0 })
    }
}

fn copy(root: &Path, from: &Path, to: &Path, buffer: &mut [u8], progress: &mut impl FnMut(u64)) -> io::Result<()> {
    let meta = metadata(from)?;
    if meta.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy(root, &entry.path(), &to.join(entry.file_name()), buffer, progress)?;
        }
    } else if meta.is_symlink() {
        let link = replacement::runtime_link(root, from).map_err(io::Error::other)?.ok_or_else(invalid)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, to)?;
        #[cfg(not(unix))]
        return Err(io::Error::new(io::ErrorKind::Unsupported, link.display().to_string()));
    } else {
        let mut input =
            faber_winproc::open_catalog_file(from.parent().ok_or_else(invalid)?, Path::new(from.file_name().ok_or_else(invalid)?))?;
        let mut output = File::create_new(to)?;
        loop {
            let count = input.read(buffer)?;
            if count == 0 {
                break;
            }
            output.write_all(&buffer[..count])?;
            progress(count as u64);
        }
        #[cfg(unix)]
        fs::set_permissions(to, meta.permissions())?;
    }
    Ok(())
}

fn hash(path: &Path, buffer: &mut [u8]) -> io::Result<([u8; 32], u64)> {
    let mut input = faber_winproc::open_catalog_file(path.parent().ok_or_else(invalid)?, Path::new(path.file_name().ok_or_else(invalid)?))?;
    let mut digest = Sha256::new();
    let mut bytes = 0;
    loop {
        let count = input.read(buffer)?;
        if count == 0 {
            return Ok((digest.finalize().into(), bytes));
        }
        bytes += count as u64;
        digest.update(&buffer[..count]);
    }
}

fn verify(from: &Path, to: Option<&Path>, buffer: &mut [u8]) -> io::Result<[u8; 32]> {
    let a = metadata(from)?;
    if let Some(target) = to {
        let b = metadata(target)?;
        if a.file_type() != b.file_type() || (a.is_file() && a.len() != b.len()) {
            return Err(invalid());
        }
    }
    let mut digest = Sha256::new();
    if a.is_dir() {
        let mut entries = fs::read_dir(from)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_unstable_by_key(fs::DirEntry::file_name);
        if let Some(target) = to
            && fs::read_dir(target)?.try_fold(0, |count, entry| entry.map(|_| count + 1))? != entries.len()
        {
            return Err(invalid());
        }
        digest.update(b"directory");
        for entry in entries {
            let name = entry.file_name();
            digest.update((name.len() as u64).to_le_bytes());
            digest.update(name.as_encoded_bytes());
            digest.update(verify(&entry.path(), to.map(|target| target.join(&name)).as_deref(), buffer)?);
        }
    } else if a.is_file() {
        let source = hash(from, buffer)?;
        if source.1 != a.len() {
            return Err(invalid());
        }
        if let Some(target) = to
            && source != hash(target, buffer)?
        {
            return Err(invalid());
        }
        digest.update(b"file");
        digest.update(source.1.to_le_bytes());
        digest.update(source.0);
    } else {
        let link = fs::read_link(from)?;
        if let Some(target) = to
            && link != fs::read_link(target)?
        {
            return Err(invalid());
        }
        digest.update(b"link");
        digest.update(link.as_os_str().as_encoded_bytes());
    }
    Ok(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fixture {
        _temp: tempfile::TempDir,
        data: PathBuf,
        target: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let base = PathBuf::from(crate::project::git::native_path(&fs::canonicalize(temp.path()).unwrap()));
            let data = base.join("profile");
            let root = base.join("chosen/Faber-storage");
            let target = root.join("tools");
            fs::create_dir_all(&target).unwrap();
            fs::write(root.join(".faber-owner"), data.to_str().unwrap()).unwrap();
            Self { _temp: temp, data, target }
        }

        fn store(&self, ids: &[&str]) -> Store {
            let items = ids
                .iter()
                .map(|id| {
                    let mut item = super::super::find("roblox-dev-skill").unwrap().clone();
                    item.id = (*id).into();
                    let folder = self.data.join("tools/plugin/skills").join(id);
                    fs::create_dir_all(&folder).unwrap();
                    fs::write(folder.join("SKILL.md"), b"Catalog file\n").unwrap();
                    item.digest = super::super::digest(&folder).unwrap();
                    item
                })
                .collect::<Vec<_>>();
            let store = Store::with_catalog(&self.data, items.clone());
            store.write_ledger(&items.into_iter().map(|item| (item.id, item.commit)).collect()).unwrap();
            store
        }
    }

    #[test]
    fn catalog_move_copies_a_skill_and_disabled_runtime_with_its_pin() {
        let fixture = Fixture::new();
        let skill = fixture.store(&["skill"]);
        let mut items = skill.items.clone();
        let runtime = super::super::catalog()
            .iter()
            .find(|item| item.runtime.as_ref().is_some_and(|r| r.kind == super::super::runtime::Kind::Python))
            .unwrap()
            .clone();
        let id = runtime.id.clone();
        let relative = Path::new("disabled").join(&id);
        let source = skill.root.join(&relative);
        fs::create_dir_all(source.join("instructions")).unwrap();
        fs::write(source.join("instructions/SKILL.md"), b"Runtime instructions").unwrap();
        let entry = b"interpreter snapshot";
        let digest = crate::hex(&Sha256::digest(entry));
        let pin = Path::new("runtime-pins").join(&digest);
        fs::create_dir_all(skill.root.join("runtime-pins")).unwrap();
        fs::write(skill.root.join(&pin), entry).unwrap();
        fs::write(source.join("interpreter.json"), digest).unwrap();
        let mut ledger = skill.initial_ledger().unwrap();
        ledger.insert(id.clone(), runtime.commit.clone());
        skill.write_ledger(&ledger).unwrap();
        items.push(runtime);
        let store = Store::with_catalog(&fixture.data, items);
        let mut moved = 0;
        let mut progress = vec![];
        store
            .move_catalog(
                &fixture.target,
                || MoveControl::Go,
                |event| match event {
                    MoveEvent::Item { number, total, .. } => progress.push((number, total)),
                    MoveEvent::Moved => moved += 1,
                    MoveEvent::Failed { id, error } => panic!("{id}: {error}"),
                    _ => {}
                },
            )
            .unwrap();
        assert_eq!(progress, [(1, 2), (2, 2)]);
        assert_eq!(moved, 2);
        for item in ["skill", &id] {
            assert_eq!(store.source_root(item), fixture.target);
            assert!(store.moved_receipt(item).is_some());
        }
        assert!(source.is_dir());
        assert!(fixture.target.join(relative).is_dir());
        assert_eq!(fs::read(fixture.target.join(pin)).unwrap(), entry);
    }

    #[test]
    fn catalog_move_keeps_a_running_servers_files_and_resolves_new_launches_at_the_new_location() {
        let fixture = Fixture::new();
        let mut item = super::super::catalog().iter().find(|item| item.server.is_some()).unwrap().clone();
        let entry = &item.server.as_ref().unwrap().entry;
        let relative = Path::new("servers").join(&item.id);
        let original = fixture.data.join("tools").join(&relative);
        let script = original.join(entry);
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, b"// Catalog server entry\n").unwrap();
        item.digest = super::super::digest(&original).unwrap();
        let store = Store::with_catalog(&fixture.data, vec![item.clone()]).with_node(fixture.data.join("node"));
        store.write_ledger(&BTreeMap::from([(item.id.clone(), item.commit.clone())])).unwrap();
        let old_launch = store.delivery().servers.pop().unwrap();
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        let new_launch = store.delivery().servers.pop().unwrap();
        assert_eq!(Path::new(&old_launch.args[0]), script);
        assert!(script.is_file());
        assert_eq!(Path::new(&new_launch.args[0]), fixture.target.join(&relative).join(entry));
        store.remove_old_catalog_copies(|| true, |_| false).unwrap();
        assert!(!original.exists());
        assert!(Path::new(&new_launch.args[0]).is_file());
    }

    #[test]
    fn catalog_move_interruption_leaves_one_authoritative_location_at_each_checkpoint() {
        for interrupted in [Stage::Copied, Stage::Verified, Stage::Published, Stage::Switched] {
            let fixture = Fixture::new();
            let store = fixture.store(&["skill"]);
            let mut reached = false;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = store.move_one(&fixture.target, "skill", &|| MoveControl::Go, &mut |_| {}, |stage| {
                    if stage == interrupted {
                        reached = true;
                        panic!("interrupt the worker before its cleanup");
                    }
                    Ok(())
                });
            }));
            assert!(reached && result.is_err());
            let relative = Path::new("plugin/skills/skill/SKILL.md");
            let reopened = Store::with_catalog(&fixture.data, store.items.clone());
            assert!(reopened.root.join(relative).is_file());
            assert_eq!(
                reopened.source_root("skill"),
                if interrupted == Stage::Switched { &fixture.target } else { &store.root }.to_path_buf()
            );
            assert_eq!(fixture.target.join(relative).exists(), matches!(interrupted, Stage::Published | Stage::Switched));
            assert_eq!(reopened.has_old_catalog_copies(), interrupted == Stage::Switched);
            if interrupted != Stage::Switched {
                let moved = reopened.move_one(&fixture.target, "skill", &|| MoveControl::Go, &mut |_| {}, |_| Ok(())).unwrap();
                assert!(matches!(moved, Outcome::Moved));
                assert_eq!(reopened.source_root("skill"), fixture.target);
                assert!(!fixture.target.join(".storage-move/skill").exists());
            }
        }
    }

    #[test]
    fn catalog_move_rejects_damaged_copies_and_continues_with_the_next_item() {
        for extra_file in [false, true] {
            let fixture = Fixture::new();
            let store = fixture.store(&["first", "second"]);
            let mut failures = vec![];
            store
                .move_catalog_with(
                    &fixture.target,
                    || MoveControl::Go,
                    |event| {
                        if let MoveEvent::Failed { id, error } = event {
                            failures.push((id.to_owned(), error));
                        }
                    },
                    |id, stage| {
                        if id == "first" && stage == Stage::Copied {
                            let path = fixture.target.join(".storage-move/first/plugin/skills/first");
                            fs::write(path.join(if extra_file { "extra" } else { "SKILL.md" }), b"Damaged file\n")?;
                        }
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(failures, [("first".into(), ITEM_FAILED)]);
            assert_eq!(store.source_root("first"), store.root);
            assert_eq!(store.source_root("second"), fixture.target);
        }
    }

    #[test]
    fn catalog_move_drive_loss_mid_item_keeps_the_old_locations_and_stops() {
        let fixture = Fixture::new();
        let store = fixture.store(&["first", "second"]);
        let owner = fixture.target.parent().unwrap().join(".faber-owner");
        let mut began = vec![];
        let result = store.move_catalog(
            &fixture.target,
            || MoveControl::Go,
            |event| match event {
                MoveEvent::Item { id, .. } => began.push(id.to_owned()),
                MoveEvent::Copied(_) if owner.exists() => fs::remove_file(&owner).unwrap(),
                _ => {}
            },
        );
        assert_eq!(result, Err(UNREACHABLE));
        assert_eq!(began, ["first"]);
        assert_eq!(store.movable_catalog_ids().unwrap(), ["first", "second"]);
        assert!(store.root.join("plugin/skills/first/SKILL.md").is_file());
        fs::write(&owner, fixture.data.to_str().unwrap()).unwrap();
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        assert_eq!(store.source_root("first"), fixture.target);
        assert!(!fixture.target.join(".storage-move/first").exists());
    }

    #[test]
    fn catalog_move_stop_finishes_the_current_item_only() {
        let fixture = Fixture::new();
        let store = fixture.store(&["first", "second"]);
        let stop = Cell::new(false);
        let copied = Cell::new(0);
        let end = store
            .move_catalog(
                &fixture.target,
                || if stop.get() { MoveControl::Stop } else { MoveControl::Go },
                |event| {
                    if let MoveEvent::Copied(bytes) = event {
                        copied.set(bytes);
                        stop.set(true);
                    }
                },
            )
            .unwrap();
        assert_eq!(end, MoveEnd::Stopped);
        assert_eq!(copied.get(), b"Catalog file\n".len() as u64);
        assert_eq!(store.source_root("first"), fixture.target);
        assert_eq!(store.source_root("second"), store.root);
        assert!(!fixture.target.join("plugin/skills/second").exists());
    }

    #[test]
    fn catalog_move_removes_retained_folders_after_uninstall_even_with_the_drive_away() {
        let fixture = Fixture::new();
        let store = fixture.store(&["skill"]);
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        store.remove("skill").unwrap();
        assert!(!store.initial_ledger().unwrap().contains_key("skill"));
        let old = store.root.join("plugin/skills/skill");
        assert!(old.is_dir(), "catalog pruning must retain the original");
        assert!(store.has_old_catalog_copies());
        let outside = fixture.target.join("removed/skill/files/SKILL.md");
        assert!(outside.is_file());
        fs::remove_file(fixture.target.parent().unwrap().join(".faber-owner")).unwrap();
        store.remove_old_catalog_copies(|| true, |_| false).unwrap();
        assert!(!old.exists() && !store.has_old_catalog_copies());
        assert!(outside.is_file(), "cleanup is confined to the default store");
    }

    #[test]
    fn catalog_move_removal_refuses_a_linked_parent_and_a_missing_ledger() {
        let fixture = Fixture::new();
        let store = fixture.store(&["skill"]);
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        let _store = lock();
        let parent = store.root.join("plugin/skills");
        let outside = fixture.data.join("outside");
        fs::rename(&parent, &outside).unwrap();
        #[cfg(windows)]
        junction::create(&outside, &parent).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &parent).unwrap();
        assert!(store.remove_old_move("skill").is_err());
        assert!(outside.join("skill/SKILL.md").is_file());
        #[cfg(windows)]
        junction::delete(&parent).unwrap();
        #[cfg(unix)]
        fs::remove_file(&parent).unwrap();
        fs::rename(outside, &parent).unwrap();
        fs::remove_file(store.ledger()).unwrap();
        fs::remove_file(fixture.target.parent().unwrap().join(".faber-owner")).unwrap();
        assert!(store.remove_old_move("skill").is_err());
        assert!(parent.join("skill/SKILL.md").is_file());
    }

    #[test]
    fn catalog_move_cleanup_accepts_a_verified_newer_replacement_and_preserves_an_unreadable_one() {
        let fixture = Fixture::new();
        let store = fixture.store(&["skill"]);
        let plan = store.capture_replacement("skill").unwrap();
        let journal = |plan| {
            serde_json::to_vec(&serde_json::json!({
                "plan": plan, "finished": true, "rolled_back": false,
            }))
            .unwrap()
        };
        let backup = Path::new("replacements/skill");
        fs::create_dir_all(store.root.join(backup).join("old")).unwrap();
        fs::write(store.root.join(backup).join("old/SKILL.md"), b"Catalog file\n").unwrap();
        fs::write(store.root.join(backup).join("state.json"), journal(&plan)).unwrap();
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        let state = fixture.target.join(backup).join("state.json");
        fs::write(&state, b"unreadable journal").unwrap();
        assert_eq!(store.remove_old_catalog_copies(|| true, |_| false), Err("error.storage.removeOld"));
        assert!(store.root.join(backup).is_dir());
        fs::write(&state, journal(&plan)).unwrap();
        let mut updated = store.items[0].clone();
        updated.commit = "new-catalog-commit".into();
        let store = Store::with_catalog(&fixture.data, vec![updated.clone()]);
        let replacement = store.capture_replacement("skill").unwrap();
        fs::write(&state, journal(&replacement)).unwrap();
        store.write_ledger(&BTreeMap::from([(updated.id, updated.commit)])).unwrap();
        store.remove_old_catalog_copies(|| true, |_| false).unwrap();
        assert!(!store.root.join(backup).exists());
        assert!(state.is_file());
    }

    /// Whether another thread can take the store lock now: an install, the catalog list or a kit would wait otherwise.
    fn store_lock_is_free() -> bool {
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _store = lock();
            let _ = sent.send(());
        });
        received.recv_timeout(std::time::Duration::from_secs(10)).is_ok()
    }

    #[test]
    fn catalog_move_copies_and_verifies_without_the_store_lock_and_publishes_only_an_unchanged_item() {
        let fixture = Fixture::new();
        let store = fixture.store(&["first", "second"]);
        let mut free = vec![];
        let mut failures = vec![];
        store
            .move_catalog_with(
                &fixture.target,
                || MoveControl::Go,
                |event| {
                    if let MoveEvent::Failed { id, error } = event {
                        failures.push((id.to_owned(), error));
                    }
                },
                |id, stage| {
                    if matches!(stage, Stage::Copied | Stage::Verified) {
                        free.push(store_lock_is_free());
                    }
                    if id == "second" && stage == Stage::Verified {
                        // An update lands after the copy was verified: same size, new contents.
                        fs::write(
                            fixture.data.join("tools/plugin/skills/second/SKILL.md"),
                            b"Updated file
",
                        )?;
                    }
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(free, [true; 4], "copying and verifying never hold the store lock");
        assert_eq!(store.source_root("first"), fixture.target);
        assert_eq!(failures, [("second".into(), ITEM_FAILED)]);
        assert_eq!(store.source_root("second"), store.root);
        assert!(!fixture.target.join("plugin/skills/second").exists());
        assert!(!fixture.target.join(".storage-move/second").exists());
        assert!(store.moved_receipt("second").is_none());
    }

    #[test]
    fn catalog_move_removal_keeps_the_originals_a_live_session_may_still_use() {
        let fixture = Fixture::new();
        let store = fixture.store(&["first", "second"]);
        store.move_catalog(&fixture.target, || MoveControl::Go, |_| {}).unwrap();
        let kept = store.remove_old_catalog_copies(|| true, |id| id == "first").unwrap();
        assert_eq!(kept, ["first"]);
        assert!(store.root.join("plugin/skills/first/SKILL.md").is_file(), "a live session still reads it");
        assert!(!store.root.join("plugin/skills/second").exists());
        assert!(store.has_old_catalog_copies());
        assert!(store.remove_old_catalog_copies(|| true, |_| false).unwrap().is_empty());
        assert!(!store.root.join("plugin/skills/first").exists() && !store.has_old_catalog_copies());
    }

    #[test]
    fn catalog_move_abandons_the_item_in_flight_when_the_destination_is_no_longer_chosen() {
        let fixture = Fixture::new();
        let store = fixture.store(&["first", "second"]);
        let changed = Cell::new(false);
        let end = store
            .move_catalog(
                &fixture.target,
                || if changed.get() { MoveControl::Abandon } else { MoveControl::Go },
                |event| {
                    if let MoveEvent::Copied(_) = event {
                        changed.set(true);
                    }
                },
            )
            .unwrap();
        assert_eq!(end, MoveEnd::Abandoned);
        for id in ["first", "second"] {
            assert_eq!(store.source_root(id), store.root);
            assert!(!fixture.target.join("plugin/skills").join(id).exists());
        }
        assert!(!fixture.target.join(".storage-move/first").exists());
        assert!(!store.has_old_catalog_copies());
    }
}
