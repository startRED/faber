//! Bounded continuations after Codex reports a busy service, on the same native thread and reply.
use super::*;

const CONTINUATION: &str = "Continue from where you stopped after the service error. Do not repeat completed work.";

impl Core {
    pub(super) fn retry_overload(&mut self, conv: &str) -> bool {
        let Some(l) = self.live.get_mut(conv) else { return false };
        let Some(delay) = crate::failure::overload_retry_delay(crate::failure::Failure::Overloaded, l.overload_retries) else {
            return false;
        };
        let Some(reply) = l.turn.clone() else { return false };
        l.overload_retries += 1;
        let count = l.overload_retries;
        let sid = l.sid;
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        l.overload_wait = Some(cancel);
        l.settled = false;
        if let Some(m) = self.turn(conv) {
            m.overload_retry = Some(count);
            m.status = Some(Status::Streaming);
            m.error = None;
            m.nudge = None;
            m.look = None;
        }
        self.save(conv);
        self.push(conv, Some(&reply), true);
        let (me, conv) = (self.me.clone(), conv.to_owned());
        self.opts.runtime.spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                _ = cancelled => return,
            }
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.closing || core.convs.get(&conv).is_none_or(|c| c.stopped) {
                return;
            }
            let Some(l) = core.live.get(&conv).filter(|l| l.sid == sid && l.turn.as_deref() == Some(&reply) && l.overload_wait.is_some())
            else {
                return;
            };
            let pick =
                Pick { vendor: l.vendor, model: l.model.clone(), effort: l.effort.clone(), reason: "reason.chosen".into(), pace: None };
            let approved = l.approved;
            if let Err(error) = core.gate_turn_approved(&pick, false, approved).and_then(|()| core.begin_unbounded(pick.vendor)) {
                if let Some(m) = core.turn(&conv) {
                    m.status = Some(Status::Error);
                    m.error = Some(error);
                }
                core.finish(&conv, Some(reply));
                return;
            }
            let started = core.secs();
            if let Some(l) = core.live.get_mut(&conv) {
                l.overload_wait = None;
                l.started = started;
                l.settled = false;
                l.break_pending = true;
                l.session.send(UserTurn::text(CONTINUATION));
            }
            core.listen(&conv);
        });
        true
    }
}
