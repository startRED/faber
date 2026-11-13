use super::*;
use crate::review::{self, Next, Reason, Role, Verdict};

/// The changes a reviewer reads inline, at most this many bytes in all and per file; the rest it reads in the folder.
const DIFF_BYTES: usize = 40_000;
const FILE_BYTES: usize = 12_000;
/// The person's request as a reviewer reads it.
const REQUEST_CHARS: usize = 4_000;

/// The real changes of the copy against its base, read with git off the engine lock: each file's patch, cut to a bound, then
/// the names of the files that did not fit. None when the conversation has no copy to compare.
fn diff_of(run: GitRunner, copy: &crate::project::copy::WorkCopy) -> Option<String> {
    let mut exec = run()?;
    let files = crate::changes::list(&mut *exec, copy);
    if files.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut left = vec![];
    for file in files {
        if text.len() >= DIFF_BYTES || file.binary || file.too_large {
            left.push(file.path);
            continue;
        }
        let patch = crate::changes::patch(&mut *exec, copy, &file.path);
        if patch.error.is_some() || patch.binary || patch.too_large {
            left.push(file.path);
            continue;
        }
        let cut: String = patch.text.chars().take(FILE_BYTES.min(DIFF_BYTES - text.len().min(DIFF_BYTES))).collect();
        let whole = cut.len() == patch.text.len();
        text.push_str(&format!("--- {}\n{cut}{}\n", file.path, if whole { "" } else { "\n[cut: read the rest in the folder]" }));
    }
    if !left.is_empty() {
        text.push_str(&format!("Also changed, read in the folder: {}\n", left.join(", ")));
    }
    Some(text)
}

pub(super) struct Run {
    pub(super) conv: String,
    sid: u64,
    call: String,
    pending: HashMap<String, String>,
    answers: Vec<(String, Verdict)>,
    omitted: Vec<String>,
    quota_only: bool,
    same_vendor: bool,
}

/// Who reviews (the owner's rule of 2026-11-13): the AI that did not write the change when it is connected and has quota,
/// with a fresh session of the writer's own AI as R543's second reviewer in the skeptical role; with one AI, two fresh
/// sessions of it, the second skeptical. Never the writer's own session.
pub(super) fn reviewers(own: Vendor, other: Vendor, other_ready: bool) -> [(Vendor, Role); 2] {
    [(if other_ready { other } else { own }, Role::Defects), (own, Role::Skeptic)]
}

fn instruction(next: Next) -> String {
    match next {
        Next::Fix => "Next: fix the rows marked Agreed, fix, then call review again".into(),
        Next::Revert => "Next: the last round made it worse; put back your edits since the previous review, then stop".into(),
        Next::Stop(reason) => format!(
            "Next: stop ({})",
            match reason {
                Reason::Target => "target",
                Reason::NoAgreed => "no agreed problems",
                Reason::Limit => "round limit",
                Reason::Flat => "no improvement",
            }
        ),
    }
}

