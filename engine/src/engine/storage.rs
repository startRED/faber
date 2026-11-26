use super::Engine;
use crate::storage::{self, StorageUsage};

impl Engine {
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
