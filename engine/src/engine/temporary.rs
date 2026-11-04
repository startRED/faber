//! Only explicitly temporary records may be removed by lifecycle cleanup.
use super::*;
use std::io::Write;

impl Engine {
    /// Creates a temporary Chat before its first send, without requiring a connected AI.
    pub fn start_temporary_chat(&self) -> Result<String, String> {
        self.with(|c| {
            if c.installing || c.closing {
                return Err("error.update.activeOperations".into());
            }
            let id = c.blank(Area::Chat, None);
            c.convs.get_mut(&id).unwrap().temporary = true;
            c.store.save_checked(&c.convs[&id]).map_err(|e| e.key().to_string())?;
            c.close_other_temporary(&id);
            c.shown(&id);
            Ok(id)
        })
    }

    /// A screen close is destructive only for a conversation explicitly marked temporary.
    pub fn close_chat(&self, id: &str) -> Result<(), String> {
        self.with(|c| c.close_temporary(id))
    }
}

impl Core {
    fn close_temporary(&mut self, id: &str) -> Result<(), String> {
        if self.load(id).is_some_and(|c| c.id == id && c.temporary) {
            // A record replaced outside this engine must not inherit the loaded record's deletion permission.
            if self.store.get(id).is_some_and(|c| c.id != id || !c.temporary) {
                return Err("error.chat.deleteFailed".into());
            }
            let c = self.convs.get_mut(id).unwrap();
            c.stopped = true;
            c.queued.clear();
            c.keep_going = None;
            self.remove(id)?;
        }
        Ok(())
    }

    pub(super) fn close_other_temporary(&mut self, keep: &str) {
        let ids: Vec<_> = self.convs.values().filter(|c| c.temporary && c.id != keep).map(|c| c.id.clone()).collect();
        for id in ids {
            if let Err(error) = self.close_temporary(&id) {
                (self.emit)(AppEvent::SaveFailed { key: error });
            }
        }
    }

    pub(super) fn sweep_temporary(&mut self) {
        for id in self.store.temporary_ids() {
            if let Err(error) = self.close_temporary(&id) {
                (self.emit)(AppEvent::SaveFailed { key: error });
            }
        }
        self.close_other_temporary("");
    }

