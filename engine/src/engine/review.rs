use super::*;
use crate::review::{self, Next, Reason, Role, Verdict};

/// The inline excerpt, the body of each patch, and the space reserved for omitted paths.
const DIFF_BYTES: usize = 40_000;
const FILE_BYTES: usize = 12_000;
const PATH_BYTES: usize = 2_000;
/// The person's request as a reviewer reads it.
const REQUEST_CHARS: usize = 4_000;

/// The real changes of the copy against its base, read with git off the engine lock.
fn diff_of(run: GitRunner, copy: &crate::project::copy::WorkCopy, requested: &[String]) -> Option<String> {
    let mut exec = run()?;
    let page = crate::changes::list_page(&mut *exec, copy, None);
    if page.files.is_empty() && !page.incomplete {
        return None;
    }
    Some(review_excerpt(page.files, page.incomplete, requested, |path| crate::changes::patch(&mut *exec, copy, path)))
}

fn review_excerpt(
    mut files: Vec<crate::changes::FileChange>,
    incomplete: bool,
    requested: &[String],
    mut read_patch: impl FnMut(&str) -> crate::changes::ChangePatch,
) -> String {
    const CUT: &str = "\n[cut: read the rest in the folder]";
    let mut unverified: HashSet<&str> = requested.iter().map(String::as_str).collect();
    // Requested paths affect order only. The change scan decides which paths can be read.
    files.sort_by_key(|file| !unverified.contains(file.path.as_str()));
    let mut text = String::new();
    let mut left = vec![];
    for file in files {
        unverified.remove(file.path.as_str());
        let header = format!("--- {}\n", file.path);
        let room = (DIFF_BYTES - PATH_BYTES).saturating_sub(text.len() + header.len() + 1);
        if room <= CUT.len() || file.binary || file.too_large {
            left.push(file.path);
            continue;
        }
        let patch = read_patch(&file.path);
        if patch.error.is_some() || patch.binary || patch.too_large {
            left.push(file.path);
            continue;
        }
        text.push_str(&header);
        if patch.text.len() <= room.min(FILE_BYTES) {
            text.push_str(&patch.text);
        } else {
            let end = patch.text.floor_char_boundary(FILE_BYTES.min(room - CUT.len()));
            text.push_str(&patch.text[..end]);
            text.push_str(CUT);
        }
        text.push('\n');
    }
    if incomplete {
        text.push_str("The change scan was incomplete. Check the change list in the folder.\n");
        let limit = if left.is_empty() { DIFF_BYTES } else { DIFF_BYTES - PATH_BYTES / 2 };
        let missing = requested.iter().map(String::as_str).filter(|path| unverified.remove(path));
        append_review_paths(&mut text, "Author-reported paths to check:", missing, limit);
    }
    append_review_paths(&mut text, "Also changed, read in the folder:", left.iter().map(String::as_str), DIFF_BYTES);
    text
}

