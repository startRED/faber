//! The lessons of building Faber by hand, as engine rules (docs/superpowers/specs/2026-10-16-own-lessons-into-faber.md):
//! Faber decides "done", a change is sized by risk, reviews end, and a red base stops the line.
use faber_engine::model::Vendor;
use faber_engine::project::coordinator::{pick_reviewer, repair_fresh};
use faber_engine::project::proof::{Definition, Observation, ProofSet};
use faber_engine::project::queue::{Candidate, Queue, Review};
use faber_engine::project::wire::{Action, ArtifactRef, Envelope, Expect, FindingClass};
use faber_engine::project::worker::ModelPin;
use faber_engine::project::{Role, lease::hash};
use std::collections::BTreeMap;
use std::path::Path;

const BASE: &str = "1111111111111111111111111111111111111111";
const TREE: &str = "2222222222222222222222222222222222222222";

fn definition() -> Definition {
    Definition {
        id: "rust".into(),
        version: 1,
        program: "/native/cargo".into(),
        argv: vec!["t".into()],
        checks: vec!["C1".into()],
        toolchain: "rust-1".into(),
        dependencies: "deps-1".into(),
        environment: BTreeMap::new(),
        heavy: true,
    }
}

fn candidate(id: &str) -> Candidate {
    Candidate::new(id, BASE.into(), TREE.into(), 3, Vendor::Codex, "input".into(), vec!["engine/src/a.rs".into()], vec![definition()])
        .unwrap()
}

/// The engine's proof of `tree`, passing or not, as the queue receives it.
fn proof(tree: &str, exit: i32) -> ProofSet {
    let mut p = ProofSet::new(tree.into(), "input".into(), vec![definition()], vec!["engine/src/a.rs".into()]).unwrap();
    let out = format!("exit {exit}").into_bytes();
    let evidence = ArtifactRef { path: "proofs/rust.log".into(), sha256: hash(&out) };
    p.record("r1", "rust", Observation { exit: Some(exit), stdout: out, stderr: vec![], evidence }).unwrap();
    p
}

fn review(session: &str) -> Review {
    Review {
        vendor: Vendor::Claude,
        tree: TREE.into(),
        proof_digest: proof(TREE, 0).digest(),
        session: session.into(),
        fresh: true,
        accepted: true,
        findings: vec![],
    }
}

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

/// Every Rust file under `dir`, as (path, text).
fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push((path.to_string_lossy().replace('\\', "/"), std::fs::read_to_string(&path).unwrap()));
        }
    }
}

#[test]
fn faber_decides_done_no_landing_without_the_engines_proof_of_the_composed_tree() {
    // A review alone, a writer's word, a failed proof or a proof of another tree never open the gate.
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    assert_eq!(q.review("w1", review("s1")), Err("reason.queue.proof"), "a review before the engine's proof");
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.proof"));
    assert_eq!(q.prove("w1", proof(TREE, 1)), Err("reason.queue.proof"), "a red proof");
    assert_eq!(q.prove("w1", proof("3333333333333333333333333333333333333333", 0)), Err("reason.queue.proof"), "another tree");
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.proof"));
    q.prove("w1", proof(TREE, 0)).unwrap();
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.review"), "proven but not reviewed");
    q.review("w1", review("s1")).unwrap();
    assert!(q.accepted("w1", BASE, 3).is_ok());
    // A repair changes the tree: the old proof and review stop counting at once.
    q.repaired("w1", "4444444444444444444444444444444444444444".into(), "input2".into(), vec!["engine/src/a.rs".into()]).unwrap();
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.proof"));
}

#[test]
fn the_only_landing_path_checks_the_queue_before_any_commit_or_move() {
    let mut files = vec![];
    sources(&repo("engine/src"), &mut files);
    sources(&repo("src-tauri/src"), &mut files);
    let callers: Vec<&str> = files
        .iter()
        .filter(|(path, _)| !path.ends_with("project/git.rs"))
        .filter(|(_, text)| ["commit_proven(", ".fast_forward(", "broker.commit("].iter().any(|call| text.contains(call)))
        .map(|(path, _)| path.as_str())
        .collect();
    assert!(callers.len() == 1 && callers[0].ends_with("engine/src/project/run.rs"), "landing effects outside run.rs: {callers:?}");
    let run = &files.iter().find(|(path, _)| path.ends_with("engine/src/project/run.rs")).unwrap().1;
    let land = run.split("pub fn land<").nth(1).expect("Runner::land");
    let body = &land[..land.find("\n    pub fn ").or_else(|| land.find("\n    fn ")).unwrap_or(land.len())];
    let gate = body.find("self.queue.accepted(").expect("land asks the queue");
    for effect in ["commit_proven(", ".fast_forward("] {
        assert!(body.find(effect).is_some_and(|at| at > gate), "{effect} comes before the queue's gate");
    }
    assert_eq!(run.matches("commit_proven(").count(), 1, "one commit call, inside land");
}

