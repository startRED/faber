//! The self-development records: what a project is allowed to do, the mission graph, the durable journal, and the
//! worker and screen contracts. No process, git or CLI runs here; a fake disk stands in for a full one.

use faber_engine::model::{Project, Vendor};
use faber_engine::project::store::{Journal, JournalError};
use faber_engine::project::wire::{self, Envelope, Expect, MissionView, StartRequest, WireError};
use faber_engine::project::{
    Author, Brief, Contract, ContractPin, DecidedBy, Decision, DecisionKind, Effect, GitMode, Grant, GraphError, Intent, IntentKind, Lease,
    LeaseState, Mission, MissionState, Outcome, Plan, ProcessId, Receipt, RepoIdentity, Role, RunMode, Runtime, SCHEMA, Task, TaskKind,
    TaskState,
};
use faber_engine::store::{Disk, Fault, RealDisk, Store};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

const COMMON: &str = "E:/src/faber/.git";

#[test]
fn screen_grant_refuses_stale_revision_repository_rebinding_and_deferred_publication() {
    let mut p = Project::new("p1".into(), "P".into(), "E:/src/faber".into());
    p.repo = Some(repo());
    p.grant = Some(grant(GitMode::Seguro, 3));
    let before = p.clone();
    assert_eq!(p.accept_grant(grant(GitMode::Seguro, 3)), Err("reason.grant.stale"));
    assert_eq!(p, before);
    let mut other = grant(GitMode::Seguro, 4);
    other.repo = "E:/another/.git".into();
    assert_eq!(p.accept_grant(other), Err("reason.grant.otherRepo"));
    assert_eq!(p, before);
    assert_eq!(p.accept_grant(grant(GitMode::Equilibrado, 4)), Err("reason.grant.mode"));
    assert_eq!(p, before);
    assert_eq!(p.accept_grant(grant(GitMode::Seguro, 4)), Ok(()));
}

#[test]
fn screen_grant_refuses_newer_authority_and_preserves_additive_metadata() {
    let mut p = Project::new("p1".into(), "P".into(), "E:/src/faber".into());
    p.repo = Some(repo());
    let mut old = grant(GitMode::Seguro, 3);
    old.extra.insert("futureMetadata".into(), serde_json::json!({"keep":true}));
    p.grant = Some(old.clone());
    p.accept_grant(grant(GitMode::Seguro, 4)).unwrap();
    assert_eq!(p.grant.as_ref().unwrap().extra, old.extra);
    p.grant.as_mut().unwrap().schema = SCHEMA + 1;
    let before = p.clone();
    assert_eq!(p.accept_grant(grant(GitMode::Seguro, 5)), Err("reason.grant.newer"));
    assert_eq!(p, before);
}

#[test]
fn starting_again_with_the_same_authority_keeps_the_grant_and_its_revision() {
    let mut p = enrolled(GitMode::Seguro);
    let kept = p.clone();
    let mut later = grant(GitMode::Seguro, 4);
    later.granted_at += 600;
    assert_eq!(p.accept_grant(later.clone()), Ok(()));
    assert_eq!(p, kept, "missions admitted under revision 3 stay current");
    later.self_update = true;
    p.accept_grant(later.clone()).unwrap();
    assert_eq!(p.grant.as_ref().map(|g| (g.revision, g.self_update)), Some((4, true)), "a changed trust is a new revision");
    p.grant.as_mut().unwrap().revoked_at = Some(1);
    later.revision = 5;
    p.accept_grant(later).unwrap();
    assert_eq!(p.grant.as_ref().map(|g| (g.revision, g.revoked_at)), Some((5, None)), "a revoked grant is given again");
}

fn repo() -> RepoIdentity {
    RepoIdentity::new(COMMON, "E:/src/faber")
}

fn grant(mode: GitMode, revision: u64) -> Grant {
    let mut g = Grant::new(COMMON, mode, revision, 1_791_000_000);
    g.author = Some(Author { name: "Erick".into(), email: "erick@example.com".into() });
    g
}

