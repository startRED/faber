//! MVP scar checks only. Prompt rules and route learning remain deferred.
use crate::project::{
    SCHEMA,
    kit::digest,
    overlaps, valid_id,
    wire::{CheckKind, CheckReceipt, CheckResult},
};
use crate::store::{RealDisk, read_checked, replace};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scar {
    pub schema: u32,
    pub id: String,
    pub paths: Vec<String>,
    pub check: String,
    pub red_tree: String,
    pub green_tree: String,
    pub red_receipt: String,
    pub green_receipt: String,
}
/// A store holds only Scar records, not route rules, session text or failure output.
pub struct ScarStore {
    path: PathBuf,
    scars: Vec<Scar>,
}
fn validate(s: &Scar) -> Result<(), &'static str> {
    if s.schema != SCHEMA
        || !valid_id(&s.id)
        || !valid_id(&s.red_receipt)
        || !valid_id(&s.green_receipt)
        || s.red_receipt == s.green_receipt
        || s.red_tree == s.green_tree
        || !crate::project::git::valid_oid(&s.red_tree)
        || !crate::project::git::valid_oid(&s.green_tree)
        || s.paths.is_empty()
        || s.paths.len() > 64
    {
        return Err("reason.scar.evidence");
    }
    crate::project::wire::check_id(&s.check).map_err(|e| e.key())?;
    for path in &s.paths {
        crate::project::lease::normalized(path).map_err(|e| e.key())?;
    }
    Ok(())
}
impl ScarStore {
    pub fn open(root: &Path) -> Result<Self, &'static str> {
        let root = crate::project::lease::real_path(root).map_err(|e| e.key())?;
        let path = root.join("scars.json");
        let scars: Vec<Scar> = read_checked(&path).map_err(|e| e.key())?.unwrap_or_default();
        if scars.len() > 4096 {
            return Err("reason.scar.history");
        }
        for s in &scars {
            validate(s)?;
        }
        Ok(Self { path, scars })
    }
    /// Receipts must come from the engine proof store, not a worker claim. Keep only their ids in the Scar.
    pub fn record(&mut self, scar: Scar, red: &CheckReceipt, green: &CheckReceipt) -> Result<(), &'static str> {
        validate(&scar)?;
        for (r, id, tree, result) in
            [(red, &scar.red_receipt, &scar.red_tree, CheckResult::Fail), (green, &scar.green_receipt, &scar.green_tree, CheckResult::Pass)]
        {
            if r.schema != SCHEMA
                || &r.id != id
                || &r.candidate != tree
                || !r.checks.iter().any(|c| {
                    c.id == scar.check
                        && matches!(c.kind, CheckKind::Program | CheckKind::ProgramJudge)
                        && c.result == result
                        && c.validate().is_ok()
                })
            {
                return Err("reason.scar.evidence");
            }
        }
        if self.scars.iter().any(|s| s.id == scar.id) || self.scars.len() >= 4096 {
            return Err("reason.scar.history");
        }
        let mut next = self.scars.clone();
        next.push(scar);
        let bytes = serde_json::to_vec_pretty(&next).map_err(|_| "error.store.write")?;
        replace(&RealDisk, &self.path, &bytes).map_err(|e| e.key())?;
        self.scars = next;
        Ok(())
    }
    pub fn checks_for(&self, paths: &[String]) -> Vec<String> {
        self.scars
            .iter()
            .filter(|s| s.paths.iter().any(|scope| paths.iter().any(|p| overlaps(scope, p))))
            .map(|s| s.check.clone())
            .collect()
    }
    pub fn digest(&self) -> String {
        digest(serde_json::to_string(&self.scars).expect("plain data").as_bytes())
    }
}
