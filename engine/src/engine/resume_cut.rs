//! Restore work cut by closing, once per interrupted automatic continuation.
use super::*;

impl Engine {
    /// Finds interrupted conversations at startup and resumes eligible work after accounts are ready.
    pub fn resume_interrupted(&self) {
        let (cut, wait, runtime) = self.with(|c| {
            let rows = c.store.list();
            let mut cut = Vec::new();
            for row in rows {
                let interrupted = row.state == Some(crate::sibling::State::Interrupted)
                    || (matches!(row.state, Some(crate::sibling::State::Working | crate::sibling::State::Stuck))
                        && !c.convs.contains_key(&row.id)
                        && !c.running.items.iter().any(|i| i.conversation_id == row.id));
                if !interrupted || row.archived || row.keep_going || c.update_resumes.contains(&row.id) {
                    continue;
                }
                let was_loaded = c.convs.contains_key(&row.id);
                let Some(conv) = c.load(&row.id) else { continue };
                let Some(last) = conv.messages.last().filter(|m| m.role == Role::Assistant && m.closed) else { continue };
                let last_id = last.id.clone();
                let queued: Vec<_> = conv.queued.iter().map(|q| q.id.clone()).collect();
                // Once per request, whatever Faber wrote in between: a resume, a conclusion ask and a second cut waits.
                let resumed = conv
                    .messages
                    .iter()
                    .rev()
                    .take_while(|m| m.role != Role::User || m.wake.is_some())
                    .any(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.resumed"));
                conv.ended = Some(Ended { at: now(), kind: EndKind::Interrupted, work_ms: 0, line: ended::conclusion_line(conv) });
                c.save(&row.id);
                if !was_loaded && !c.save_failed.lock().unwrap_or_else(|e| e.into_inner()).contains(&row.id) {
                    c.convs.remove(&row.id);
                }
                // A team leader nobody else will wake goes on through `resume_cut_leaders`, under the same switch.
                if !resumed && c.store.settings().resume_interrupted() && !c.quiet_teams.contains_key(&row.id) {
                    cut.push((row.id, last_id, queued));
                }
            }
            (cut, install_gate::RESUME_WAIT * c.keep_pace, c.opts.runtime.clone())
        });
        if cut.is_empty() {
            return;
        }
        let engine = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            engine.with(|c| {
                for (id, last, queued) in cut {
                    if c.closing || !c.store.settings().resume_interrupted() {
                        continue;
                    }
                    let Some(conv) = c.load(&id) else { continue };
                    if conv.archived_at.is_some()
                        || conv.keep_going.is_some()
                        || conv.stopped
                        || !conv.queued.iter().map(|q| &q.id).eq(queued.iter())
                        || verifying::checking(conv)
                        || !conv.messages.last().is_some_and(|m| m.id == last && m.closed)
                        || c.live.get(&id).is_some_and(|l| l.turn.is_some())
                    {
                        continue;
                    }
                    let at = now();
                    let times = c.closed_continues.entry(id.clone()).or_default();
                    times.retain(|t| at.saturating_sub(*t) < closed::CONTINUES_SECS * 1000);
                    if times.len() >= closed::CONTINUES {
                        continue;
                    }
                    let waiting = std::mem::take(&mut c.convs.get_mut(&id).unwrap().queued);
                    let sent = c.send_by_itself(&id, closed::CONTINUE.into(), Wake::of("wake.resumed")).is_ok();
                    if let Some(conv) = c.convs.get_mut(&id) {
                        let added = std::mem::replace(&mut conv.queued, waiting);
                        conv.queued.extend(added);
                    }
                    if sent {
                        c.closed_continues.entry(id.clone()).or_default().push(at);
                    }
                    c.save(&id);
                    c.emit_queue(&id);
                }
            });
        });
    }
}
