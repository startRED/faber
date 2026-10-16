use faber_engine::model::Vendor;
use faber_engine::project::Role;
use faber_engine::project::kit::*;
use faber_engine::project::wire::{Facets, Fresh, Labels, Risk, Verb};
use serde_json::json;

#[test]
fn review_card_writer_and_wire_stay_strict() {
    let source = card("craft", "ui:component", "ui.craft", Origin::User).source;
    let mut reply = json!({"verbs":["build"],"serves":["ui:component"],"job":"ui.craft","exclusive":true,
        "stage":"during","conflicts":[],"examples":["Build a screen"],"blindText":"Component craft",
        "purposes":[],"passFocus":[],"priorLiftPp":4,
        "requirements":{"os":[],"apps":[],"gpuGb":0}});
    write_card(&source, &reply.to_string()).unwrap();
    reply["requirements"]["futureWriterField"] = json!(true);
    assert!(write_card(&source, &reply.to_string()).is_err());
    reply["requirements"].as_object_mut().unwrap().remove("futureWriterField");
    reply["futureWriterField"] = json!(true);
    assert!(write_card(&source, &reply.to_string()).is_err());
    let mut raw = serde_json::to_value(labels(&[])).unwrap();
    raw["futureWire"] = json!(true);
    assert!(serde_json::from_value::<Labels>(raw).is_err());
    let mut raw = serde_json::to_value(Facets::default()).unwrap();
    raw["futureWire"] = json!(true);
    assert!(serde_json::from_value::<Facets>(raw).is_err());
}

#[test]
fn durable_labels_keep_the_original_wire_shape_when_extensions_are_empty() {
    let empty = labels(&[]);
    let mut stated = labels(&["ui:component"]);
    stated.facets.purpose = "personal".into();
    for wire in [empty, stated] {
        let durable = KitLabels::from(wire.clone());
        assert_eq!(serde_json::to_value(&durable).unwrap(), serde_json::to_value(&wire).unwrap());
    }
}

fn labels(needs: &[&str]) -> Labels {
    Labels {
        verb: Verb::Build,
        needs: needs.iter().map(|s| s.to_string()).collect(),
        risk: Risk::Low,
        fresh: Fresh::Stable,
        facets: Facets::default(),
    }
}

fn card(id: &str, need: &str, job: &str, origin: Origin) -> Card {
    let source = ReportedTool {
        id: id.into(),
        version: "1".into(),
        origin,
        form: Form::Skill,
        description: "Untrusted description".into(),
        content_hash: "0".repeat(64),
        listing_tokens: 20,
        core_tokens: 100,
        tokens_measured: true,
        inline_available: true,
        runs_code: false,
        sends_data: SendsData::None,
        path: Some(format!("tools/{id}.md")),
        extra: Default::default(),
    };
    write_card(
        &source,
        &json!({"verbs": ["build", "review"], "serves": [need], "job": job, "exclusive": true,
        "stage": "during", "conflicts": [], "examples": ["build this screen"], "blindText": "Applies the selected craft",
        "purposes": ["*"], "passFocus": ["base"], "priorLiftPp": 5.0})
        .to_string(),
    )
    .unwrap()
}

fn request(l: Labels, cards: Vec<Card>) -> Assembly {
    Assembly {
        labels: l,
        fingerprint: vec![],
        inventory: Inventory::new(cards).unwrap(),
        evidence: vec![],
        pins: vec![],
        role: Role::Writer,
        vendor: Vendor::Codex,
        model: "gpt-6.1-sol".into(),
        effort: "medium".into(),
        mandatory_proofs: vec!["lint".into()],
        machine: Machine { os: "windows".into(), apps: vec![], gpu_gb: 0 },
    }
}

#[test]
fn inventory_is_automatically_written_cached_and_untrusted() {
    let c = card("tool", "ui:component", "ui.craft", Origin::User);
    assert_eq!(c.source.content_hash, "0".repeat(64));
    assert!(!serde_json::to_string(&c).unwrap().contains("Untrusted description"));
    let mut cache = CardCache::default();
    assert!(cache.insert(c.clone()).unwrap());
    assert!(!cache.insert(c).unwrap());
    assert_eq!(cache.cards().len(), 1);
}

#[test]
fn cached_metadata_cannot_restore_old_source_permissions_or_paths() {
    let mut source = card("craft", "ui:component", "ui.craft", Origin::User).source;
    let reply = json!({"verbs":["build"],"serves":["ui:component"],"job":"ui.craft","exclusive":true,
        "stage":"during","conflicts":[],"examples":["Build a component"],"blindText":"Component craft",
        "purposes":[],"passFocus":[],"priorLiftPp":4})
    .to_string();
    let mut cache = CardCache::default();
    cache.resolve(&source, |_| Ok(reply)).unwrap();
    source.runs_code = true;
    source.inline_available = false;
    source.path = None;
    let cached = cache.resolve(&source, |_| panic!("same version has no extra writing call")).unwrap();
    assert!(assemble(&request(labels(&["ui:component"]), vec![cached])).unwrap().items.is_empty());
}

