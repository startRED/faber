//! One serial exact-tree acceptance queue per repository coordinator. Writer claims never open its gate.
//!
//! A candidate lands only with the engine's own verified proof of its exact composed tree and as many accepted fresh
//! reviews of that tree as its size asks for (one for a light change, two from different sessions for a full one,
//! R146). When the integration head's own proof is red the line stops: nothing lands but the one candidate that owns
//! the repair of the base, while every other candidate may still be written, proven and reviewed.
use super::{Extra, SCHEMA, git::valid_oid, proof::ProofSet, valid_id};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

fn one() -> u8 {
    1
}
fn is_one(n: &u8) -> bool {
    *n == 1
}

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
    /// The earlier accepted reviews of the current tree and proof, in order. Any change of tree or proof empties it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reviews: Vec<Review>,
    /// How many accepted reviews from different sessions the candidate's size asks for. It only grows.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    needed: u8,
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
            reviews: vec![],
            needed: 1,
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

    /// The latest review attached to this exact tree, if any.
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }

    /// How many accepted reviews from different sessions it needs before it may land.
    pub fn reviews_needed(&self) -> u8 {
        self.needed
    }

    /// The sessions whose accepted review still holds for the current tree and proof.
    pub fn accepted_reviews(&self) -> BTreeSet<&str> {
        let Some(p) = self.proof.as_ref().filter(|p| p.verified() && p.tree == self.tree && p.input == self.input) else {
            return BTreeSet::new();
        };
        let digest = p.digest();
        self.reviews
            .iter()
            .chain(self.review.iter())
            .filter(|r| {
                r.accepted && r.findings.is_empty() && r.fresh && r.vendor != self.writer && r.tree == self.tree && r.proof_digest == digest
            })
            .map(|r| r.session.as_str())
            .collect()
    }

    fn forget_reviews(&mut self) {
        self.review = None;
        self.reviews.clear();
    }
}
/// Observed by a fresh read-only reviewer driver after admission, never chosen by the writer. `findings` holds only the
/// findings that block (R147); a review with notes alone is accepted, and the notes stay with the coordinator.
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

