use super::Engine;
use crate::storage::{self, StorageUsage};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn protect_roots(roots: BTreeSet<PathBuf>) -> Vec<Arc<PathBuf>> {
    roots.into_iter().filter(|root| storage::external_root_name(root)).map(crate::store::Store::protect_external_storage).collect()
}

impl Engine {
    pub fn owns_storage_profile(&self) -> bool {
        self.with(|core| core.store.owns_budget())
    }

    pub fn set_new_storage(&self, root: Option<PathBuf>, roots: BTreeSet<PathBuf>) {
        self.with(|core| {
            core.new_storage = root;
            core.storage_roots = protect_roots(roots);
        });
    }

    /// The catalog items in a live session's kit. A runtime reaches a session by its path in the tool store, not as a copy
    /// in the kit, so the originals of a moved item stay while a session that may have started before the move lives.
    pub fn catalog_ids_in_live_sessions(&self) -> BTreeSet<String> {
        self.with(|core| core.live.values().flat_map(|live| live.tool_kit.ids.iter().cloned()).collect())
    }

    pub fn tool_store(&self) -> crate::tools::Store {
        self.with(|core| core.catalog_store())
    }

    pub async fn storage_usage(&self) -> Result<StorageUsage, String> {
        static SCAN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let scan = SCAN.lock().await;
        let (result, ready) = tokio::sync::oneshot::channel();
        let data = self.with(|core| core.opts.data_dir.clone());
        let engine = self.clone();
        std::thread::Builder::new()
            .name("storage-usage".into())
            .spawn(move || {
                let _scan = scan;
                // Background priority stays on this worker, away from the reusable Tokio pool.
                #[cfg(any(windows, target_os = "macos"))]
                let _ = faber_winproc::background_thread();
                let usage = storage::measure(&data, |id| {
                    engine.with(|core| match core.convs.get(id) {
                        Some(conversation) => conversation.project_id.clone(),
                        None => core.store.cached_conversation_project(id),
                    })
                });
                let _ = result.send(usage);
            })
            .map_err(|_| "error.store.read")?;
        ready.await.map_err(|_| "error.store.read".into())
    }
}
