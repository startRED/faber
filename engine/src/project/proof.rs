//! Check receipts, partial reruns, content addressing, and first-party native checks.
pub mod native;
pub mod screen;
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

impl CheckCandidate {
    pub fn capture_environment(&mut self) {
        self.environment.retain(|v| !v.starts_with("runtime."));
        self.environment.push(format!("runtime.platform={}/{}", std::env::consts::OS, std::env::consts::ARCH));
        for name in [
            "CI",
            "GITHUB_ACTIONS",
            "LANG",
            "LC_ALL",
            "TZ",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_PROFILE_DEV_DEBUG",
            "CARGO_PROFILE_TEST_DEBUG",
        ] {
            let digest = std::env::var_os(name).map(|v| super::lease::hash(v.as_encoded_bytes()));
            self.environment.push(format!("runtime.{name}={digest:?}"));
        }
    }
    /// Capture file contents through the broker's link-safe IO, never hash path names alone.
    pub fn capture_inputs(&mut self, root: &std::path::Path, paths: &[String]) -> Result<(), ProofError> {
        if paths.is_empty() || paths.len() > super::wire::MAX_PATHS {
            return Err(ProofError::MalformedArtifact);
        }
        let mut inputs = BTreeMap::new();
        for path in paths {
            super::kit::delivery::scoped_path(path).map_err(|_| ProofError::MalformedArtifact)?;
            let path = super::lease::normalized(path).map_err(|_| ProofError::MalformedArtifact)?;
            let file = super::files::ScopedFile::new(&root.join(&path), false).map_err(|_| ProofError::MalformedArtifact)?;
            let hash = file.hash().map_err(|_| ProofError::MalformedArtifact)?.ok_or(ProofError::MalformedArtifact)?;
            inputs.insert(path, hash);
        }
        self.inputs = inputs.into_iter().map(|(path, hash)| serde_json::json!({"path":path,"sha256":hash}).to_string()).collect();
        Ok(())
    }
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
        || !super::git::valid_oid(&receipt.candidate)
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
        || !super::git::valid_oid(&receipt.candidate)
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
        if required.from == "library:software@2:not-applicable" {
            if outcome.result != CheckResult::NotRun
                || outcome.reason.as_deref() != Some("reason.proof.notApplicable")
                || outcome.evidence.is_empty()
            {
                return Err(ProofError::CheckFailed);
            }
            continue;
        }
        if (required.mandatory || required.severity == super::wire::Severity::Blocking) && !outcome.is_pass() {
            return Err(ProofError::CheckFailed);
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct RefactorEvidence {
    pub captured_at: u64,
    pub code_started_at: u64,
    pub golden_base: BTreeMap<String, String>,
    pub golden_after: BTreeMap<String, String>,
    pub api_base: Vec<u8>,
    pub api_after: Vec<u8>,
    pub schema_base: Vec<u8>,
    pub schema_after: Vec<u8>,
    pub benchmark_base: Vec<u64>,
    pub benchmark_after: Vec<u64>,
}

pub fn validate_refactor(evidence: &RefactorEvidence) -> Result<(), ProofError> {
    if evidence.captured_at >= evidence.code_started_at
        || evidence.golden_base.is_empty()
        || evidence.golden_base != evidence.golden_after
        || evidence.api_base.is_empty()
        || evidence.schema_base.is_empty()
        || evidence.api_base != evidence.api_after
        || evidence.schema_base != evidence.schema_after
        || !(3..=10000).contains(&evidence.benchmark_base.len())
        || !(3..=10000).contains(&evidence.benchmark_after.len())
        || evidence.benchmark_base.iter().chain(&evidence.benchmark_after).any(|v| *v == 0)
    {
        return Err(ProofError::CheckFailed);
    }
    let mut base = evidence.benchmark_base.clone();
    let mut after = evidence.benchmark_after.clone();
    base.sort_unstable();
    after.sort_unstable();
    let noise = base[base.len() - 1] - base[0];
    if after[after.len() / 2] > base[base.len() / 2].saturating_add(noise) {
        return Err(ProofError::CheckFailed);
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
        for (prefix, length) in [("ASIA", 20), ("github_pat_", 40), ("gho_", 36), ("ghs_", 36), ("sk-proj-", 32), ("glpat-", 26)] {
            if line.match_indices(prefix).any(|(at, _)| {
                line.as_bytes().get(at..at + length).is_some_and(|s| s.iter().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b)))
            }) {
                violations.push(SecretViolation { pattern: "token".into(), snippet: "[redacted]".into() });
            }
        }
    }

    if violations.is_empty() { Ok(()) } else { Err(violations) }
}

