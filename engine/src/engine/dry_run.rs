use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::answer::tests::engine;

    fn add_dry(c: &mut Conversation, count: u32, at: u64) {
        for n in 0..count {
            let mut m = Message::new(format!("dry-{}-{n}", c.messages.len()), Role::Assistant, "Still waiting".into(), at);
            m.status = Some(Status::Done);
            c.messages.push(m);
        }
    }

    #[tokio::test]
    async fn thresholds_ask_once_halt_and_delivery_clears() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let clock = Arc::new(Mutex::new(4600.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|core| {
            let id = core.blank(Area::Chat, None);
            let c = core.convs.get_mut(&id).unwrap();
            add_dry(c, 11, 1_000_000);
            assert!(matches!(core.dry_gate(&id), Gate::Go));
            add_dry(core.convs.get_mut(&id).unwrap(), 1, 1_000_000);
            *clock.lock().unwrap() = 4599.0;
            assert!(matches!(core.dry_gate(&id), Gate::Go));
            *clock.lock().unwrap() = 4600.0;
            let Gate::Ask(text) = core.dry_gate(&id) else { panic!("expected warning") };
            assert!(text.contains("12 turns over 1.0 hours"));
            assert!(text.contains("US$0.00"));
            assert!(text.contains("Still waiting"));
            assert!(text.contains("wrap_up with done=false"));
            assert!(core.store.get(&id).unwrap().stuck.is_some());
            assert!(matches!(core.dry_gate(&id), Gate::Go));
            add_dry(core.convs.get_mut(&id).unwrap(), 2, 4_600_000);
            assert!(matches!(core.dry_gate(&id), Gate::Go), "two replies after the ask are still its chance");
            add_dry(core.convs.get_mut(&id).unwrap(), 1, 4_600_000);
            assert!(matches!(core.dry_gate(&id), Gate::Halt), "the third with nothing stops Keep going");
            assert!(core.store.get(&id).unwrap().stuck.is_none(), "a person who goes on starts a new window");
            assert!(matches!(core.dry_gate(&id), Gate::Ask(_)), "and the AI is asked again before anything stops it");
            let c = core.convs.get_mut(&id).unwrap();
            c.messages.last_mut().unwrap().commits.push("done".into());
            assert!(matches!(core.dry_gate(&id), Gate::Go));
            assert!(core.store.get(&id).unwrap().stuck.is_none());
        });
    }
}

#[cfg(test)]
mod child_tests {
    use super::*;
    use crate::engine::answer::tests::engine;

    #[tokio::test]
    async fn a_child_delivery_ends_the_ask_and_the_run_counts_from_it() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let clock = Arc::new(Mutex::new(4600.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|core| {
            let parent = core.blank(Area::Chat, None);
            let child = core.blank(Area::Chat, None);
            let c = core.convs.get_mut(&parent).unwrap();
            for n in 0..12 {
                let mut m = Message::new(n.to_string(), Role::Assistant, "waiting".into(), 1_000_000);
                m.status = Some(Status::Done);
                c.messages.push(m);
            }
            assert!(matches!(core.dry_gate(&parent), Gate::Ask(_)));
            *clock.lock().unwrap() = 4700.0;
            let c = core.convs.get_mut(&child).unwrap();
            c.started_by = Some(parent.clone());
            let mut m = Message::new("done".into(), Role::Assistant, String::new(), 4_650_000);
            m.status = Some(Status::Done);
            m.commits.push("commit".into());
            c.messages.push(m);
            core.save(&child);
            assert!(matches!(core.dry_gate(&parent), Gate::Go));
            assert!(core.store.get(&parent).unwrap().stuck.is_none(), "what the child delivered ended the ask");
            // An hour after the child's delivery with the parent still dry, the parent is asked again: the child's work does not
            // silence it for good.
            *clock.lock().unwrap() = 8249.0;
            assert!(matches!(core.dry_gate(&parent), Gate::Go));
            *clock.lock().unwrap() = 8250.0;
            assert!(matches!(core.dry_gate(&parent), Gate::Ask(_)));
        });
    }

