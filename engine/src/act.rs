use crate::{
    model::{Vendor, Wake},
    router::Pick,
    tools::kits::Report,
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
    pub fn recovery(what: &str, why: &str) -> Self {
        let mut act = Self::new(ActKind::Recovery, what);
        act.why = Some(why.into());
        act
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            ("wake.asked", None),
            ("wake.childDone", None),
            ("wake.unknown", None),
        ] {
            assert_eq!(FaberAct::wake(&Wake::of(key), "sent").why.as_deref(), why, "{key}");
        }
    }
}
