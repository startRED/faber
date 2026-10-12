//! On-the-fly kits: pure assembly, fixture-friendly inventory/delivery, no installs or admission authority.
pub mod delivery;
pub mod gaps;
pub mod inventory;
pub mod measure;
pub mod passes;
pub mod picker;
pub mod request;
pub use delivery::*;
pub use gaps::*;
pub use inventory::*;
pub use measure::*;
pub use passes::*;
pub use picker::*;
pub use request::*;

use super::{Extra, Record, Role, SCHEMA, valid_id, wire};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use wire::{Fresh, Labels, Verb, WireError, normalize_need};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinChoice {
    Always,
    Never,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub id: String,
    pub choice: PinChoice,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub version: String,
    pub kept: u64,
    pub discarded: u64,
}

#[derive(Debug, Clone)]
pub struct Assembly {
    pub labels: Labels,
    pub fingerprint: Vec<String>,
    pub inventory: Inventory,
    pub evidence: Vec<Evidence>,
    pub pins: Vec<Pin>,
    pub role: Role,
    pub vendor: Vendor,
    pub model: String,
    pub effort: String,
    pub mandatory_proofs: Vec<String>,
    pub machine: Machine,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryMode {
    Inline,
    Proof,
    ReviewLens,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitItem {
    pub id: String,
    pub version: String,
    pub sha256: String,
    pub stage: Stage,
    pub delivery: DeliveryMode,
    pub why: String,
    pub resident_tokens: u32,
    pub job: String,
    pub source_path: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dropped {
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Kit {
    pub labels: Labels,
    pub needs: Vec<String>,
    pub items: Vec<KitItem>,
    pub proofs: Vec<String>,
    pub review_lenses: Vec<String>,
    pub gaps: Vec<String>,
    pub dropped: Vec<Dropped>,
    pub role: Role,
    pub vendor: Vendor,
    pub model: String,
    pub effort: String,
    pub inventory_hash: String,
    pub resident_tokens: u32,
    pub token_cap: u32,
    pub over_budget_pin: bool,
    pub assemble_micros: u64,
}
impl Kit {
    pub fn validate(&self) -> Result<(), WireError> {
        self.labels.validate()?;
        validate_model(self.vendor, &self.model, &self.effort)?;
        let cap = match self.role {
            Role::Lead => 6000,
            Role::Reviewer => 3000,
            Role::Research => 1000,
            _ => 4000,
        };
        if self.token_cap != cap
            || self.resident_tokens > cap && !self.over_budget_pin
            || self.items.len() > 64
            || self.proofs.len() > 64
            || self.needs.len() > 256
            || self.gaps.len() > 256
            || self.dropped.len() > 10000
        {
            return Err(WireError::TooBig);
        }
        let mut ids = BTreeSet::new();
        let mut total = 0u32;
        for item in &self.items {
            wire::check_id(&item.id)?;
            if !ids.insert(&item.id) || item.version.is_empty() || item.version.len() > 128 || item.why.len() > wire::MAX_TEXT {
                return Err(WireError::Malformed);
            }
            wire::ArtifactRef { path: "source".into(), sha256: item.sha256.clone() }.validate()?;
            if let Some(path) = &item.source_path {
                scoped_path(path)?;
            }
            if item.delivery != DeliveryMode::Proof && item.source_path.is_none() {
                return Err(WireError::Path);
            }
            total = total.checked_add(item.resident_tokens).ok_or(WireError::TooBig)?;
        }
        if total != self.resident_tokens {
            return Err(WireError::Malformed);
        }
        for need in self.needs.iter().chain(&self.gaps) {
            normalize_need(need)?;
        }
        for id in &self.proofs {
            wire::check_id(id)?;
        }
        wire::ArtifactRef { path: "inventory".into(), sha256: self.inventory_hash.clone() }.validate()?;
        Ok(())
    }
}

fn validate_model(vendor: Vendor, model: &str, effort: &str) -> Result<(), WireError> {
    let prefix = match vendor {
        Vendor::Claude => "claude-",
        Vendor::Codex => "gpt-",
    };
    if !model.starts_with(prefix)
        || model.len() <= prefix.len()
        || model.len() > 128
        || !model.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        || !matches!(effort, "low" | "medium" | "high" | "xhigh" | "max")
    {
        return Err(WireError::Malformed);
    }
    Ok(())
}

fn strength(pattern: &str, need: &str) -> f64 {
    let need = need.trim_end_matches('!');
    if pattern == need {
        1.0
    } else if pattern.strip_suffix('*').is_some_and(|family| need.starts_with(family)) {
        0.5
    } else {
        0.0
    }
}
fn cost(c: &Card) -> u32 {
    if c.source.form == Form::Check { 0 } else { c.source.core_tokens }
}
fn beta(c: &Card, evidence: &[Evidence]) -> f64 {
    evidence
        .iter()
        .find(|e| e.id == c.source.id && e.version == c.source.version)
        .map_or(0.5, |e| (e.kept as f64 + 1.0) / (e.kept as f64 + e.discarded as f64 + 2.0))
}
fn utility(c: &Card, needs: &BTreeMap<String, f64>) -> f64 {
    needs.iter().map(|(n, w)| c.serves.iter().map(|s| strength(s, n)).fold(0.0, f64::max) * w * c.prior_lift_pp).sum::<f64>()
        - f64::from(cost(c)) / 1000.0
}
fn purpose_needs(purpose: &str) -> &'static [&'static str] {
    match purpose {
        "sell" => &[
            "growth:positioning",
            "growth:landing",
            "growth:seo",
            "growth:analytics",
            "growth:email",
            "growth:launch",
            "commerce:pricing",
            "commerce:payments",
            "commerce:tax-invoice",
            "legal:terms-privacy",
            "legal:consumer",
            "infra:monitoring",
            "security:hardening",
        ],
        "client" | "client work" => &["artifact:handover", "artifact:manual", "infra:handover"],
        "internal team" => &["security:auth", "security:audit-log", "infra:deploy"],
        "open source" => &["legal:oss-license", "artifact:contributing", "artifact:changelog", "infra:ci"],
        "learning" => &["domain:teaching"],
        _ => &[],
    }
}

/// Priors select; Beta outcomes break ties only. A kit can never grant native tools, network or process authority.
pub fn assemble(input: &Assembly) -> Result<Kit, WireError> {
    let start = Instant::now();
    input.labels.validate()?;
    if input.fingerprint.len() > 128 || input.pins.len() > 64 || input.evidence.len() > 10000 {
        return Err(WireError::TooBig);
    }
    validate_model(input.vendor, &input.model, &input.effort)?;
    let mut needs = BTreeMap::new();
    let mut pin_ids = BTreeSet::new();
    for pin in &input.pins {
        wire::check_id(&pin.id)?;
        if !pin_ids.insert(&pin.id) {
            return Err(WireError::Malformed);
        }
    }
    for raw in &input.fingerprint {
        let n = normalize_need(raw)?;
        let weight = if n.ends_with('!') { 0.5 } else { 0.3 };
        needs.insert(n, weight);
    }
    for raw in purpose_needs(&input.labels.facets.purpose) {
        needs.insert(raw.to_string(), 0.6);
    }
    for raw in &input.labels.needs {
        let n = normalize_need(raw)?;
        if n.ends_with('!') {
            needs.remove(n.trim_end_matches('!'));
        }
        let weight = if n.ends_with('!') { 1.0 } else { 0.6 };
        needs.insert(n, weight);
    }
    let cap = match input.role {
        Role::Lead => 6000,
        Role::Reviewer => 3000,
        Role::Research => 1000,
        _ => 4000,
    };
    let mut dropped = Vec::new();
    let mut candidates = Vec::new();
    let query: BTreeSet<_> = needs.keys().flat_map(|n| n.trim_end_matches('!').split([':', '-', '_', '.'])).map(str::to_string).collect();
    let documents = &input.inventory.documents;
    let average = input.inventory.average_words;
    let frequencies: BTreeMap<_, _> = query.iter().map(|term| (term, documents.iter().filter(|d| d.contains(term)).count())).collect();
    for (index, c) in input.inventory.cards.iter().enumerate() {
        let pin = input.pins.iter().find(|p| p.id == c.source.id).map(|p| &p.choice);
        let blocked = if pin == Some(&PinChoice::Never) {
            Some("reason.kit.pinnedNever")
        } else if c.job.starts_with("process.") {
            Some("reason.kit.processOwned")
        } else if c.source.form == Form::Check && (c.source.origin != Origin::Engine || c.stage != Stage::After) {
            Some("reason.kit.unverifiedCheck")
        } else if c.source.form != Form::Check
            && (c.source.runs_code
                || c.source.sends_data != SendsData::None
                || matches!(c.source.form, Form::Mcp | Form::Plugin | Form::CliFeature)
                || !c.source.inline_available
                || c.source.path.is_none())
        {
            Some("reason.kit.inlineUnavailable")
        } else if !c.verbs.contains(&input.labels.verb) {
            Some("reason.kit.verb")
        } else if (!c.requirements.os.is_empty() && !c.requirements.os.contains(&input.machine.os))
            || !c.requirements.apps.iter().all(|app| input.machine.apps.contains(app))
            || c.requirements.gpu_gb > input.machine.gpu_gb
        {
            Some("reason.kit.requirements")
        } else if !c.purposes.is_empty() && !c.purposes.iter().any(|p| p == "*" || p == &input.labels.facets.purpose) {
            Some("reason.kit.purpose")
        } else {
            None
        };
        if let Some(reason) = blocked {
            dropped.push(Dropped { id: c.source.id.clone(), reason: reason.into() });
            continue;
        }
        let u = utility(c, &needs);
        if !u.is_finite() || u <= 0.0 && pin != Some(&PinChoice::Always) {
            dropped.push(Dropped { id: c.source.id.clone(), reason: "reason.kit.utility".into() });
            continue;
        }
        let document = &documents[index];
        let example_score = query
            .iter()
            .map(|term| {
                let tf = document.iter().filter(|w| *w == term).count() as f64;
                let df = frequencies[term] as f64;
                let idf = (1.0 + (documents.len() as f64 - df + 0.5) / (df + 0.5)).ln();
                idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * document.len() as f64 / average.max(1.0)))
            })
            .sum::<f64>();
        candidates.push((c, u, pin == Some(&PinChoice::Always), example_score));
    }
    // Project ownership is evaluated before packing, so a cheap third-party style cannot steal its slot.
    let project_jobs: BTreeSet<_> = candidates
        .iter()
        .filter(|(c, _, _, _)| c.source.origin == Origin::Project && c.exclusive)
        .map(|(c, _, _, _)| c.job.as_str())
        .collect();
    candidates.sort_by(|(a, ua, pa, ea), (b, ub, pb, eb)| {
        pb.cmp(pa)
            .then_with(|| (ub / f64::from(cost(b).max(1))).total_cmp(&(ua / f64::from(cost(a).max(1)))))
            .then_with(|| eb.total_cmp(ea))
            .then_with(|| beta(b, &input.evidence).total_cmp(&beta(a, &input.evidence)))
            .then_with(|| a.source.id.cmp(&b.source.id))
    });
    let mut selected: Vec<&Card> = Vec::new();
    let mut resident = 0_u32;
    let mut during = 0;
    let mut over_budget_pin = false;
    for (c, _, pinned, _) in candidates {
        let reason = if !pinned && c.source.origin != Origin::Project && project_jobs.contains(c.job.as_str()) {
            Some("reason.kit.projectOwnsJob")
        } else if selected.iter().any(|s| {
            (s.job == c.job && (s.exclusive || c.exclusive)) || s.conflicts.contains(&c.source.id) || c.conflicts.contains(&s.source.id)
        }) {
            Some("reason.kit.conflict")
        } else if c.stage == Stage::During && c.source.form == Form::Skill && during >= 4 {
            Some("reason.kit.skillCap")
        } else if resident.checked_add(cost(c)).is_none_or(|n| n > cap) && !pinned {
            Some("reason.kit.tokenCap")
        } else {
            None
        };
        if let Some(reason) = reason {
            dropped.push(Dropped { id: c.source.id.clone(), reason: reason.into() });
            continue;
        }
        resident = resident.checked_add(cost(c)).ok_or(WireError::TooBig)?;
        over_budget_pin |= resident > cap;
        if c.stage == Stage::During && c.source.form == Form::Skill {
            during += 1;
        }
        selected.push(c);
    }
    let mut proofs: BTreeSet<String> = input.mandatory_proofs.iter().cloned().collect();
    for id in &proofs {
        wire::check_id(id)?;
    }
    let mut lenses = Vec::new();
    let mut items = Vec::new();
    selected.sort_by_key(|c| {
        (
            match c.stage {
                Stage::Before => 0,
                Stage::During => 1,
                Stage::After => 2,
            },
            &c.source.id,
        )
    });
    for c in &selected {
        let delivery = if c.source.form == Form::Check {
            proofs.insert(c.source.id.clone());
            DeliveryMode::Proof
        } else if c.stage == Stage::After {
            lenses.push(c.source.id.clone());
            DeliveryMode::ReviewLens
        } else {
            DeliveryMode::Inline
        };
        items.push(KitItem {
            id: c.source.id.clone(),
            version: c.source.version.clone(),
            sha256: c.source.content_hash.clone(),
            stage: c.stage,
            delivery,
            why: c.serves.iter().filter(|s| needs.keys().any(|n| strength(s, n) > 0.0)).cloned().collect::<Vec<_>>().join(", "),
            resident_tokens: cost(c),
            job: c.job.clone(),
            source_path: c.source.path.clone(),
        });
    }
    let gaps = needs.keys().filter(|n| !selected.iter().any(|c| c.serves.iter().any(|s| strength(s, n) > 0.0))).cloned().collect();
    let inventory_hash = input.inventory.digest.clone();
    Ok(Kit {
        labels: input.labels.clone(),
        needs: needs.into_keys().collect(),
        items,
        proofs: proofs.into_iter().collect(),
        review_lenses: lenses,
        gaps,
        dropped,
        role: input.role,
        vendor: input.vendor,
        model: input.model.clone(),
        effort: input.effort.clone(),
        inventory_hash,
        resident_tokens: resident,
        token_cap: cap,
        over_budget_pin,
        assemble_micros: start.elapsed().as_micros() as u64,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shadow {
    pub ids: Vec<String>,
    pub valid: bool,
    pub disagrees: bool,
    pub elapsed_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitOutcome {
    pub proof_first_try: Option<bool>,
    pub repairs: Option<u32>,
    pub review_blocking: Option<u32>,
    pub kit_used: Vec<String>,
    pub user: String,
    pub tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KitReceipt {
    pub schema: u32,
    pub mission: String,
    pub task: String,
    pub attempt: u32,
    pub kit: Kit,
    pub selected_at: u64,
    pub attestation: Option<Attestation>,
    pub outcome: Option<KitOutcome>,
    pub shadow: Option<Shadow>,
    pub costs: BTreeMap<String, Option<u64>>,
    pub completed_at: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Record for KitReceipt {
    fn schema(&self) -> u32 {
        self.schema
    }
}
impl KitReceipt {
    pub fn new(mission: &str, task: &str, attempt: u32, kit: Kit, at: u64) -> Result<Self, WireError> {
        if !valid_id(mission) || !valid_id(task) || attempt == 0 {
            return Err(WireError::Malformed);
        }
        Ok(Self {
            schema: SCHEMA,
            mission: mission.into(),
            task: task.into(),
            attempt,
            kit,
            selected_at: at,
            attestation: None,
            outcome: None,
            shadow: None,
            costs: BTreeMap::new(),
            completed_at: None,
            extra: Extra::new(),
        })
    }
    pub fn id(&self) -> String {
        let id = format!("{}-{}", self.task, self.attempt);
        if id.len() <= 64 { id } else { digest(id.as_bytes()) }
    }
    pub fn pin(&self) -> wire::KitPin {
        let kit = &self.kit;
        let approved = (
            &kit.labels,
            &kit.needs,
            &kit.items,
            &kit.proofs,
            &kit.review_lenses,
            &kit.gaps,
            kit.role,
            kit.vendor,
            &kit.model,
            &kit.effort,
            kit.resident_tokens,
            kit.token_cap,
            kit.over_budget_pin,
        );
        wire::KitPin { receipt: self.id(), digest: digest(&serde_json::to_vec(&approved).expect("plain kit data")) }
    }
}

#[derive(Debug, Clone)]
pub struct PassDecision {
    pub writer: Vendor,
    pub judge: Vendor,
    pub proofs_passed: bool,
    pub forward_win: bool,
    pub reverse_win: bool,
    pub completed_polish: u8,
    pub budget_left: bool,
    pub piece_cost_micros: u64,
    pub pass_cost_micros: u64,
}
pub fn keep_pass(p: &PassDecision) -> bool {
    p.writer != p.judge
        && p.proofs_passed
        && p.forward_win
        && p.reverse_win
        && p.completed_polish <= 2
        && p.budget_left
        && u128::from(p.pass_cost_micros) * 100 <= u128::from(p.piece_cost_micros) * 30
}
