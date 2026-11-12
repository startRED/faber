//! Where a stored conversation stands, read from what is stored alone: the end of its last turn, the state the sidebar, the
//! running list and another agent's read share, and the progress line of a running row. Pure, so the store can build its
//! rows below the engine.

use crate::model::{Area, Conversation, EndKind, Message, Outcome, Plan, Role, Status, StepState, WrapUp};
use crate::sibling::State;

/// The truth about how the last turn ended, whether or not the conversation was archived or queued. A turn whose process
/// exited without an error is not a conclusion: the end is `Done` only when the agent concluded it (a plain answer in a chat
/// is its own conclusion), and `Unconcluded` otherwise. None while the reply still streams, or when no reply is last.
pub fn result_kind(c: &Conversation) -> Option<EndKind> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))?;
    let outcome = reply.wrap_up.as_ref().and_then(WrapUp::outcome);
    Some(if reply.error.is_some() || reply.status == Some(Status::Error) {
        EndKind::Failed
    } else if reply.closed {
        EndKind::Interrupted
    } else if reply.halt.as_deref().is_some_and(|why| !matches!(why, "reason.keepGoing.done" | "reason.keepGoing.silent"))
        || unanswered_question(reply)
        || reply.checks.as_ref().is_some_and(|k| k.state == crate::model::ChecksState::NeedsYou)
        || reply.wrap_up.as_ref().is_some_and(asks)
        || (matches!(outcome, Some(Outcome::Blocked | Outcome::NeedsYou)) && reply.wrap_up.as_ref().is_some_and(|w| w.answered.is_none()))
    {
        EndKind::NeedsYou
    } else if reply.status == Some(Status::Stopped)
        || reply.halt.as_deref() == Some("reason.keepGoing.silent")
        || fresh_plan(c).is_some_and(|p| p.steps.iter().any(|s| s.state != StepState::Done))
        || (needs_conclusion(c) && outcome != Some(Outcome::Done))
        || outcome.is_some_and(|o| o != Outcome::Done)
        || (reply.wrap_up.is_some() && outcome.is_none())
        // A project turn that ended with no words and no conclusion has nothing a person can read as its answer.
        || (c.area == Area::Project && reply.text.trim().is_empty() && outcome.is_none())
    {
        EndKind::Unconcluded
    } else {
        EndKind::Done
    })
}

/// Where the conversation stands now, from what is stored and whether a turn runs (`busy`). The one answer the sidebar, the
/// running list, another agent's read and the command line share.
pub fn run_state(c: &Conversation, busy: bool) -> State {
    let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
    let asking = last.is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
    let working = busy || c.compacting || !c.queued.is_empty() || last.is_some_and(|m| m.status == Some(Status::Streaming));
    match last {
        _ if asking => State::Waiting,
        // Its copy is still being made: the brief follows it.
        _ if c.messages.is_empty() => State::Working,
        _ if working && c.stuck.is_some() => State::Stuck,
        _ if working => State::Working,
        None => State::Unconcluded,
        Some(_) if c.messages.last().is_some_and(|m| m.role == Role::User) => State::Unconcluded,
        Some(m) if m.status == Some(Status::Stopped) && m.closed => State::Interrupted,
        Some(m) if m.status == Some(Status::Stopped) => State::Stopped,
        Some(_) => match result_kind(c) {
            Some(EndKind::Done) => State::Concluded,
            Some(EndKind::NeedsYou) => State::Waiting,
            Some(EndKind::Failed) => State::Failed,
            Some(EndKind::Interrupted) => State::Interrupted,
            Some(EndKind::Unconcluded) | None => State::Unconcluded,
        },
    }
}

/// The plan, when the agent wrote it for the request now under way: a checklist left over from an earlier request says
/// nothing about this one. A request starts at the person's last message; Faber's own wakes belong to the same request.
pub fn fresh_plan(c: &Conversation) -> Option<&Plan> {
    let start = c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()).map_or(0, |m| m.at);
    c.plan.as_ref().filter(|p| p.at >= start)
}

/// Whether the request did work that wants a conclusion: a file was edited, checks ran or a plan was kept. An answer built from
/// reading, searching or a command that changed nothing is its own conclusion. Wakes belong to the same request: a closing
/// reply cannot erase the work before it.
pub fn needs_conclusion(c: &Conversation) -> bool {
    c.area == Area::Project
        && (c.messages.last().is_some_and(|m| m.conclusion_error.is_some())
            || fresh_plan(c).is_some()
            || c.messages.iter().rev().take_while(|m| m.role != Role::User || m.wake.is_some()).any(|m| {
                m.role == Role::Assistant
                    && (m.checks.is_some()
                        || m.tools
                            .as_ref()
                            .is_some_and(|t| t.iter().any(|t| !t.declined && t.ok != Some(false) && !crate::nudge::edited(t).is_empty())))
            }))
}
/// The longest the last progress line of a running row is.
pub const LINE_CHARS: usize = 160;

/// A closing box still waiting for the person's answer.
pub fn asks(w: &WrapUp) -> bool {
    !w.done && w.ask.is_some() && !w.actions.is_empty() && w.answered.is_none()
}

/// Where the agent's work stands, for the running row and the done card: the first sentence of its newest closing box, else
/// what it said it is doing now (task 017), else the last line it wrote, without the marks of a list, a heading or a quote.
pub fn progress_line(messages: &[Message]) -> Option<String> {
    let text = messages.iter().rev().filter(|m| m.role == Role::Assistant).take(3).find_map(|m| {
        if let Some(box_) = &m.wrap_up {
            return Some(crate::progress::first_sentence(&box_.recap));
        }
        // Only while the reply works: once it ended, "Fixing the login page" would read as still at it.
        if m.status == Some(Status::Streaming)
            && let Some(now) = &m.now
        {
            return Some(now.clone());
        }
        m.text
            .lines()
            .rev()
            .map(|l| l.trim_start_matches(|ch: char| ch.is_whitespace() || "#>*-`".contains(ch)).trim_end())
            .find(|l| !l.is_empty())
            .map(str::to_string)
    })?;
    Some(match text.char_indices().nth(LINE_CHARS) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    })
}

/// A question the agent asked the person that nobody has answered: its card still waits, or its call never got a result.
pub fn unanswered_question(m: &Message) -> bool {
    m.tools.iter().flatten().any(|t| t.parent.is_none() && t.name == "AskUserQuestion" && t.ok != Some(true))
        || m.permissions.iter().flatten().any(|p| p.tool == "AskUserQuestion")
}
/// Where a conversation stands; `busy` is whether its CLI runs a turn now.
pub fn state_of(busy: bool, c: &Conversation) -> State {
    if !busy && c.waiting_on.is_some() {
        return State::Waiting;
    }
    let state = run_state(c, busy);
    if matches!(state, State::Working | State::Stuck)
        && !busy
        && !c.compacting
        && c.messages.last().is_some_and(|m| {
            m.role == Role::Assistant && (m.status == Some(Status::Stopped) || (c.stopped && m.status != Some(Status::Streaming)))
        })
    {
        if c.messages.last().is_some_and(|m| m.closed) { State::Interrupted } else { State::Stopped }
    } else {
        state
    }
}
/// The durable projection used by linked conversation rows.
pub fn child_state(c: &Conversation) -> String {
    match state_of(false, c) {
        State::Working | State::Stuck => "working",
        State::Waiting => "waiting",
        State::Concluded => "done",
        State::Failed => "failed",
        State::Stopped | State::Interrupted | State::Unconcluded => "stopped",
    }
    .into()
}

pub fn child_line(c: &Conversation) -> Option<String> {
    progress_line(&c.messages)
}
