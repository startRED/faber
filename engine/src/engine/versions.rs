//! The versions of a message (parity T7). Editing a message, retrying an answer and "Branch from here" each make a new
//! conversation (`Core::branch`) that keeps the messages before that point with their ids. So the versions of a message are
//! the conversations of the same family (linked by `branch_of`) that hold the same messages before it and a message of the
//! person's own at its place: no second history is stored, and each version keeps its own answers.
use super::*;
use crate::model::{VersionAt, Versions};

/// What a family member says about its messages: its id, when it was made and, per message, its id, role and time.
type Shape = (String, u64, Vec<(String, Role, u64)>);

impl Core {
    /// Which conversation branched from which, read once from the files that mention a branch and kept up to date by
    /// `branch`. A conversation removed since stays listed; nothing finds it, so it is left out.
    fn links(&mut self) -> &[(String, String)] {
        self.links.get_or_insert_with(|| self.store.branch_links())
    }

    /// A conversation that goes takes no family apart: what branched from it now branches from where it came from (from
    /// the first of its branches when it was the first of the family), so the versions that stay keep finding each other
    /// after a restart too. `heir` is where `gone` itself branched from, read before its record went.
    pub(super) fn keep_family(&mut self, gone: &str, mut heir: Option<BranchOf>) {
        let ids: Vec<String> = self.links().iter().filter(|(_, parent)| parent == gone).map(|(child, _)| child.clone()).collect();
        if ids.is_empty() {
            return;
        }
        // The folder lists files in no fixed order (macOS differs from Windows): the oldest branch is the first, then by id.
        let mut children: Vec<(u64, String)> = ids.into_iter().filter_map(|c| self.load(&c).map(|x| (x.created_at, c))).collect();
        children.sort();
        for (_, child) in &children {
            let Some(c) = self.load(child) else { continue };
            let own = c.branch_of.take();
            c.branch_of = heir.clone();
            if heir.is_none() {
                heir = own.map(|own| BranchOf { conversation_id: child.clone(), message_id: own.message_id });
            }
            self.save(child);
        }
        // Read again from the files at the next look.
        self.links = None;
    }

    pub(super) fn note_branch(&mut self, child: &str, parent: &str) {
        if let Some(links) = &mut self.links {
            links.push((child.to_string(), parent.to_string()));
        }
    }

    /// The messages of conversation `id` that have more than one version, each with every version in the order they were
    /// written. An archived conversation of the family is put away and shows as no version.
    pub(super) fn versions(&mut self, id: &str) -> Vec<Versions> {
        let mut family = vec![id.to_string()];
        let links = self.links().to_vec();
        let mut next = 0;
        while let Some(at) = family.get(next).cloned() {
            for (child, parent) in &links {
                let other = if *child == at {
                    parent
                } else if *parent == at {
                    child
                } else {
                    continue;
                };
                if !family.contains(other) {
                    family.push(other.clone());
                }
            }
            next += 1;
        }
        if family.len() < 2 {
            return vec![];
        }
        let shape = |c: &Conversation| -> Option<Shape> {
            (c.archived_at.is_none() || c.id == id)
                .then(|| (c.id.clone(), c.created_at, c.messages.iter().map(|m| (m.id.clone(), m.role, m.at)).collect()))
        };
        // The open ones from memory; the others are read and dropped, so looking holds nothing.
        let members: Vec<Shape> = family
            .iter()
            .filter_map(|f| match self.convs.get(f) {
                Some(c) => shape(c),
                None => self.store.get(f).as_ref().and_then(shape),
            })
            .collect();
        let Some(me) = members.iter().find(|m| m.0 == id) else { return vec![] };
        // How many messages each member shares with this one from the start: one pass each, however long the history.
        let shared: Vec<usize> = members.iter().map(|m| m.2.iter().zip(&me.2).take_while(|(a, b)| a.0 == b.0).count()).collect();
        let mut found = vec![];
        for (i, (mid, role, _)) in me.2.iter().enumerate() {
            if *role != Role::User {
                continue;
            }
            // (message id, its time, the conversation that shows it, when that one was made)
            let mut seen: Vec<(String, u64, String, u64)> = vec![];
            for ((cid, created, messages), shared) in members.iter().zip(&shared) {
                if *shared < i || messages.len() <= i || messages[i].1 != Role::User {
                    continue;
                }
                match seen.iter_mut().find(|s| s.0 == messages[i].0) {
                    // A later branch holds this version too; it shows in the conversation where it was written, or here.
                    Some(s) => {
                        if cid == id || (s.2 != id && *created < s.3) {
                            (s.2, s.3) = (cid.clone(), *created);
                        }
                    }
                    None => seen.push((messages[i].0.clone(), messages[i].2, cid.clone(), *created)),
                }
            }
            if seen.len() < 2 {
                continue;
            }
            seen.sort_by_key(|s| (s.1, s.3));
            let at = seen.iter().position(|s| s.0 == *mid).unwrap_or(0);
            let list = seen.into_iter().map(|(message_id, _, conversation_id, _)| VersionAt { conversation_id, message_id }).collect();
            found.push(Versions { message_id: mid.clone(), at, list });
        }
        found
    }
}
