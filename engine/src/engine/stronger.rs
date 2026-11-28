//! A piece that keeps ending with nothing changed (D7 step A of the harness study; task 023 step 4). Faber, not the agent,
//! counts each handed-out conversation's turns in a row that ended without changing a file and without concluding: unconcluded,
//! failed, blocked, or cut by Faber for its silence (`Conversation::empty_ends`, saved, so a restart keeps the count). A
//! person's own Stop is theirs and counts nothing; a turn that changes a file or concludes starts the count again.
//! At the second, the lead's report says so, with the next stronger model of the same AI and how much more quota a turn
//! costs there (`nudge::stronger_model`). The owner shares a subscription, so moving costs his quota only when he chose it:
//! with "Stronger AI if stuck" off (the default) the lead decides, with its own tools, or asks the person; on,
//! Faber moves the piece itself, once, and sends it a narrower instruction (one checkable step). Never on time alone, and
//! never twice: a third empty end only reports.
use super::*;
use crate::model::{Outcome, Wake};
use crate::router::Effort;
use crate::sibling::State;

/// What a piece moved to the stronger model is told, in the CLI's words: one checkable step, then conclude.
const NARROWER: &str = "Faber sends this, not the person: your last two turns on this task ended without changing anything, \
so you now run on a stronger model. Do only the first step of your task that can be checked: make that change, run the check \
that proves it, then end with the <|faber-wrap-up|> JSON block and say what is left.";

impl Core {
    /// Counts the end of `conv`'s latest turn; at the second empty end in a row, the line its report adds (and, with the
    /// setting on, the move itself).
    pub(super) fn empty_end(&mut self, conv: &str, state: State) -> Option<String> {
        let c = self.convs.get(conv)?;
        // Only a project's agents start conversations (`siblings::offers`), so a started one is a handed-out piece.
        c.started_by.as_ref()?;
        let reply = c.messages.iter().rev().find(|m| m.role == Role::Assistant)?;
        let changed = reply
            .tools
            .iter()
            .flatten()
            .any(|t| t.ok == Some(true) && !t.declined && !t.interrupted && !crate::nudge::edited(t).is_empty());
        let empty = !changed
            && match state {
                State::Unconcluded | State::Failed => true,
                State::Stopped => reply.error.as_deref() == Some(silence::KEY),
                State::Waiting => reply.wrap_up.as_ref().and_then(|w| w.outcome()) == Some(Outcome::Blocked),
                _ => false,
            };
        let (id, vendor, model, effort) = (reply.id.clone(), reply.vendor, reply.model.clone(), reply.effort.clone());
        // The person's own Stop is theirs: it neither counts nor starts the count again.
        if state == State::Stopped && reply.error.as_deref() != Some(silence::KEY) {
            return None;
        }
        let c = self.convs.get_mut(conv)?;
        if !empty {
            if !c.empty_ends.is_empty() {
                c.empty_ends.clear();
                self.save(conv);
            }
            return None;
        }
        let fresh = !c.empty_ends.contains(&id);
        if fresh {
            // Only the count matters past the third; the ids keep a reply from counting twice.
            if c.empty_ends.len() >= 3 {
                c.empty_ends.remove(0);
            }
            c.empty_ends.push(id);
            self.save(conv);
        }
        let count = self.convs[conv].empty_ends.len();
        if count < 2 {
            return None;
        }
        let now = Pick {
            vendor: vendor?,
            model,
            effort: effort.unwrap_or_else(|| crate::router::EFFORT.into()),
            reason: String::new(),
            pace: None,
        };
        let can_run = crate::route::candidates(&self.vendors(), &self.codex_models);
        let Some((up, times)) = crate::nudge::stronger_model(&now, &can_run) else {
            return Some(format!(
                "It ended {count} times in a row without changing anything, and no stronger model of the same AI is available: \
                 narrow its task to one checkable step, try the other AI, or ask the person."
            ));
        };
        let name = format!("{} at {} effort", up.model.as_deref().unwrap_or("?"), up.effort);
        if count == 2 && fresh && self.store.settings().stronger_when_stuck() && self.move_up(conv, &up) {
            return Some(format!(
                "It ended twice in a row without changing anything, so Faber moved it to {name} (about {times:.1}x the quota a \
                 turn costs now) with a narrower instruction; its next report comes when that turn ends."
            ));
        }
        Some(format!(
            "It ended {count} times in a row without changing anything. The next step up is {name} on the same AI (about \
             {times:.1}x the quota a turn costs now): start the piece again on that model with one checkable step, or ask \
             the person; the same brief again rarely helps."
        ))
    }

    /// Moves the piece to `up` and sends it the narrower instruction. False when it could not go (the person stopped it, its
    /// lead's day is out of money, no room in its queue): the report then names the step up for the lead to decide.
    fn move_up(&mut self, conv: &str, up: &Pick) -> bool {
        let Some(c) = self.convs.get_mut(conv).filter(|c| !c.stopped && c.archived_at.is_none()) else { return false };
        let before = c.choice.clone();
        let effort = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh].into_iter().find(|e| e.name() == up.effort);
        c.choice = Some(Choice { vendor: up.vendor.name().into(), model: up.model.clone(), effort });
        self.save(conv);
        if self.send_by_itself(conv, NARROWER.into(), Wake::of("wake.stronger")).is_ok() {
            return true;
        }
        if let Some(c) = self.convs.get_mut(conv) {
            c.choice = before;
            self.save(conv);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;
    use crate::budget::Billing;
    use crate::cli::Cli;
    use crate::events::Session;
    use crate::model::Settings;

    /// A project lead and one piece it handed out on Haiku at low effort; each `end` is a turn that changed nothing.
    fn piece(dir: &crate::workspace::TestRoot, on: bool) -> (Engine, String, String) {
        let (engine, _) = engine(dir);
        engine.save_settings(Settings { stronger_when_stuck: Some(on), ..Default::default() }).unwrap();
        let parent = seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        let kid = seed(&engine, Session::from_fn(|_| {}), &[("work-0", &[])]);
        engine.with(|core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            core.ledger.claude.billing = Billing::Subscription;
            core.live.remove(&parent);
            core.convs.get_mut(&parent).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
            let c = core.convs.get_mut(&kid).unwrap();
            c.started_by = Some(parent.clone());
            c.title = "Fix the login".into();
            c.choice = Some(Choice { vendor: "claude".into(), model: Some("claude-haiku-5-5".into()), effort: Some(Effort::Low) });
            core.save(&kid);
        });
        (engine, parent, kid)
    }

