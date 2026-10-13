use faber_engine::model::Vendor;
use faber_engine::project::bar::GhostPersona;
use faber_engine::project::proof::screen::*;
use faber_engine::project::wire::{ArtifactRef, CheckResult};

fn report() -> ArtifactRef {
    ArtifactRef { path: "proof/trace.json".into(), sha256: "a".repeat(64) }
}

#[test]
fn drafts_require_two_or_three_full_width_judged_probes_before_code() {
    let mut records = Vec::new();
    for draft in ["one", "two"] {
        for width in [720, 900, 1100] {
            records.push(DraftProof {
                draft: draft.into(),
                width,
                judged_at: 10,
                vendor: Vendor::Codex,
                session: "fresh-review".into(),
                result: CheckResult::Pass,
                report: report(),
            });
        }
    }
    assert!(validate_drafts(&records, "one", 20, Vendor::Claude, "writer").is_ok());
    let mut missing = records.clone();
    missing.pop();
    assert!(validate_drafts(&missing, "one", 20, Vendor::Claude, "writer").is_err());
    let mut not_run = records.clone();
    not_run[0].result = CheckResult::NotRun;
    assert!(validate_drafts(&not_run, "one", 20, Vendor::Claude, "writer").is_err());
    assert!(validate_drafts(&records, "one", 5, Vendor::Claude, "writer").is_err());
    assert!(validate_drafts(&records, "one", 20, Vendor::Codex, "writer").is_err());
    assert!(validate_drafts(&records, "one", 20, Vendor::Claude, "fresh-review").is_err());
}

#[test]
fn ghosts_require_every_persona_hidden_execution_and_exact_candidate() {
    let records = [GhostPersona::Hurried, GhostPersona::Novice, GhostPersona::Distracted, GhostPersona::KeyboardOnly]
        .into_iter()
        .map(|persona| GhostProof {
            persona,
            candidate: "c".repeat(64),
            width: 720,
            hidden: true,
            vendor: Vendor::Codex,
            session: "fresh-review".into(),
            interrupted: persona == GhostPersona::Distracted,
            result: CheckResult::Pass,
            report: report(),
        })
        .collect::<Vec<_>>();
    assert!(validate_ghosts(&records, &"c".repeat(64), Vendor::Claude, "writer").is_ok());
    assert!(validate_ghosts(&records[..3], &"c".repeat(64), Vendor::Claude, "writer").is_err());
    for mutation in 0..4 {
        let mut bad = records.clone();
        match mutation {
            0 => bad[0].hidden = false,
            1 => bad[2].interrupted = false,
            2 => bad[0].candidate = "b".repeat(64),
            _ => bad[0].result = CheckResult::NotRun,
        }
        assert!(validate_ghosts(&bad, &"c".repeat(64), Vendor::Claude, "writer").is_err());
    }
}
