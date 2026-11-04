use super::*;
use crate::review::{self, Next, Reason, Verdict};

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

fn instruction(next: Next) -> String {
    match next {
        Next::Fix => "Next: fix the agreed problems, then call review again".into(),
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
        let prompt = review::brief(request, &note);
        let other = crate::helper::other(own);
        let vendors = [if self.opts.clis.contains_key(&other) { other } else { own }, own];
        let mut pending = HashMap::new();
        let mut jobs = Vec::new();
        for (i, vendor) in vendors.into_iter().enumerate() {
            let id = format!("review-{}", uuid::Uuid::new_v4().simple());
            let name = format!("{} {}", vendor.name(), i + 1);
            pending.insert(id.clone(), name);
            jobs.push((id, vendor));
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
                same_vendor: vendors[0] == vendors[1],
            },
        );
        for (id, vendor) in jobs {
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
            self.launch_reviewer(conv.to_string(), call.clone(), id, vendor, prompt.clone());
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
                "\nAgreed: line {}, {}{}: {}; should: {}",
                r.line,
                r.file,
                r.at.map(|n| format!(":{n}")).unwrap_or_default(),
                r.what,
                r.should
            ));
        }
        text.push_str(&format!("\nNotes seen by one reviewer: {}", round.notes.len()));
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
