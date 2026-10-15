use crate::support;
use faber_engine::project::bar::*;
use faber_engine::project::lead::*;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{ArtifactRef, Assumption, ChangeKind};
use faber_engine::project::{Extra, Task, TaskKind};
use support::canonical_tempdir as tempdir;

fn sheet() -> DeliverableSheet {
    DeliverableSheet {
        pieces: vec![Piece {
            id: "code".into(),
            kind: "change".into(),
            lives_in: "desktop".into(),
            paths: vec!["engine/src/example.rs".into()],
            made_from: vec![],
            pairs_with: vec![],
            protected: vec![],
            change_kind: Some(ChangeKind::BugFix),
        }],
        facets: Facets::default(),
        risk: RiskAnswers::faber_defaults(),
        facts: vec![],
        questions: vec![],
        assumptions: vec![Assumption { id: "A1".into(), text: "No UI change".into(), source: "audited paths".into(), confirmed: false }],
        user_actions: vec![],
        taste: vec![],
    }
}

fn tasks(sheet: &DeliverableSheet) -> Vec<Task> {
    software_bar(sheet)
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut task = Task::new(&format!("proof-{i}"), &c.check, TaskKind::Research);
            task.checks = vec![c.id.clone()];
            task.proofs = vec![c.proof.clone()];
            task.paths = c.paths.clone();
            task
        })
        .collect()
}

#[test]
fn compiler_persists_plan_sheet_facts_and_complete_phase_receipt() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir_all(root.join("engine/src")).unwrap();
    std::fs::write(root.join("engine/src/example.rs"), "base").unwrap();
    let journal = Journal::open(&root, &dir.path().join("data"), "mission").unwrap();
    let sheet = sheet();
    let mut tasks = tasks(&sheet);
    tasks.push(Task::new("dead", "Unnecessary work", TaskKind::Research));
    let compiled = compile_software(&root, "mission", 1, &"a".repeat(64), sheet, tasks, vec![]).unwrap();
    assert_eq!(compiled.receipt.phases.len(), 5);
    assert_eq!(compiled.receipt.dropped[0].id, "dead");
    assert!(!compiled.plan.tasks.iter().any(|t| t.id == "dead"));
    journal.record_compilation(&compiled.plan, &compiled.receipt).unwrap();
    assert_eq!(journal.sheet().unwrap().unwrap(), compiled.plan.sheet.clone().unwrap());
    assert_eq!(
        journal.facts().unwrap().unwrap().facts.iter().find(|f| f.key == "device.platforms").unwrap().value,
        "Windows, macOS, Linux"
    );
    assert!(journal.verify_acceptance(&root, &"a".repeat(64)).is_err(), "no proof ran");
}

#[test]
fn acceptance_hashes_evidence_and_rejects_public_receipt_tampering() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir_all(root.join("engine/src")).unwrap();
    std::fs::write(root.join("engine/src/example.rs"), "base").unwrap();
    std::fs::create_dir_all(root.join("proof")).unwrap();
    std::fs::write(root.join("proof/result.json"), "executed").unwrap();
    let journal = Journal::open(&root, &dir.path().join("data"), "mission").unwrap();
    let sheet = sheet();
    let tasks = tasks(&sheet);
    let compiled = compile_software(&root, "mission", 1, &"a".repeat(64), sheet, tasks, vec![]).unwrap();
    assert_eq!(compiled.receipt.check_receipts.len(), compiled.plan.tasks.len(), "reserve every proof receipt before dispatch");
    journal.record_compilation(&compiled.plan, &compiled.receipt).unwrap();
    assert!(journal.verify_acceptance(&root, &compiled.receipt.candidate).is_err(), "no reserved proof has run");
    let artifact = ArtifactRef { path: "proof/result.json".into(), sha256: faber_engine::project::lease::hash(b"executed") };
    for c in &compiled.plan.bar {
        let receipt = CheckReceipt {
            schema: 1,
            id: check_receipt_id(&compiled.plan.mission, compiled.plan.revision, &c.owner),
            task: c.owner.clone(),
            candidate: compiled.receipt.candidate.clone(),
            checks: vec![CheckOutcome {
                id: c.id.clone(),
                kind: c.kind,
                result: if c.from == "library:software@2:not-applicable" { CheckResult::NotRun } else { CheckResult::Pass },
                evidence: vec![artifact.clone()],
                reason: if c.from == "library:software@2:not-applicable" { Some("reason.proof.notApplicable".into()) } else { None },
            }],
            artifacts: vec![artifact.clone()],
            model: "fixture-native".into(),
            effort: "none".into(),
            cost_micros: Some(0),
            effect_receipts: vec![],
            extra: Extra::new(),
        };
        journal.save_check_receipt(&receipt).unwrap();
    }
    assert!(journal.verify_acceptance(&root, &compiled.receipt.candidate).is_ok());
    std::fs::write(journal.records().join("checks").join(format!("{}.json", compiled.receipt.check_receipts[0])), "{}").unwrap();
    assert!(journal.verify_acceptance(&root, &compiled.receipt.candidate).is_ok(), "public copies grant no proof authority");
    std::fs::write(root.join("proof/result.json"), "changed").unwrap();
    assert!(journal.verify_acceptance(&root, &compiled.receipt.candidate).is_err());
}