fn enrolled(mode: GitMode) -> Project {
    let mut p = Project::new("p1".into(), "faber".into(), "E:/src/faber".into());
    p.repo = Some(repo());
    p.grant = Some(grant(mode, 3));
    p
}

// Authority

#[test]
fn a_project_saved_before_missions_loads_and_grants_nothing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("projects.json"), r#"[{"id": "p1", "name": "faber", "path": "E:/src/faber"}]"#).unwrap();
    let projects = Store::new(dir.path()).projects_checked().unwrap();
    assert_eq!((projects[0].name.as_str(), projects[0].repo.is_none(), projects[0].grant.is_none()), ("faber", true, true));
    for effect in Effect::ALL {
        assert_eq!(projects[0].allows(effect, 0), Err("reason.grant.none"), "{effect:?}");
    }
}

#[test]
fn the_repository_and_its_grant_survive_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let project = enrolled(GitMode::Seguro);
    Store::new(dir.path()).save_projects(std::slice::from_ref(&project)).unwrap();
    let back = Store::new(dir.path()).projects_checked().unwrap();
    assert_eq!(back, vec![project]);
    assert_eq!(back[0].allows(Effect::Commit, 3), Ok(()));
    assert_eq!(back[0].allows(Effect::Commit, 2), Err("reason.grant.stale"), "a grant answers only for its own revision");
}

#[test]
fn seguro_stays_local_and_equilibrado_adds_only_its_own_push_and_draft() {
    let seguro = grant(GitMode::Seguro, 1);
    for effect in [Effect::Worktree, Effect::Commit, Effect::FastForward] {
        assert_eq!(seguro.allows(&repo(), 1, effect), Ok(()), "{effect:?}");
    }
    for effect in [Effect::Push, Effect::DraftPr] {
        assert_eq!(seguro.allows(&repo(), 1, effect), Err("reason.grant.mode"), "{effect:?}");
    }
    assert_eq!(seguro.allows(&repo(), 1, Effect::SelfUpdate), Err("reason.grant.selfUpdate"), "trust is a separate grant");
    let mut balanced = grant(GitMode::Equilibrado, 1);
    assert_eq!(balanced.allows(&repo(), 1, Effect::Push), Err("reason.grant.noRemote"));
    balanced.remote = Some("origin".into());
    assert_eq!(balanced.allows(&repo(), 1, Effect::Push), Ok(()));
    assert_eq!(balanced.allows(&repo(), 1, Effect::DraftPr), Ok(()));
    assert_eq!(GitMode::default(), GitMode::Seguro, "the start card selects Seguro; selecting is not granting");
}

#[test]
fn a_revoked_grant_another_repository_or_a_missing_author_allows_nothing_it_should_not() {
    let mut g = grant(GitMode::Seguro, 1);
    let other = RepoIdentity::new("E:/src/other/.git", "E:/src/other");
    assert_eq!(g.allows(&other, 1, Effect::Worktree), Err("reason.grant.otherRepo"));
    g.author = None;
    assert_eq!(g.allows(&repo(), 1, Effect::Commit), Err("reason.grant.noAuthor"), "no invented author");
    g.revoked_at = Some(1_791_000_100);
    assert_eq!(g.allows(&repo(), 1, Effect::Worktree), Err("reason.grant.revoked"));
    let newer = Grant { schema: SCHEMA + 1, ..grant(GitMode::Seguro, 1) };
    assert_eq!(newer.allows(&repo(), 1, Effect::Worktree), Err("reason.grant.newer"), "an older Faber does not guess");
}

#[test]
fn fields_a_newer_faber_wrote_survive_an_older_reader() {
    let dir = tempfile::tempdir().unwrap();
    let json = r#"[{"id": "p1", "name": "faber", "path": "x", "future": {"a": 1},
        "grant": {"schema": 1, "repo": "x/.git", "mode": "seguro", "revision": 2, "grantedAt": 5, "selfUpdate": false, "later": true}}]"#;
    fs::write(dir.path().join("projects.json"), json).unwrap();
    let store = Store::new(dir.path());
    let projects = store.projects_checked().unwrap();
    store.save_projects(&projects).unwrap();
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.path().join("projects.json")).unwrap()).unwrap();
    assert_eq!(raw[0]["future"]["a"], 1);
    assert_eq!(raw[0]["grant"]["later"], true);
}

