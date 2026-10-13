//! Equilibrado's remote effects against the scripted git and gh: no process, no network, no login.
//! These are unit tests of the testkit library on purpose: an integration test would make `cargo test` rebuild the
//! `faber-test` binary that is running them.
//!
//! Every safety guard of `project::remote` has a test that fails when the guard is removed; see the mutation list in
//! the pull request that added them.
use super::{Delivery, FakePr, FakeRemote};
use faber_engine::project::remote::{
    Account, Call, Ci, CiState, Ledger, Lost, MergeCard, Open, OwnerYes, PrSpec, PrState, Process, RemoteError, Reply, Runner, Step,
    Target, Tool, blocking_key, ci, merge, merge_card, open_pr, preflight, publish, push, read_capped, reconcile, reply_code,
};
use faber_engine::project::{DecidedBy, Decision, GitMode, Grant, Outcome, RepoIdentity};
use std::collections::BTreeMap;
use std::path::PathBuf;

const OLD: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const OTHER: &str = "3333333333333333333333333333333333333333";
const TAG: &str = "4444444444444444444444444444444444444444";
const COMMON: &str = "/repo/.git";

fn target() -> Target {
    Target {
        cwd: PathBuf::from("/repo"),
        identity: RepoIdentity::new(COMMON, "/repo"),
        revision: 3,
        repo: "owner/repo".into(),
        remote: "origin".into(),
        url: FakeRemote::URL.into(),
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
    /// The person revokes the moment an intent is written, before its effect.
    revoke_on_intent: bool,
}
struct Entry {
    id: String,
    step: Step,
    expected: BTreeMap<String, String>,
    receipt: Option<(Outcome, BTreeMap<String, String>, Option<String>)>,
}
impl Mem {
    fn new(mode: GitMode) -> Self {
        Mem { grant: Some(grant(mode)), entries: vec![], revoke_on_intent: false }
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
        if self.revoke_on_intent {
            self.grant.as_mut().unwrap().revoked_at = Some(9);
        }
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
        cross: false,
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

/// A key of the repository's config, in the scope it was set in.
fn scoped(scope: &str, key: &str) -> FakeRemote {
    let mut fake = FakeRemote::default();
    fake.config.push((scope.into(), key.into()));
    fake
}

/// The fake, with one answer changed after the fact: `change` sees each call, the fake and the reply it just gave.
struct Tweak<F: FnMut(&Call, &mut FakeRemote, Result<Reply, Lost>) -> Result<Reply, Lost>> {
    fake: FakeRemote,
    change: F,
}
impl<F: FnMut(&Call, &mut FakeRemote, Result<Reply, Lost>) -> Result<Reply, Lost>> Runner for Tweak<F> {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost> {
        let reply = self.fake.run(call);
        (self.change)(call, &mut self.fake, reply)
    }
}

fn has(call: &Call, word: &str) -> bool {
    call.args.iter().any(|a| a == word)
}

fn said(code: i32, stdout: &str) -> Result<Reply, Lost> {
    Ok(Reply { code, stdout: stdout.into() })
}

// Authority

#[test]
fn seguro_and_missing_grants_deny_every_remote_effect() {
    let mut ledger = Mem::new(GitMode::Seguro);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(HEAD));
    let card = MergeCard { repo: "owner/repo".into(), pr: 7, head: HEAD.into(), base: "selfdev".into(), ci: Ci::none(HEAD) };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.grant.mode");
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.mode");
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.grant.mode");
    assert_eq!(key(merge_card(&mut fake, &mut ledger, &target(), 7)), "reason.grant.mode");
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, None)), "reason.grant.mode");
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.mode");
    assert_eq!(key(reconcile(&mut fake, &mut ledger, &target())), "reason.grant.mode");
    assert!(fake.calls.is_empty(), "a denied effect must not even read the remote");
    assert!(ledger.entries.is_empty());
    let mut fake = FakeRemote::default();
    let mut none = Mem { grant: None, entries: vec![], revoke_on_intent: false };
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
fn a_grant_for_another_remote_is_refused() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    ledger.grant.as_mut().unwrap().remote = Some("upstream".into());
    let mut fake = FakeRemote::default();
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.grant.noRemote");
    assert!(fake.calls.is_empty());
}

#[test]
fn a_revocation_between_the_intent_and_the_effect_stops_every_effect() {
    let mut ledger = Mem { revoke_on_intent: true, ..Mem::new(GitMode::Equilibrado) };
    let mut fake = FakeRemote::default();
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.grant.revoked");
    assert!(fake.pushes().is_empty());
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Failed))]);

    let mut ledger = Mem { revoke_on_intent: true, ..Mem::new(GitMode::Equilibrado) };
    let mut fake = pushed(Some(HEAD));
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.revoked");
    assert!(fake.gh_calls("create").is_empty());
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Failed))]);

    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(HEAD));
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    ledger.revoke_on_intent = true;
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.grant.revoked");
    assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty());
    assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Failed))]);
}

// gh and git