#[test]
fn a_full_change_lands_only_with_two_accepted_reviews_from_different_sessions() {
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    q.prove("w1", proof(TREE, 0)).unwrap();
    q.require_reviews("w1", 2).unwrap();
    q.require_reviews("w1", 1).unwrap();
    assert_eq!(q.get("w1").unwrap().reviews_needed(), 2, "a size is never lowered");
    assert!(q.require_reviews("w1", 3).is_err());
    q.review("w1", review("s1")).unwrap();
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.review"), "one of two");
    assert_eq!(q.review("w1", review("s1")), Err("reason.queue.review"), "the same session twice");
    q.review("w1", review("s2")).unwrap();
    assert_eq!(q.get("w1").unwrap().accepted_reviews().len(), 2);
    assert!(q.accepted("w1", BASE, 3).is_ok());
    // A third round that blocks stops it again, whatever came before.
    let blocked = Review { accepted: false, findings: vec!["security engine/src/a.rs:3: leaks".into()], ..review("s3") };
    assert!(q.review("w1", blocked).is_err());
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.review"));
    // A new proof of the same tree starts the reviews over.
    q.prove("w1", proof(TREE, 0)).unwrap();
    assert!(q.get("w1").unwrap().accepted_reviews().is_empty());
    // The size survives a restart, and a record written before sizes existed reads as one review.
    let restored: Queue = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
    assert_eq!(restored.get("w1").unwrap().reviews_needed(), 2);
    let mut old = serde_json::to_value(&q).unwrap();
    old["entries"][0].as_object_mut().unwrap().remove("needed");
    let old: Queue = serde_json::from_value(old).unwrap();
    assert_eq!(old.get("w1").unwrap().reviews_needed(), 1);
}

#[test]
fn a_red_base_stops_the_line_for_everyone_but_its_one_repair_owner() {
    let ready = |q: &mut Queue, id: &str, session: &str| {
        q.enqueue(candidate(id)).unwrap();
        q.prove(id, proof(TREE, 0)).unwrap();
        q.review(id, review(session)).unwrap();
    };
    let mut q = Queue::default();
    ready(&mut q, "w1", "s1");
    q.stop_line(BASE, "head-proof-1").unwrap();
    assert_eq!(q.accepted("w1", BASE, 3).err(), Some("reason.queue.baseRed"), "a ready candidate waits");
    q.own_repair("w1").unwrap();
    q.own_repair("w1").unwrap();
    assert_eq!(q.own_repair("w2"), Err("reason.queue.baseRed"), "one repair owner");
    assert!(q.accepted("w1", BASE, 3).is_ok(), "the owner lands first");
    // Stopping the same head again keeps its owner; the head's own proof turning green moves the line.
    q.stop_line(BASE, "head-proof-2").unwrap();
    assert_eq!(q.line().unwrap().owner.as_deref(), Some("w1"));
    q.clear_line("3333333333333333333333333333333333333333");
    assert!(q.line().is_some(), "another head clears nothing");
    q.clear_line(BASE);
    assert!(q.line().is_none());
    let restored: Queue = {
        q.stop_line(BASE, "head-proof-3").unwrap();
        serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap()
    };
    assert_eq!(restored.line().unwrap().receipt, "head-proof-3", "the stopped line survives a restart");
}

fn complete(findings: serde_json::Value) -> Result<Envelope, &'static str> {
    let text = serde_json::json!({"schema": 2, "task": "reviewer", "attempt": 1, "base": BASE, "contract": "c", "lease": 0,
        "ping": "DoneReady", "action": {"complete": {"summary": "s", "verdict": "pass", "next": "land", "findings": findings}}})
    .to_string();
    let expect = Expect { task: "reviewer".into(), attempt: 1, base: BASE.into(), contract: "c".into(), lease: 0 };
    Envelope::parse(&text, &expect, Role::Reviewer).map_err(|e| e.key())
}

