//! The lock only selects a conversation's copy; git and disk reads happen after it is released.
use super::*;
use crate::changes::{ChangeList, ChangePatch, FileChange};
use crate::project::copy::WorkCopy;

impl Engine {
    fn change_plan(&self, id: &str) -> Option<(GitRunner, WorkCopy)> {
        self.with(|c| {
            let run = c.git.clone()?;
            let conv = c.load(id)?;
            if conv.area != Area::Project {
                return None;
            }
            Some((run, conv.copy.clone().filter(|copy| !copy.root.is_empty())?))
        })
    }

    pub fn changes(&self, id: &str) -> Vec<FileChange> {
        let Some((run, copy)) = self.change_plan(id) else { return vec![] };
        let Some(mut exec) = run() else { return vec![] };
        crate::changes::list(&mut *exec, &copy)
    }

    pub fn changes_page(&self, id: &str, cursor: Option<&str>) -> ChangeList {
        let Some((run, copy)) = self.change_plan(id) else { return ChangeList::default() };
        let Some(mut exec) = run() else { return ChangeList::default() };
        crate::changes::list_page(&mut *exec, &copy, cursor)
    }

    pub fn change_patch(&self, id: &str, path: &str) -> ChangePatch {
        let Some((run, copy)) = self.change_plan(id) else { return ChangePatch::failed() };
        let Some(mut exec) = run() else { return ChangePatch::failed() };
        crate::changes::patch(&mut *exec, &copy, path)
    }
}
