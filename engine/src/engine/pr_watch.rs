//! The engine side of `follow.rs` (R200): a project conversation whose agent opened, queued or pushed a pull request
//! follows it. One timer per followed pull request reads it with gh (a short-lived hidden process, never under the lock),
//! first after 90 s, then up to 10 minutes apart while nothing changes; nothing runs for a conversation that follows none.
//! A failure or a conflict continues the conversation once per commit, through `send_by_itself`, when nothing runs there and
//! the quota allows it, and otherwise needs the person.
use super::*;
use crate::follow::{self, Found, Kind, Next};
use crate::project::remote::{self, Look};

/// Reads one pull request: `owner/name` and its number. Errors are `reason.*` keys.
pub type PrReader = Arc<dyn Fn(&str, u64) -> Result<Look, &'static str> + Send + Sync>;

/// The person's own gh, shell-free and hidden, run from a folder of Faber's own (see `remote::Runner::neutral`).
pub fn gh_reader(data_dir: &Path) -> PrReader {
    let neutral = data_dir.join("neutral");
    Arc::new(move |repo, number| remote::look(&mut remote::Process::find(neutral.clone()), repo, number).map_err(|e| e.key()))
}

/// One followed pull request's timer.
pub(super) struct Watch {
    ticker: u64,
    delay: Duration,
    fails: u32,
    /// A push was seen and no reading has shown a new commit since.
    pushed: bool,
}

/// A window this full leaves the rest of it to the person.
const NEAR: f64 = 0.9;

/// Whether a reading answers a declared wait: an end always does, a green result only when newer than the wait's baseline.
/// A wait on conversations returns on their reports instead.
fn answers(w: &WaitingOn, number: u64, look: &Look) -> bool {
    if !w.ids.is_empty() {
        return false;
    }
    match look.state {
        remote::PrState::Merged | remote::PrState::Closed => true,
        remote::PrState::Open if look.ci.state == remote::CiState::Passing && !look.conflict => match &w.pr {
            Some(b) if b.number == number => b.head.as_deref() != Some(&look.head) || (!b.pushed && b.state != FollowState::Passing),
            _ => true,
        },
        remote::PrState::Open => false,
    }
}

/// The errors after which looking again cannot help.
fn hopeless(key: &str) -> bool {
    matches!(key, "reason.remote.ghMissing" | "reason.remote.ghLoggedOut" | "reason.remote.target")
}

impl Core {
    /// The followed pull request as last read, for a wait starting now.
    pub(super) fn wait_pr(&self, conv: &str) -> Option<WaitPr> {
        let f = self.convs.get(conv)?.messages.get(self.followed(conv)?)?.follow.as_ref()?;
        let pushed = self.follows.get(conv).is_some_and(|w| w.pushed);
        Some(WaitPr { number: f.number, head: f.head.clone(), state: f.state, pushed })
    }

    pub(super) fn refresh_wait_pr(&mut self, conv: &str) {
        if self.followed(conv).and_then(|at| self.convs.get(conv)?.messages[at].follow.as_ref()).is_some_and(|f| f.state.watched()) {
            // A new wait invalidates even a manual read already in flight, without requiring a recognized push.
            self.look_later(conv, follow::FIRST);
        }
        self.wait_on_ended_pr(conv);
    }

    pub(super) fn restore_wait_pr(&mut self, conv: &str) {
        // A wait saved before baselines existed starts from what was read before the restart, never from an old green.
        if let Some(pr) = self.convs.get(conv).filter(|c| c.waiting_on.as_ref().is_some_and(|w| w.pr.is_none())).and(self.wait_pr(conv)) {
            self.convs.get_mut(conv).and_then(|c| c.waiting_on.as_mut()).expect("checked above").pr = Some(pr);
            self.save(conv);
        }
        if !self.follows.contains_key(conv)
            && self.followed(conv).and_then(|at| self.convs.get(conv)?.messages[at].follow.as_ref()).is_some_and(|f| f.state.watched())
        {
            self.look_later(conv, follow::FIRST);
        }
        self.wait_on_ended_pr(conv);
    }

