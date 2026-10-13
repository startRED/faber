//! T5 adds gates to the broker; fixture executor only, never real git.
use faber_engine::project::git::{Broker, Executor, GitError, Invocation, Output, Repository};
use faber_engine::project::lease::{Termination, hash};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{FileOp, WriteOp};
use faber_engine::project::{Author, GitMode, Grant, Lease, RepoIdentity};
use std::collections::VecDeque;
const BASE: &str = "1111111111111111111111111111111111111111";
const TREE: &str = "2222222222222222222222222222222222222222";
struct Fake {
    outputs: VecDeque<String>,
    calls: Vec<Invocation>,
}
impl Executor for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        self.calls.push(call.clone());
        if call.args.contains(&"remove".into()) {
            let path = std::path::Path::new(call.args.last().unwrap());
            std::fs::remove_dir_all(path).unwrap();
        }
        Ok(Output { success: true, stdout: self.outputs.pop_front().expect("unexpected git").into_bytes() })
    }
}
fn setup(outputs: &[&str]) -> (tempfile::TempDir, tempfile::TempDir, Broker<Fake>, Grant, Lease, std::path::PathBuf) {
    let dir = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let data = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let identity = RepoIdentity::new(
        &dir.path().join(".git").canonicalize().unwrap().to_string_lossy(),
        &dir.path().canonicalize().unwrap().to_string_lossy(),
    );
    let repository = Repository {
        identity: identity.clone(),
        base: BASE.into(),
        author: Some(Author { name: "Test".into(), email: "test@example.invalid".into() }),
    };
    let mut grant = Grant::new(&identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    let mut broker = Broker::open(
        repository,
        Journal::open(dir.path(), data.path(), "m1").unwrap(),
        "m1",
        3,
        Fake { outputs: std::iter::once(String::new()).chain(outputs.iter().map(|s| s.to_string())).collect(), calls: vec![] },
    )
    .unwrap();
    broker.create_branch(|| Ok(Some(grant.clone())), "branch").unwrap();
    let lease =
        broker.leases().acquire(Lease::new(&identity.common_dir, "m1", "w1", 1, vec!["src".into()], BASE, &"a".repeat(64), 1)).unwrap();
    let root = broker.journal().private().join(format!("writer-{}", lease.generation));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "before").unwrap();
    broker.leases().bind(&lease, &root).unwrap();
    broker
        .leases()
        .patch(
            &lease,
            Some(&grant),
            3,
            &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "after".into() }),
        )
        .unwrap();
    (dir, data, broker, grant, lease, root)
}
#[test]
fn a_tree_different_from_the_proof_never_reaches_commit_tree() {
    let (_dir, _data, mut b, g, l, _root) = setup(&[BASE, "", "", "", "src/a.rs\0", TREE]);
    assert!(b.commit_proven(|| Ok(Some(g.clone())), &l, "commit", "Fix", &["src/a.rs".into()], &"3".repeat(40)).is_err());
    assert!(!b.executor().calls.iter().any(|c| c.args.contains(&"commit-tree".into())));
}
#[test]
fn compose_uses_a_private_index_and_the_current_own_head() {
    let (_dir, _data, mut b, g, l, _root) = setup(&[BASE, BASE, "", "", TREE]);
    assert_eq!(b.compose_tree(|| Ok(Some(g.clone())), &l, &["src/a.rs".into()]).unwrap(), TREE);
    let calls = &b.executor().calls;
    let indexed: Vec<_> = calls.iter().filter(|c| c.private_index.is_some()).collect();
    assert_eq!(indexed.len(), 3);
    for c in indexed {
        assert!(c.private_index.as_ref().unwrap().starts_with(b.journal().private()));
    }
}
#[test]
fn r106_worktrees_delete_only_after_termination_settlement_and_lease_release() {
    let (_dir, _data, mut b, g, l, root) = setup(&[""]);
    assert!(b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").is_err());
    assert_eq!(b.executor().calls.len(), 1);
    assert!(root.exists());
    b.leases().release(&l, Termination { process: None, tree_dead: true, cost_settled: true }).unwrap();
    b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").unwrap();
    assert!(!root.exists());
    assert!(b.journal().private().join("effects.json").exists());
    assert!(b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").is_err());
    assert_eq!(b.executor().calls.len(), 2);
}

#[test]
fn r106_disk_budget_counts_owned_worktrees_as_well_as_build_resources() {
    let (_repo, _data, b, _grant, _lease, _root) = setup(&[]);
    let resources = faber_engine::project::disk::Resources::load(b.journal()).unwrap();
    assert!(resources.used_bytes(b.journal()).unwrap() >= 5);
}
