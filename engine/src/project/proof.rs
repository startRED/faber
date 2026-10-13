//! Check receipts, partial reruns, content addressing, and first-party native checks.
use super::wire::{CheckOutcome, CheckReceipt, CheckResult, CompileReceipt};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckCandidate {
    pub check_id: String,
    pub brief: String,
    pub inputs: Vec<String>,
    pub loadout: String,
    pub model: String,
    pub base_revision: String,
    pub toolchain: String,
    pub dependencies: Vec<String>,
    pub environment: Vec<String>,
}

/// Computes the content-addressed digest of a check execution candidate.
pub fn compute_check_digest(candidate: &CheckCandidate) -> String {
    // JSON preserves both field and list boundaries, including embedded delimiters.
    super::lease::hash(&serde_json::to_vec(candidate).expect("string-only candidate serializes"))
}

#[derive(Debug, Clone, Default)]
pub struct CheckCache {
    by_digest: BTreeMap<String, CheckOutcome>,
    by_task: BTreeMap<String, Vec<CheckOutcome>>,
}

impl CheckCache {
    pub fn record(&mut self, digest: &str, outcome: &CheckOutcome) {
        if outcome.validate().is_ok() {
            self.by_digest.insert(digest.to_string(), outcome.clone());
        } else {
            self.by_digest.remove(digest);
        }
    }

    pub fn get(&self, digest: &str) -> Option<&CheckOutcome> {
        self.by_digest.get(digest)
    }

    pub fn record_task(&mut self, task: &str, outcome: &CheckOutcome) {
        self.by_task.entry(task.to_string()).or_default().push(outcome.clone());
    }

    pub fn get_task(&self, task: &str) -> Option<&Vec<CheckOutcome>> {
        self.by_task.get(task)
    }