#[test]
fn project_jobs_win_and_process_tools_never_load_even_when_pinned() {
    let own = card("project", "style:clean", "ui.style", Origin::Project);
    let third = card("third", "style:clean", "ui.style", Origin::User);
    let process = card("process", "style:clean", "process.plan", Origin::User);
    let mut r = request(labels(&["style:clean!"]), vec![third, process, own]);
    r.pins.push(Pin { id: "process".into(), choice: PinChoice::Always });
    let kit = assemble(&r).unwrap();
    assert_eq!(kit.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["project"]);
    assert!(kit.dropped.iter().any(|d| d.id == "process"));
    assert_eq!(kit.proofs, ["lint"]);
}

#[test]
fn conflicts_caps_stages_and_missing_required_capabilities_are_explicit() {
    let mut a = card("a", "ui:component", "ui.craft", Origin::User);
    a.conflicts.push("b".into());
    let b = card("b", "ui:component", "ui.style", Origin::User);
    let mut check = card("screen", "verify:screen", "verify.screen", Origin::Engine);
    check.source.form = Form::Check;
    check.stage = Stage::After;
    let r = request(labels(&["ui:component!", "verify:screen", "app:missing!"]), vec![a, b, check]);
    let kit = assemble(&r).unwrap();
    assert!(!(kit.items.iter().any(|i| i.id == "a") && kit.items.iter().any(|i| i.id == "b")));
    assert!(kit.proofs.contains(&"screen".to_string()));
    assert!(kit.gaps.contains(&"app:missing!".to_string()));
    assert!(kit.resident_tokens <= 4000);
}

#[test]
fn purpose_changes_needs_without_fixed_kits_and_shadow_never_controls_dispatch() {
    let personal = request(labels(&["lang:rust"]), vec![]);
    let mut commercial = personal.clone();
    commercial.labels.facets.purpose = "sell".into();
    assert!(!assemble(&personal).unwrap().needs.iter().any(|n| n.starts_with("commerce:")));
    assert!(assemble(&commercial).unwrap().needs.iter().any(|n| n.starts_with("commerce:")));
    let picked = assemble(&commercial).unwrap();
    let list = shortlist(&commercial, 1).unwrap();
    let shadow = validate_shadow(&commercial, &picked, &list, "{\"ids\":[\"unknown\"]}", 100).unwrap();
    assert!(!shadow.valid);
    assert!(picked.items.is_empty());
}

#[test]
fn fingerprint_scopes_review_and_covers_software_and_non_js_inputs() {
    let sources = vec![
        SourceFile { path: "Cargo.toml".into(), text: "[package]".into() },
        SourceFile { path: "src-tauri/tauri.conf.json".into(), text: "{}".into() },
        SourceFile { path: "package.json".into(), text: "{\"dependencies\":{\"solid-js\":\"1\",\"typescript\":\"7\"}}".into() },
        SourceFile { path: "mix.exs".into(), text: "".into() },
        SourceFile { path: "audio.wav".into(), text: "".into() },
    ];
    let all = fingerprint(&sources, None).unwrap();
    for n in ["lang:rust", "framework:tauri", "framework:solidjs", "lang:typescript", "lang:elixir", "media:audio"] {
        assert!(all.contains(&n.to_string()), "{n}");
    }
    let diff = fingerprint(&sources, Some(&["Cargo.toml".into()])).unwrap();
    assert_eq!(diff, ["lang:rust"]);
}

#[test]
fn inline_prompt_keeps_role_control_and_rejects_changed_hash_and_credential_paths() {
    let body = "Use the project tokens. Never redefine permissions.";
    let mut c = card("tokens", "style:clean", "ui.style", Origin::Project);
    c.source.content_hash = digest(body.as_bytes());
    c.source.path = Some("ui/tokens.md".into());
    let kit = assemble(&request(labels(&["style:clean"]), vec![c])).unwrap();
    let args = faber_engine::claude::project_claude_args(
        &faber_engine::claude::ClaudeLaunch {
            helper: Default::default(),
            area: faber_engine::model::Area::Project,
            model: Some("claude-sonnet-5-5".into()),
            effort: Some("medium".into()),
            resume: None,
            user_hooks: false,
            instructions: None,
            user_name: None,
            permissions: faber_engine::model::Permissions::Isolated,
            flags: Default::default(),
            tuning: Default::default(),
        },
        kit.role,
    )
    .unwrap();
    let index = args.iter().position(|arg| arg == "--system-prompt").unwrap();
    let safe = args[index + 1].as_str();
    let text = compose_role_prompt(
        safe,
        &kit,
        &[InlineBody { id: "tokens".into(), version: "1".into(), path: "ui/tokens.md".into(), text: body.into() }],
    )
    .unwrap();
    assert!(text.starts_with(safe));
    assert!(text.contains(body));
    for (path, content) in [(".codex/skills/a.md", body), ("ui/tokens.md", "changed")] {
        assert!(
            compose_role_prompt(
                safe,
                &kit,
                &[InlineBody { id: "tokens".into(), version: "1".into(), path: path.into(), text: content.into() }]
            )
            .is_err()
        );
    }
}