    /// No reading comes after a merge or close, so a wait declared later returns on it at once, once per pull request end.
    fn wait_on_ended_pr(&mut self, conv: &str) {
        let Some(at) = self.followed(conv) else { return };
        let Some(c) = self.convs.get_mut(conv) else { return };
        if !c.waiting_on.as_ref().is_some_and(|w| w.ids.is_empty()) {
            return;
        }
        let Some(f) = c.messages[at].follow.as_mut().filter(|f| matches!(f.state, FollowState::Merged | FollowState::Closed)) else {
            return;
        };
        let key = format!("wait-end-{:?}-{}", f.state, f.head.as_deref().unwrap_or_default());
        if f.woke.as_deref() == Some(&key) {
            return;
        }
        (f.woke, f.kept) = (Some(key), true);
        let cause = format!(
            "Pull request #{} ({}) is already {:?}. Read the actual result and continue your saved work.",
            f.number, f.repo, f.state
        );
        self.save(conv);
        self.return_from_wait(conv, &cause);
    }

    fn now_ms(&self) -> u64 {
        (self.secs() * 1000.0) as u64
    }

    /// The reply holding the conversation's latest follow, open or not.
    fn followed(&self, conv: &str) -> Option<usize> {
        self.convs.get(conv)?.messages.iter().rposition(|m| m.follow.is_some())
    }

    /// A command step of a project conversation ended: a pull request it opened or queued is followed from now on, and a
    /// push to one already followed brings it back to pending.
    pub(super) fn pr_seen(&mut self, conv: &str, id: &str, output: &str) {
        if self.convs.get(conv).is_none_or(|c| c.area != Area::Project) {
            return;
        }
        let Some(command) = self.tool_of(conv, id).filter(|t| t.ok == Some(true)).and_then(|t| nudge::command(t).map(str::to_string))
        else {
            return;
        };
        let seen = self.convs.get(conv).map(|c| c.messages.as_slice()).unwrap_or_default();
        // land.mjs prints only the number: its address is the one this conversation follows or followed, or one its own
        // `gh pr create` printed. An address any other step showed (a file read, a page) proves nothing.
        let opened = |t: &&ToolUse| t.ok == Some(true) && nudge::command(t).and_then(follow::kind) == Some(Kind::Opens);
        let known = |n: u64| {
            seen.iter().rev().find_map(|m| {
                let f = m.follow.as_ref().filter(|f| f.number == n).map(|f| Found { repo: f.repo.clone(), number: n, url: f.url.clone() });
                f.or_else(|| {
                    m.tools
                        .iter()
                        .flatten()
                        .filter(opened)
                        .filter_map(|t| t.output.as_deref())
                        .flat_map(follow::urls)
                        .rfind(|f| f.number == n)
                })
            })
        };
        match follow::found(&command, output, known) {
            Some((_, Some(found))) => self.follow_pr(conv, found),
            Some((Kind::Pushes, None)) => self.pr_pushed(conv),
            _ => {}
        }
    }

    /// The reply a new line goes under: the running turn's, or the latest one.
    fn latest_reply(&self, conv: &str) -> Option<String> {
        let running = self.live.get(conv).and_then(|l| l.turn.clone());
        running.or_else(|| self.convs.get(conv)?.messages.iter().rev().find(|m| m.role == Role::Assistant).map(|m| m.id.clone()))
    }

    /// Takes the conversation's latest follow off its reply.
    fn take_follow(&mut self, conv: &str) -> Option<(String, Follow)> {
        let at = self.followed(conv)?;
        let m = self.convs.get_mut(conv)?.messages.get_mut(at)?;
        Some((m.id.clone(), m.follow.take()?))
    }

