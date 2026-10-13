use faber_engine::project::bar::*;
use faber_engine::project::proof::*;
use faber_engine::project::wire::{ArtifactRef, ChangeKind};
use faber_engine::project::{Plan, Task, TaskKind};
mod support;
use support::canonical_tempdir as tempdir;

fn sheet() -> DeliverableSheet {
    DeliverableSheet {
        pieces: vec![Piece {
            id: "change".into(),
            kind: "change".into(),
            lives_in: "desktop".into(),
            paths: vec!["engine/src/project.rs".into()],
            made_from: vec![],
            pairs_with: vec![],
            protected: vec![],
            change_kind: Some(ChangeKind::BugFix),
        }],
        facets: Facets::default(),
        risk: RiskAnswers::faber_defaults(),
        facts: vec![],
        questions: vec![],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    }
}

fn outcome() -> CheckOutcome {
    CheckOutcome {
        id: "rust.fmt".into(),
        kind: CheckKind::Program,
        result: CheckResult::Pass,
        evidence: vec![ArtifactRef { path: "proof/fmt.log".into(), sha256: "a".repeat(64) }],
        reason: None,
    }
}

fn candidate() -> CheckCandidate {
    CheckCandidate {
        check_id: "rust.fmt".into(),
        brief: "format".into(),
        inputs: vec!["a;b".into()],
        loadout: "native".into(),
        model: "none".into(),
        base_revision: "base".into(),
        toolchain: "cargo".into(),
        dependencies: vec![],
        environment: vec![],
    }
}

#[test]
fn mandatory_checks_cannot_be_weakened_with_the_same_id() {
    let sheet = sheet();
    let original = software_bar(&sheet);
    for change in 0..7 {
        let mut bar = original.clone();
        let check = bar.iter_mut().find(|c| c.id == "rust.fmt").unwrap();
        match change {
            0 => check.mandatory = false,
            1 => check.severity = Severity::Minor,
            2 => check.kind = CheckKind::Gate,
            3 => check.proof = "always.pass".into(),
            4 => check.pass = "anything".into(),
            5 => check.paths = vec!["unrelated.rs".into()],
            _ => check.check = "Skip formatting".into(),
        }
        assert!(verify_mandatory_checks(&sheet, &bar).is_err(), "mutation {change}");
    }
}

#[test]
fn critic_cannot_replace_a_library_requirement() {
    let bar = software_bar(&sheet());
    let mut replacement = bar.iter().find(|c| c.id == "rust.fmt").unwrap().clone();
    replacement.id = "skip.fmt".into();
    replacement.mandatory = false;
    assert!(apply_critic(&bar, &[replacement]).is_err());
}

#[test]
fn candidate_hash_has_unambiguous_list_boundaries() {
    let a = candidate();
    let mut b = a.clone();
    b.inputs = vec!["a".into(), "b".into()];
    assert_ne!(compute_check_digest(&a), compute_check_digest(&b));
    let mut a = a;
    a.inputs = vec![];
    let mut b = a.clone();
    a.dependencies = vec!["a;b".into()];
    b.dependencies = vec!["a".into(), "b".into()];
    assert_ne!(compute_check_digest(&a), compute_check_digest(&b));
}

#[test]
fn cache_does_not_reuse_malformed_passes() {
    let mut cache = CheckCache::default();
    let mut pass = outcome();
    pass.evidence[0].sha256 = "z".repeat(64);
    assert!(!pass.is_pass(), "malformed evidence is not a verified pass");
    let digest = compute_check_digest(&candidate());
    cache.record(&digest, &pass);
    assert!(cache.get(&digest).is_none());
}

#[test]
fn invalidation_also_removes_digest_entries_for_changed_checks() {
    let mut cache = CheckCache::default();
    let pass = outcome();
    let digest = compute_check_digest(&candidate());
    cache.record(&digest, &pass);
    cache.record_task("writer", &pass);
    cache.invalidate_changed(&["writer".into()], &Default::default());
    assert!(cache.get(&digest).is_none());
}