#[test]
fn gap_candidates_are_suggestions_and_required_gaps_park_only_their_step() {
    let gap = resolve_gap("app:blender!", Fresh::Recent, Presence::Away,
        "{\"candidates\":[{\"name\":\"Tool\",\"url\":\"https://example.org/tool\",\"license\":\"MIT\",\"release\":\"1\",\"kind\":\"skill\",\"runsCode\":true,\"downside\":\"Needs a separate install\"}]}", None).unwrap();
    assert_eq!(gap.tier, GapTier::G3);
    assert!(gap.blocked);
    assert!(gap.install_parked);
    assert_eq!(gap.candidates.len(), 1);
}

#[test]
fn attestation_replays_init_and_logs_best_effort_without_granting_containment() {
    let kit = assemble(&request(labels(&["lang:rust"]), vec![])).unwrap();
    let ok =
        attest(Vendor::Claude, &kit, &json!({"type":"system", "subtype":"init", "tools":[], "skills":[], "plugins":[], "mcp_servers":[]}))
            .unwrap();
    assert_eq!(ok.status, AttestationStatus::Matched);
    let mismatch = attest(
        Vendor::Claude,
        &kit,
        &json!({"type":"system", "subtype":"init", "tools":["Bash"], "skills":[], "plugins":[], "mcp_servers":[]}),
    )
    .unwrap();
    assert_eq!(mismatch.status, AttestationStatus::Mismatch);
    let best = attest(Vendor::Codex, &kit, &json!({"tools":[], "skills":[], "plugins":[], "mcp_servers":[]})).unwrap();
    assert_eq!(best.status, AttestationStatus::BestEffort);
    assert!(!best.containment_proven);
}

#[test]
fn polish_requires_proofs_two_other_vendor_orders_and_previous_wins_ties() {
    let mut p = PassDecision {
        writer: Vendor::Codex,
        judge: Vendor::Claude,
        proofs_passed: true,
        forward_win: true,
        reverse_win: true,
        completed_polish: 1,
        budget_left: true,
        piece_cost_micros: 100,
        pass_cost_micros: 20,
    };
    assert!(keep_pass(&p));
    p.reverse_win = false;
    assert!(!keep_pass(&p));
    p.reverse_win = true;
    p.judge = Vendor::Codex;
    assert!(!keep_pass(&p));
    p.judge = Vendor::Claude;
    p.completed_polish = 3;
    assert!(!keep_pass(&p));
}

#[test]
fn pins_override_token_caps_only_and_never_override_safety() {
    let mut c = card("large", "ui:component", "ui.craft", Origin::User);
    c.source.core_tokens = 5000;
    let mut r = request(labels(&["ui:component!"]), vec![c.clone()]);
    assert!(assemble(&r).unwrap().items.is_empty());
    r.pins.push(Pin { id: "large".into(), choice: PinChoice::Always });
    assert!(assemble(&r).unwrap().over_budget_pin);
    let mut cards = r.inventory.cards().to_vec();
    cards[0].source.runs_code = true;
    r.inventory = Inventory::new(cards.clone()).unwrap();
    assert!(assemble(&r).unwrap().items.is_empty());
    cards[0].source.runs_code = false;
    r.inventory = Inventory::new(cards).unwrap();
    r.pins[0].choice = PinChoice::Never;
    assert!(assemble(&r).unwrap().items.is_empty());
}

#[test]
fn beta_counts_break_equal_prior_ties_without_changing_priors() {
    let a = card("a", "ui:component", "ui.craft", Origin::User);
    let b = card("b", "ui:component", "ui.craft", Origin::User);
    let mut r = request(labels(&["ui:component"]), vec![a, b]);
    r.evidence.push(Evidence { id: "b".into(), version: "1".into(), kept: 99, discarded: 0 });
    assert_eq!(assemble(&r).unwrap().items[0].id, "b");
    let mut cards = r.inventory.cards().to_vec();
    cards[0].prior_lift_pp = 10.0;
    r.inventory = Inventory::new(cards).unwrap();
    assert_eq!(assemble(&r).unwrap().items[0].id, "a");
}

#[test]
fn environment_drops_arbitrary_token_names_and_keeps_platform_locations() {
    use std::collections::BTreeMap;
    let parent = BTreeMap::from([
        ("PATH".into(), "bin".into()),
        ("HOME".into(), "profile".into()),
        ("TOKEN_THAT_IS_NOT_ON_A_DENYLIST".into(), "fixture-secret".into()),
        ("GITHUB_TOKEN".into(), "fixture-secret".into()),
    ]);
    let clean = worker_environment(&parent);
    assert_eq!(clean.len(), 2);
    assert_eq!(clean["PATH"], "bin");
    assert!(!clean.values().any(|v| v == "fixture-secret"));
}

