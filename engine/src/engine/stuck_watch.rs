//! Stuck subagents (R161 step 6): the engine side of `stuck.rs`. Each subagent call of the running reply is followed through
//! the steps the CLI reports under it, and its sign goes on the call itself, so the reply's row says it and the running list
//! counts a stuck one as needing the person. A timer looks again while a subagent works, because silence brings no event.
use super::*;
use crate::stuck::{self, Child, Sign};

/// How often a reply with a subagent at work is looked at again; silence is counted in minutes.
const EVERY: Duration = Duration::from_secs(15);

/// The subagents of one conversation's running reply.
#[derive(Default)]
pub(super) struct Watched {
    turn: String,
    /// The process that runs the reply: a reply that moved to a new one (another AI after a quota stop) left these subagents behind.
    sid: u64,
    children: HashMap<String, Child>,
    /// The timer that looks again: a newer one replaces an older one, which then ends.
    ticker: u64,
}

/// A call that hands work to a subagent, of the agent's own CLI or a helper from the other one. One sent to the background
/// answers its call at once and is not followed.
fn is_subagent_call(tool: &ToolUse) -> bool {
    tool.parent.is_none() && matches!(tool.name.as_str(), "Agent" | "Task" | "Helper") && !runs_in_background(tool)
}

/// Whether a question is about this step. Claude Code asks by the tool's own name; Codex by its kind (`command`, `file`)
/// for a `commandExecution` or `fileChange` step. Both carry the step's input.
fn asks_about(question: &Permission, step: &ToolUse) -> bool {
    let same = question.tool == step.name
        || matches!((question.tool.as_str(), step.name.as_str()), ("command", "commandExecution") | ("file" | "fileChange", "fileChange"));
    step.ok.is_none() && same && question.input == step.input
}

impl Core {
    /// A step started in the running reply: a new subagent to follow, or one more step of one.
    pub(super) fn watch_started(&mut self, conv: &str, id: &str) {
        let now = self.secs();
        let Some((turn, sid)) = self.live.get(conv).and_then(|l| Some((l.turn.clone()?, l.sid))) else { return };
        let Some(tools) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn)).and_then(|m| m.tools.as_deref())
        else {
            return;
        };
        let Some(tool) = tools.iter().rev().find(|t| t.id == id) else { return };
        if is_subagent_call(tool) {
            let watched = self.watched.entry(conv.to_string()).or_default();
            if watched.turn != turn || watched.sid != sid {
                *watched = Watched { turn, sid, ..Watched::default() };
            }
            watched.children.insert(id.to_string(), Child::new(now));
            self.look_again(conv);
        } else if let Some(root) = stuck::root(tools, tool)
            && let Some(child) =
                self.watched.get_mut(conv).filter(|w| w.turn == turn && w.sid == sid).and_then(|w| w.children.get_mut(root))
        {
            child.started(tool, now);
        }
        self.weigh(conv);
    }

    /// A step of the running reply ended: a subagent's call (it is done, and followed no more) or one of its steps.
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
        } else if let Some(tool) = tools.iter().find(|t| t.id == id)
            && let Some(child) = stuck::root(tools, tool).and_then(|root| watched.children.get_mut(root))
        {
            child.ended(tool, now);
        }
        self.weigh(conv);
    }

    /// Puts each followed subagent's sign on its call, and tells the screen when one changed. A question the person has not
    /// answered stops every clock: the subagent that asked it waits for them, and the others are not counted meanwhile.
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
        // The step a question is about is the open step it names (`asks_about`); a question none of them matches is the
        // only subagent's, when only one works.
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
        let mut signs = Vec::with_capacity(watched.children.len());
        for (call, child) in &mut watched.children {
            let sign = if askers.contains(call) {
                child.asked(now);
                Some(Sign::Waiting)
            } else if !questions.is_empty() {
                child.asked(now);
                None
            } else {
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
        if changed {
            let turn = watched.turn.clone();
            self.push(conv, Some(&turn), true);
        }
    }

    /// The reply ended: nothing is followed, and no sign outlives it.
    pub(super) fn unwatch(&mut self, conv: &str) {
        let Some(watched) = self.watched.remove(conv) else { return };
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == watched.turn)) {
            m.tools.iter_mut().flatten().for_each(|t| t.sign = None);
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
            parent: Some("call".into()),
            sign: None,
            facts: None,
            told: vec![],
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