/// Verifies dependency licenses against allowed SPDX list.
pub fn check_spdx_licenses(licenses: &[&str], is_commercial: bool) -> Result<(), Vec<String>> {
    let flagged: Vec<_> = licenses.iter().filter(|lic| !native::license_allowed(lic, is_commercial)).map(|lic| lic.to_string()).collect();

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
pub fn check_package_name(package: &str, popular: &[&str]) -> Result<(), RegistryIssue> {
    if package.is_empty() || package.len() > 214 || popular.len() > 2048 || popular.iter().any(|p| p.len() > 214) {
        return Err(RegistryIssue::NotFound("invalid-public-name".into()));
    }
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

use super::{Extra, SCHEMA, git::valid_oid, kit::digest, valid_id, wire};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub version: u32,
    pub program: PathBuf,
    pub argv: Vec<String>,
    pub checks: Vec<String>,
    pub toolchain: String,
    pub dependencies: String,
    pub environment: BTreeMap<String, String>,
    pub heavy: bool,
}
impl Definition {
    pub fn validate(&self) -> Result<(), &'static str> {
        wire::check_id(&self.id).map_err(|e| e.key())?;
        let name = self.program.file_name().and_then(|s| s.to_str()).ok_or("reason.proof.command")?.to_ascii_lowercase();
        let stem = name.trim_end_matches(".exe");
        if !self.program.has_root()
            || self.program.to_string_lossy().contains('\0')
            || self.version == 0
            || name.ends_with(".cmd")
            || name.ends_with(".bat")
            || ["cmd", "sh", "bash", "powershell", "pwsh", "wscript", "cscript"].contains(&stem)
            || self.argv.len() > 64
            || self.argv.iter().any(|a| a.len() > wire::MAX_TEXT || a.contains('\0'))
            || self.checks.is_empty()
            || self.checks.len() > 64
            || self.toolchain.is_empty()
            || self.dependencies.is_empty()
            || self.environment.len() > 64
            || self.environment.iter().any(|(k, v)| k.contains(['=', '\0']) || v.contains('\0'))
        {
            return Err("reason.proof.command");
        }
        for id in &self.checks {
            wire::check_id(id).map_err(|e| e.key())?;
        }
        if self.checks.iter().collect::<BTreeSet<_>>().len() != self.checks.len() {
            return Err("reason.proof.command");
        }
        Ok(())
    }
}
/// Delivered by a trusted contained executor, never parsed from a worker's completion claims.
pub struct Observation {
    pub exit: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub evidence: wire::ArtifactRef,
}

/// How long a proof may run before it counts as failed. A heavy proof (a build and its tests) holds the build lane, so it
/// gets the time a cold compile needs; every other proof is a quick check.
pub fn deadline(definition: &Definition) -> std::time::Duration {
    std::time::Duration::from_secs(if definition.heavy { 900 } else { 120 })
}