    #[tokio::test]
    async fn the_last_line_skips_empty_words_and_a_clock_that_jumps_back_cannot_keep_the_mark() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        engine.set_time_source(Arc::new(|| 4600.0));
        engine.with(|core| {
            let id = core.blank(Area::Chat, None);
            let c = core.convs.get_mut(&id).unwrap();
            for n in 0..12 {
                let text = if n == 11 {
                    "Checking the queue.
   
"
                } else {
                    ""
                };
                let mut m = Message::new(n.to_string(), Role::Assistant, text.into(), 1_000_000);
                m.status = Some(Status::Done);
                c.messages.push(m);
            }
            let Gate::Ask(text) = core.dry_gate(&id) else { panic!("expected the ask") };
            assert!(text.contains("Your last line: \"Checking the queue.\""), "{text}");
            // A delivery stamped before the dry run began (a clock that went back) still ends it.
            let c = core.convs.get_mut(&id).unwrap();
            let mut m = Message::new("late".into(), Role::Assistant, "Done".into(), 500);
            m.status = Some(Status::Done);
            m.commits.push("c".into());
            c.messages.push(m);
            assert!(matches!(core.dry_gate(&id), Gate::Go));
            assert!(core.store.get(&id).unwrap().stuck.is_none());
        });
    }
}

const TURNS: u32 = 12;
const WINDOW_SECS: f64 = 3600.0;
/// Replies the AI gets after Faber's ask to deliver something; with none, Keep going stops (one lock with the turn limit of the honest end).
const AFTER_ASK: u32 = 3;

pub(super) enum Gate {
    Go,
    Ask(String),
    Halt,
}

impl Core {
    /// Drops the saved mark once the run it was made for is over: the conversation's tail is no longer dry, or a newer dry run
    /// began after a delivery. Read from the replies, never from clock times, so a clock that jumps cannot keep or lose it.
    pub(super) fn clear_dry_delivery(&mut self, conv: &str) {
        let over = self.convs.get(conv).is_some_and(|c| {
            c.stuck.as_ref().is_some_and(|s| {
                let dry = crate::results::tally(c).dry;
                dry.turns == 0 || dry.since != Some(s.since)
            })
        });
        if over {
            self.unmark_stuck(conv);
        }
    }

    fn unmark_stuck(&mut self, conv: &str) {
        if let Some(c) = self.convs.get_mut(conv) {
            c.stuck = None;
            self.save(conv);
            self.list_changed();
        }
    }

    /// The ask did not go out (nothing could take the turn): the mark stays, but the AI has not been asked yet.
    pub(super) fn unask_stuck(&mut self, conv: &str) {
        if let Some(s) = self.convs.get_mut(conv).and_then(|c| c.stuck.as_mut()) {
            s.asked_at = None;
            s.ask_turns = 0;
            self.save(conv);
        }
    }

    /// The last time a conversation this one started delivered something (its own, not its children's children).
    fn child_delivery(&self, conv: &str) -> Option<u64> {
        self.store
            .list()
            .iter()
            .filter(|s| s.started_by.as_deref() == Some(conv))
            .filter_map(|s| self.convs.get(&s.id).map(crate::results::tally).or_else(|| s.results.clone()).and_then(|r| r.last_delivery))
            .max()
    }