#[test]
fn a_missing_or_logged_out_gh_is_a_key_and_changes_nothing() {
    let mut fake = FakeRemote { gh_missing: true, ..FakeRemote::default() };
    assert_eq!(key(preflight(&mut fake, FakeRemote::URL)), "reason.remote.ghMissing");
    let mut fake = FakeRemote { logged_out: true, ..FakeRemote::default() };
    assert_eq!(key(preflight(&mut fake, FakeRemote::URL)), "reason.remote.ghLoggedOut");
    let mut fake = FakeRemote::default();
    assert_eq!(preflight(&mut fake, FakeRemote::URL).unwrap(), Account { login: "owner".into(), repo: "owner/repo".into() });
    // Publishing with no gh stops before the push: the local commit stays and no remote receipt exists.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { gh_missing: true, ..FakeRemote::default() };
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.ghMissing");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn what_gh_reports_is_checked_before_it_is_trusted() {
    for url in ["--web", "https://x/o/r .git", "file:///o/r", ""] {
        let mut fake = FakeRemote::default();
        assert_eq!(key(preflight(&mut fake, url)), "reason.remote.target", "{url:?}");
        assert!(fake.calls.is_empty());
    }
    for login in ["bad login", "", "a/b", &"a".repeat(40)] {
        let mut fake = FakeRemote { login: login.into(), ..FakeRemote::default() };
        assert_eq!(key(preflight(&mut fake, FakeRemote::URL)), "reason.git.command", "login {login:?}");
    }
    // A failing `gh api user` is not an answer, whatever it printed.
    let mut failing = Tweak {
        fake: FakeRemote::default(),
        change: |call: &Call, _: &mut FakeRemote, reply| if has(call, "user") { said(1, "owner\n") } else { reply },
    };
    assert_eq!(key(preflight(&mut failing, FakeRemote::URL)), "reason.git.command");
    // The repository is resolved from the URL, outside the checkout.
    let mut fake = FakeRemote::default();
    preflight(&mut fake, FakeRemote::URL).unwrap();
    let view = fake.calls.iter().find(|c| c.tool == Tool::Gh && has(c, "repo")).unwrap();
    assert!(has(view, FakeRemote::URL) && view.cwd != target().cwd);
    for slug in ["a/b/c", "nowhere", "../x", "x/..", "./x", "x/."] {
        let mut fake = FakeRemote { slug: slug.into(), ..FakeRemote::default() };
        assert_eq!(key(preflight(&mut fake, FakeRemote::URL)), "reason.git.command", "slug {slug:?}");
    }
}

#[test]
fn a_missing_git_is_a_key_with_no_receipt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { git_missing: true, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.git.unknown");
    assert!(ledger.entries.is_empty());
}

// Push

#[test]
fn a_push_sends_the_exact_proven_commit_to_the_granted_url_without_force() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    let done = push(&mut fake, &mut ledger, &target()).unwrap();
    assert_eq!((done.head.as_str(), done.already), (HEAD, false));
    assert_eq!(fake.remote_head.as_deref(), Some(HEAD));
    let pushes = fake.pushes();
    assert_eq!(pushes.len(), 1);
    assert!(pushes[0].args.contains(&format!("{HEAD}:refs/heads/faber/m1")));
    assert!(pushes[0].args.contains(&"--no-verify".to_string()));
    // The destination is the URL itself, so the remote name's own settings (more URLs, a mirror) cannot apply.
    assert!(pushes[0].args.contains(&FakeRemote::URL.to_string()) && !pushes[0].args.contains(&"origin".to_string()));
    assert_eq!(pushes[0].cwd, target().cwd);
    assert!(!fake.used_force());
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Done))]);
    assert_eq!(ledger.entries[0].expected["new"], HEAD);
    // Every git call runs with hooks and the pager off, and never through a shell.
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    assert!(fake.calls.iter().filter(|c| c.tool == Tool::Git).all(|c| c.args.contains(&format!("core.hooksPath={null}"))));
    for option in [
        "core.hooksPath=",
        "core.pager=",
        "core.fsmonitor=false",
        "diff.external=",
        "protocol.ext.allow=never",
        "submodule.recurse=false",
        "push.recurseSubmodules=no",
        "push.followTags=false",
        "push.gpgSign=false",
        "core.attributesFile=",
    ] {
        let git = fake.calls.iter().filter(|c| c.tool == Tool::Git);
        assert!(git.into_iter().all(|c| c.args.iter().any(|a| a.starts_with(option))), "every git call carries {option}");
    }
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
fn an_unreachable_remote_leaves_the_local_commit_and_no_receipt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { reads_down: true, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.unreachable");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn the_remote_list_is_checked_before_it_is_trusted() {
    // A line that is not a pair, an oid that is not one, a failed listing, and a good one for contrast.
    for (code, text, wanted) in [
        (0, "no tab here\n", "reason.git.command"),
        (0, "nothex\trefs/heads/faber/m1\n", "reason.git.command"),
        (128, "", "reason.remote.unreachable"),
    ] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut runner = Tweak {
            fake: FakeRemote::default(),
            change: |call: &Call, _: &mut FakeRemote, reply| if has(call, "ls-remote") { said(code, text) } else { reply },
        };
        assert_eq!(key(push(&mut runner, &mut ledger, &target())), wanted, "{text:?}");
        assert!(runner.fake.pushes().is_empty() && ledger.entries.is_empty());
    }
}

// The secret scan

#[test]
fn a_secret_in_the_outgoing_commits_blocks_the_push() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { outgoing: "+-----BEGIN PRIVATE KEY-----\n".into(), ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
}

#[test]
fn the_scan_excludes_what_the_live_remote_advertises_and_never_local_tracking_refs() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(OLD));
    fake.live_oids.push(TAG.into());
    fake.ancestors.push((OLD.into(), HEAD.into()));
    fake.outgoing = "+-----BEGIN PRIVATE KEY-----\n".into();
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
    let log = fake.git_calls("log");
    assert_eq!(log.len(), 1);
    assert!(!log[0].args.iter().any(|a| a.starts_with("--remotes") || a == "--all"), "tracking refs are forgeable");
    let after = log[0].args.iter().position(|a| a == "--not").unwrap();
    assert_eq!(log[0].args[after + 1..], [OLD, TAG], "exactly the live heads and tags");
    assert!(fake.pushes().is_empty());
}

#[test]
fn binary_files_merge_commits_and_messages_are_scanned_as_text() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { outgoing: "+-----BEGIN PRIVATE KEY-----\n".into(), ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
    let args = &fake.git_calls("log")[0].args;
    for wanted in ["--text", "-m", "--no-renames", "--ignore-missing", "--no-ext-diff", "--no-textconv", "--format=%B", "-p"] {
        assert!(args.iter().any(|a| a == wanted), "the scan must pass {wanted}");
    }
    // The ignored-missing flag must come before the revisions, or git stops at the first unknown one.
    let flag = args.iter().position(|a| a == "--ignore-missing").unwrap();
    assert!(flag < args.iter().position(|a| a == HEAD).unwrap());
    // And the proven commit is the one scanned, from the repository itself.
    assert!(args.iter().any(|a| a == HEAD));
    assert_eq!(fake.git_calls("log")[0].cwd, target().cwd);
}