#[test]
fn deleting_public_copies_cannot_allow_private_receipt_replacement() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir_all(root.join("engine/src")).unwrap();
    std::fs::write(root.join("engine/src/example.rs"), "base").unwrap();
    let journal = Journal::open(&root, &dir.path().join("data"), "mission").unwrap();
    let sheet = sheet();
    let tasks = tasks(&sheet);
    let compiled = compile_software(&root, "mission", 1, &"a".repeat(64), sheet, tasks, vec![]).unwrap();
    journal.record_compilation(&compiled.plan, &compiled.receipt).unwrap();
    std::fs::remove_file(journal.records().join("compiles/rev-1.json")).unwrap();
    assert!(journal.save_compile_receipt(&compiled.receipt).is_err());
}

#[test]
fn older_software_records_remain_readable_but_have_no_acceptance_authority() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    let journal = Journal::open(&root, &dir.path().join("data"), "mission").unwrap();
    let sheet = sheet();
    let mut plan = faber_engine::project::Plan::new("mission", 1, vec![]);
    plan.sheet = Some(sheet.clone());
    plan.bar = software_bar(&sheet);
    for check in &mut plan.bar {
        check.mandatory = false;
    }
    plan.extra.insert("future".into(), serde_json::json!({"keep":true}));
    std::fs::write(journal.records().join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
    let loaded = journal.plan().unwrap().unwrap();
    assert_eq!(loaded, plan);
    let old_compile = CompileReceipt {
        schema: 1,
        id: "old".into(),
        mission: "mission".into(),
        plan_revision: 1,
        candidate: "legacy-candidate".into(),
        phases: vec![],
        check_receipts: vec![],
        dropped: vec![],
        extra: Extra::new(),
    };
    std::fs::create_dir_all(journal.records().join("compiles")).unwrap();
    std::fs::write(journal.records().join("compiles/rev-1.json"), serde_json::to_vec(&old_compile).unwrap()).unwrap();
    assert_eq!(journal.compile_receipt(1).unwrap().unwrap(), old_compile);
    assert!(journal.verify_acceptance(&root, &"a".repeat(64)).is_err());
}

#[test]
fn sheet_records_keep_version_extras_and_the_legacy_display_shape() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    let journal = Journal::open(&root, &dir.path().join("data"), "mission").unwrap();
    let sheet = sheet();
    std::fs::write(journal.records().join("sheet.json"), serde_json::to_vec(&sheet).unwrap()).unwrap();
    assert_eq!(journal.sheet().unwrap().unwrap(), sheet);
    journal.save_sheet(&sheet).unwrap();
    let path = journal.records().join("sheet-record.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(record["schema"], 1);
    record["future"] = serde_json::json!({"keep":true});
    std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert_eq!(journal.sheet().unwrap().unwrap(), sheet);
    journal.save_sheet(&sheet).unwrap();
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(kept["future"], record["future"]);
    let legacy: DeliverableSheet = serde_json::from_slice(&std::fs::read(journal.records().join("sheet.json")).unwrap()).unwrap();
    assert_eq!(legacy, sheet, "older readers retain their display shape");
    record["schema"] = serde_json::json!(2);
    std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(journal.sheet().is_err(), "future schemas cannot be guessed at");
}

#[test]
fn software_library_miss_requires_isolated_read_only_vendors_and_fresh_critic() {
    use faber_engine::model::Vendor;
    use faber_engine::project::Role;
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("design.md"), "# Contract\n").unwrap();
    let mut check = software_bar(&sheet()).remove(0);
    check.mandatory = false;
    check.ground = Ground { kind: GroundKind::Repository, reference: "design.md#contract".into() };
    let a = BarDraft { vendor: Vendor::Claude, session: "draft-a".into(), role: Role::Research, checks: vec![check.clone()] };
    let b = BarDraft { vendor: Vendor::Codex, session: "draft-b".into(), role: Role::Research, checks: vec![check] };
    let critic = BarDraft { vendor: Vendor::Codex, session: "critic".into(), role: Role::Reviewer, checks: vec![] };
    assert!(merge_software_miss(root.path(), Vendor::Claude, "writer", &a, &b, &critic).is_ok());
    let mut unsafe_draft = a.clone();
    unsafe_draft.session = "writer".into();
    assert!(merge_software_miss(root.path(), Vendor::Claude, "writer", &unsafe_draft, &b, &critic).is_err());
    unsafe_draft = a.clone();
    unsafe_draft.role = Role::Writer;
    assert!(merge_software_miss(root.path(), Vendor::Claude, "writer", &unsafe_draft, &b, &critic).is_err());
    let mut unsafe_critic = critic.clone();
    unsafe_critic.vendor = Vendor::Claude;
    assert!(merge_software_miss(root.path(), Vendor::Claude, "writer", &a, &b, &unsafe_critic).is_err());
}

