//! Chat folders own plain instructions and bounded, local copies of their knowledge files.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_BYTES: usize = 5_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGroup {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub files: Vec<GroupFile>,
    pub conversation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupFile {
    pub id: String,
    pub name: String,
    pub size: usize,
    pub mime: String,
}

fn name(raw: &str) -> Result<String, String> {
    let clean: String = raw.chars().filter(|c| !c.is_control()).collect();
    let clean = clean.trim();
    if clean.is_empty() || clean.chars().count() > 120 {
        return Err("error.groupName".into());
    }
    Ok(clean.into())
}

fn mime(name: &str, bytes: &[u8]) -> Result<String, String> {
    let extension = Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    Ok(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" | "avif" => return Err("error.groupFileType".into()),
        _ if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) => "text/plain",
        _ => return Err("error.groupFileType".into()),
    }
    .into())
}

impl Engine {
    pub fn create_chat_group(&self, raw: &str) -> Result<ChatGroup, String> {
        let name = name(raw)?;
        self.with(|c| {
            let mut groups = c.store.chat_groups().map_err(|e| e.key())?;
            let group = ChatGroup { id: new_id(), name, instructions: String::new(), files: vec![], conversation_ids: vec![] };
            groups.push(group.clone());
            c.store.save_chat_groups(&groups).map_err(|e| e.key())?;
            c.list_changed();
            Ok(group)
        })
    }

    pub fn save_chat_group(&self, id: &str, raw: Option<&str>, instructions: Option<&str>) -> Result<ChatGroup, String> {
        let name = raw.map(name).transpose()?;
        if instructions.is_some_and(|text| text.chars().count() > 8000) {
            return Err("error.groupInstructions".into());
        }
        self.with(|c| {
            let mut groups = c.store.chat_groups().map_err(|e| e.key())?;
            let group = groups.iter_mut().find(|g| g.id == id).ok_or("error.groupGone")?;
            if let Some(name) = name {
                group.name = name;
            }
            if let Some(instructions) = instructions {
                group.instructions = instructions.into();
            }
            let result = group.clone();
            c.store.save_chat_groups(&groups).map_err(|e| e.key())?;
            c.list_changed();
            Ok(result)
        })
    }

    pub fn add_chat_group_files(&self, id: &str, files: &[Attachment]) -> Result<ChatGroup, String> {
        self.with(|c| {
            let mut groups = c.store.chat_groups().map_err(|e| e.key())?;
            let group = groups.iter_mut().find(|g| g.id == id).ok_or("error.groupGone")?;
            if group.files.len() + files.len() > 10 {
                return Err("error.groupFiles".into());
            }
            let mut total: usize = group.files.iter().map(|f| f.size).sum();
            let mut incoming = vec![];
            for file in files {
                if file.base64.len() > MAX_BYTES.div_ceil(3) * 4 {
                    return Err("error.groupFiles".into());
                }
                let bytes = base64::engine::general_purpose::STANDARD.decode(&file.base64).map_err(|_| "error.groupFileType")?;
                total += bytes.len();
                if total > MAX_BYTES {
                    return Err("error.groupFiles".into());
                }
                let name = file.name.replace('\\', "/");
                let name: String = name.rsplit('/').next().unwrap_or("file").chars().filter(|c| !c.is_control()).take(240).collect();
                if name.is_empty() {
                    return Err("error.groupFileType".into());
                }
                incoming.push((GroupFile { id: new_id(), mime: mime(&name, &bytes)?, name, size: bytes.len() }, bytes));
            }
            let dir = c.store.chat_groups_dir().join(&group.id);
            std::fs::create_dir_all(&dir).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            let mut written = vec![];
            let save = (|| {
                for (file, bytes) in &incoming {
                    let path = dir.join(&file.id);
                    let mut output = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|e| crate::store::Fault::of_write(&e).key())?;
                    written.push(path);
                    std::io::Write::write_all(&mut output, bytes).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                    output.sync_all().map_err(|e| crate::store::Fault::of_write(&e).key())?;
                }
                Ok(())
            })();
            group.files.extend(incoming.into_iter().map(|(file, _)| file));
            let save = save.and_then(|()| c.store.save_chat_groups(&groups).map_err(|e| e.key()));
            if let Err(error) = save {
                for path in written {
                    let _ = std::fs::remove_file(path);
                }
                return Err(error.into());
            }
            let result = groups.into_iter().find(|g| g.id == id).expect("group remains");
            c.list_changed();
            Ok(result)
        })
    }

    pub fn remove_chat_group_file(&self, id: &str, file_id: &str) -> Result<ChatGroup, String> {
        self.with(|c| {
            let mut groups = c.store.chat_groups().map_err(|e| e.key())?;
            let group = groups.iter_mut().find(|g| g.id == id).ok_or("error.groupGone")?;
            if Uuid::parse_str(file_id).is_err() {
                return Err("error.groupGone".into());
            }
            let path = c.store.chat_groups_dir().join(id).join(file_id);
            let staged = path.with_extension("removing");
            if !group.files.iter().any(|f| f.id == file_id) {
                if staged.exists() {
                    std::fs::remove_file(staged).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                    return Ok(group.clone());
                }
                return Err("error.groupGone".into());
            }
            let moved = stage_removal(&path, &staged)?;
            group.files.retain(|f| f.id != file_id);
            let result = group.clone();
            if let Err(error) = c.store.save_chat_groups(&groups) {
                if moved {
                    std::fs::rename(staged, path).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                }
                return Err(error.key().into());
            }
            if moved {
                std::fs::remove_file(staged).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            }
            c.list_changed();
            Ok(result)
        })
    }

    pub fn remove_chat_group(&self, id: &str) -> Result<(), String> {
        self.with(|c| {
            let mut groups = c.store.chat_groups().map_err(|e| e.key())?;
            if Uuid::parse_str(id).is_err() {
                return Err("error.groupGone".into());
            }
            let dir = c.store.chat_groups_dir().join(id);
            let staged = dir.with_extension("removing");
            if !groups.iter().any(|g| g.id == id) {
                if staged.exists() {
                    std::fs::remove_dir_all(staged).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                    c.list_changed();
                    return Ok(());
                }
                return Err("error.groupGone".into());
            }
            let moved = stage_removal(&dir, &staged)?;
            groups.retain(|g| g.id != id);
            if let Err(error) = c.store.save_chat_groups(&groups) {
                if moved {
                    std::fs::rename(staged, dir).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                }
                return Err(error.key().into());
            }
            if moved {
                std::fs::remove_dir_all(staged).map_err(|e| crate::store::Fault::of_write(&e).key())?;
            }
            c.list_changed();
            Ok(())
        })
    }

    pub fn move_chat_to_group(&self, conversation_id: &str, group_id: Option<&str>) -> Result<(), String> {
        self.with(|c| c.move_chat_to_group(conversation_id, group_id))
    }

    pub fn new_group_chat(&self, group_id: &str) -> Result<Conversation, String> {
        self.with(|c| {
            let id = c.blank(Area::Chat, None);
            if let Err(error) = c.store.save_checked(&c.convs[&id]) {
                c.convs.remove(&id);
                return Err(error.key().into());
            }
            if let Err(error) = c.move_chat_to_group(&id, Some(group_id)) {
                c.convs.remove(&id);
                c.store.remove(&id);
                return Err(error);
            }
            Ok(c.convs[&id].clone())
        })
    }
}