impl Core {
    pub(super) fn start_review(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(live) = self.live.get(conv) else { return };
        let (sid, own, session) = (live.sid, live.vendor, live.session.clone());
        let reply = |text: &str| session.helper_reply(&call, true, text.into());
        if self.convs.get(conv).is_none_or(|c| c.area != Area::Project) {
            reply("Next: stop (unavailable)");
            return;
        }
        let history = self.turn(conv).and_then(|m| m.review.clone()).unwrap_or_default();
        if let Some(reason) = history.stop {
            reply(&format!("Next: stop ({reason})"));
            return;
        }
        let rounds = history.rounds;
        if !rounds.is_empty() && !matches!(review::next(&rounds), Next::Fix) {
            reply(&instruction(review::next(&rounds)));
            return;
        }
        if rounds.len() >= review::MAX_ROUNDS {
            reply("Next: stop (round limit)");
            return;
        }
        if self.helpers_in(conv) || self.reviews.values().any(|r| r.conv == conv) {
            session.helper_reply(&call, false, "error.review.busy".into());
            return;
        }
        let Some(request) = input.get("request").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) else {
            session.helper_reply(&call, false, "error.review.request".into());
            return;
        };
        let Some(files) = input.get("files").and_then(|v| v.as_array()) else {
            session.helper_reply(&call, false, "error.review.files".into());
            return;
        };
        if files.iter().any(|v| v.as_str().is_none()) {
            session.helper_reply(&call, false, "error.review.files".into());
            return;
        }
        let note = format!(
            "Compare with the copy base. Changed paths:\n{}",
            files.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n")
        );
        let c = &self.convs[conv];
        // The person's words, not the writer's account: the writer's text goes in as a claim to check (task A3).
        let asked: String = crate::proof::asked(c).chars().take(REQUEST_CHARS).collect();
        let asked = if asked.trim().is_empty() { request.to_string() } else { asked };
        let claim = request.to_string();
        let source = self.git.clone().zip(c.copy.clone().filter(|copy| !copy.root.is_empty()));
        let other = crate::helper::other(own);
        let other_ready = self.opts.clis.contains_key(&other) && self.has_quota(other);
        let picked = reviewers(own, other, other_ready);
        let mut pending = HashMap::new();
        let mut jobs = Vec::new();
        for (i, (vendor, role)) in picked.into_iter().enumerate() {
            let id = format!("review-{}", uuid::Uuid::new_v4().simple());
            let name = format!("{} {}", vendor.name(), i + 1);
            pending.insert(id.clone(), name);
            jobs.push((id, vendor, role));
        }
        self.reviews.insert(
            call.clone(),
            Run {
                conv: conv.into(),
                sid,
                call: call.clone(),
                pending,
                answers: vec![],
                omitted: vec![],
                quota_only: true,
                same_vendor: picked[0].0 == picked[1].0,
            },
        );
        // The diff is read with git off the lock, then the reviewers start; a stopped or replaced reply cancels them.
        let Some((run, copy)) = source else {
            self.start_reviewers(conv, call, jobs, &asked, &claim, &note);
            return;
        };
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn_blocking(move || {
            let diff = diff_of(run, &copy).unwrap_or(note);
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.start_reviewers(&conv, call, jobs, &asked, &claim, &diff);
        });
    }

    /// Whether some model of `vendor` may run a helper now (quota and billing), checked before a reviewer is picked so a
    /// vendor out of quota is replaced instead of left out.
    fn has_quota(&self, vendor: Vendor) -> bool {
        crate::route::candidates(&self.vendors(), &self.codex_models)
            .into_iter()
            .filter(|p| p.vendor == vendor)
            .any(|p| self.allows(p.vendor, crate::budget::family_of(p.model), Purpose::UserTurn { by_hand: false }, false).is_ok())
    }

    fn start_reviewers(&mut self, conv: &str, call: String, jobs: Vec<(String, Vendor, Role)>, asked: &str, claim: &str, diff: &str) {
        let Some(sid) = self.reviews.get(&call).map(|r| r.sid) else { return };
        if self.live.get(conv).is_none_or(|l| l.sid != sid || l.turn.is_none()) {
            self.reviews.remove(&call);
            return;
        }
        for (id, vendor, role) in jobs {
            let prompt = review::brief(role, asked, claim, diff);
            self.handle(
                conv,
                sid,
                EngineEvent::Tool {
                    id: id.clone(),
                    name: "Helper".into(),
                    input: serde_json::json!({"title":"Review"}),
                    parent: Some(call.clone()),
                },
            );
            self.launch_reviewer(conv.to_string(), call.clone(), id, vendor, prompt);
        }
    }

    fn launch_reviewer(&mut self, conv: String, parent: String, id: String, vendor: Vendor, prompt: String) {
        if !self.reviews.get(&parent).is_some_and(|r| r.pending.contains_key(&id)) {
            return;
        }
        let input = serde_json::json!({"title":"Review", "brief":prompt});
        match self.launch_helper(&conv, &id, &input, Some(vendor)) {
            Ok(()) => {}
            Err(why) if why == super::worker_memory::PENDING => {
                let owner = conv.clone();
                self.defer_worker(format!("helper:{id}"), owner, move |core| core.launch_reviewer(conv, parent, id, vendor, prompt));
            }
            Err(why) if why == super::mcp_servers::CHECK_PENDING => {
                let key = format!("helper:{id}");
                self.after_kit_check(&key, move |core| core.launch_reviewer(conv, parent, id, vendor, prompt));
            }
            Err(why) => {
                self.finish_worker_admission(&format!("helper:{id}"));
                self.kit_checks.remove(&format!("helper:{id}"));
                let sid = self.reviews.get(&parent).map(|r| r.sid).unwrap_or_default();
                let quota = why.contains("quota") || why.contains("billed");
                self.handle(
                    &conv,
                    sid,
                    EngineEvent::ToolResult {
                        id: id.clone(),
                        ok: false,
                        declined: false,
                        text: if quota { "error.review.quota" } else { "error.review.failed" }.into(),
                    },
                );
                self.review_ended(&conv, &id, false, "", quota);
            }
        }
    }

    pub(super) fn review_ended(&mut self, conv: &str, id: &str, ok: bool, text: &str, quota: bool) -> bool {
        let Some(parent) = self.reviews.iter().find_map(|(key, r)| (r.conv == conv && r.pending.contains_key(id)).then(|| key.clone()))
        else {
            return false;
        };
        let run = self.reviews.get_mut(&parent).unwrap();
        let name = run.pending.remove(id).unwrap();
        run.quota_only &= quota;
        match ok.then(|| review::parse(text)).transpose() {
            Ok(Some(verdict)) => run.answers.push((name, verdict)),
            _ => run.omitted.push(name),
        }
        if !run.pending.is_empty() {
            return true;
        }
        let run = self.reviews.remove(&parent).unwrap();
        let Some(session) = self.live.get(conv).filter(|l| l.sid == run.sid).map(|l| l.session.clone()) else { return true };
        if run.answers.is_empty() {
            let reason = if run.quota_only { "review.stop.quota" } else { "review.stop.invalid" };
            if let Some(m) = self.turn(conv) {
                m.review.get_or_insert_default().stop = Some(reason.into());
                let mid = m.id.clone();
                self.save(conv);
                self.emit_message(conv, &mid);
            }
            session.helper_reply(&run.call, true, format!("Reviewers left out: {}.\nNext: stop ({reason})", run.omitted.join(", ")));
            return true;
        }
        let mut round = review::score(&run.answers);
        round.omitted = run.omitted.clone();
        round.call = Some(run.call.clone());
        let mut text = format!(
            "Score: {} out of 25\nDoes what was asked: {}\nCorrect: {}\nSafe: {}\nSimple: {}\nChecked: {}",
            round.total, round.lines[0], round.lines[1], round.lines[2], round.lines[3], round.lines[4]
        );
        for f in &round.agreed {
            let r = &f.row;
            text.push_str(&format!(
                "\n{}: line {}, {}{}: {}; should: {}; evidence: {}",
                if r.blocks() { "Agreed, fix" } else { "Agreed, optional" },
                r.line,
                r.file,
                r.at.map(|n| format!(":{n}")).unwrap_or_default(),
                r.what,
                r.should,
                r.evidence
            ));
        }
        text.push_str(&format!("\nNotes seen by one reviewer or without evidence: {}", round.notes.len()));
        if round.reviewers.len() == 1 {
            text.push_str("\nOnly one reviewer answered: its findings with evidence count as agreed.");
        }
        if run.same_vendor {
            text.push_str("\nBoth reviews used the same AI.");
        }
        if !run.omitted.is_empty() {
            text.push_str(&format!("\nReviewers left out: {}", run.omitted.join(", ")));
        }
        let Some(m) = self.turn(conv) else { return true };
        let history = m.review.get_or_insert_default();
        history.rounds.push(round);
        let next = review::next(&history.rounds);
        match next {
            Next::Revert => history.stop = Some("review.stop.worse".into()),
            Next::Stop(reason) => history.stop = Some(reason.key().into()),
            Next::Fix => {}
        }
        text.push_str(&format!("\n{}", instruction(next)));
        let mid = m.id.clone();
        self.save(conv);
        self.emit_message(conv, &mid);
        session.helper_reply(&run.call, true, text);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ai_that_did_not_write_reviews_first_and_a_fresh_skeptic_of_the_writers_ai_second() {
        assert_eq!(reviewers(Vendor::Codex, Vendor::Claude, true), [(Vendor::Claude, Role::Defects), (Vendor::Codex, Role::Skeptic)]);
        assert_eq!(
            reviewers(Vendor::Codex, Vendor::Claude, false),
            [(Vendor::Codex, Role::Defects), (Vendor::Codex, Role::Skeptic)],
            "the other AI missing or out of quota: two fresh sessions of the writer's AI, one skeptical"
        );
    }
}
