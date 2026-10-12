//! Kit labels share the existing Auto classification turn. The model-choice table is unchanged.
use super::*;

pub fn request_instructions() -> String {
    format!(
        "{}\nAlso return a labels object with verb (research,plan,design,build,fix,verify,review,write,produce,operate), at most 8 family:value needs (a trailing ! is required), risk (low,medium,high), fresh (stable,recent,unknown), and facets (purpose,audience,deliverable,stage,region). Need families: lang,framework,platform,service,data,artifact,media,domain,op,verify,style,app,ui,growth,commerce,legal,security,infra. Never name a tool or a vendor. Keep labels within 150 additional output tokens. Reuse stated facets; omit unstated ones.",
        crate::route::INSTRUCTIONS
    )
}

pub fn request_labels(answer: &str) -> Result<Labels, WireError> {
    if answer.len() > wire::MAX_TEXT {
        return Err(WireError::TooBig);
    }
    let start = answer.find('{').ok_or(WireError::Malformed)?;
    let end = answer.rfind('}').ok_or(WireError::Malformed)?;
    if start > end {
        return Err(WireError::Malformed);
    }
    let value: serde_json::Value = serde_json::from_str(&answer[start..=end]).map_err(|_| WireError::Malformed)?;
    let mut labels: Labels =
        serde_json::from_value(value.get("labels").cloned().ok_or(WireError::Malformed)?).map_err(|_| WireError::Malformed)?;
    labels.validate()?;
    labels.needs = labels.needs.iter().map(|n| normalize_need(n)).collect::<Result<Vec<_>, _>>()?;
    labels.needs.sort();
    labels.needs.dedup();
    labels.facets.purpose = match labels.facets.purpose.trim().to_ascii_lowercase().as_str() {
        "personal use" | "personal-use" => "personal".into(),
        "client work" | "client-work" => "client".into(),
        "commercial" | "for sale" | "selling" => "sell".into(),
        other => other.into(),
    };
    Ok(labels)
}

pub fn inherit_facets(labels: &mut Labels, saved: &wire::Facets) {
    for (current, previous) in [
        (&mut labels.facets.purpose, &saved.purpose),
        (&mut labels.facets.audience, &saved.audience),
        (&mut labels.facets.deliverable, &saved.deliverable),
        (&mut labels.facets.stage, &saved.stage),
        (&mut labels.facets.region, &saved.region),
    ] {
        if current.is_empty() {
            current.clone_from(previous);
        }
    }
}