#[test]
fn a_secret_spread_with_zero_bytes_is_found() {
    let spread: String = "-----BEGIN PRIVATE KEY-----".chars().flat_map(|c| [c, '\0']).collect();
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { outgoing: format!("+{spread}\n"), ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
    assert!(fake.pushes().is_empty());
}

#[test]
fn zero_bytes_cannot_hide_a_key_from_the_raw_copy_either() {
    // Without the zero byte the first AKIA is the real key; with it removed the first AKIA is a short fake that ends the line's check.
    let outgoing = "+AK\0IA junk AKIA0123456789ABCDEF\n".to_string();
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { outgoing, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.secret");
}

#[test]
fn a_scan_that_fails_or_is_too_big_fails_closed_with_its_own_key() {
    for (code, wanted) in [(1, "reason.git.command"), (128, "reason.git.command"), (125, "reason.remote.oversized")] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { log_code: code, ..FakeRemote::default() };
        assert_eq!(key(push(&mut fake, &mut ledger, &target())), wanted);
        assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
    }
}

#[test]
fn the_output_cap_keeps_the_start_and_says_more_came() {
    let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 20_000]), 16_384);
    assert!(over && kept.len() <= 16_384 && kept.len() >= 8_192);
    let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 100]), 16_384);
    assert!(!over && kept.len() == 100);
    // Exactly the cap fits; one byte more is over.
    let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 100]), 100);
    assert!(!over && kept.len() == 100);
    let (kept, over) = read_capped(&mut std::io::Cursor::new(vec![b'a'; 101]), 100);
    assert!(over && kept.len() <= 100);
    // Too much output is its own failure, even when the tool said all was well.
    assert_eq!(
        (reply_code(true, Some(0)), reply_code(false, Some(0)), reply_code(false, Some(3)), reply_code(false, None)),
        (125, 0, 3, -1)
    );
}

// Repository config and URLs

#[test]
fn config_that_runs_a_command_blocks_the_push_in_every_scope_but_the_persons() {
    for scope in ["local", "worktree", "command"] {
        for key_ in [
            "core.sshcommand",
            "credential.helper",
            "credential.https://x.example.helper",
            "http.proxy",
            "url.https://evil/.insteadof",
            "remote.origin.uploadpack",
        ] {
            let mut ledger = Mem::new(GitMode::Equilibrado);
            let mut fake = scoped(scope, key_);
            assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.config", "{scope} {key_}");
            assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
        }
    }
    // The person's own scopes are theirs: a global credential helper and a system proxy are what they want.
    for scope in ["system", "global"] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = scoped(scope, "core.sshcommand");
        push(&mut fake, &mut ledger, &target()).unwrap();
    }
}

#[test]
fn config_that_cannot_be_read_or_understood_blocks_the_push() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { config_code: 128, ..FakeRemote::default() };
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.git.command");
    let mut fake = scoped("somewhere", "core.bare");
    assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.config");
    let mut fake = FakeRemote::default();
    fake.config.push(("not a pair".into(), String::new()));
    fake.config.clear();
    struct NoTab;
    impl faber_engine::project::remote::Runner for NoTab {
        fn run(&mut self, _: &Call) -> Result<faber_engine::project::remote::Reply, faber_engine::project::remote::Lost> {
            Ok(faber_engine::project::remote::Reply { code: 0, stdout: "local core.bare\nunparsable\n".into() })
        }
    }
    assert_eq!(key(push(&mut NoTab, &mut ledger, &target())), "reason.remote.config");
    assert!(ledger.entries.is_empty() && fake.pushes().is_empty());
}

#[test]
fn which_config_keys_run_commands_or_reroute() {
    for blocked in [
        "core.sshCommand",
        "core.gitProxy",
        "core.askPass",
        "credential.helper",
        "credential.https://github.com.username",
        "http.proxy",
        "http.https://x/.extraheader",
        "url.ssh://evil/.pushInsteadOf",
        "hook.pre-push.command",
        "ssh.variant",
        "protocol.allow",
        "remote.origin.proxy",
        "remote.origin.receivepack",
        "remote.origin.uploadpack",
        "remote.origin.vcs",
        "remote.origin.mirror",
    ] {
        assert!(blocking_key(blocked), "{blocked}");
    }
    for harmless in [
        "core.bare",
        "core.filemode",
        "remote.origin.url",
        "remote.origin.fetch",
        "branch.main.remote",
        "user.name",
        "core.autocrlf",
        "remote.proxy",
    ] {
        assert!(!blocking_key(harmless), "{harmless}");
    }
}

#[test]
fn a_remote_with_more_than_the_granted_url_blocks_the_push() {
    for urls in [vec![FakeRemote::URL, "https://evil.invalid/owner/repo.git"], vec!["https://evil.invalid/owner/repo.git"], vec![]] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { push_urls: urls.into_iter().map(String::from).collect(), ..FakeRemote::default() };
        assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.url");
        assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
    }
}

#[test]
fn a_failed_url_lookup_is_not_trusted_even_if_it_printed_the_url() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut runner = Tweak {
        fake: FakeRemote::default(),
        change: |call: &Call, _: &mut FakeRemote, reply| {
            if has(call, "get-url") { said(2, &format!("{}\n", FakeRemote::URL)) } else { reply }
        },
    };
    assert_eq!(key(push(&mut runner, &mut ledger, &target())), "reason.remote.url");
    assert!(runner.fake.pushes().is_empty());
}