#[test]
fn a_damaged_project_list_is_an_error_for_projects() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("projects.json"), "[{").unwrap();
    let store = Store::new(dir.path());
    assert_eq!(store.projects_checked(), Err(Fault::Corrupt));
    assert_eq!(Fault::Corrupt.key(), "error.store.corrupt");
    assert!(Store::new(tempfile::tempdir().unwrap().path()).projects_checked().unwrap().is_empty(), "no file is no project");
}

// The graph

fn task(id: &str, requires: &[&str], deliverables: &[&str]) -> Task {
    let mut t = Task::new(id, id, TaskKind::Research);
    t.requires = requires.iter().map(|s| s.to_string()).collect();
    t.deliverables = deliverables.iter().map(|s| s.to_string()).collect();
    t
}

fn writer(id: &str, requires: &[&str], writes: &[&str]) -> Task {
    let mut t = task(id, requires, &[]);
    t.kind = TaskKind::Writer;
    t.writes = writes.iter().map(|s| s.to_string()).collect();
    t
}

fn plan(tasks: Vec<Task>) -> Plan {
    Plan::new("m1", 1, tasks)
}

#[test]
fn a_plan_with_a_cycle_is_refused() {
    let p = plan(vec![task("a", &["c"], &[]), task("b", &["a"], &[]), task("c", &["b"], &[]), task("d", &[], &[])]);
    let err = p.validate().unwrap_err();
    assert_eq!(err, GraphError::Cycle(vec!["a".into(), "b".into(), "c".into()]));
    assert_eq!(err.key(), "error.plan.cycle");
    assert_eq!(plan(vec![task("a", &["a"], &[])]).validate(), Err(GraphError::Cycle(vec!["a".into()])));
}

#[test]
fn a_plan_that_names_a_missing_or_repeated_task_is_refused() {
    let missing = plan(vec![task("a", &[], &[]), task("b", &["a", "ghost"], &[])]).validate().unwrap_err();
    assert_eq!(missing, GraphError::MissingTask { task: "b".into(), missing: "ghost".into() });
    assert_eq!(missing.key(), "error.plan.missingTask");
    let twice = plan(vec![task("a", &[], &[]), task("a", &[], &[])]).validate().unwrap_err();
    assert_eq!(twice.key(), "error.plan.duplicateTask");
    assert_eq!(plan(vec![task("../x", &[], &[])]).validate().unwrap_err().key(), "error.plan.badId");
}

#[test]
fn two_tasks_cannot_deliver_the_same_thing() {
    let p = plan(vec![task("a", &[], &["engine/src/project.rs"]), task("b", &["a"], &["engine/src/project.rs"])]);
    let err = p.validate().unwrap_err();
    assert_eq!(
        err,
        GraphError::DuplicateDeliverable { deliverable: "engine/src/project.rs".into(), first: "a".into(), second: "b".into() }
    );
    assert_eq!(err.key(), "error.plan.duplicateDeliverable");
}

#[test]
fn unordered_writers_cannot_share_an_area_but_ordered_ones_can() {
    let parent = plan(vec![writer("a", &[], &["engine/src"]), writer("b", &[], &["Engine\\SRC\\project.rs"])]);
    let err = parent.validate().unwrap_err();
    assert_eq!(err.key(), "error.plan.overlap", "a parent folder and a case alias are the same area");
    assert_eq!(plan(vec![writer("a", &[], &["engine/src"]), writer("b", &["a"], &["engine/src/project.rs"])]).validate(), Ok(()));
    assert_eq!(plan(vec![writer("a", &[], &["engine/src"]), writer("b", &[], &["engine/srcx"])]).validate(), Ok(()));
    let mut reader = task("r", &[], &[]);
    reader.writes = vec!["ui".into()];
    assert_eq!(plan(vec![reader]).validate().unwrap_err().key(), "error.plan.readOnlyWrites");
    for bad in ["/etc", "C:/x", "a/../b", ""] {
        assert_eq!(plan(vec![writer("a", &[], &[bad])]).validate().unwrap_err().key(), "error.plan.badPath", "{bad:?}");
    }
}