#[test]
fn compiler_rejects_missing_owner_even_when_another_task_mentions_check() {
    let mut task = Task::new("writer", "Write", TaskKind::Writer);
    task.paths = vec!["engine/src/project.rs".into()];
    task.checks = vec!["rust.fmt".into()];
    task.proofs = vec!["proof.cargo-fmt".into()];
    let mut check = software_bar(&sheet()).into_iter().find(|c| c.id == "rust.fmt").unwrap();
    check.owner = "absent".into();
    assert!(validate_compiler_graph(&[task], &[check], &[]).is_err());
}

#[test]
fn compiler_rejects_cycles_and_unknown_checks_before_dispatch() {
    let mut task = Task::new("writer", "Write", TaskKind::Writer);
    task.paths = vec!["engine/src/project.rs".into()];
    task.checks = vec!["rust.fmt".into()];
    task.proofs = vec!["proof.cargo-fmt".into()];
    let mut check = software_bar(&sheet()).into_iter().find(|c| c.id == "rust.fmt").unwrap();
    check.owner = "writer".into();
    task.requires = vec!["writer".into()];
    assert!(validate_compiler_graph(&[task.clone()], &[check.clone()], &[]).is_err());
    task.requires.clear();
    task.checks.push("invented.check".into());
    assert!(validate_compiler_graph(&[task], &[check], &[]).is_err());
}

#[test]
fn plan_cannot_carry_sheet_without_a_complete_bar() {
    let mut plan = Plan::new("mission", 1, vec![]);
    plan.sheet = Some(sheet());
    assert!(plan.validate().is_err());
}

#[test]
fn repository_ground_requires_a_heading_and_cannot_leave_root() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("design.md"), "# Design\n").unwrap();
    std::fs::write(dir.path().join("outside.md"), "# Secret\n").unwrap();
    for reference in ["design.md", "../outside.md#secret"] {
        assert!(validate_ground(&root, &Ground { kind: GroundKind::Repository, reference: reference.into() }).is_err());
    }
}

#[test]
fn secret_scanner_handles_unicode_and_never_echoes_a_secret() {
    let content = "AKIAaaaaaaaaaaaaaaaé token ghp_1234567890123456789012345678901234567890";
    let found = scan_secrets(content).unwrap_err();
    assert!(found.iter().all(|v| !v.snippet.contains("ghp_")));
}

fn receipt() -> CheckReceipt {
    CheckReceipt {
        schema: 1,
        id: "check1".into(),
        task: "change".into(),
        candidate: "c".repeat(64),
        checks: vec![outcome()],
        artifacts: outcome().evidence,
        model: "native".into(),
        effort: "none".into(),
        cost_micros: Some(0),
        effect_receipts: vec![],
        extra: Default::default(),
    }
}

#[test]
fn receipt_rejects_empty_duplicate_and_malformed_outcomes() {
    let valid = receipt();
    assert!(validate_check_receipt(&valid, &valid.candidate).is_ok());
    for mutation in 0..5 {
        let mut bad = valid.clone();
        match mutation {
            0 => bad.checks.clear(),
            1 => bad.checks.push(outcome()),
            2 => bad.checks[0].evidence[0].sha256 = "z".repeat(64),
            3 => bad.checks[0].evidence[0].path = "../out".into(),
            _ => bad.artifacts[0].sha256 = "z".repeat(64),
        }
        assert!(validate_check_receipt(&bad, &valid.candidate).is_err(), "mutation {mutation}");
    }
}

#[test]
fn receipt_accepts_the_exact_git_sha1_tree_as_well_as_sha256_candidates() {
    let mut receipt = receipt();
    receipt.candidate = "c".repeat(40);
    assert!(validate_check_receipt(&receipt, &receipt.candidate).is_ok());
}

