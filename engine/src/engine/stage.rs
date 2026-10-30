//! One tray notice when a leading agent's job ends (task 045): a project conversation that started others ends its run on a
//! closing box that is done or asks the person, once none of its members still works. It goes through the same `Halted`
//! event Keep going uses when it stops, so the tray's words and R92's delivery rules hold. A member stays silent: the leader
//! reports.
use super::*;

impl Core {
    /// Reads the running list about to be sent against the last one, before `mark_ended`: each leader that left it tells once.
    pub(super) fn tell_stages(&mut self, items: &[RunningItem]) {
        let listed = |id: &str| items.iter().any(|i| i.conversation_id == id);
        let left: Vec<(String, &'static str)> = self
            .running
            .items
            .iter()
            .filter(|old| !listed(&old.conversation_id))
            .filter_map(|old| {
                let c = self.convs.get(&old.conversation_id).filter(|c| !c.stopped)?;
                (ended::end_kind(c)? != EndKind::Failed).then_some(())?;
                Some((old.conversation_id.clone(), stage_reason(c)?))
            })
            .collect();
        for (id, why) in left {
            self.tell_stage(&id, why, items);
        }
    }

    fn tell_stage(&mut self, id: &str, why: &'static str, items: &[RunningItem]) {
        let Some(c) = self.convs.get(id) else { return };
        let Some(reply) = c.messages.last() else { return };
        if self.stage_told.get(id) == Some(&reply.id) {
            return;
        }
        let member = |started_by: Option<&str>| started_by == Some(id);
        // A member still working tells the leader when it stops (`tell_parent`), and the leader's next box is told then.
        let working = items.iter().any(|i| self.convs.get(&i.conversation_id).is_some_and(|m| member(m.started_by.as_deref())));
        if working || !self.store.list().iter().any(|s| member(s.started_by.as_deref())) {
            return;
        }
        let (title, vendor) = (c.title.clone(), reply.vendor);
        self.stage_told.insert(id.into(), reply.id.clone());
        (self.emit)(AppEvent::Halted { conversation_id: id.into(), title, why: why.into(), vendor });
    }
}

/// Why a leading agent's ended run tells the person, from its closing box: the job is done, or it asks them to decide. None
/// for a member, or when Keep going already told why it stopped on this reply.
fn stage_reason(c: &Conversation) -> Option<&'static str> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant && m.halt.is_none() && c.started_by.is_none())?;
    let w = reply.wrap_up.as_ref()?;
    if w.done { Some("reason.keepGoing.stage") } else { keep_going::asks(w).then_some("reason.keepGoing.decision") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_leader_tells_once_when_its_job_ends_and_its_members_never() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = super::super::answer::tests::engine(&dir);
        let told = |id: &str| {
            let list = events.lock().unwrap();
            list.iter()
                .filter_map(|e| match e {
                    AppEvent::Halted { conversation_id, why, .. } if conversation_id == id => Some(why.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let boxed = |done: bool| crate::model::WrapUp {
            recap: "The site is up.".into(),
            ask: (!done).then(|| "Publish it?".into()),
            actions: if done { vec![] } else { vec!["Yes".into()] },
            done,
            answered: None,
        };
        // A reply of `conv` that runs, then ends with `box_`.
        let run = |conv: &str, reply: &str, box_: Option<crate::model::WrapUp>, end: bool| {
            engine.with(|c| {
                let m = c.convs.get_mut(conv).unwrap();
                if m.messages.last().is_none_or(|r| r.id != reply) {
                    m.messages.push(Message::new(format!("ask-{reply}"), Role::User, "Build it".into(), 1));
                    m.messages.push(Message::new(reply.into(), Role::Assistant, String::new(), 2));
                }
                let r = m.messages.last_mut().unwrap();
                r.status = Some(if end { Status::Done } else { Status::Streaming });
                r.wrap_up = box_;
                c.save(conv);
                c.refresh_running();
            })
        };
        let lead = engine.with(|c| c.blank(Area::Project, Some("p".into())));
        run(&lead, "r1", None, false);
        run(&lead, "r1", Some(boxed(true)), true);
        assert!(told(&lead).is_empty(), "a conversation that started nobody is not a leader");

        let member = engine.with(|c| {
            let id = c.blank(Area::Project, Some("p".into()));
            c.convs.get_mut(&id).unwrap().started_by = Some(lead.clone());
            id
        });
        run(&member, "m1", None, false);
        run(&lead, "r2", None, false);
        run(&lead, "r2", Some(boxed(true)), true);
        assert!(told(&lead).is_empty(), "not while a member still works");
        run(&member, "m1", Some(boxed(true)), true);
        assert!(told(&member).is_empty(), "a member stays silent");

        run(&lead, "r3", None, false);
        run(&lead, "r3", Some(boxed(true)), true);
        assert_eq!(told(&lead), ["reason.keepGoing.stage"]);
        run(&lead, "r3", Some(boxed(true)), false);
        run(&lead, "r3", Some(boxed(true)), true);
        assert_eq!(told(&lead).len(), 1, "one closing box is told once");
        run(&lead, "r4", None, false);
        run(&lead, "r4", Some(boxed(false)), true);
        assert_eq!(told(&lead)[1], "reason.keepGoing.decision");

        // Keep going stops on the same reply: its own notice, not a second one.
        engine.with(|c| c.convs.get_mut(&lead).unwrap().keep_going = Some(crate::model::KeepGoing::default()));
        run(&lead, "r5", None, false);
        engine.with(|c| {
            let r = c.convs.get_mut(&lead).unwrap().messages.last_mut().unwrap();
            (r.status, r.wrap_up) = (Some(Status::Done), Some(boxed(true)));
            c.halt(&lead, "r5", "reason.keepGoing.done");
            c.refresh_running();
        });
        assert_eq!(told(&lead)[2..], ["reason.keepGoing.done"]);
    }
}