fn append_review_paths<'a>(text: &mut String, label: &str, paths: impl Iterator<Item = &'a str>, limit: usize) {
    const MORE: &str = "\nMore paths omitted. Check the change list in the folder.";
    for (index, path) in paths.enumerate() {
        let separator = if index == 0 {
            text.push_str(label);
            " "
        } else {
            ", "
        };
        if text.len() + separator.len() + path.len() + MORE.len() + 1 > limit {
            text.push_str(MORE);
            text.push('\n');
            return;
        }
        text.push_str(separator);
        text.push_str(path);
    }
    if text.as_bytes().last().is_some_and(|b| *b != b'\n') {
        text.push('\n');
    }
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
pub(super) fn reviewers(own: Vendor, mut alternates: impl Iterator<Item = Vendor>) -> [(Vendor, Role); 2] {
    let first = alternates.next().unwrap_or(own);
    let second = if own == Vendor::Gemini { alternates.next().unwrap_or(first) } else { own };
    [(first, Role::Defects), (second, Role::Skeptic)]
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
        let requested: Vec<String> = files.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        let mut note = "Compare with the copy base.\n".to_string();
        append_review_paths(&mut note, "Author-reported paths to check:", requested.iter().map(String::as_str), DIFF_BYTES);
        let c = &self.convs[conv];
        // The person's words, not the writer's account: the writer's text goes in as a claim to check (task A3).
        let asked: String = crate::proof::asked(c).chars().take(REQUEST_CHARS).collect();
        let asked = if asked.trim().is_empty() { request.to_string() } else { asked };
        let claim = request.to_string();
        let source = self.git.clone().zip(c.copy.clone().filter(|copy| !copy.root.is_empty()));
        let alternates = Vendor::ALL.into_iter().filter(|v| *v != own && self.has_quota(*v));
        let picked = reviewers(own, alternates);
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
            let diff = diff_of(run, &copy, &requested).unwrap_or(note);
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.start_reviewers(&conv, call, jobs, &asked, &claim, &diff);
        });
    }

    /// Whether some model of `vendor` may run a helper now (quota and billing), checked before a reviewer is picked so a
    /// vendor out of quota is replaced instead of left out.
    fn has_quota(&self, vendor: Vendor) -> bool {
        // A reviewer needs enforceable plan permissions, which the Antigravity session cannot provide yet.
        if vendor == Vendor::Gemini {
            return false;
        }
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
                        exit_code: None,
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

    fn changed(path: &str) -> crate::changes::FileChange {
        crate::changes::FileChange {
            path: path.into(),
            added: 1,
            removed: 0,
            binary: false,
            counts_known: true,
            too_large: false,
            status: "modified".into(),
        }
    }

    #[test]
    fn reviewers_read_requested_changes_first_and_still_see_the_other_paths() {
        let files = ["a", "b", "c", "d", "z"].map(changed).to_vec();
        let mut reads = vec![];
        let excerpt = review_excerpt(files, false, &["z".into(), "unlisted".into()], |path| {
            reads.push(path.to_string());
            crate::changes::ChangePatch { text: path.repeat(FILE_BYTES), ..Default::default() }
        });
        assert_eq!(reads.first().map(String::as_str), Some("z"));
        assert!(!reads.iter().any(|path| path == "unlisted"));
        assert!(!excerpt.contains("unlisted"));
        assert!(excerpt.contains("Also changed, read in the folder: d"));
    }

    #[test]
    fn an_incomplete_change_scan_retains_unverified_requested_paths_without_reading_them() {
        let paths = ["z".into(), "z".into(), "a".into()];
        let excerpt = review_excerpt(vec![], true, &paths, |_| panic!("no canonical file was found"));
        assert!(excerpt.contains("The change scan was incomplete"));
        assert!(excerpt.contains("Author-reported paths to check: z, a\n"));
        let excerpt = review_excerpt(vec![changed("a")], true, &paths, |path| {
            assert_eq!(path, "a");
            crate::changes::ChangePatch { text: "real patch".into(), ..Default::default() }
        });
        assert!(excerpt.contains("--- a\nreal patch"));
        assert!(excerpt.contains("Author-reported paths to check: z\n"));
    }

    #[test]
    fn review_excerpt_counts_utf8_bytes_and_leaves_room_for_the_omission_notice() {
        let first = "a".repeat(100);
        let mut files = vec![changed(&first), changed("b"), changed("c"), changed("d")];
        files.extend((0..100).map(|i| changed(&format!("{i}-{}", "e".repeat(100)))));
        let excerpt =
            review_excerpt(files, true, &[], |_| crate::changes::ChangePatch { text: "🙂界".repeat(FILE_BYTES), ..Default::default() });
        assert!(excerpt.len() <= DIFF_BYTES);
        let body = excerpt.split('\n').nth(1).unwrap();
        assert!(body.len() <= FILE_BYTES);
        assert!(body.len() >= FILE_BYTES - 3);
        assert!(excerpt.contains("[cut: read the rest in the folder]"));
        assert!(excerpt.contains("Also changed, read in the folder:"));
        assert!(excerpt.contains("More paths omitted"));
        assert!(excerpt.contains("The change scan was incomplete"));
    }

    #[test]
    fn the_ai_that_did_not_write_reviews_first_and_a_fresh_skeptic_of_the_writers_ai_second() {
        assert_eq!(
            reviewers(Vendor::Codex, [Vendor::Claude].into_iter()),
            [(Vendor::Claude, Role::Defects), (Vendor::Codex, Role::Skeptic)]
        );
        assert_eq!(
            reviewers(Vendor::Codex, std::iter::empty()),
            [(Vendor::Codex, Role::Defects), (Vendor::Codex, Role::Skeptic)],
            "the other AI missing or out of quota: two fresh sessions of the writer's AI, one skeptical"
        );
        assert_eq!(
            reviewers(Vendor::Gemini, [Vendor::Claude, Vendor::Codex].into_iter()),
            [(Vendor::Claude, Role::Defects), (Vendor::Codex, Role::Skeptic)]
        );
    }
}
