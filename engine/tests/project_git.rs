//! Git arguments and durable effects with a scripted executor; no processes or real repositories.
use faber_engine::project::git::{Broker, Executor, GitError, Invocation, Output, Repository, enroll, valid_oid};
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
    let journal = Journal::open(dir.path(), data.path(), "m1").unwrap();
    let mut g = grant(&repo);
    let fake = Fake::replies(&["", "", "", ""]);
    let mut broker = Broker::open(repo, journal, "m1", 3, fake).unwrap();
    assert_eq!(broker.create_branch(|| Ok(None), "create"), Err(GitError::Authority("reason.grant.none")));
    g.revision = 4;
    assert_eq!(broker.create_branch(|| Ok(Some(g.clone())), "create"), Err(GitError::Authority("reason.grant.stale")));
    g.revision = 3;
    broker.create_branch(|| Ok(Some(g.clone())), "create").unwrap();
    broker.fast_forward(|| Ok(Some(g.clone())), "land", BASE, NEXT).unwrap();
    g.revoked_at = Some(2);
    assert_eq!(broker.fast_forward(|| Ok(Some(g.clone())), "again", NEXT, BASE), Err(GitError::Authority("reason.grant.revoked")));
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
    let journal = Journal::open(dir.path(), data.path(), "m1").unwrap();
    let mut fake = Fake::replies(&["", "", ""]);
    fake.replies.push_back(Ok(Output { success: false, stdout: vec![] }));
    fake.replies.push_back(Ok(Output { success: true, stdout: vec![] }));
    fake.replies.push_back(Ok(Output { success: true, stdout: vec![] }));
    fake.replies.push_back(Err(GitError::Process));
    let mut broker = Broker::open(repo, journal, "m1", 3, fake).unwrap();
    broker.create_branch(|| Ok(Some(g.clone())), "create").unwrap();
    assert_eq!(broker.fast_forward(|| Ok(Some(g.clone())), "stale", BASE, NEXT), Err(GitError::Command));
    assert_eq!(broker.fast_forward(|| Ok(Some(g.clone())), "uncertain", BASE, NEXT), Err(GitError::Process));
    assert!(broker.fast_forward(|| Ok(Some(g.clone())), "uncertain", BASE, NEXT).is_err());
    assert_eq!(broker.executor().calls.len(), 7, "uncertain effects must not repeat");
}

#[test]
fn review_7_null_oids_are_rejected_before_any_git_effect() {
    for width in [40, 64] {
        assert!(!valid_oid(&"0".repeat(width)));
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let g = grant(&repo);
    let data = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path(), data.path(), "m1").unwrap();
    let mut broker = Broker::open(repo, journal, "m1", 3, Fake::replies(&["", "", "", ""])).unwrap();
    broker.create_branch(|| Ok(Some(g.clone())), "create").unwrap();
    assert_eq!(broker.fast_forward(|| Ok(Some(g.clone())), "delete", BASE, &"0".repeat(40)), Err(GitError::Ref));
    assert_eq!(broker.executor().calls.len(), 1);
}

struct Revokes {
    inner: Fake,
    grant: std::sync::Arc<std::sync::Mutex<Grant>>,
}
impl Executor for Revokes {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let out = self.inner.execute(call)?;
        if call.args.contains(&"add".into()) {
            self.grant.lock().unwrap().revoked_at = Some(2);
        }
        Ok(out)
    }
}

#[test]
fn opus_l1_revocation_after_staging_blocks_commit_dispatch() {
    use faber_engine::project::Lease;
    use faber_engine::project::lease::hash;
    use faber_engine::project::wire::{FileOp, WriteOp};
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let g = grant(&repo);
    let live = std::sync::Arc::new(std::sync::Mutex::new(g.clone()));
    let executor = Revokes {
        inner: Fake::replies(&[
            BASE,
            "",
            "",
            "",
            "src/a.rs\0",
            NEXT,
            NEXT,
            &format!("{NEXT}\n{BASE}\nTest\ntest@example.invalid\nTest\ntest@example.invalid"),
            "",
            NEXT,
        ]),
        grant: live.clone(),
    };
    let request = Lease::new(&repo.identity.common_dir, "m1", "writer", 1, vec!["src".into()], BASE, &"a".repeat(64), 1);
    let mut broker = Broker::open(repo, Journal::open(dir.path(), data.path(), "m1").unwrap(), "m1", 3, executor).unwrap();
    let lease = broker.leases().acquire(request).unwrap();
    let root = data.path().join("missions/m1/candidate");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "before").unwrap();
    broker.leases().bind(&lease, &root).unwrap();
    broker
        .leases()
        .patch(
            &lease,
            Some(&g),
            3,
            &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "approved".into() }),
        )
        .unwrap();
    assert_eq!(
        broker.commit(|| Ok(Some(live.lock().unwrap().clone())), &lease, "commit", "approved", &["src/a.rs".into()]),
        Err(GitError::Authority("reason.grant.revoked"))
    );
    assert!(!broker.executor().inner.calls.iter().any(|c| c.args.contains(&"commit".into()) || c.args.contains(&"commit-tree".into())));
}

#[test]
fn opus_h1_and_l4_command_config_overrides_are_pure_and_fail_closed() {
    use faber_engine::project::git::config_overrides;
    let args =
        config_overrides(b"filter.trap.process\nfilter.trap.required\nhook.lint.command\nhook.lint.event\nhook.ref.enabled\n").unwrap();
    assert_eq!(
        args,
        ["-c", "filter.trap.process=", "-c", "filter.trap.required=false", "-c", "hook.lint.enabled=false", "-c", "hook.ref.enabled=false"]
    );
    for bad in ["hook.unsafe name.command", "hook..command", "hook.x.command\nremote.x.url", "filter.bad;command.clean"] {
        assert_eq!(config_overrides(bad.as_bytes()), Err(GitError::Command));
    }
}
