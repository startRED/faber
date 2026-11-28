//! Answer review borrows private history after releasing the engine lock.

use super::*;
use crate::rewind::{AnswerChangePatch, AnswerChanges};

impl Engine {
    fn answer_history(&self, id: &str, message: &str) -> Result<(Arc<Mutex<History>>, PointInfo), String> {
        self.with(|core| {
            let conversation = core.load(id).ok_or("error.conversationGone")?;
            let entry =
                conversation.messages.iter().find(|entry| entry.id == message && entry.role == Role::User).ok_or("error.rewind.message")?;
            let marker = entry.rewind.clone().filter(|point| !point.coverage.unavailable).ok_or("error.rewind.unavailable")?;
            Ok((core.rewind.history.clone().ok_or("error.rewind.unavailable")?, marker))
        })
    }

    pub fn answer_changes(&self, id: &str, message: &str, offset: usize) -> Result<AnswerChanges, String> {
        let (history, marker) = self.answer_history(id, message)?;
        let history = history.lock().unwrap_or_else(|e| e.into_inner());
        if !history.review_matches(id, message, &marker.id) {
            return Err("error.rewind.unavailable".into());
        }
        history.review(id, message, offset)
    }

    pub fn answer_change_patch(&self, id: &str, message: &str, path: &str, offset: usize) -> Result<AnswerChangePatch, String> {
        let (history, marker) = self.answer_history(id, message)?;
        let history = history.lock().unwrap_or_else(|e| e.into_inner());
        if !history.review_matches(id, message, &marker.id) {
            return Err("error.rewind.unavailable".into());
        }
        history.review_patch(id, message, path, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answer_review_requires_the_original_prompt_and_exact_durable_point() {
        struct Listing;
        impl crate::project::git::Executor for Listing {
            fn execute(
                &mut self,
                call: &crate::project::git::Invocation,
            ) -> Result<crate::project::git::Output, crate::project::git::GitError> {
                let ignored = call.args.iter().any(|arg| arg == "--ignored");
                Ok(crate::project::git::Output { success: true, stdout: if ignored { vec![] } else { b"file.txt\0".to_vec() } })
            }
        }
        let data = crate::workspace::TestRoot::new();
        let (engine, _) = crate::engine::answer::tests::engine(&data);
        let (id, root, history) = engine.with(|core| {
            let id = core.blank(Area::Chat, None);
            let conversation = core.convs.get_mut(&id).unwrap();
            conversation.messages.push(Message::new("question".into(), Role::User, "Change this file".into(), 1));
            conversation.messages.push(Message::new("reply".into(), Role::Assistant, "Done".into(), 2));
            core.save(&id);
            (id.clone(), core.cwd_of(&core.convs[&id]).unwrap(), core.rewind.history.clone().unwrap())
        });
        std::fs::write(root.join("file.txt"), "before\n").unwrap();
        let conversation = engine.open(&id).unwrap();
        let marker = history.lock().unwrap().capture(&mut Listing, &root, &conversation, "question", "reply", 1, false).unwrap();
        engine.with(|core| core.mark_rewind(&id, "reply", Ok(marker)));
        std::fs::write(root.join("file.txt"), "saved after\n").unwrap();
        {
            let mut history = history.lock().unwrap();
            let scan = history.scan_seal(&mut Listing, &root).unwrap();
            history.seal(scan, 2).unwrap();
        }
        std::fs::write(root.join("file.txt"), "later personal bytes\n").unwrap();
        assert_eq!(engine.answer_changes(&id, "question", 0).unwrap().total, 1, "durable history accepts older unsealed message markers");
        let patch = engine.answer_change_patch(&id, "question", "file.txt", 0).unwrap();
        assert!(patch.lines.iter().any(|line| line == "+saved after"));
        assert_eq!(engine.answer_changes(&id, "reply", 0).unwrap_err(), "error.rewind.message");
        let branch = engine.with(|core| {
            let copied = core.convs[&id].messages.clone();
            let branch = core.blank(Area::Chat, None);
            core.convs.get_mut(&branch).unwrap().messages = copied;
            core.convs.get_mut(&id).unwrap().messages[0].rewind.as_mut().unwrap().id = "stale-point".into();
            branch
        });
        for conversation in [&id, &branch] {
            assert_eq!(engine.answer_changes(conversation, "question", 0).unwrap_err(), "error.rewind.unavailable");
            assert_eq!(engine.answer_change_patch(conversation, "question", "file.txt", 0).unwrap_err(), "error.rewind.unavailable");
        }
        assert_eq!(std::fs::read_to_string(root.join("file.txt")).unwrap(), "later personal bytes\n");
        engine.shutdown();
    }
}