    /// Puts a follow under a reply, saves, and tells the screen about both replies.
    fn place_follow(&mut self, conv: &str, from: Option<String>, to: &str, follow: Follow) {
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == to)) {
            m.follow = Some(follow);
        }
        self.save(conv);
        if let Some(from) = from.filter(|f| f != to) {
            self.push(conv, Some(&from), true);
        }
        self.push(conv, Some(to), true);
    }

    fn follow_pr(&mut self, conv: &str, found: Found) {
        let Some(reply) = self.latest_reply(conv) else { return };
        let now = self.now_ms();
        let earlier = self.take_follow(conv);
        let mut from = None;
        let follow = match earlier {
            Some((at, mut f)) if f.repo == found.repo && f.number == found.number => {
                from = Some(at);
                (f.state, f.problem, f.held, f.kept, f.since) = (FollowState::Pending, None, false, false, now);
                f
            }
            Some((at, mut f)) => {
                // Another pull request: the one before stops being followed, and its line says so where it was.
                if f.state.open() {
                    (f.state, f.held) = (FollowState::Stopped, false);
                }
                self.place_follow(conv, None, &at, f);
                follow::start(found, now)
            }
            None => follow::start(found, now),
        };
        self.place_follow(conv, from, &reply, follow);
        self.look_later(conv, follow::FIRST);
    }

    fn pr_pushed(&mut self, conv: &str) {
        let resumes =
            self.followed(conv).and_then(|at| self.convs.get(conv)?.messages[at].follow.as_ref()).is_some_and(|f| f.state.watched());
        let Some(reply) = self.latest_reply(conv).filter(|_| resumes) else { return };
        let Some((from, mut f)) = self.take_follow(conv) else { return };
        (f.state, f.kept, f.since) = (FollowState::Pending, false, self.now_ms());
        self.place_follow(conv, Some(from), &reply, f);
        self.look_later(conv, follow::FIRST);
        self.follows.get_mut(conv).expect("just armed").pushed = true;
    }

    /// The person stops following the conversation's pull request.
    fn unfollow(&mut self, conv: &str) {
        self.follows.remove(conv);
        let Some(at) = self.followed(conv) else { return };
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.get_mut(at)) else { return };
        let Some(f) = m.follow.as_mut().filter(|f| f.state.watched()) else { return };
        (f.state, f.held) = (FollowState::Stopped, false);
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// Looks at the pull request again after `delay`; a newer timer replaces this one.
    fn look_later(&mut self, conv: &str, delay: Duration) {
        let ticker = self.next_sid;
        self.next_sid += 1;
        let (fails, pushed) = self.follows.get(conv).map_or((0, false), |w| (w.fails, w.pushed));
        self.follows.insert(conv.to_string(), Watch { ticker, delay, fails, pushed });
        let (me, conv, runtime) = (self.me.clone(), conv.to_string(), self.opts.runtime.clone());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            let Some(core) = me.upgrade() else { return };
            let engine = Engine { core, runtime };
            engine.look_at_pr(&conv, Some(ticker)).await;
        });
    }

    /// What to read for a conversation, if it still follows an open pull request (and, for a timer, it is still the current one).
    fn to_read(&self, conv: &str, ticker: Option<u64>) -> Option<(String, u64)> {
        let watch = self.follows.get(conv)?;
        if ticker.is_some_and(|t| t != watch.ticker) || self.closing {
            return None;
        }
        let f = self.convs.get(conv)?.messages.get(self.followed(conv)?)?.follow.as_ref().filter(|f| f.state.watched())?;
        Some((f.repo.clone(), f.number))
    }

    /// Nothing runs and nothing waits to be sent in the conversation.
    fn idle(&self, conv: &str) -> bool {
        let running = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let c = self.convs.get(conv);
        !running && c.is_some_and(|c| c.queued.is_empty() && c.messages.last().is_none_or(|m| m.status != Some(Status::Streaming)))
    }

    fn read_pr(&mut self, conv: &str, read: Result<Look, &'static str>) {
        let Some(watch) = self.follows.get_mut(conv) else { return };
        let previous = watch.delay;
        let look = match read {
            Ok(look) => {
                watch.fails = 0;
                look
            }
            Err(key) => {
                watch.fails += 1;
                if hopeless(key) || watch.fails >= follow::MAX_FAILS {
                    return self.stop_following(conv, "follow.gh");
                }
                return self.look_later(conv, follow::delay(previous, false));
            }
        };
        let Some((from, mut f)) = self.take_follow(conv) else { return };
        let now = self.now_ms();
        let head_before = f.head.clone();
        let (next, changed) = follow::step(&mut f, &look, now);
        if f.head != head_before {
            self.follows.get_mut(conv).expect("read while followed").pushed = false;
        }
        // Successful/terminal readings only return a conversation that explicitly declared a wait.
        // Use the actual remote reading: quiet/no-checks timeouts are not a successful result.
        if let Some(w) = self.convs.get(conv).and_then(|c| c.waiting_on.as_ref()).filter(|w| answers(w, f.number, &look)) {
            let ended = look.state != remote::PrState::Open;
            // An end is one event whatever wait sees it; a green result is news only against the wait's own baseline.
            let key = if ended {
                format!("wait-end-{:?}-{}", f.state, look.head)
            } else {
                format!("wait-{}-{}-{:?}-{}", w.since, w.until, look.state, look.head)
            };
            if f.woke.as_deref() != Some(&key) {
                f.woke = Some(key);
                f.kept = true;
                self.return_from_wait(
                    conv,
                    &format!(
                        "Pull request #{} ({}) is now {:?}, checks {:?}. Read the actual result and continue your saved work.",
                        f.number, f.repo, look.state, look.ci.state
                    ),
                );
            }
        }
        // While open, the line sits under the latest reply; a reply since the person was told means they have seen it.
        let to = self.latest_reply(conv).unwrap_or_else(|| from.clone());
        if to != from {
            f.held = false;
        }
        let mut place = to;
        let wait = match next {
            Next::Keep => follow::delay(previous, changed),
            Next::End => {
                // A reply since the line was placed means someone wrote while gh ran: that conversation goes on (`auto_archive`).
                f.kept |= place != from;
                self.follows.remove(conv);
                return self.place_follow(conv, Some(from), &place, f);
            }
            // The agent is at work or a message waits: look again soon, and wake it then if it is still needed.
            Next::Wake { .. } if !self.idle(conv) => follow::FIRST,
            Next::Wake { key, .. } if !self.may_spend(conv) => {
                (f.woke, f.held) = (Some(key), true);
                follow::delay(previous, true)
            }
            Next::Wake { why, key, text } => {
                f.woke = Some(key);
                let wake = Wake { pr: Some(f.number), checks: f.failed.clone(), ..Wake::of(why) };
                // Only an idle conversation gets here, so the turn starts now and its reply is the latest one.
                match self.send_by_itself(conv, text, wake).map(|()| self.latest_reply(conv)) {
                    Ok(reply) => place = reply.unwrap_or(place),
                    // A turn that failed may have left an error reply: the line goes under the latest one, so the person sees it.
                    Err(error) => {
                        f.held = true;
                        // The day's money for a fix is gone: the reply says so, never only the held line.
                        if error == goals::CAP {
                            self.goal_capped(conv);
                        }
                        place = self.latest_reply(conv).unwrap_or(place);
                    }
                }
                follow::FIRST
            }
        };
        self.place_follow(conv, Some(from), &place, f);
        self.look_later(conv, wait);
    }

    /// Whether Auto's pacing lets Faber spend a turn of its own here: the vendor the conversation runs on is usable without
    /// the person's say and none of its windows is nearly used up. A "no" goes to the person instead (R92).
    pub(super) fn may_spend(&self, conv: &str) -> bool {
        let Some(c) = self.convs.get(conv) else { return false };
        let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
        let fallback = pick_model(&choice, &self.vendors(), self.codex_recommended.as_deref());
        // The vendor `send` will use: the one picked by hand, or under Auto the one the conversation runs on.
        let pick = if choice.vendor == "auto" {
            same_as_before(c, self.live.get(conv)).filter(|p| self.opts.clis.contains_key(&p.vendor)).or(fallback)
        } else {
            fallback
        };
        let Some(pick) = pick else { return false };
        let now = self.secs();
        let family = pick.model.as_deref().and_then(family_of);
        // Over 90% in a window of the vendor or of the model's family: the person decides how to spend the rest.
        let tight = self.snap(pick.vendor, now).windows.iter().any(|(name, w)| {
            w.used > NEAR && w.resets_at > now && (crate::budget::vendor_wide(name) || family.is_some_and(|f| family_of(name) == Some(f)))
        });
        self.allows(pick.vendor, family, Purpose::UserTurn { by_hand: false }, false).is_ok() && !tight
    }

    fn stop_following(&mut self, conv: &str, problem: &str) {
        self.follows.remove(conv);
        let Some((from, mut f)) = self.take_follow(conv) else { return };
        (f.state, f.held, f.problem) = (FollowState::Stopped, false, Some(problem.into()));
        self.place_follow(conv, None, &from, f);
    }
}

