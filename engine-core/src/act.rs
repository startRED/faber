use crate::{
    model::{KitReport as Report, Vendor, Wake},
    router::Pick,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaberAct {
    pub kind: ActKind,
    pub actor: Actor,
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ActChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActKind {
    Route,
    Kit,
    Wake,
    Recovery,
    Helper,
    /// An agent's `Ask` command ran without the person's yes, because the conversation is in Full Access (R587).
    Reach,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "camelCase")]
pub enum Actor {
    Faber,
    Ai {
        vendor: Vendor,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        role: ActorRole,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorRole {
    Router,
    Helper,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActChange {
    ComposerModel,
    KeepGoing,
    ToolKit,
    Checks,
    Settings,
}
impl FaberAct {
    fn new(kind: ActKind, what: &str) -> Self {
        Self { kind, actor: Actor::Faber, what: what.into(), why: None, params: BTreeMap::new(), change: None, sent: None }
    }
    pub fn wake(wake: &Wake, sent: &str) -> Self {
        let mut act = Self::new(ActKind::Wake, &wake.why);
        act.why = match wake.why.as_str() {
            "wake.keepGoing" => Some("act.why.keepGoing"),
            "wake.asked" => Some("act.why.asked"),
            "wake.childDone" => Some("act.why.childDone"),
            "wake.compacted" => Some("act.why.compacted"),
            "wake.compactFailed" => Some("act.why.compactFailed"),
            _ => None,
        }
        .map(str::to_string);
        if let Some(pr) = wake.pr {
            act.params.insert("pr".into(), pr.to_string());
        }
        if !wake.checks.is_empty() {
            act.params.insert("checks".into(), wake.checks.join(", "));
        }
        act.change = match wake.why.as_str() {
            "wake.keepGoing" => Some(ActChange::KeepGoing),
            "wake.checksFixing" => Some(ActChange::Checks),
            _ => None,
        };
        act.sent = Some(sent.chars().take(4000).collect());
        act
    }
    pub fn route(pick: &Pick, actor: Actor) -> Self {
        let mut act = Self::new(ActKind::Route, "act.route.chose");
        act.actor = actor;
        act.why = Some(pick.reason.clone());
        act.params.insert("vendor".into(), pick.vendor.name().into());
        if let Some(model) = &pick.model {
            act.params.insert("model".into(), model.clone());
        }
        act.params.insert("effort".into(), pick.effort.clone());
        act.change = Some(ActChange::ComposerModel);
        act
    }
    pub fn kit(report: &Report) -> Self {
        let mut act = Self::new(ActKind::Kit, "act.kit.chose");
        act.params.insert("ids".into(), report.ids.join(", "));
        act.change = Some(ActChange::ToolKit);
        act
    }
    /// Faber moved a reply to another subscription (`vendor` is the one it moved to) or waited for quota.
    pub fn recovery(what: &str, vendor: Option<Vendor>) -> Self {
        let mut act = Self::new(ActKind::Recovery, what);
        if let Some(vendor) = vendor {
            act.params.insert("vendor".into(), vendor.name().into());
        }
        act
    }
    /// Faber ran an agent's `Ask` command without asking, because the conversation is in Full Access (R587); `done` is false
    /// when the command failed, which may have changed part of what it touched. `label` is the group the request card would
    /// have named (`host/src/ask.rs`); the line says what was done in Faber's own words, never the agent's.
    pub fn ran_unasked(label: &str, done: bool) -> Self {
        let group = match label {
            "wake" | "settings" | "projects" | "environments" | "rewind" | "panel" => label,
            _ => "organize",
        };
        let what = match (group, done) {
            ("wake", true) => "act.reach.wake",
            ("settings", true) => "act.reach.settings",
            ("projects", true) => "act.reach.projects",
            ("environments", true) => "act.reach.environments",
            ("rewind", true) => "act.reach.rewind",
            ("panel", true) => "act.reach.panel",
            (_, true) => "act.reach.organize",
            ("wake", false) => "act.reach.failed.wake",
            ("settings", false) => "act.reach.failed.settings",
            ("projects", false) => "act.reach.failed.projects",
            ("environments", false) => "act.reach.failed.environments",
            ("rewind", false) => "act.reach.failed.rewind",
            ("panel", false) => "act.reach.failed.panel",
            (_, false) => "act.reach.failed.organize",
        };
        let mut act = Self::new(ActKind::Reach, what);
        act.why = Some("act.why.fullAccess".into());
        act.change = Some(ActChange::Settings);
        act
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_command_run_without_asking_names_its_group_in_faber_s_own_words() {
        let act = FaberAct::ran_unasked("settings", true);
        assert_eq!((act.kind, act.what.as_str(), act.why.as_deref()), (ActKind::Reach, "act.reach.settings", Some("act.why.fullAccess")));
        assert_eq!(act.change, Some(ActChange::Settings), "the person can change the mode from the line");
        assert_eq!(FaberAct::ran_unasked("anything else", true).what, "act.reach.organize");
        assert_eq!(FaberAct::ran_unasked("rewind", false).what, "act.reach.failed.rewind", "a failure is said too");
    }

    #[test]
    fn wake_reasons_are_specific_and_never_repeated() {
        for (key, why) in [
            ("wake.keepGoing", Some("act.why.keepGoing")),
            ("wake.checksFixing", None),
            ("wake.checksFailed", None),
            ("wake.conflict", None),
            ("wake.goalBroke", None),
            ("wake.compacted", Some("act.why.compacted")),
            ("wake.compactFailed", Some("act.why.compactFailed")),
            ("wake.cleared", None),
            ("wake.closed", None),
            ("wake.updated", None),
            ("wake.approvedPlan", None),
            ("wake.asked", Some("act.why.asked")),
            ("wake.childDone", Some("act.why.childDone")),
            ("wake.commandDone", None),
            ("wake.nativeContinuation", None),
            ("wake.carryOn", None),
            ("wake.unknown", None),
        ] {
            assert_eq!(FaberAct::wake(&Wake::of(key), "sent").why.as_deref(), why, "{key}");
        }
    }
}
