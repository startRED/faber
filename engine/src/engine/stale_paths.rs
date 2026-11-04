//! Close-time file references: disk reads stay on a blocking worker, with fresh admission before edits.
use super::*;
use crate::project::memory;

// Keep changes consent without changing a note's revision; serialize it with automatic edits outside Core's lock.
pub(super) static EDITS: Mutex<()> = Mutex::new(());

impl Core {
    pub(super) fn check_stale_paths(&self, conv: &str, reply: &str) {
        let Some(c) = self.convs.get(conv).filter(|c| c.area == Area::Project) else { return };
        let Some(project_id) = c.project_id.as_deref() else { return };
        let read_project = self.store.project_reader(project_id);
        let read_kept = self.store.kept_decisions_checked_reader(project_id);
        let (me, conv, reply) = (self.me.clone(), conv.to_string(), reply.to_string());
        self.opts.runtime.spawn_blocking(move || {
            let Ok(_edit) = EDITS.lock() else { return };
            let Some(shared) = me.upgrade() else { return };
            let Some(project) = read_project() else { return };
            let admitted = || {
                if read_project().is_none_or(|p| p.path != project.path) {
                    return false;
                }
                let core = shared.lock().unwrap_or_else(|e| e.into_inner());
                !core.closing
                    && !core.installing
                    && core.convs.get(&conv).is_some_and(|c| {
                        !c.stopped
                            && c.area == Area::Project
                            && c.project_id.as_deref() == Some(&project.id)
                            && c.messages.last().is_some_and(|m| m.id == reply && m.wrap_up.is_some())
                    })
            };
            if !admitted() {
                return;
            }
            let Ok(kept) = read_kept() else { return };
            let Ok(updated) =
                memory::refresh_stale(Path::new(&project.path), &kept, || admitted() && read_kept().is_ok_and(|now| now == kept))
            else {
                return;
            };
            if updated != kept && admitted() {
                let core = shared.lock().unwrap_or_else(|e| e.into_inner());
                let _ = core.store.migrate_kept_decisions(&project.id, &kept, &updated);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stale_paths_run_when_the_closing_box_is_written() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        let folder = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
        std::fs::create_dir(folder.path().join(".faber")).unwrap();
        let note = folder.path().join(".faber/decisions.md");
        std::fs::write(&note, "- Use `src/gone.rs`. [by AI]\n").unwrap();
        engine.with(|core| {
            let c = core.convs.get_mut(&id).unwrap();
            c.area = Area::Project;
            c.project_id = Some(project.id);
            c.started_by = Some("lead".into());
            core.write_progress(
                &id,
                crate::progress::WRAP_UP_TOOL,
                &serde_json::json!({
                    "outcome":"done", "did":"Finished", "result":"Ready", "verified":"Checks passed"
                }),
            )
            .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(10), async {
            while !std::fs::read_to_string(&note).unwrap().contains(memory::STALE) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        engine.shutdown();
    }
}
