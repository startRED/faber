//! The code pick remains authoritative. Blind picks are validated and recorded in shadow only.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindCard {
    pub id: String,
    pub text: String,
    pub examples: Vec<String>,
    pub job: String,
    pub conflicts: Vec<String>,
    pub tokens: u32,
    pub prior_lift_pp: f64,
}
pub struct Shortlist {
    pub cards: Vec<BlindCard>,
    mapping: BTreeMap<String, String>,
}

pub fn shortlist(input: &Assembly, seed: u64) -> Result<Shortlist, WireError> {
    let code = assemble(input)?;
    let safety_drops: BTreeSet<_> = code
        .dropped
        .iter()
        .filter(|d| !matches!(d.reason.as_str(), "reason.kit.conflict" | "reason.kit.skillCap" | "reason.kit.tokenCap"))
        .map(|d| d.id.as_str())
        .collect();
    let mut candidates: Vec<_> = input.inventory.cards().iter().filter(|c| !safety_drops.contains(c.source.id.as_str())).collect();
    candidates.sort_by(|a, b| {
        let selected = |c: &Card| code.items.iter().any(|i| i.id == c.source.id);
        selected(b).cmp(&selected(a)).then_with(|| b.prior_lift_pp.total_cmp(&a.prior_lift_pp)).then_with(|| a.source.id.cmp(&b.source.id))
    });
    let mut counts = BTreeMap::new();
    let mut mapping = BTreeMap::new();
    let mut cards = Vec::new();
    for c in candidates {
        let count = counts.entry(c.job.clone()).or_insert(0);
        if *count >= 3 || cards.len() >= 24 {
            continue;
        }
        *count += 1;
        let id = format!("c{}", cards.len());
        mapping.insert(id.clone(), c.source.id.clone());
        cards.push(BlindCard {
            id,
            text: c.blind_text.clone(),
            examples: c.examples.clone(),
            job: c.job.clone(),
            conflicts: vec![],
            tokens: cost(c),
            prior_lift_pp: c.prior_lift_pp,
        });
    }
    for card in &mut cards {
        let original = input.inventory.cards().iter().find(|c| c.source.id == mapping[&card.id]).ok_or(WireError::Malformed)?;
        card.conflicts = mapping.iter().filter(|(_, id)| original.conflicts.contains(id)).map(|(opaque, _)| opaque.clone()).collect();
    }
    cards.sort_by_key(|c| digest(format!("{seed}:{}", c.id).as_bytes()));
    let jobs: BTreeSet<_> = cards.iter().map(|c| c.job.clone()).collect();
    let job_ids: BTreeMap<_, _> = jobs.into_iter().enumerate().map(|(i, job)| (job, format!("j{i}"))).collect();
    for card in &mut cards {
        card.job = job_ids[&card.job].clone();
    }
    Ok(Shortlist { cards, mapping })
}

pub fn validate_shadow(input: &Assembly, code: &Kit, list: &Shortlist, reply: &str, elapsed_ms: u64) -> Result<Shadow, WireError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply {
        ids: Vec<String>,
    }
    if reply.len() > wire::MAX_TEXT {
        return Err(WireError::TooBig);
    }
    let reply: Reply = serde_json::from_str(reply).map_err(|_| WireError::Malformed)?;
    let unique: BTreeSet<_> = reply.ids.iter().collect();
    let ids: Vec<_> = reply.ids.iter().filter_map(|id| list.mapping.get(id).cloned()).collect();
    let chosen: BTreeSet<_> = code.items.iter().map(|i| i.id.clone()).collect();
    let mut valid = elapsed_ms <= 3000 && ids.len() == reply.ids.len() && unique.len() == ids.len() && ids.len() <= 24;
    valid &= input.pins.iter().filter(|p| p.choice == PinChoice::Always).all(|p| ids.contains(&p.id));
    if valid {
        let mut proposed = input.clone();
        proposed.inventory = Inventory::new(input.inventory.cards().iter().filter(|c| ids.contains(&c.source.id)).cloned().collect())?;
        let checked = assemble(&proposed)?;
        valid = checked.items.len() == ids.len() && !checked.gaps.iter().any(|n| n.ends_with('!'));
    }
    let proposed: BTreeSet<_> = ids.iter().cloned().collect();
    Ok(Shadow { ids, valid, disagrees: proposed != chosen, elapsed_ms, extra: Extra::new() })
}
