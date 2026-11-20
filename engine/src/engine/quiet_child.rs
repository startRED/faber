//! A conversation an agent started that has gone quiet tells that agent (D5 and D8 of the harness study). Its signs used to
//! reach only the screen (`stuck.rs`, `silence.rs`): a child whose test run hung for an hour, or that kept repeating a step,
//! went on while its lead waited for a report that never came. Now the lead gets one line per reply, once:
//! - quiet: a step of its own has said nothing for `QUIET` (30 minutes), or it repeats the same step, or the same step
//!   failed three times with no file changed (the signs `show_stall` puts on the reply);
//! - busy, delivering nothing: its reply has run `BARREN` (an hour) with steps arriving, yet changed no file and passed
//!   no check (D8, checked in the silence look that already runs).
//!
//! It stops nothing: the lead reads it, then stops it, asks it or lets it go on with the tools it has. A held report goes
//! out with it (`report_hold.rs`), since trouble never waits. The notice is a queued item, saved like any; its id is
//! the reply's, so a restart or a second look never sends it twice. Cost: no process, no timer, no tokens while nobody is
//! quiet; a lookup per running child at each half-minute look, and one lead turn per incident.
use super::*;
use crate::model::{StallKind, Wake};

/// Minutes a child's step may say nothing before its lead is told.
pub(super) const QUIET: u32 = 30;
/// Seconds a child's reply may work without changing a file or passing a check before its lead is told.
pub(super) const BARREN: f64 = 3600.0;

/// Why the lead is told.
enum Why {
    Quiet(u32),
    Repeat,
    Failing,
    Barren(u32),
}

impl Core {
    /// The reply's sign changed (`show_stall`): a child's lead hears of a stall once per reply.
    pub(super) fn stall_seen(&mut self, conv: &str, turn: &str, kind: StallKind, minutes: Option<u32>) {
        let why = match (kind, minutes) {
            (StallKind::Quiet, Some(m)) if m >= QUIET => Why::Quiet(m),
            (StallKind::Repeat, _) => Why::Repeat,
            (StallKind::Failing, _) => Why::Failing,
            _ => return,
        };
        self.tell_lead(conv, turn, why);
    }