    /// Invalidates only readers and descendants of changed nodes.
    pub fn invalidate_changed(&mut self, changed: &[String], graph: &BTreeMap<String, Vec<String>>) {
        let mut to_invalidate: BTreeSet<String> = changed.iter().cloned().collect();
        let mut queue: Vec<String> = changed.to_vec();

        while let Some(current) = queue.pop() {
            if let Some(dependents) = graph.get(&current) {
                for dep in dependents {
                    if to_invalidate.insert(dep.clone()) {
                        queue.push(dep.clone());
                    }
                }
            }
        }

        for task_id in to_invalidate {
            if let Some(checks) = self.by_task.remove(&task_id) {
                let ids: BTreeSet<_> = checks.iter().map(|c| &c.id).collect();
                self.by_digest.retain(|_, c| !ids.contains(&c.id));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofError {
    CandidateMismatch,
    InferredPass,
    MissingReason,
    MalformedArtifact,
    MissingPhases,
    MissingCheck,
    DuplicateCheck,
    CheckFailed,
}

impl ProofError {
    pub fn key(&self) -> &'static str {
        match self {
            ProofError::CandidateMismatch => "error.proof.candidateMismatch",
            ProofError::InferredPass => "error.proof.inferredPass",
            ProofError::MissingReason => "error.proof.missingReason",
            ProofError::MalformedArtifact => "error.proof.malformedArtifact",
            ProofError::MissingPhases => "error.proof.missingPhases",
            ProofError::MissingCheck => "error.proof.missingCheck",
            ProofError::DuplicateCheck => "error.proof.duplicateCheck",
            ProofError::CheckFailed => "error.proof.checkFailed",
        }
    }
}

/// Owner decision E10: never infer a pass.
pub fn validate_check_receipt(receipt: &CheckReceipt, expected_candidate: &str) -> Result<(), ProofError> {
    if receipt.candidate != expected_candidate {
        return Err(ProofError::CandidateMismatch);
    }
    if receipt.schema != super::SCHEMA
        || !super::valid_id(&receipt.id)
        || !super::valid_id(&receipt.task)
        || receipt.checks.is_empty()
        || receipt.checks.len() > super::MAX_TASKS * super::wire::MAX_PATHS
        || receipt.model.trim().is_empty()
        || receipt.effort.trim().is_empty()
        || receipt.candidate.len() != 64
        || !receipt.candidate.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ProofError::MissingCheck);
    }
    let mut ids = BTreeSet::new();
    for check in &receipt.checks {
        if !ids.insert(&check.id) {
            return Err(ProofError::DuplicateCheck);
        }
        if check.result == CheckResult::Pass {
            if check.evidence.is_empty() {
                return Err(ProofError::InferredPass);
            }
        } else if check.reason.as_deref().unwrap_or("").trim().is_empty() {
            return Err(ProofError::MissingReason);
        }
        check.validate().map_err(|_| ProofError::MalformedArtifact)?;
    }
    for art in &receipt.artifacts {
        art.validate().map_err(|_| ProofError::MalformedArtifact)?;
    }
    if receipt.effect_receipts.iter().any(|id| !super::valid_id(id)) {
        return Err(ProofError::MalformedArtifact);
    }
    Ok(())
}

pub fn validate_compile_receipt(receipt: &CompileReceipt, expected_candidate: &str) -> Result<(), ProofError> {
    if receipt.candidate != expected_candidate {
        return Err(ProofError::CandidateMismatch);
    }
    use super::wire::CompilePhaseKind as P;
    let expected =
        [(P::Sheet, "plan.sheet"), (P::Bar, "plan.bar"), (P::Critic, "plan.critic"), (P::Steps, "plan.compile"), (P::Code, "plan.compile")];
    if receipt.schema != super::SCHEMA
        || !super::valid_id(&receipt.id)
        || !super::valid_id(&receipt.mission)
        || receipt.phases.len() != expected.len()
        || receipt.plan_revision == 0
        || receipt.candidate.len() != 64
        || !receipt.candidate.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ProofError::MissingPhases);
    }
    let mut end = 0;
    for (actual, (phase, tag)) in receipt.phases.iter().zip(expected) {
        if actual.phase != phase || actual.cost_tag != tag || actual.started_at < end {
            return Err(ProofError::MissingPhases);
        }
        end = actual.started_at.checked_add(actual.duration_micros).ok_or(ProofError::MissingPhases)?;
    }
    if receipt.check_receipts.iter().any(|id| !super::valid_id(id))
        || receipt.dropped.iter().any(|d| !super::valid_id(&d.id) || d.text.trim().is_empty())
    {
        return Err(ProofError::MissingPhases);
    }
    Ok(())
}