    /// Asked at the moment Keep going is about to send its next turn: go on, ask the AI once, or stop.
    pub(super) fn dry_gate(&mut self, conv: &str) -> Gate {
        self.clear_dry_delivery(conv);
        let Some(c) = self.convs.get(conv) else { return Gate::Go };
        let dry = crate::results::tally(c).dry;
        let (now, window) = ((self.secs() * 1000.0) as u64, (WINDOW_SECS * 1000.0) as u64);
        let Some(since) = dry.since else { return Gate::Go };
        if dry.turns < TURNS || now.saturating_sub(since) < window {
            return Gate::Go;
        }
        // Work a conversation it started delivered is its result too, and the run counts from there.
        let child = self.child_delivery(conv);
        let since = since.max(child.unwrap_or(0));
        let wall = now.saturating_sub(since);
        if wall < window {
            if child.is_some_and(|at| self.convs.get(conv).and_then(|c| c.stuck.as_ref()).is_some_and(|s| at > s.noticed_at)) {
                self.unmark_stuck(conv);
            }
            return Gate::Go;
        }
        let c = self.convs.get(conv).unwrap();
        if let Some(stuck) = c.stuck.clone().filter(|s| s.asked_at.is_some()) {
            if dry.turns >= stuck.ask_turns.saturating_add(AFTER_ASK) {
                // The run is over: a person who goes on starts a new window, and the AI is asked again before anything stops it.
                self.unmark_stuck(conv);
                return Gate::Halt;
            }
            return Gate::Go;
        }
        let last: String = c
            .messages
            .iter()
            .rev()
            .filter(|m| m.role == Role::Assistant && m.status == Some(Status::Done))
            .find_map(|m| m.text.lines().rev().map(str::trim).find(|l| !l.is_empty()))
            .map(|l| l.chars().take(200).collect())
            .unwrap_or_default();
        let text = format!(
            "The last {} turns over {:.1} hours delivered nothing: no file changed, no check passed, no commit, no pull request, no answer to the person, and no closing box with done. They cost about US${:.2} at API price. Your last line: \"{}\". Choose exactly one: (a) change approach and say in one sentence what is different; (b) ask the person with the question tool; or (c) call wrap_up with done=false and say what is not done. Waiting on CI, a queue, a build or another conversation is not a result and must not be repeated as a loop.",
            dry.turns,
            wall as f64 / 3_600_000.0,
            dry.usd,
            last
        );
        let noticed = c.stuck.as_ref().map_or(now, |s| s.noticed_at);
        self.convs.get_mut(conv).unwrap().stuck = Some(crate::results::Stuck {
            noticed_at: noticed,
            since,
            turns: dry.turns,
            wall_ms: wall,
            usd: dry.usd,
            asked_at: Some(now),
            ask_turns: dry.turns,
        });
        self.save(conv);
        self.list_changed();
        Gate::Ask(text)
    }
}

#[cfg(test)]
mod send_tests {
    use super::*;
    use crate::engine::answer::tests::engine;

    #[tokio::test]
    async fn dry_run_sends_the_warning_and_halts_after_the_next_window() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let clock = Arc::new(Mutex::new(4600.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|core| {
            let id = core.blank(Area::Project, None);
            let c = core.convs.get_mut(&id).unwrap();
            c.keep_going = Some(KeepGoing::default());
            c.stopped = true;
            for n in 0..12 {
                let mut m = Message::new(n.to_string(), Role::Assistant, "waiting".into(), 1_000_000);
                m.status = Some(Status::Done);
                c.messages.push(m);
            }
            core.send_next(&id, "11");
            let c = core.convs.get_mut(&id).unwrap();
            assert_eq!(c.queued.last().unwrap().wake.as_ref().unwrap().why, "wake.stuck");
            for n in 12..24 {
                let mut m = Message::new(n.to_string(), Role::Assistant, "waiting".into(), 4_600_000);
                m.status = Some(Status::Done);
                c.messages.push(m);
            }
            *clock.lock().unwrap() = 8200.0;
            core.send_next(&id, "23");
            let c = &core.convs[&id];
            assert!(c.keep_going.is_none());
            assert!(c.queued.is_empty());
            assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.stuck"));
        });
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::engine::answer::tests::engine;
    use serde_json::json;

    #[tokio::test]
    async fn an_edit_or_an_answer_clears_the_saved_marker() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        engine.set_time_source(Arc::new(|| 4600.0));
        engine.with(|core| {
            for answer in [false, true] {
                let id = core.blank(Area::Chat, None);
                let c = core.convs.get_mut(&id).unwrap();
                for n in 0..12 {
                    let mut m = Message::new(n.to_string(), Role::Assistant, "waiting".into(), 1_000_000);
                    m.status = Some(Status::Done);
                    c.messages.push(m);
                }
                assert!(matches!(core.dry_gate(&id), Gate::Ask(_)));
                let c = core.convs.get_mut(&id).unwrap();
                if answer {
                    c.messages.push(Message::new("question".into(), Role::User, "Question?".into(), 4_600_001));
                }
                let mut m = Message::new("delivered".into(), Role::Assistant, "Done".into(), 4_600_002);
                m.status = Some(Status::Done);
                if !answer {
                    m.tools = Some(vec![
                        serde_json::from_value(json!({"id":"edit", "name":"Edit", "input":{"file_path":"a.rs"}, "ok":true})).unwrap(),
                    ]);
                }
                c.messages.push(m);
                core.clear_dry_delivery(&id);
                assert!(matches!(core.dry_gate(&id), Gate::Go));
                assert!(core.store.get(&id).unwrap().stuck.is_none());
            }
        });
    }
}