/// Execute a trusted definition through an owned process; missing exits never become passing evidence.
pub async fn execute(definition: &Definition, cwd: &std::path::Path, spawn: &crate::proc::Spawner, path: String) -> Observation {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut exit = None;
    if let Ok(mut process) = spawn(&definition.program.to_string_lossy(), &definition.argv, cwd) {
        let capture = tokio::time::timeout(deadline(definition), async {
            process.stdin.shutdown().await?;
            let limit = wire::MAX_ENVELOPE as u64 + 1;
            let mut stdout = process.stdout.take(limit);
            let stdout = stdout.read_to_end(&mut output);
            let mut stderr = process.stderr.take().unwrap_or_else(|| Box::new(tokio::io::empty())).take(limit);
            let stderr = stderr.read_to_end(&mut errors);
            tokio::try_join!(stdout, stderr)?;
            if output.len().saturating_add(errors.len()) > wire::MAX_ENVELOPE {
                return Err(std::io::Error::other("proof output limit"));
            }
            if let Some(child) = &mut process.child {
                exit = child.wait().await?.code();
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(capture, Ok(Ok(()))) {
            output.clear();
            errors.clear();
            crate::proc::stop(&mut *process.stdin, &mut process.child, &mut process.contained).await;
        }
    }
    let mut combined = output.clone();
    combined.extend(&errors);
    Observation { exit, stdout: output, stderr: errors, evidence: wire::ArtifactRef { path, sha256: digest(&combined) } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofSet {
    pub tree: String,
    pub input: String,
    definitions: Vec<Definition>,
    paths: Vec<String>,
    receipts: BTreeMap<String, wire::CheckReceipt>,
    active: BTreeSet<String>,
}
impl ProofSet {
    pub fn new(tree: String, input: String, mut defs: Vec<Definition>, mut paths: Vec<String>) -> Result<Self, &'static str> {
        if !valid_oid(&tree) || input.is_empty() || input.len() > wire::MAX_TEXT || defs.is_empty() || defs.len() > 64 || paths.len() > 64 {
            return Err("reason.proof.definition");
        }
        let mut seen = BTreeSet::new();
        for d in &mut defs {
            d.validate()?;
            if !seen.insert(d.id.clone()) {
                return Err("reason.proof.definition");
            }
            // A clean candidate has no dirty status to select tests from. The full suite is always a safe selection.
            if d.argv.first().is_some_and(|a| a == "t") && !d.argv.iter().any(|a| a == "--all") {
                d.argv.push("--all".into());
            }
        }
        for path in &paths {
            super::lease::normalized(path).map_err(|e| e.key())?;
        }
        paths.sort();
        paths.dedup();
        Ok(Self { tree, input, definitions: defs, paths, receipts: BTreeMap::new(), active: BTreeSet::new() })
    }
    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }
    pub fn paths(&self) -> &[String] {
        &self.paths
    }
    pub fn receipts(&self) -> impl Iterator<Item = &wire::CheckReceipt> {
        self.receipts.values()
    }
    /// Called only by the execution coordinator. One heavy proof occupies the build lane at a time.
    pub(crate) fn start(&mut self, id: &str) -> Result<(), &'static str> {
        let d = self.definitions.iter().find(|d| d.id == id).ok_or("reason.proof.definition")?;
        if self.active.contains(id)
            || self.receipts.contains_key(id)
            || d.heavy && self.active.iter().any(|id| self.definitions.iter().any(|d| &d.id == id && d.heavy))
        {
            return Err("reason.proof.busy");
        }
        self.active.insert(id.into());
        Ok(())
    }
    pub fn record(&mut self, receipt: &str, id: &str, obs: Observation) -> Result<wire::CheckReceipt, &'static str> {
        if !valid_id(receipt) || self.receipts.contains_key(id) || self.receipts.values().any(|r| r.id == receipt) {
            return Err("reason.proof.receipt");
        }
        let definition = self.definitions.iter().find(|d| d.id == id).ok_or("reason.proof.definition")?;
        obs.evidence.validate().map_err(|e| e.key())?;
        if obs.stdout.len().saturating_add(obs.stderr.len()) > wire::MAX_ENVELOPE {
            return Err("error.wire.tooBig");
        }
        let mut output = obs.stdout;
        output.extend(obs.stderr);
        if digest(&output) != obs.evidence.sha256 {
            return Err("reason.proof.hash");
        }
        let result = match obs.exit {
            Some(0) => wire::CheckResult::Pass,
            Some(_) => wire::CheckResult::Fail,
            None => wire::CheckResult::NotRun,
        };
        let reason = match result {
            wire::CheckResult::Pass => None,
            wire::CheckResult::Fail => Some("reason.proof.failed".into()),
            wire::CheckResult::NotRun => Some("reason.proof.notRun".into()),
        };
        let mut extra = Extra::new();
        extra.insert("definition".into(), serde_json::to_value(definition).expect("plain data"));
        extra.insert("input".into(), self.input.clone().into());
        extra.insert("changedPaths".into(), serde_json::to_value(&self.paths).expect("plain data"));
        extra.insert("exit".into(), serde_json::json!(obs.exit));
        let receipt = wire::CheckReceipt {
            schema: SCHEMA,
            id: receipt.into(),
            task: "engine".into(),
            candidate: self.tree.clone(),
            checks: definition
                .checks
                .iter()
                .map(|id| wire::CheckOutcome {
                    id: id.clone(),
                    kind: wire::CheckKind::Program,
                    result,
                    evidence: vec![obs.evidence.clone()],
                    reason: reason.clone(),
                })
                .collect(),
            artifacts: vec![obs.evidence],
            model: "engine".into(),
            effort: "none".into(),
            cost_micros: Some(0),
            effect_receipts: vec![],
            extra,
        };
        self.receipts.insert(id.into(), receipt.clone());
        self.active.remove(id);
        Ok(receipt)
    }
    pub fn verified(&self) -> bool {
        !self.definitions.is_empty()
            && self.active.is_empty()
            && self.definitions.iter().all(|d| {
                d.validate().is_ok()
                    && self.receipts.get(&d.id).is_some_and(|r| {
                        r.schema == SCHEMA
                            && r.candidate == self.tree
                            && r.extra.get("input").and_then(serde_json::Value::as_str) == Some(&self.input)
                            && r.extra.get("definition") == serde_json::to_value(d).ok().as_ref()
                            && r.extra.get("exit") == Some(&serde_json::json!(0))
                            && r.checks.len() == d.checks.len()
                            && r.checks.iter().zip(&d.checks).all(|(c, id)| {
                                &c.id == id
                                    && c.kind == wire::CheckKind::Program
                                    && c.result == wire::CheckResult::Pass
                                    && c.validate().is_ok()
                            })
                    })
            })
    }
    pub fn reusable(&self, tree: &str, input: &str, defs: &[Definition], paths: &[String]) -> bool {
        let mut paths = paths.to_vec();
        paths.sort();
        paths.dedup();
        self.verified() && self.tree == tree && self.input == input && self.definitions == defs && self.paths == paths
    }
    pub fn digest(&self) -> String {
        digest(serde_json::to_string(self).expect("plain data").as_bytes())
    }
}