#[test]
fn a_contract_revision_invalidates_only_the_tasks_built_on_it_and_their_descendants() {
    // C is accepted; E, U and T build on it; I integrates E, U and T; X is unrelated.
    let pin = |id: &str, digest: &str| ContractPin { id: id.into(), digest: digest.into() };
    let mut e = writer("e", &[], &["engine"]);
    e.contracts = vec![pin("engine-shape", "d1")];
    let mut u = writer("u", &[], &["ui"]);
    u.contracts = vec![pin("screen-shape", "s1")];
    let t = task("t", &[], &["tests/proposal.rs"]);
    let i = task("i", &["e", "u", "t"], &[]);
    let x = task("x", &[], &[]);
    let mut p = plan(vec![e, u, t, i, x]);
    p.validate().unwrap();
    for t in &mut p.tasks {
        t.state = TaskState::Done;
    }
    assert!(p.revise("engine-shape", "d1").is_empty(), "the same digest changes nothing");
    let hit = p.revise("engine-shape", "d2");
    assert_eq!(hit, ["e", "i"]);
    let states: BTreeMap<&str, (TaskState, Option<&str>)> =
        p.tasks.iter().map(|t| (t.id.as_str(), (t.state, t.reason.as_deref()))).collect();
    assert_eq!(states["e"], (TaskState::Blocked, Some("reason.contract.revised")));
    assert_eq!(states["i"], (TaskState::Blocked, Some("reason.contract.revised")));
    for id in ["u", "t", "x"] {
        assert_eq!(states[id], (TaskState::Done, None), "{id} did not depend on the revised contract");
    }
}

fn contract() -> Contract {
    let mut c = Contract::new("engine-shape", "lead", "abc123");
    c.boundary.paths = vec!["engine/src/project.rs".into()];
    c.examples.success = "a plan with two tasks validates".into();
    c.examples.failure = "a cycle is refused with error.plan.cycle".into();
    c.proof.commands = vec!["cargo-t".into()];
    c.seal()
}

#[test]
fn a_contract_digest_covers_its_content_whatever_the_key_order() {
    let c = contract();
    assert!(c.verify() && c.digest.len() == 64);
    let mut changed = c.clone();
    changed.examples.failure = "something else".into();
    assert!(!changed.verify());
    assert_ne!(changed.clone().seal().digest, c.digest);
    // A newer Faber's field lands in `extra` here, wherever it sat in the file, and its digest still holds.
    let mut json: serde_json::Value = serde_json::to_value(c.clone()).unwrap();
    json["zeta"] = serde_json::json!({"b": 1, "a": [2]});
    let newer: Contract = serde_json::from_value(json).unwrap();
    assert!(!newer.verify(), "an added field is a change");
    let mut sealed = serde_json::to_value(newer.seal()).unwrap().as_object().unwrap().clone();
    let zeta = sealed.remove("zeta").unwrap();
    let rest = serde_json::to_string(&sealed).unwrap();
    let moved = format!("{{\"zeta\": {zeta}, {}", &rest[1..]);
    assert!(serde_json::from_str::<Contract>(&moved).unwrap().verify(), "key order does not change the digest");
}

#[test]
fn the_ai_answers_only_technical_choices_and_only_once() {
    let mut money = Decision::new("d1", "lead", "Spend the canary allowance today?", DecisionKind::Money);
    money.options = vec!["yes".into(), "no".into()];
    money.default = Some("no".into());
    assert_eq!(money.answer(DecidedBy::Ai, "no", 1), Err("error.decision.person"), "autonomous mode never defaults money");
    assert_eq!(money.answer(DecidedBy::Person, "maybe", 1), Err("error.decision.option"));
    assert_eq!(money.answer(DecidedBy::Person, "yes", 2), Ok(()));
    assert_eq!(money.answer(DecidedBy::Person, "no", 3), Err("error.decision.answered"));
    assert_eq!((money.answered_by, money.answered_at), (Some(DecidedBy::Person), Some(2)));
    let mut technical = Decision::new("d2", "lead", "Which module owns the lease?", DecisionKind::Technical);
    assert_eq!(technical.answer(DecidedBy::Ai, "project::lease", 1), Ok(()));
}

