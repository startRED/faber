use faber_engine::project::bar::*;
use faber_engine::project::proof::*;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{
    ArtifactRef, Assumption, CheckOutcome, CheckReceipt, CheckResult, CompilePhase, CompilePhaseKind, CompileReceipt, DeliverableSheet,
    Facets, Fact, FactSheet, Piece, RiskAnswers, SheetQuestion,
};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn deliverable_sheet_validates_pieces_contexts_and_seven_risk_answers() {
    let sheet = DeliverableSheet {
        pieces: vec![
            Piece {
                id: "p1".into(),
                kind: "screen".into(),
                lives_in: "720 px, 900 px".into(),
                paths: vec!["ui/Composer.tsx".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
            Piece {
                id: "p2".into(),
                kind: "change".into(),
                lives_in: "engine/src".into(),
                paths: vec!["engine/src/project.rs".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
            Piece {
                id: "p3".into(),
                kind: "doc".into(),
                lives_in: "docs".into(),
                paths: vec!["docs/roadmap.md".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
        ],
        facets: Facets {
            purpose: "personal".into(),
            audience: "developer".into(),
            deliverable: "desktop".into(),
            stage: "build".into(),
            region: "BR".into(),
        },
        risk: RiskAnswers {
            public: Some(true),
            others_data: Some(false),
            money: Some(false),
            minors: Some(false),
            regulated: Some(false),
            commercial: Some(false),
            irreversible: vec!["publish".into()],
        },
        facts: vec![],
        questions: vec![],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    };

    assert!(sheet.validate().is_ok());
    assert_eq!(sheet.risk.public, Some(true));
    assert_eq!(sheet.risk.others_data, Some(false));
    assert_eq!(sheet.risk.money, Some(false));
    assert_eq!(sheet.risk.minors, Some(false));
    assert_eq!(sheet.risk.regulated, Some(false));
    assert_eq!(sheet.risk.commercial, Some(false));
    assert_eq!(sheet.risk.irreversible, vec!["publish"]);
}

#[test]
fn faber_public_and_device_facts_are_seeded() {
    let defaults = RiskAnswers::faber_defaults();
    assert_eq!(defaults.public, Some(true));
    assert_eq!(defaults.others_data, Some(false));
    assert_eq!(defaults.money, Some(false));
    assert_eq!(defaults.minors, Some(false));
    assert_eq!(defaults.regulated, Some(false));
    assert_eq!(defaults.commercial, Some(false));
    assert!(defaults.irreversible.is_empty());
}

#[test]
fn sheet_questions_reject_non_result_changing_and_enforce_three_limit() {
    let mut sheet = DeliverableSheet {
        pieces: vec![Piece {
            id: "p1".into(),
            kind: "change".into(),
            lives_in: "desktop".into(),
            paths: vec!["src/main.rs".into()],
            made_from: vec![],
            pairs_with: vec![],
            protected: vec![],
            change_kind: None,
        }],
        facets: Facets::default(),
        risk: RiskAnswers::faber_defaults(),
        facts: vec![],
        questions: vec![SheetQuestion {
            text: "Cosmetic question that changes no check?".into(),
            options: vec!["yes".into(), "no".into()],
            default: "yes".into(),
            changes: vec![], // Empty changes! Not result-changing!
        }],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    };

    // A question with empty `changes` must fail validation (only result-changing ambiguities ask)
    assert!(sheet.validate().is_err());

    // With a result-changing attribute/check:
    sheet.questions[0].changes = vec!["bar:rust.unwrap".into()];
    assert!(sheet.validate().is_ok());

    // More than 3 questions must fail validation:
    for i in 1..4 {
        sheet.questions.push(SheetQuestion {
            text: format!("Question {i}"),
            options: vec!["a".into(), "b".into()],
            default: "a".into(),
            changes: vec![format!("bar:check.{i}")],
        });
    }
    assert_eq!(sheet.questions.len(), 4);
    assert!(sheet.validate().is_err());
}

#[test]
fn assumptions_must_have_provenance() {
    let mut a = Assumption {
        id: "A1".into(),
        text: "Assume single-user desktop".into(),
        source: "".into(), // missing provenance!
        confirmed: false,
    };
    assert!(a.validate().is_err());

    a.source = "request".into();
    assert!(a.validate().is_ok());
}

#[test]
fn fact_sheet_persists_in_journal_and_validates_readers() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let data = dir.path().join("data");
    let journal = Journal::open(&repo, &data, "m1").unwrap();

    let fact_sheet = FactSheet {
        schema: 1,
        mission: "m1".into(),
        facts: vec![Fact { key: "desktop.min_width".into(), value: "720".into(), source: "AGENTS.md".into(), readers: vec!["p1".into()] }],
        extra: Default::default(),
    };

    journal.save_facts(&fact_sheet).unwrap();
    let read = journal.facts().unwrap().expect("fact sheet should exist");
    assert_eq!(read.facts.len(), 1);
    assert_eq!(read.facts[0].key, "desktop.min_width");
    assert_eq!(read.facts[0].value, "720");
    assert_eq!(read.facts[0].readers, vec!["p1"]);
}

#[test]
fn complete_software_bar_contains_all_b1_to_b10_groups() {
    let sheet = DeliverableSheet {
        pieces: vec![
            Piece {
                id: "rust-core".into(),
                kind: "change".into(),
                lives_in: "engine/src".into(),
                paths: vec!["engine/src/project.rs".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
            Piece {
                id: "ui-screen".into(),
                kind: "screen".into(),
                lives_in: "720 px".into(),
                paths: vec!["ui/App.tsx".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
        ],
        facets: Facets {
            purpose: "personal".into(),
            audience: "self".into(),
            deliverable: "desktop".into(),
            stage: "build".into(),
            region: "BR".into(),
        },
        risk: RiskAnswers::faber_defaults(),
        facts: vec![],
        questions: vec![],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    };

    let bar = software_bar(&sheet);
    let ids: Vec<&str> = bar.iter().map(|c| c.id.as_str()).collect();

    // B1: Software Line
    assert!(ids.contains(&"software.risk-answers"));
    assert!(ids.contains(&"software.callers-map"));
    assert!(ids.contains(&"software.test-first"));
    assert!(ids.contains(&"software.dependency-registry"));
    assert!(ids.contains(&"software.quick-tests"));
    assert!(ids.contains(&"software.secret-scan"));
    assert!(ids.contains(&"software.other-vendor-review"));
    assert!(ids.contains(&"software.ui-screen-check"));
    assert!(ids.contains(&"software.full-suite"));
    assert!(ids.contains(&"software.receipt"));
    assert!(ids.contains(&"software.fast-path"));

    // B2: Client website adapted to desktop
    assert!(ids.contains(&"desktop.spend-paths"));
    assert!(ids.contains(&"desktop.real-width-drafts"));
    assert!(ids.contains(&"desktop.ghost-hurried"));
    assert!(ids.contains(&"desktop.ghost-novice"));
    assert!(ids.contains(&"desktop.ghost-distracted"));
    assert!(ids.contains(&"desktop.ghost-keyboard"));
    assert!(ids.contains(&"desktop.staging-only"));
    assert!(ids.contains(&"desktop.no-signing-keys"));

    // B3: Rust
    assert!(ids.contains(&"rust.fmt"));
    assert!(ids.contains(&"rust.clippy"));
    assert!(ids.contains(&"rust.audit-unwrap"));
    assert!(ids.contains(&"rust.no-blocking-async"));
    assert!(ids.contains(&"rust.unsafe-safety-note"));
    assert!(ids.contains(&"rust.locale-keys"));

    // B4: Processes and data
    assert!(ids.contains(&"process.shell-free"));
    assert!(ids.contains(&"process.no-visible-windows"));
    assert!(ids.contains(&"process.stop-kills-tree"));
    assert!(ids.contains(&"data.atomic-versioned"));
    assert!(ids.contains(&"data.crash-matrix"));
    assert!(ids.contains(&"process.idle-memory"));
    assert!(ids.contains(&"process.long-lived-audit"));
    assert!(ids.contains(&"data.credential-folders-untouched"));

    // B5: Tauri
    assert!(ids.contains(&"tauri.command-inventory"));
    assert!(ids.contains(&"tauri.csp"));
    assert!(ids.contains(&"tauri.no-remote-content"));
    assert!(ids.contains(&"tauri.scoped-input-checks"));
    assert!(ids.contains(&"tauri.updater-signature"));
    assert!(ids.contains(&"tauri.webview-ci"));

    // B6: SolidJS, TypeScript and screen
    assert!(ids.contains(&"screen.lint"));
    assert!(ids.contains(&"screen.typecheck"));
    assert!(ids.contains(&"screen.build"));
    assert!(ids.contains(&"screen.no-props-destructure"));
    assert!(ids.contains(&"screen.windowed-lists"));
    assert!(ids.contains(&"screen.locales-hand-authored"));
    assert!(ids.contains(&"screen.design-tokens"));
    assert!(ids.contains(&"screen.no-duplicate-controls"));
    assert!(ids.contains(&"screen.coherence-widths"));

    // B7: Accessibility
    assert!(ids.contains(&"a11y.contrast"));
    assert!(ids.contains(&"a11y.target-size"));
    assert!(ids.contains(&"a11y.keyboard-reach"));
    assert!(ids.contains(&"a11y.reduced-motion"));
    assert!(ids.contains(&"a11y.text-size"));

    // B8: Supply chain and safety
    assert!(ids.contains(&"safety.osv-vulnerabilities"));
    assert!(ids.contains(&"safety.spdx-licenses"));
    assert!(ids.contains(&"safety.secret-scan"));
    assert!(ids.contains(&"safety.no-silent-telemetry"));

    // B9: Change kinds
    assert!(ids.contains(&"kind.feature-test-first"));
    assert!(ids.contains(&"kind.bugfix-reproduce"));
    assert!(ids.contains(&"kind.refactor-golden"));
    assert!(ids.contains(&"kind.upgrade-changelog"));

    // B10: Any change
    assert!(ids.contains(&"any.root-cause-regression"));
    assert!(ids.contains(&"any.edge-cases"));
    assert!(ids.contains(&"any.schema-versioned"));
    assert!(ids.contains(&"any.token-latency"));
    assert!(ids.contains(&"any.docs-and-locales"));
    assert!(ids.contains(&"any.coherent-commit"));
}

#[test]
fn owner_e10_rejects_removing_mandatory_checks() {
    let sheet = DeliverableSheet {
        pieces: vec![Piece {
            id: "p1".into(),
            kind: "change".into(),
            lives_in: "engine".into(),
            paths: vec!["engine/src/main.rs".into()],
            made_from: vec![],
            pairs_with: vec![],
            protected: vec![],
            change_kind: None,
        }],
        facets: Facets::default(),
        risk: RiskAnswers::faber_defaults(),
        facts: vec![],
        questions: vec![],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    };

    let full_bar = software_bar(&sheet);
    assert!(verify_mandatory_checks(&sheet, &full_bar).is_ok());

    // Remove a mandatory check: rust.fmt
    let mut bad_bar = full_bar.clone();
    bad_bar.retain(|c| c.id != "rust.fmt");
    let err = verify_mandatory_checks(&sheet, &bad_bar).unwrap_err();
    assert!(matches!(err, BarError::MissingMandatoryCheck(ref id) if id == "rust.fmt"));

    // Remove another mandatory check: safety.secret-scan
    let mut bad_bar2 = full_bar.clone();
    bad_bar2.retain(|c| c.id != "safety.secret-scan");
    let err2 = verify_mandatory_checks(&sheet, &bad_bar2).unwrap_err();
    assert!(matches!(err2, BarError::MissingMandatoryCheck(ref id) if id == "safety.secret-scan"));
}

#[test]
fn grounded_additions_verify_repo_heading_exists() {
    let repo_dir = tempdir().unwrap();
    let doc_path = repo_dir.path().join("docs");
    std::fs::create_dir_all(&doc_path).unwrap();
    std::fs::write(doc_path.join("design.md"), "# Overview\n\nSome text.\n\n## Screen tests\n\nTesting screen.\n").unwrap();

    // Valid heading:
    let valid_ground = Ground { kind: GroundKind::Repository, reference: "docs/design.md#screen-tests".into() };
    assert!(validate_ground(repo_dir.path(), &valid_ground).is_ok());

    // Non-existent heading:
    let invalid_heading = Ground { kind: GroundKind::Repository, reference: "docs/design.md#non-existent-section".into() };
    assert!(matches!(validate_ground(repo_dir.path(), &invalid_heading), Err(BarError::InvalidGround(_))));

    // Non-existent file:
    let invalid_file = Ground { kind: GroundKind::Repository, reference: "docs/missing.md#overview".into() };
    assert!(matches!(validate_ground(repo_dir.path(), &invalid_file), Err(BarError::InvalidGround(_))));
}

#[test]
fn grounded_additions_validate_official_urls() {
    let repo_dir = tempdir().unwrap();

    let valid_url = Ground { kind: GroundKind::Standard, reference: "https://www.w3.org/TR/WCAG22/".into() };
    assert!(validate_ground(repo_dir.path(), &valid_url).is_ok());

    let invalid_url = Ground { kind: GroundKind::Standard, reference: "not-a-valid-url".into() };
    assert!(matches!(validate_ground(repo_dir.path(), &invalid_url), Err(BarError::InvalidGround(_))));
}

#[test]
fn judged_check_filters_enforce_caps() {
    let mut checks = vec![];
    // Create 13 judged checks:
    for i in 0..13 {
        checks.push(BarCheck {
            id: format!("judge.{i}"),
            piece: "p1".into(),
            attribute: format!("attr.{i}"),
            check: format!("Check {i}"),
            kind: CheckKind::Judge,
            severity: Severity::Minor,
            ground: Ground { kind: GroundKind::Evidence, reference: "outlier".into() },
            proof: "proof.judge".into(),
            owner: "task1".into(),
            paths: vec!["file.rs".into()],
            pass: "pass".into(),
            evidence: "evidence".into(),
            also: vec![],
            from: "drafter".into(),
            mandatory: false,
        });
    }

    // > 12 judged checks -> TooManyJudgedChecks
    assert!(matches!(filter_and_validate_bar("p1", &checks), Err(BarError::TooManyJudgedChecks)));

    // Reduce to 10 judged checks, but 7 are blocking:
    checks.truncate(10);
    for c in checks.iter_mut().take(7) {
        c.severity = Severity::Blocking;
    }
    // > 6 blocking judged checks -> TooManyBlockingJudgedChecks
    assert!(matches!(filter_and_validate_bar("p1", &checks), Err(BarError::TooManyBlockingJudgedChecks)));

    // Reduce blocking to 4, but make 4 out of 10 checks have GroundKind::Judgement (40% > 30%):
    for c in checks.iter_mut().take(4) {
        c.severity = Severity::Minor;
    }
    for c in checks.iter_mut().take(4) {
        c.ground.kind = GroundKind::Judgement;
    }
    // 40% judgement-only -> TooManyJudgementOnly
    assert!(matches!(filter_and_validate_bar("p1", &checks), Err(BarError::TooManyJudgementOnly)));

    // Make 2 out of 10 (20% <= 30%) judgement-only:
    for c in checks.iter_mut().skip(2).take(2) {
        c.ground.kind = GroundKind::Evidence;
    }
    assert!(filter_and_validate_bar("p1", &checks).is_ok());
}

#[test]
fn drafters_merge_and_critic_normalizes_and_caps_critic() {
    let drafter_a = vec![BarCheck {
        id: "check.a".into(),
        piece: "p1".into(),
        attribute: "Color.Contrast".into(), // mixed case
        check: "Verify contrast".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Standard, reference: "https://www.w3.org/".into() },
        proof: "proof.a".into(),
        owner: "t1".into(),
        paths: vec!["ui/App.tsx".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "drafter-a".into(),
        mandatory: false,
    }];

    let drafter_b = vec![BarCheck {
        id: "check.b".into(),
        piece: "p1".into(),
        attribute: "color.contrast".into(), // matches normalized
        check: "Ensure contrast is 4.5:1".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Standard, reference: "https://www.w3.org/".into() },
        proof: "proof.b".into(),
        owner: "t1".into(),
        paths: vec!["ui/App.tsx".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "drafter-b".into(),
        mandatory: false,
    }];

    let merged = merge_drafters(&drafter_a, &drafter_b);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].attribute, "color.contrast");
    assert_eq!(merged[0].from, "agreed");

    // Critic can add up to 3 additions:
    let mut critic_additions = vec![];
    for i in 0..4 {
        critic_additions.push(BarCheck {
            id: format!("critic.{i}"),
            piece: "p1".into(),
            attribute: format!("critic.attr.{i}"),
            check: format!("Critic check {i}"),
            kind: CheckKind::Program,
            severity: Severity::Minor,
            ground: Ground { kind: GroundKind::Standard, reference: "https://www.w3.org/".into() },
            proof: "proof.c".into(),
            owner: "t1".into(),
            paths: vec!["ui/App.tsx".into()],
            pass: "pass".into(),
            evidence: "ev".into(),
            also: vec![],
            from: "critic".into(),
            mandatory: false,
        });
    }

    // 4 critic additions exceeds cap of 3
    assert!(apply_critic(&merged, &critic_additions).is_err());

    critic_additions.truncate(3);
    let final_bar = apply_critic(&merged, &critic_additions).unwrap();
    assert_eq!(final_bar.len(), 4);
}

#[test]
fn moves_inserted_by_triggers_for_screens_and_irreversible_actions() {
    let sheet = DeliverableSheet {
        pieces: vec![
            Piece {
                id: "screen-main".into(),
                kind: "screen".into(),
                lives_in: "720 px".into(),
                paths: vec!["ui/App.tsx".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
            Piece {
                id: "code-backend".into(),
                kind: "change".into(),
                lives_in: "engine/src".into(),
                paths: vec!["engine/src/lib.rs".into()],
                made_from: vec![],
                pairs_with: vec![],
                protected: vec![],
                change_kind: None,
            },
        ],
        facets: Facets::default(),
        risk: RiskAnswers {
            public: Some(true),
            others_data: Some(false),
            money: Some(true), // money moves -> spend paths test
            minors: Some(false),
            regulated: Some(false),
            commercial: Some(false),
            irreversible: vec!["publish".into()], // irreversible -> gate
        },
        facts: vec![],
        questions: vec![],
        assumptions: vec![],
        user_actions: vec![],
        taste: vec![],
    };

    let screen_moves = plan_moves_for_piece(&sheet.pieces[0], &sheet);
    assert!(screen_moves.contains(&MoveKind::Reuse));
    assert!(screen_moves.contains(&MoveKind::Diverge));
    assert!(screen_moves.contains(&MoveKind::InPlace));
    assert!(screen_moves.contains(&MoveKind::Measure));
    assert!(screen_moves.contains(&MoveKind::Judge));

    let backend_moves = plan_moves_for_piece(&sheet.pieces[1], &sheet);
    assert!(backend_moves.contains(&MoveKind::Measure)); // spend path tested first
    assert!(backend_moves.contains(&MoveKind::Gate)); // irreversible publish gate
    assert!(backend_moves.contains(&MoveKind::Craft));
    assert!(backend_moves.contains(&MoveKind::Judge));
}

#[test]
fn screen_drafts_judged_at_real_widths_and_ghost_personas() {
    let widths = screen_draft_widths();
    assert_eq!(widths, &[720, 900, 1100]);

    let ghosts = all_ghost_personas();
    assert_eq!(ghosts.len(), 4);
    assert!(ghosts.contains(&GhostPersona::Hurried));
    assert!(ghosts.contains(&GhostPersona::Novice));
    assert!(ghosts.contains(&GhostPersona::Distracted));
    assert!(ghosts.contains(&GhostPersona::KeyboardOnly));
}

#[test]
fn stalled_flow_returns_to_drafts_or_spend_path() {
    // Screen flow stall returns to diverge (drafts):
    assert_eq!(stalled_flow_target(MoveKind::InPlace), MoveKind::Diverge);
    assert_eq!(stalled_flow_target(MoveKind::Judge), MoveKind::Diverge);

    // Spend path stall returns to measure (the spend test):
    assert_eq!(stalled_flow_target(MoveKind::Craft), MoveKind::Diverge);
}

#[test]
fn fast_path_qualification_limits_and_fallback() {
    let valid_fast_path = FastPathCandidate {
        changed_piece: "p1".into(),
        changed_attribute_or_fact: "color.primary".into(),
        is_reversible: true,
        risk_switched_on: false,
    };
    assert!(valid_fast_path.qualifies());

    // Irreversible change cannot take fast path
    let irreversible = FastPathCandidate { is_reversible: false, ..valid_fast_path.clone() };
    assert!(!irreversible.qualifies());

    // Risk switched on cannot take fast path
    let risk_on = FastPathCandidate { risk_switched_on: true, ..valid_fast_path.clone() };
    assert!(!risk_on.qualifies());
}

#[test]
fn compiler_rejects_generic_nodes_lacking_check_or_path() {
    let mut t1 = faber_engine::project::Task::new("t1", "Task 1", faber_engine::project::TaskKind::Writer);
    t1.paths = vec!["src/main.rs".into()];
    t1.checks = vec![]; // missing check!

    let tasks = vec![t1];
    let bar = vec![BarCheck {
        id: "check.1".into(),
        piece: "p1".into(),
        attribute: "attr.1".into(),
        check: "Check 1".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
        proof: "proof.1".into(),
        owner: "t1".into(),
        paths: vec!["src/main.rs".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "lib".into(),
        mandatory: true,
    }];

    // Missing check ID on task -> rejected as generic node
    assert!(matches!(
        validate_compiler_graph(&tasks, &bar, &[]),
        Err(CompilerError::GenericNodeLacksCheck(ref id)) if id == "t1"
    ));

    let mut t2 = faber_engine::project::Task::new("t2", "Task 2", faber_engine::project::TaskKind::Writer);
    t2.checks = vec!["check.1".into()];
    t2.paths = vec![]; // missing path!

    assert!(matches!(
        validate_compiler_graph(&[t2], &bar, &[]),
        Err(CompilerError::GenericNodeLacksPath(ref id)) if id == "t2"
    ));
}

#[test]
fn compiler_rejects_orphan_checks_evidence_and_tasks() {
    let mut t1 = faber_engine::project::Task::new("t1", "Task 1", faber_engine::project::TaskKind::Writer);
    t1.paths = vec!["src/main.rs".into()];
    t1.checks = vec!["check.1".into()];

    // Bar check points to non-existent owner task "missing_task"
    let orphan_check_bar = vec![BarCheck {
        id: "check.orphan".into(),
        piece: "p1".into(),
        attribute: "attr.orphan".into(),
        check: "Orphan Check".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
        proof: "proof.orphan".into(),
        owner: "missing_task".into(), // does not exist in tasks!
        paths: vec!["src/main.rs".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "lib".into(),
        mandatory: false,
    }];

    assert!(matches!(
        validate_compiler_graph(&[t1.clone()], &orphan_check_bar, &[]),
        Err(CompilerError::OrphanCheck(ref id)) if id == "check.orphan"
    ));

    // Task that feeds nothing (no check, no deliverable):
    let mut t_orphan = faber_engine::project::Task::new("t_orphan", "Orphan", faber_engine::project::TaskKind::Writer);
    t_orphan.paths = vec!["src/orphan.rs".into()];
    t_orphan.checks = vec!["check.unrelated".into()]; // check not in bar and no deliverable

    let valid_bar = vec![BarCheck {
        id: "check.1".into(),
        piece: "p1".into(),
        attribute: "attr.1".into(),
        check: "Check 1".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
        proof: "proof.1".into(),
        owner: "t1".into(),
        paths: vec!["src/main.rs".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "lib".into(),
        mandatory: true,
    }];

    assert!(matches!(
        validate_compiler_graph(&[t1, t_orphan], &valid_bar, &[]),
        Err(CompilerError::OrphanTask(ref id)) if id == "t_orphan"
    ));
}

#[test]
fn compiler_back_chains_and_prunes_unneeded_tasks() {
    let mut t1 = faber_engine::project::Task::new("t1", "Task 1", faber_engine::project::TaskKind::Writer);
    t1.paths = vec!["src/main.rs".into()];
    t1.checks = vec!["check.1".into()];

    let mut t_dead = faber_engine::project::Task::new("t_dead", "Dead task", faber_engine::project::TaskKind::Writer);
    t_dead.paths = vec!["src/dead.rs".into()];
    t_dead.checks = vec!["check.unused".into()];

    let bar = vec![BarCheck {
        id: "check.1".into(),
        piece: "p1".into(),
        attribute: "attr.1".into(),
        check: "Check 1".into(),
        kind: CheckKind::Program,
        severity: Severity::Blocking,
        ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
        proof: "proof.1".into(),
        owner: "t1".into(),
        paths: vec!["src/main.rs".into()],
        pass: "pass".into(),
        evidence: "ev".into(),
        also: vec![],
        from: "lib".into(),
        mandatory: true,
    }];

    let (pruned_tasks, dropped) = back_chain_and_prune(vec![t1, t_dead], &bar, &[]);
    assert_eq!(pruned_tasks.len(), 1);
    assert_eq!(pruned_tasks[0].id, "t1");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].id, "t_dead");
}

#[test]
fn compiler_orders_kill_early_cheap_proofs_first() {
    let mut t_cheap = faber_engine::project::Task::new("cheap", "Cheap check", faber_engine::project::TaskKind::Writer);
    t_cheap.paths = vec!["src/cheap.rs".into()];
    t_cheap.checks = vec!["check.cheap".into()];

    let mut t_expensive = faber_engine::project::Task::new("expensive", "Expensive judge", faber_engine::project::TaskKind::Review);
    t_expensive.paths = vec!["src/expensive.rs".into()];
    t_expensive.checks = vec!["check.judge".into()];

    let bar = vec![
        BarCheck {
            id: "check.judge".into(),
            piece: "p1".into(),
            attribute: "attr.judge".into(),
            check: "Judge".into(),
            kind: CheckKind::Judge,
            severity: Severity::Blocking,
            ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
            proof: "proof.judge".into(),
            owner: "expensive".into(),
            paths: vec!["src/expensive.rs".into()],
            pass: "pass".into(),
            evidence: "ev".into(),
            also: vec![],
            from: "lib".into(),
            mandatory: true,
        },
        BarCheck {
            id: "check.cheap".into(),
            piece: "p1".into(),
            attribute: "attr.cheap".into(),
            check: "Program".into(),
            kind: CheckKind::Program,
            severity: Severity::Blocking,
            ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
            proof: "proof.cheap".into(),
            owner: "cheap".into(),
            paths: vec!["src/cheap.rs".into()],
            pass: "pass".into(),
            evidence: "ev".into(),
            also: vec![],
            from: "lib".into(),
            mandatory: true,
        },
    ];

    let ordered = order_kill_early(&[t_expensive, t_cheap], &bar).unwrap();
    assert_eq!(ordered[0].id, "cheap");
    assert_eq!(ordered[1].id, "expensive");
}

#[test]
fn failing_path_lease_holder_owns_repair() {
    let mut t1 = faber_engine::project::Task::new("t1", "Writer 1", faber_engine::project::TaskKind::Writer);
    t1.writes = vec!["engine/src/project.rs".into()];
    t1.paths = vec!["engine/src/project.rs".into()];

    let mut t2 = faber_engine::project::Task::new("t2", "Writer 2", faber_engine::project::TaskKind::Writer);
    t2.writes = vec!["ui/App.tsx".into()];
    t2.paths = vec!["ui/App.tsx".into()];

    let tasks = vec![t1, t2];
    assert_eq!(failing_path_lease_holder("engine/src/project.rs", &tasks), Some("t1".into()));
    assert_eq!(failing_path_lease_holder("ui/App.tsx", &tasks), Some("t2".into()));
    assert_eq!(failing_path_lease_holder("other/file.rs", &tasks), None);
}

#[test]
fn compiler_handles_200_nodes_in_under_200_ms() {
    let mut tasks = vec![];
    let mut bar = vec![];

    for i in 0..200 {
        let id = format!("task_{i}");
        let mut t = faber_engine::project::Task::new(&id, &format!("Task {i}"), faber_engine::project::TaskKind::Writer);
        t.paths = vec![format!("src/file_{i}.rs")];
        t.checks = vec![format!("check.{i}")];
        if i > 0 && i % 2 == 0 {
            t.requires = vec![format!("task_{}", i - 1)];
        }
        tasks.push(t);

        bar.push(BarCheck {
            id: format!("check.{i}"),
            piece: "p1".into(),
            attribute: format!("attr.{i}"),
            check: format!("Check {i}"),
            kind: if i % 5 == 0 { CheckKind::Judge } else { CheckKind::Program },
            severity: Severity::Blocking,
            ground: Ground { kind: GroundKind::Owner, reference: "AGENTS.md".into() },
            proof: format!("proof.{i}"),
            owner: id,
            paths: vec![format!("src/file_{i}.rs")],
            pass: "pass".into(),
            evidence: "ev".into(),
            also: vec![],
            from: "lib".into(),
            mandatory: true,
        });
    }

    let start = std::time::Instant::now();
    let validated = validate_compiler_graph(&tasks, &bar, &[]);
    assert!(validated.is_ok());

    let (pruned, dropped) = back_chain_and_prune(tasks.clone(), &bar, &[]);
    assert_eq!(pruned.len(), 200);
    assert_eq!(dropped.len(), 0);

    let ordered = order_kill_early(&pruned, &bar);
    assert!(ordered.is_ok());

    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() <= 200, "Compilation of 200 nodes took {} ms (limit 200 ms)", elapsed.as_millis());
}

#[test]
fn check_candidate_digest_incorporates_full_toolchain_and_base() {
    let candidate = CheckCandidate {
        check_id: "rust.fmt".into(),
        brief: "format code".into(),
        inputs: vec!["src/main.rs".into()],
        loadout: "rustc-1.99".into(),
        model: "claude-3-7-sonnet".into(),
        base_revision: "commit_abc".into(),
        toolchain: "cargo 1.99".into(),
        dependencies: vec!["serde=1.0".into()],
        environment: vec!["RUST_BACKTRACE=1".into()],
    };

    let digest1 = compute_check_digest(&candidate);
    assert_eq!(digest1.len(), 64);

    // Identical candidate yields identical digest
    let digest2 = compute_check_digest(&candidate);
    assert_eq!(digest1, digest2);

    // Changing toolchain changes digest
    let mut modified = candidate.clone();
    modified.toolchain = "cargo 1.100".into();
    assert_ne!(digest1, compute_check_digest(&modified));

    // Changing base revision changes digest
    let mut modified_base = candidate.clone();
    modified_base.base_revision = "commit_xyz".into();
    assert_ne!(digest1, compute_check_digest(&modified_base));

    // Changing environment changes digest
    let mut modified_env = candidate.clone();
    modified_env.environment = vec!["RUST_BACKTRACE=0".into()];
    assert_ne!(digest1, compute_check_digest(&modified_env));
}

#[test]
fn partial_reruns_reuse_pass_but_never_infer_pass_or_reuse_not_run() {
    let mut cache = CheckCache::default();
    let digest = "a".repeat(64);

    // Pass outcome with evidence
    let pass_outcome = CheckOutcome {
        id: "rust.fmt".into(),
        kind: CheckKind::Program,
        result: CheckResult::Pass,
        evidence: vec![ArtifactRef { path: "target/fmt.txt".into(), sha256: "b".repeat(64) }],
        reason: None,
    };

    cache.record(&digest, &pass_outcome);
    let reused = cache.get(&digest).expect("should find cached entry");
    assert_eq!(reused.result, CheckResult::Pass);

    // NotRun outcome: CANNOT be reused as a pass!
    let not_run_outcome = CheckOutcome {
        id: "screen.webview".into(),
        kind: CheckKind::ProgramJudge,
        result: CheckResult::NotRun,
        evidence: vec![],
        reason: Some("No display server on CI".into()),
    };

    let digest_not_run = "c".repeat(64);
    cache.record(&digest_not_run, &not_run_outcome);
    let cached_not_run = cache.get(&digest_not_run).unwrap();
    assert_eq!(cached_not_run.result, CheckResult::NotRun);
    assert!(!cached_not_run.is_pass());

    // Fail outcome: CANNOT be reused as a pass!
    let fail_outcome = CheckOutcome {
        id: "rust.clippy".into(),
        kind: CheckKind::Program,
        result: CheckResult::Fail,
        evidence: vec![],
        reason: Some("error on line 42".into()),
    };

    let digest_fail = "d".repeat(64);
    cache.record(&digest_fail, &fail_outcome);
    let cached_fail = cache.get(&digest_fail).unwrap();
    assert_eq!(cached_fail.result, CheckResult::Fail);
    assert!(!cached_fail.is_pass());
}

#[test]
fn invalidation_graph_invalidates_only_readers_and_descendants() {
    let mut cache = CheckCache::default();

    // Setup tasks: A -> B -> C; and independent D -> E
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    graph.insert("A".into(), vec!["B".into()]);
    graph.insert("B".into(), vec!["C".into()]);
    graph.insert("C".into(), vec![]);
    graph.insert("D".into(), vec!["E".into()]);
    graph.insert("E".into(), vec![]);

    let ev = vec![ArtifactRef { path: "out".into(), sha256: "0".repeat(64) }];
    let pass =
        |id: &str| CheckOutcome { id: id.into(), kind: CheckKind::Program, result: CheckResult::Pass, evidence: ev.clone(), reason: None };

    cache.record_task("A", &pass("c.A"));
    cache.record_task("B", &pass("c.B"));
    cache.record_task("C", &pass("c.C"));
    cache.record_task("D", &pass("c.D"));
    cache.record_task("E", &pass("c.E"));

    // Changing A must invalidate A, B, C, but retain D and E
    cache.invalidate_changed(&["A".into()], &graph);

    assert!(cache.get_task("A").is_none());
    assert!(cache.get_task("B").is_none());
    assert!(cache.get_task("C").is_none());

    // Independent nodes survive!
    assert!(cache.get_task("D").is_some());
    assert!(cache.get_task("E").is_some());
}

#[test]
fn check_receipt_rejects_inferred_pass_or_candidate_mismatch() {
    let candidate = "c".repeat(64);

    // Valid receipt
    let valid_receipt = CheckReceipt {
        schema: 1,
        id: "cr1".into(),
        task: "task1".into(),
        candidate: candidate.clone(),
        checks: vec![CheckOutcome {
            id: "rust.fmt".into(),
            kind: CheckKind::Program,
            result: CheckResult::Pass,
            evidence: vec![ArtifactRef { path: "target/fmt.txt".into(), sha256: "0".repeat(64) }],
            reason: None,
        }],
        artifacts: vec![ArtifactRef { path: "target/fmt.txt".into(), sha256: "0".repeat(64) }],
        model: "model".into(),
        effort: "low".into(),
        cost_micros: Some(100),
        effect_receipts: vec![],
        extra: Default::default(),
    };

    assert!(validate_check_receipt(&valid_receipt, &candidate).is_ok());

    // Candidate mismatch
    assert!(matches!(validate_check_receipt(&valid_receipt, &"d".repeat(64)), Err(ProofError::CandidateMismatch)));

    // Pass with NO evidence is rejected as inferred pass (Owner decision E10: never infer a pass)
    let mut inferred_pass_receipt = valid_receipt.clone();
    inferred_pass_receipt.checks[0].evidence = vec![];
    assert!(matches!(validate_check_receipt(&inferred_pass_receipt, &candidate), Err(ProofError::InferredPass)));

    // NotRun with empty reason is rejected
    let mut not_run_receipt = valid_receipt;
    not_run_receipt.checks[0].result = CheckResult::NotRun;
    not_run_receipt.checks[0].reason = None; // missing reason!
    assert!(matches!(validate_check_receipt(&not_run_receipt, &candidate), Err(ProofError::MissingReason)));
}

#[test]
fn compile_receipt_stores_phase_timestamps_and_dropped_tasks() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let data = dir.path().join("data");
    let journal = Journal::open(&repo, &data, "m1").unwrap();

    let candidate = "e".repeat(64);
    let receipt = CompileReceipt {
        schema: 1,
        id: "comp1".into(),
        mission: "m1".into(),
        plan_revision: 1,
        candidate: candidate.clone(),
        phases: vec![
            CompilePhase {
                phase: CompilePhaseKind::Sheet,
                started_at: 1000,
                duration_micros: 200,
                cost_tag: "free".into(),
                cost_micros: Some(0),
            },
            CompilePhase {
                phase: CompilePhaseKind::Bar,
                started_at: 1200,
                duration_micros: 300,
                cost_tag: "free".into(),
                cost_micros: Some(0),
            },
            CompilePhase {
                phase: CompilePhaseKind::Steps,
                started_at: 1500,
                duration_micros: 400,
                cost_tag: "free".into(),
                cost_micros: Some(0),
            },
            CompilePhase {
                phase: CompilePhaseKind::Code,
                started_at: 1900,
                duration_micros: 150,
                cost_tag: "free".into(),
                cost_micros: Some(0),
            },
        ],
        check_receipts: vec!["cr1".into()],
        dropped: vec![faber_engine::project::wire::Verbatim { id: "unused_task".into(), text: "dropped during back-chaining".into() }],
        extra: Default::default(),
    };

    assert!(validate_compile_receipt(&receipt, &candidate).is_ok());

    journal.save_compile_receipt(&receipt).unwrap();
    let loaded = journal.compile_receipt(1).unwrap().expect("compile receipt should exist");
    assert_eq!(loaded.id, "comp1");
    assert_eq!(loaded.phases.len(), 4);
    assert_eq!(loaded.dropped.len(), 1);
    assert_eq!(loaded.dropped[0].id, "unused_task");
}

#[test]
fn secret_scanner_detects_keys_and_tokens() {
    let clean = "fn main() { println!(\"Hello\"); }";
    assert!(scan_secrets(clean).is_ok());

    let with_private_key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0...\n-----END RSA PRIVATE KEY-----";
    let violations = scan_secrets(with_private_key).unwrap_err();
    assert!(!violations.is_empty());

    let with_aws_key = "let aws_key = \"AKIAIOSFODNN7EXAMPLE\";";
    let violations_aws = scan_secrets(with_aws_key).unwrap_err();
    assert!(!violations_aws.is_empty());

    let with_github_pat = "let token = \"ghp_1234567890abcdefghijklmnopqrstuvwxyz\";";
    let violations_gh = scan_secrets(with_github_pat).unwrap_err();
    assert!(!violations_gh.is_empty());
}

#[test]
fn spdx_license_checker_flags_copyleft_for_commercial() {
    let permissive = &["MIT", "Apache-2.0", "BSD-3-Clause", "FSL-1.1-ALv2"];
    assert!(check_spdx_licenses(permissive, true).is_ok());

    let copyleft = &["MIT", "GPL-3.0", "AGPL-3.0"];
    let flagged = check_spdx_licenses(copyleft, true).unwrap_err();
    assert!(flagged.contains(&"GPL-3.0".to_string()));
    assert!(flagged.contains(&"AGPL-3.0".to_string()));

    // Non-commercial mode allows copyleft:
    assert!(check_spdx_licenses(copyleft, false).is_ok());
}

#[test]
fn registry_checker_flags_typo_squatting() {
    let popular = &["serde", "tokio", "react", "solid-js", "tauri"];

    // Exact match passes
    assert!(check_dependency_registry("serde", popular).is_ok());
    assert!(check_dependency_registry("tokio", popular).is_ok());

    // Near typo distance 1 or 2 is flagged
    assert!(check_dependency_registry("serdde", popular).is_err());
    assert!(check_dependency_registry("tokiio", popular).is_err());
    assert!(check_dependency_registry("taurii", popular).is_err());

    // Distinct package name passes
    assert!(check_dependency_registry("completely-different-crate", popular).is_ok());
}

#[test]
fn osv_advisory_checker_matches_known_advisories_offline() {
    let fixture_content = r#"[
        {
            "package": "vulnerable-crate",
            "vulnerable_versions": ["0.1.0", "0.1.1"],
            "id": "RUSTSEC-2026-0001",
            "summary": "Buffer overflow in vulnerable-crate"
        }
    ]"#;

    let db: Vec<OsvAdvisory> = serde_json::from_str(fixture_content).unwrap();

    let safe_packages = &[("vulnerable-crate", "0.2.0"), ("safe-crate", "1.0.0")];
    let matches_safe = check_osv_advisories(safe_packages, &db);
    assert!(matches_safe.is_empty());

    let vuln_packages = &[("vulnerable-crate", "0.1.1"), ("safe-crate", "1.0.0")];
    let matches_vuln = check_osv_advisories(vuln_packages, &db);
    assert_eq!(matches_vuln.len(), 1);
    assert_eq!(matches_vuln[0].id, "RUSTSEC-2026-0001");
}

#[test]
fn osv_advisory_fixtures_are_valid_and_detect_known_advisories() {
    let cargo_fixture = std::fs::read_to_string("tests/fixtures/projects/advisories-cargo.json")
        .or_else(|_| std::fs::read_to_string("../tests/fixtures/projects/advisories-cargo.json"))
        .expect("cargo advisory fixture must exist");
    let cargo_db: Vec<OsvAdvisory> = serde_json::from_str(&cargo_fixture).unwrap();
    assert!(!cargo_db.is_empty());

    let matches = check_osv_advisories(&[("hyper", "0.14.0")], &cargo_db);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, "RUSTSEC-2024-0002");

    let npm_fixture = std::fs::read_to_string("tests/fixtures/projects/advisories-npm.json")
        .or_else(|_| std::fs::read_to_string("../tests/fixtures/projects/advisories-npm.json"))
        .expect("npm advisory fixture must exist");
    let npm_db: Vec<OsvAdvisory> = serde_json::from_str(&npm_fixture).unwrap();
    assert!(!npm_db.is_empty());

    let matches_npm = check_osv_advisories(&[("lodash", "4.17.15")], &npm_db);
    assert_eq!(matches_npm.len(), 1);
    assert_eq!(matches_npm[0].id, "GHSA-jf85-cpcp-j695");
}