#[test]
fn scoped_project_sections_and_named_proofs_generate_inventory_without_fixed_loadouts() {
    let input = vec![SourceFile { path: "AGENTS.md".into(), text: "# Rules\nKeep source scoped.\n## Proofs\nRun named checks.\n".into() }];
    let tools = project_inventory(&input, &["lint".into(), "fmt".into()]).unwrap();
    assert_eq!(tools.len(), 4);
    assert_eq!(tools.iter().filter(|t| t.form == Form::Check).count(), 2);
    assert_ne!(tools[0].content_hash, tools[1].content_hash);
    assert!(project_inventory(&[SourceFile { path: ".claude/config.json".into(), text: "do not read".into() }], &[]).is_err());
}

#[test]
fn field_briefs_are_cited_bounded_cached_by_hash_and_invalidated_by_version_or_age() {
    let text = "Use the documented API for this fixture version.";
    let b = FieldBrief {
        schema: 1,
        need: "platform:tmodloader".into(),
        version: "1".into(),
        text: text.into(),
        tokens: 12,
        sources: vec![PrimarySource {
            url: "https://example.org/docs".into(),
            version: "1".into(),
            fetched_at: 10,
            sha256: "0".repeat(64),
            extra: Default::default(),
        }],
        sha256: digest(text.as_bytes()),
        fetched_at: 10,
        extra: Default::default(),
    };
    b.validate().unwrap();
    assert!(!b.stale(11, "1"));
    assert!(b.stale(11, "2"));
    assert!(b.stale(10 + 30 * 86400, "1"));
    let gap = resolve_gap("platform:tmodloader!", Fresh::Recent, Presence::Waiting, "{\"candidates\":[]}", Some(b.clone())).unwrap();
    assert_eq!(gap.tier, GapTier::G2);
    assert!(!gap.blocked);
    assert!(gap.install_parked);
    let mut too_big = b;
    too_big.tokens = 3001;
    assert!(too_big.validate().is_err());
    assert!(resolve_gap("media:audio", Fresh::Stable, Presence::Shaping, "{\"candidates\":[]}", None).unwrap().choices.len() == 2);
    assert!(resolve_gap("app:missing",Fresh::Stable,Presence::Away,&json!({"candidates":vec![json!({"name":"x","url":"file:///secret","license":"MIT","release":"1","kind":"skill","runsCode":true,"downside":"x"})]}).to_string(),None).is_err());
}

#[test]
fn shadow_shortlists_are_blind_bounded_shuffled_and_never_replace_the_code_pick() {
    let r = request(
        labels(&["ui:component!"]),
        vec![
            card("thirdparty-a", "ui:component", "ui.craft", Origin::User),
            card("thirdparty-b", "ui:component", "ui.style", Origin::User),
        ],
    );
    let code = assemble(&r).unwrap();
    let list = shortlist(&r, 1).unwrap();
    let text = serde_json::to_string(&list.cards).unwrap();
    assert!(!text.contains("thirdparty"));
    assert!(list.cards.len() <= 24);
    let reply = json!({"ids":list.cards.iter().map(|c| &c.id).collect::<Vec<_>>()}).to_string();
    assert!(validate_shadow(&r, &code, &list, &reply, 100).unwrap().valid);
    assert!(!validate_shadow(&r, &code, &list, &reply, 3001).unwrap().valid);
    assert!(!validate_shadow(&r, &code, &list, "{\"ids\":[\"unknown\"]}", 1).unwrap().valid);
    assert_eq!(code.items.len(), 2);
}

#[test]
fn each_polish_pass_gets_a_fresh_kit_and_prototype_skips_polish() {
    let mut ux = card("ux", "ui:component", "ui.craft", Origin::User);
    ux.pass_focus = vec!["ux".into()];
    let mut ui = card("ui", "ui:component", "ui.craft", Origin::User);
    ui.pass_focus = vec!["ui".into()];
    let mut r = request(labels(&["ui:component"]), vec![card("base", "ui:component", "ui.craft", Origin::User), ux, ui]);
    let passes = pass_kits(&r).unwrap();
    assert_eq!(passes.iter().map(|p| p.focus.as_str()).collect::<Vec<_>>(), ["base", "ux", "ui"]);
    assert!(passes.iter().all(|p| p.fresh_session));
    assert_eq!(passes[1].kit.items[0].id, "ux");
    assert_eq!(passes[2].kit.items[0].id, "ui");
    r.labels.facets.purpose = "prototype".into();
    assert_eq!(pass_kits(&r).unwrap().len(), 1);
}

#[test]
fn reported_inventory_never_claims_an_unread_body_is_inline_available() {
    let tools=reported_inventory(&json!({"skills":[{"name":"Installed craft","description":"untrusted author text","version":"1"}],"mcp_servers":[{"name":"server","status":"dead"}]})).unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().all(|t| !t.inline_available && t.path.is_none()));
    assert_eq!(tools[1].form, Form::Mcp);
}

#[test]
fn machine_requirements_and_unavailable_native_tools_stay_gaps() {
    let mut gpu = card("gpu", "media:image", "media.craft", Origin::User);
    gpu.requirements.gpu_gb = 8;
    let mut native = card("native", "media:image", "media.native", Origin::User);
    native.source.form = Form::Mcp;
    let r = request(labels(&["media:image!"]), vec![gpu, native]);
    let kit = assemble(&r).unwrap();
    assert!(kit.items.is_empty());
    assert_eq!(kit.gaps, ["media:image!"]);
}

