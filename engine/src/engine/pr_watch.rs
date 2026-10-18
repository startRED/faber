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
}

/// A window this full leaves the rest of it to the person.
const NEAR: f64 = 0.9;

/// The errors after which looking again cannot help.
fn hopeless(key: &str) -> bool {
    matches!(key, "reason.remote.ghMissing" | "reason.remote.ghLoggedOut" | "reason.remote.target")
}

impl Core {
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
        let resumes = self
            .followed(conv)
            .and_then(|at| self.convs.get(conv)?.messages[at].follow.as_ref())
            .is_some_and(|f| f.state.open() || f.state == FollowState::Passing);
        let Some(reply) = self.latest_reply(conv).filter(|_| resumes) else { return };
        let Some((from, mut f)) = self.take_follow(conv) else { return };
        (f.state, f.kept, f.since) = (FollowState::Pending, false, self.now_ms());
        self.place_follow(conv, Some(from), &reply, f);
        self.look_later(conv, follow::FIRST);
    }

    /// The person stops following the conversation's pull request.
    fn unfollow(&mut self, conv: &str) {
        self.follows.remove(conv);
        let Some(at) = self.followed(conv) else { return };
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.get_mut(at)) else { return };
        let Some(f) = m.follow.as_mut().filter(|f| f.state.open()) else { return };
        (f.state, f.held) = (FollowState::Stopped, false);
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// Looks at the pull request again after `delay`; a newer timer replaces this one.
    fn look_later(&mut self, conv: &str, delay: Duration) {
        let ticker = self.next_sid;
        self.next_sid += 1;
        let fails = self.follows.get(conv).map_or(0, |w| w.fails);
        self.follows.insert(conv.to_string(), Watch { ticker, delay, fails });
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
        let f = self.convs.get(conv)?.messages.get(self.followed(conv)?)?.follow.as_ref().filter(|f| f.state.open())?;
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
        let (next, changed) = follow::step(&mut f, &look, now);
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
                let wake = Wake { why: why.into(), pr: Some(f.number), checks: f.failed.clone() };
                // Only an idle conversation gets here, so the turn starts now and its reply is the latest one.
                match self.send_by_itself(conv, text, wake).map(|()| self.latest_reply(conv)) {
                    Ok(reply) => place = reply.unwrap_or(place),
                    // A turn that failed may have left an error reply: the line goes under the latest one, so the person sees it.
                    Err(_) => {
                        f.held = true;
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
    fn may_spend(&self, conv: &str) -> bool {
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
        let Some(((repo, number), reader)) = self.with(|c| Some((c.to_read(conv, ticker)?, c.pr_reader.clone()))) else { return };
        let read = tokio::task::spawn_blocking(move || reader(&repo, number)).await.unwrap_or(Err("reason.remote.unreachable"));
        self.with(|c| {
            // The person may have stopped following, or another pull request taken its place, while gh ran.
            if c.to_read(conv, ticker).is_some_and(|(_, n)| n == number) {
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
        self.with(|c| {
            let week_ago = c.now_ms().saturating_sub(7 * 24 * 3600 * 1000);
            let ids: Vec<String> =
                c.store.list().into_iter().filter(|s| s.area == Area::Project && s.updated_at >= week_ago).map(|s| s.id).collect();
            for id in ids {
                let open = c
                    .store
                    .get(&id)
                    .and_then(|conv| conv.messages.iter().rev().find_map(|m| m.follow.clone()))
                    .is_some_and(|f| f.state.open());
                if open && c.load(&id).is_some() {
                    c.look_later(&id, follow::FIRST);
                }
            }
        })
    }
}