/// A stopped line: the integration head whose own proof is red, the receipt that showed it, and the one candidate that
/// owns its repair once one is named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    pub head: String,
    pub receipt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    schema: u32,
    entries: VecDeque<Candidate>,
    review_sessions: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<Line>,
    #[serde(flatten)]
    extra: Extra,
}
impl Default for Queue {
    fn default() -> Self {
        Self { schema: SCHEMA, entries: VecDeque::new(), review_sessions: BTreeSet::new(), line: None, extra: Extra::new() }
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
        c.forget_reviews();
        Ok(())
    }
    /// The size of the candidate asks for `n` accepted reviews (1 or 2). A smaller number than it already needs changes
    /// nothing: a size is never lowered once measured.
    pub fn require_reviews(&mut self, id: &str, n: u8) -> Result<(), &'static str> {
        if !(1..=2).contains(&n) {
            return Err("reason.queue.review");
        }
        let c = self.entries.iter_mut().find(|c| c.id == id).ok_or("reason.queue.serial")?;
        c.needed = c.needed.max(n);
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
        // Even a refused review consumed a fresh session; it cannot be relabelled and reused after repair. An earlier
        // accepted review of the same tree stays counted, and a refused one now on top of it stops the landing.
        if let Some(previous) = c.review.take().filter(|p| p.accepted && p.findings.is_empty()) {
            c.reviews.push(previous);
        }
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
        if self.line.as_ref().is_some_and(|line| line.owner.as_deref() != Some(id)) {
            return Err("reason.queue.baseRed");
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
            || c.accepted_reviews().len() < usize::from(c.needed)
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
        c.forget_reviews();
        Ok(())
    }
    /// A repair changed the candidate's files: the new tree and its paths replace the old ones, and the proof and every
    /// review start again.
    pub fn repaired(&mut self, id: &str, tree: String, input: String, paths: Vec<String>) -> Result<(), &'static str> {
        if !valid_oid(&tree) || input.is_empty() || paths.is_empty() || paths.len() > 64 {
            return Err("reason.queue.candidate");
        }
        let c = self.front_mut(id)?;
        let proof = ProofSet::new(tree.clone(), input.clone(), c.definitions.clone(), paths)?;
        c.paths = proof.paths().to_vec();
        c.tree = tree;
        c.input = input;
        c.proof = None;
        c.forget_reviews();
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
        c.forget_reviews();
        Ok(())
    }

    /// The integration head's own proof is red: stop the line. A line already stopped on the same head keeps its owner.
    pub fn stop_line(&mut self, head: &str, receipt: &str) -> Result<(), &'static str> {
        if !valid_oid(head) || !valid_id(receipt) {
            return Err("reason.queue.candidate");
        }
        if self.line.as_ref().is_some_and(|line| line.head == head) {
            return Ok(());
        }
        self.line = Some(Line { head: head.into(), receipt: receipt.into(), owner: None });
        Ok(())
    }
    /// Name the one candidate that repairs the base. Asking again for the same one is fine; a second owner is refused.
    pub fn own_repair(&mut self, id: &str) -> Result<(), &'static str> {
        let line = self.line.as_mut().ok_or("reason.queue.serial")?;
        match &line.owner {
            Some(owner) if owner != id => Err("reason.queue.baseRed"),
            _ => {
                line.owner = Some(id.into());
                Ok(())
            }
        }
    }
    /// The head's own proof is green again (a new run, or the project's CI): the line moves.
    pub fn clear_line(&mut self, head: &str) {
        if self.line.as_ref().is_some_and(|line| line.head == head) {
            self.line = None;
        }
    }
    pub fn line(&self) -> Option<&Line> {
        self.line.as_ref()
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
    /// Keep an engine record beside a candidate (its measured size, say). It never opens the gate.
    pub fn note(&mut self, id: &str, key: &str, value: serde_json::Value) -> Result<(), &'static str> {
        let c = self.entries.iter_mut().find(|c| c.id == id).ok_or("reason.queue.serial")?;
        c.extra.insert(key.into(), value);
        Ok(())
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
    /// The caller has persisted the broker's commit and CAS receipts. This is not a worker completion action. A repair of
    /// a red base that lands is the new head, proven green, so the line moves again.
    pub(crate) fn landed(&mut self, id: &str) -> Result<Candidate, &'static str> {
        if self.front().is_none_or(|c| c.id != id) {
            return Err("reason.queue.serial");
        }
        if self.line.as_ref().is_some_and(|line| line.owner.as_deref() == Some(id)) {
            self.line = None;
        }
        self.entries.pop_front().ok_or("reason.queue.serial")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_landed_repair_of_a_red_base_moves_the_line_and_any_other_landing_keeps_it() {
        let definition = super::super::proof::Definition {
            id: "rust".into(),
            version: 1,
            program: "/native/cargo".into(),
            argv: vec!["t".into()],
            checks: vec!["C1".into()],
            toolchain: "rust-1".into(),
            dependencies: "deps-1".into(),
            environment: Default::default(),
            heavy: false,
        };
        let candidate = |id: &str| {
            Candidate::new(
                id,
                "1".repeat(40),
                "2".repeat(40),
                3,
                Vendor::Codex,
                "input".into(),
                vec!["a.rs".into()],
                vec![definition.clone()],
            )
            .unwrap()
        };
        let mut q = Queue::default();
        q.enqueue(candidate("other")).unwrap();
        q.enqueue(candidate("owner")).unwrap();
        q.stop_line(&"1".repeat(40), "head-1").unwrap();
        q.own_repair("owner").unwrap();
        q.landed("other").unwrap();
        assert!(q.line().is_some(), "only the owner's landing moves the line");
        q.landed("owner").unwrap();
        assert!(q.line().is_none());
    }
}
