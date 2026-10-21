//! Moving a Chat record into a project. Its next turn starts a project session with the visible history.
use super::*;

impl Engine {
    pub fn move_to_project(&self, id: &str, project_id: &str) -> Result<Conversation, String> {
        self.with(|core| {
            let mut moved = core.load(id).ok_or("error.conversationGone")?.clone();
            if moved.area != Area::Chat || moved.archived_at.is_some() {
                return Err("error.moveChat.unavailable".into());
            }
            if core.rewind_blocked(id) {
                return Err("error.rewind.busy".into());
            }
            if running_item(&moved, core.live.get(id)).is_some()
                || core.transfers.get(id).is_some_and(|t| core.pending_turn(id, &t.reply).is_ok())
                || !moved.queued.is_empty()
                || moved.keep_going.is_some()
                || moved.messages.iter().any(|m| m.status == Some(Status::Streaming))
                || core.live.get(id).is_some_and(|l| l.turn.is_some())
                || core.rewind_writers_active(id)
                || core.asides.get(id).is_some_and(|rows| rows.iter().any(|a| a.state == "running"))
            {
                return Err("error.moveChat.busy".into());
            }
            let project =
                core.store.projects_checked().map_err(|e| e.key())?.into_iter().find(|p| p.id == project_id).ok_or("error.projectGone")?;
            crate::permissions::checked_folder(Path::new(&project.path)).ok_or("error.workingFolder")?;
            // Copy before the record changes; keep the originals so a failed write or a crash loses nothing.
            core.copy_project_attachments(&moved, id, &moved.messages)?;
            moved.area = Area::Project;
            moved.project_id = Some(project.id);
            // Keep the old folder for file cards; project sessions resolve their folder independently.
            moved.project_start_pending = true;
            moved.sessions.clear();
            moved.tentative.clear();
            moved.cost_base.clear();
            moved.commands.clear();
            moved.context = None;
            moved.reported = (None, None);
            moved.command_gate = false;
            core.store.move_chat_to_project(&moved).map_err(|e| e.key())?;
            if let Some(live) = core.retire(id) {
                live.session.close();
            }
            core.asides.remove(id);
            core.convs.insert(id.to_string(), moved.clone());
            core.list_changed();
            Ok(moved)
        })
    }
}

impl Core {
    /// Copy referenced attachments before a move or branch publishes their new project locations.
    pub(super) fn copy_project_attachments(&self, source: &Conversation, id: &str, messages: &[Message]) -> Result<(), String> {
        let data = PathBuf::from(crate::project::git::native_path(&self.opts.data_dir));
        let root = if source.area == Area::Chat {
            PathBuf::from(crate::project::git::native_path(&self.workspace_of(&source.id)))
        } else {
            data.clone()
        };
        let target = data.join("attachments").join(id);
        for name in messages.iter().flat_map(|m| m.attachments.iter().flatten()).collect::<HashSet<_>>() {
            let relative =
                if source.area == Area::Chat { format!("attachments/{name}") } else { format!("attachments/{}/{name}", source.id) };
            let from = inside_folder(&root, &relative).ok_or("error.projectSandbox")?;
            let relative = format!("attachments/{id}/{name}");
            if !crate::permissions::allows(&data, "Write", &serde_json::json!({"file_path": relative})) {
                return Err("error.projectSandbox".into());
            }
            std::fs::create_dir_all(&target).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            let mut file = tempfile::NamedTempFile::new_in(&target).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            let mut input = std::fs::File::open(from).map_err(|_| "error.store.read")?;
            std::io::copy(&mut input, &mut file).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            file.as_file().sync_all().map_err(|e| crate::store::Fault::of_write(&e).key())?;
            file.persist(data.join(relative)).map_err(|e| crate::store::Fault::of_write(&e.error).key())?;
        }
        Ok(())
    }
}