    /// The silence look, for one running reply: a child busy for an hour that delivered nothing tells its lead once.
    pub(super) fn check_barren(&mut self, conv: &str, turn: &str, now: f64) {
        let Some(started) = self.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn)).map(|l| l.started) else { return };
        if now - started < BARREN || self.convs.get(conv).is_none_or(|c| c.started_by.is_none()) {
            return;
        }
        let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn)) else { return };
        let delivered = m
            .tools
            .iter()
            .flatten()
            .filter(|t| t.ok == Some(true) && !t.declined && !t.interrupted)
            .any(|t| t.check || !crate::nudge::edited(t).is_empty());
        if !delivered {
            self.tell_lead(conv, turn, Why::Barren(((now - started) / 60.0) as u32));
        }
    }

    fn tell_lead(&mut self, conv: &str, turn: &str, why: Why) {
        let Some(c) = self.convs.get(conv) else { return };
        let Some(parent) = c.started_by.clone() else { return };
        let id = format!("quiet-{conv}-{turn}");
        if self.load(&parent).is_none_or(|p| {
            p.archived_at.is_some()
                || p.queued.iter().any(|q| q.id == id)
                || p.messages.iter().any(|m| Wake::carries(&m.id, m.wake.as_ref(), &id))
        }) {
            return;
        }
        let c = &self.convs[conv];
        let step =
            c.messages.iter().find(|m| m.id == turn).and_then(|m| m.tools.iter().flatten().rev().find(|t| t.parent.is_none())).map(|t| {
                match crate::nudge::command(t) {
                    Some(command) => crate::conversation_state::line(command, 80),
                    None => t.name.clone(),
                }
            });
        let what = match why {
            Why::Quiet(m) => format!("has said nothing for {m} minutes while a step of its own runs"),
            Why::Repeat => "keeps starting the same step with no file changed".into(),
            Why::Failing => "failed the same step three times with no file changed".into(),
            Why::Barren(m) => format!("has worked for {m} minutes without changing a file or passing a check"),
        };
        let step = step.map(|s| format!(" (its latest step: {s})")).unwrap_or_default();
        let text = format!(
            "The conversation \"{}\" (id {conv}) that you started {what}{step}. It still runs: Faber stopped nothing. Look at it \
             with `read`, then stop it, ask it with a narrower instruction, or let it go on; tell the person in one line if it \
             changes what they get.",
            c.title
        );
        // Trouble never waits: reports held for the lead go out first, with this right behind them.
        self.flush_hold(&parent);
        let _ = self.send_by_itself_with_id(&parent, text, Wake::of("wake.childQuiet"), id);
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;
    use crate::budget::Billing;
    use crate::cli::Cli;
    use crate::events::Session;
    use serde_json::json;

    /// A lead at rest and one conversation it started, whose reply runs `cargo test` from t = 1000.
    fn child_at_work() -> (Engine, String, String, Arc<Mutex<f64>>, crate::workspace::TestRoot) {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let parent = seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        let kid = seed(&engine, Session::from_fn(|_| {}), &[("work", &[])]);
        let clock = Arc::new(Mutex::new(1000.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            core.ledger.claude.billing = Billing::Subscription;
            core.live.remove(&parent);
            core.convs.get_mut(&parent).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
            let c = core.convs.get_mut(&kid).unwrap();
            c.started_by = Some(parent.clone());
            c.title = "Run the tests".into();
            core.live.get_mut(&kid).unwrap().started = 1000.0;
            core.listen(&kid);
            let tool = EngineEvent::Tool { id: "t".into(), name: "Bash".into(), input: json!({"command": "cargo test"}), parent: None };
            core.handle(&kid, 1, tool);
        });
        (engine, parent, kid, clock, dir)
    }

    fn walk(engine: &Engine, clock: &Mutex<f64>, to: f64, hear: Option<&str>) {
        let mut now = *clock.lock().unwrap();
        while now < to {
            now = (now + 30.0).min(to);
            *clock.lock().unwrap() = now;
            if let Some(kid) = hear {
                engine.with(|c| c.hear(kid, 1));
            }
            engine.check_silence();
        }
    }

    fn notices(engine: &Engine, parent: &str) -> Vec<Message> {
        engine.with(|c| {
            c.convs[parent].messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.childQuiet")).cloned().collect()
        })
    }

    #[tokio::test]
    async fn a_child_whose_command_says_nothing_for_half_an_hour_tells_its_lead_once() {
        let (engine, parent, kid, clock, _dir) = child_at_work();
        walk(&engine, &clock, 1000.0 + 1799.0, None);
        assert!(notices(&engine, &parent).is_empty(), "a long build is not trouble yet");
        walk(&engine, &clock, 1000.0 + 1830.0, None);
        let told = notices(&engine, &parent);
        assert_eq!(told.len(), 1, "half an hour without a word reaches the lead");
        assert!(told[0].text.contains("has said nothing for 30 minutes"), "{}", told[0].text);
        assert!(told[0].text.contains("cargo test"));
        assert!(told[0].text.contains(&kid));
        walk(&engine, &clock, 1000.0 + 2700.0, None);
        assert_eq!(notices(&engine, &parent).len(), 1, "once per reply, not once a minute");
        assert_eq!(engine.with(|c| c.live[&kid].turn.clone()).as_deref(), Some("work"), "nothing was stopped");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_child_busy_for_an_hour_that_delivered_nothing_tells_its_lead() {
        let (engine, parent, kid, clock, _dir) = child_at_work();
        walk(&engine, &clock, 1000.0 + BARREN - 30.0, Some(&kid));
        assert!(notices(&engine, &parent).is_empty(), "it keeps talking, and an hour is not up");
        walk(&engine, &clock, 1000.0 + BARREN + 30.0, Some(&kid));
        let told = notices(&engine, &parent);
        assert_eq!(told.len(), 1);
        assert!(told[0].text.contains("without changing a file or passing a check"), "{}", told[0].text);
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_child_that_changed_a_file_is_not_called_barren() {
        let (engine, parent, kid, clock, _dir) = child_at_work();
        engine.with(|core| {
            let edit = EngineEvent::Tool { id: "e".into(), name: "Edit".into(), input: json!({"file_path": "a.rs"}), parent: None };
            core.handle(&kid, 1, edit);
            core.handle(&kid, 1, EngineEvent::ToolResult { id: "e".into(), ok: true, declined: false, text: String::new() });
        });
        walk(&engine, &clock, 1000.0 + BARREN + 60.0, Some(&kid));
        assert!(notices(&engine, &parent).is_empty());
        engine.shutdown();
    }
}