    pub(super) fn save_attachments(&mut self, id: &str, root: &Path, dir: &Path, files: &[Attachment]) -> Result<Vec<PathBuf>, String> {
        if files.is_empty() {
            return Ok(vec![]);
        }
        if !self.convs.get(id).is_some_and(|c| c.temporary) {
            return Ok(save_files(root, dir, files));
        }
        let writable = |path: &Path| {
            path.strip_prefix(root).is_ok_and(|relative| crate::permissions::host_write_allowed(root, &relative.to_string_lossy()))
        };
        if !writable(dir) {
            return Err("error.projectSandbox".into());
        }
        std::fs::create_dir_all(dir).map_err(|_| "error.projectSandbox")?;
        let mut saved = Vec::new();
        for file in files {
            let bytes = base64::engine::general_purpose::STANDARD.decode(&file.base64).map_err(|_| "error.projectSandbox")?;
            let name = file.name.replace('\\', "/");
            let extension = Path::new(&name).extension().and_then(|e| e.to_str()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()));
            let mut path = dir.join(format!("{id}-{}", new_id()));
            if let Some(extension) = extension {
                path.set_extension(extension);
            }
            if !writable(&path) {
                return Err("error.projectSandbox".into());
            }
            let mut output = std::fs::File::create_new(&path).map_err(|_| "error.projectSandbox")?;
            let c = self.convs.get_mut(id).ok_or("error.conversationGone")?;
            c.temporary_attachments.push(path.to_string_lossy().into_owned());
            // An interrupted ownership save leaves no private contents in the exclusively created file.
            self.store.save_checked(c).map_err(|e| e.key().to_string())?;
            output.write_all(&bytes).map_err(|_| "error.projectSandbox")?;
            saved.push(path);
        }
        Ok(saved)
    }

    pub(super) fn remove_temporary_attachments(&mut self, id: &str) -> Result<(), String> {
        let Some(c) = self.load(id).filter(|c| c.id == id && c.temporary) else { return Ok(()) };
        for file in &c.temporary_attachments {
            let path = Path::new(file);
            if path.symlink_metadata().is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
                continue;
            }
            let Some(parent) = path.parent().and_then(crate::permissions::checked_folder) else {
                return Err("error.chat.deleteFailed".into());
            };
            let Some(name) = path.file_name().and_then(|n| n.to_str()).filter(|n| n.starts_with(&format!("{id}-"))) else {
                return Err("error.chat.deleteFailed".into());
            };
            if !crate::permissions::host_write_allowed(&parent, name) {
                return Err("error.chat.deleteFailed".into());
            }
            match crate::tools::retry_busy(|| std::fs::remove_file(parent.join(name))) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("error.chat.deleteFailed".into()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionCommand;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn temporary_chat_failed_cleanup_still_stops_the_session_and_pending_work() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        let blocked = dir.work().join(format!("{id}-blocked"));
        std::fs::create_dir(&blocked).unwrap();
        engine.with(|core| {
            let c = core.convs.get_mut(&id).unwrap();
            c.temporary = true;
            c.temporary_attachments.push(blocked.to_string_lossy().into_owned());
            core.store.save_checked(c).unwrap();
        });
        assert_eq!(engine.close_chat(&id), Err("error.chat.deleteFailed".into()));
        engine.with(|core| {
            assert!(!core.live.contains_key(&id));
            let c = &core.convs[&id];
            assert!(c.stopped);
            assert!(c.queued.is_empty());
            assert_ne!(c.messages.last().unwrap().status, Some(Status::Streaming));
            assert!(core.store.get(&id).unwrap().temporary);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn temporary_chat_compaction_never_starts_an_empty_replacement_session() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = engine.start_temporary_chat().unwrap();
        engine.with(|core| {
            let pick = Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: String::new(), pace: None };
            assert_eq!(core.launch_session(&id, &pick, kits::SessionStart::Compact), Err("error.compact.busy".into()));
            assert!(!core.live.contains_key(&id));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn temporary_chat_close_removes_owned_attachments_and_session_but_keeps_shared_files() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let closed = Arc::new(AtomicBool::new(false));
        let seen = closed.clone();
        let id = super::super::answer::tests::seed(
            &engine,
            Session::from_fn(move |cmd| {
                if matches!(cmd, SessionCommand::Close) {
                    seen.store(true, Ordering::SeqCst);
                }
            }),
            &[("reply", &[])],
        );
        let shared = dir.work().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let attached = shared.join("attachments");
        let files = [Attachment { name: "private.txt".into(), base64: "cHJpdmF0ZQ==".into() }];
        let owned = engine.with(|core| {
            let ordinary = core.blank(Area::Chat, None);
            core.convs.get_mut(&ordinary).unwrap().working_folder = Some(shared.to_string_lossy().into_owned());
            let other = core.save_attachments(&ordinary, &shared, &attached, &files).unwrap();
            core.save(&ordinary);
            let c = core.convs.get_mut(&id).unwrap();
            c.working_folder = Some(shared.to_string_lossy().into_owned());
            c.temporary = true;
            let owned = core.save_attachments(&id, &shared, &attached, &files).unwrap();
            let disk = core.store.get(&id).unwrap();
            assert_eq!(disk.temporary_attachments, [owned[0].to_string_lossy()]);
            assert_ne!(owned, other);
            assert_eq!(std::fs::read_to_string(&owned[0]).unwrap(), "private");
            owned
        });
        engine.close_chat(&id).unwrap();
        assert!(closed.load(Ordering::SeqCst));
        assert!(!owned[0].exists());
        assert_eq!(std::fs::read_to_string(attached.join("private.txt")).unwrap(), "private");
        assert!(attached.is_dir());
        engine.with(|c| assert!(!c.live.contains_key(&id)));
        engine.shutdown();
    }

    #[tokio::test]
    async fn temporary_chat_crash_sweep_cleans_only_owned_attachments() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let shared = dir.work().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let attached = shared.join("attachments");
        let id = engine.start_temporary_chat().unwrap();
        let owned = engine.with(|core| {
            let files = [Attachment { name: "private.pdf".into(), base64: "cHJpdmF0ZQ==".into() }];
            core.save_attachments(&id, &shared, &attached, &files).unwrap()
        });
        std::fs::write(attached.join("ordinary.pdf"), "ordinary attachment").unwrap();
        let leftover = std::fs::read(dir.path().join("conversations").join(format!("{id}.json"))).unwrap();
        engine.shutdown();
        drop(engine);
        // Recreate the durable state a crash would leave, without letting shutdown clean it.
        std::fs::write(&owned[0], "private").unwrap();
        std::fs::write(dir.path().join("conversations").join(format!("{id}.json")), leftover).unwrap();
        let (restarted, _) = super::super::answer::tests::engine(&dir);
        assert!(!owned[0].exists());
        assert!(!dir.path().join("conversations").join(format!("{id}.json")).exists());
        assert_eq!(std::fs::read_to_string(attached.join("ordinary.pdf")).unwrap(), "ordinary attachment");
        assert!(attached.is_dir());
        restarted.shutdown();
    }

    #[tokio::test]
    async fn temporary_chat_never_recursively_deletes_a_shared_scratch_folder() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = engine.start_temporary_chat().unwrap();
        let (scratch, ordinary) = engine.with(|core| {
            let scratch = core.scratch_of(&id);
            std::fs::create_dir_all(scratch.join("attachments")).unwrap();
            std::fs::write(scratch.join("attachments/ordinary.txt"), "keep").unwrap();
            let ordinary = core.blank(Area::Chat, None);
            core.convs.get_mut(&ordinary).unwrap().working_folder = Some(scratch.to_string_lossy().into_owned());
            core.store.save_checked(&core.convs[&ordinary]).unwrap();
            (scratch, ordinary)
        });
        engine.close_chat(&id).unwrap();
        engine.shutdown();
        drop(engine);
        let (restarted, _) = super::super::answer::tests::engine(&dir);
        assert!(restarted.open(&ordinary).is_some());
        assert_eq!(std::fs::read_to_string(scratch.join("attachments/ordinary.txt")).unwrap(), "keep");
        restarted.shutdown();
    }
}
