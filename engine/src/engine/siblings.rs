//! Conversations a project's agent starts by itself (`crate::sibling`): the engine makes a new conversation in the same
//! project, sends it the agent's brief as its first message on the conversation's own model choice, and answers the call
//! at once with the new id. A read answers with where that conversation stands and its latest answer.
use super::*;
use crate::sibling::{self, Request, State};

/// Whether a conversation's agent gets the tool: a project's own agent that no other agent started, so one level only.
pub(super) fn offers(area: Area, started_by_agent: bool) -> bool {
    area == Area::Project && !started_by_agent
}

impl Core {
    /// The agent called the conversation tool. Every call is answered at once, with why when nothing was done.
    pub(super) fn conversation_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let (ok, text) = match sibling::request(&input) {
            Err(why) => (false, format!("Nothing was done: {why}.")),
            Ok(Request::Start { title, brief }) => match self.start_sibling(conv, &title, &brief) {
                Ok((id, true)) => (true, sibling::started(&title, &id)),
                Ok((id, false)) => (false, sibling::failed(&title, &id)),
                Err(why) => (false, format!("No conversation was started: {why}.")),
            },
            Ok(Request::Read { id }) => match self.read_sibling(conv, &id) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing to read: {why}.")),
            },
        };
        // The answer goes back only to the process that asked.
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(&call, ok, text);
        }
    }

    fn start_sibling(&mut self, conv: &str, title: &str, brief: &str) -> Result<(String, bool), &'static str> {
        let parent = self.convs.get(conv).ok_or("the conversation is gone")?;
        if !offers(parent.area, parent.started_by.is_some()) {
            return Err("only a project's own agent can start conversations");
        }
        let project = parent.project_id.clone().ok_or("this conversation has no project")?;
        let choice = parent.choice.clone().ok_or("this conversation has no model chosen")?;
        // The cap is on what agents start: a conversation counts from its first message on, routing and waiting for quota
        // included, not only once its CLI runs. A message the person sends to one is the person's own.
        let running = self
            .convs
            .values()
            .filter(|c| c.started_by.is_some() && c.project_id.as_deref() == Some(project.as_str()))
            .filter(|c| {
                self.live.get(&c.id).is_some_and(|l| l.turn.is_some())
                    || !c.queued.is_empty()
                    || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
            })
            .count();
        if running >= sibling::MAX_RUNNING {
            return Err("three conversations started by agents are already working in this project; wait for one to finish");
        }
        // A conversation an agent starts is never the person's own turn: the same quota rules as a helper hold.
        if let Some(l) = self.live.get(conv) {
            let family = l.model.as_deref().and_then(family_of);
            self.allows(l.vendor, family, Purpose::UserTurn { by_hand: false }, false).map_err(|why| match why {
                Why::Billing => "this AI may be billed per use, so Faber does not start conversations on it by itself",
                _ => "this AI has no quota left right now",
            })?;
        }
        let id = self.blank(Area::Project, Some(project.clone()));
        if let Some(c) = self.convs.get_mut(&id) {
            c.started_by = Some(conv.to_string());
            c.title = title.to_string();
        }
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(project),
            text: sibling::prompt(title, brief),
            images: vec![],
            files: vec![],
            choice,
        };
        match self.send(req) {
            Ok(_) => {
                // The agent's brief is never the person's own turn, whatever model it runs: billing stops it like a helper. A
                // message the person writes to the conversation later is theirs.
                if let Some(t) = self.transfers.get_mut(&id) {
                    t.by_hand = false;
                }
                if let Some(l) = self.live.get_mut(&id) {
                    l.by_hand = false;
                }
                Ok((id, true))
            }
            // Saved with its first message before it failed: it stays, with the error in its reply, and the agent is told.
            Err(_) if self.convs.get(&id).is_some_and(|c| !c.messages.is_empty()) => Ok((id, false)),
            Err(_) => {
                self.convs.remove(&id);
                Err("the conversation could not start")
            }
        }
    }

    fn read_sibling(&mut self, conv: &str, id: &str) -> Result<String, &'static str> {
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
        let c = self.load(id).filter(|c| c.started_by.as_deref() == Some(conv)).ok_or("no conversation with that id was started here")?;
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        let asking = last.is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
        let state = match last.and_then(|m| m.status.as_ref()) {
            _ if asking => State::Waiting,
            Some(Status::Streaming) => State::Working,
            _ if busy || !c.queued.is_empty() => State::Working,
            Some(Status::Error) => State::Failed,
            Some(Status::Stopped) => State::Stopped,
            _ => State::Finished,
        };
        Ok(sibling::read(&c.title, state, last.map(|m| m.text.as_str()).unwrap_or_default()))
    }
}