fn stage_removal(path: &Path, staged: &Path) -> Result<bool, String> {
    match std::fs::rename(path, staged) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::store::Fault::of_write(&error).key().into()),
    }
}

impl Core {
    pub(super) fn move_chat_to_group(&mut self, conversation_id: &str, group_id: Option<&str>) -> Result<(), String> {
        if self.load(conversation_id).is_none_or(|c| c.area != Area::Chat) {
            return Err("error.conversationGone".into());
        }
        let mut groups = self.store.chat_groups().map_err(|e| e.key())?;
        if group_id.is_some_and(|id| !groups.iter().any(|g| g.id == id)) {
            return Err("error.groupGone".into());
        }
        for group in &mut groups {
            group.conversation_ids.retain(|id| id != conversation_id);
            if Some(group.id.as_str()) == group_id {
                group.conversation_ids.push(conversation_id.into());
            }
        }
        self.store.save_chat_groups(&groups).map_err(|e| e.key())?;
        self.list_changed();
        Ok(())
    }

    pub(super) fn group_for(&self, conversation_id: &str) -> Result<Option<ChatGroup>, String> {
        Ok(self.store.chat_groups().map_err(|e| e.key())?.into_iter().find(|g| g.conversation_ids.iter().any(|id| id == conversation_id)))
    }

    pub(super) fn group_files(&self, group: Option<&ChatGroup>, cwd: &Path) -> Result<(Vec<String>, Vec<Image>), String> {
        let Some(group) = group else {
            return Ok((vec![], vec![]));
        };
        let mut paths = vec![];
        let mut images = vec![];
        for file in &group.files {
            let path = self.store.chat_groups_dir().join(&group.id).join(&file.id);
            let input = std::fs::File::open(path).map_err(|_| "error.store.read")?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut std::io::Read::take(input, MAX_BYTES as u64 + 1), &mut bytes)
                .map_err(|_| "error.store.read")?;
            if bytes.len() != file.size {
                return Err("error.store.corrupt".into());
            }
            let media_type = mime(&file.name, &bytes)?;
            if media_type.starts_with("image/") {
                images.push(Image { media_type, base64: base64::engine::general_purpose::STANDARD.encode(&bytes) });
            } else {
                // Each immutable copy gets one path, so resuming never fills the chat with duplicate files.
                let relative = format!("attachments/group-{}/{}.txt", group.id, file.id);
                let path = cwd.join(&relative);
                if !crate::permissions::allows(cwd, "Write", &serde_json::json!({"file_path": relative})) {
                    return Err("error.projectSandbox".into());
                }
                std::fs::create_dir_all(path.parent().expect("attachment folder")).map_err(|_| "error.store.write")?;
                std::fs::write(&path, &bytes).map_err(|_| "error.store.write")?;
                paths.push(format!("{relative} ({})", file.name));
            }
        }
        Ok((paths, images))
    }
}
