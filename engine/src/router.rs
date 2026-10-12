//! Which model answers a turn, and why.

use crate::model::{PaceNote, Vendor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    /// A vendor, or "auto".
    pub vendor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    pub vendor: Vendor,
    pub model: Option<String>,
    pub effort: String,
    pub reason: String,
    /// Why the quota changed the pick, when it did (Auto only).
    pub pace: Option<PaceNote>,
}

/// Medium effort by default: high quality for most work without spending quota on easy turns.
const EFFORT: &str = "medium";

/// ChatGPT without a model runs `codex_recommended`, the account's recommended model (the one ChatGPT on the web
/// uses), so a default someone set by hand in the CLI's own settings never answers by surprise.
// ponytail: fixed default until the measured router (phase F5) replaces it; the reason line is already shown.
pub fn pick_model(choice: &Choice, available: &[Vendor], codex_recommended: Option<&str>) -> Option<Pick> {
    let pick = |vendor, model: Option<&str>, reason: &str| Pick {
        vendor,
        model: model.or(codex_recommended.filter(|_| vendor == Vendor::Codex)).map(String::from),
        effort: EFFORT.into(),
        reason: reason.into(),
        pace: None,
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
