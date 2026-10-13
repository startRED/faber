//! R106: owned operational data is retired when its last consumer releases it. Safety records are never targets.
use super::{
    Extra, ProcessId, Record, SCHEMA,
    lease::{Termination, real_path},
    store::Journal,
    valid_id,
};
use crate::store::Fault;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct DiskBudget {
    pub limit_bytes: u64,
    pub reserve_bytes: u64,
}
impl DiskBudget {
    pub fn admit(&self, free: Option<u64>, used: u64, build: u64) -> Result<(), &'static str> {
        let free = free.ok_or("reason.disk.unknown")?;
        if used.checked_add(build).is_none_or(|n| n > self.limit_bytes) || build.checked_add(self.reserve_bytes).is_none_or(|n| n > free) {
            return Err("reason.disk.low");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceKind {
    Build,
    Temp,
    Log,
}
impl ResourceKind {
    fn folder(self) -> &'static str {
        match self {
            Self::Build => "builds",
            Self::Temp => "temp",
            Self::Log => "logs",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub kind: ResourceKind,
    pub task: String,
    pub process: Option<ProcessId>,
    needed: bool,
    #[serde(flatten)]
    extra: Extra,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    schema: u32,
    entries: Vec<Resource>,
    #[serde(flatten)]
    extra: Extra,
}
impl Record for Resources {
    fn schema(&self) -> u32 {
        self.schema
    }
}
impl Resources {
    pub fn load(j: &Journal) -> Result<Self, &'static str> {
        let value: Self = j.get(&j.private().join("resources.json")).map_err(|e| e.key())?.unwrap_or(Self {
            schema: SCHEMA,
            entries: vec![],
            extra: Extra::new(),
        });
        if value.schema != SCHEMA || value.entries.len() > 4096 || value.entries.iter().any(|r| !valid_id(&r.id)) {
            return Err("error.store.corrupt");
        }
        Ok(value)
    }
    fn save(&self, j: &Journal) -> Result<(), &'static str> {
        j.put(&j.private().join("resources.json"), self).map_err(|e| e.key())
    }
    fn path(j: &Journal, r: &Resource) -> Result<PathBuf, &'static str> {
        if !valid_id(&r.id) {
            return Err("error.wire.path");
        }
        let root = real_path(j.private()).map_err(|e| e.key())?.canonicalize().map_err(|_| "error.store.read")?;
        let path = real_path(&root.join("resources").join(r.kind.folder()).join(&r.id)).map_err(|e| e.key())?;
        if !path.starts_with(&root) || path == root {
            return Err("error.wire.path");
        }
        Ok(path)
    }
    pub fn register(&mut self, j: &Journal, id: &str, kind: ResourceKind, process: Option<ProcessId>) -> Result<PathBuf, &'static str> {
        self.register_for(j, id, id, kind, process)
    }
    pub fn register_for(
        &mut self,
        j: &Journal,
        id: &str,
        task: &str,
        kind: ResourceKind,
        process: Option<ProcessId>,
    ) -> Result<PathBuf, &'static str> {
        if !valid_id(task) || !valid_id(id) || self.entries.len() >= 4096 || self.entries.iter().any(|r| r.id == id) {
            return Err("reason.disk.resource");
        }
        let resource = Resource { id: id.into(), kind, task: task.into(), process, needed: true, extra: Extra::new() };
        let path = Self::path(j, &resource)?;
        if path.exists() {
            return Err("reason.disk.resource");
        }
        let mut next = self.clone();
        next.entries.push(resource);
        next.save(j)?;
        *self = next;
        Ok(path)
    }
    pub fn unneeded(&mut self, j: &Journal, id: &str) -> Result<(), &'static str> {
        let mut next = self.clone();
        next.entries.iter_mut().find(|r| r.id == id).ok_or("reason.disk.resource")?.needed = false;
        next.save(j)?;
        *self = next;
        Ok(())
    }
    pub fn clean(&mut self, j: &Journal, id: &str, proof: Termination) -> Result<(), &'static str> {
        let r = self.entries.iter().find(|r| r.id == id).ok_or("reason.disk.resource")?;
        if r.needed
            || !proof.tree_dead
            || !proof.cost_settled
            || r.process != proof.process
            || !j.pending().map_err(|e| e.key())?.is_empty()
        {
            return Err("reason.disk.needed");
        }
        let path = Self::path(j, r)?;
        // Re-check each entry without following links. A stopped resource has no worker left to replace its paths.
        remove_owned(&path)?;
        let mut next = self.clone();
        next.entries.retain(|r| r.id != id);
        next.save(j)?;
        *self = next;
        Ok(())
    }
    pub fn entries(&self) -> &[Resource] {
        &self.entries
    }
    pub fn used_bytes(&self, j: &Journal) -> Result<u64, &'static str> {
        let mut used = self.entries.iter().try_fold(0_u64, |n, r| n.checked_add(size(&Self::path(j, r)?)?).ok_or("reason.disk.low"))?;
        let private = real_path(j.private()).map_err(|e| e.key())?.canonicalize().map_err(|_| "error.store.read")?;
        for lease in j.runtime().map_err(|e| e.key())?.leases {
            if let Some(value) = lease.extra.get("gitRoot") {
                let root: PathBuf = serde_json::from_value(value.clone()).map_err(|_| "error.store.corrupt")?;
                let root = real_path(&root).map_err(|e| e.key())?;
                if !root.starts_with(&private) || root == private {
                    return Err("error.wire.path");
                }
                used = used.checked_add(size(&root)?).ok_or("reason.disk.low")?;
            }
        }
        Ok(used)
    }
}
fn size(path: &Path) -> Result<u64, &'static str> {
    real_path(path).map_err(|e| e.key())?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err("error.store.read"),
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    fs::read_dir(path).map_err(|_| "error.store.read")?.try_fold(0_u64, |n, e| {
        let e = e.map_err(|_| "error.store.read")?;
        n.checked_add(size(&e.path())?).ok_or("reason.disk.low")
    })
}
fn remove_owned(path: &Path) -> Result<(), &'static str> {
    real_path(path).map_err(|e| e.key())?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("error.store.read"),
    };
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| "error.store.read")? {
            remove_owned(&entry.map_err(|_| "error.store.read")?.path())?;
        }
        real_path(path).map_err(|e| e.key())?;
        fs::remove_dir(path).map_err(|e| Fault::of_write(&e).key())
    } else {
        fs::remove_file(path).map_err(|e| Fault::of_write(&e).key())
    }
}

/// One native reading at admission, not a polling process or a filesystem scan outside owned data.
pub fn available_bytes(root: &Path) -> Option<u64> {
    let root = real_path(root).ok()?.canonicalize().ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount = disk.mount_point().canonicalize().ok()?;
            root.starts_with(&mount).then_some((mount.components().count(), disk.available_space()))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, bytes)| bytes)
}