#[test]
fn kit_selection_precedes_dispatch_and_completion_cannot_rewrite_frozen_content() {
    use faber_engine::project::store::{Journal, JournalError};
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let kit = assemble(&request(labels(&["lang:rust"]), vec![])).unwrap();
    let mut receipt = KitReceipt::new("m1", "a", 1, kit, 1).unwrap();
    journal.record_kit(&receipt).unwrap();
    assert_eq!(journal.record_kit(&receipt), Err(JournalError::ReceiptExists));
    let saved = journal.kit("a", 1).unwrap().unwrap();
    assert_eq!(saved.pin(), receipt.pin());
    receipt.attestation =
        Some(attest(Vendor::Codex, &receipt.kit, &json!({"tools":[],"skills":[],"plugins":[],"mcp_servers":[]})).unwrap());
    receipt.outcome = Some(KitOutcome {
        proof_first_try: Some(true),
        repairs: Some(0),
        review_blocking: Some(0),
        kit_used: vec![],
        user: "unknown".into(),
        tokens: None,
        cost_micros: None,
        extra: Default::default(),
    });
    receipt.completed_at = Some(2);
    assert_eq!(journal.complete_kit(&receipt), Err(JournalError::Mismatch));
    journal.attest_kit("a", 1, receipt.attestation.as_ref().unwrap()).unwrap();
    let mut changed = receipt.clone();
    changed.kit.model = "different".into();
    assert_eq!(journal.complete_kit(&changed), Err(JournalError::Mismatch));
    journal.complete_kit(&receipt).unwrap();
    assert_eq!(journal.complete_kit(&receipt), Err(JournalError::ReceiptExists));
    let reopened = Journal::open(repo.path(), data.path(), "m1").unwrap();
    assert_eq!(reopened.kit("a", 1).unwrap().unwrap(), receipt);
    assert!(repo.path().join(".faber/missions/m1/kits/a-1.json").is_file());
}

#[test]
fn measurement_targets_require_complete_inputs_and_never_claim_fixture_evidence_is_live_g9() {
    let pairs = vec![
        LabelPair {
            old_micros: 100,
            new_micros: 200,
            old_output_tokens: 30,
            new_output_tokens: 60,
            old_cost_micros: 1,
            new_cost_micros: 2
        };
        22
    ];
    let times = vec![1000; 100];
    let input = MeasureInput {
        source: MeasurementSource::Fixture,
        cards: 2000,
        assembly_micros: &times,
        labels: &pairs,
        plan_extra_tokens: Some(80),
        plan_nodes: 2,
        receipts: &[],
        mission_cost_micros: None,
        observed_resident_tokens: &[],
        gaps: &[],
        covered_start_delays_micros: &[],
        card_writes: &[],
        passes: &[],
    };
    let m = measure(&input);
    assert_eq!(m.assembly_target, Some(true));
    assert_eq!(m.label_target, Some(true));
    assert_eq!(m.plan_target, Some(true));
    assert!(!m.real_goal_proven);
    assert!(!m.attestation_complete);
    assert_eq!(m.selection_target, None);
    assert_eq!(m.resident_target, None);
}

#[test]
fn two_thousand_card_assembly_has_measured_percentiles() {
    let cards: Vec<_> = (0..2000).map(|i| card(&format!("tool{i}"), "ui:component", &format!("job{i}"), Origin::User)).collect();
    let r = request(labels(&["ui:component"]), cards);
    let mut times = Vec::new();
    for _ in 0..100 {
        times.push(assemble(&r).unwrap().assemble_micros);
    }
    let m = measure(&MeasureInput {
        source: MeasurementSource::Synthetic,
        cards: 2000,
        assembly_micros: &times,
        labels: &[],
        plan_extra_tokens: None,
        plan_nodes: 0,
        receipts: &[],
        mission_cost_micros: None,
        observed_resident_tokens: &[],
        gaps: &[],
        covered_start_delays_micros: &[],
        card_writes: &[],
        passes: &[],
    });
    println!("{}", serde_json::to_string(&m).unwrap());
    assert_eq!(m.assembly_target, Some(true));
}

#[test]
fn card_cache_replays_one_writer_per_content_version_and_survives_reload() {
    use faber_engine::project::store::Journal;
    let source = card("craft", "ui:component", "ui.craft", Origin::User).source;
    let reply=json!({"verbs":["build"],"serves":["ui:component"],"job":"ui.craft","exclusive":true,"stage":"during","conflicts":[],"examples":["build a component"],"blindText":"Applies component craft","purposes":["*"],"passFocus":["base"],"priorLiftPp":99.0}).to_string();
    let mut cache = CardCache::default();
    let mut calls = 0;
    let first = cache
        .resolve(&source, |_| {
            calls += 1;
            Ok(reply.clone())
        })
        .unwrap();
    assert_eq!(first.prior_lift_pp, 4.0);
    cache
        .resolve(&source, |_| {
            calls += 1;
            Ok(reply.clone())
        })
        .unwrap();
    assert_eq!(calls, 1);
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    journal.save_card_cache(&cache).unwrap();
    let mut restored = journal.card_cache().unwrap();
    restored.resolve(&source, |_| panic!("cached card calls no model")).unwrap();
    let mut next = source;
    next.version = "2".into();
    restored
        .resolve(&next, |_| {
            calls += 1;
            Ok(reply.clone())
        })
        .unwrap();
    assert_eq!(calls, 2);
}

