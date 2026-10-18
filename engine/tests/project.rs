//! What a project is allowed to do: its repository, its grant, and the project list that keeps them. No process, git or CLI
//! runs here.

use faber_engine::model::Project;
use faber_engine::project::{Author, Effect, GitMode, Grant, RepoIdentity, SCHEMA};
use faber_engine::store::{Fault, Store};
use std::fs;

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
