//! Faber puts away a project conversation that is done (R222): the pull request it follows was merged or closed with
//! nothing said since, or a week passed with nothing in it. It never touches one that is answering, doing Faber's checks,
//! on Keep going, waiting for the person or for a retry, with messages queued, watching an open pull request, or whose
//! separate copy holds work found nowhere else (`copy::unsaved`, as for deleting). Chat is never archived. It runs a minute
//! after start, then once a day, and for one conversation when its followed pull request is read as merged or closed. The
//! engine cannot tell which conversation is open on screen, so that one is not spared: it shows the archived bar as it
//! does when the person archives it. Git and gh run away from the lock, one after the other, and only for a conversation
//! that already passed every check that costs nothing; a conversation not in memory is read, archived and let go.
use super::*;
use crate::project::copy::{self, WorkCopy};
use crate::project::remote::PrState;

/// A conversation with nothing in it for this long is put away.
const IDLE_MS: u64 = 7 * 24 * 3600 * 1000;
const EVERY: Duration = Duration::from_secs(24 * 3600);
const FIRST: Duration = Duration::from_secs(60);

enum Why {
    /// Its pull request ended (merged or closed) and nothing was said in the conversation since (the line is under its last message).
    Over,
    Idle,
    /// Its pull request passed its checks and following ended; whether it was merged since is read before archiving.
    Passing(String, u64),
}

struct Due {
    id: String,
    why: Why,
    copy: Option<WorkCopy>,
    /// What the conversation looked like when it was found; a change means someone wrote to it meanwhile.
    mark: (usize, u64),
}

impl Core {
    /// A conversation as it is now: the one in memory, or a read of its file as `load` would settle it (nothing runs it).
    fn read_conv(&self, id: &str) -> Option<Conversation> {
        self.convs.get(id).cloned().or_else(|| {
            let mut c = Conversation { keep_going: None, ..self.store.get(id)? };
            c.messages.iter_mut().for_each(settle);
            Some(c)
        })
    }

    fn due(&self, c: &Conversation, now: u64) -> Option<Why> {
        let follow = c.messages.iter().rev().find_map(|m| m.follow.as_ref());
        let rests = c.area == Area::Project
            && c.archived_at.is_none()
            && c.pinned_at.is_none()
            && c.queued.is_empty()
            && c.keep_going.is_none()
            && c.weighing.is_none()
            && !self.copying.contains_key(&c.id)
            && self.live.get(&c.id).is_none_or(|l| l.turn.is_none())
            && running_item(c, self.live.get(&c.id)).is_none()
            && c.messages.last().is_none_or(|m| {
                m.recovery.is_none()
                    && m.status != Some(Status::Streaming)
                    // Out of quota: the person tries again, or Faber does when it is back.
                    && !matches!(m.error.as_deref(), Some("error.quotaOut" | "error.quotaWait"))
            })
            // An open pull request is watched by its timer (an old one nobody watches any more is not).
            && follow.is_none_or(|f| !f.state.open() || !self.follows.contains_key(&c.id));
        if !rests {
            return None;
        }
        // The pull request's line sits under the latest message: nothing was said since it ended.
        let ended = c.messages.last().and_then(|m| m.follow.as_ref()).filter(|f| !f.kept);
        match ended {
            Some(f) if matches!(f.state, FollowState::Merged | FollowState::Closed) => Some(Why::Over),
            _ if now.saturating_sub(c.updated_at) >= IDLE_MS => Some(Why::Idle),
            Some(f) if f.state == FollowState::Passing => Some(Why::Passing(f.repo.clone(), f.number)),
            _ => None,
        }
    }

    fn archive_plan(&self, now: u64, only: Option<&str>) -> Vec<Due> {
        if self.closing {
            return vec![];
        }
        let ids: Vec<String> = match only {
            Some(id) => vec![id.to_string()],
            None => self.store.list().into_iter().filter(|s| s.area == Area::Project && !s.archived).map(|s| s.id).collect(),
        };
        ids.into_iter()
            .filter_map(|id| {
                let c = self.read_conv(&id)?;
                let why = self.due(&c, now)?;
                let copy = c.copy.clone().filter(|copy| !copy.root.is_empty());
                Some(Due { mark: (c.messages.len(), c.updated_at), id, why, copy })
            })
            .collect()
    }

    /// Archives it if it is still as found and still rests.
    fn archive_now(&mut self, due: &Due, now: u64) {
        let Some(c) = self.read_conv(&due.id) else { return };
        if (c.messages.len(), c.updated_at) != due.mark || self.due(&c, now).is_none() {
            return;
        }
        let loaded = self.convs.contains_key(&due.id);
        let _ = self.set_archived(&due.id, true);
        if !loaded {
            self.convs.remove(&due.id);
        }
    }

    /// The conversation's followed pull request was read as merged or closed.
    pub(super) fn pr_over(&self, conv: &str) -> bool {
        let follow = self.convs.get(conv).and_then(|c| c.messages.iter().rev().find_map(|m| m.follow.as_ref()));
        follow.is_some_and(|f| matches!(f.state, FollowState::Merged | FollowState::Closed))
    }
}

impl Engine {
    /// Puts away what is done, now: every project conversation, or just `only`.
    pub fn archive_finished(&self, only: Option<&str>) {
        self.archive_at(now(), only)
    }

    /// As [`Engine::archive_finished`] at a given time (milliseconds); tests move the clock with it.
    pub fn archive_at(&self, now: u64, only: Option<&str>) {
        let (dues, run, read) = self.with(|c| (c.archive_plan(now, only), c.git.clone(), c.pr_reader.clone()));
        let mut git: Option<Option<Box<dyn crate::project::git::Executor + Send>>> = None;
        for due in dues {
            if let Why::Passing(repo, number) = &due.why
                && !read(repo, *number).is_ok_and(|look| matches!(look.state, PrState::Merged | PrState::Closed))
            {
                continue;
            }
            if let Some(copy) = &due.copy {
                // A copy git cannot be asked about counts as unsaved.
                let exec = git.get_or_insert_with(|| run.as_ref().and_then(|run| run()));
                if exec.as_mut().is_none_or(|exec| copy::unsaved(&mut **exec, copy)) {
                    continue;
                }
            }
            self.with(|c| c.archive_now(&due, now));
        }
    }

    /// A day's wait costs nothing: one timer, and a pass over the conversation files that the sidebar's own list already does.
    pub fn watch_archive(&self) {
        let (me, runtime) = (self.with(|c| c.me.clone()), self.runtime.clone());
        self.runtime.spawn(async move {
            tokio::time::sleep(FIRST).await;
            loop {
                let Some(core) = me.upgrade() else { return };
                let engine = Engine { core, runtime: runtime.clone() };
                let _ = tokio::task::spawn_blocking(move || engine.archive_finished(None)).await;
                tokio::time::sleep(EVERY).await;
            }
        });
    }
}
