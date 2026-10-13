//! Equilibrado's remote effects against the scripted git and gh: no process, no network, no login.
//! These are unit tests of the testkit library on purpose: an integration test would make `cargo test` rebuild the
//! `faber-test` binary that is running them.
use super::{Delivery, FakePr, FakeRemote};
use faber_engine::project::remote::{
    Account, Call, Ci, CiState, Ledger, Open, OwnerYes, PrSpec, PrState, Process, RemoteError, Step, Target, ci, merge, merge_card,
    open_pr, preflight, publish, push, reconcile,
};
use faber_engine::project::{DecidedBy, Decision, GitMode, Grant, Outcome, RepoIdentity};
use std::collections::BTreeMap;
use std::path::PathBuf;

const OLD: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const OTHER: &str = "3333333333333333333333333333333333333333";
const COMMON: &str = "/repo/.git";

fn target() -> Target {
    Target {
        cwd: PathBuf::from("/repo"),
        identity: RepoIdentity::new(COMMON, "/repo"),
        revision: 3,
        repo: "owner/repo".into(),
        remote: "origin".into(),
        url: "https://example.invalid/owner/repo.git".into(),
        branch: "faber/m1".into(),
        head: HEAD.into(),
        base: "selfdev".into(),
        attempt: 1,
    }
}

fn grant(mode: GitMode) -> Grant {
    let mut grant = Grant::new(COMMON, mode, 3, 1);
    grant.remote = Some("origin".into());
    grant.remote_account = Some("owner".into());
    grant
}

/// The journal's rules, in memory: an intent exists once, a receipt is final unless it is `Unknown`.
struct Mem {
    grant: Option<Grant>,
    entries: Vec<Entry>,
}
struct Entry {
    id: String,
    step: Step,
    expected: BTreeMap<String, String>,
    receipt: Option<(Outcome, BTreeMap<String, String>, Option<String>)>,
}
impl Mem {
    fn new(mode: GitMode) -> Self {
        Mem { grant: Some(grant(mode)), entries: vec![] }
    }
    fn outcomes(&self) -> Vec<(Step, Option<Outcome>)> {
        self.entries.iter().map(|e| (e.step, e.receipt.as_ref().map(|r| r.0))).collect()
    }
}
impl Ledger for Mem {
    fn grant(&mut self) -> Result<Option<Grant>, RemoteError> {
        Ok(self.grant.clone())
    }
    fn open(&mut self) -> Result<Vec<Open>, RemoteError> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.receipt.as_ref().is_none_or(|r| r.0 == Outcome::Unknown))
            .map(|e| Open { id: e.id.clone(), step: e.step, expected: e.expected.clone() })
            .collect())
    }
    fn intent(&mut self, id: &str, step: Step, expected: BTreeMap<String, String>) -> Result<(), RemoteError> {
        if self.entries.iter().any(|e| e.id == id) {
            return Err(RemoteError::Ledger("error.journal.intentExists"));
        }
        self.entries.push(Entry { id: id.into(), step, expected, receipt: None });
        Ok(())
    }
    fn receipt(&mut self, id: &str, outcome: Outcome, evidence: BTreeMap<String, String>, reason: Option<&str>) -> Result<(), RemoteError> {
        let entry = self.entries.iter_mut().find(|e| e.id == id).ok_or(RemoteError::Ledger("error.journal.noIntent"))?;
        if entry.receipt.is_some() {
            return Err(RemoteError::Ledger("error.journal.receiptExists"));
        }
        entry.receipt = Some((outcome, evidence, reason.map(Into::into)));
        Ok(())
    }
    fn settle(&mut self, id: &str, outcome: Outcome, evidence: BTreeMap<String, String>) -> Result<(), RemoteError> {
        let entry = self.entries.iter_mut().find(|e| e.id == id).ok_or(RemoteError::Ledger("error.journal.noIntent"))?;
        if entry.receipt.as_ref().is_some_and(|r| r.0 != Outcome::Unknown) || outcome == Outcome::Unknown {
            return Err(RemoteError::Ledger("error.journal.receiptExists"));
        }
        entry.receipt = Some((outcome, evidence, None));
        Ok(())
    }
}