#[test]
fn g9_gaps_cards_passes_and_unknown_cost_are_measured_without_inventing_evidence() {
    let gaps = [
        GapSample { tier: GapTier::G0, elapsed_micros: 0, cost_micros: Some(0) },
        GapSample { tier: GapTier::G1, elapsed_micros: 120000001, cost_micros: Some(100) },
        GapSample { tier: GapTier::G2, elapsed_micros: 5, cost_micros: None },
    ];
    let cards = [CardSample { calls: 2, cost_micros: Some(1) }];
    let passes = [PassSample { piece_cost_micros: 100, pass_cost_micros: 31, kept: true, won_both_orders: true }];
    let m = measure(&MeasureInput {
        source: MeasurementSource::Fixture,
        cards: 0,
        assembly_micros: &[],
        labels: &[],
        plan_extra_tokens: None,
        plan_nodes: 0,
        receipts: &[],
        mission_cost_micros: None,
        observed_resident_tokens: &[(100, 50)],
        gaps: &gaps,
        covered_start_delays_micros: &[0],
        card_writes: &cards,
        passes: &passes,
    });
    assert_eq!(m.gap_tiers[0].target, Some(true));
    assert_eq!(m.gap_tiers[1].target, Some(false));
    assert_eq!(m.gap_tiers[2].target, None);
    assert_eq!(m.card_writing_target, Some(false));
    assert_eq!(m.pass_cost_target, Some(false));
    assert_eq!(m.resident_target, Some(false));
    assert_eq!(m.covered_start_target, Some(true));
    assert!(!m.real_goal_proven);
}

#[test]
fn attestation_and_gap_replies_are_fixture_replays() {
    for line in include_str!("../../tests/fixtures/projects/kit-init.jsonl").lines() {
        let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
        let vendor = if fixture["vendor"] == "claude" { Vendor::Claude } else { Vendor::Codex };
        let mut r = request(labels(&[]), vec![]);
        r.vendor = vendor;
        if vendor == Vendor::Claude {
            r.model = "claude-sonnet-5-5".into();
        }
        let kit = assemble(&r).unwrap();
        let result = attest(vendor, &kit, &fixture["init"]).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap()["status"], fixture["expected"]);
    }
    for line in include_str!("../../tests/fixtures/projects/kit-gaps.jsonl").lines() {
        let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
        let gap =
            resolve_gap(fixture["need"].as_str().unwrap(), Fresh::Unknown, Presence::Away, &fixture["reply"].to_string(), None).unwrap();
        assert_eq!(gap.blocked, fixture["blocked"].as_bool().unwrap());
        assert!(gap.install_parked);
    }
}

#[test]
fn empty_catalog_and_knowledge_floor_do_not_block_independent_work() {
    assert!(G0_CANDIDATES.is_empty());
    assert!(!resolve_gap("framework:unfamiliar!", Fresh::Unknown, Presence::Away, "{\"candidates\":[]}", None).unwrap().blocked);
    assert!(resolve_gap("app:missing!", Fresh::Unknown, Presence::Waiting, "{\"candidates\":[]}", None).unwrap().blocked);
}

#[test]
fn shared_auto_labels_do_not_change_model_choice_and_inherit_only_unstated_facets() {
    use faber_engine::route::{PROFILES, asked, read};
    let plain = "{\"kind\":\"code\",\"difficulty\":\"medium\",\"confidence\":1.0}";
    let mut extended: serde_json::Value = serde_json::from_str(plain).unwrap();
    extended["labels"] = json!({"verb":"build","needs":["lang:TS","lang:typescript"],"risk":"low","fresh":"stable","facets":{"purpose":"selling","region":"BR"}});
    let answer = extended.to_string();
    assert_eq!(asked(plain), asked(&answer));
    assert_eq!(read(plain, &PROFILES), read(&answer, &PROFILES));
    let mut l = request_labels(&answer).unwrap();
    assert_eq!(l.needs, ["lang:typescript"]);
    assert_eq!(l.facets.purpose, "sell");
    inherit_facets(&mut l, &Facets { purpose: "personal".into(), audience: "developers".into(), region: "US".into(), ..Facets::default() });
    assert_eq!(l.facets.region, "BR");
    assert_eq!(l.facets.audience, "developers");
    assert_eq!(l.facets.purpose, "sell");
    assert!(request_instructions().starts_with(faber_engine::route::INSTRUCTIONS));
    extended["labels"]["needs"] = json!(["grant:money"]);
    assert!(request_labels(&extended.to_string()).is_err());
}

