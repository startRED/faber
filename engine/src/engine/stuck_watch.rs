//! Stuck subagents (R161 step 6): the engine side of `stuck.rs`. Each subagent call of the running reply is followed through
//! the steps the CLI reports under it, and its sign goes on the call itself, so the reply's row says it and the running list
//! counts a stuck one as needing the person. A timer looks again while a subagent works, because silence brings no event.
//! One sent to the background is followed the same way while the reply that sent it runs. The main agent's own steps are
//! followed too (task 110): the same step many times, or failing with nothing changed, puts a sign on the reply itself
//! (`Message::stall`), beside the one for a step that says nothing (`silence.rs`). A sign never stops anything.
use super::*;
use crate::stuck::{self, Child, Sign};

/// How often a reply with a subagent at work is looked at again; silence is counted in minutes.
const EVERY: Duration = Duration::from_secs(15);

/// The steps of one conversation's running reply.
#[derive(Default)]
pub(super) struct Watched {
    turn: String,
    /// The process that runs the reply: a reply that moved to a new one (another AI after a quota stop) left these subagents behind.
    sid: u64,
    children: HashMap<String, Child>,
    /// The main agent's own steps, once it ran one.
    main: Option<Child>,
    /// The timer that looks again: a newer one replaces an older one, which then ends.
    ticker: u64,
}

/// A call that hands work to a subagent, of the agent's own CLI or a helper from the other one, in the foreground or the
/// background (whose call answers at once and stays open until its task notice).
fn is_subagent_call(tool: &ToolUse) -> bool {
    tool.parent.is_none() && matches!(tool.name.as_str(), "Agent" | "Task" | "Helper")
}

/// Whether a question is about this step. Claude Code asks by the tool's own name; Codex by its kind (`command`, `file`)
/// for a `commandExecution` or `fileChange` step. Both carry the step's input.
fn asks_about(question: &Permission, step: &ToolUse) -> bool {
    let same = question.tool == step.name
        || matches!((question.tool.as_str(), step.name.as_str()), ("command", "commandExecution") | ("file" | "fileChange", "fileChange"));
    step.ok.is_none() && same && question.input == step.input
}

