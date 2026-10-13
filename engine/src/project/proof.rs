//! Engine check receipts, separate from the effect journal's Receipt. This module never executes candidate code.
use super::{Extra, SCHEMA, git::valid_oid, kit::digest, valid_id, wire};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
    fn validate(&self) -> Result<(), &'static str> {
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
    pub fn start(&mut self, id: &str) -> Result<(), &'static str> {
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
