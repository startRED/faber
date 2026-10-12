//! Git arguments and durable effects with a scripted executor; no processes or real repositories.
use faber_engine::project::git::{Broker, Executor, GitError, Invocation, Output, Repository, enroll};
use faber_engine::project::store::Journal;
use faber_engine::project::{Author, GitMode, Grant, RepoIdentity};
use std::collections::VecDeque;
use std::path::Path;

const BASE: &str = "1111111111111111111111111111111111111111";
const NEXT: &str = "2222222222222222222222222222222222222222";

#[derive(Default)]
struct Fake {
    replies: VecDeque<Result<Output, GitError>>,
    calls: Vec<Invocation>,
}
impl Fake {
    fn replies(lines: &[&str]) -> Self {
        Self { replies: lines.iter().map(|s| Ok(Output { success: true, stdout: s.as_bytes().to_vec() })).collect(), calls: vec![] }
    }
}
impl Executor for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        self.calls.push(call.clone());
        self.replies.pop_front().expect("unexpected git command")
    }
}
fn repository(root: &Path) -> Repository {
    std::fs::create_dir(root.join(".git")).unwrap();
    Repository {
        identity: RepoIdentity::new(
            &root.join(".git").canonicalize().unwrap().to_string_lossy(),
            &root.canonicalize().unwrap().to_string_lossy(),
        ),
        base: BASE.into(),
        author: Some(Author { name: "Test".into(), email: "test@example.invalid".into() }),
    }
}
fn grant(repo: &Repository) -> Grant {
    let mut grant = Grant::new(&repo.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repo.author.clone();
    grant
}

#[test]
fn enrollment_uses_the_canonical_common_directory_and_a_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let mut fake =
        Fake::replies(&["false\n", &repo.identity.root, &repo.identity.common_dir, BASE, "Test\n", "test@example.invalid\n", ""]);
    let found = enroll(&mut fake, dir.path(), "HEAD").unwrap();
    assert_eq!(found.identity, repo.identity);
    assert_eq!(found.base, BASE);
    assert!(fake.calls.iter().any(|c| c.args.iter().any(|a| a == "--git-common-dir")));
    for call in fake.calls {
        assert!(call.args.contains(&"core.fsmonitor=false".into()));
        assert!(call.args.contains(&"protocol.allow=never".into()));
        assert!(call.args.contains(&"credential.helper=".into()));
    }
}

#[test]
fn enrollment_rejects_non_git_bare_missing_base_and_injected_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::default();
    for bad in ["--help", "HEAD;touch x", "HEAD\n", "a:b", "../HEAD", "HEAD^{tree}"] {
        assert_eq!(enroll(&mut fake, dir.path(), bad), Err(GitError::Ref));
    }
    assert!(fake.calls.is_empty());
    fake.replies.push_back(Ok(Output { success: false, stdout: vec![] }));
    assert_eq!(enroll(&mut fake, dir.path(), "HEAD"), Err(GitError::Repository));
    assert_eq!(enroll(&mut Fake::replies(&["true"]), dir.path(), "HEAD"), Err(GitError::Repository));
    let repo = repository(dir.path());
    let mut fake = Fake::replies(&["false", &repo.identity.root, &repo.identity.common_dir]);
    fake.replies.push_back(Ok(Output { success: false, stdout: vec![] }));
    assert_eq!(enroll(&mut fake, dir.path(), "HEAD"), Err(GitError::Base));
}

#[test]
fn effects_require_a_current_grant_and_cas_only_the_own_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(data.path(), data.path(), "m1").unwrap();
    let mut g = grant(&repo);
    let fake = Fake::replies(&["", "", "", ""]);
    let mut broker = Broker::open(repo, journal, "m1", 3, fake).unwrap();
    assert_eq!(broker.create_branch(None, "create"), Err(GitError::Authority("reason.grant.none")));
    g.revision = 4;
    assert_eq!(broker.create_branch(Some(&g), "create"), Err(GitError::Authority("reason.grant.stale")));
    g.revision = 3;
    broker.create_branch(Some(&g), "create").unwrap();
    broker.fast_forward(Some(&g), "land", BASE, NEXT).unwrap();
    g.revoked_at = Some(2);
    assert_eq!(broker.fast_forward(Some(&g), "again", NEXT, BASE), Err(GitError::Authority("reason.grant.revoked")));
    let calls = &broker.executor().calls;
    let changes: Vec<_> = calls.iter().filter(|c| c.args.contains(&"update-ref".into())).collect();
    assert_eq!(changes.len(), 2);
    assert!(changes[0].args.ends_with(&["refs/heads/faber/m1".into(), BASE.into(), "0".repeat(40)]));
    assert!(changes[1].args.ends_with(&["refs/heads/faber/m1".into(), NEXT.into(), BASE.into()]));
    assert!(!calls.iter().flat_map(|c| &c.args).any(|a| ["push", "reset", "clean", "stash", "-D", "--force"].contains(&a.as_str())));
    assert!(broker.journal().pending().unwrap().is_empty());
}

#[test]
fn stale_heads_and_unknown_delivery_preserve_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let g = grant(&repo);
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(data.path(), data.path(), "m1").unwrap();
    let mut fake = Fake::replies(&["", "", ""]);
    fake.replies.push_back(Ok(Output { success: false, stdout: vec![] }));
    fake.replies.push_back(Ok(Output { success: true, stdout: vec![] }));
    fake.replies.push_back(Ok(Output { success: true, stdout: vec![] }));
    fake.replies.push_back(Err(GitError::Process));
    let mut broker = Broker::open(repo, journal, "m1", 3, fake).unwrap();
    broker.create_branch(Some(&g), "create").unwrap();
    assert_eq!(broker.fast_forward(Some(&g), "stale", BASE, NEXT), Err(GitError::Command));
    assert_eq!(broker.fast_forward(Some(&g), "uncertain", BASE, NEXT), Err(GitError::Process));
    assert!(broker.fast_forward(Some(&g), "uncertain", BASE, NEXT).is_err());
    assert_eq!(broker.executor().calls.len(), 7, "uncertain effects must not repeat");
}