// The journal

/// A disk that fills up halfway through writing the temporary file.
struct FullDisk;
impl Disk for FullDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(path, &bytes[..bytes.len() / 2])?;
        Err(io::Error::from(io::ErrorKind::StorageFull))
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealDisk.rename(from, to)
    }
}

/// A disk whose final rename is refused, as an antivirus holding the file does.
struct NoRename;
impl Disk for NoRename {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealDisk.write(path, bytes)
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }
}

struct Dirs {
    repo: tempfile::TempDir,
    data: tempfile::TempDir,
}

fn dirs() -> Dirs {
    Dirs { repo: tempfile::tempdir().unwrap(), data: tempfile::tempdir().unwrap() }
}

fn mission() -> Mission {
    Mission::new("m1", "p1", COMMON, "Fix the failing normalization test", "abc123", 3, 1_791_000_000)
}

#[test]
fn mission_records_live_in_the_repository_and_sessions_stay_private() {
    let d = dirs();
    let journal = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    let m = mission();
    assert_eq!((m.branch.as_str(), m.state, m.mode), ("faber/m1", MissionState::Planning, RunMode::Normal));
    journal.save_mission(&m).unwrap();
    journal.save_plan(&plan(vec![task("a", &[], &[])])).unwrap();
    journal.save_contract(&contract()).unwrap();
    let brief = Brief::new("a", 1, Role::Research, "Find the callers of clean_path", "abc123", contract().pin());
    journal.save_brief(&brief).unwrap();
    journal.save_decision(&Decision::new("d1", "lead", "Which name?", DecisionKind::Taste)).unwrap();
    let mut runtime = Runtime::new();
    runtime.sessions.entry("a".into()).or_default().insert(Vendor::Codex, "thread-1".into());
    runtime.leases.push(Lease {
        process: Some(ProcessId { pid: 42, started_at: 1_791_000_001 }),
        ..Lease::new(COMMON, "m1", "a", 1, vec!["engine/src".into()], "abc123", &contract().digest, 1)
    });
    journal.save_runtime(&runtime).unwrap();

    let records = d.repo.path().join(".faber").join("missions").join("m1");
    for file in ["mission.json", "plan.json", "contracts/engine-shape.json", "briefs/a.json", "decisions/d1.json"] {
        assert!(records.join(file).is_file(), "{file}");
    }
    let everything: String = walk(d.repo.path()).iter().map(|p| fs::read_to_string(p).unwrap()).collect();
    assert!(!everything.contains("thread-1") && !everything.contains("\"pid\""), "sessions and processes stay out of the repo");

    let again = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    assert_eq!(again.mission().unwrap(), Some(m));
    assert_eq!(again.plan().unwrap().unwrap().tasks.len(), 1);
    assert_eq!(again.contract("engine-shape").unwrap(), Some(contract()));
    assert_eq!(again.brief("a").unwrap(), Some(brief));
    assert_eq!(again.decisions().unwrap().len(), 1);
    let back = again.runtime().unwrap();
    assert_eq!(back, runtime);
    assert_eq!(back.leases[0].state, LeaseState::Held);
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() { out.extend(walk(&path)) } else { out.push(path) }
    }
    out
}

