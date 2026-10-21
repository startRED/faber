//! A SHA-256 bare Git object store owned by Faber. No working tree, index, alternates, hooks or user refs.
use super::*;
use faber_winproc::rewind::{allocated, private_dir};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::io::{Seek, SeekFrom, Write};

struct PendingBatch {
    id: String,
    staging: String,
    folder: String,
    writer: pack::Batch,
    charge: u64,
}

#[derive(Default)]
struct Budget {
    total: u64,
    folders: BTreeMap<String, u64>,
    free: u64,
}

impl Budget {
    fn capture(&mut self, folder: &str, bytes: u64, points: usize) -> Result<(), String> {
        if points >= 100 {
            return Err("error.rewind.budget".into());
        }
        if self.free < TOTAL_LIMIT {
            return Err("error.rewind.space".into());
        }
        self.reserve(folder, bytes, false)?;
        self.change(folder, bytes, 0)
    }

    fn reserve(&mut self, folder: &str, bytes: u64, recovery: bool) -> Result<(), String> {
        let reserve = if recovery { 0 } else { RECOVERY_RESERVE };
        if self.total.saturating_add(bytes) > TOTAL_LIMIT - reserve
            || self.folders.get(folder).copied().unwrap_or(0).saturating_add(bytes) > FOLDER_LIMIT - reserve
        {
            return Err("error.rewind.budget".into());
        }
        self.change(folder, 0, bytes)
    }

    fn change(&mut self, folder: &str, old: u64, new: u64) -> Result<(), String> {
        let total = self.total.checked_sub(old).and_then(|n| n.checked_add(new)).ok_or("error.rewind.storage")?;
        let folder_total = self
            .folders
            .get(folder)
            .copied()
            .unwrap_or(0)
            .checked_sub(old)
            .and_then(|n| n.checked_add(new))
            .ok_or("error.rewind.storage")?;
        self.total = total;
        self.folders.insert(folder.into(), folder_total);
        Ok(())
    }
}

pub struct History {
    data: PathBuf,
    root: Root,
    pub(super) points: BTreeMap<String, Point>,
    pub(super) journals: BTreeMap<String, Journal>,
    /// A failed recovery/cleanup closes history admission, never ordinary conversation admission.
    healthy: bool,
    budget: RefCell<Option<Budget>>,
    prepared: RefCell<BTreeSet<String>>,
    work: Cell<Work>,
    batch: RefCell<Option<PendingBatch>>,
    charges: RefCell<BTreeMap<String, u64>>,
}

fn hex_name(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid_name(s: &str) -> bool {
    Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s)
}

impl History {
    pub fn open(data: &Path, at: u64) -> Result<Self, String> {
        let data = data.join("rewind");
        private_dir(&data).map_err(error)?;
        let root = Root::open(&data).map_err(error)?;
        let mut history = Self {
            data,
            root,
            points: BTreeMap::new(),
            journals: BTreeMap::new(),
            healthy: true,
            budget: RefCell::new(None),
            prepared: RefCell::new(BTreeSet::new()),
            work: Cell::new(Work::default()),
            batch: RefCell::new(None),
            charges: RefCell::new(BTreeMap::new()),
        };
        history.load()?;
        history.recover_history_cleanup()?;
        history.inspect_project_cleanup()?;
        history.cleanup(at)?;
        Ok(history)
    }

    pub(super) fn folder_dir(&self, folder: &str) -> PathBuf {
        self.data.join(folder)
    }

    pub fn work(&self) -> Work {
        self.work.get()
    }
    pub fn reset_work(&self) {
        self.work.set(Work::default());
    }
    pub(super) fn record_work(&self, update: impl FnOnce(&mut Work)) {
        let mut work = self.work.get();
        update(&mut work);
        self.work.set(work);
    }

    fn prepare_dir(&self, name: &str) -> Result<(), String> {
        if !self.prepared.borrow().contains(name) {
            private_dir(&self.data.join(name)).map_err(error)?;
            self.prepared.borrow_mut().insert(name.into());
        }
        Ok(())
    }

    pub(super) fn folder_id(&self, path: &Path, identity: &[u8]) -> String {
        let mut hash = Sha256::new();
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(identity);
        crate::hex(&hash.finalize())
    }

