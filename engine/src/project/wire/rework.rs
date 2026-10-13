//! Additive wire v2 contracts shared by kits, the software compiler and the runner.
use super::{WireError, hash, ids, path, paths, text, texts};
use crate::project::{Extra, Record};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ping {
    Order,
    Found,
    Blocked,
    NeedEyes,
    DoneReady,
    Disagree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Research,
    Plan,
    Design,
    Build,
    Fix,
    Verify,
    Review,
    Write,
    Produce,
    Operate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fresh {
    Stable,
    Recent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Facets {
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub deliverable: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub region: String,
}

impl Facets {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), WireError> {
        for value in [&self.purpose, &self.audience, &self.deliverable, &self.stage, &self.region] {
            text(value)?;
        }
        Ok(())
    }
}

/// Needs are normalized data; never tool names or executable instructions.
pub fn normalize_need(raw: &str) -> Result<String, WireError> {
    const FAMILIES: [&str; 18] = [
        "lang",
        "framework",
        "platform",
        "service",
        "data",
        "artifact",
        "media",
        "domain",
        "op",
        "verify",
        "style",
        "app",
        "ui",
        "growth",
        "commerce",
        "legal",
        "security",
        "infra",
    ];
    if raw.len() > 128 {
        return Err(WireError::TooBig);
    }
    let required = raw.ends_with('!');
    let raw = raw.strip_suffix('!').unwrap_or(raw).to_ascii_lowercase();
    let (family, value) = raw.split_once(':').ok_or(WireError::Malformed)?;
    let value = match (family, value) {
        ("lang", "c#") => "csharp",
        _ => value,
    };
    if !FAMILIES.contains(&family)
        || value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(WireError::Malformed);
    }
    let value = match (family, value) {
        ("platform", "tml") => "tmodloader",
        ("lang", "ts") => "typescript",
        ("lang", "js") => "javascript",
        ("lang", "c#") => "csharp",
        _ => value,
    };
    Ok(format!("{family}:{value}{}", if required { "!" } else { "" }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Labels {
    pub verb: Verb,
    pub needs: Vec<String>,
    pub risk: Risk,
    pub fresh: Fresh,
    #[serde(default, skip_serializing_if = "Facets::is_empty")]
    pub facets: Facets,
}

impl Labels {
    pub fn validate(&self) -> Result<(), WireError> {
        if self.needs.len() > 8 {
            return Err(WireError::TooBig);
        }
        self.facets.validate()?;
        for need in &self.needs {
            normalize_need(need)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitPin {
    pub receipt: String,
    pub digest: String,
}
impl KitPin {
    pub fn validate(&self) -> Result<(), WireError> {
        ids(std::slice::from_ref(&self.receipt))?;
        hash(&self.digest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Verbatim {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assumption {
    pub id: String,
    pub text: String,
    pub source: String,
    pub confirmed: bool,
}
impl Assumption {
    pub fn validate(&self) -> Result<(), WireError> {
        ids(std::slice::from_ref(&self.id))?;
        text(&self.text)?;
        text(&self.source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Piece {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub lives_in: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub made_from: Vec<String>,
    #[serde(default)]
    pub pairs_with: Vec<String>,
    #[serde(default)]
    pub protected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_kind: Option<ChangeKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Feature,
    BugFix,
    Refactor,
    DependencyUpgrade,
}

/// Unknown answers remain unknown, never silently switch a risk group off.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RiskAnswers {
    #[serde(default)]
    pub public: Option<bool>,
    #[serde(default)]
    pub others_data: Option<bool>,
    #[serde(default)]
    pub money: Option<bool>,
    #[serde(default)]
    pub minors: Option<bool>,
    #[serde(default)]
    pub regulated: Option<bool>,
    #[serde(default)]
    pub commercial: Option<bool>,
    #[serde(default)]
    pub irreversible: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SheetQuestion {
    pub text: String,
    pub options: Vec<String>,
    pub default: String,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliverableSheet {
    pub pieces: Vec<Piece>,
    pub facets: Facets,
    pub risk: RiskAnswers,
    #[serde(default)]
    pub facts: Vec<Verbatim>,
    #[serde(default)]
    pub questions: Vec<SheetQuestion>,
    #[serde(default)]
    pub assumptions: Vec<Assumption>,
    #[serde(default)]
    pub user_actions: Vec<String>,
    #[serde(default)]
    pub taste: Vec<String>,
}
impl DeliverableSheet {
    pub fn validate(&self) -> Result<(), WireError> {
        if self.pieces.len() > 64 || self.questions.len() > 3 || self.facts.len() > 64 || self.assumptions.len() > 64 {
            return Err(WireError::TooBig);
        }
        self.facets.validate()?;
        texts(&self.risk.irreversible)?;
        texts(&self.user_actions)?;
        texts(&self.taste)?;
        for p in &self.pieces {
            ids(std::slice::from_ref(&p.id))?;
            text(&p.kind)?;
            text(&p.lives_in)?;
            paths(&p.paths)?;
            ids(&p.made_from)?;
            ids(&p.pairs_with)?;
            texts(&p.protected)?;
        }
        for f in &self.facts {
            ids(std::slice::from_ref(&f.id))?;
            text(&f.text)?;
        }
        for a in &self.assumptions {
            a.validate()?;
        }
        for q in &self.questions {
            text(&q.text)?;
            texts(&q.options)?;
            text(&q.default)?;
            texts(&q.changes)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckKind {
    Program,
    #[serde(rename = "program+judge")]
    ProgramJudge,
    Judge,
    Gate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Blocking,
    Major,
    Minor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroundKind {
    Owner,
    Repository,
    Standard,
    Documentation,
    Evidence,
    Judgement,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ground {
    pub kind: GroundKind,
    pub reference: String,
}

/// Dot-separated check/proof ids are distinct from record filenames.
pub fn check_id(id: &str) -> Result<(), WireError> {
    if (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        Ok(())
    } else {
        Err(WireError::Malformed)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BarCheck {
    pub id: String,
    pub piece: String,
    pub attribute: String,
    pub check: String,
    pub kind: CheckKind,
    pub severity: Severity,
    pub ground: Ground,
    pub proof: String,
    pub owner: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub pass: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub also: Vec<Ground>,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub mandatory: bool,
}
impl BarCheck {
    pub fn validate(&self) -> Result<(), WireError> {
        check_id(&self.id)?;
        check_id(&self.proof)?;
        ids(&[self.piece.clone(), self.owner.clone()])?;
        text(&self.attribute)?;
        text(&self.check)?;
        text(&self.ground.reference)?;
        text(&self.pass)?;
        text(&self.evidence)?;
        text(&self.from)?;
        if self.also.len() > 64 {
            return Err(WireError::TooBig);
        }
        for ground in &self.also {
            text(&ground.reference)?;
        }
        paths(&self.paths)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRef {
    pub path: String,
    pub sha256: String,
}
impl ArtifactRef {
    pub fn validate(&self) -> Result<(), WireError> {
        path(&self.path)?;
        hash(&self.sha256)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Claim {
    pub check: String,
    pub text: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckResult {
    Pass,
    Fail,
    NotRun,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckOutcome {
    pub id: String,
    pub kind: CheckKind,
    pub result: CheckResult,
    #[serde(default)]
    pub evidence: Vec<ArtifactRef>,
    #[serde(default)]
    pub reason: Option<String>,
}
impl CheckOutcome {
    pub fn validate(&self) -> Result<(), WireError> {
        check_id(&self.id)?;
        if self.evidence.len() > 64 {
            return Err(WireError::TooBig);
        }
        for e in &self.evidence {
            e.validate()?;
        }
        if self.result == CheckResult::Pass && self.evidence.is_empty() {
            return Err(WireError::Malformed);
        }
        if self.result != CheckResult::Pass && self.reason.as_ref().is_none_or(|s| s.trim().is_empty()) {
            return Err(WireError::Malformed);
        }
        self.reason.as_deref().map_or(Ok(()), text)
    }
}

/// Separate from T1's effect Receipt; evidence applies to this exact accepted candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReceipt {
    pub schema: u32,
    pub id: String,
    pub task: String,
    pub candidate: String,
    pub checks: Vec<CheckOutcome>,
    pub artifacts: Vec<ArtifactRef>,
    pub model: String,
    pub effort: String,
    pub cost_micros: Option<u64>,
    #[serde(default)]
    pub effect_receipts: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Record for CheckReceipt {
    fn schema(&self) -> u32 {
        self.schema
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompilePhaseKind {
    Sheet,
    Bar,
    Critic,
    Steps,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilePhase {
    pub phase: CompilePhaseKind,
    pub started_at: u64,
    pub duration_micros: u64,
    pub cost_tag: String,
    pub cost_micros: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileReceipt {
    pub schema: u32,
    pub id: String,
    pub mission: String,
    pub plan_revision: u64,
    pub candidate: String,
    pub phases: Vec<CompilePhase>,
    pub check_receipts: Vec<String>,
    pub dropped: Vec<Verbatim>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Record for CompileReceipt {
    fn schema(&self) -> u32 {
        self.schema
    }
}