fn key<T: std::fmt::Debug>(result: Result<T, RemoteError>) -> &'static str {
    result.expect_err("expected a refusal").key()
}

fn pr(oid: &str) -> FakePr {
    FakePr {
        number: 7,
        head_ref: "faber/m1".into(),
        head_oid: oid.into(),
        base: "selfdev".into(),
        draft: true,
        state: "OPEN".into(),
        checks: vec![],
    }
}

fn spec() -> PrSpec {
    PrSpec { title: "feat: add the remote module".into(), body: "Proven locally.\n".into() }
}

fn pushed(remote_head: Option<&str>) -> FakeRemote {
    FakeRemote { remote_head: remote_head.map(Into::into), ..FakeRemote::default() }
}

#[test]
fn seguro_and_missing_grants_deny_every_remote_effect() {
    let mut ledger = Mem::new(GitMode::Seguro);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(HEAD));
    let card = faber_engine::project::remote::MergeCard { pr: 7, head: HEAD.into(), base: "selfdev".into(), ci: Ci::none(HEAD) };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.grant.mode");
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.mode");
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.grant.mode");
    assert_eq!(key(merge_card(&mut fake, &mut ledger, &target(), 7)), "reason.grant.mode");
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, None)), "reason.grant.mode");
    assert!(fake.calls.is_empty(), "a denied effect must not even read the remote");
    assert!(ledger.entries.is_empty());
    let mut fake = FakeRemote::default();
    let mut none = Mem { grant: None, entries: vec![] };
    assert_eq!(key(push(&mut fake, &mut none, &target())), "reason.grant.none");
    let mut revoked = Mem::new(GitMode::Equilibrado);
    revoked.grant.as_mut().unwrap().revoked_at = Some(9);
    assert_eq!(key(push(&mut fake, &mut revoked, &target())), "reason.grant.revoked");
    let mut stale = Mem::new(GitMode::Equilibrado);
    stale.grant.as_mut().unwrap().revision = 4;
    assert_eq!(key(push(&mut fake, &mut stale, &target())), "reason.grant.stale");
    assert!(fake.calls.is_empty());
}

#[test]
fn a_missing_or_logged_out_gh_is_a_key_and_changes_nothing() {
    let mut fake = FakeRemote { gh_missing: true, ..FakeRemote::default() };
    assert_eq!(key(preflight(&mut fake, &target().cwd)), "reason.remote.ghMissing");
    let mut fake = FakeRemote { logged_out: true, ..FakeRemote::default() };
    assert_eq!(key(preflight(&mut fake, &target().cwd)), "reason.remote.ghLoggedOut");
    let mut fake = FakeRemote::default();
    assert_eq!(preflight(&mut fake, &target().cwd).unwrap(), Account { login: "owner".into(), repo: "owner/repo".into() });
    // Publishing with no gh stops before the push: the local commit stays and no remote receipt exists.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { gh_missing: true, ..FakeRemote::default() };
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.ghMissing");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn a_missing_git_is_a_key_with_no_receipt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { git_missing: true, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.git.unknown");
    assert!(ledger.entries.is_empty());
}

#[test]
fn a_push_sends_the_exact_proven_commit_without_force() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    let done = push(&mut fake, &mut ledger, &target()).unwrap();
    assert_eq!((done.head.as_str(), done.already), (HEAD, false));
    assert_eq!(fake.remote_head.as_deref(), Some(HEAD));
    let pushes = fake.pushes();
    assert_eq!(pushes.len(), 1);
    assert!(pushes[0].args.contains(&format!("{HEAD}:refs/heads/faber/m1")));
    assert!(pushes[0].args.contains(&"--no-verify".to_string()));
    assert!(!fake.used_force());
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Done))]);
    assert_eq!(ledger.entries[0].expected["new"], HEAD);
    // Every git call runs with hooks and the pager off, and never through a shell.
    assert!(fake.calls.iter().all(|c| c.args.iter().any(|a| a.starts_with("core.hooksPath="))));
}