#[test]
fn approval_pins_ignore_measurement_timing_and_inventory_order_but_bind_selected_versions() {
    let kit = assemble(&request(labels(&["ui:component"]), vec![card("craft", "ui:component", "ui.craft", Origin::User)])).unwrap();
    let receipt = KitReceipt::new("m1", "a", 1, kit, 1).unwrap();
    let mut timed = receipt.clone();
    timed.kit.assemble_micros += 1;
    assert_eq!(receipt.pin(), timed.pin());
    timed.kit.items[0].version = "2".into();
    assert_ne!(receipt.pin(), timed.pin());
    let long = KitReceipt::new("m1", &"a".repeat(64), u32::MAX, receipt.kit, 1).unwrap();
    long.pin().validate().unwrap();
}

#[test]
fn primary_source_fetch_is_scoped_and_brief_cache_preserves_versions_and_freshness() {
    use faber_engine::project::store::Journal;
    let fetch = |url: &str| {
        assert_eq!(url, "https://example.org/docs");
        Ok(b"Documented fixture API, version 1".to_vec())
    };
    let pages = fetch_primary_pages(&[("https://example.org/docs".into(), "1".into())], 10, &fetch).unwrap();
    assert!(brief_question("framework:unfamiliar", &pages).unwrap().contains("no repository access"));
    let brief = written_brief("framework:unfamiliar", "1", "Use the API: https://example.org/docs", 12, &pages, 10).unwrap();
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    journal.save_field_brief(&brief).unwrap();
    assert_eq!(journal.field_brief("framework:unfamiliar!", 11, "1").unwrap(), Some(brief));
    assert!(journal.field_brief("framework:unfamiliar", 11, "2").unwrap().is_none());
    assert!(journal.field_brief("framework:unfamiliar", 10 + 30 * 86400, "1").unwrap().is_none());
    assert!(fetch_primary_pages(&[("file:///secret".into(), "1".into())], 1, &|_| panic!("invalid URL is never fetched")).is_err());
}

#[test]
fn failed_selection_write_is_not_dispatchable_and_corrupt_receipts_never_become_empty() {
    use faber_engine::project::store::{Journal, JournalError};
    use faber_engine::store::{Disk, Fault};
    use std::{io, path::Path, sync::Arc};
    struct Refuse;
    impl Disk for Refuse {
        fn write(&self, _: &Path, _: &[u8]) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::StorageFull))
        }
        fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
            panic!("a failed write never renames")
        }
    }
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let receipt = KitReceipt::new("m1", "a", 1, assemble(&request(labels(&[]), vec![])).unwrap(), 1).unwrap();
    let good = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let full = Journal::open_with(repo.path(), data.path(), "m1", Arc::new(Refuse)).unwrap();
    assert_eq!(full.record_kit(&receipt), Err(JournalError::Store(Fault::DiskFull)));
    assert!(full.kit("a", 1).unwrap().is_none());
    good.record_kit(&receipt).unwrap();
    std::fs::write(repo.path().join(".faber/missions/m1/kits/a-1.json"), "{broken").unwrap();
    assert!(good.kit("a", 1).is_err());
    assert!(good.record_kit(&receipt).is_err());
}

#[test]
fn beta_outcomes_are_versioned_immutable_and_derived_once_per_completed_receipt() {
    use faber_engine::project::store::Journal;
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let kit = assemble(&request(labels(&["ui:component"]), vec![card("craft", "ui:component", "ui.craft", Origin::User)])).unwrap();
    let mut receipt = KitReceipt::new("m1", "a", 1, kit, 1).unwrap();
    journal.record_kit(&receipt).unwrap();
    receipt.attestation = Some(attest(Vendor::Codex, &receipt.kit, &json!({})).unwrap());
    receipt.outcome = Some(KitOutcome {
        proof_first_try: Some(true),
        repairs: Some(0),
        review_blocking: Some(0),
        kit_used: vec!["craft".into()],
        user: "kept".into(),
        tokens: Some(50),
        cost_micros: Some(1),
        extra: Default::default(),
    });
    receipt.completed_at = Some(2);
    journal.attest_kit("a", 1, receipt.attestation.as_ref().unwrap()).unwrap();
    journal.complete_kit(&receipt).unwrap();
    let evidence = journal.kit_evidence().unwrap();
    assert_eq!(evidence[0].version, "1");
    assert_eq!(evidence[0].kept, 1);
    assert_eq!(journal.kit_evidence().unwrap(), evidence);
}

#[test]
fn pins_override_project_precedence_but_conflicting_pins_and_process_aliases_fail_closed() {
    let own = card("own", "style:clean", "ui.style", Origin::Project);
    let user = card("user", "style:clean", "ui.style", Origin::User);
    let mut r = request(labels(&["style:clean"]), vec![own, user]);
    r.pins.push(Pin { id: "user".into(), choice: PinChoice::Always });
    assert_eq!(assemble(&r).unwrap().items[0].id, "user");
    r.pins.push(Pin { id: "user".into(), choice: PinChoice::Never });
    assert!(assemble(&r).is_err());
    let upper = card("process", "style:clean", "PROCESS.PLAN", Origin::User);
    assert_eq!(upper.job, "process.plan");
    let r = request(labels(&["style:clean"]), vec![upper]);
    assert!(assemble(&r).unwrap().items.is_empty());
}