impl Engine {
    /// Reads a followed pull request now and acts on what it finds. `ticker` names the timer that asked, so an older one
    /// does nothing; tests pass none to look at once.
    async fn look_at_pr(&self, conv: &str, ticker: Option<u64>) {
        let Some(((repo, number), observed, reader)) =
            self.with(|c| Some((c.to_read(conv, ticker)?, c.follows.get(conv)?.ticker, c.pr_reader.clone())))
        else {
            return;
        };
        let read = tokio::task::spawn_blocking(move || reader(&repo, number)).await.unwrap_or(Err("reason.remote.unreachable"));
        self.with(|c| {
            // The person may have stopped following, or another pull request taken its place, while gh ran.
            if c.to_read(conv, Some(observed)).is_some_and(|(_, n)| n == number) {
                c.read_pr(conv, read);
            }
        });
        // Merged or closed: the conversation is put away if nothing keeps it (git runs away from the lock).
        if self.with(|c| c.pr_over(conv)) {
            let (engine, conv) = (self.clone(), conv.to_string());
            let _ = tokio::task::spawn_blocking(move || engine.archive_finished(Some(&conv))).await;
        }
    }

    /// Looks at every followed pull request now, as the timers do; tests call it in place of waiting.
    pub async fn look_at_prs(&self) {
        let convs: Vec<String> = self.with(|c| c.follows.keys().cloned().collect());
        for conv in convs {
            self.look_at_pr(&conv, None).await;
        }
    }