#[test]
fn malformed_targets_never_reach_a_process() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    let long = "a".repeat(129);
    type Edit = Box<dyn Fn(&mut Target)>;
    let edits: Vec<Edit> = vec![
        Box::new(|t| t.branch = "main".into()),
        Box::new(|t| t.branch = "faber/../main".into()),
        Box::new(|t| t.branch = "faber/x y".into()),
        Box::new(|t| t.branch = "faber/a@{b".into()),
        Box::new(|t| t.branch = "faber//x".into()),
        Box::new(|t| t.branch = "faber/.x".into()),
        Box::new(|t| t.branch = "faber/a..b".into()),
        Box::new(|t| t.branch = "faber/x.lock".into()),
        Box::new(|t| t.branch = "faber/x/".into()),
        Box::new(|t| t.branch = "faber/x.".into()),
        Box::new(move |t| t.branch = format!("faber/{long}")),
        Box::new(|t| t.base = "-x".into()),
        Box::new(|t| t.base = "/x".into()),
        Box::new(|t| t.base = ".x".into()),
        Box::new(|t| t.base = "x/".into()),
        Box::new(|t| t.base = "x.".into()),
        Box::new(|t| t.base = String::new()),
        Box::new(|t| t.base = "a:b".into()),
        Box::new(|t| t.remote = "--upload-pack=evil".into()),
        Box::new(|t| t.remote = "-x".into()),
        Box::new(|t| t.remote = ".x".into()),
        Box::new(|t| t.remote = String::new()),
        Box::new(|t| t.remote = "a".repeat(101)),
        Box::new(|t| t.head = "main".into()),
        Box::new(|t| t.repo = "not a slug".into()),
        Box::new(|t| t.repo = "a/b/c".into()),
        Box::new(|t| t.repo = "../x".into()),
        Box::new(|t| t.repo = "x/..".into()),
        Box::new(|t| t.repo = "./x".into()),
        Box::new(|t| t.repo = "x/.".into()),
        Box::new(|t| {
            t.repo = "x/..".into();
            t.url = "https://example.invalid/x/..".into();
        }),
        Box::new(|t| {
            t.repo = "./x".into();
            t.url = "https://example.invalid/./x".into();
        }),
        Box::new(|t| {
            t.repo = "a/b/c".into();
            t.url = "https://example.invalid/a/b/c".into();
        }),
        Box::new(|t| t.url = "https://example.invalid/other/repo.git".into()),
        Box::new(|t| t.url = "https://example.invalid/fork-of/owner/repo-fork.git".into()),
        Box::new(|t| t.url = "file:///x/owner/repo".into()),
        Box::new(|t| t.url = "ext::sh -c evil/owner/repo".into()),
        Box::new(|t| t.url = "-oProxyCommand=x@h:owner/repo".into()),
        Box::new(|t| t.url = "https://example.invalid/owner/repo.git ".into()),
        Box::new(|t| t.url = "https://example.invalid/owner/repo.git\n".into()),
        Box::new(|t| t.url = format!("https://example.invalid/{}/owner/repo.git", "a".repeat(600))),
        Box::new(|t| t.url = String::new()),
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut t = target();
        edit(&mut t);
        assert_eq!(key(push(&mut fake, &mut ledger, &t)), "reason.remote.target", "edit {i}");
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &t, &spec())), "reason.remote.target", "edit {i}");
    }
    assert!(fake.calls.is_empty());
    // The shapes that must keep working.
    for good in ["git@example.invalid:owner/repo.git", "ssh://git@example.invalid/owner/repo", "https://example.invalid/OWNER/Repo"] {
        let mut t = target();
        t.url = good.into();
        let mut fake = FakeRemote { push_urls: vec![good.into()], ..FakeRemote::default() };
        let mut ledger = Mem::new(GitMode::Equilibrado);
        push(&mut fake, &mut ledger, &t).unwrap_or_else(|e| panic!("{good}: {}", e.key()));
    }
}

#[test]
fn network_reads_never_run_inside_the_repository() {
    // A worker can write the repository's config, refs and attributes; reads of the remote must not apply any of it.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote::default();
    publish(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    let mut other = target();
    other.attempt = 2;
    open_pr(&mut fake, &mut ledger, &other, &spec()).unwrap();
    ci(&mut fake, &mut ledger, &target(), 1).unwrap();
    let card = merge_card(&mut fake, &mut ledger, &target(), 1).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    merge(&mut fake, &mut ledger, &target(), &card, Some(&yes)).unwrap();
    reconcile(&mut fake, &mut ledger, &target()).unwrap();
    assert!(!fake.git_calls("ls-remote").is_empty() && !fake.gh_calls("view").is_empty());
    // Only the push itself needs the repository; every gh call and every listing runs elsewhere.
    assert!(fake.calls.iter().filter(|c| c.tool == Tool::Gh).all(|c| c.cwd != target().cwd));
    for call in fake.calls.iter().filter(|c| c.tool == Tool::Gh && has(c, "pr")) {
        assert!(call.args.windows(2).any(|w| w == ["--repo", "owner/repo"]), "{:?}", call.args);
    }
    for call in fake.git_calls("ls-remote") {
        assert_ne!(call.cwd, target().cwd);
        assert!(call.args.contains(&FakeRemote::URL.to_string()), "reads go to the granted URL, not to a remote name");
    }
    let config_at = fake.calls.iter().position(|c| c.args.iter().any(|a| a == "config")).unwrap();
    let push_at = fake.calls.iter().position(|c| c.args.iter().any(|a| a == "push")).unwrap();
    assert!(config_at < push_at);
}

// Delivery that gives no clean answer

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
fn a_killed_push_is_unknown_at_once_and_only_reconcile_settles_it() {
    for (delivery, expected) in [(Delivery::TimedOutAfter, Outcome::Done), (Delivery::TimedOutBefore, Outcome::Failed)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { push: delivery, ..FakeRemote::default() };
        assert_eq!(key(push(&mut fake, &mut ledger, &target())), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Unknown))]);
        // Nothing was read after the kill: a helper it started could still land the push.
        assert!(fake.calls.last().unwrap().args.iter().any(|a| a == "push"));
        reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(ledger.outcomes(), [(Step::Push, Some(expected))]);
        assert_eq!(fake.pushes().len(), 1);
    }
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
fn git_missing_at_push_time_fails_the_receipt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut runner = Tweak {
        fake: FakeRemote::default(),
        change: |call: &Call, _: &mut FakeRemote, reply| if has(call, "push") { Err(Lost::Missing) } else { reply },
    };
    assert_eq!(key(push(&mut runner, &mut ledger, &target())), "reason.git.unknown");
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Failed))]);
}