#[test]
fn persisted_kits_reject_model_aliases_and_keep_unknown_receipt_fields() {
    use faber_engine::project::store::Journal;
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut kit = assemble(&request(labels(&[]), vec![])).unwrap();
    kit.model = "default".into();
    assert!(kit.validate().is_err());
    let kit = assemble(&request(labels(&[]), vec![])).unwrap();
    let mut receipt = KitReceipt::new("m1", "a", 1, kit, 1).unwrap();
    receipt.extra.insert("futureReceipt".into(), json!({"pinned":true}));
    journal.record_kit(&receipt).unwrap();
    receipt.attestation = Some(attest(Vendor::Codex, &receipt.kit, &json!({})).unwrap());
    journal.attest_kit("a", 1, receipt.attestation.as_ref().unwrap()).unwrap();
    receipt.outcome = Some(KitOutcome {
        proof_first_try: None,
        repairs: None,
        review_blocking: None,
        kit_used: vec![],
        user: "unknown".into(),
        tokens: None,
        cost_micros: None,
        extra: Default::default(),
    });
    receipt.completed_at = Some(2);
    receipt.extra.clear();
    journal.complete_kit(&receipt).unwrap();
    assert_eq!(journal.kit("a", 1).unwrap().unwrap().extra["futureReceipt"], json!({"pinned":true}));
}
#[test]
fn review_field_brief_additions_survive_rollback_and_newer_is_not_corrupt() {
    use faber_engine::project::store::{Journal, JournalError};
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let pages = fetch_primary_pages(&[("https://example.org/docs".into(), "1".into())], 10, &|_| Ok(b"fixture".to_vec())).unwrap();
    let brief = written_brief("framework:fixture", "1", "See https://example.org/docs", 10, &pages, 10).unwrap();
    journal.save_field_brief(&brief).unwrap();
    let path = repo.path().join(".faber/briefs").join(format!("{}.json", digest(b"framework:fixture")));
    let mut raw = serde_json::to_value(&brief).unwrap();
    raw["futureBrief"] = json!({"note":"additive"});
    raw["sources"][0]["futureSource"] = json!(true);
    std::fs::write(&path, raw.to_string()).unwrap();
    let read = journal.field_brief("framework:fixture", 11, "1").unwrap().unwrap();
    journal.save_field_brief(&read).unwrap();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(), raw);
    raw["schema"] = json!(2);
    std::fs::write(&path, raw.to_string()).unwrap();
    assert_eq!(journal.field_brief("framework:fixture", 11, "1"), Err(JournalError::Newer));
}

#[test]
fn review_nested_card_additions_roundtrip() {
    use faber_engine::project::store::Journal;
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut cache = CardCache::default();
    cache.insert(card("craft", "ui:component", "ui.craft", Origin::User)).unwrap();
    journal.save_card_cache(&cache).unwrap();
    let mut raw = serde_json::to_value(&cache).unwrap();
    let c = raw["cards"].as_object_mut().unwrap().values_mut().next().unwrap();
    for pointer in ["", "/source", "/requirements"] {
        c.pointer_mut(pointer).unwrap()["futureCard"] = json!({"value":7});
    }
    std::fs::write(journal.records().join("cards.json"), raw.to_string()).unwrap();
    let read = journal.card_cache().unwrap();
    journal.save_card_cache(&read).unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap(), raw);
}

#[test]
fn review_nested_kit_additions_roundtrip_without_relaxing_wire() {
    use faber_engine::project::store::Journal;
    let repo = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut l = labels(&["ui:component"]);
    l.facets.purpose = "personal".into();
    let kit = assemble(&request(
        l,
        vec![card("craft", "ui:component", "ui.craft", Origin::User), card("process", "ui:component", "process.plan", Origin::User)],
    ))
    .unwrap();
    let receipt = KitReceipt::new("m1", "a", 1, kit, 1).unwrap();
    journal.record_kit(&receipt).unwrap();
    let mut raw = serde_json::to_value(&receipt).unwrap();
    raw["attestation"] = serde_json::to_value(attest(Vendor::Codex, &receipt.kit, &json!({})).unwrap()).unwrap();
    raw["outcome"] =
        json!({"proofFirstTry":true,"repairs":0,"reviewBlocking":0,"kitUsed":["craft"],"user":"kept","tokens":10,"costMicros":1});
    raw["shadow"] = json!({"ids":["craft"],"valid":true,"disagrees":false,"elapsedMs":1});
    raw["completedAt"] = json!(2);
    for pointer in ["/kit", "/kit/items/0", "/kit/dropped/0", "/kit/labels", "/kit/labels/facets", "/attestation", "/outcome", "/shadow"] {
        raw.pointer_mut(pointer).unwrap()["futureMetadata"] = json!({"value":7});
        std::fs::write(journal.records().join("kits/a-1.json"), raw.to_string()).unwrap();
        let read = journal.kit("a", 1).unwrap().unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), raw, "{pointer}");
    }
}
