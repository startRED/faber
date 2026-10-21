//! Deep research is a normal Plan turn followed by the plan card's ordinary approved turn.
use super::*;
use crate::persona::Research;

pub(super) fn question(text: &str) -> Option<&str> {
    text.strip_prefix("/research").filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)).map(str::trim_start)
}

/// Existing messages identify the phase after restart too; edits stay in Plan, a later ordinary question ends research.
pub(super) fn phase(messages: &[Message], permissions: Permissions) -> Option<Research> {
    let start = messages.iter().rposition(|m| m.role == Role::User && m.wake.is_none() && question(&m.text).is_some())?;
    let last = messages.iter().rposition(|m| m.role == Role::User)?;
    let approved = messages[start..].iter().position(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.approvedPlan"));
    if let Some(approved) = approved {
        (last == start + approved).then_some(Research::Report)
    } else {
        (permissions == Permissions::Plan).then_some(Research::Plan)
    }
}

impl Core {
    pub(super) fn validate_research(&self, text: &str) -> Result<(), String> {
        let Some(question) = question(text) else { return Ok(()) };
        if question.trim().is_empty() {
            return Err("error.research.question".into());
        }
        let settings = self.store.settings();
        if settings.tuning().web == crate::model::WebSearch::Off {
            return Err("error.research.searchOff".into());
        }
        Ok(())
    }

    pub(super) fn prepare_research(&mut self, text: &str) -> Result<(), String> {
        self.validate_research(text)?;
        if question(text).is_some() && self.store.settings().permissions() != Permissions::Plan {
            self.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() })?;
        }
        Ok(())
    }
}
