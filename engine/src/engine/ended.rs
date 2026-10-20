//! A run that ended while nobody looked (task 016, the legible agent runs design): the conversation keeps one mark,
//! `Conversation::ended`, from the moment it leaves the running list until the person opens it. The running column draws it
//! as a done card and the sidebar as a dot, both from the conversation list (`Summary::ended`), so they agree and survive a
//! restart. A run the person was there for (their own Stop, an archive) leaves no mark, and a new run takes the old one away.
use super::*;

impl Engine {
    /// The person has the conversation open on a visible window: its done card and its dot go.
    pub fn seen(&self, id: &str) -> Result<(), String> {
        self.with(|c| c.seen(id))
    }
}

impl Core {
    pub(super) fn seen(&mut self, id: &str) -> Result<(), String> {
        let c = self.load(id).ok_or("error.conversationGone")?;
        if c.ended.take().is_none() {
            return Ok(());
        }
        self.save(id);
        (self.emit)(AppEvent::List);
        Ok(())
    }

    /// Reads the running list about to be sent against the last one: a conversation that leaves it ended its run; one back at
    /// work starts a new run and loses its old mark. A reply only closing (`settled`) has not left yet: Faber's checks may still
    /// follow it, and a card that came and went would be noise.
    pub(super) fn mark_ended(&mut self, items: &[RunningItem]) {
        let at = now();
        let listed = |id: &str| items.iter().any(|i| i.conversation_id == id);
        let mut changed = Vec::new();
        for old in self.running.items.iter().filter(|i| !listed(&i.conversation_id)) {
            let Some(c) = self.convs.get_mut(&old.conversation_id) else { continue };
            // The person's own Stop: they were there.
            if c.stopped {
                continue;
            }
            let Some(kind) = end_kind(c) else { continue };
            let start = old.keep_progress.as_ref().map(|p| p.since).filter(|s| *s > 0).unwrap_or(old.since);
            c.ended = Some(Ended { at, kind, work_ms: at.saturating_sub(start), line: keep_going::progress_line(&c.messages) });
            changed.push(old.conversation_id.clone());
        }
        for item in items.iter().filter(|i| !i.settled) {
            let Some(c) = self.convs.get_mut(&item.conversation_id) else { continue };
            if c.ended.take().is_some() {
                changed.push(item.conversation_id.clone());
            }
        }
        if changed.is_empty() {
            return;
        }
        for id in &changed {
            self.save(id);
        }
        (self.emit)(AppEvent::List);
    }
}

/// How the conversation's run ended, or None when it has not ended for the person: they put it away, or Keep going or a
/// waiting message goes on with it.
fn end_kind(c: &Conversation) -> Option<EndKind> {
    if c.archived_at.is_some() || c.keep_going.is_some() || !c.queued.is_empty() {
        return None;
    }
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant)?;
    Some(if reply.error.is_some() || reply.status == Some(Status::Error) {
        EndKind::Failed
    } else if reply.halt.as_deref().is_some_and(|why| why != "reason.keepGoing.done")
        || keep_going::unanswered_question(reply)
        || reply.wrap_up.as_ref().is_some_and(keep_going::asks)
    {
        EndKind::NeedsYou
    } else {
        EndKind::Done
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(messages: Vec<Message>) -> Conversation {
        let mut c: Conversation = serde_json::from_value(serde_json::json!({
            "id": "c", "title": "Fix the login page", "area": "project", "createdAt": 0, "updatedAt": 0, "messages": []
        }))
        .unwrap();
        c.messages = messages;
        c
    }

    fn reply(status: Status) -> Message {
        let mut m = Message::new("r".into(), Role::Assistant, "Fixed.".into(), 0);
        m.status = Some(status);
        m
    }

    fn person() -> Message {
        Message::new("u".into(), Role::User, "Fix it".into(), 0)
    }

    #[test]
    fn a_run_ends_done_failed_or_needing_the_person() {
        assert_eq!(end_kind(&conv(vec![person(), reply(Status::Done)])), Some(EndKind::Done));
        let mut failed = reply(Status::Error);
        failed.error = Some("error.turnFailed".into());
        assert_eq!(end_kind(&conv(vec![person(), failed])), Some(EndKind::Failed));
        let mut capped = reply(Status::Done);
        capped.halt = Some("reason.keepGoing.cap".into());
        assert_eq!(end_kind(&conv(vec![person(), capped])), Some(EndKind::NeedsYou));
        let mut done = reply(Status::Done);
        done.halt = Some("reason.keepGoing.done".into());
        assert_eq!(end_kind(&conv(vec![person(), done])), Some(EndKind::Done));
        // A closing box that asks the person something waits on them until it is answered.
        let mut boxed = reply(Status::Done);
        let ask = crate::model::WrapUp {
            recap: "Fixed.".into(),
            ask: Some("Ship it?".into()),
            actions: vec!["Yes".into()],
            done: false,
            answered: None,
        };
        boxed.wrap_up = Some(ask.clone());
        assert_eq!(end_kind(&conv(vec![person(), boxed.clone()])), Some(EndKind::NeedsYou));
        boxed.wrap_up = Some(crate::model::WrapUp { answered: Some("Yes".into()), ..ask });
        assert_eq!(end_kind(&conv(vec![person(), boxed])), Some(EndKind::Done));
        assert_eq!(end_kind(&conv(vec![person(), reply(Status::Stopped)])), Some(EndKind::Done), "a turn Faber cut still ends the run");
        let mut archived = conv(vec![person(), reply(Status::Done)]);
        archived.archived_at = Some(1);
        assert_eq!(end_kind(&archived), None);
    }

    #[test]
    fn a_card_nobody_opens_goes_after_a_week_and_a_failure_after_a_day() {
        let day = 86_400_000;
        let mark = |kind| Ended { at: 0, kind, work_ms: 0, line: None };
        assert!(mark(EndKind::Done).current(6 * day) && !mark(EndKind::Done).current(7 * day));
        assert!(mark(EndKind::NeedsYou).current(6 * day));
        assert!(mark(EndKind::Failed).current(day - 1) && !mark(EndKind::Failed).current(day));
    }
}