// Pull requests

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
    assert!(creates[0].args.windows(2).any(|w| w == ["--repo", "owner/repo"]));
    // Text that starts with a dash is a value, never a flag.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    open_pr(&mut fake, &mut ledger, &target(), &PrSpec { title: "-x".into(), body: "--web".into() }).unwrap();
    let args = &fake.gh_calls("create")[0].args;
    assert!(args.contains(&"--title=-x".to_string()) && args.contains(&"--body=--web".to_string()));
    assert!(!args.contains(&"-x".to_string()) && !args.contains(&"--web".to_string()));
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
fn a_forks_pr_with_the_same_branch_name_is_not_ours() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { number: 9, cross: true, ..pr(OTHER) });
    let own = open_pr(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert_eq!((own.number, own.existing, own.cross), (10, false, false));
    assert_eq!(fake.gh_calls("create").len(), 1);
    // And a fork's PR can neither be read as ours nor merged, whatever number it is asked for.
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 9)), "reason.remote.foreign");
    assert_eq!(key(merge_card(&mut fake, &mut ledger, &target(), 9)), "reason.remote.foreign");
    let mut elsewhere = pushed(Some(HEAD));
    elsewhere.prs.push(FakePr { head_ref: "faber/other".into(), ..pr(HEAD) });
    assert_eq!(key(ci(&mut elsewhere, &mut ledger, &target(), 7)), "reason.remote.foreign");
}

#[test]
fn a_pr_of_the_branch_aimed_at_another_base_is_refused_not_adopted() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { base: "main".into(), ..pr(HEAD) });
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.headMoved");
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
    let in_title = PrSpec { title: "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB".into(), body: "y".into() };
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &in_title)), "reason.remote.secret");
    for bad in [
        PrSpec { title: String::new(), body: String::new() },
        PrSpec { title: "x".repeat(257), body: String::new() },
        PrSpec { title: "line\nbreak".into(), body: String::new() },
        PrSpec { title: "x".into(), body: "y".repeat(16_001) },
        PrSpec { title: "x".into(), body: "bell\u{7}".into() },
    ] {
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &bad)), "reason.remote.target");
    }
    assert!(fake.calls.is_empty());
    // Newlines and tabs belong in a body.
    let fine = PrSpec { title: "x".repeat(256), body: "line one\r\n\tline two\n".repeat(100) };
    open_pr(&mut fake, &mut ledger, &target(), &fine).unwrap();
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
fn a_clean_create_whose_pr_cannot_be_found_is_unknown_not_done() {
    // gh said yes, but the list shows a PR of the branch for another commit: nothing proves ours.
    struct Mismatch(FakeRemote);
    impl faber_engine::project::remote::Runner for Mismatch {
        fn run(&mut self, call: &Call) -> Result<faber_engine::project::remote::Reply, faber_engine::project::remote::Lost> {
            let reply = self.0.run(call);
            if call.args.iter().any(|a| a == "create") {
                self.0.prs.last_mut().unwrap().head_oid = OTHER.into();
            }
            reply
        }
    }
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = Mismatch(pushed(Some(HEAD)));
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.maybe");
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Unknown))]);
}

#[test]
fn a_create_is_not_proven_by_a_pr_of_the_branch_aimed_elsewhere_or_by_no_pr_at_all() {
    // The list shows our branch's PR, but into another base: nothing proves our create.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut runner = Tweak {
        fake: pushed(Some(HEAD)),
        change: |call: &Call, fake: &mut FakeRemote, reply| {
            if has(call, "create") {
                fake.prs.last_mut().unwrap().base = "main".into();
            }
            reply
        },
    };
    assert_eq!(key(open_pr(&mut runner, &mut ledger, &target(), &spec())), "reason.remote.maybe");
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Unknown))]);
    // gh said yes but the list shows nothing: that is not a failure we can prove either.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut runner = Tweak {
        fake: pushed(Some(HEAD)),
        change: |call: &Call, fake: &mut FakeRemote, reply| {
            if has(call, "create") {
                fake.prs.clear();
            }
            reply
        },
    };
    assert_eq!(key(open_pr(&mut runner, &mut ledger, &target(), &spec())), "reason.remote.maybe");
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Unknown))]);
}

#[test]
fn a_loose_list_answer_is_filtered_to_the_own_branch() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { loose_head: true, ..pushed(Some(HEAD)) };
    fake.prs.push(FakePr { head_ref: "faber/other".into(), ..pr(HEAD) });
    let own = open_pr(&mut fake, &mut ledger, &target(), &spec()).unwrap();
    assert!(!own.existing && own.branch == "faber/m1");
    assert_eq!(fake.gh_calls("create").len(), 1);
}

#[test]
fn a_killed_or_unreadable_create_stays_unknown_and_reconciliation_decides() {
    for (delivery, down, expected) in [
        (Delivery::LostAfter, true, Outcome::Done),
        (Delivery::LostBefore, true, Outcome::Failed),
        (Delivery::TimedOutAfter, false, Outcome::Done),
        (Delivery::TimedOutBefore, false, Outcome::Failed),
    ] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = FakeRemote { create: delivery, down_after_effect: down, ..pushed(Some(HEAD)) };
        assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Unknown))]);
        if !down {
            assert!(fake.calls.last().unwrap().args.iter().any(|a| a == "create"), "no read after a kill");
        }
        fake.reads_down = false;
        reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(ledger.outcomes(), [(Step::Pr, Some(expected))]);
        assert_eq!(fake.gh_calls("create").len(), 1);
    }
}

