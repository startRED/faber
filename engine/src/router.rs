//! Which model answers a turn, and why.

use crate::model::{PaceNote, Vendor};
use serde::{Deserialize, Serialize};

/// The default is the screen's too (`DEFAULT_EFFORT` in `ui/api.ts`, which shows it before the engine sees the choice);
/// a test below keeps the two the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

impl Effort {
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    /// A vendor, or "auto".
    pub vendor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
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

/// The picker's aliases ("sonnet") as the same full IDs as Auto's measured table, so the name an answer shows when it
/// starts is the one the CLI reports a moment later. New models enter through that table, which changes with each Faber release.
fn pinned(vendor: Vendor, model: &str) -> &str {
    crate::route::PROFILES
        .iter()
        .find(|p| p.vendor == Vendor::Claude && vendor == Vendor::Claude && p.model.split('-').nth(1) == Some(model))
        .map_or(model, |p| p.model)
}

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
        return available.contains(&vendor).then(|| {
            let model = choice.model.as_deref().map(|model| pinned(vendor, model));
            let mut chosen = pick(vendor, model, "reason.chosen");
            chosen.effort = choice.effort.unwrap_or_default().name().into();
            chosen
        });
    }
    if available.contains(&Vendor::Claude) {
        return Some(pick(Vendor::Claude, Some(pinned(Vendor::Claude, "sonnet")), "reason.autoDefault"));
    }
    if available.contains(&Vendor::Codex) {
        return Some(pick(Vendor::Codex, None, "reason.autoOnly"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::Effort;

    #[test]
    fn the_screen_starts_a_model_at_the_level_the_engine_does() {
        let wanted = format!("export const DEFAULT_EFFORT: Effort = '{}';", Effort::default().name());
        assert!(include_str!("../../ui/api.ts").contains(&wanted), "ui/api.ts must say: {wanted}");
    }
}
