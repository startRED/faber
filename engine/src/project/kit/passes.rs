//! Each pass is a new bounded kit; the runner owns fresh sessions, proof and both-order other-vendor comparisons.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PassKit {
    pub focus: String,
    pub kit: Kit,
    pub fresh_session: bool,
}

pub fn pass_kits(input: &Assembly) -> Result<Vec<PassKit>, WireError> {
    let prototype = matches!(input.labels.facets.purpose.as_str(), "prototype") || input.labels.facets.stage == "prototype";
    let mut focuses = vec!["base"];
    if !prototype {
        let ui = input.labels.needs.iter().any(|n| n.starts_with("ui:") || n.starts_with("style:"));
        let optional: &[&str] = if ui { &["ux", "ui"] } else { &["simplify"] };
        for &focus in optional {
            if input.inventory.cards().iter().any(|c| c.pass_focus.iter().any(|f| f == focus)) {
                focuses.push(focus);
            }
        }
    }
    focuses
        .into_iter()
        .map(|focus| {
            let mut pass = input.clone();
            pass.inventory = Inventory::new(
                input
                    .inventory
                    .cards()
                    .iter()
                    .filter(|c| {
                        c.source.origin == Origin::Project
                            || c.source.form == Form::Check
                            || c.pass_focus.iter().any(|f| f == focus || f == "*")
                    })
                    .cloned()
                    .collect(),
            )?;
            Ok(PassKit { focus: focus.into(), kit: assemble(&pass)?, fresh_session: true })
        })
        .collect()
}