#[test]
fn a_full_disk_keeps_the_previous_record_and_says_why() {
    let d = dirs();
    Journal::open(d.repo.path(), d.data.path(), "m1").unwrap().save_mission(&mission()).unwrap();
    let full = Journal::open_with(d.repo.path(), d.data.path(), "m1", Arc::new(FullDisk)).unwrap();
    let mut changed = mission();
    changed.goal = "Something else".into();
    let err = full.save_mission(&changed).unwrap_err();
    assert_eq!(err, JournalError::Store(Fault::DiskFull));
    assert_eq!(err.key(), "error.store.full");
    assert_eq!(full.mission().unwrap(), Some(mission()), "the old record is intact");
    let records = d.repo.path().join(".faber/missions/m1");
    assert!(!records.join("mission.json.tmp").exists(), "the half-written temp file is gone");
}

#[test]
fn a_refused_rename_keeps_the_previous_record_and_says_why() {
    let d = dirs();
    Journal::open(d.repo.path(), d.data.path(), "m1").unwrap().save_mission(&mission()).unwrap();
    let stuck = Journal::open_with(d.repo.path(), d.data.path(), "m1", Arc::new(NoRename)).unwrap();
    let mut changed = mission();
    changed.state = MissionState::Running;
    assert_eq!(stuck.save_mission(&changed).unwrap_err().key(), "error.store.write");
    assert_eq!(stuck.mission().unwrap().unwrap().state, MissionState::Planning);
}

#[test]
fn a_damaged_or_newer_record_blocks_instead_of_reading_as_empty() {
    let d = dirs();
    let journal = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    assert_eq!(journal.mission().unwrap(), None, "no record yet is no mission");
    assert_eq!(journal.runtime().unwrap(), Runtime::new(), "no runtime yet is an empty one");
    fs::write(journal.records().join("mission.json"), "{\"schema\": 1, \"id\": ").unwrap();
    assert_eq!(journal.mission().unwrap_err(), JournalError::Store(Fault::Corrupt));
    fs::create_dir_all(journal.private()).unwrap();
    fs::write(journal.private().join("runtime.json"), "garbage").unwrap();
    assert_eq!(journal.runtime().unwrap_err().key(), "error.store.corrupt", "damaged leases never become no leases");
    let mut newer = serde_json::to_value(mission()).unwrap();
    newer["schema"] = serde_json::json!(SCHEMA + 1);
    fs::write(journal.records().join("mission.json"), newer.to_string()).unwrap();
    assert_eq!(journal.mission().unwrap_err(), JournalError::Newer);
    assert_eq!(JournalError::Newer.key(), "error.journal.newer");
}

fn intent(id: &str) -> Intent {
    let mut i = Intent::new(id, "m1", IntentKind::Commit, 3, 1_791_000_000);
    i.task = Some("e".into());
    i.expected.insert("parent".into(), "abc123".into());
    i
}

#[test]
fn an_intent_without_a_receipt_is_pending_after_reopening() {
    let d = dirs();
    let journal = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    journal.record_intent(&intent("i1")).unwrap();
    journal.record_intent(&intent("i2")).unwrap();
    journal.record_receipt(&Receipt::new("i1", Outcome::Done, 1_791_000_010)).unwrap();
    let again = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    assert_eq!(again.pending().unwrap(), vec![intent("i2")], "an effect that may have happened is never forgotten");
}

#[test]
fn an_intent_is_recorded_once_and_its_receipt_lands_once() {
    let d = dirs();
    let journal = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    assert_eq!(journal.record_receipt(&Receipt::new("i1", Outcome::Done, 1)), Err(JournalError::NoIntent));
    journal.record_intent(&intent("i1")).unwrap();
    assert_eq!(journal.record_intent(&intent("i1")), Err(JournalError::IntentExists), "a replayed effect is refused");
    journal.record_receipt(&Receipt::new("i1", Outcome::Unknown, 2)).unwrap();
    assert_eq!(journal.record_receipt(&Receipt::new("i1", Outcome::Done, 3)), Err(JournalError::ReceiptExists));
    assert_eq!(journal.pending().unwrap(), vec![intent("i1")], "an unknown outcome still requires reconciliation");
}