#[test]
fn a_branch_already_on_the_remote_sends_nothing() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    assert!(push(&mut fake, &mut ledger, &target()).unwrap().already);
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn a_fast_forward_updates_the_branch_and_records_the_old_head() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(OLD));
    fake.ancestors.push((OLD.into(), HEAD.into()));
    push(&mut fake, &mut ledger, &target()).unwrap();
    assert_eq!(ledger.entries[0].expected["old"], OLD);
    assert_eq!(fake.remote_head.as_deref(), Some(HEAD));
}

#[test]
fn divergence_blocks_without_sending_or_recording() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(OTHER));
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.diverged");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
    assert_eq!(fake.remote_head.as_deref(), Some(OTHER));
}

#[test]
fn a_secret_in_the_outgoing_commits_blocks_the_push() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { outgoing: "+-----BEGIN PRIVATE KEY-----\n".into(), ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
    // Nothing else went to the network either.
    assert!(fake.calls.iter().all(|c| !c.args.iter().any(|a| a == "ls-remote")));
}

#[test]
fn repository_config_that_runs_commands_and_a_redirected_url_block_the_push() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { local_config: vec!["core.sshcommand".into()], ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.config");
    let mut fake = FakeRemote { url: "https://elsewhere.invalid/x.git".into(), ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.url");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn malformed_targets_never_reach_a_process() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    for edit in [
        (|t: &mut Target| t.branch = "main".into()) as fn(&mut Target),
        |t| t.branch = "faber/../main".into(),
        |t| t.branch = "faber/x y".into(),
        |t| t.remote = "--upload-pack=evil".into(),
        |t| t.head = "main".into(),
        |t| t.repo = "not a slug".into(),
        |t| t.base = "-x".into(),
    ] {
        let mut t = target();
        edit(&mut t);
        assert_eq!(key(push(&mut fake, &mut ledger, &t)), "reason.remote.target");
    }
    assert!(fake.calls.is_empty());
}

#[test]
fn an_unreachable_remote_leaves_the_local_commit_and_no_receipt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { reads_down: true, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.unreachable");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn a_rejected_push_fails_cleanly_and_only_a_new_attempt_may_retry() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { push: Delivery::Rejected, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.rejected");
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Failed))]);
    // The same attempt id is refused by the journal: an effect is never replayed under its old intent.
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "error.journal.intentExists");
    assert_eq!(fake.pushes().len(), 1);
    fake.push = Delivery::Ok;
    let mut second = target();
    second.attempt = 2;
    assert!(!push(&mut fake, &mut ledger, &second).unwrap().already);
    assert_eq!(fake.pushes().len(), 2);
}

#[test]
fn a_link_lost_after_the_push_is_settled_by_reading_the_remote_never_by_repeating() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { push: Delivery::LostAfter, ..FakeRemote::default() };
    let done = push(&mut fake, &mut ledger, &target()).unwrap();
    assert_eq!(done.head, HEAD);
    assert_eq!(fake.pushes().len(), 1);
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Done))]);
    assert_eq!(ledger.entries[0].receipt.as_ref().unwrap().1["via"], "reconcile");
}

#[test]
fn a_link_lost_before_the_push_is_settled_as_failed() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { push: Delivery::LostBefore, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.rejected");
    assert_eq!(fake.pushes().len(), 1);
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Failed))]);
}

#[test]
fn an_unreadable_remote_after_a_lost_push_stays_unknown_until_reconciled() {
    for (delivery, expected) in [(Delivery::LostAfter, Outcome::Done), (Delivery::LostBefore, Outcome::Failed)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { push: delivery, down_after_effect: true, ..FakeRemote::default() };
        assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Unknown))]);
        // While it is uncertain nothing else may leave: not another push, not a PR, and nothing is even read.
        let calls = fake.calls.len();
        let mut next = target();
        next.attempt = 2;
        assert_eq!(key(push(&mut fake, &mut ledger, &next)), "reason.remote.pending");
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &next, &spec())), "reason.remote.pending");
        assert_eq!(fake.calls.len(), calls);
        // Still down: reconciliation leaves it unknown instead of guessing.
        assert_eq!(reconcile(&mut fake, &mut ledger, &target()).unwrap(), [(ledger.entries[0].id.clone(), None)]);
        assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Unknown))]);
        fake.reads_down = false;
        let settled = reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(settled, [(ledger.entries[0].id.clone(), Some(expected))]);
        assert_eq!(ledger.outcomes(), [(Step::Push, Some(expected))]);
        assert_eq!(fake.pushes().len(), 1, "an uncertain push is never replayed");
        assert!(reconcile(&mut fake, &mut ledger, &target()).unwrap().is_empty());
    }
}