#[test]
fn conditional_rules_are_retained_and_excluded_only_with_recorded_applicability() {
    let sheet = sheet();
    let bar = software_bar(&sheet);
    let mut check = bar.iter().find(|c| c.id == "kind.refactor-golden").unwrap().clone();
    assert!(!check.mandatory);
    let removed = bar.iter().filter(|c| c.id != check.id).cloned().collect::<Vec<_>>();
    assert!(verify_mandatory_checks(&sheet, &removed).is_err());
    check.owner = "change".into();
    let mut receipt = receipt();
    receipt.checks[0].id = check.id.clone();
    receipt.checks[0].result = CheckResult::NotRun;
    receipt.checks[0].reason = Some("reason.proof.notApplicable".into());
    assert!(validate_acceptance(&[check.clone()], std::slice::from_ref(&receipt), &receipt.candidate).is_ok());
    receipt.checks[0].reason = Some("reason.proof.offline".into());
    assert!(validate_acceptance(&[check], std::slice::from_ref(&receipt), &receipt.candidate).is_err());
}

#[test]
fn compiler_requires_evidence_producer_and_matching_scope() {
    let mut task = Task::new("writer", "Write", TaskKind::Writer);
    task.paths = vec!["engine/src/project.rs".into()];
    task.checks = vec!["rust.fmt".into()];
    let mut check = software_bar(&sheet()).into_iter().find(|c| c.id == "rust.fmt").unwrap();
    check.owner = "writer".into();
    assert!(validate_compiler_graph(&[task.clone()], &[check.clone()], &[]).is_err());
    task.proofs.push(check.proof.clone());
    assert!(validate_compiler_graph(&[task.clone()], &[check.clone()], &[]).is_ok());
    task.paths = vec!["unrelated.rs".into()];
    assert!(validate_compiler_graph(&[task], &[check], &[]).is_err());
}

#[test]
fn sheet_rejects_empty_duplicate_unscoped_and_dangling_pieces() {
    let valid = sheet();
    for mutation in 0..5 {
        let mut bad = valid.clone();
        match mutation {
            0 => bad.pieces.clear(),
            1 => bad.pieces.push(bad.pieces[0].clone()),
            2 => bad.pieces[0].paths.clear(),
            3 => bad.pieces[0].made_from.push("missing".into()),
            _ => bad.pieces[0].kind.clear(),
        }
        assert!(bad.validate().is_err(), "mutation {mutation}");
    }
}

#[test]
fn acceptance_requires_each_check_pass_on_the_same_candidate() {
    let mut check = software_bar(&sheet()).into_iter().find(|c| c.id == "rust.fmt").unwrap();
    check.owner = "change".into();
    let bar = vec![check];
    let good = receipt();
    assert!(validate_acceptance(&bar, std::slice::from_ref(&good), &good.candidate).is_ok());
    assert!(validate_acceptance(&bar, &[], &good.candidate).is_err());
    for result in [CheckResult::Fail, CheckResult::NotRun] {
        let mut bad = good.clone();
        bad.checks[0].result = result;
        bad.checks[0].reason = Some("reason.proof.unavailable".into());
        assert!(validate_acceptance(&bar, &[bad], &good.candidate).is_err());
    }
    assert!(validate_acceptance(&bar, std::slice::from_ref(&good), &"d".repeat(64)).is_err());
    assert!(validate_acceptance(&bar, &[good.clone(), good.clone()], &good.candidate).is_err());
    let mut bad = good.clone();
    bad.checks[0].kind = CheckKind::Judge;
    assert!(validate_acceptance(&bar, &[bad], &good.candidate).is_err());
}

