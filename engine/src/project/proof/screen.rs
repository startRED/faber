//! Validate engine-captured draft and hidden-app records. Worker claims are not these records.
//! The runner owns session identity and hidden execution; the journal verifies report hashes.
use super::ProofError;
use crate::model::Vendor;
use crate::project::bar::{GhostPersona, all_ghost_personas, screen_draft_widths};
use crate::project::wire::{ArtifactRef, CheckResult};
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct DraftProof {
    pub draft: String,
    pub width: u32,
    pub judged_at: u64,
    pub vendor: Vendor,
    pub session: String,
    pub result: CheckResult,
    pub report: ArtifactRef,
}

pub fn validate_drafts(
    records: &[DraftProof],
    selected: &str,
    code_started_at: u64,
    writer: Vendor,
    writer_session: &str,
) -> Result<(), ProofError> {
    let drafts = records.iter().map(|r| r.draft.as_str()).collect::<BTreeSet<_>>();
    if !(2..=3).contains(&drafts.len()) || !drafts.contains(selected) || records.len() != drafts.len() * screen_draft_widths().len() {
        return Err(ProofError::MissingCheck);
    }
    let mut seen = BTreeSet::new();
    for r in records {
        r.report.validate().map_err(|_| ProofError::MalformedArtifact)?;
        if r.draft.is_empty()
            || !screen_draft_widths().contains(&r.width)
            || !seen.insert((&r.draft, r.width))
            || r.vendor == writer
            || r.session.is_empty()
            || r.session == writer_session
            || r.judged_at >= code_started_at
            || r.result == CheckResult::NotRun
            || r.draft == selected && r.result != CheckResult::Pass
        {
            return Err(ProofError::CheckFailed);
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct GhostProof {
    pub persona: GhostPersona,
    pub candidate: String,
    pub width: u32,
    pub hidden: bool,
    pub vendor: Vendor,
    pub session: String,
    pub interrupted: bool,
    pub result: CheckResult,
    pub report: ArtifactRef,
}

pub fn validate_ghosts(records: &[GhostProof], candidate: &str, writer: Vendor, writer_session: &str) -> Result<(), ProofError> {
    if records.len() != all_ghost_personas().len() {
        return Err(ProofError::MissingCheck);
    }
    let mut seen = Vec::new();
    for r in records {
        r.report.validate().map_err(|_| ProofError::MalformedArtifact)?;
        if r.candidate != candidate
            || !r.hidden
            || r.vendor == writer
            || r.session.is_empty()
            || r.session == writer_session
            || r.result != CheckResult::Pass
            || !screen_draft_widths().contains(&r.width)
            || seen.contains(&r.persona)
            || r.persona == GhostPersona::Distracted && (!r.interrupted || r.width != screen_draft_widths()[0])
        {
            return Err(ProofError::CheckFailed);
        }
        seen.push(r.persona);
    }
    Ok(())
}