#[test]
fn a_crash_between_intent_and_receipt_is_settled_the_same_way() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let expected = BTreeMap::from([("old".to_string(), String::new()), ("new".to_string(), HEAD.to_string())]);
    ledger.intent("push-222222222222-1", Step::Push, expected).unwrap();
    let mut fake = pushed(Some(HEAD));
    assert_eq!(reconcile(&mut fake, &mut ledger, &target()).unwrap(), [("push-222222222222-1".to_string(), Some(Outcome::Done))]);
    assert!(fake.pushes().is_empty());
}

#[test]
fn a_draft_pr_is_opened_once_for_the_own_branch() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    let first = open_pr(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert_eq!((first.number, first.draft, first.existing, first.state), (1, true, false, PrState::Open));
    assert_eq!((first.head.as_str(), first.base.as_str()), (HEAD, "selfdev"));
    let creates = fake.gh_calls("create");
    assert_eq!(creates.len(), 1);
    assert!(creates[0].args.contains(&"--draft".to_string()));
    assert!(creates[0].args.iter().any(|a| a == "faber/m1") && creates[0].args.iter().any(|a| a == "selfdev"));
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Done))]);
    assert_eq!(ledger.entries[0].receipt.as_ref().unwrap().1["number"], "1");
    // A second call, even with a new attempt, finds the PR instead of creating another.
    let mut again = target();
    again.attempt = 2;
    let second = open_pr(&mut fake, &mut ledger, &again, &spec()).unwrap();
    assert_eq!((second.number, second.existing), (1, true));
    assert_eq!(fake.gh_calls("create").len(), 1);
    assert_eq!(ledger.entries.len(), 1);
}

#[test]
fn a_closed_or_merged_pr_of_the_branch_is_not_replaced_by_a_new_one() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { state: "MERGED".into(), ..pr(HEAD) });
    let found = open_pr(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert_eq!((found.existing, found.state), (true, PrState::Merged));
    assert!(fake.gh_calls("create").is_empty());
}

#[test]
fn a_pr_needs_the_proven_commit_on_the_remote_first() {
    for remote_head in [None, Some(OLD)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = pushed(remote_head);
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.notPushed");
        assert!(fake.gh_calls("create").is_empty() && ledger.entries.is_empty());
    }
}

#[test]
fn a_secret_or_oversized_text_never_reaches_gh() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    let secret = PrSpec { title: "x".into(), body: "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB".into() };
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &secret)), "reason.remote.secret");
    for bad in [PrSpec { title: String::new(), body: String::new() }, PrSpec { title: "x".into(), body: "y".repeat(20_000) }] {
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &bad)), "reason.remote.target");
    }
    assert!(fake.calls.is_empty());
}

#[test]
fn a_lost_create_is_found_by_reading_back_and_never_duplicated() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { create: Delivery::LostAfter, ..pushed(Some(HEAD)) };
    let found = open_pr(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert_eq!(found.number, 1);
    assert_eq!(fake.gh_calls("create").len(), 1);
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Done))]);

    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { create: Delivery::LostBefore, ..pushed(Some(HEAD)) };
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.prFailed");
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Failed))]);
}

#[test]
fn an_unreadable_create_stays_unknown_and_reconciliation_decides() {
    for (delivery, expected) in [(Delivery::LostAfter, Outcome::Done), (Delivery::LostBefore, Outcome::Failed)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { create: delivery, ..pushed(Some(HEAD)) };
        fake.down_after_effect = true;
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Unknown))]);
        fake.reads_down = false;
        reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(ledger.outcomes(), [(Step::Pr, Some(expected))]);
        assert_eq!(fake.gh_calls("create").len(), 1);
    }
}