#[test]
fn refactor_requires_base_golden_compatible_shapes_and_measured_noise() {
    use faber_engine::project::proof::{RefactorEvidence, validate_refactor};
    let good = RefactorEvidence {
        captured_at: 10,
        code_started_at: 20,
        golden_base: std::collections::BTreeMap::from([("output".into(), "a".repeat(64))]),
        golden_after: std::collections::BTreeMap::from([("output".into(), "a".repeat(64))]),
        api_base: b"public shape".to_vec(),
        api_after: b"public shape".to_vec(),
        schema_base: b"schema1".to_vec(),
        schema_after: b"schema1".to_vec(),
        benchmark_base: vec![99, 100, 101],
        benchmark_after: vec![100, 100, 101],
    };
    assert!(validate_refactor(&good).is_ok());
    for mutation in 0..5 {
        let mut bad = good.clone();
        match mutation {
            0 => bad.captured_at = 30,
            1 => bad.api_after.push(b'2'),
            2 => bad.schema_after.push(b'2'),
            3 => bad.benchmark_after = vec![120, 121, 122],
            _ => bad.golden_after.clear(),
        }
        assert!(validate_refactor(&bad).is_err());
    }
}

#[test]
fn captured_inputs_change_digest_on_content_and_environment() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("source.rs"), "base").unwrap();
    let mut candidate = faber_engine::project::proof::CheckCandidate {
        check_id: "fmt".into(),
        brief: "format".into(),
        inputs: vec![],
        loadout: "native".into(),
        model: "none".into(),
        base_revision: "base".into(),
        toolchain: "cargo".into(),
        dependencies: vec![],
        environment: vec![],
    };
    candidate.capture_inputs(root, &["source.rs".into()]).unwrap();
    let first = faber_engine::project::proof::compute_check_digest(&candidate);
    std::fs::write(root.join("source.rs"), "change").unwrap();
    candidate.capture_inputs(root, &["source.rs".into()]).unwrap();
    assert_ne!(first, faber_engine::project::proof::compute_check_digest(&candidate));
    assert!(candidate.capture_inputs(root, &["../escape".into()]).is_err());
}

#[test]
fn durable_cache_reuses_only_identical_inputs_and_verified_passes() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir_all(root.join("proof")).unwrap();
    std::fs::write(root.join("proof/log"), "executed").unwrap();
    std::fs::write(root.join("source.rs"), "base").unwrap();
    let data = dir.path().join("data");
    let journal = Journal::open(&root, &data, "mission").unwrap();
    let mut candidate = faber_engine::project::proof::CheckCandidate {
        check_id: "fmt".into(),
        brief: "format".into(),
        inputs: vec![],
        loadout: "native".into(),
        model: "none".into(),
        base_revision: "base".into(),
        toolchain: "cargo".into(),
        dependencies: vec![],
        environment: vec![],
    };
    candidate.capture_inputs(&root, &["source.rs".into()]).unwrap();
    let result = CheckOutcome {
        id: "fmt".into(),
        kind: CheckKind::Program,
        result: CheckResult::Pass,
        evidence: vec![ArtifactRef { path: "proof/log".into(), sha256: faber_engine::project::lease::hash(b"executed") }],
        reason: None,
    };
    journal.cache_check(&root, "task", &candidate, &result).unwrap();
    drop(journal);
    let journal = Journal::open(&root, &data, "mission").unwrap();
    assert!(journal.reuse_check(&root, "task", &candidate).unwrap().is_some());
    for changed in 0..4 {
        let mut changed_candidate = candidate.clone();
        match changed {
            0 => changed_candidate.toolchain.push('2'),
            1 => changed_candidate.environment.push("OS=changed".into()),
            2 => changed_candidate.dependencies.push("dep=2".into()),
            _ => changed_candidate.inputs[0] = serde_json::json!({"path":"source.rs","sha256":"0".repeat(64)}).to_string(),
        }
        assert!(journal.reuse_check(&root, "task", &changed_candidate).unwrap().is_none());
    }
    std::fs::write(root.join("source.rs"), "changed input").unwrap();
    assert!(journal.reuse_check(&root, "task", &candidate).unwrap().is_none(), "stale caller snapshots cannot authorize reuse");
    std::fs::write(root.join("source.rs"), "base").unwrap();
    let mut not_run = result.clone();
    not_run.result = CheckResult::NotRun;
    not_run.reason = Some("reason.proof.offline".into());
    assert!(journal.cache_check(&root, "task", &candidate, &not_run).is_err());
    assert!(journal.reuse_check(&root, "task", &candidate).unwrap().is_none());
    journal.cache_check(&root, "task", &candidate, &result).unwrap();
    std::fs::write(root.join("proof/log"), "changed").unwrap();
    assert!(journal.reuse_check(&root, "task", &candidate).is_err());
}
