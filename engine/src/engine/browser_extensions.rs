//! The browser tool can load new extension code only after a current, folder-specific answer from the person.
use super::*;
use serde_json::{Value, json};

const CARD: &str = "BrowserExtensionLoad";

pub(super) struct PendingExtension {
    call: String,
    sid: u64,
    turn: String,
    folder: String,
    revision: u64,
}

fn command(input: &Value) -> Option<(&'static str, Option<&str>)> {
    if input.as_object()?.iter().any(|(key, value)| !value.is_null() && !matches!(key.as_str(), "action" | "folder")) {
        return None;
    }
    let folder = match input.get("folder") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let path = value.as_str()?;
            if path.is_empty() || path.len() > 4096 || path.chars().any(|c| c.is_control()) {
                return None;
            }
            Some(path)
        }
    };
    match (input["action"].as_str()?, folder) {
        ("extensions_list", None) => Some(("list", None)),
        ("extensions_load", Some(path)) => Some(("load", Some(path))),
        ("extensions_reload", Some(path)) => Some(("reload", Some(path))),
        ("extensions_remove", Some(path)) => Some(("remove", Some(path))),
        _ => None,
    }
}

/// Resolve Windows' extended-length spelling for the shell without accepting UNC or device paths.
fn project_folder(root: &std::path::Path, requested: &str) -> Option<String> {
    #[cfg(windows)]
    if requested.starts_with(['\\', '/']) || requested.get(2..).is_some_and(|tail| tail.contains(':')) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let folder = root.join(requested).canonicalize().ok()?;
    if folder == root || !folder.starts_with(&root) || !folder.is_dir() {
        return None;
    }
    let path = folder.to_str()?;
    #[cfg(windows)]
    let path = {
        let local = path.strip_prefix(r"\\?\")?;
        if local.as_bytes().get(1..3) != Some(b":\\") {
            return None;
        }
        local
    };
    Some(path.to_string())
}

impl Core {
    pub(super) fn browse_extensions(&mut self, conv: &str, sid: u64, call: String, input: Value) {
        let Some((action, folder)) = command(&input) else {
            return self.answer_toolbox(conv, sid, &call, false, "Use one extension action and its folder when required.".into());
        };
        if !self.browser.as_ref().is_some_and(|browser| browser.supports_extensions()) {
            return self.answer_toolbox(conv, sid, &call, false, "error.browser.extensionsUnavailable".into());
        }
        if self.store.settings().extensions_developer_mode != Some(true) {
            return self.answer_toolbox(conv, sid, &call, false, "error.browser.extensionsDisabled".into());
        }
        let Some(requested) = folder.filter(|_| action == "load") else {
            return self.run_extension(conv, sid, call, action, folder.map(str::to_string));
        };
        let folder = self.convs.get(conv).and_then(|c| self.cwd_of(c).ok()).and_then(|cwd| project_folder(&cwd, requested));
        let Some(folder) = folder else {
            // The shell can use the exact path of a folder already registered through the person's picker.
            // Reload never registers a folder, so this cannot approve an outside path.
            if std::path::Path::new(requested).is_absolute() {
                return self.run_extension(conv, sid, call, "reload", Some(requested.into()));
            }
            return self.answer_toolbox(
                conv,
                sid,
                &call,
                false,
                "Choose an extension directory inside your project. The person can pick an external folder in the browser.".into(),
            );
        };
        if self.extension_loads.contains_key(conv) {
            return self.answer_toolbox(conv, sid, &call, false, "Finish the pending extension approval first.".into());
        }
        let Some(turn) = self.live.get(conv).and_then(|live| live.turn.clone()) else {
            return self.answer_toolbox(conv, sid, &call, false, "There is no active turn to approve the extension.".into());
        };
        let Some(message) = self.turn(conv) else {
            return self.answer_toolbox(conv, sid, &call, false, "There is no active reply to show the approval.".into());
        };
        message.permissions.get_or_insert_default().push(Permission {
            request_id: call.clone(),
            tool: CARD.into(),
            input: json!({ "folder": folder }),
            epoch: None,
        });
        self.extension_loads
            .insert(conv.into(), PendingExtension { call, sid, turn: turn.clone(), folder, revision: self.extension_revision });
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(&turn), true);
    }

    pub(super) fn answer_extension(&mut self, conv: &str, turn: &str, call: &str, allow: bool) -> Option<Result<(), String>> {
        let message = self.convs.get_mut(conv)?.messages.iter_mut().find(|m| m.id == turn)?;
        let cards = message.permissions.as_mut()?;
        let index = cards.iter().position(|p| p.request_id == call && p.tool == CARD)?;
        let card = cards.remove(index);
        let asked = self
            .extension_loads
            .get(conv)
            .is_some_and(|p| p.call == call && p.turn == turn)
            .then(|| self.extension_loads.remove(conv))
            .flatten();
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(turn), true);
        let Some(asked) = asked else { return Some(Err(super::answer::STALE.into())) };
        let current = self
            .live
            .get(conv)
            .is_some_and(|l| l.sid == asked.sid && l.turn.as_deref() == Some(turn) && l.permissions == Permissions::FullAccess);
        if !current
            || asked.revision != self.extension_revision
            || self.store.settings().extensions_developer_mode != Some(true)
            || card.input["folder"].as_str() != Some(&asked.folder)
        {
            self.answer_toolbox(conv, asked.sid, call, false, "The extension approval expired. Request it again.".into());
            return Some(Err(super::answer::STALE.into()));
        }
        if !allow {
            self.answer_toolbox(conv, asked.sid, call, false, "The person declined that extension folder.".into());
            return Some(Ok(()));
        }
        let folder = self.convs.get(conv).and_then(|c| self.cwd_of(c).ok()).and_then(|cwd| project_folder(&cwd, &asked.folder));
        if folder.as_ref() != Some(&asked.folder) {
            self.answer_toolbox(conv, asked.sid, call, false, "The approved extension directory changed.".into());
            return Some(Ok(()));
        }
        self.run_extension(conv, asked.sid, call.into(), "load", folder);
        Some(Ok(()))
    }

    fn run_extension(&self, conv: &str, sid: u64, call: String, action: &'static str, folder: Option<String>) {
        let Some(browser) = self.browser.clone() else { return };
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            let (ok, text) = match browser.extensions(conv.clone(), action, folder).await {
                Ok(rows) => (true, rows.to_string()),
                Err(error) => (false, error),
            };
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_actions_cannot_mix_page_navigation_and_loading_code() {
        assert_eq!(command(&json!({"action":"extensions_load","folder":"dev"})), Some(("load", Some("dev"))));
        assert_eq!(command(&json!({"action":"extensions_list","url":"https://example.org/"})), None);
        assert_eq!(command(&json!({"action":"extensions_list","folder":"dev"})), None);
        assert_eq!(command(&json!({"action":"extensions_load","folder":"dev\nmisleading"})), None);
    }

    #[test]
    fn an_agent_extension_must_resolve_inside_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("extension")).unwrap();
        assert!(project_folder(&root, "extension").is_some());
        assert!(project_folder(&root, "..").is_none());
        assert!(project_folder(&root, ".").is_none());
    }
}