impl Core {
    /// A step started in the running reply: a new subagent to follow, one more step of one, or one of the main agent's own.
    pub(super) fn watch_started(&mut self, conv: &str, id: &str) {
        let now = self.secs();
        let Some((turn, sid)) = self.live.get(conv).and_then(|l| Some((l.turn.clone()?, l.sid))) else { return };
        let Some(tools) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn)).and_then(|m| m.tools.as_deref())
        else {
            return;
        };
        let Some(tool) = tools.iter().rev().find(|t| t.id == id) else { return };
        let watched = self.watched.entry(conv.to_string()).or_default();
        if watched.turn != turn || watched.sid != sid {
            *watched = Watched { turn, sid, ..Watched::default() };
        }
        let subagent = is_subagent_call(tool);
        if subagent {
            watched.children.insert(id.to_string(), Child::new(now));
        } else if tool.parent.is_none() {
            watched.main.get_or_insert_with(|| Child::new(now)).started(tool, now);
        } else if let Some(child) = stuck::root(tools, tool).and_then(|root| watched.children.get_mut(root)) {
            child.started(tool, now);
        }
        if subagent {
            self.look_again(conv);
        }
        self.weigh(conv);
    }

    /// A step of the running reply ended: a subagent's call (it is done, and followed no more), one of its steps, or one of the
    /// main agent's own.
    pub(super) fn watch_ended(&mut self, conv: &str, id: &str) {
        let now = self.secs();
        let Some(watched) = self.watched.get_mut(conv) else { return };
        let Some(tools) =
            self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == watched.turn)).and_then(|m| m.tools.as_mut())
        else {
            return;
        };
        if watched.children.remove(id).is_some() {
            if let Some(call) = tools.iter_mut().find(|t| t.id == id) {
                call.sign = None;
            }
        } else if let Some(tool) = tools.iter().find(|t| t.id == id) {
            if tool.parent.is_none() {
                if let Some(main) = watched.main.as_mut() {
                    main.ended(tool, now);
                }
            } else if let Some(child) = stuck::root(tools, tool).and_then(|root| watched.children.get_mut(root)) {
                child.ended(tool, now);
                // A helper that changed a file changed it for the main agent too.
                if stuck::Child::edits(tool)
                    && let Some(main) = watched.main.as_mut()
                {
                    main.changed();
                }
            }
        }
        self.weigh(conv);
    }

    /// Puts each followed subagent's sign on its call and the main agent's on the reply, and tells the screen when one changed.
    /// A question the person has not answered stops the clock of the subagent that asked it, and only that one's: another
    /// that hangs meanwhile is still seen. A question no open step names belongs to the only subagent at work; with several,
    /// nobody can tell which asked, so all of them wait while it does, and their other signs still show.
    pub(super) fn weigh(&mut self, conv: &str) {
        let now = self.secs();
        let current = self.live.get(conv).and_then(|l| Some((l.turn.clone()?, l.sid)));
        if self.watched.get(conv).is_some_and(|w| current.as_ref() != Some(&(w.turn.clone(), w.sid))) {
            return self.unwatch(conv);
        }
        let Some(watched) = self.watched.get_mut(conv) else { return };
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == watched.turn)) else { return };
        let questions = m.permissions.as_deref().unwrap_or_default();
        let tools = m.tools.as_deref().unwrap_or_default();
        // The step a question is about is the open step it names (`asks_about`); one of the main agent's own steps belongs to
        // no subagent.
        let mut askers = HashSet::new();
        let mut unplaced = false;
        for q in questions {
            match tools.iter().rev().find(|t| asks_about(q, t)) {
                Some(step) => askers.extend(stuck::root(tools, step).map(str::to_string)),
                None => unplaced = true,
            }
        }
        if unplaced && watched.children.len() == 1 {
            askers.extend(watched.children.keys().cloned());
        }
        let unsure = unplaced && watched.children.len() > 1;
        let mut signs = Vec::with_capacity(watched.children.len());
        for (call, child) in &mut watched.children {
            let sign = if askers.contains(call) {
                child.asked(now);
                Some(Sign::Waiting)
            } else {
                if unsure {
                    child.asked(now);
                }
                child.sign(now)
            };
            signs.push((call.clone(), sign));
        }
        let mut changed = false;
        for (call, sign) in signs {
            if let Some(tool) = m.tools.iter_mut().flatten().find(|t| t.id == call)
                && tool.sign != sign
            {
                tool.sign = sign;
                changed = true;
            }
        }
        let turn = watched.turn.clone();
        if !self.show_stall(conv) && changed {
            self.push(conv, Some(&turn), true);
        }
    }

    /// Puts the main agent's own sign on its running reply: its steps first (the same step again and again, or failing),
    /// then a step that has said nothing for long (`silence.rs`). True when it changed, and the screen was told.
    pub(super) fn show_stall(&mut self, conv: &str) -> bool {
        let Some(turn) = self.live.get(conv).and_then(|l| l.turn.clone()) else { return false };
        let own = self
            .watched
            .get(conv)
            .filter(|w| w.turn == turn)
            .and_then(|w| w.main.as_ref())
            .and_then(Child::main_sign)
            .map(|sign| Stall { kind: if sign == Sign::Failing { StallKind::Failing } else { StallKind::Repeat }, minutes: None });
        let stall = own.or_else(|| self.quiet_for(conv, &turn).map(|minutes| Stall { kind: StallKind::Quiet, minutes: Some(minutes) }));
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn)) else { return false };
        if m.stall == stall {
            return false;
        }
        m.stall = stall;
        self.push(conv, Some(&turn), true);
        true
    }

    /// The reply ended: nothing is followed, and no sign outlives it.
    pub(super) fn unwatch(&mut self, conv: &str) {
        let Some(watched) = self.watched.remove(conv) else { return };
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == watched.turn)) {
            m.tools.iter_mut().flatten().for_each(|t| t.sign = None);
            m.stall = None;
        }
    }

    /// Looks at the conversation again every few seconds while a subagent of its running reply works.
    fn look_again(&mut self, conv: &str) {
        let ticker = self.next_sid;
        self.next_sid += 1;
        let Some(watched) = self.watched.get_mut(conv) else { return };
        watched.ticker = ticker;
        let (me, conv) = (self.me.clone(), conv.to_string());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(EVERY).await;
                let Some(core) = me.upgrade() else { return };
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if !core.watched.get(&conv).is_some_and(|w| w.ticker == ticker && !w.children.is_empty()) {
                    return;
                }
                core.weigh(&conv);
            }
        });
    }
}

impl Engine {
    /// Looks at every running reply's subagents now, as the timer does every few seconds; tests call it after moving the clock.
    pub fn weigh_subagents(&self) {
        self.with(|c| {
            let convs: Vec<String> = c.watched.keys().cloned().collect();
            for conv in convs {
                c.weigh(&conv);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_names_its_step_by_tool_or_by_codex_kind_and_input() {
        let line = serde_json::json!("cargo build");
        let step = |name: &str, ok: Option<bool>| ToolUse {
            id: "s".into(),
            name: name.into(),
            input: line.clone(),
            ok,
            output: None,
            declined: false,
            interrupted: false,
            parent: Some("call".into()),
            sign: None,
            facts: None,
            told: vec![],
            words: vec![],
            words_cut: false,
            words_chars: None,
            at: None,
            transcript: None,
            change: None,
            check: false,
            started: None,
            ended: None,
        };
        let ask = |tool: &str, input: serde_json::Value| Permission { request_id: "0".into(), tool: tool.into(), input, epoch: None };
        assert!(asks_about(&ask("command", line.clone()), &step("commandExecution", None)), "Codex asks by kind");
        assert!(asks_about(&ask("Bash", line.clone()), &step("Bash", None)));
        assert!(!asks_about(&ask("command", line.clone()), &step("commandExecution", Some(true))), "a finished step asks nothing");
        assert!(!asks_about(&ask("command", serde_json::json!("cargo test")), &step("commandExecution", None)));
        assert!(!asks_about(&ask("file", line.clone()), &step("commandExecution", None)));
    }
}