#[test]
fn gh_missing_at_create_time_fails_the_receipt() {
    struct GoneAtCreate(FakeRemote);
    impl faber_engine::project::remote::Runner for GoneAtCreate {
        fn run(&mut self, call: &Call) -> Result<faber_engine::project::remote::Reply, faber_engine::project::remote::Lost> {
            if call.args.iter().any(|a| a == "create") {
                return Err(faber_engine::project::remote::Lost::Missing);
            }
            self.0.run(call)
        }
    }
    let mut ledger = Mem::new(GitMode::Equilibrado);
    assert_eq!(key(open_pr(&mut GoneAtCreate(pushed(Some(HEAD))), &mut ledger, &target(), &spec())), "reason.remote.ghMissing");
    assert_eq!(ledger.outcomes(), [(Step::Pr, Some(Outcome::Failed))]);
}

// Accounts

#[test]
fn gh_acting_as_another_account_than_the_granted_one_is_refused() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { login: "someone-else".into(), ..FakeRemote::default() };
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.account");
    assert!(fake.pushes().is_empty() && ledger.entries.is_empty());
    fake.remote_head = Some(HEAD.into());
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.account");
    assert!(fake.gh_calls("create").is_empty() && ledger.entries.is_empty());
    // A card made while the account was right cannot be merged by another account.
    let mut right = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(HEAD));
    let card = merge_card(&mut fake, &mut right, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    fake.login = "someone-else".into();
    assert_eq!(key(merge(&mut fake, &mut right, &target(), &card, Some(&yes))), "reason.remote.account");
    assert!(fake.gh_calls("merge").is_empty() && right.entries.is_empty());
    // The same name in another case is the same GitHub account.
    fake.login = "OWNER".into();
    merge(&mut fake, &mut right, &target(), &card, Some(&yes)).unwrap();
}

#[test]
fn a_grant_that_names_no_account_acts_as_none() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    ledger.grant.as_mut().unwrap().remote_account = None;
    let mut fake = pushed(Some(HEAD));
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.noAccount");
    assert_eq!(key(open_pr(&mut fake, &mut ledger, &target(), &spec())), "reason.grant.noAccount");
}

#[test]
fn publish_refuses_a_gh_that_resolves_another_repository() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { slug: "owner/fork".into(), ..FakeRemote::default() };
    assert_eq!(key(publish(&mut fake, &mut ledger, &target(), &spec())), "reason.remote.url");
    assert!(fake.pushes().is_empty());
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

// Checks

#[test]
fn ci_is_read_as_pending_passing_failing_or_absent() {
    let cases: Vec<(Vec<serde_json::Value>, CiState)> = vec![
        (vec![], CiState::None),
        (vec![FakeRemote::check_run("COMPLETED", "SUCCESS"), FakeRemote::check_run("COMPLETED", "SKIPPED")], CiState::Passing),
        (vec![FakeRemote::check_run("COMPLETED", "NEUTRAL")], CiState::Passing),
        (vec![FakeRemote::check_run("COMPLETED", "SUCCESS"), FakeRemote::check_run("IN_PROGRESS", "")], CiState::Pending),
        (vec![FakeRemote::check_run("QUEUED", "")], CiState::Pending),
        (vec![FakeRemote::check_run("COMPLETED", "FAILURE"), FakeRemote::check_run("IN_PROGRESS", "")], CiState::Failing),
        (vec![FakeRemote::check_run("COMPLETED", "CANCELLED")], CiState::Failing),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "PENDING"})], CiState::Pending),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "EXPECTED"})], CiState::Pending),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "SUCCESS"})], CiState::Passing),
        (vec![serde_json::json!({"__typename": "StatusContext", "context": "ci/x", "state": "ERROR"})], CiState::Failing),
        (vec![serde_json::json!({"__typename": "SomethingNew", "name": "x"})], CiState::Pending),
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
    let read = ci(&mut fake, &mut ledger, &target(), 7).unwrap();
    assert_eq!((read.failed, read.total), (vec!["COMPLETED-FAILURE".to_string()], 1));
}

#[test]
fn ci_of_a_moved_head_or_an_unreadable_or_odd_pr_is_a_key() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr(OTHER));
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.remote.headMoved");
    fake.reads_down = true;
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.remote.unreachable");
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(FakePr { state: "WEIRD".into(), ..pr(HEAD) });
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.git.command");
    let mut fake = pushed(Some(HEAD));
    fake.prs.push(pr("not-an-oid"));
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 7)), "reason.git.command");
    assert_eq!(key(ci(&mut fake, &mut ledger, &target(), 99)), "reason.remote.unreachable");
}

// Merge

fn yes_for(card: &MergeCard, by: DecidedBy, answer: &str) -> Decision {
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

fn card_and_yes(fake: &mut FakeRemote, ledger: &mut Mem) -> (MergeCard, OwnerYes) {
    let card = merge_card(fake, ledger, &target(), 7).unwrap();
    let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
    (card, yes)
}

#[test]
fn a_merge_card_describes_the_pr_and_its_checks_but_merges_nothing() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    assert_eq!(
        (card.repo.as_str(), card.pr, card.head.as_str(), card.base.as_str(), card.ci.state),
        ("owner/repo", 7, HEAD, "selfdev", CiState::Passing)
    );
    assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty() && ledger.entries.is_empty());
    let mut closed = ready_pr(vec![]);
    closed.prs[0].state = "CLOSED".into();
    assert_eq!(key(merge_card(&mut closed, &mut ledger, &target(), 7)), "reason.remote.notOpen");
    // A PR retargeted to another branch, or moved to another commit, is not offered.
    let mut retargeted = ready_pr(vec![]);
    retargeted.prs[0].base = "main".into();
    assert_eq!(key(merge_card(&mut retargeted, &mut ledger, &target(), 7)), "reason.remote.headMoved");
    let mut moved = ready_pr(vec![]);
    moved.prs[0].head_oid = OTHER.into();
    assert_eq!(key(merge_card(&mut moved, &mut ledger, &target(), 7)), "reason.remote.headMoved");
}