    /// Replaces the gh that reads pull requests; tests give a fake one.
    pub fn set_pr_reader(&self, reader: PrReader) {
        self.with(|c| c.pr_reader = reader)
    }

    /// The person stops following a conversation's pull request (`unfollow_pr`).
    pub fn unfollow_pr(&self, conv: &str) {
        self.with(|c| c.unfollow(conv))
    }

    /// At start: the project conversations of the past week that still follow an open pull request go on following it.
    pub fn follow_open_prs(&self) {
        // The files are read with the engine free, so the screen's calls never wait behind them.
        let (store, week_ago) = self.with(|c| (c.store.clone(), c.now_ms().saturating_sub(7 * 24 * 3600 * 1000)));
        let open: Vec<String> = store
            .list()
            .into_iter()
            .filter(|s| s.area == Area::Project && s.updated_at >= week_ago)
            .map(|s| s.id)
            .filter(|id| {
                store.get(id).and_then(|conv| conv.messages.iter().rev().find_map(|m| m.follow.clone())).is_some_and(|f| f.state.watched())
            })
            .collect();
        self.with(|c| {
            for id in open {
                if c.load(&id).is_some() {
                    c.look_later(&id, follow::FIRST);
                }
            }
        })
    }
}

#[cfg(test)]
mod durable_wait_tests {
    use super::*;
    use crate::events::Session;

    fn look(state: remote::PrState, ci: remote::CiState, head: &str) -> Look {
        Look {
            state,
            head: head.into(),
            ci: remote::Ci { state: ci, head: head.into(), failed: vec![], total: 1 },
            conflict: false,
            failures: vec![],
        }
    }

    fn follow_synthetic(core: &mut Core, id: &str, number: u64) {
        let found = Found { repo: "synthetic/repository".into(), number, url: format!("https://example.invalid/{number}") };
        core.convs.get_mut(id).unwrap().messages.last_mut().unwrap().follow = Some(follow::start(found, 1));
        core.look_later(id, follow::FIRST);
    }

    /// A return was attempted: with no AI connected it stays as a visible problem instead of spending.
    fn returned(core: &Core, id: &str) -> bool {
        core.convs[id].waiting_on.as_ref().is_none_or(|w| w.problem.is_some())
    }