#[test]
fn findings_carry_a_class_and_only_notes_let_the_candidate_go_on() {
    let reply = complete(serde_json::json!([
        "an old plain finding",
        {"class": "security", "at": "engine/src/a.rs:3", "text": "the path leaves the worktree"},
        {"class": "note", "text": "a name could be clearer"},
        {"text": "no class given"}
    ]))
    .unwrap();
    let Action::Complete(done) = reply.action else { panic!("not a hand-back") };
    let blocks: Vec<bool> = done.findings.iter().map(|f| f.blocks()).collect();
    assert_eq!(blocks, [true, true, false, true], "a finding nobody classified blocks");
    assert_eq!(done.findings[1].class, Some(FindingClass::Security));
    assert_eq!(done.findings[1].line(), "security engine/src/a.rs:3: the path leaves the worktree");
    assert_eq!(done.findings[0].line(), "unclassified: an old plain finding");
    for class in ["person", "data", "security", "rule"] {
        let one = complete(serde_json::json!([{"class": class, "text": "x"}])).unwrap();
        let Action::Complete(done) = one.action else { panic!() };
        assert!(done.findings[0].blocks(), "{class} blocks");
    }
    assert_eq!(complete(serde_json::json!([{"class": "style", "text": "x"}])).err(), Some("error.wire.malformed"));
    assert_eq!(complete(serde_json::json!([{"text": "x", "severity": "high"}])).err(), Some("error.wire.malformed"));
    assert_eq!(complete(serde_json::json!([{"at": "a\nb", "text": "x"}])).err(), Some("error.wire.malformed"));
}

#[test]
fn fabers_own_rules_size_its_changes_and_number_its_decisions_and_notes() {
    use faber_engine::project::rules::{FILE, Rules, Tier};
    let text = std::fs::read_to_string(repo(FILE)).unwrap();
    let rules = Rules::from_sources(&[(FILE.into(), text)]).expect("faber-rules.json is valid");
    let tier = |paths: &[&str]| rules.size(&paths.iter().map(|p| p.to_string()).collect::<Vec<_>>()).tier();
    assert_eq!(tier(&["ui/Chat.tsx", "ui/styles.css"]), Tier::Light);
    assert_eq!(tier(&["engine/src/project/lead.rs"]), Tier::Light);
    for full in [
        &["engine/src/project/coordinator.rs"][..],
        &["engine/src/proc.rs"],
        &["winproc/src/lib.rs"],
        &["src-tauri/src/terminal.rs"],
        &["engine/src/permissions.rs"],
        &["src-tauri/src/update.rs"],
        &["ui/Chat.tsx", "engine/src/engine.rs"],
        &["somewhere-new.txt"],
    ] {
        assert_eq!(tier(full), Tier::Full, "{full:?}");
    }
    let names: Vec<&str> = rules.sequences.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decision", "notes"]);
    let decisions = std::fs::read_to_string(repo("docs/decisions.md")).unwrap();
    assert!(faber_engine::project::sequence::highest(&rules.sequences[0], &decisions) >= 160, "the decision numbers are read");
    let notes = std::fs::read_to_string(repo("ui/locales/en.ts")).unwrap();
    assert!(faber_engine::project::sequence::highest(&rules.sequences[1], &notes) >= 15, "the notes numbers are read");
}

fn pin(vendor: Vendor, model: &str) -> ModelPin {
    ModelPin { vendor, model: model.into(), effort: "medium".into(), cli_version: "1.0.0".into() }
}

#[test]
fn later_rounds_go_to_the_eligible_vendor_furthest_behind_its_pace_and_repairs_start_fresh() {
    let first = pin(Vendor::Codex, "gpt-6.1-sol");
    let pool = vec![pin(Vendor::Claude, "claude-opus-5-5"), pin(Vendor::Codex, "gpt-6-luna")];
    let pace: faber_engine::project::coordinator::Pace =
        std::sync::Arc::new(|vendor| Some(if vendor == Vendor::Claude { 0.30 } else { 0.05 }));
    // The first round is the product's pin, whatever the pace.
    assert_eq!(pick_reviewer(&first, &pool, Vendor::Claude, 1, Some(&pace)), first);
    // The Writer's vendor is never eligible, however much it has to spare.
    assert_eq!(pick_reviewer(&first, &pool, Vendor::Claude, 2, Some(&pace)).vendor, Vendor::Codex);
    // Among the eligible, the one furthest behind wins; with no pace known the first pin stays.
    let behind: faber_engine::project::coordinator::Pace =
        std::sync::Arc::new(|vendor| Some(if vendor == Vendor::Codex { 0.4 } else { 0.0 }));
    assert_eq!(pick_reviewer(&pin(Vendor::Codex, "a"), &[pin(Vendor::Codex, "b")], Vendor::Claude, 2, Some(&behind)).model, "a");
    assert_eq!(pick_reviewer(&first, &pool, Vendor::Claude, 3, None), first);
    assert!(repair_fresh(false, Some(1_000)), "a cold cache starts fresh");
    assert!(repair_fresh(true, None), "an unknown context starts fresh");
    assert!(repair_fresh(true, Some(200_000)), "a big context starts fresh");
    assert!(!repair_fresh(true, Some(12_000)), "warm and small resumes");
}
