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
pub const EFFORT: &str = "medium";

/// The picker's aliases ("sonnet") as the same full IDs as Auto's measured table, so the name an answer shows when it
/// starts is the one the CLI reports a moment later. New models enter through that table, which changes with each Faber release.
pub fn pinned(vendor: Vendor, model: &str) -> &str {
    crate::route::PROFILES
        .iter()
        .find(|p| p.vendor == Vendor::Claude && vendor == Vendor::Claude && p.model.split('-').nth(1) == Some(model))
        .map_or(model, |p| p.model)
}

/// The model a start runs when nothing named one, so that no start leaves the choice to the CLI (rule 2 of
/// docs/product/models.md): Claude's is Auto's everyday model. ChatGPT's comes from the account first: its recommended model,
/// else the first model it lists that Auto's table knows, else the first it lists; the table's own first model only when the
/// account has listed nothing yet.
pub fn default_model(vendor: Vendor, recommended: Option<&str>, listed: &[String]) -> String {
    let table = || crate::route::PROFILES.iter().filter(|p| p.vendor == Vendor::Codex).map(|p| p.model);
    match vendor {
        Vendor::Claude => pinned(Vendor::Claude, "sonnet").into(),
        Vendor::Codex => recommended
            .or_else(|| listed.iter().map(String::as_str).find(|m| table().any(|t| t == *m)))
            .or_else(|| listed.first().map(String::as_str))
            .or_else(|| table().next())
            .unwrap_or_default()
            .into(),
        // Not yet: no model of it is known.
        Vendor::Gemini => String::new(),
    }
}

impl Pick {
    /// The same pick with the model and the effort filled in where it named none, for the start of a CLI session.
    pub fn resolved(&self, recommended: Option<&str>, listed: &[String]) -> Pick {
        let mut pick = self.clone();
        if pick.model.as_deref().is_none_or(str::is_empty) {
            pick.model = Some(default_model(pick.vendor, recommended, listed));
        }
        if pick.effort.is_empty() {
            pick.effort = EFFORT.into();
        }
        pick
    }
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
    use super::{Effort, Pick, Vendor, default_model};

    fn bare(vendor: Vendor) -> Pick {
        Pick { vendor, model: None, effort: String::new(), reason: String::new(), pace: None }
    }

    #[test]
    fn a_start_that_named_no_model_or_effort_gets_the_default_ones() {
        let claude = bare(Vendor::Claude).resolved(None, &[]);
        assert_eq!((claude.model.as_deref(), claude.effort.as_str()), (Some("claude-sonnet-5-5"), "medium"));
        // The account decides for ChatGPT: its recommended model, else the first it lists that Auto knows, else the first it lists;
        // the table's first stands in only before the account has listed anything.
        let listed = |m: &[&str]| m.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(bare(Vendor::Codex).resolved(Some("gpt-6-luna"), &listed(&["gpt-6.1-sol"])).model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(default_model(Vendor::Codex, None, &listed(&["some-new-model", "gpt-6-luna", "gpt-6.1-sol"])), "gpt-6-luna");
        assert_eq!(default_model(Vendor::Codex, None, &listed(&["some-new-model", "another"])), "some-new-model");
        assert_eq!(default_model(Vendor::Codex, None, &[]), "gpt-6.1-sol");
        // A choice made is never replaced.
        let chosen = Pick { model: Some("claude-opus-5-5".into()), effort: "high".into(), ..bare(Vendor::Claude) };
        assert_eq!(chosen.resolved(None, &[]), chosen);
    }

    #[test]
    fn the_screen_starts_a_model_at_the_level_the_engine_does() {
        let wanted = format!("export const DEFAULT_EFFORT: Effort = '{}';", Effort::default().name());
        assert!(include_str!("../../ui/api.ts").contains(&wanted), "ui/api.ts must say: {wanted}");
    }
}
