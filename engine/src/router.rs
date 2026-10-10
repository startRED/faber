//! Which model answers a turn, and why.

use crate::model::Vendor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    /// A vendor, or "auto".
    pub vendor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub vendor: Vendor,
    pub model: Option<String>,
    pub effort: String,
    pub reason: String,
}

/// Medium effort by default: high quality for most work without spending quota on easy turns.
const EFFORT: &str = "medium";

// ponytail: fixed default until the measured router (phase F5) replaces it; the reason line is already shown.
pub fn pick_model(choice: &Choice, available: &[Vendor]) -> Option<Pick> {
    let pick = |vendor, model: Option<&str>, reason: &str| Pick {
        vendor,
        model: model.map(String::from),
        effort: EFFORT.into(),
        reason: reason.into(),
    };
    if choice.vendor != "auto" {
        let vendor = Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor)?;
        return available.contains(&vendor).then(|| pick(vendor, choice.model.as_deref(), "reason.chosen"));
    }
    if available.contains(&Vendor::Claude) {
        return Some(pick(Vendor::Claude, Some("sonnet"), "reason.autoDefault"));
    }
    if available.contains(&Vendor::Codex) {
        return Some(pick(Vendor::Codex, None, "reason.autoOnly"));
    }
    None
}
