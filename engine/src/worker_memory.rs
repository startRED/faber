//! Read-only memory samples for the CLI trees Faber owns.
use crate::proc::OwnedTree;
#[cfg(any(windows, target_os = "macos", test))]
use crate::proc::ProcessIdentity;
#[cfg(any(windows, target_os = "macos", test))]
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Windows private working set or macOS resident bytes (shared pages may count per process).
    pub used: u64,
    pub total: u64,
    pub available: u64,
}

/// Read-only ownership retained independently of conversation/thread lifetime.
#[derive(Clone, Default)]
pub struct Registry {
    trees: Arc<Mutex<Vec<OwnedTree>>>,
}

impl Registry {
    pub fn register(&self, tree: Option<OwnedTree>) {
        let Some(tree) = tree else { return };
        let mut trees = self.trees.lock().unwrap_or_else(|error| error.into_inner());
        trees.retain(|tree| !tree.is_retired());
        if !trees.iter().any(|existing| existing.same_tree(&tree)) {
            trees.push(tree);
        }
    }

    /// One system sample, joined only to exact OS identities from owned containers.
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        #[cfg(windows)]
        {
            self.snapshot_with(|_| {
                let processes = faber_winproc::processes()?
                    .into_iter()
                    .map(|process| (ProcessIdentity { pid: process.pid, created: process.created }, process.private_bytes))
                    .collect();
                let mut system = sysinfo::System::new();
                system.refresh_memory();
                Ok((processes, system.total_memory(), system.available_memory()))
            })
        }
        #[cfg(target_os = "macos")]
        {
            self.snapshot_with(|members| {
                let unique: HashSet<_> = members.iter().copied().collect();
                let processes = unique
                    .into_iter()
                    .map(|identity| faber_winproc::process_tree::resident_bytes(identity).map(|bytes| (identity, bytes)))
                    .collect::<io::Result<Vec<_>>>()?;
                let mut system = sysinfo::System::new();
                system.refresh_memory();
                Ok((processes, system.total_memory(), system.available_memory()))
            })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err(io::Error::new(io::ErrorKind::Unsupported, "worker memory sampling needs exact native process identities"))
        }
    }

    #[cfg(any(windows, target_os = "macos", test))]
    fn snapshot_with(&self, read: impl FnOnce(&[ProcessIdentity]) -> io::Result<Reading>) -> io::Result<Snapshot> {
        let trees = {
            let mut trees = self.trees.lock().unwrap_or_else(|error| error.into_inner());
            trees.retain(|tree| !tree.is_retired());
            trees.clone()
        };
        let mut members = Vec::new();
        for tree in &trees {
            let sample = tree.members()?;
            if sample.is_empty() {
                if !tree.confirm_empty()? {
                    return Err(io::Error::other("owned container changed during memory sampling"));
                }
            } else {
                members.extend(sample);
            }
        }
        self.trees.lock().unwrap_or_else(|error| error.into_inner()).retain(|tree| !tree.is_retired());
        let (processes, total, available) = read(&members)?;
        sample_members(&members, &processes, total, available)
    }
}

#[cfg(any(windows, target_os = "macos", test))]
type Reading = (Vec<(ProcessIdentity, u64)>, u64, u64);