#[test]
fn only_a_persons_yes_on_that_exact_card_becomes_an_owner_yes() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).is_some());
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Ai, "yes")).is_none(), "the AI never answers for the owner");
    assert!(OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "no")).is_none());
    assert!(OwnerYes::from_decision(&card, &card.decision()).is_none(), "unanswered");
    let mut no_time = yes_for(&card, DecidedBy::Person, "yes");
    no_time.answered_at = None;
    assert!(OwnerYes::from_decision(&card, &no_time).is_none());
    let mut other = card.clone();
    other.head = OTHER.into();
    assert!(OwnerYes::from_decision(&card, &yes_for(&other, DecidedBy::Person, "yes")).is_none(), "a yes for another head");
}

#[test]
fn a_yes_is_bound_to_the_full_head_the_base_the_number_and_the_repository() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    // A head that shares the first 12 characters with the answered one is another card with another decision.
    let sibling = MergeCard { head: format!("{}9999999999999999999999999999", &HEAD[..12]), ..card.clone() };
    assert_eq!(&sibling.head[..12], &card.head[..12]);
    assert_eq!(sibling.head.len(), 40);
    assert_ne!(sibling.decision_id(), card.decision_id());
    assert!(OwnerYes::from_decision(&sibling, &yes_for(&card, DecidedBy::Person, "yes")).is_none());
    for changed in [
        MergeCard { base: "main".into(), ..card.clone() },
        MergeCard { pr: 8, ..card.clone() },
        MergeCard { repo: "owner/other".into(), ..card.clone() },
    ] {
        assert_ne!(changed.decision_id(), card.decision_id());
        assert!(OwnerYes::from_decision(&changed, &yes_for(&card, DecidedBy::Person, "yes")).is_none());
    }
    // A decision under the right id but carrying another card's binding does not count either.
    let mut forged = yes_for(&sibling, DecidedBy::Person, "yes");
    forged.id = card.decision_id();
    assert!(OwnerYes::from_decision(&card, &forged).is_none());
    assert!(card.decision_id().starts_with("merge-7-") && card.decision_id().len() == "merge-7-".len() + 40);
}

#[test]
fn merging_without_the_owners_yes_is_refused_and_sends_nothing() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let card = merge_card(&mut fake, &mut ledger, &target(), 7).unwrap();
    let calls = fake.calls.len();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, None)), "reason.remote.needsYes");
    // A yes made for another card does not unlock this one, whichever part differs.
    for elsewhere in [
        MergeCard { pr: 8, ..card.clone() },
        MergeCard { head: OTHER.into(), ..card.clone() },
        MergeCard { base: "main".into(), ..card.clone() },
        MergeCard { repo: "owner/other".into(), ..card.clone() },
    ] {
        let stray = OwnerYes::from_decision(&elsewhere, &yes_for(&elsewhere, DecidedBy::Person, "yes")).unwrap();
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&stray))), "reason.remote.needsYes");
    }
    assert_eq!(fake.calls.len(), calls);
    assert!(ledger.entries.is_empty() && fake.prs[0].state == "OPEN" && fake.prs[0].draft);
}

#[test]
fn a_person_yes_marks_ready_and_merges_once_with_the_head_guard() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![FakeRemote::check_run("COMPLETED", "SUCCESS")]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    merge(&mut fake, &mut ledger, &target(), &card, Some(&yes)).unwrap();
    assert_eq!((fake.prs[0].state.as_str(), fake.prs[0].draft), ("MERGED", false));
    let merges = fake.gh_calls("merge");
    assert_eq!(merges.len(), 1);
    assert!(merges[0].args.windows(2).any(|w| w == ["--match-head-commit", HEAD]));
    assert!(merges[0].args.windows(2).any(|w| w == ["--repo", "owner/repo"]));
    assert!(merges[0].args.contains(&"--merge".to_string()));
    assert!(!fake.used_force(), "no force, no admin override, no branch deletion");
    assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Done))]);
}

#[test]
fn a_ready_pr_is_not_marked_ready_again_and_a_rejected_merge_may_be_retried_by_a_new_attempt() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    fake.prs[0].draft = false;
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    merge(&mut fake, &mut ledger, &target(), &card, Some(&yes)).unwrap();
    assert!(fake.gh_calls("ready").is_empty());

    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.merge = Delivery::Rejected;
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.mergeFailed");
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "error.journal.intentExists");
    fake.merge = Delivery::Ok;
    let mut again = target();
    again.attempt = 2;
    merge(&mut fake, &mut ledger, &again, &card, Some(&yes)).unwrap();
    assert_eq!(fake.prs[0].state, "MERGED");
}

#[test]
fn merge_waits_for_green_checks() {
    for checks in [vec![FakeRemote::check_run("IN_PROGRESS", "")], vec![FakeRemote::check_run("COMPLETED", "FAILURE")]] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = ready_pr(checks);
        let (card, yes) = card_and_yes(&mut fake, &mut ledger);
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.ci");
        assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty() && ledger.entries.is_empty());
    }
    // No checks at all is the person's call, and the yes was theirs.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    merge(&mut fake, &mut ledger, &target(), &card, Some(&yes)).unwrap();
}

#[test]
fn a_merge_follows_nothing_that_moved_after_the_card() {
    // The branch moved to another commit.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.prs[0].head_oid = OTHER.into();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.headMoved");
    // The PR was retargeted to another branch.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.prs[0].base = "main".into();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.headMoved");
    // It was closed.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.prs[0].state = "CLOSED".into();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.notOpen");
    // It was swapped for a fork's.
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.prs[0].cross = true;
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.foreign");
    assert!(fake.gh_calls("merge").is_empty() && fake.gh_calls("ready").is_empty() && ledger.entries.is_empty());
}