    #[tokio::test]
    async fn durable_wait_pr_return_needs_a_reading_newer_than_the_wait() {
        use remote::{CiState::*, PrState::*};
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            follow_synthetic(core, &id, 136);
            core.read_pr(&id, Ok(look(Open, Passing, "aaa")));
            core.session_wait(&id, vec![], Some("Await CI".into())).unwrap();
            let saved = serde_json::to_value(core.convs[&id].waiting_on.clone()).unwrap();
            assert_eq!(saved["pr"], serde_json::json!({"number": 136, "head": "aaa", "state": "passing"}), "the baseline is saved");
            core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap().pr = Option::None;
            core.restore_wait_pr(&id);
            assert_eq!(
                core.convs[&id].waiting_on.as_ref().unwrap().pr.as_ref().unwrap().head.as_deref(),
                Some("aaa"),
                "a legacy wait gets one"
            );
            core.read_pr(&id, Ok(look(Open, Passing, "aaa")));
            assert!(!returned(core, &id), "a green reading already seen before the wait is not news");
            core.pr_pushed(&id);
            core.session_wait(&id, vec![], Some("Await CI of the update".into())).unwrap();
            core.read_pr(&id, Ok(look(Open, Passing, "aaa")));
            assert!(!returned(core, &id), "after a push, the previous commit's green result is not the update's");
            core.read_pr(&id, Ok(look(Open, Pending, "bbb")));
            assert!(!returned(core, &id));
            core.read_pr(&id, Ok(look(Open, Passing, "bbb")));
            assert!(returned(core, &id), "the new commit's green result returns the wait");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_pr_pending_then_green_on_the_same_commit_returns() {
        use remote::{CiState::*, PrState::*};
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            follow_synthetic(core, &id, 137);
            core.read_pr(&id, Ok(look(Open, Pending, "aaa")));
            core.session_wait(&id, vec![], Some("Await CI".into())).unwrap();
            core.read_pr(&id, Ok(look(Open, Passing, "aaa")));
            assert!(returned(core, &id), "checks observed running when the wait began, now green");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_on_children_ignores_pr_readings() {
        use remote::{CiState::*, PrState::*};
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            follow_synthetic(core, &id, 138);
            let wait = serde_json::from_value(serde_json::json!({"ids": ["child"], "note": "review", "since": 1})).unwrap();
            core.convs.get_mut(&id).unwrap().waiting_on = Some(wait);
            core.read_pr(&id, Ok(look(Open, Passing, "aaa")));
            core.read_pr(&id, Ok(look(Merged, Passing, "aaa")));
            assert!(!returned(core, &id), "a wait on conversations returns on their reports, not on the pull request");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_on_an_already_merged_pr_returns_once() {
        use remote::{CiState::*, PrState::*};
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            follow_synthetic(core, &id, 139);
            core.read_pr(&id, Ok(look(Merged, Passing, "aaa")));
            core.session_wait(&id, vec![], Some("Await merge".into())).unwrap();
            assert!(returned(core, &id), "a pull request already merged answers the wait at once");
            core.session_wait(&id, vec![], Some("Await merge again".into())).unwrap();
            assert!(!returned(core, &id), "the same merge never returns a second wait");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_new_wait_discards_an_inflight_manual_read_of_the_same_pr() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        let (started, ready) = tokio::sync::oneshot::channel();
        let started = Mutex::new(Some(started));
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(released);
        engine.with_saves(false, |core| {
            let found = Found { repo: "synthetic/repository".into(), number: 134, url: "https://example.invalid/134".into() };
            core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().follow = Some(follow::start(found, 1));
            core.look_later(&id, follow::FIRST);
            core.pr_reader = Arc::new(move |_, _| {
                started.lock().unwrap().take().unwrap().send(()).unwrap();
                released.lock().unwrap().recv().unwrap();
                Ok(Look {
                    state: remote::PrState::Open,
                    head: "old-head".into(),
                    ci: remote::Ci { state: remote::CiState::Passing, head: "old-head".into(), failed: vec![], total: 2 },
                    conflict: false,
                    failures: vec![],
                })
            });
        });
        let (reader, conv) = (engine.clone(), id.clone());
        let read = tokio::spawn(async move { reader.look_at_pr(&conv, None).await });
        tokio::time::timeout(Duration::from_secs(5), ready).await.unwrap().unwrap();
        engine.session_wait(&id, vec![], Some("Await CI of the update".into())).unwrap();
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), read).await.unwrap().unwrap();
        let wait = engine.open(&id).unwrap().waiting_on.unwrap();
        assert!(wait.problem.is_none(), "the stale passing read must not attempt a return");
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_same_pr_new_head_gets_a_new_return_and_invalidates_old_reads() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "never-spawn".into(), prefix: vec![] });
            core.ledger.of_mut(Vendor::Claude).billing = Billing::Subscription;
            core.convs.get_mut(&id).unwrap().choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            let found = Found { repo: "synthetic/repository".into(), number: 135, url: "https://example.invalid/135".into() };
            core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().follow = Some(follow::start(found, 1));
            core.look_later(&id, follow::FIRST);
            for (at, head) in [(2000.0, "first-head"), (2001.0, "updated-head")] {
                core.clock = Some(Arc::new(move || at));
                let old = core.follows[&id].ticker;
                core.pr_pushed(&id);
                assert!(core.to_read(&id, Some(old)).is_none(), "a previous update's in-flight read is discarded");
                core.session_wait(&id, vec![], Some("Await CI of the update".into())).unwrap();
                let mut look = Look {
                    state: remote::PrState::Open,
                    head: head.into(),
                    ci: remote::Ci { state: remote::CiState::Pending, head: head.into(), failed: vec![], total: 2 },
                    conflict: false,
                    failures: vec![],
                };
                core.read_pr(&id, Ok(look.clone()));
                assert!(core.convs[&id].waiting_on.is_some());
                assert!(core.convs[&id].queued.is_empty());
                look.ci.state = remote::CiState::Passing;
                core.read_pr(&id, Ok(look.clone()));
                assert!(core.convs[&id].waiting_on.is_none());
                assert_eq!(core.convs[&id].queued.len(), 1, "each updated head must return exactly once");
                core.read_pr(&id, Ok(look));
                assert_eq!(core.convs[&id].queued.len(), 1);
                core.convs.get_mut(&id).unwrap().queued.clear();
            }
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_real_pr_results_return_waiters_but_not_ordinary_conversations() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        for state in [remote::PrState::Open, remote::PrState::Merged, remote::PrState::Closed] {
            for waiting in [false, true] {
                let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
                if waiting {
                    engine.session_wait(&id, vec![], Some("synthetic checks".into())).unwrap();
                }
                engine.with_saves(false, |core| {
                    let found = Found { repo: "synthetic/repository".into(), number: 1, url: "https://example.invalid/1".into() };
                    core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().follow = Some(follow::start(found, core.now_ms()));
                    core.follows.insert(id.clone(), Watch { ticker: 0, delay: follow::FIRST, fails: 0, pushed: false });
                    let look = Look {
                        state,
                        head: "aaa".into(),
                        ci: remote::Ci { state: remote::CiState::Passing, head: "aaa".into(), failed: vec![], total: 1 },
                        conflict: false,
                        failures: vec![],
                    };
                    core.read_pr(&id, Ok(look.clone()));
                    assert!(core.convs[&id].queued.is_empty(), "no connected AI: no spending");
                    if waiting {
                        assert!(core.convs[&id].waiting_on.as_ref().unwrap().problem.is_some());
                        assert!(core.convs[&id].messages.last().unwrap().follow.as_ref().unwrap().kept);
                        if state == remote::PrState::Open {
                            core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap().problem = Some("sentinel".into());
                            core.read_pr(&id, Ok(look));
                            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("sentinel"));
                        }
                    } else {
                        assert!(core.convs[&id].waiting_on.is_none());
                        assert!(core.convs[&id].messages.last().unwrap().follow.as_ref().unwrap().woke.is_none());
                    }
                });
            }
        }
        engine.shutdown();
    }
}