#[test]
fn ids_that_could_leave_the_folder_are_refused() {
    let d = dirs();
    for bad in ["", "..", "a/b", "a\\b", "C:", "m 1", &"x".repeat(65)] {
        assert_eq!(Journal::open(d.repo.path(), d.data.path(), bad).err(), Some(JournalError::BadId), "{bad:?}");
    }
    let journal = Journal::open(d.repo.path(), d.data.path(), "m1").unwrap();
    assert_eq!(journal.contract("../mission").unwrap_err(), JournalError::BadId);
    let mut other = mission();
    other.id = "m2".into();
    assert_eq!(journal.save_mission(&other), Err(JournalError::Mismatch), "one journal holds one mission");
}

// The worker contract

fn expect() -> Expect {
    Expect { task: "e".into(), attempt: 1, base: "abc123".into(), contract: "d1".into(), lease: 4 }
}

fn envelope(action: serde_json::Value) -> String {
    serde_json::json!({"schema": wire::SCHEMA, "task": "e", "attempt": 1, "base": "abc123", "contract": "d1", "lease": 4, "action": action})
        .to_string()
}

const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn a_writer_patch_parses_into_explicit_file_operations() {
    let text = envelope(serde_json::json!({"patch": {"ops": [
        {"write": {"path": "engine/src/project.rs", "expect": HASH, "content": "pub fn x() {}\n"}},
        {"write": {"path": "engine/src/new.rs", "content": "// new\n"}},
        {"rename": {"from": "a.rs", "to": "b.rs", "expect": HASH}},
        {"delete": {"path": "c.rs", "expect": HASH}}
    ]}}));
    let env = Envelope::parse(&text, &expect(), Role::Writer).unwrap();
    assert!(matches!(env.action, wire::Action::Patch(ref p) if p.ops.len() == 4));
}

#[test]
fn an_unknown_field_or_action_is_refused() {
    let extra_field = envelope(serde_json::json!({"read": {"paths": ["a.rs"], "shell": "rm -rf /"}}));
    assert_eq!(Envelope::parse(&extra_field, &expect(), Role::Writer), Err(WireError::Malformed));
    let unknown_action = envelope(serde_json::json!({"exec": {"argv": ["git", "push"]}}));
    assert_eq!(Envelope::parse(&unknown_action, &expect(), Role::Writer), Err(WireError::Malformed));
    let mut top: serde_json::Value = serde_json::from_str(&envelope(serde_json::json!({"read": {"paths": []}}))).unwrap();
    top["grant"] = serde_json::json!(1000);
    assert_eq!(Envelope::parse(&top.to_string(), &expect(), Role::Writer).unwrap_err().key(), "error.wire.malformed");
    let bad_hash = envelope(serde_json::json!({"patch": {"ops": [{"delete": {"path": "c.rs", "expect": "abc"}}]}}));
    assert_eq!(Envelope::parse(&bad_hash, &expect(), Role::Writer), Err(WireError::Malformed));
}

#[test]
fn a_stale_or_newer_envelope_is_refused() {
    let text = envelope(serde_json::json!({"read": {"paths": ["a.rs"]}}));
    for stale in [
        Expect { attempt: 2, ..expect() },
        Expect { base: "def456".into(), ..expect() },
        Expect { contract: "d2".into(), ..expect() },
        Expect { lease: 5, ..expect() },
        Expect { task: "u".into(), ..expect() },
    ] {
        assert_eq!(Envelope::parse(&text, &stale, Role::Writer), Err(WireError::Stale));
    }
    let mut newer: serde_json::Value = serde_json::from_str(&text).unwrap();
    newer["schema"] = serde_json::json!(wire::SCHEMA + 1);
    assert_eq!(Envelope::parse(&newer.to_string(), &expect(), Role::Writer), Err(WireError::Schema));
}

#[test]
fn paths_must_stay_relative_and_inside_the_worktree() {
    for bad in ["/etc/passwd", "C:\\Windows\\x", "..\\..\\x", "a/../../b", "", "a:stream"] {
        let text = envelope(serde_json::json!({"patch": {"ops": [{"write": {"path": bad, "content": ""}}]}}));
        assert_eq!(Envelope::parse(&text, &expect(), Role::Writer), Err(WireError::Path), "{bad:?}");
    }
    let rename = envelope(serde_json::json!({"patch": {"ops": [{"rename": {"from": "a.rs", "to": "../b.rs", "expect": HASH}}]}}));
    assert_eq!(Envelope::parse(&rename, &expect(), Role::Writer), Err(WireError::Path), "both rename paths are checked");
}