#[test]
fn a_card_for_another_commit_base_or_repository_than_the_target_is_refused_even_if_the_pr_agrees() {
    // The live PR and the card agree with each other, but not with what Faber proved and was granted.
    for (live_head, card_edit) in [(OTHER, 0), (HEAD, 1), (HEAD, 2)] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = ready_pr(vec![]);
        let (card, _) = card_and_yes(&mut fake, &mut ledger);
        let card = match card_edit {
            0 => MergeCard { head: OTHER.into(), ..card },
            1 => MergeCard { base: "main".into(), ..card },
            _ => MergeCard { repo: "owner/other".into(), ..card },
        };
        fake.prs[0].head_oid = live_head.into();
        if card_edit == 0 {
            fake.prs[0].head_oid = OTHER.into();
        }
        if card_edit == 1 {
            fake.prs[0].base = "main".into();
        }
        let yes = OwnerYes::from_decision(&card, &yes_for(&card, DecidedBy::Person, "yes")).unwrap();
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.headMoved", "edit {card_edit}");
        assert!(fake.gh_calls("merge").is_empty() && ledger.entries.is_empty());
    }
}

#[test]
fn a_merge_waits_for_every_earlier_remote_effect_to_settle() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    ledger.intent("push-222222222222-1", Step::Push, BTreeMap::new()).unwrap();
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.pending");
    assert!(fake.gh_calls("merge").is_empty());
}

#[test]
fn a_revoked_grant_stops_a_merge_that_already_has_a_yes() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    ledger.grant.as_mut().unwrap().revoked_at = Some(9);
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.grant.revoked");
    assert!(fake.gh_calls("merge").is_empty());
}

#[test]
fn a_merge_whose_answer_is_lost_or_killed_is_settled_by_reading_the_pr_state() {
    for (delivery, down, expected) in [
        (Delivery::LostAfter, true, Outcome::Done),
        (Delivery::LostBefore, true, Outcome::Failed),
        (Delivery::TimedOutAfter, false, Outcome::Done),
        (Delivery::TimedOutBefore, false, Outcome::Failed),
    ] {
        let mut ledger = Mem::new(GitMode::Equilibrado);
        let mut fake = ready_pr(vec![]);
        let (card, yes) = card_and_yes(&mut fake, &mut ledger);
        fake.merge = delivery;
        fake.down_after_effect = down;
        assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.maybe");
        assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Unknown))]);
        fake.reads_down = false;
        reconcile(&mut fake, &mut ledger, &target()).unwrap();
        assert_eq!(ledger.outcomes(), [(Step::Merge, Some(expected))]);
        assert_eq!(fake.gh_calls("merge").len(), 1);
    }
}

#[test]
fn a_merge_the_remote_refuses_fails_cleanly() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = ready_pr(vec![]);
    let (card, yes) = card_and_yes(&mut fake, &mut ledger);
    fake.merge = Delivery::Rejected;
    assert_eq!(key(merge(&mut fake, &mut ledger, &target(), &card, Some(&yes))), "reason.remote.mergeFailed");
    assert_eq!(ledger.outcomes(), [(Step::Merge, Some(Outcome::Failed))]);
}

// Reconciliation

#[test]
fn reconciliation_reads_only_and_only_under_a_valid_grant() {
    let mut ledger = Mem::new(GitMode::Equilibrado);
    let mut fake = FakeRemote { push: Delivery::TimedOutAfter, ..FakeRemote::default() };
    push(&mut fake, &mut ledger, &target()).unwrap_err();
    let calls = fake.calls.len();
    ledger.grant.as_mut().unwrap().revoked_at = Some(9);
    assert_eq!(key(reconcile(&mut fake, &mut ledger, &target())), "reason.grant.revoked");
    assert_eq!(fake.calls.len(), calls, "a revoked grant reads nothing");
    assert_eq!(ledger.outcomes(), [(Step::Push, Some(Outcome::Unknown))]);
    ledger.grant.as_mut().unwrap().revoked_at = None;
    ledger.grant.as_mut().unwrap().mode = GitMode::Seguro;
    assert_eq!(key(reconcile(&mut fake, &mut ledger, &target())), "reason.grant.mode");
    assert_eq!(fake.calls.len(), calls);
    assert!(fake.calls.iter().all(|c| !c.args.iter().any(|a| a == "create" || a == "merge")));
    ledger.grant.as_mut().unwrap().mode = GitMode::Equilibrado;
    let mut bad = target();
    bad.branch = "main".into();
    assert_eq!(key(reconcile(&mut fake, &mut ledger, &bad)), "reason.remote.target");
    assert_eq!(fake.calls.len(), calls);
}

// The real tools

#[test]
fn the_real_process_is_shell_free_hidden_and_never_prompts() {
    let process = Process::at("C:/tools/git.exe".into(), "C:/tools/gh.exe".into());
    let call = Call { tool: Tool::Gh, cwd: PathBuf::from("/work/repo"), args: vec!["pr".into(), "view".into(), "7".into()] };
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
    // git looks at the folder it runs in and never above it.
    assert_eq!(envs.get("GIT_CEILING_DIRECTORIES"), Some(&Some(PathBuf::from("/work").to_string_lossy().into_owned())));
    // The person's own credential setup is neither replaced nor cleared.
    assert!(
        !envs.contains_key("GIT_CONFIG_GLOBAL")
            && !envs.contains_key("GIT_CONFIG_NOSYSTEM")
            && !envs.contains_key("GH_TOKEN")
            && !envs.contains_key("GIT_SSH_COMMAND")
    );
    let git = process.command(&Call { tool: Tool::Git, cwd: PathBuf::from("/work/repo"), args: vec!["status".into()] });
    assert_eq!(git.get_program(), "C:/tools/git.exe");
}