    pub(super) fn prepare_folder(&self, folder: &str) -> Result<(), String> {
        if !hex_name(folder, 64) {
            return Err("error.rewind.path".into());
        }
        if self.prepared.borrow().contains(&format!("{folder}/ready")) {
            return Ok(());
        }
        let dir = self.folder_dir(folder);
        for suffix in ["", "objects", "objects/info", "objects/pack", "refs", "refs/heads", "points", "journals", "staging"] {
            private_dir(&dir.join(suffix)).map_err(error)?;
        }
        for (name, bytes) in [
            ("config", b"[core]\nrepositoryformatversion = 1\nbare = true\n[extensions]\nobjectformat = sha256\n".as_slice()),
            ("HEAD", b"ref: refs/heads/history\n".as_slice()),
        ] {
            match self.root.open_file(&format!("{folder}/{name}")) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => self.write_new(&format!("{folder}/{name}"), bytes, false)?,
                Err(e) => return Err(error(e)),
            }
        }
        self.prepared.borrow_mut().insert(format!("{folder}/ready"));
        Ok(())
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, name: &str) -> Result<T, String> {
        let mut file = self.root.open_file(name).map_err(error)?;
        if file.metadata().map_err(error)?.len() > RECOVERY_RESERVE {
            return Err("error.rewind.budget".into());
        }
        let mut bytes = Vec::new();
        let identity = fingerprint(&file).map_err(error)?;
        StableRead { file: &mut file, identity: &identity }.take(RECOVERY_RESERVE + 1).read_to_end(&mut bytes).map_err(error)?;
        serde_json::from_slice(&bytes).map_err(error)
    }

    fn load(&mut self) -> Result<(), String> {
        for entry in std::fs::read_dir(&self.data).map_err(error)? {
            let entry = entry.map_err(error)?;
            let folder = entry.file_name().to_string_lossy().into_owned();
            if !hex_name(&folder, 64) || entry.file_type().map_err(error)?.is_symlink() || !entry.file_type().map_err(error)?.is_dir() {
                continue;
            }
            for kind in ["points", "journals"] {
                let dir = entry.path().join(kind);
                if dir.symlink_metadata().is_err() {
                    continue;
                }
                if !dir.symlink_metadata().is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
                    return Err("error.rewind.path".into());
                }
                for entry in std::fs::read_dir(dir).map_err(error)? {
                    let entry = entry.map_err(error)?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let Some(id) = name.strip_suffix(".json").filter(|id| uuid_name(id)) else {
                        continue;
                    };
                    let relative = format!("{folder}/{kind}/{name}");
                    if kind == "points" {
                        let point: Point = self.read_json(&relative)?;
                        if point.folder != folder
                            || point.info.id != id
                            || !uuid_name(&point.conversation)
                            || point
                                .before
                                .files
                                .keys()
                                .chain(point.before.absent.iter())
                                .chain(point.before.skipped.iter())
                                .any(|p| !allowed(&point.root, p))
                        {
                            return Err("error.rewind.path".into());
                        }
                        self.points.insert(id.into(), point);
                    } else {
                        let journal: Journal = self.read_json(&relative)?;
                        if journal.folder != folder
                            || journal.id != id
                            || !uuid_name(&journal.conversation)
                            || journal.operations.iter().any(|op| !allowed(&journal.root, &op.path))
                        {
                            return Err("error.rewind.path".into());
                        }
                        self.journals.insert(id.into(), journal);
                    }
                }
            }
        }
        Ok(())
    }

    /// Account for every owned file, including staging and unfinished journals. Logical object sizes and allocation
    /// are both counted: compressible data and sparse metadata cannot buy extra history.
    fn measured(&self, name: &str) -> Result<u64, String> {
        let mut file = self.root.open_file(name).map_err(error)?;
        let mut size = file.metadata().map_err(error)?.len().max(allocated(&file).map_err(error)?);
        let parts: Vec<_> = name.split('/').collect();
        let object = parts.len() == 4 && parts[1] == "objects" && hex_name(parts[2], 2) && hex_name(parts[3], 62);
        let staging = parts.len() == 3 && parts[1] == "staging" && parts[2].strip_suffix(".tmp").is_some_and(uuid_name);
        if object || staging {
            let identity = fingerprint(&file).map_err(error)?;
            let mut decoded = ZlibDecoder::new(StableRead { file: &mut file, identity: &identity });
            let mut head = [0u8; 32];
            let raw = (|| {
                let n = decoded.read(&mut head).map_err(error)?;
                let end = head[..n].iter().position(|b| *b == 0).ok_or("error.rewind.storage")?;
                let raw = std::str::from_utf8(&head[..end])
                    .map_err(error)?
                    .strip_prefix("blob ")
                    .ok_or("error.rewind.storage")?
                    .parse::<u64>()
                    .map_err(error)?;
                if raw > FILE_LIMIT {
                    return Err("error.rewind.storage".into());
                }
                Ok::<_, String>(raw + end as u64 + 1)
            })();
            size = size.max(match raw {
                Ok(raw) => raw,
                Err(_) if staging => FILE_LIMIT + 32,
                Err(e) => return Err(e),
            });
        }
        Ok(size)
    }

    fn refresh_usage(&self) -> Result<(), String> {
        fn walk(
            history: &History,
            relative: &str,
            total: &mut u64,
            folders: &mut BTreeMap<String, u64>,
            count: &mut usize,
            charges: &mut BTreeMap<String, u64>,
        ) -> Result<(), String> {
            let path = history.data.join(relative);
            for entry in std::fs::read_dir(&path).map_err(error)? {
                let entry = entry.map_err(error)?;
                *count += 1;
                if *count > 200_000 {
                    return Err("error.rewind.budget".into());
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let child = if relative.is_empty() { name.clone() } else { format!("{relative}/{name}") };
                let kind = entry.file_type().map_err(error)?;
                if kind.is_symlink() {
                    return Err("error.rewind.path".into());
                }
                if kind.is_dir() {
                    walk(history, &child, total, folders, count, charges)?;
                    continue;
                }
                let size = history.measured(&child)?;
                charges.insert(child.clone(), size);
                let parts: Vec<_> = child.split('/').collect();
                *total = total.checked_add(size).ok_or("error.rewind.budget")?;
                *folders.entry(parts[0].into()).or_default() += size;
            }
            Ok(())
        }
        let (mut total, mut folders) = (0, BTreeMap::new());
        let mut charges = BTreeMap::new();
        walk(self, "", &mut total, &mut folders, &mut 0, &mut charges)?;
        let free = self.root.available_space().map_err(error)?;
        *self.budget.borrow_mut() = Some(Budget { total, folders, free });
        *self.charges.borrow_mut() = charges;
        Ok(())
    }

    fn usage(&self) -> Result<(u64, BTreeMap<String, u64>), String> {
        if self.budget.borrow().is_none() {
            self.refresh_usage()?;
        }
        let budget = self.budget.borrow();
        let budget = budget.as_ref().ok_or("error.rewind.storage")?;
        Ok((budget.total, budget.folders.clone()))
    }

    fn reserve(&self, folder: &str, bytes: u64, recovery: bool) -> Result<(), String> {
        if !self.healthy {
            return Err("error.rewind.storage".into());
        }
        if self.budget.borrow().is_none() {
            self.refresh_usage()?;
        }
        self.budget.borrow_mut().as_mut().ok_or("error.rewind.storage")?.reserve(folder, bytes, recovery)
    }

    fn account(&self, folder: &str, old: u64, new: u64) -> Result<(), String> {
        self.budget.borrow_mut().as_mut().ok_or("error.rewind.storage")?.change(folder, old, new)
    }

    pub(super) fn admit(&mut self, folder: &str, bytes: u64) -> Result<(), String> {
        if !self.healthy {
            return Err("error.rewind.storage".into());
        }
        if self.budget.borrow().is_none() {
            self.refresh_usage()?;
        }
        self.budget.borrow_mut().as_mut().ok_or("error.rewind.storage")?.free = self.root.available_space().map_err(error)?;
        // An impossible reservation must not retire otherwise usable history.
        Budget::default().reserve(folder, bytes, false)?;
        let count = self.points.values().filter(|p| p.folder == folder).count();
        match self.budget.borrow_mut().as_mut().ok_or("error.rewind.storage")?.capture(folder, bytes, count) {
            Ok(()) => return Ok(()),
            Err(e) if e == "error.rewind.budget" => {}
            Err(e) => return Err(e),
        }
        let result = self.make_room(folder, bytes);
        if result.as_ref().is_err_and(|e| e != "error.rewind.budget" && e != "error.rewind.space") {
            self.healthy = false;
        }
        result
    }

    fn make_room(&mut self, folder: &str, bytes: u64) -> Result<(), String> {
        let mut ordered: Vec<_> = self.points.values().map(|p| (p.info.at, p.info.id.clone())).collect();
        ordered.sort();
        loop {
            let count = self.points.values().filter(|p| p.folder == folder).count();
            match self.budget.borrow_mut().as_mut().ok_or("error.rewind.storage")?.capture(folder, bytes, count) {
                Ok(()) => return Ok(()),
                Err(e) if e == "error.rewind.budget" => {}
                Err(e) => return Err(e),
            }
            let (_, folders) = self.usage()?;
            // A folder or point-count limit can only be relieved by retiring this folder's points.
            let folder_only =
                count >= 100 || folders.get(folder).copied().unwrap_or(0).saturating_add(bytes) > FOLDER_LIMIT - RECOVERY_RESERVE;
            let Some(id) = ordered.iter().map(|(_, id)| id).find(|id| {
                self.points.get(*id).is_some_and(|p| !p.active && (!folder_only || p.folder == folder))
                    && !self.journals.values().any(|j| j.point.as_ref() == Some(*id))
            }) else {
                return Err("error.rewind.budget".into());
            };
            let point = &self.points[id];
            let owned_folder = point.folder.clone();
            self.delete(&format!("{owned_folder}/points/{id}.json"))?;
            self.points.remove(id);
            self.collect_unreferenced()?;
        }
    }

    pub(super) fn begin_batch(&self, folder: &str) -> Result<(), String> {
        // An abandoned scan never publishes its references. Closing it leaves only owned, reclaimable staging.
        self.batch.borrow_mut().take();
        self.reserve(folder, 64 * 1024, false)?;
        let id = Uuid::new_v4().to_string();
        let staging = format!("{folder}/staging/{id}.tmp");
        let file = match self.root.private_create(&staging) {
            Ok(file) => file,
            Err(error) => {
                self.account(folder, 64 * 1024, 0)?;
                return Err(super::error(error));
            }
        };
        let result = (|| {
            let writer = pack::Batch::new(file).map_err(error)?;
            let charge = writer.file().metadata().map_err(error)?.len().max(allocated(writer.file()).map_err(error)?);
            self.account(folder, 64 * 1024, charge)?;
            self.charges.borrow_mut().insert(staging.clone(), charge);
            *self.batch.borrow_mut() = Some(PendingBatch { id, staging: staging.clone(), folder: folder.into(), writer, charge });
            Ok(())
        })();
        if result.is_err() && self.root.remove_owned(&staging).is_ok() {
            let _ = self.account(folder, 64 * 1024, 0);
        }
        result
    }

    pub(super) fn write_capture_blob(&self, folder: &str, source: &mut dyn Read, len: u64) -> Result<(String, String, u64), String> {
        let began = Instant::now();
        let result = (|| {
            if len > FILE_LIMIT {
                return Err("error.rewind.tooLarge".into());
            }
            let reservation = len + 64 * 1024;
            self.reserve(folder, reservation, false)?;
            let mut batch = self.batch.borrow_mut();
            let batch = batch.as_mut().filter(|batch| batch.folder == folder).ok_or("error.rewind.storage")?;
            let count = batch.writer.count();
            let saved = batch.writer.add(source, len).map_err(error);
            let charge = batch.writer.file().metadata().map_err(error)?.len().max(allocated(batch.writer.file()).map_err(error)?);
            self.account(folder, batch.charge + reservation, charge)?;
            batch.charge = charge;
            self.charges.borrow_mut().insert(batch.staging.clone(), charge);
            let entry = saved?;
            if entry.bytes != len {
                return Err("error.rewind.storage".into());
            }
            if batch.writer.count() != count {
                self.record_work(|work| work.objects_written += 1);
            }
            Ok((entry.oid, batch.id.clone(), entry.offset))
        })();
        self.record_work(|work| work.object_us += micros(began));
        result
    }

    pub(super) fn finish_batch(&self, snapshot: &mut Snapshot) -> Result<(), String> {
        let began = Instant::now();
        let batch = self.batch.borrow_mut().take().ok_or("error.rewind.storage")?;
        let PendingBatch { id, staging, folder, writer, charge } = batch;
        if writer.count() == 0 {
            drop(writer.into_file());
            self.root.remove_owned(&staging).map_err(error)?;
            self.account(&folder, charge, 0)?;
            self.charges.borrow_mut().remove(&staging);
            return Ok(());
        }
        // The trailer, index and allocation rounding all remain reserved until the payload is durably published.
        self.reserve(&folder, 2 * 1024 * 1024, false)?;
        let (file, checksum, index) = writer.finish().map_err(error)?;
        file.sync_all().map_err(error)?;
        self.record_work(|work| work.object_flushes += 1);
        let final_charge = file.metadata().map_err(error)?.len().max(allocated(&file).map_err(error)?);
        drop(file);
        let destination = format!("{folder}/objects/pack/pack-{checksum}.pack");
        match self.root.rename_new(&staging, &destination) {
            Ok(()) => {
                self.account(&folder, charge, final_charge)?;
                self.charges.borrow_mut().insert(destination.clone(), final_charge);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                self.verify_pack(&destination, &checksum)?;
                self.root.remove_owned(&staging).map_err(error)?;
                self.account(&folder, charge, 0)?;
            }
            Err(e) => return Err(error(e)),
        }
        self.charges.borrow_mut().remove(&staging);
        // An index is derived metadata; engine reads use the durable manifest's checked pack offsets.
        // Losing it cannot lose history. A later publication can recreate it from the same payload.
        let index_name = format!("{folder}/objects/pack/pack-{checksum}.idx");
        match self.root.private_create(&index_name) {
            Ok(mut file) => {
                file.write_all(&index).map_err(error)?;
                let size = file.metadata().map_err(error)?.len().max(allocated(&file).map_err(error)?);
                self.account(&folder, 2 * 1024 * 1024, size)?;
                self.charges.borrow_mut().insert(index_name, size);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let mut file = self.root.open_file(&index_name).map_err(error)?;
                let identity = fingerprint(&file).map_err(error)?;
                let mut saved = Vec::new();
                StableRead { file: &mut file, identity: &identity }.take(2 * 1024 * 1024).read_to_end(&mut saved).map_err(error)?;
                if saved != index {
                    return Err("error.rewind.storage".into());
                }
                self.account(&folder, 2 * 1024 * 1024, 0)?;
            }
            Err(e) => return Err(error(e)),
        }
        for image in snapshot.files.values_mut().filter(|image| image.pack.as_deref() == Some(&id)) {
            image.pack = Some(checksum.clone());
        }
        self.record_work(|work| work.object_us += micros(began));
        Ok(())
    }

    fn verify_pack(&self, name: &str, expected: &str) -> Result<(), String> {
        let mut file = self.root.open_file(name).map_err(error)?;
        let len = file.metadata().map_err(error)?.len();
        if !(44..=BASELINE_LIMIT + 1024 * 1024).contains(&len) {
            return Err("error.rewind.storage".into());
        }
        let identity = fingerprint(&file).map_err(error)?;
        let mut source = StableRead { file: &mut file, identity: &identity };
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut remaining = len - 32;
        while remaining != 0 {
            let count = source.read(&mut buffer[..remaining.min(1024 * 1024) as usize]).map_err(error)?;
            if count == 0 {
                return Err("error.rewind.storage".into());
            }
            hash.update(&buffer[..count]);
            remaining -= count as u64;
        }
        let mut trailer = [0u8; 32];
        source.read_exact(&mut trailer).map_err(error)?;
        if crate::hex(&hash.finalize()) != expected || crate::hex(&trailer) != expected {
            return Err("error.rewind.storage".into());
        }
        Ok(())
    }

    fn write_blob_reserved(&self, folder: &str, source: &mut dyn Read, len: u64, recovery: bool) -> Result<String, String> {
        let began = Instant::now();
        let result = self.write_blob_inner(folder, source, len, recovery);
        self.record_work(|work| work.object_us += micros(began));
        result
    }

    fn write_blob_inner(&self, folder: &str, source: &mut dyn Read, len: u64, recovery: bool) -> Result<String, String> {
        if len > FILE_LIMIT {
            return Err("error.rewind.tooLarge".into());
        }
        let reservation = len + 64 * 1024;
        self.reserve(folder, reservation, recovery)?;
        let staging = format!("{folder}/staging/{}.tmp", Uuid::new_v4());
        let file = match self.root.private_create(&staging) {
            Ok(file) => file,
            Err(e) => {
                self.account(folder, reservation, 0)?;
                return Err(error(e));
            }
        };
        self.charges.borrow_mut().insert(staging.clone(), reservation);
        let mut reserved = true;
        let result = (|| {
            // Stored DEFLATE blocks retain Git's loose-object format without spending seconds compressing a baseline.
            let mut encoder = ZlibEncoder::new(file, Compression::none());
            let header = format!("blob {len}\0");
            encoder.write_all(header.as_bytes()).map_err(error)?;
            let mut hash = Sha256::new();
            hash.update(header.as_bytes());
            let mut buffer = vec![0u8; (len as usize + 1).min(1024 * 1024)];
            let mut read = 0;
            loop {
                let n = source.read(&mut buffer).map_err(error)?;
                if n == 0 {
                    break;
                }
                read += n as u64;
                if read > len {
                    return Err("error.rewind.changed".into());
                }
                hash.update(&buffer[..n]);
                encoder.write_all(&buffer[..n]).map_err(error)?;
            }
            if read != len {
                return Err("error.rewind.changed".into());
            }
            let file = encoder.finish().map_err(error)?;
            file.sync_all().map_err(error)?;
            self.record_work(|work| work.object_flushes += 1);
            let charge = file.metadata().map_err(error)?.len().max(allocated(&file).map_err(error)?).max(len + header.len() as u64);
            drop(file);
            let oid = crate::hex(&hash.finalize());
            self.prepare_dir(&format!("{folder}/objects/{}", &oid[..2]))?;
            let destination = format!("{folder}/objects/{}/{}", &oid[..2], &oid[2..]);
            match self.root.rename_new(&staging, &destination) {
                Ok(()) => {
                    // A published file remains conservatively charged if measuring it fails.
                    reserved = false;
                    self.account(folder, reservation, charge)?;
                    self.charges.borrow_mut().remove(&staging);
                    self.charges.borrow_mut().insert(destination, charge);
                    self.record_work(|work| work.objects_written += 1);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    // A deduplicated blob still has to be readable and valid before the new copy can go.
                    let probe = Image {
                        oid: oid.clone(),
                        bytes: len,
                        protection: Vec::new(),
                        identity: Vec::new(),
                        token: None,
                        pack: None,
                        offset: 0,
                    };
                    self.blob(folder, &probe)?;
                    self.delete_disk(&staging)?;
                    self.account(folder, reservation, 0)?;
                    reserved = false;
                }
                Err(e) => return Err(error(e)),
            }
            Ok(oid)
        })();
        if result.is_err() && reserved && self.delete_disk(&staging).is_ok() {
            let _ = self.account(folder, reservation, 0);
        }
        result
    }

    pub(super) fn save_bytes(&self, folder: &str, bytes: &[u8], protection: Vec<u8>, identity: Vec<u8>) -> Result<Image, String> {
        let mut hash = Sha256::new();
        hash.update(format!("blob {}\0", bytes.len()).as_bytes());
        hash.update(bytes);
        let oid = crate::hex(&hash.finalize());
        if let Some(existing) = self
            .points
            .values()
            .filter(|point| point.folder == folder)
            .flat_map(|point| point.before.files.values().chain(point.after.iter().flat_map(|after| after.files.values())))
            .find(|image| image.oid == oid)
            && self.blob(folder, existing)? == bytes
        {
            let mut image = existing.clone();
            image.protection = protection;
            image.identity = identity;
            image.token = None;
            return Ok(image);
        }
        let oid = self.write_blob_reserved(folder, &mut &bytes[..], bytes.len() as u64, true)?;
        Ok(Image { oid, bytes: bytes.len() as u64, protection, identity, token: None, pack: None, offset: 0 })
    }

    pub(super) fn blob(&self, folder: &str, image: &Image) -> Result<Vec<u8>, String> {
        if !hex_name(folder, 64) || !hex_name(&image.oid, 64) || image.bytes > FILE_LIMIT {
            return Err("error.rewind.path".into());
        }
        self.record_work(|work| work.object_reads += 1);
        if let Some(pack) = &image.pack {
            if !hex_name(pack, 64) {
                return Err("error.rewind.path".into());
            }
            let mut file = self.root.open_file(&format!("{folder}/objects/pack/pack-{pack}.pack")).map_err(error)?;
            let identity = fingerprint(&file).map_err(error)?;
            let size = file.metadata().map_err(error)?.len();
            if image.offset < 12 || image.offset >= size.saturating_sub(32) || size > BASELINE_LIMIT + 1024 * 1024 {
                return Err("error.rewind.storage".into());
            }
            file.seek(SeekFrom::Start(image.offset)).map_err(error)?;
            let bytes = pack::read_entry(&mut StableRead { file: &mut file, identity: &identity }, image.bytes).map_err(error)?;
            if !self.matches(folder, image, &bytes) {
                return Err("error.rewind.storage".into());
            }
            return Ok(bytes);
        }
        let mut file = self.root.open_file(&format!("{folder}/objects/{}/{}", &image.oid[..2], &image.oid[2..])).map_err(error)?;
        let mut bytes = Vec::new();
        let identity = fingerprint(&file).map_err(error)?;
        ZlibDecoder::new(StableRead { file: &mut file, identity: &identity })
            .take(FILE_LIMIT + 64)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if crate::hex(&Sha256::digest(&bytes)) != image.oid {
            return Err("error.rewind.storage".into());
        }
        let header = format!("blob {}\0", image.bytes);
        if !bytes.starts_with(header.as_bytes()) || bytes.len() as u64 != image.bytes + header.len() as u64 {
            return Err("error.rewind.storage".into());
        }
        bytes.drain(..header.len());
        Ok(bytes)
    }

    pub(super) fn matches(&self, _folder: &str, image: &Image, bytes: &[u8]) -> bool {
        let mut hash = Sha256::new();
        hash.update(format!("blob {}\0", bytes.len()).as_bytes());
        hash.update(bytes);
        image.bytes == bytes.len() as u64 && crate::hex(&hash.finalize()) == image.oid
    }

    fn replace_json(&self, name: &str, bytes: &[u8], reserve: bool) -> Result<(), String> {
        let folder = name.split('/').next().ok_or("error.rewind.path")?;
        if bytes.len() as u64 > RECOVERY_RESERVE / 2 {
            return Err("error.rewind.budget".into());
        }
        match self.root.open_file(name) {
            Ok(mut file) => {
                let old_size = self.measured(name)?;
                let identity = fingerprint(&file).map_err(error)?;
                let protect = protection(&file).map_err(error)?;
                let mut old = Vec::new();
                StableRead { file: &mut file, identity: &identity }.take(RECOVERY_RESERVE + 1).read_to_end(&mut old).map_err(error)?;
                if old.len() as u64 > RECOVERY_RESERVE {
                    return Err("error.rewind.budget".into());
                }
                drop(file);
                let reservation = bytes.len() as u64 + 64 * 1024;
                self.reserve(folder, reservation, reserve)?;
                // A failed mutation retains the temporary reservation until the next validated refresh.
                self.root.replace_owned(name, (&identity, &old), (bytes, &protect)).map_err(error)?;
                let size = self.measured(name)?;
                self.account(folder, reservation + old_size, size)?;
                self.charges.borrow_mut().insert(name.into(), size);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => self.write_new(name, bytes, reserve),
            Err(e) => Err(error(e)),
        }
    }

    fn write_new(&self, name: &str, bytes: &[u8], recovery: bool) -> Result<(), String> {
        let folder = name.split('/').next().ok_or("error.rewind.path")?;
        let reservation = bytes.len() as u64 + 64 * 1024;
        self.reserve(folder, reservation, recovery)?;
        let staged = format!("{folder}/staging/{}.tmp", Uuid::new_v4());
        let mut file = self.root.private_create(&staged).map_err(error)?;
        self.charges.borrow_mut().insert(staged.clone(), reservation);
        let result = (|| {
            file.write_all(bytes).map_err(error)?;
            file.sync_all().map_err(error)
        })();
        drop(file);
        let result = result.and_then(|_| self.root.rename_new(&staged, name).map_err(error));
        if let Err(e) = result {
            if self.delete_disk(&staged).is_ok() {
                self.account(folder, reservation, 0)?;
            }
            return Err(e);
        }
        let size = self.measured(name)?;
        self.account(folder, reservation, size)?;
        self.charges.borrow_mut().remove(&staged);
        self.charges.borrow_mut().insert(name.into(), size);
        Ok(())
    }

    pub(super) fn save_point(&self, point: &Point) -> Result<(), String> {
        let began = Instant::now();
        let bytes = serde_json::to_vec(point).map_err(error)?;
        let result = self.replace_json(&format!("{}/points/{}.json", point.folder, point.info.id), &bytes, false);
        self.record_work(|work| work.manifest_us += micros(began));
        result
    }

    pub(super) fn save_journal(&self, journal: &Journal) -> Result<(), String> {
        let bytes = serde_json::to_vec(journal).map_err(error)?;
        self.replace_json(&format!("{}/journals/{}.json", journal.folder, journal.id), &bytes, true)
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        // Retire authoritative metadata durably before collecting its payloads. Orphan deletions may be lazy;
        // resurrecting an old point after its pack was removed must never make a missing version look available.
        let parts: Vec<_> = name.split('/').collect();
        let retired = if parts.len() == 3 && matches!(parts[1], "points" | "journals") && parts[2].ends_with(".json") {
            self.retire_metadata(name)?
        } else {
            name.into()
        };
        let size = self.delete_disk(&retired)?;
        self.account(name.split('/').next().ok_or("error.rewind.path")?, size, 0)
    }

    fn retire_metadata(&self, name: &str) -> Result<String, String> {
        let parts: Vec<_> = name.split('/').collect();
        if parts.len() != 3
            || !hex_name(parts[0], 64)
            || !matches!(parts[1], "points" | "journals")
            || !parts[2].strip_suffix(".json").is_some_and(uuid_name)
        {
            return Err("error.rewind.path".into());
        }
        let size = match self.charges.borrow().get(name).copied() {
            Some(size) => size,
            None => self.measured(name)?,
        };
        let retired = format!("{}/staging/{}.tmp", parts[0], Uuid::new_v4());
        self.root.rename_new(name, &retired).map_err(error)?;
        self.charges.borrow_mut().remove(name);
        self.charges.borrow_mut().insert(retired.clone(), size);
        Ok(retired)
    }

    fn delete_disk(&self, name: &str) -> Result<u64, String> {
        let size = match self.charges.borrow().get(name).copied() {
            Some(size) => size,
            None => match self.measured(name) {
                Ok(size) => size,
                Err(_) if self.root.open_file(name).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) => return Ok(0),
                Err(e) => return Err(e),
            },
        };
        self.root.remove_owned(name).map_err(error)?;
        self.charges.borrow_mut().remove(name);
        self.record_work(|work| work.owned_files_deleted += 1);
        Ok(size)
    }

    pub fn cleanup(&mut self, at: u64) -> Result<(), String> {
        self.batch.borrow_mut().take();
        self.record_work(|work| work.full_cleanup_runs += 1);
        let began = Instant::now();
        let result = self.recover_history_cleanup().and_then(|_| self.refresh_usage()).and_then(|_| self.clean(at));
        self.record_work(|work| work.cleanup_us += micros(began));
        if result.is_err() {
            self.healthy = false;
        }
        result
    }

    fn recover_history_cleanup(&self) -> Result<(), String> {
        let mut incomplete = false;
        for (path, preserved) in self.root.recover_cleanup_tree().map_err(error)? {
            let held = self.root.open_removable(&path).map_err(error)?;
            if preserved.as_ref() != Some(&file_identity(&held).map_err(error)?) {
                // Without the receipt, this may be a new version rather than an already retired history object.
                incomplete = true;
                continue;
            }
            if self.root.remove_preserved(&path, &held).map_err(error)?.is_some() {
                incomplete = true;
            }
        }
        // The final unlink can precede removal of the now empty receipt and quarantine directories.
        self.root.recover_cleanup_tree().map_err(error)?;
        if incomplete { Err("error.rewind.storage".into()) } else { Ok(()) }
    }

    /// This instance owns every history write. Its charged allocations survive ordinary admission and sealing.
    pub(crate) fn cleanup_known(&mut self, at: u64) -> Result<(), String> {
        self.batch.borrow_mut().take();
        if self.budget.borrow().is_none() {
            return self.cleanup(at);
        }
        let began = Instant::now();
        let result = self.clean(at);
        self.record_work(|work| work.cleanup_us += micros(began));
        if result.is_err() {
            self.healthy = false;
            *self.budget.borrow_mut() = None;
        }
        result
    }

    fn clean(&mut self, at: u64) -> Result<(), String> {
        // Unfinished journals and open baselines are pinned. Archive/retirement never changes ownership of history.
        let expired: Vec<_> =
            self.journals.values().filter(|j| j.conversation_done && j.expires <= at).map(|j| (j.id.clone(), j.folder.clone())).collect();
        for (id, folder) in expired {
            self.delete(&format!("{folder}/journals/{id}.json"))?;
            self.journals.remove(&id);
        }
        let mut ordered: Vec<_> = self.points.values().map(|p| (p.info.at, p.info.id.clone())).collect();
        ordered.sort();
        for (_, id) in ordered {
            let point = &self.points[&id];
            if point.active || self.journals.values().any(|j| j.point.as_deref() == Some(&id)) {
                continue;
            }
            let count = self.points.values().filter(|p| p.folder == point.folder).count();
            let (total, folders) = self.usage()?;
            if at.saturating_sub(point.info.at) < RETENTION
                && count < 100
                && total < TOTAL_LIMIT - RECOVERY_RESERVE
                && folders.get(&point.folder).copied().unwrap_or(0) < FOLDER_LIMIT - RECOVERY_RESERVE
            {
                continue;
            }
            let folder = point.folder.clone();
            self.delete(&format!("{folder}/points/{id}.json"))?;
            self.points.remove(&id);
            self.collect_unreferenced()?;
        }
        self.collect_unreferenced()?;
        let (total, folders) = self.usage()?;
        if total > TOTAL_LIMIT || folders.values().any(|bytes| *bytes > FOLDER_LIMIT) {
            return Err("error.rewind.budget".into());
        }
        Ok(())
    }

    fn collect_unreferenced(&self) -> Result<(), String> {
        let mut referenced: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut packs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut reference = |folder: &str, image: &Image| {
            if let Some(pack) = &image.pack {
                packs.entry(folder.into()).or_default().insert(pack.clone());
            } else {
                referenced.entry(folder.into()).or_default().insert(image.oid.clone());
            }
        };
        for point in self.points.values() {
            for image in point.before.files.values().chain(point.after.iter().flat_map(|s| s.files.values())) {
                reference(&point.folder, image);
            }
        }
        for journal in self.journals.values() {
            for image in journal.operations.iter().flat_map(|op| op.before.iter().chain(op.after.iter())) {
                reference(&journal.folder, image);
            }
            for (_, image) in &journal.retained {
                reference(&journal.folder, image);
            }
        }
        for entry in std::fs::read_dir(&self.data).map_err(error)? {
            let entry = entry.map_err(error)?;
            let folder = entry.file_name().to_string_lossy().into_owned();
            if !hex_name(&folder, 64) || !entry.file_type().map_err(error)?.is_dir() || entry.file_type().map_err(error)?.is_symlink() {
                continue;
            }
            let objects = self.folder_dir(&folder).join("objects");
            for prefix in std::fs::read_dir(objects).map_err(error)? {
                let prefix = prefix.map_err(error)?;
                let name = prefix.file_name().to_string_lossy().into_owned();
                if !hex_name(&name, 2) || !prefix.file_type().map_err(error)?.is_dir() || prefix.file_type().map_err(error)?.is_symlink() {
                    continue;
                }
                for blob in std::fs::read_dir(prefix.path()).map_err(error)? {
                    let blob = blob.map_err(error)?;
                    let tail = blob.file_name().to_string_lossy().into_owned();
                    if hex_name(&tail, 62) && !referenced.get(&folder).is_some_and(|set| set.contains(&format!("{name}{tail}"))) {
                        self.delete(&format!("{folder}/objects/{name}/{tail}"))?;
                    }
                }
            }
            for entry in std::fs::read_dir(self.folder_dir(&folder).join("objects/pack")).map_err(error)? {
                let name = entry.map_err(error)?.file_name().to_string_lossy().into_owned();
                if let Some((checksum, extension)) = name.strip_prefix("pack-").and_then(|name| name.rsplit_once('.'))
                    && matches!(extension, "pack" | "idx")
                    && hex_name(checksum, 64)
                    && !packs.get(&folder).is_some_and(|set| set.contains(checksum))
                {
                    self.delete(&format!("{folder}/objects/pack/{name}"))?;
                }
            }
            // A crash can leave a staging blob. Only our exact UUID naming is eligible, never a project path.
            for entry in std::fs::read_dir(self.folder_dir(&folder).join("staging")).map_err(error)? {
                let name = entry.map_err(error)?.file_name().to_string_lossy().into_owned();
                if name.strip_suffix(".tmp").is_some_and(uuid_name) {
                    self.delete(&format!("{folder}/staging/{name}"))?;
                }
            }
            for kind in ["points", "journals"] {
                for entry in std::fs::read_dir(self.folder_dir(&folder).join(kind)).map_err(error)? {
                    let name = entry.map_err(error)?.file_name().to_string_lossy().into_owned();
                    if name
                        .strip_prefix(".faber-rewind-")
                        .map(|s| s.strip_prefix("metadata-").unwrap_or(s))
                        .and_then(|s| s.split_once('-'))
                        .is_some_and(|(pid, sequence)| {
                            !pid.is_empty()
                                && !sequence.is_empty()
                                && pid.bytes().all(|b| b.is_ascii_digit())
                                && sequence.bytes().all(|b| b.is_ascii_digit())
                        })
                    {
                        self.delete(&format!("{folder}/{kind}/{name}"))?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn interrupted_cleanup(root: &Root, source: &str, receipt: bool) -> String {
        let parent = Path::new(source).parent().unwrap_or(Path::new(""));
        let directory = parent.join(".faber-rewind-cleanup-0-4");
        let disk = root.path().join(&directory);
        private_dir(&disk).unwrap();
        if receipt {
            let held = root.open_file(source).unwrap();
            private_dir(&disk.join(format!(".identity-{}", crate::hex(&file_identity(&held).unwrap())))).unwrap();
        }
        let retained = directory.join("retained").to_string_lossy().into_owned();
        root.rename_new(source, &retained).unwrap();
        retained
    }

    #[cfg(unix)]
    #[test]
    fn rewind_startup_reclaims_receipted_history_quarantines_at_every_owned_depth() {
        for depth in ["", "staging", "objects/aa"] {
            let data = tempfile::tempdir().unwrap();
            let history = History::open(data.path(), 1).unwrap();
            let folder = "a".repeat(64);
            history.prepare_folder(&folder).unwrap();
            if depth == "objects/aa" {
                private_dir(&history.data.join(&folder).join(depth)).unwrap();
            }
            history.refresh_usage().unwrap();
            let baseline = history.usage().unwrap().0;
            let source = if depth.is_empty() {
                "retired-root.tmp".into()
            } else if depth == "staging" {
                format!("{folder}/{depth}/{}.tmp", Uuid::new_v4())
            } else {
                format!("{folder}/{depth}/{}", "b".repeat(62))
            };
            let mut encoded = ZlibEncoder::new(Vec::new(), Compression::fast());
            encoded.write_all(b"blob 7\0retired").unwrap();
            history.root.private_write(&source, &encoded.finish().unwrap()).unwrap();
            history.refresh_usage().unwrap();
            assert!(history.usage().unwrap().0 > baseline, "the retired file was charged before the crash");
            let retained = interrupted_cleanup(&history.root, &source, true);
            let quarantine = Path::new(&retained).parent().unwrap().to_owned();
            drop(history);
            let reopened = History::open(data.path(), 2).unwrap();
            assert!(!reopened.data.join(&retained).exists());
            assert!(!reopened.data.join(&quarantine).exists());
            assert_eq!(reopened.usage().unwrap().0, baseline, "retired bytes no longer consume admission headroom");
            drop(reopened);
            assert_eq!(History::open(data.path(), 3).unwrap().usage().unwrap().0, baseline);
        }
    }

    #[cfg(unix)]
    #[test]
    fn rewind_startup_keeps_history_quarantines_without_a_matching_preservation_receipt() {
        for receipt in [false, true] {
            let data = tempfile::tempdir().unwrap();
            let history = History::open(data.path(), 1).unwrap();
            history.root.private_write("retired", b"preserved B").unwrap();
            let retained = interrupted_cleanup(&history.root, "retired", receipt);
            if receipt {
                history.root.rename_new(&retained, "previous-B").unwrap();
                history.root.private_write(&retained, b"unpreserved C").unwrap();
            }
            let expected = std::fs::read(history.data.join(&retained)).unwrap();
            let disk = history.data.clone();
            drop(history);
            for at in [2, 3] {
                assert_eq!(History::open(data.path(), at).err().unwrap(), "error.rewind.storage");
                assert_eq!(std::fs::read(disk.join(&retained)).unwrap(), expected);
            }
            if receipt {
                assert_eq!(std::fs::read(disk.join("previous-B")).unwrap(), b"preserved B");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn rewind_startup_finishes_history_cleanup_interrupted_before_source_rename() {
        let data = tempfile::tempdir().unwrap();
        let history = History::open(data.path(), 1).unwrap();
        history.root.private_write("still-live", b"the source rename never happened").unwrap();
        let held = history.root.open_file("still-live").unwrap();
        let quarantine = history.data.join(".faber-rewind-cleanup-0-4");
        private_dir(&quarantine).unwrap();
        private_dir(&quarantine.join(format!(".identity-{}", crate::hex(&file_identity(&held).unwrap())))).unwrap();
        drop(held);
        drop(history);
        let reopened = History::open(data.path(), 2).unwrap();
        assert!(!quarantine.exists());
        assert_eq!(std::fs::read(reopened.data.join("still-live")).unwrap(), b"the source rename never happened");
        drop(reopened);
        assert!(History::open(data.path(), 3).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rewind_startup_preserves_project_quarantines_through_the_journals_original_policy_path() {
        for receipt in [false, true] {
            let data = tempfile::tempdir().unwrap();
            let project = tempfile::tempdir().unwrap();
            let project_root = Root::open(project.path()).unwrap();
            let mut history = History::open(data.path(), 1).unwrap();
            let folder = "a".repeat(64);
            history.prepare_folder(&folder).unwrap();
            let id = budget_point(&mut history, &folder, 1, 4096, false);
            let point = history.points.get_mut(&id).unwrap();
            point.root = project.path().canonicalize().unwrap();
            point.root_identity = project_root.identity().unwrap();
            Arc::make_mut(&mut point.before).files.clear();
            let point = point.clone();
            history.save_point(&point).unwrap();
            budget_pin(&mut history, &id);
            let mut journal = history.journals.values().next().unwrap().clone();
            journal.root_identity = project_root.identity().unwrap();
            journal.operations.push(FileOperation {
                path: "file.txt".into(),
                before: None,
                after: None,
                applied: false,
                after_identity: None,
                temporary: None,
                displaced: None,
                installing: true,
            });
            history.save_journal(&journal).unwrap();
            let swap = format!(".faber-rewind-{}-0", journal.id);
            private_dir(&project.path().join(&swap)).unwrap();
            let source = format!("{swap}/old");
            project_root.private_write(&source, b"version from abandoned project cleanup").unwrap();
            let retained = interrupted_cleanup(&project_root, &source, receipt);
            drop(history);
            let mut reopened = History::open(data.path(), 2).unwrap();
            assert!(project.path().join(&retained).exists(), "startup discovery never changes a possibly live project");
            let mut recovering = reopened.journals[&journal.id].clone();
            reopened.recover_temporaries(&mut recovering).unwrap();
            assert!(!project.path().join(&retained).exists());
            let journal = &reopened.journals[&journal.id];
            assert!(
                journal
                    .retained
                    .iter()
                    .any(|(_, image)| { reopened.blob(&folder, image).unwrap() == b"version from abandoned project cleanup" })
            );
            drop(reopened);
            assert!(History::open(data.path(), 3).is_ok(), "startup recovery is repeatable");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rewind_startup_keeps_an_unmapped_project_quarantine_and_reports_recovery() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let project_root = Root::open(project.path()).unwrap();
        let mut history = History::open(data.path(), 1).unwrap();
        let folder = "a".repeat(64);
        history.prepare_folder(&folder).unwrap();
        let id = budget_point(&mut history, &folder, 1, 4096, false);
        let point = history.points.get_mut(&id).unwrap();
        point.root = project.path().canonicalize().unwrap();
        point.root_identity = project_root.identity().unwrap();
        Arc::make_mut(&mut point.before).files.clear();
        let point = point.clone();
        history.save_point(&point).unwrap();
        project_root.private_write("credentials.json", b"fake credentials must not be read").unwrap();
        let retained = interrupted_cleanup(&project_root, "credentials.json", true);
        std::fs::set_permissions(project.path().join(&retained), std::fs::Permissions::from_mode(0o0)).unwrap();
        drop(history);
        for at in [2, 3] {
            assert_eq!(History::open(data.path(), at).err().unwrap(), "error.rewind.recovery");
            assert!(project.path().join(&retained).exists());
            assert_eq!(std::fs::metadata(project.path().join(&retained)).unwrap().permissions().mode() & 0o777, 0);
        }
        std::fs::set_permissions(project.path().join(&retained), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read(project.path().join(retained)).unwrap(), b"fake credentials must not be read");
    }

    fn budget_history(data: &Path) -> History {
        History {
            data: data.into(),
            root: Root::open(data).unwrap(),
            points: BTreeMap::new(),
            journals: BTreeMap::new(),
            healthy: true,
            budget: RefCell::new(Some(Budget { free: TOTAL_LIMIT, ..Budget::default() })),
            prepared: RefCell::new(BTreeSet::new()),
            work: Cell::new(Work::default()),
            batch: RefCell::new(None),
            charges: RefCell::new(BTreeMap::new()),
        }
    }

    fn budget_point(history: &mut History, folder: &str, at: u64, charge: u64, active: bool) -> String {
        for suffix in ["points", "journals", "staging", "objects/pack"] {
            std::fs::create_dir_all(history.folder_dir(folder).join(suffix)).unwrap();
        }
        let id = Uuid::new_v4().to_string();
        let pack = format!("{:064x}", history.points.len() + 1);
        let saved = SavedConversation {
            messages: Vec::new(),
            piece_values: Vec::new(),
            sessions: BTreeMap::new(),
            tentative: BTreeMap::new(),
            choice: None,
            plan: None,
            context: None,
            summary: None,
            queued: Vec::new(),
        };
        let point = Point {
            info: PointInfo {
                id: id.clone(),
                message_id: Uuid::new_v4().to_string(),
                turn_id: Uuid::new_v4().to_string(),
                at,
                coverage: Coverage::default(),
                sealed: !active,
                cursor: Cursor::default(),
            },
            folder: folder.into(),
            root: history.data.clone(),
            root_identity: Vec::new(),
            conversation: Uuid::new_v4().to_string(),
            parent: None,
            sequence: at,
            before: Arc::new(Snapshot {
                files: BTreeMap::from([(
                    "file.txt".into(),
                    Image {
                        oid: "a".repeat(64),
                        bytes: 1,
                        protection: Vec::new(),
                        identity: Vec::new(),
                        pack: Some(pack.clone()),
                        offset: 12,
                        token: None,
                    },
                )]),
                ..Snapshot::default()
            }),
            after: None,
            saved,
            writers: Vec::new(),
            unknown_author: false,
            active,
        };
        let metadata = format!("{folder}/points/{id}.json");
        std::fs::write(history.data.join(&metadata), serde_json::to_vec(&point).unwrap()).unwrap();
        let payload = format!("{folder}/objects/pack/pack-{pack}.pack");
        std::fs::write(history.data.join(&payload), b"charged pack").unwrap();
        // Model large charged allocations without writing hundreds of MiB; collection deletes the real owned files.
        for (name, bytes) in [(metadata, 4096), (payload, charge)] {
            history.account(folder, 0, bytes).unwrap();
            history.charges.borrow_mut().insert(name, bytes);
        }
        history.points.insert(id.clone(), point);
        id
    }

    fn budget_pin(history: &mut History, id: &str) {
        let point = &history.points[id];
        let journal = Journal {
            id: Uuid::new_v4().to_string(),
            conversation: point.conversation.clone(),
            point: Some(id.into()),
            message_id: Some(point.info.message_id.clone()),
            mode: Some(Mode::Both),
            folder: point.folder.clone(),
            root: point.root.clone(),
            root_identity: Vec::new(),
            at: 1,
            expires: RETENTION,
            operations: Vec::new(),
            before: point.saved.clone(),
            after: point.saved.clone(),
            files_done: false,
            conversation_done: false,
            redo: false,
            retained: Vec::new(),
            partial: false,
        };
        history.journals.insert(journal.id.clone(), journal);
    }

    #[test]
    fn rewind_capture_admission_evicts_payloads_for_reservation_and_preserves_pins() {
        let data = tempfile::tempdir().unwrap();
        let mut history = budget_history(data.path());
        let folder = "a".repeat(64);
        let mib = 1024 * 1024;
        let journal_pin = budget_point(&mut history, &folder, 1, 20 * mib, false);
        budget_pin(&mut history, &journal_pin);
        let active = budget_point(&mut history, &folder, 2, 20 * mib, true);
        let oldest = budget_point(&mut history, &folder, 3, 110 * mib, false);
        let newer = budget_point(&mut history, &folder, 4, 41 * mib, false);
        assert!(history.usage().unwrap().0 < FOLDER_LIMIT - RECOVERY_RESERVE);
        history.admit(&folder, BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * mib).unwrap();
        assert!(!history.points.contains_key(&oldest));
        for id in [journal_pin, active, newer] {
            assert!(history.points.contains_key(&id), "one collected pack makes enough room; keep the remaining history");
        }
        assert!(!history.data.join(format!("{folder}/objects/pack/pack-{:064x}.pack", 3)).exists());
        assert_eq!(history.work().owned_files_deleted, 2, "retire just one point and its now-unreferenced pack");
        assert!(history.usage().unwrap().0 + BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * mib <= FOLDER_LIMIT - RECOVERY_RESERVE);
    }

    #[test]
    fn rewind_capture_admission_rejects_required_headroom_without_retiring_pins() {
        let data = tempfile::tempdir().unwrap();
        let mut history = budget_history(data.path());
        let folder = "a".repeat(64);
        let mib = 1024 * 1024;
        let journal_pin = budget_point(&mut history, &folder, 1, 95 * mib, false);
        budget_pin(&mut history, &journal_pin);
        let active = budget_point(&mut history, &folder, 2, 96 * mib, true);
        let usage = history.usage().unwrap();
        assert_eq!(history.admit(&folder, BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * mib).unwrap_err(), "error.rewind.budget");
        assert_eq!(history.usage().unwrap(), usage);
        assert!(history.points.contains_key(&journal_pin));
        assert!(history.points.contains_key(&active));
        assert_eq!(history.work().owned_files_deleted, 0);
    }

    #[test]
    fn rewind_capture_admission_reclaims_other_folders_only_for_total_headroom() {
        let data = tempfile::tempdir().unwrap();
        let mut history = budget_history(data.path());
        let mib = 1024 * 1024;
        let mut ids = Vec::new();
        for n in 0..6 {
            let folder = format!("{n:064x}");
            ids.push(budget_point(&mut history, &folder, n + 1, 159 * mib, false));
        }
        let folder = format!("{:064x}", 7);
        assert!(history.usage().unwrap().0 < TOTAL_LIMIT - RECOVERY_RESERVE);
        history.admit(&folder, BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * mib).unwrap();
        assert!(!history.points.contains_key(&ids[0]));
        assert!(ids[1..].iter().all(|id| history.points.contains_key(id)));
        assert_eq!(history.work().owned_files_deleted, 2);
        assert!(history.usage().unwrap().0 + BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * mib <= TOTAL_LIMIT - RECOVERY_RESERVE);
    }

    #[test]
    fn rewind_retired_metadata_is_unreachable_before_payload_collection() {
        let folder = tempfile::tempdir().unwrap();
        let id = "a".repeat(64);
        for suffix in ["points", "journals", "staging", "objects/pack"] {
            std::fs::create_dir_all(folder.path().join(&id).join(suffix)).unwrap();
        }
        let name = format!("{id}/points/{}.json", Uuid::new_v4());
        std::fs::write(folder.path().join(&name), b"unpublished test metadata").unwrap();
        // Ordinary test files isolate retirement ordering from owner-only sandbox restrictions.
        // The production constructor still creates and validates its private store before any write.
        let mut history = History {
            data: folder.path().into(),
            root: Root::open(folder.path()).unwrap(),
            points: BTreeMap::new(),
            journals: BTreeMap::new(),
            healthy: true,
            budget: RefCell::new(None),
            prepared: RefCell::new(BTreeSet::new()),
            work: Cell::new(Work::default()),
            batch: RefCell::new(None),
            charges: RefCell::new(BTreeMap::new()),
        };
        history.refresh_usage().unwrap();
        let retired = history.retire_metadata(&name).unwrap();
        assert!(!folder.path().join(name).exists());
        assert!(folder.path().join(&retired).exists(), "simulate a crash before deleting the retired metadata");
        history.load().unwrap();
        assert!(history.points.is_empty(), "retired metadata can never publish a point at recovery");
        history.cleanup(1).unwrap();
        assert!(!folder.path().join(retired).exists());
        assert!(history.charges.borrow().is_empty());
    }

    #[test]
    fn rewind_budget_rejects_pinned_point_overflow_and_low_disk_without_spending_reserve() {
        let mut budget = Budget { free: TOTAL_LIMIT, ..Budget::default() };
        assert!(budget.capture("folder", 1, 100).is_err(), "100 pinned points leave no capture admission");
        assert_eq!(budget.total, 0);
        budget.free = TOTAL_LIMIT - 1;
        assert_eq!(budget.capture("folder", 1, 99).unwrap_err(), "error.rewind.space");
        assert_eq!(budget.total, 0);
        budget.free = TOTAL_LIMIT;
        assert!(budget.capture("folder", 1, 99).is_ok());
        assert_eq!(budget.total, 0, "admission checks do not themselves allocate files");
    }

    #[test]
    fn rewind_budget_counts_staging_physical_allocation_and_metadata_inside_both_caps() {
        let mut budget = Budget { total: TOTAL_LIMIT - RECOVERY_RESERVE, free: TOTAL_LIMIT, ..Budget::default() };
        // Already charged bytes include physical allocation, uncompressed blobs, staging and metadata.
        budget.folders.insert("folder".into(), FOLDER_LIMIT - RECOVERY_RESERVE);
        assert!(budget.reserve("folder", 1, false).is_err());
        assert!(budget.reserve("folder", RECOVERY_RESERVE, true).is_ok());
        assert_eq!(budget.total, TOTAL_LIMIT);
        assert_eq!(budget.folders["folder"], FOLDER_LIMIT);
        assert!(budget.reserve("folder", 1, true).is_err(), "recovery does not get an extra cap");
        budget.change("folder", RECOVERY_RESERVE, 4096).unwrap();
        assert_eq!(budget.total, TOTAL_LIMIT - RECOVERY_RESERVE + 4096);
        assert!(budget.reserve("folder", RECOVERY_RESERVE - 4096 + 1, true).is_err());
    }

    #[test]
    fn rewind_budget_keeps_existing_metadata_charged_while_replacement_is_staged() {
        let mut budget = Budget { free: TOTAL_LIMIT, ..Budget::default() };
        budget.change("folder", 0, FOLDER_LIMIT - RECOVERY_RESERVE - 4096).unwrap();
        assert!(budget.reserve("folder", 8192, false).is_err(), "old and temporary metadata coexist until rename");
        budget.reserve("folder", 4096, false).unwrap();
        budget.change("folder", 8192, 1024).unwrap();
        assert_eq!(budget.folders["folder"], FOLDER_LIMIT - RECOVERY_RESERVE - 7168);
    }

    #[test]
    fn rewind_store_accepts_only_owned_names_and_keeps_recovery_reserve_inside_the_cap() {
        assert!(hex_name(&"a".repeat(64), 64));
        assert!(!hex_name(&format!("../{}", "a".repeat(61)), 64));
        assert!(!uuid_name("../outside"));
        assert_eq!(TOTAL_LIMIT - RECOVERY_RESERVE, 960 * 1024 * 1024);
        assert_eq!(FOLDER_LIMIT, 256 * 1024 * 1024);
    }
}