#[test]
fn an_oversized_envelope_is_refused_before_parsing() {
    let huge = "x".repeat(wire::MAX_ENVELOPE + 1);
    assert_eq!(Envelope::parse(&huge, &expect(), Role::Writer), Err(WireError::TooBig));
    let paths: Vec<String> = (0..=wire::MAX_PATHS).map(|i| format!("f{i}.rs")).collect();
    let many = envelope(serde_json::json!({"read": {"paths": paths}}));
    assert_eq!(Envelope::parse(&many, &expect(), Role::Writer), Err(WireError::TooBig));
    let content = "y".repeat(wire::MAX_FILE + 1);
    let big = envelope(serde_json::json!({"patch": {"ops": [{"write": {"path": "a.rs", "content": content}}]}}));
    assert_eq!(Envelope::parse(&big, &expect(), Role::Writer).unwrap_err().key(), "error.wire.tooBig");
}

#[test]
fn each_role_sends_only_its_own_actions() {
    let patch = envelope(serde_json::json!({"patch": {"ops": [{"write": {"path": "a.rs", "content": ""}}]}}));
    for role in [Role::Lead, Role::Research, Role::Reviewer] {
        assert_eq!(Envelope::parse(&patch, &expect(), role), Err(WireError::Role), "{role:?}");
    }
    assert!(Envelope::parse(&patch, &expect(), Role::TestProposal).is_ok(), "kept as an artifact outside the source");
    let plan = envelope(
        serde_json::json!({"plan": {"summary": "s", "acceptance": "a", "tasks": [{"id": "a", "title": "A", "kind": "research"}]}}),
    );
    assert_eq!(Envelope::parse(&plan, &expect(), Role::Writer), Err(WireError::Role), "workers cannot plan or spawn peers");
    assert!(Envelope::parse(&plan, &expect(), Role::Lead).is_ok());
    let ask = envelope(serde_json::json!({"decision": {"question": "q", "kind": "money"}}));
    assert_eq!(Envelope::parse(&ask, &expect(), Role::Writer), Err(WireError::Role));
    let done = envelope(serde_json::json!({"complete": {"summary": "done", "next": "review"}}));
    for role in [Role::Lead, Role::Research, Role::TestProposal, Role::Writer, Role::Reviewer] {
        assert!(Envelope::parse(&done, &expect(), role).is_ok(), "{role:?}");
    }
}

#[test]
fn a_lead_plan_is_checked_like_the_graph() {
    let cyclic = envelope(serde_json::json!({"plan": {"summary": "s", "acceptance": "a", "tasks": [
        {"id": "a", "title": "A", "kind": "research", "requires": ["b"]},
        {"id": "b", "title": "B", "kind": "research", "requires": ["a"]}
    ]}}));
    let err = Envelope::parse(&cyclic, &expect(), Role::Lead).unwrap_err();
    assert_eq!(err.key(), "error.plan.cycle");
}

// The screen contract

#[test]
fn the_screen_contract_examples_parse() {
    let start: StartRequest = serde_json::from_str(r#"{"projectId": "p1", "goal": "Fix the failing test"}"#).unwrap();
    assert_eq!(start.goal, "Fix the failing test");
    assert!(serde_json::from_str::<StartRequest>(r#"{"projectId": "p1", "goal": "x", "mode": "livre"}"#).is_err());
    let view: MissionView = serde_json::from_str(wire::MISSION_EXAMPLE).unwrap();
    assert_eq!(view.tasks.len(), 2);
    assert_eq!(view.cost.own_micros, None, "unknown cost is unknown, not zero");
    assert_eq!(serde_json::from_str::<MissionView>(&serde_json::to_string(&view).unwrap()).unwrap(), view);
    assert_eq!(wire::COMMANDS.len(), 9);
}