#[test]
fn ci_is_read_as_pending_passing_failing_or_absent() {
    let cases: Vec<(Vec<serde_json::Value>, CiState)> = vec![
        (vec![], CiState::None),
        (vec![FakeRemote::check_run("COMPLETED", "SUCCESS"), FakeRemote::check_run("COMPLETED", "SKIPPED")], CiState::Passing),
        (vec![FakeRemote::check_run("COMPLETED", "SUCCESS"), FakeRemote::check_run("IN_PROGRESS", "")], CiState::Pending),
        (vec![FakeRemote::check_run("QUEUED", "")], CiState::Pending),
        (vec![FakeRemote::check_run("COMPLETED", "FAILURE"), FakeRemote::check_run("IN_PROGRESS", "")], CiState::Failing),
        (vec![FakeRemote::check_run("COMPLETED", "CANCELLED")], CiState::Failing),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "PENDING"})], CiState::Pending),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "SUCCESS"})], CiState::Passing),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "ERROR"})], CiState::Failing),
    ];
    for (checks, expected) in cases {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = pushed(Some(HEAD));
        fake.prs.push(FakePr { checks, ..pr(HEAD) });
        let read = ci(&mut fake, &mut ledger, &target(), 7).unwrap();
        assert_eq!(read.state, expected);
        assert_eq!(read.head, HEAD);
        assert!(ledger.entries.is_empty(), "reading is not an effect");
    }
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { checks: vec![FakeRemote::check_run("COMPLETED", "FAILURE")], ..pr(HEAD) });
    assert_eq!(ci(&mut fake, &mut ledger, &target(), 7).unwrap().failed, ["COMPLETED-FAILURE"]);
}

#[test]
fn ci_of_a_moved_head_or_an_unreadable_pr_is_a_key() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(OTHER));
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.remote.headMoved");
    fake.reads_down = true;
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.remote.unreachable");
}

fn yes_for(card: &faber_engine::project::remote::MergeCard, by: DecidedBy, answer: &str) -> Decision {
    let mut decision = card.decision();
    if by == DecidedBy::Person {
        decision.answer(by, answer, 5).unwrap();
    } else {
        // The API refuses an AI answer to a direction decision; a record written some other way must still not count.
        decision.answer = Some(answer.into());
        decision.answered_by = Some(by);
        decision.answered_at = Some(5);
    }
    decision
}

fn ready_pr(checks: Vec<serde_json::Value>) -> FakeRemote {
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { checks, ..pr(HEAD) });
    fake
}

#[test]
fn a_merge_card_describes_the_pr_and_its_checks_but_merges_nothing() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    assert_eq!((card.pr, card.head.as_str(), card.base.as_str(), card.ci.state), (7, HEAD, "selfdev", CiState::Passing));
    assert_eq!(card.decision_id(), "merge-7-222222222222");
    assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty() && ledger.entries.is_empty());
    let mut closed = ready_pr(vec![]);
    closed.prs[0].state = "CLOSED".into();
    assert_eq!(key(merge_card(&mut closed, &mut ledger, &target(), 7)), "reason.remote.notOpen");
}

#[test]
fn only_a_persons_yes_on_that_exact_card_becomes_an_owner_yes() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).is_some());
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Ai, "yes")).is_none(), "the AI never answers for the owner");
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "no")).is_none());
    assert!(OwnerYes::from_decision(&card, &card.decision()).is_none());
    let mut other = card.clone();
    other.head = OTHER.into();
    assert!(OwnerYes::from_decision(&card, &yes_for(&other, DecidedBy::Person, "yes")).is_none(), "a yes for another head");
}

#[test]
fn merging_without_the_owners_yes_is_refused_and_sends_nothing() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let calls = fake.calls.len();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, None)), "reason.remote.needsYes");
    // A yes made for another card does not unlock this one.
    let mut elsewhere = card.clone();
    elsewhere.pr = 8;
    let stray = OwnerYes::from_decision(&elsewhere, &yes_for(&elsewhere, DecidedBy::Person, "yes")).unwrap();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&stray))), "reason.remote.needsYes");
    assert_eq!(fake.calls.len(), calls);
    assert!(ledger.entries.is_empty() && fake.prs[0].state == "OPEN" && fake.prs[0].draft);
}