#[test]
fn compile_receipt_rejects_missing_phase_bad_tag_and_overlapping_time() {
    let phases = [
        (CompilePhaseKind::Sheet, "plan.sheet"),
        (CompilePhaseKind::Bar, "plan.bar"),
        (CompilePhaseKind::Critic, "plan.critic"),
        (CompilePhaseKind::Steps, "plan.compile"),
        (CompilePhaseKind::Code, "plan.compile"),
    ];
    let good = CompileReceipt {
        schema: 1,
        id: "compile1".into(),
        mission: "mission".into(),
        plan_revision: 1,
        candidate: "c".repeat(64),
        phases: phases
            .into_iter()
            .enumerate()
            .map(|(i, (phase, cost_tag))| CompilePhase {
                phase,
                started_at: i as u64 * 10,
                duration_micros: 10,
                cost_tag: cost_tag.into(),
                cost_micros: Some(0),
            })
            .collect(),
        check_receipts: vec![],
        dropped: vec![],
        extra: Default::default(),
    };
    assert!(validate_compile_receipt(&good, &good.candidate).is_ok());
    for mutation in 0..3 {
        let mut bad = good.clone();
        match mutation {
            0 => {
                bad.phases.pop();
            }
            1 => bad.phases[0].cost_tag = "free".into(),
            _ => bad.phases[1].started_at = 0,
        }
        assert!(validate_compile_receipt(&bad, &bad.candidate).is_err());
    }
    let dir = tempdir().unwrap();
    let journal = faber_engine::project::store::Journal::open(&dir.path().join("repo"), &dir.path().join("data"), "mission").unwrap();
    journal.save_compile_receipt(&good).unwrap();
    assert!(journal.save_compile_receipt(&good).is_err());
    std::fs::write(journal.records().join("compiles/rev-1.json"), "{}").unwrap();
    assert!(journal.compile_receipt(1).is_err());
}

#[test]
fn every_piece_receives_locked_requirements_with_all_its_paths() {
    let mut sheet = sheet();
    sheet.pieces[0].change_kind = None;
    let mut another = sheet.pieces[0].clone();
    another.id = "other".into();
    another.paths = vec!["engine/src/model.rs".into(), "engine/src/events.rs".into()];
    sheet.pieces.push(another);
    let bar = software_bar(&sheet);
    assert!(bar.iter().any(|c| c.piece == "other" && c.attribute == "rust.fmt" && c.paths.len() == 2));
    assert_eq!(bar.iter().map(|c| &c.id).collect::<std::collections::BTreeSet<_>>().len(), bar.len());
    for id in ["software.callers-map", "desktop.real-width-drafts", "screen.coherence-widths", "any.token-latency"] {
        let original = bar.iter().find(|c| c.id == id).unwrap();
        let removed = bar.iter().filter(|c| c.id != original.id).cloned().collect::<Vec<_>>();
        assert!(verify_mandatory_checks(&sheet, &removed).is_err(), "{id}");
    }
}

#[test]
fn baseline_bar_respects_judge_caps_with_screen_context() {
    let mut sheet = sheet();
    let mut screen = sheet.pieces[0].clone();
    screen.id = "screen".into();
    screen.kind = "screen".into();
    screen.paths = vec!["ui/App.tsx".into()];
    sheet.pieces.push(screen);
    let bar = software_bar(&sheet);
    for piece in &sheet.pieces {
        assert!(filter_and_validate_bar(&piece.id, &bar).is_ok(), "{}", piece.id);
    }
}

#[test]
fn moves_keep_ask_measure_guard_and_safe_gate() {
    let mut sheet = sheet();
    sheet.questions.push(SheetQuestion {
        text: "Keep layout?".into(),
        options: vec!["keep".into(), "change".into()],
        default: "keep".into(),
        changes: vec!["bar:screen.coherence-widths".into()],
    });
    let moves = plan_moves_for_piece(&sheet.pieces[0], &sheet);
    for expected in [MoveKind::Ask, MoveKind::Measure, MoveKind::Guard, MoveKind::Judge, MoveKind::Gate] {
        assert!(moves.contains(&expected), "{expected:?}");
    }
    assert!(moves.iter().position(|m| *m == MoveKind::Measure) < moves.iter().position(|m| *m == MoveKind::Judge));
}

#[test]
fn repair_ownership_does_not_match_sibling_prefixes_or_readers() {
    let mut reader = Task::new("reader", "Read", TaskKind::Research);
    reader.paths = vec!["ui".into()];
    let mut writer = Task::new("writer", "Write", TaskKind::Writer);
    writer.writes = vec!["ui/App.tsx".into()];
    assert_eq!(failing_path_lease_holder("ui/App.tsx", &[reader, writer.clone()]), Some("writer".into()));
    assert_eq!(failing_path_lease_holder("ui/App.tsx.bak", &[writer]), None);
}