/// A receipt may describe a failed/not-run proof; it cannot accept a candidate.
/// The caller supplies engine-owned receipts, never a worker's Complete claims.
pub fn validate_acceptance(bar: &[super::wire::BarCheck], receipts: &[CheckReceipt], candidate: &str) -> Result<(), ProofError> {
    if bar.is_empty() {
        return Err(ProofError::MissingCheck);
    }
    let mut outcomes = BTreeMap::new();
    for receipt in receipts {
        validate_check_receipt(receipt, candidate)?;
        for check in &receipt.checks {
            let required = bar.iter().find(|c| c.id == check.id).ok_or(ProofError::MissingCheck)?;
            if receipt.task != required.owner || check.kind != required.kind {
                return Err(ProofError::CandidateMismatch);
            }
            if outcomes.insert(&check.id, check).is_some() {
                return Err(ProofError::DuplicateCheck);
            }
        }
    }
    let mut ids = BTreeSet::new();
    for required in bar {
        if !ids.insert(&required.id) {
            return Err(ProofError::DuplicateCheck);
        }
        let outcome = outcomes.get(&required.id).ok_or(ProofError::MissingCheck)?;
        if (required.mandatory || required.severity == super::wire::Severity::Blocking) && !outcome.is_pass() {
            return Err(ProofError::CheckFailed);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretViolation {
    pub pattern: String,
    pub snippet: String,
}

/// Secret scanner on outgoing changes before review.
pub fn scan_secrets(content: &str) -> Result<(), Vec<SecretViolation>> {
    let mut violations = vec![];

    if content.contains("-----BEGIN PRIVATE KEY-----")
        || content.contains("-----BEGIN RSA PRIVATE KEY-----")
        || content.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
    {
        violations.push(SecretViolation { pattern: "private_key".into(), snippet: "private key header found".into() });
    }

    for line in content.lines() {
        if let Some(idx) = line.find("AKIA") {
            let rest = &line[idx..];
            if rest.as_bytes().get(..20).is_some_and(|s| s.iter().all(u8::is_ascii_alphanumeric)) {
                violations.push(SecretViolation { pattern: "aws_access_key".into(), snippet: "[redacted]".into() });
            }
        }
        if let Some(idx) = line.find("ghp_") {
            let rest = &line[idx..];
            if rest.len() >= 36 {
                violations.push(SecretViolation { pattern: "github_pat".into(), snippet: "[redacted]".into() });
            }
        }
    }

    if violations.is_empty() { Ok(()) } else { Err(violations) }
}

/// Verifies dependency licenses against allowed SPDX list.
pub fn check_spdx_licenses(licenses: &[&str], is_commercial: bool) -> Result<(), Vec<String>> {
    const ALLOWED_PERMISSIVE: &[&str] =
        &["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "CC0-1.0", "0BSD", "FSL-1.1-ALv2", "Zlib"];
    const COPYLEFT: &[&str] = &["GPL-2.0", "GPL-3.0", "AGPL-3.0", "LGPL-2.1", "LGPL-3.0", "SSPL-1.0"];

    let mut flagged = vec![];
    for &lic in licenses {
        if is_commercial {
            if COPYLEFT.contains(&lic) || !ALLOWED_PERMISSIVE.contains(&lic) {
                flagged.push(lic.to_string());
            }
        } else if !ALLOWED_PERMISSIVE.contains(&lic) && !COPYLEFT.contains(&lic) {
            flagged.push(lic.to_string());
        }
    }

    if flagged.is_empty() { Ok(()) } else { Err(flagged) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryIssue {
    TypoSquatting { candidate: String, target: String, distance: usize },
    NotFound(String),
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0; b_chars.len() + 1]; a_chars.len() + 1];

    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, val) in d[0].iter_mut().enumerate() {
        *val = j;
    }

    for i in 1..=a_chars.len() {
        for j in 1..=b_chars.len() {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
        }
    }
    d[a_chars.len()][b_chars.len()]
}

/// Detects typo-squatting by computing Levenshtein distance to known popular packages.
pub fn check_dependency_registry(package: &str, popular: &[&str]) -> Result<(), RegistryIssue> {
    if popular.contains(&package) {
        return Ok(());
    }

    for &pop in popular {
        let dist = levenshtein(package, pop);
        if dist > 0 && dist <= 2 {
            return Err(RegistryIssue::TypoSquatting { candidate: package.to_string(), target: pop.to_string(), distance: dist });
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OsvAdvisory {
    pub package: String,
    pub vulnerable_versions: Vec<String>,
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsvMatch {
    pub id: String,
    pub package: String,
    pub version: String,
    pub summary: String,
}

/// Offline advisory matching against public OSV vulnerability database fixtures.
pub fn check_osv_advisories(packages: &[(&str, &str)], db: &[OsvAdvisory]) -> Vec<OsvMatch> {
    let mut matches = vec![];
    for &(pkg, ver) in packages {
        for adv in db {
            if adv.package == pkg && adv.vulnerable_versions.iter().any(|v| v == ver) {
                matches.push(OsvMatch {
                    id: adv.id.clone(),
                    package: pkg.to_string(),
                    version: ver.to_string(),
                    summary: adv.summary.clone(),
                });
            }
        }
    }
    matches
}