#[test]
fn a_person_yes_marks_ready_and_merges_once_with_the_head_guard() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    merge(&mut fake, &mut ledger, &target(), &card, Some(&yes)).unwrap();
    assert_eq!((fake.prs[0].state.as_str(), fake.prs[0].draft), ("MERGED", false));
    let merges = fake.gh_calls("merge");
    assert_eq!(merges.len(), 1);
    assert!(merges[0].args.windows(2).any(|w| w == ["--match-head-commit", HEAD]));
    assert!(merges[0].args.contains(&"--merge".to_string()));
    assert!(!fake.used_force(), "no force, no admin override, no branch deletion");
    assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Done))]);
}

#[test]
fn merge_waits_for_green_checks_and_an_unmoved_head() {
    for (checks, key_) in [
        (vec![FakeRemote::check_run("IN_PROGRESS", "")], "reason.remote.ci"),
        (vec![FakeRemote::check_run("COMPLETED", "FAILURE")], "reason.remote.ci"),
    ] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = ready_pr(checks);
        let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
        let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), key_);
        assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty() && ledger.entries.is_empty());
    }
    // The branch moved after the card was shown: the yes belongs to the old commit.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    fake.prs[0].head_oid = OTHER.into();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.headMoved");
    assert!(fake.gh_calls("merge").is_empty() && ledger.entries.is_empty());
}

#[test]
fn a_revoked_grant_stops_a_merge_that_already_has_a_yes() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    ledger.grant.as_mut().unwrap().revoked_at = Some(9);
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.grant.revoked");
    assert!(fake.gh_calls("merge").is_empty());
}

#[test]
fn a_merge_whose_answer_is_lost_is_settled_by_reading_the_pr_state() {
    for (delivery, expected) in [(Delivery::LostAfter, Outcome::Done), (Delivery::LostBefore, Outcome::Failed)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = ready_pr(vec![]);
        let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
        let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
        fake.merge = delivery;
        fake.down_after_effect = true;
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Unknown))]);
        fake.reads_down = false;
        reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(ledger.outcomes(), [(Step::Merge, Some(expected))]);
        assert_eq!(fake.gh_calls("merge").len(), 1);
    }
}

#[test]
fn publish_runs_preflight_push_and_one_draft_pr() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    let out = publish(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert_eq!((out.pushed.head.as_str(), out.pr.number, out.pr.draft), (HEAD, 1, true));
    assert_eq!(out.account.login, "owner");
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Done)), (Step::Pr, Some(Outcome::Done))]);
    assert!(!fake.used_force());
}

#[test]
fn the_real_process_is_shell_free_hidden_and_never_prompts() {
    let process = Process::at("C:/tools/git.exe".into(), "C:/tools/gh.exe".into());
    let call =
        Call { tool: faber_engine::project::remote::Tool::Gh, cwd: PathBuf::from("."), args: vec!["pr".into(), "view".into(), "7".into()] };
    let command = process.command(&call);
    assert_eq!(command.get_program(), "C:/tools/gh.exe");
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["pr", "view", "7"]);
    let envs: BTreeMap<_, _> =
        command.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
    for (name, value) in
        [("GIT_TERMINAL_PROMPT", "0"), ("GCM_INTERACTIVE", "never"), ("GH_PROMPT_DISABLED", "1"), ("SSH_ASKPASS_REQUIRE", "never")]
    {
        assert_eq!(envs.get(name), Some(&Some(value.to_string())), "{name}");
    }
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"] {
        assert_eq!(envs.get(name), Some(&None), "{name} must be removed");
    }
    // The person's own credential setup is neither replaced nor cleared.
    assert!(
        !envs.contains_key("GIT_CONFIG_GLOBAL")
            && !envs.contains_key("GIT_CONFIG_NOSYSTEM")
            && !envs.contains_key("GH_TOKEN")
            && !envs.contains_key("GIT_SSH_COMMAND")
    );
}