#[test]
fn drafters_do_not_merge_different_pieces_by_attribute() {
    let a = software_bar(&sheet()).into_iter().find(|c| c.id == "rust.fmt").unwrap();
    let mut b = a.clone();
    b.id = "other.fmt".into();
    b.piece = "other".into();
    assert_eq!(merge_drafters(&[a], &[b]).len(), 2);
}

#[test]
fn official_grounds_cannot_name_unapproved_hosts_or_credentials() {
    let root = tempdir().unwrap();
    for reference in ["https://evil.example/docs", "http://www.w3.org/TR/WCAG22/", "https://secret@www.w3.org/TR/WCAG22/"] {
        assert!(validate_ground(root.path(), &Ground { kind: GroundKind::Standard, reference: reference.into() }).is_err());
    }
}

#[test]
fn ground_cannot_read_a_credential_directory_even_inside_the_repo() {
    let root = tempdir().unwrap();
    // A synthetic document in a temporary directory; never a real credential home.
    std::fs::create_dir(root.path().join(".codex")).unwrap();
    std::fs::write(root.path().join(".codex/fixture.md"), "# Synthetic fixture\n").unwrap();
    assert!(
        validate_ground(root.path(), &Ground { kind: GroundKind::Repository, reference: ".codex/fixture.md#synthetic-fixture".into() })
            .is_err()
    );
}

#[test]
fn grounds_do_not_treat_fenced_or_indented_code_as_document_headings() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("design.md"), "```rust\n# Fake heading\n```\n    # Indented code\n#No space\n## Real heading\n")
        .unwrap();
    for heading in ["fake-heading", "indented-code", "no-space"] {
        assert!(validate_ground(root.path(), &Ground { kind: GroundKind::Repository, reference: format!("design.md#{heading}") }).is_err());
    }
    assert!(validate_ground(root.path(), &Ground { kind: GroundKind::Repository, reference: "design.md#real-heading".into() }).is_ok());
}

#[test]
fn fast_path_checks_existing_attribute_and_actual_risk_delta() {
    let before = sheet();
    let mut after = before.clone();
    let bar = software_bar(&after);
    let candidate = FastPathCandidate {
        changed_piece: "change".into(),
        changed_attribute_or_fact: "rust.fmt".into(),
        is_reversible: true,
        risk_switched_on: false,
    };
    assert!(candidate.qualifies_for(&before, &after, &bar));
    after.risk.money = Some(true);
    assert!(!candidate.qualifies_for(&before, &after, &bar), "a caller cannot lie about a newly enabled risk");
    let mut candidate = candidate;
    candidate.changed_attribute_or_fact = "rust.fmt,screen.layout".into();
    assert!(!candidate.qualifies_for(&before, &before, &bar));
}

#[test]
fn failed_spend_path_goes_to_held_writer_and_failed_screen_to_drafts() {
    let sheet = sheet();
    let bar = software_bar(&sheet);
    let spend = bar.iter().find(|c| c.id == "desktop.spend-paths").unwrap();
    let mut lease = faber_engine::project::Lease::new("repo", "mission", "writer", 1, vec!["engine/src".into()], "base", "contract", 1);
    lease.generation = 1;
    assert_eq!(
        failed_check_target(spend, "engine/src/project.rs", "mission", &[lease.clone()]).unwrap(),
        RepairTarget::Owner { task: "writer".into(), generation: 1 }
    );
    lease.state = faber_engine::project::LeaseState::Released;
    assert!(failed_check_target(spend, "engine/src/project.rs", "mission", &[lease]).is_err());
    let screen = bar.iter().find(|c| c.id == "screen.coherence-widths").unwrap();
    assert_eq!(failed_check_target(screen, "ui/App.tsx", "mission", &[]).unwrap(), RepairTarget::Drafts(screen.piece.clone()));
}
