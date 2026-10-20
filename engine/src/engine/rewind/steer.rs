//! A steered line saves its own baseline before native input; only that line waits for its bounded capture.
use super::*;

fn unavailable(message: &str, turn: &str, at: u64) -> PointInfo {
    PointInfo {
        id: new_id(),
        message_id: message.into(),
        turn_id: turn.into(),
        at,
        coverage: Coverage { unavailable: true, incomplete: true, ..Coverage::default() },
        sealed: false,
        cursor: Cursor::default(),
    }
}

impl Core {
    pub(in crate::engine) fn take_rewind_steer(&mut self, queued: &str) -> Option<PointInfo> {
        self.rewind.steers.remove(queued)
    }

    pub(in crate::engine) fn preparing_rewind_steer(&self, queued: &str) -> bool {
        self.rewind.preparing_steers.contains_key(queued)
    }

    /// True only while this line is being captured. Busy or failed history never holds native steering.
    pub(in crate::engine) fn prepare_rewind_steer(&mut self, id: &str, item: &mut QueuedTurn) -> bool {
        if self.rewind.steers.contains_key(&item.id) {
            return false;
        }
        if self.rewind.preparing_steers.contains_key(&item.id) {
            return true;
        }
        let Some(conversation) = self.convs.get(id).filter(|c| c.area == Area::Project || c.working_folder.is_some()) else {
            return false;
        };
        let source = crate::rewind::Capture::new(conversation);
        let turn = new_id();
        let at = now();
        let fallback = unavailable(&item.id, &turn, at);
        // Skipping busy history must not let a later line overtake an earlier native input.
        if self.rewind.preparing_steers.values().any(|owner| owner == id) {
            self.rewind.steers.insert(item.id.clone(), fallback);
            self.rewind.preparing_steers.insert(item.id.clone(), id.into());
            item.sending = true;
            return true;
        }
        let (Ok(root), Some(history), Some(git), Some(sid)) =
            (self.cwd_of(conversation), self.rewind.history.clone(), self.git.clone(), self.live.get(id).map(|live| live.sid))
        else {
            self.rewind.steers.insert(item.id.clone(), fallback);
            return false;
        };
        if self.rewind.worker.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            self.rewind.steers.insert(item.id.clone(), fallback);
            return false;
        }
        item.sending = true;
        self.rewind.preparing_steers.insert(item.id.clone(), id.into());
        self.rewind.bump(id);
        let (me, id, queued, runtime) = (self.me.clone(), id.to_string(), item.id.clone(), self.opts.runtime.clone());
        let capture_message = queued.clone();
        let work = runtime.spawn_blocking(move || {
            let mut exec = git().ok_or("error.rewind.git")?;
            history.lock().unwrap_or_else(|e| e.into_inner()).capture_prepared(&mut *exec, &root, source, &capture_message, &turn, at, true)
        });
        runtime.spawn(async move {
            let result = work.await.unwrap_or_else(|_| Err("error.rewind.storage".into()));
            let Some(core) = me.upgrade() else {
                return;
            };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.rewind.preparing_steers.remove(&queued);
            core.rewind.bump(&id);
            let stopped = core.convs.get(&id).is_none_or(|c| c.stopped);
            let same_turn = core.live.get(&id).is_some_and(|live| live.sid == sid && live.turn.is_some());
            core.rewind.steers.insert(queued.clone(), result.unwrap_or(fallback));
            let ready: Vec<_> = core
                .convs
                .get(&id)
                .into_iter()
                .flat_map(|c| &c.queued)
                .filter(|item| {
                    item.id == queued || (core.rewind.preparing_steers.contains_key(&item.id) && core.rewind.steers.contains_key(&item.id))
                })
                .map(|item| item.id.clone())
                .collect();
            for queued in ready {
                core.rewind.preparing_steers.remove(&queued);
                let pos = core.convs[&id].queued.iter().position(|item| item.id == queued).expect("queue was just read");
                if !stopped && same_turn && !core.rewind_blocked(&id) {
                    let mut item = core.convs.get_mut(&id).expect("queue exists").queued.remove(pos);
                    core.steer(&id, &mut item);
                    core.convs.get_mut(&id).expect("conversation exists").queued.insert(pos, item);
                } else {
                    core.rewind.steers.remove(&queued);
                    core.convs.get_mut(&id).expect("queue exists").queued[pos].sending = false;
                }
            }
            if !core.convs.get(&id).is_some_and(|c| c.queued.iter().any(|item| item.id == queued)) {
                core.rewind.steers.remove(&queued);
            }
            if !core.convs.contains_key(&id) {
                let abandoned: Vec<_> =
                    core.rewind.preparing_steers.iter().filter(|(_, owner)| *owner == &id).map(|(queued, _)| queued.clone()).collect();
                for queued in abandoned {
                    core.rewind.preparing_steers.remove(&queued);
                    core.rewind.steers.remove(&queued);
                }
            }
            core.save(&id);
            core.emit_queue(&id);
            if !stopped {
                core.drain(&id);
            }
            core.release_rewind_worker();
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_unavailable_steering_keeps_the_queued_message_and_reserved_turn_ids() {
        let point = unavailable("queued-message", "reserved-reply", 42);
        assert_eq!(point.message_id, "queued-message");
        assert_eq!(point.turn_id, "reserved-reply");
        assert!(point.coverage.incomplete && point.coverage.unavailable);
    }
}