#[cfg(any(windows, target_os = "macos", test))]
fn sample_members(members: &[ProcessIdentity], processes: &[(ProcessIdentity, u64)], total: u64, available: u64) -> io::Result<Snapshot> {
    if total == 0 || available > total {
        return Err(io::Error::other("system memory reading is unavailable"));
    }
    let processes: HashMap<_, _> = processes.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut used = 0u64;
    for member in members {
        if seen.insert(*member) {
            let bytes = processes.get(member).ok_or_else(|| io::Error::other("owned process memory reading is unavailable"))?;
            used = used.checked_add(*bytes).ok_or_else(|| io::Error::other("owned process memory total overflowed"))?;
        }
    }
    Ok(Snapshot { used, total, available })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(pid: u32, created: u64) -> ProcessIdentity {
        ProcessIdentity { pid, created }
    }

    #[test]
    fn worker_memory_counts_each_real_generation_once_including_root() {
        let root = identity(1, 10);
        let child = identity(2, 20);
        let processes = [(root, 100), (child, 40), (identity(9, 90), 900)];
        let sample = sample_members(&[root, child, root, child], &processes, 1000, 500).unwrap();
        assert_eq!(sample, Snapshot { used: 140, total: 1000, available: 500 });
    }

    #[test]
    fn worker_memory_refuses_reused_or_missing_processes_and_overflow() {
        let root = identity(1, 10);
        assert!(sample_members(&[root], &[(identity(1, 11), 90)], 1000, 500).is_err());
        assert!(sample_members(&[root], &[], 1000, 500).is_err());
        assert!(sample_members(&[root, identity(2, 20)], &[(root, u64::MAX), (identity(2, 20), 1)], u64::MAX, 1).is_err());
        assert!(sample_members(&[], &[], 0, 0).is_err());
        assert!(sample_members(&[], &[], 100, 101).is_err());
    }

    #[test]
    fn worker_memory_retains_the_owned_root_after_sessions_go_and_never_stops_it() {
        use crate::proc::Containment;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = identity(42, 10);
        let stops = Arc::new(AtomicUsize::new(0));
        let record = stops.clone();
        let container = Containment::scripted_descendants(
            root,
            move || Ok(vec![root]),
            move || {
                record.fetch_add(1, Ordering::Relaxed);
            },
        );
        let tree = container.owned_tree().unwrap();
        let registry = Registry::default();
        registry.register(Some(tree.clone()));
        registry.register(Some(tree.clone()));
        assert_eq!(registry.trees.lock().unwrap().len(), 1, "shared sessions register one tree");
        let session = crate::events::Session::from_fn(|_| {}).with_tree(Some(tree));
        assert!(session.owned_tree().is_some());
        drop(session);
        assert_eq!(registry.clone().snapshot_with(|_| Ok((vec![(root, 99)], 1000, 500))).unwrap().used, 99);
        assert_eq!(stops.load(Ordering::Relaxed), 0, "sampling never stops an owned process");
        drop(container);
        let reading = registry.clone().snapshot_with(|_| Ok((vec![(root, 99)], 1000, 500))).unwrap();
        assert_eq!(reading.used, 99, "root stays counted without a conversation");
        assert_eq!(stops.load(Ordering::Relaxed), 1, "retaining samples never prevents the driver's drop cleanup");
    }

    #[tokio::test]
    async fn worker_memory_releases_only_confirmed_retirement_and_propagates_unknown() {
        use crate::proc::Containment;
        let root = identity(42, 10);
        let members = Arc::new(Mutex::new(vec![root]));
        let query = members.clone();
        let container = Containment::scripted_descendants(root, move || Ok(query.lock().unwrap().clone()), || {});
        let tree = container.owned_tree().unwrap();
        let registry = Registry::default();
        registry.register(Some(tree.clone()));
        assert!(registry.snapshot_with(|_| Err(io::Error::other("process sample failed"))).is_err());
        assert_eq!(registry.trees.lock().unwrap().len(), 1);
        members.lock().unwrap().clear();
        assert_eq!(registry.snapshot_with(|_| Ok((vec![], 1000, 500))).unwrap().used, 0);
        assert!(registry.trees.lock().unwrap().is_empty(), "an independent empty-container query confirms retirement");
        tree.retire().await.unwrap();
        registry.snapshot_with(|_| Ok((vec![], 1000, 500))).unwrap();
        assert!(registry.trees.lock().unwrap().is_empty());

        let failing = Containment::scripted_descendants(root, || Err(io::Error::other("container sample failed")), || {});
        registry.register(failing.owned_tree());
        assert!(registry.snapshot_with(|_| Ok((vec![], 1000, 500))).is_err());
    }

    #[test]
    fn worker_memory_queries_only_exact_owned_members_and_propagates_memory_failure() {
        use crate::proc::Containment;
        let root = identity(42, 10);
        let child = identity(43, 11);
        let container = Containment::scripted_descendants(root, move || Ok(vec![root, child]), || {});
        let registry = Registry::default();
        registry.register(container.owned_tree());
        let sample = registry
            .snapshot_with(|members| {
                assert_eq!(members, &[root, child]);
                Ok((members.iter().map(|member| (*member, 123)).collect(), 1000, 500))
            })
            .unwrap();
        assert_eq!(sample.used, 246);
        assert!(
            registry
                .snapshot_with(|members| {
                    assert_eq!(members, &[root, child]);
                    Err(io::Error::other("identity changed around the resident-memory query"))
                })
                .is_err()
        );
    }

    #[test]
    fn worker_memory_confirms_an_empty_container_after_driver_drop_without_signalling_again() {
        use crate::proc::Containment;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = identity(42, 10);
        let members = Arc::new(Mutex::new(vec![root]));
        let query = members.clone();
        let stops = Arc::new(AtomicUsize::new(0));
        let record = stops.clone();
        let container = Containment::scripted_descendants(
            root,
            move || Ok(query.lock().unwrap().clone()),
            move || {
                record.fetch_add(1, Ordering::Relaxed);
            },
        );
        let registry = Registry::default();
        registry.register(container.owned_tree());
        drop(container);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
        members.lock().unwrap().clear();
        assert_eq!(registry.snapshot_with(|_| Ok((vec![], 1000, 500))).unwrap().used, 0);
        assert!(registry.trees.lock().unwrap().is_empty(), "a confirmed empty owned container no longer needs a retained handle");
        assert_eq!(stops.load(Ordering::Relaxed), 1, "empty confirmation never terminates a process");
    }
}
