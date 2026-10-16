//! One serial exact-tree acceptance queue per repository coordinator. Writer claims never open its gate.
use super::{Extra, SCHEMA, git::valid_oid, proof::ProofSet, valid_id};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub base: String,
    pub tree: String,
    pub grant_revision: u64,
    pub writer: Vendor,
    pub input: String,
    pub paths: Vec<String>,
    pub conflicts: Vec<String>,
    definitions: Vec<super::proof::Definition>,
    proof: Option<ProofSet>,
    review: Option<Review>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Candidate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        base: String,
        tree: String,
        revision: u64,
        writer: Vendor,
        input: String,
        paths: Vec<String>,
        definitions: Vec<super::proof::Definition>,
    ) -> Result<Self, &'static str> {
        if !valid_id(id)
            || !valid_oid(&base)
            || !valid_oid(&tree)
            || revision == 0
            || input.is_empty()
            || paths.is_empty()
            || paths.len() > 64
        {
            return Err("reason.queue.candidate");
        }
        for path in &paths {
            super::lease::normalized(path).map_err(|e| e.key())?;
        }
        let proof = ProofSet::new(tree.clone(), input.clone(), definitions, paths.clone())?;
        Ok(Self {
            id: id.into(),
            base,
            tree,
            grant_revision: revision,
            writer,
            input,
            paths: proof.paths().to_vec(),
            conflicts: vec![],
            definitions: proof.definitions().to_vec(),
            proof: None,
            review: None,
            extra: Extra::new(),
        })
    }

    pub fn definitions(&self) -> &[super::proof::Definition] {
        &self.definitions
    }

    /// The engine's proof of this exact tree, once it has been attached.
    pub fn proof(&self) -> Option<&ProofSet> {
        self.proof.as_ref()
    }

    /// The review attached to this exact tree, if any.
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }
}
/// Observed by a fresh read-only reviewer driver after admission, never chosen by the writer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub vendor: Vendor,
    pub tree: String,
    pub proof_digest: String,
    pub session: String,
    pub fresh: bool,
    pub accepted: bool,
    pub findings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    schema: u32,
    entries: VecDeque<Candidate>,
    review_sessions: BTreeSet<String>,
    #[serde(flatten)]
    extra: Extra,
}
impl Default for Queue {
    fn default() -> Self {
        Self { schema: SCHEMA, entries: VecDeque::new(), review_sessions: BTreeSet::new(), extra: Extra::new() }
    }
}
impl Queue {
    pub fn enqueue(&mut self, c: Candidate) -> Result<(), &'static str> {
        if self.schema != SCHEMA || self.entries.len() >= 2 || self.get(&c.id).is_some() {
            return Err("reason.queue.capacity");
        }
        self.entries.push_back(c);
        Ok(())
    }
    pub fn prove(&mut self, id: &str, p: ProofSet) -> Result<(), &'static str> {
        let c = self.front_mut(id)?;
        if !p.reusable(&c.tree, &c.input, &c.definitions, &c.paths) {
            return Err("reason.queue.proof");
        }
        c.proof = Some(p);
        c.review = None;
        Ok(())
    }
    pub fn review(&mut self, id: &str, r: Review) -> Result<(), &'static str> {
        if self.review_sessions.contains(&r.session) {
            return Err("reason.queue.review");
        }
        let c = self.front_mut(id)?;
        let p = c.proof.as_ref().filter(|p| p.verified()).ok_or("reason.queue.proof")?;
        if !r.fresh || !valid_id(&r.session) || r.vendor == c.writer || r.tree != c.tree || r.proof_digest != p.digest() {
            return Err("reason.queue.review");
        }
        // Even a refused review consumed a fresh session; it cannot be relabelled and reused after repair.
        c.review = Some(r.clone());
        self.review_sessions.insert(r.session);
        if !r.accepted || !r.findings.is_empty() {
            return Err("reason.queue.review");
        }
        Ok(())
    }
    pub fn accepted(&self, id: &str, head: &str, revision: u64) -> Result<&Candidate, &'static str> {
        let c = self.front().filter(|c| c.id == id).ok_or("reason.queue.serial")?;
        if c.base != head || c.grant_revision != revision {
            return Err("reason.queue.stale");
        }
        let p = c.proof.as_ref().filter(|p| p.verified() && p.tree == c.tree && p.input == c.input).ok_or("reason.queue.proof")?;
        if !c.conflicts.is_empty()
            || c.review.as_ref().is_none_or(|r| {
                !r.accepted
                    || !r.findings.is_empty()
                    || !r.fresh
                    || r.vendor == c.writer
                    || r.tree != c.tree
                    || r.proof_digest != p.digest()
            })
        {
            return Err("reason.queue.review");
        }
        Ok(c)
    }
    pub fn recompose(&mut self, id: &str, base: String, tree: String, input: String) -> Result<(), &'static str> {
        if !valid_oid(&base) || !valid_oid(&tree) || input.is_empty() {
            return Err("reason.queue.candidate");
        }
        let c = self.front_mut(id)?;
        c.base = base;
        c.tree = tree;
        c.input = input;
        c.proof = None;
        c.review = None;
        Ok(())
    }
    pub fn conflict(&mut self, id: &str, artifacts: Vec<String>) -> Result<(), &'static str> {
        if artifacts.len() < 2 || artifacts.len() > 64 {
            return Err("reason.queue.conflict");
        }
        for path in &artifacts {
            super::clean_path(path).ok_or("error.wire.path")?;
        }
        let c = self.front_mut(id)?;
        c.conflicts = artifacts;
        c.proof = None;
        c.review = None;
        Ok(())
    }
    pub(crate) fn landing_ids(&mut self, id: &str) -> Result<(String, String), &'static str> {
        let c = self.front_mut(id)?;
        for key in ["commitIntent", "forwardIntent"] {
            c.extra.entry(key.into()).or_insert_with(|| uuid::Uuid::new_v4().to_string().into());
        }
        let value = |key| {
            c.extra.get(key).and_then(serde_json::Value::as_str).filter(|s| valid_id(s)).map(str::to_string).ok_or("error.store.corrupt")
        };
        Ok((value("commitIntent")?, value("forwardIntent")?))
    }
    pub fn get(&self, id: &str) -> Option<&Candidate> {
        self.entries.iter().find(|c| c.id == id)
    }
    pub(crate) fn abandon(&mut self, id: &str) -> Result<Candidate, &'static str> {
        let index = self.entries.iter().position(|c| c.id == id).ok_or("reason.queue.serial")?;
        self.entries.remove(index).ok_or("reason.queue.serial")
    }
    pub fn front(&self) -> Option<&Candidate> {
        self.entries.front()
    }
    fn front_mut(&mut self, id: &str) -> Result<&mut Candidate, &'static str> {
        self.entries.front_mut().filter(|c| c.id == id).ok_or("reason.queue.serial")
    }
    /// The caller has persisted the broker's commit and CAS receipts. This is not a worker completion action.
    pub(crate) fn landed(&mut self, id: &str) -> Result<Candidate, &'static str> {
        if self.front().is_none_or(|c| c.id != id) {
            return Err("reason.queue.serial");
        }
        self.entries.pop_front().ok_or("reason.queue.serial")
    }
}