    /// The piece's turn `n` ends with words and no file changed, never concluded, and reports to its lead.
    fn end_empty(engine: &Engine, kid: &str, n: usize) {
        engine.with(|core| {
            core.live.remove(kid);
            let c = core.convs.get_mut(kid).unwrap();
            if n > 0 {
                c.messages.push(Message::new(format!("ask-{n}"), Role::User, "Go on.".into(), 10 + n as u64));
                c.messages.push(Message::new(format!("work-{n}"), Role::Assistant, String::new(), 11 + n as u64));
            }
            let reply = c.messages.last_mut().unwrap();
            reply.status = Some(Status::Done);
            reply.vendor = Some(Vendor::Claude);
            reply.model = Some("claude-haiku-5-5".into());
            reply.effort = Some("low".into());
            reply.text = "This looks risky, so I stopped before changing anything.".into();
            // Blocked, as a worker that stops short says it.
            reply.wrap_up = Some(serde_json::from_value(serde_json::json!({ "recap": "Stopped: risky.", "outcome": "blocked" })).unwrap());
            core.tell_parent(kid);
        });
    }

    fn reports(engine: &Engine, parent: &str) -> Vec<String> {
        engine.with(|c| {
            let p = &c.convs[parent];
            p.messages
                .iter()
                .map(|m| (m.wake.as_ref(), &m.text))
                .chain(p.queued.iter().map(|q| (q.wake.as_ref(), &q.text)))
                .filter(|(w, _)| w.is_some_and(|w| w.why == "wake.childDone"))
                .map(|(_, t)| t.clone())
                .collect()
        })
    }

    #[tokio::test]
    async fn the_second_empty_end_names_the_stronger_model_and_its_cost_and_the_lead_decides() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kid) = piece(&dir, false);
        end_empty(&engine, &kid, 0);
        assert!(!reports(&engine, &parent)[0].contains("step up"), "one empty end is not a pattern");
        end_empty(&engine, &kid, 1);
        let second = reports(&engine, &parent).pop().unwrap();
        assert!(second.contains("ended 2 times in a row without changing anything"), "{second}");
        assert!(second.contains("The next step up is claude-"), "{second}");
        assert!(second.contains("x the quota a turn costs now"), "{second}");
        assert_eq!(
            engine.with(|c| c.convs[&kid].choice.as_ref().and_then(|ch| ch.model.clone())).as_deref(),
            Some("claude-haiku-5-5"),
            "off by default: nothing moves by itself"
        );
        assert_eq!(engine.with(|c| c.store.get(&kid).unwrap().empty_ends.len()), 2, "the count is saved, so a restart keeps it");
        engine.shutdown();
    }

    #[tokio::test]
    async fn with_the_setting_on_the_piece_moves_once_with_a_narrower_instruction() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kid) = piece(&dir, true);
        end_empty(&engine, &kid, 0);
        end_empty(&engine, &kid, 1);
        let second = reports(&engine, &parent).pop().unwrap();
        assert!(second.contains("so Faber moved it to claude-"), "{second}");
        let (model, told) = engine.with(|c| {
            let k = &c.convs[&kid];
            let told = k
                .messages
                .iter()
                .any(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.stronger") && m.text.contains("Do only the first step"))
                || k.queued.iter().any(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.stronger"));
            (k.choice.as_ref().and_then(|ch| ch.model.clone()), told)
        });
        assert_ne!(model.as_deref(), Some("claude-haiku-5-5"), "the piece now runs on the stronger model");
        assert!(told, "and it got the narrower instruction");
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_persons_stop_neither_counts_nor_clears_the_count() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _, kid) = piece(&dir, false);
        end_empty(&engine, &kid, 0);
        engine.with(|core| {
            assert_eq!(core.empty_end(&kid, State::Stopped), None);
            assert_eq!(core.convs[&kid].empty_ends.len(), 1, "the person's Stop leaves the count where it was");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_turn_that_changes_a_file_starts_the_count_again() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kid) = piece(&dir, false);
        end_empty(&engine, &kid, 0);
        engine.with(|core| {
            let c = core.convs.get_mut(&kid).unwrap();
            c.messages.push(Message::new("ask-1".into(), Role::User, "Go on.".into(), 11));
            let mut reply = Message::new("work-1".into(), Role::Assistant, "Changed a.rs.".into(), 12);
            reply.status = Some(Status::Done);
            reply.tools = Some(vec![
                serde_json::from_value(serde_json::json!({
                    "id": "e", "name": "Edit", "input": { "file_path": "a.rs" }, "ok": true
                }))
                .unwrap(),
            ]);
            c.messages.push(reply);
            core.tell_parent(&kid);
        });
        end_empty(&engine, &kid, 2);
        assert!(reports(&engine, &parent).iter().all(|r| !r.contains("step up")), "the edit broke the run of empty ends");
        engine.shutdown();
    }
}
