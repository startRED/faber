//! Helpers (R161): when a project's agent calls Faber's helper tool, a selected vendor runs that piece of work in a session of
//! its own, in the same folder and with the same permission mode. What the helper runs shows inside the agent's call, its
//! final words go back as the call's result, and its cost joins the reply's.
use super::*;
use crate::helper::{self, Helper};

/// The longest a helper may work; the agent and its turn wait for it all that time.
const MAX_RUN: Duration = Duration::from_secs(30 * 60);
/// How long a stopped helper has to report before it is ended.
const STOP_GRACE: Duration = Duration::from_secs(10);

pub(super) struct Run {
    /// The conversation and the agent's process (its `sid`) that called the helper; the report goes back only to that process.
    conv: String,
    sid: u64,
    /// Tells this helper's events apart from a helper that replaced it under the same call id.
    pub(super) hsid: u64,
    vendor: Vendor,
    model: String,
    permissions: Permissions,
    web: WebSearch,
    session: Session,
    started: f64,
    /// The helper's latest block of text: its final words are the report.
    report: String,
    /// Bytes already persisted from this block; flush on a step or at 8 KiB, rather than syncing every streamed token.
    kept: usize,
    new_block: bool,
    /// Tools the helper was not allowed to use without the person's approval.
    refused: Vec<String>,
    /// Messages the person wrote to it that it has not taken in yet (their ids), and the words of those it took (R161 step 5).
    waiting: Vec<String>,
    told: Vec<String>,
    /// A turn's end retained until a queued message or one of this helper's commands is resolved.
    held: Option<Ended>,
    command_wake: u64,
    command_cost: Option<f64>,
    /// A previous turn has unaccounted cost. A cumulative Claude total can account for it later.
    prior_cost_incomplete: bool,
    /// The agent stopped: the turn that ends now is the last, whatever is still on its way.
    stopping: Option<&'static str>,
    /// Whether a message can reach the helper's turn now: Claude Code reads its input from the start, Codex only once its turn
    /// has begun (`turn/steer` names the turn). What the person wrote before that waits in `early` (id, words).
    ready: bool,
    /// A note sent as a new Codex turn while the helper was waiting on a command.
    note_turn: Option<String>,
    early: Vec<(String, String)>,
    lead_call: Option<String>,
    question: Option<String>,
    question_timed: bool,
    sent: usize,
    reviewer: bool,
}

struct Ended {
    ok: bool,
    error: Option<String>,
    cost: Option<f64>,
    incomplete: bool,
    wait: HeldBy,
}

#[derive(PartialEq, Eq)]
enum HeldBy {
    Message,
    Command(Option<u64>),
}

impl Core {
    /// Existing helper-start acts survive helper exit and automatic continuations; the next user message is the boundary.
    fn reply_has_maker(&self, conv: &str, vendor: Vendor) -> bool {
        self.convs.get(conv).is_some_and(|c| {
            c.messages.iter().rev().take_while(|m| m.role != Role::User || m.wake.is_some()).flat_map(|m| &m.acts).any(|act| {
                act.kind == crate::act::ActKind::Helper
                    && matches!(act.actor, crate::act::Actor::Ai { vendor: v, role: crate::act::ActorRole::Helper, .. } if v == vendor)
            })
        })
    }

    pub(super) fn helpers_in(&self, conv: &str) -> bool {
        self.helpers.values().any(|h| h.conv == conv)
            || self.waiting_helpers(conv, "") > 0
            || self.worker_memory.admitted.iter().any(|(key, owner)| key.starts_with("helper:") && owner == conv)
            || self.kit_checks.iter().any(|(key, check)| key.starts_with("helper:") && check.owner == conv)
    }
    /// The team names of the helpers still at work for `conv`, in order.
    pub(super) fn working_helpers(&self, conv: &str) -> Vec<String> {
        let mut names: Vec<String> =
            self.helpers.iter().filter(|(_, h)| h.conv == conv).map(|(call, _)| self.team_name(conv, call)).collect();
        names.sort();
        names
    }
    /// The quota factors a helper's model is priced with, as Auto prices it.
    fn helper_factor(&self, p: &crate::route::Profile, now: f64) -> crate::route::Factor {
        let snap = self.snap(p.vendor, now);
        let ledger = self.ledger.of(p.vendor);
        crate::route::Factor {
            m: snap.profile_m(family_of(p.model), &ledger.shape(), self.utc_offset, now),
            w: ledger.weights.w.get(p.model).copied().unwrap_or(1.0),
            step_ok: snap.step_ok(now),
            read: 0.0,
        }
    }

    /// The models a project agent's own subagents run on (`helper::Subagents`), chosen when it starts as its helpers are
    /// chosen, among its own CLI's models that may run now. None when the agent offers no helper tool.
    pub(super) fn subagent_models(&self, conv: &str, helper: Helper, vendor: Vendor, model: Option<&str>) -> Option<helper::Subagents> {
        if helper != Helper::Offers {
            return None;
        }
        let lead =
            crate::route::PROFILES.into_iter().filter(|p| p.vendor == vendor && Some(p.model) == model).max_by_key(|p| (p.iq, p.agent));
        let candidates: Vec<_> = crate::route::candidates(&[vendor], &self.codex_models)
            .into_iter()
            .filter(|p| self.allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false).is_ok())
            .collect();
        let now = self.secs();
        helper::subagents(lead, vendor, &candidates, &|p| self.helper_factor(p, now), self.economy(conv))
    }

    /// Whether a project conversation on `vendor` offers the helper tool.
    pub(super) fn offers_helper(&self, area: Area, vendor: Vendor) -> Helper {
        if area == Area::Project && self.opts.clis.contains_key(&vendor) && !self.bench.off(BenchPart::Delegation) {
            Helper::Offers
        } else {
            Helper::Off
        }
    }

    /// The agent called the helper tool. A helper that cannot start answers the call at once with why, so the agent goes on alone.
    pub(super) fn start_helper(&mut self, conv: &str, call: String, input: serde_json::Value) {
        if ["to", "answer", "tell", "wait"].iter().any(|key| input.get(key).is_some()) {
            return self.helper_control(conv, &call, &input);
        }
        if helper::prompt(&input).is_some() {
            self.init_team(conv, &call, &input);
            if self.helpers.values().any(|h| h.conv == conv && h.question.is_some()) {
                self.release_team_calls(conv, &call);
                self.start_team_timers(conv);
            }
        }
        if let Err(why) = self.launch_helper(conv, &call, &input, None) {
            if why == super::worker_memory::PENDING {
                let Some(sid) = self.live.get(conv).map(|live| live.sid) else { return };
                let owner = conv.to_string();
                self.defer_worker(format!("helper:{call}"), owner.clone(), move |core| {
                    if core.live.get(&owner).is_some_and(|live| live.sid == sid)
                        && core.convs.get(&owner).is_some_and(|conversation| !conversation.stopped && conversation.archived_at.is_none())
                    {
                        core.start_helper(&owner, call, input);
                    }
                });
                return;
            }
            if why == super::mcp_servers::CHECK_PENDING {
                let Some(sid) = self.live.get(conv).map(|live| live.sid) else {
                    self.finish_worker_admission(&format!("helper:{call}"));
                    return;
                };
                let (owner, key) = (conv.to_string(), format!("helper:{call}"));
                let admitted_key = key.clone();
                self.after_kit_check(&key, move |core| {
                    if !core.closing
                        && !core.installing
                        && core.live.get(&owner).is_some_and(|live| live.sid == sid)
                        && core.convs.get(&owner).is_some_and(|conversation| !conversation.stopped)
                    {
                        core.start_helper(&owner, call, input);
                    } else {
                        core.finish_worker_admission(&admitted_key);
                    }
                });
                return;
            }
            self.finish_worker_admission(&format!("helper:{call}"));
            self.kit_checks.remove(&format!("helper:{call}"));
            self.mark_helper_seen(&call);
            if let Some(early) = self.helper_early.remove(&call) {
                let ids: Vec<String> = early.into_iter().map(|(id, _)| id).collect();
                self.never_read(conv, &call, &ids);
            }
            let (_, text) = helper::report(false, "", &[], Some(why));
            self.change_team(conv, &call, |team| team.state = TeamState::Ended);
            if let Some(l) = self.live.get(conv) {
                if self.team_of(conv, &call).is_some() && !l.open_tools.contains(&call) {
                    self.handle(conv, l.sid, EngineEvent::HelperDone { id: call, ok: false, text });
                } else {
                    l.session.helper_reply(&call, false, text);
                }
            }
        }
    }

    pub(super) fn launch_helper(
        &mut self,
        conv: &str,
        call: &str,
        input: &serde_json::Value,
        review: Option<Vendor>,
    ) -> Result<(), &'static str> {
        if self.goal_out_of_money(conv) {
            return Err(super::siblings::GOAL_CAP);
        }
        if self.rewind_blocked(conv) {
            return Err("error.rewind.busy");
        }
        let l = self.live.get(conv).ok_or("the conversation is gone")?;
        let mut piece = helper::Piece::of(input);
        if review.is_some() {
            piece.job = "review";
        }
        let job = piece.job;
        let lead_vendor = l.vendor;
        // A model absent from the table stays uncapped: its strength is unknown.
        let lead = crate::route::PROFILES
            .into_iter()
            .filter(|p| p.vendor == l.vendor && Some(p.model) == l.model.as_deref())
            .max_by_key(|p| (p.iq, p.agent));
        let (sid, permissions, tuning, cwd) = (
            l.sid,
            if review.is_some() || piece.read_only() { Permissions::Plan } else { l.permissions },
            Tuning { web: l.tuning.web, summary: l.tuning.summary, language: l.tuning.language.clone(), ..Default::default() },
            l.cwd.clone(),
        );
        if self.convs.get(conv).map(|c| c.area) != Some(Area::Project) {
            return Err("helpers work only in projects");
        }
        let check_name = format!("helper:{call}");
        if self.helpers.values().filter(|h| h.conv == conv).count()
            + self
                .kit_checks
                .iter()
                .filter(|(name, check)| {
                    name.starts_with("helper:") && name.as_str() != check_name && check.owner == conv && !self.worker_waiting(name)
                })
                .count()
            + self.waiting_helpers(conv, &check_name)
            >= helper::MAX_PER_CONVERSATION
        {
            return Err("three helpers are already working; wait for one to finish");
        }
        let prompt = helper::worker_prompt(input, review.is_none()).ok_or("the brief was empty")?;
        let economy = self.economy(conv);
        let review_vendor = review.unwrap_or_else(|| {
            Vendor::RUNNABLE
                .into_iter()
                .find(|v| *v != lead_vendor && *v != Vendor::Gemini && self.opts.clis.contains_key(v))
                .unwrap_or(lead_vendor)
        });
        let refusal = |why| match why {
            Why::Billing => "that AI may be billed per use, so Faber does not call it on its own",
            _ => "that AI has no quota left right now",
        };
        let mut refused = None;
        let candidates: Vec<_> = crate::route::candidates(&self.vendors(), &self.codex_models)
            .into_iter()
            // Antigravity's driver cannot enforce the helper's read-only plan permissions.
            .filter(|p| p.vendor != Vendor::Gemini)
            .filter(|p| job != "review" || p.vendor == review_vendor)
            .filter(|p| match self.allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false) {
                Ok(()) => true,
                Err(why) => {
                    refused.get_or_insert(why);
                    false
                }
            })
            .collect();
        let now = self.secs();
        let factor = |p: &crate::route::Profile| self.helper_factor(p, now);
        let selected = if let Some(vendor) = review {
            helper::profile(&candidates, vendor, economy)
        } else {
            let other = helper::other(lead_vendor);
            let makers = [lead_vendor, if self.reply_has_maker(conv, other) { other } else { lead_vendor }];
            helper::select(piece, lead, lead_vendor, &candidates, &makers, &factor, economy)
        };
        // Before the account lists its models, retain the recommended Codex model for independent reviews.
        let fallback = (job == "review" && review_vendor == Vendor::Codex && self.vendors().contains(&Vendor::Codex))
            .then(|| self.codex_recommended.clone())
            .flatten();
        let (vendor, model, effort) = if let Some(p) = selected.filter(|p| fallback.is_none() || p.vendor == Vendor::Codex) {
            (p.vendor, Some(p.model.to_string()), p.effort.to_string())
        } else if let Some(model) = fallback {
            (Vendor::Codex, Some(model), if economy { "low" } else { "medium" }.to_string())
        } else if let Some(p) = selected {
            (p.vendor, Some(p.model.to_string()), p.effort.to_string())
        } else {
            return Err(if job == "advise" {
                helper::NO_ADVISER
            } else {
                refused.map(refusal).unwrap_or("no connected AI has a model for this")
            });
        };
        let cli = self.opts.clis.get(&vendor).cloned().ok_or("the AI is not connected")?;
        let family = model.as_deref().and_then(family_of);
        self.allows(vendor, family, Purpose::UserTurn { by_hand: false }, false).map_err(refusal)?;
        let project = self.convs.get(conv).and_then(|c| c.project_id.clone());
        let disabled = self.project_disabled_servers(project.as_deref());
        // Skills are picked from the agent's own words for the piece, never from Faber's standing helper text: the review
        // wording (`helper::SUBSTANCE`) would otherwise pull the same kits into every review.
        let words = ["title", "brief"].map(|k| input[k].as_str().unwrap_or_default().trim()).join("\n\n");
        let raw = self.session_tools(vendor, Area::Project, permissions, project.as_deref(), &words, &cwd);
        if !self.worker_admitted(&check_name, conv) {
            return Err(super::worker_memory::PENDING);
        }
        let (mut tools, _) = self
            .check_kit(&check_name, conv, vendor, Area::Project, permissions, &cwd, words, None, vec![], disabled, raw, None)
            .map_err(|_| super::mcp_servers::CHECK_PENDING)?;
        self.finish_worker_admission(&check_name);
        let flags = if vendor == Vendor::Claude { self.claude_flags_for(&cli) } else { Default::default() };
        if vendor == Vendor::Claude && !flags.plugin_dir {
            tools.plugin = None;
        }
        self.begin_unbounded(vendor).map_err(|_| "that AI cannot start right now")?;
        let user_hooks = self.store.settings().user_hooks == Some(true) && permissions.guarded();
        self.next_sid += 1;
        let hsid = self.next_sid;
        self.rewind_helper_started(call, hsid, conv, cwd.clone());
        let (me, key) = (self.me.clone(), call.to_string());
        let sink: EventSink = Arc::new(move |event| {
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).helper_event(&key, hsid, event);
            }
        });
        let shown = model.clone().unwrap_or_else(|| vendor.name().to_string());
        let shown_effort = effort.clone();
        let effort = Some(effort);
        let resources = (vendor == Vendor::Claude).then(|| super::kits::claude_resources(Area::Project, &tools)).flatten();
        let web = tuning.web;
        let session = match vendor {
            Vendor::Gemini => return Err("error.notConnected"),
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch {
                        subagents: None,
                        temporary: false,
                        area: Area::Project,
                        model: model.clone(),
                        effort,
                        resume: None,
                        user_hooks,
                        permissions,
                        instructions: None,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        flags,
                        tuning,
                        helper: if review.is_some() { Helper::Reviewer } else { Helper::Is },
                        browse: false,
                        conversations: false,
                        leads: false,
                        progress: false,
                        faber: false,
                        verify: false,
                        tools,
                    },
                    spawn: spawner(&self.opts.spawn, &cli),
                    sandbox: self.opts.clis.get(&Vendor::Codex).cloned(),
                },
                sink,
            ),
            Vendor::Codex => self.open_codex(
                Area::Project,
                user_hooks,
                permissions,
                &cli,
                CodexOpen {
                    tools: Some(tools),
                    ..CodexOpen::new(
                        CodexThread {
                            subagents: None,
                            temporary: false,
                            title: None,
                            leave_plan: false,
                            cwd,
                            model: model.clone(),
                            effort,
                            resume: None,
                            instructions: None,
                            research: None,
                            user_name: None,
                            group_instructions: None,
                            permissions,
                            tuning,
                            helper: if review.is_some() { Helper::Reviewer } else { Helper::Is },
                            browse: false,
                            conversations: false,
                            leads: false,
                            progress: false,
                            faber: false,
                            verify: false,
                        },
                        sink,
                    )
                },
            ),
        };
        self.worker_memory.registry.register(session.owned_tree());
        self.rewind_helper_session(call, hsid, session.clone());
        let prompt = super::kits::with_resources(prompt, resources);
        if let Some(reply) = self.turn(conv) {
            reply.acts.push(crate::act::FaberAct {
                kind: crate::act::ActKind::Helper,
                actor: crate::act::Actor::Ai { vendor, model: model.clone(), role: crate::act::ActorRole::Helper },
                what: "act.helper.started".into(),
                why: None,
                params: [
                    ("vendor".into(), vendor.name().into()),
                    ("model".into(), shown.clone()),
                    ("effort".into(), shown_effort),
                    ("job".into(), job.into()),
                    ("call".into(), call.into()),
                ]
                .into(),
                change: None,
                sent: Some(prompt.chars().take(4000).collect()),
            });
            let id = reply.id.clone();
            self.save(conv);
            self.emit_message(conv, &id);
        }
        session.send(UserTurn::text(prompt));
        self.init_team(conv, call, input);
        let started = self.secs();
        let mut run = Run {
            conv: conv.to_string(),
            sid,
            hsid,
            vendor,
            model: shown,
            permissions,
            web,
            session,
            started,
            report: String::new(),
            kept: 0,
            new_block: false,
            refused: vec![],
            waiting: vec![],
            told: vec![],
            held: None,
            command_wake: 0,
            command_cost: None,
            prior_cost_incomplete: false,
            stopping: None,
            ready: vendor == Vendor::Claude,
            note_turn: None,
            early: vec![],
            lead_call: (self.team_of(conv, call).is_none()
                || self.live.get(conv).is_some_and(|l| l.open_tools.iter().any(|id| id == call)))
            .then(|| call.to_string()),
            question: None,
            question_timed: false,
            sent: 0,
            reviewer: review.is_some(),
        };
        self.mark_helper_seen(call);
        // What the person wrote while the helper was still starting (the box shows from its call on) is first in line.
        if let Some(early) = self.helper_early.remove(call) {
            run.waiting.extend(early.iter().map(|(id, _)| id.clone()));
            run.early = early;
        }
        if let Some(old) = self.helpers.insert(call.to_string(), run) {
            self.rewind_helper_retire(call, old.hsid);
            old.session.close();
        }
        self.expire_helper(call, hsid, MAX_RUN, "it ran out of time");
        self.flush_early(call);
        if self.helpers.values().any(|h| h.conv == conv && h.question.is_some()) {
            self.release_team_calls(conv, call);
            self.start_team_timers(conv);
        }
        Ok(())
    }

    fn init_team(&mut self, conv: &str, call: &str, input: &serde_json::Value) {
        if self.team_of(conv, call).is_some() {
            return;
        }
        let job = helper::Piece::of(input).job;
        let title = input["title"].as_str().unwrap_or("A piece of work").to_string();
        let number = self.turn(conv).map_or(1, |m| {
            m.tools
                .iter()
                .flatten()
                .filter(|t| matches!(t.name.as_str(), "Helper" | "Agent" | "Task" | "Review"))
                .position(|t| t.id == call)
                .unwrap_or(0)
                + 1
        });
        let name = self.new_team_name(conv, &title, number);
        if let Some(tool) = self.tool_of(conv, call) {
            tool.team = Some(crate::model::HelperTeam {
                name,
                title,
                job: job.into(),
                files: helper::brief_files(input["brief"].as_str().unwrap_or_default()),
                state: crate::model::TeamState::Running,
                messages: vec![],
            });
        }
        self.change_team(conv, call, |_| {});
    }

    /// Remembers a call that started or failed to start; only recent ones matter, so the set stays small.
    fn mark_helper_seen(&mut self, call: &str) {
        self.helper_seen.insert(call.to_string());
        if self.helper_seen.len() > 256 {
            self.helper_seen.retain(|seen| seen == call || self.helpers.contains_key(seen));
        }
    }

    /// The helper can take messages now: what the person wrote before goes in, in order.
    fn flush_early(&mut self, call: &str) {
        let screen = Some(self.screen_code());
        let Some(h) = self.helpers.get_mut(call) else { return };
        if h.ready || h.vendor == Vendor::Claude {
            for (id, text) in std::mem::take(&mut h.early) {
                let text = if id.starts_with("team:") { text } else { from_person(&text, screen) };
                h.session.steer(UserTurn::text(text), &id);
            }
        }
    }

    /// A helper whose call shows but whose CLI is not at work yet (waiting for memory or a check of its tools, or starting).
    fn helper_starting(&self, conv: &str, call: &str) -> bool {
        !self.helper_seen.contains(call)
            && self.convs.get(conv).is_some_and(|c| {
                c.messages
                    .iter()
                    .rev()
                    .flat_map(|m| m.tools.iter().flatten())
                    .any(|t| t.id == call && t.name == "Helper" && t.ended.is_none())
            })
    }

    /// The person wrote to a helper at work (R161 step 5). Codex takes the message into its running turn; Claude Code reads it
    /// at its next step, or as a turn of its own right after the one that is ending. The message shows on the helper's call.
    pub(super) fn tell_helper(&mut self, conv: &str, call: &str, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("error.helperEmpty".into());
        }
        self.rewind.bump(conv);
        let id = new_id();
        let running = self.helpers.get(call).is_some_and(|h| h.conv == conv);
        let starting = self.helper_starting(conv, call);
        if !running && !starting {
            return Err("error.helperDone".into());
        }
        self.mark_told(conv, call, |told| told.push(Told { id: id.clone(), text: text.to_string(), taken: None }));
        if running {
            self.deliver_helper_note(call, id, text.into());
        } else {
            self.helper_early.entry(call.to_string()).or_default().push((id, text.into()));
        }
        Ok(())
    }

    fn deliver_helper_note(&mut self, call: &str, id: String, text: String) -> bool {
        let h = &self.helpers[call];
        let waiting_for_command = h.held.as_ref().is_some_and(|e| e.wait != HeldBy::Message);
        let new_turn = h.vendor == Vendor::Codex && waiting_for_command;
        let screen = self.screen_code();
        self.helpers.get_mut(call).unwrap().waiting.push(id.clone());
        if new_turn && let Some(reason) = self.helper_wake_refusal(call) {
            self.end_helper(call, false, Some(reason), None, true);
            return false;
        }
        let h = self.helpers.get_mut(call).unwrap();
        if new_turn || (h.ready && !waiting_for_command) {
            let text = if id.starts_with("team:") { text } else { from_person(&text, Some(screen)) };
            let turn = UserTurn::text(text);
            if new_turn {
                h.held = None;
                h.ready = false;
                h.note_turn = Some(id);
                h.session.send(turn);
            } else {
                h.session.steer(turn, &id);
            }
        } else {
            h.early.push((id, text));
        }
        true
    }

    /// Changes what the person wrote to a helper, on its call in the conversation, and shows it.
    fn mark_told(&mut self, conv: &str, call: &str, change: impl FnOnce(&mut Vec<Told>)) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(m) = c.messages.iter_mut().rev().find(|m| m.tools.iter().flatten().any(|t| t.id == call)) else { return };
        let Some(tool) = m.tools.iter_mut().flatten().find(|t| t.id == call) else { return };
        change(&mut tool.told);
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// A message to the helper reached it, or did not: `taken` says which, and `tag` names it when the CLI echoes it.
    fn helper_took(&mut self, call: &str, tag: Option<String>, taken: bool) {
        let Some(h) = self.helpers.get_mut(call) else { return };
        let at = match tag {
            Some(tag) => h.waiting.iter().position(|id| *id == tag),
            None => (h.waiting.len() > h.early.len()).then_some(0),
        };
        let Some(at) = at else { return };
        let id = h.waiting.remove(at);
        let conv = h.conv.clone();
        let mut words = None;
        self.mark_told(&conv, call, |told| {
            if let Some(t) = told.iter_mut().find(|t| t.id == id) {
                t.taken = Some(taken);
                words = Some(t.text.clone());
            }
        });
        let Some(h) = self.helpers.get_mut(call) else { return };
        if taken {
            h.told.extend(words);
            // A turn that ended before this message was read is not the last: the message is a turn of its own now.
            h.held = None;
        } else if h.waiting.len() == h.early.len() {
            if h.held.as_ref().is_some_and(|e| e.wait != HeldBy::Message) {
                let hsid = h.hsid;
                self.helper_command_ended(call, hsid);
            } else if h.waiting.is_empty()
                && let Some(e) = h.held.take()
            {
                self.end_helper(call, e.ok, e.error.as_deref(), e.cost, e.incomplete);
            }
        }
    }

    /// What a helper's own session reports. Its tools show inside the agent's call; nobody answers its approvals, because the
    /// person sees only the agent: the agent hears what was refused and can do it itself, where the person approves it.
    fn helper_event(&mut self, call: &str, hsid: u64, event: EngineEvent) {
        if self.helpers.get(call).is_some_and(|h| h.hsid == hsid)
            && let EngineEvent::TeamCall { id, input } = &event
        {
            return self.team_call(call, id, input);
        }
        self.rewind_helper_event(call, hsid, &event);
        if let EngineEvent::Background(update) = event {
            let ended = matches!(update, crate::background::Update::End { .. });
            let owner = self
                .helpers
                .get(call)
                .filter(|h| h.hsid == hsid)
                .map(|h| (h.conv.clone(), h.vendor, h.session.clone()))
                .or_else(|| self.background.owner(hsid));
            let changed = if let Some((conv, vendor, session)) = owner {
                self.background.update(&conv, hsid, vendor, &session, update, now())
            } else {
                self.background.late_update(None, hsid, &update);
                false
            };
            if changed {
                self.emit_background();
                self.refresh_running();
                if ended {
                    self.helper_command_ended(call, hsid);
                }
            }
            return;
        }
        if matches!(event, EngineEvent::WriterExit { confirmed: true })
            && let Some((conv, ..)) = self.background.owner(hsid)
            && self.background.exited(&conv, hsid, now())
        {
            self.emit_background();
            self.refresh_running();
        }
        if !self.helpers.get(call).is_some_and(|h| h.hsid == hsid) {
            return;
        }
        if matches!(event, EngineEvent::OwnTurn) && self.helpers[call].held.as_ref().is_some_and(|e| e.wait != HeldBy::Message) {
            if let Some(reason) = self.helper_wake_refusal(call) {
                self.end_helper(call, false, Some(reason), None, true);
            } else {
                self.helpers.get_mut(call).unwrap().held = None;
                self.flush_early(call);
            }
            return;
        }
        // Writer/cursor metadata accompanies streamed tokens; only content boundaries flush the buffer.
        if matches!(
            &event,
            EngineEvent::Tool { .. }
                | EngineEvent::ChildWords { .. }
                | EngineEvent::ToolResult { .. }
                | EngineEvent::Break { .. }
                | EngineEvent::Permission { .. }
                | EngineEvent::Injected { .. }
                | EngineEvent::SteerFailed { .. }
                | EngineEvent::Settled
                | EngineEvent::MainIdle
                | EngineEvent::Done { .. }
                | EngineEvent::Error { .. }
        ) {
            self.flush_helper_words(call);
        }
        if matches!(event, EngineEvent::TurnStarted)
            && let Some(h) = self.helpers.get_mut(call).filter(|h| h.hsid == hsid)
        {
            h.ready = true;
            if let Some(tag) = h.note_turn.take() {
                self.helper_took(call, Some(tag), true);
            }
            self.flush_early(call);
        }
        if let EngineEvent::Done { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens, .. } = &event
            && let Some(conv) = self.helpers.get(call).filter(|h| h.hsid == hsid).map(|h| h.conv.clone())
            && let Some(m) = self.turn(&conv)
        {
            m.input_tokens = Some(m.input_tokens.unwrap_or(0).saturating_add(*input_tokens));
            m.output_tokens = Some(m.output_tokens.unwrap_or(0).saturating_add(*output_tokens));
            m.cache_read_tokens = Some(m.cache_read_tokens.unwrap_or(0).saturating_add(*cache_read_tokens));
            m.cache_write_tokens = Some(m.cache_write_tokens.unwrap_or(0).saturating_add(*cache_write_tokens));
            m.cache_write_1h_tokens = Some(m.cache_write_1h_tokens.unwrap_or(0).saturating_add(*cache_write_1h_tokens));
        }
        let Some(h) = self.helpers.get_mut(call).filter(|h| h.hsid == hsid) else { return };
        let (conv, sid, vendor) = (h.conv.clone(), h.sid, h.vendor);
        match event {
            EngineEvent::Tool { name, .. } if matches!(name.as_str(), helper::TEAM_NAME | helper::TEAM_CLAUDE_NAME) => {}
            EngineEvent::Tool { id, name, input, parent } => {
                self.handle(&conv, sid, EngineEvent::Tool { id, name, input, parent: parent.or_else(|| Some(call.to_string())) })
            }
            EngineEvent::ChildWords { .. } => self.handle(&conv, sid, event),
            EngineEvent::ToolResult { .. } => self.handle(&conv, sid, event),
            EngineEvent::Break { .. } => h.new_block = true,
            EngineEvent::Text(delta) => {
                if std::mem::take(&mut h.new_block) && !delta.is_empty() {
                    h.report.clear();
                    h.kept = 0;
                }
                h.report.push_str(&delta);
                if h.report.len().saturating_sub(h.kept) >= crate::transcript::MAX_TEXT_CHUNK_BYTES {
                    self.flush_helper_words(call);
                }
            }
            EngineEvent::Permission { request_id, tool, .. } => {
                drop(h.session.answer(&request_id, false));
                if !h.refused.contains(&tool) {
                    h.refused.push(tool);
                }
            }
            EngineEvent::Injected { tag } => self.helper_took(call, tag, true),
            EngineEvent::SteerFailed { tag } => self.helper_took(call, Some(tag), false),
            EngineEvent::Quota { window, used, resets_at } => self.note_quota(vendor, &window, used, resets_at),
            EngineEvent::Signals(s) => self.apply_signals(vendor, s),
            EngineEvent::WriterExit { confirmed } => {
                let reason = match (confirmed, h.stopping.is_some()) {
                    (true, true) => "the helper process ended before reporting its result",
                    (true, false) => "the helper process ended before reporting its result; the stop cause is unknown",
                    (false, _) => "the helper session ended before reporting its result; writer exit is unconfirmed",
                };
                self.end_helper(call, false, Some(reason), None, true);
            }
            EngineEvent::Done {
                ok,
                cost_total_usd,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cache_write_1h_tokens,
                incomplete,
                reason,
            } => {
                let model = h.model.clone();
                // A helper's session is new, so Claude's running total is the cost of its turns so far; Codex prices each turn,
                // and a message steered into a Codex turn stays in it. A price from tokens covers only the last turn.
                let total = cost_total_usd.filter(|c| c.is_finite() && *c >= 0.0);
                let cumulative = vendor == Vendor::Claude && total.is_some();
                let cost = total.map(|sum| if cumulative { (sum - h.command_cost.unwrap_or(0.0)).max(0.0) } else { sum }).or_else(|| {
                    usage::api_price_for_prompt(
                        &model,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        cache_write_1h_tokens,
                        0,
                    )
                });
                let incomplete = incomplete || cost.is_none() || (h.prior_cost_incomplete && !cumulative);
                let error = (!ok).then_some(reason);
                if ok && h.stopping.is_none() && self.background.running_for(hsid) {
                    (h.new_block, h.prior_cost_incomplete) = (true, incomplete);
                    if let Some(cost) = cost {
                        h.command_cost = Some(h.command_cost.unwrap_or(0.0) + cost);
                    }
                    h.held = Some(Ended { ok, error, cost: None, incomplete, wait: HeldBy::Command(None) });
                    if self.background.end_turn(&conv, hsid) {
                        self.emit_background();
                    }
                    self.add_spend(&conv, cost);
                    self.save(&conv);
                    return;
                }
                // A message to the helper still on its way: this end waits to see whether another turn follows.
                if let Some(h) = self.helpers.get_mut(call).filter(|h| !h.waiting.is_empty() && h.stopping.is_none()) {
                    (h.new_block, h.prior_cost_incomplete) = (true, true);
                    h.held = Some(Ended { ok, error, cost, incomplete, wait: HeldBy::Message });
                    return;
                }
                self.end_helper(call, ok, error.as_deref(), cost, incomplete);
            }
            // A turn Codex refused to start never ends on its own: it is the helper's end too.
            EngineEvent::Error { message, fatal } if fatal || message.starts_with("Codex turn/start failed") => {
                self.end_helper(call, false, Some(&message), None, true)
            }
            _ => {}
        }
    }

    fn helper_wake_refusal(&mut self, call: &str) -> Option<&'static str> {
        let h = self.helpers.get(call)?;
        let (conv, vendor, model) = (h.conv.clone(), h.vendor, h.model.clone());
        if self.closing || h.stopping.is_some() || self.convs.get(&conv).is_none_or(|c| c.stopped || c.archived_at.is_some()) {
            Some("the calling conversation closed, stopped or was archived")
        } else if self.goal_out_of_money(&conv) {
            Some("the project's goal reached its spending limit before the helper could continue")
        } else if self.allows(vendor, family_of(&model), Purpose::UserTurn { by_hand: false }, false).is_err() {
            Some("account or quota evidence did not allow the helper to continue")
        } else {
            None
        }
    }

    fn helper_command_ended(&mut self, call: &str, hsid: u64) {
        if self.background.running_for(hsid) {
            return;
        }
        let Some(h) = self.helpers.get_mut(call).filter(|h| h.hsid == hsid && h.waiting.len() == h.early.len() && h.stopping.is_none())
        else {
            return;
        };
        let Some(e) = h.held.as_mut().filter(|e| e.wait == HeldBy::Command(None)) else { return };
        h.command_wake += 1;
        let wake = h.command_wake;
        e.wait = HeldBy::Command(Some(wake));
        let grace = if h.vendor == Vendor::Claude { keep_going::COMMAND_OWN } else { keep_going::COMMAND_GRACE };
        let delay = Duration::from_secs_f64(grace * self.keep_pace);
        let (me, call) = (self.me.clone(), call.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).wake_helper_command(&call, hsid, wake);
            }
        });
    }

    fn wake_helper_command(&mut self, call: &str, hsid: u64, wake: u64) {
        let Some(h) = self.helpers.get_mut(call).filter(|h| h.hsid == hsid) else { return };
        let Some(e) = h.held.as_mut().filter(|e| e.wait == HeldBy::Command(Some(wake))) else { return };
        if h.waiting.len() > h.early.len() || self.background.running_for(hsid) {
            e.wait = HeldBy::Command(None);
            return;
        }
        // A late native Claude turn could race a second sent turn. Return control to the caller if the native wake is absent.
        let missing = (h.vendor == Vendor::Claude).then_some("the command ended but the helper's CLI did not start a follow-up");
        let incomplete = e.incomplete;
        if let Some(reason) = self.helper_wake_refusal(call).or(missing) {
            self.end_helper(call, false, Some(reason), None, incomplete);
            return;
        }
        if let Some(h) = self.helpers.get_mut(call) {
            h.held = None;
            h.ready = false;
            h.session.send(UserTurn::text(by_itself::OWN_TURN));
        }
    }

    fn flush_helper_words(&mut self, call: &str) {
        let Some(h) = self.helpers.get_mut(call) else { return };
        if h.kept == h.report.len() {
            return;
        }
        let (conv, text) = (h.conv.clone(), h.report[h.kept..].to_string());
        h.kept = h.report.len();
        self.keep_child_event(&conv, call, crate::transcript::TranscriptKind::Text, None, &text);
    }

    fn end_helper(&mut self, call: &str, ok: bool, error: Option<&str>, cost: Option<f64>, incomplete: bool) {
        self.answer_team(call, helper::NO_ANSWER, true);
        self.flush_helper_words(call);
        let Some(h) = self.helpers.remove(call) else { return };
        if self.background.end_turn(&h.conv, h.hsid) {
            self.emit_background();
        }
        self.rewind_helper_retire(call, h.hsid);
        h.session.close();
        self.add_spend(&h.conv, cost);
        let cost = h.command_cost.map(|spent| spent + cost.unwrap_or(0.0)).or(cost);
        self.note_span(h.vendor, h.started, &h.model, cost, incomplete);
        let reason = h.stopping.map(|why| match error {
            Some(error) => format!("{why}; helper result: {error}"),
            None => why.to_string(),
        });
        let error = reason.as_deref().or(error);
        let (ok, mut text) = helper::report(ok && h.stopping.is_none(), &h.report, &h.refused, error);
        if let Some(live) = self.live.get(&h.conv) {
            let long = text.chars().count() > 2000;
            text = helper::short_report(&live.cwd, &text, ok);
            if long
                && !ok
                && let Some(reason) = error
            {
                text.push_str(&format!("\nFailure reason: {}", reason.chars().take(300).collect::<String>()));
            }
            if long && !h.refused.is_empty() {
                let refused = h.refused.join(", ");
                text.push_str(&format!("\nThe helper was refused approval for: {}.", refused.chars().take(200).collect::<String>()));
            }
        }
        if !ok && !h.reviewer {
            text.push_str("\n\nFor interrupted edits, inspect the same folder before recovery. If available, use conversation with action workspace for the current project copy and observed work. Confirm writer exit separately. Describe work as running only when there is current evidence.");
        }
        if !h.told.is_empty() {
            text.push_str(&helper::told_note(&h.told));
        }
        if !h.early.is_empty() {
            text.push_str("\n\nThe helper ended before receiving its queued notes. Their text is still in the conversation.");
        }
        self.never_read(&h.conv, call, &h.waiting);
        if self.review_ended(&h.conv, call, ok, &h.report, false) {
            if !ok {
                text = "error.review.failed".into();
            }
            self.handle(&h.conv, h.sid, EngineEvent::ToolResult { id: call.to_string(), ok, declined: false, exit_code: None, text });
        } else {
            self.record_helper_result(&h.conv, h.sid, call, ok, &text);
            if let Some(l) = self.live.get(&h.conv).filter(|l| l.sid == h.sid && l.turn.is_some())
                && let Some(pending) = h.lead_call.as_deref()
            {
                let delivered = l.session.helper_reply_delivered(pending, ok, text.clone());
                // A reply the CLI never took (write failed, call gone, process ended) resumes the caller once, durably.
                let (me, conv, call, sid, hsid, turn) = (self.me.clone(), h.conv.clone(), call.to_string(), h.sid, h.hsid, l.turn.clone());
                self.opts.runtime.spawn(async move {
                    // Bound the ACK independently: a queued command can retain its sender without writing.
                    if matches!(tokio::time::timeout(Duration::from_secs(35), delivered).await, Ok(Ok(crate::events::Delivery::Delivered)))
                    {
                        return;
                    }
                    let Some(core) = me.upgrade() else { return };
                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(live) = core.live.get(&conv).filter(|l| l.sid == sid && l.turn.is_some() && l.turn == turn) {
                        live.session.interrupt();
                    }
                    core.resume_helper_caller(&conv, sid, &call, hsid, text);
                });
            } else {
                self.resume_helper_caller(&h.conv, h.sid, call, h.hsid, text);
            }
        }
        self.change_team(&h.conv, call, |team| team.state = crate::model::TeamState::Ended);
        if let Some(m) = self.turn(&h.conv) {
            if incomplete {
                m.incomplete = Some(true);
            }
            if let Some(cost) = cost {
                m.cost_usd = Some(m.cost_usd.unwrap_or(0.0) + cost);
            }
        }
        self.save(&h.conv);
    }

    /// The caller's turn cannot take the helper's result: it comes back as the caller's next turn by itself, which waits
    /// behind a Stop (`send_by_itself`); at shutdown a finite saved wait restores that return.
    fn resume_helper_caller(&mut self, conv: &str, sid: u64, call: &str, hsid: u64, text: String) {
        if !self.convs.contains_key(conv) {
            return;
        }
        let receipt = format!("helper-report-{call}-{hsid}");
        if self.closing {
            let c = self.convs.get_mut(conv).unwrap();
            if c.waiting_on.is_none() && !c.queued.iter().any(|q| q.id == receipt) && !c.messages.iter().any(|m| m.id == receipt) {
                let at = now();
                c.waiting_on = Some(crate::model::WaitingOn {
                    storage_blocked: false,
                    ids: vec![],
                    note: format!(
                        "Helper {} ended while the engine is shutting down. Read its saved tool result for the observed outcome.",
                        call.chars().take(64).collect::<String>()
                    ),
                    since: at,
                    until: at,
                    problem: None,
                    pr: None,
                });
                self.save(conv);
            }
            return;
        }
        if let Err(why) = self.send_by_itself_with_id(conv, text.clone(), crate::model::Wake::of("wake.childDone"), receipt) {
            // The retained result names the refusal even if the conversation cannot accept a continuation.
            let report = format!("{text}\n\nThe helper ended, but its caller could not be resumed: {why}.");
            self.record_helper_result(conv, sid, call, false, &report);
            self.save(conv);
        }
    }

    /// A wrapper result cannot override the actual worker's outcome, including after restart.
    fn record_helper_result(&mut self, conv: &str, sid: u64, call: &str, ok: bool, text: &str) {
        if let Some(tool) = self.tool_of(conv, call) {
            tool.ok = Some(ok);
            tool.ended = Some(now());
            tool.output = Some(text.chars().take(TOOL_OUTPUT_CHARS).collect());
        }
        if self.live.get(conv).is_some_and(|live| live.sid == sid) {
            self.handle(conv, sid, EngineEvent::HelperDone { id: call.to_string(), ok, text: text.to_string() });
        } else {
            self.keep_child_event(conv, call, crate::transcript::TranscriptKind::Result, Some(call), text);
            self.settle_facts(conv, call);
            self.watch_ended(conv, call);
        }
    }

    /// A message still on its way when the helper ended never reached it.
    fn never_read(&mut self, conv: &str, call: &str, waiting: &[String]) {
        if !waiting.is_empty() {
            self.mark_told(conv, call, |told| {
                for t in told.iter_mut().filter(|t| waiting.contains(&t.id)) {
                    t.taken = Some(false);
                }
            });
        }
    }

    /// Ends the helper `hsid` after `after` unless it has ended by itself, so the agent waiting on it always hears back.
    fn expire_helper(&self, call: &str, hsid: u64, after: Duration, why: &'static str) {
        let (me, call) = (self.me.clone(), call.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(after).await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.helpers.get(&call).is_some_and(|h| h.hsid == hsid) {
                core.end_helper(&call, false, Some(why), None, true);
            }
        });
    }

    /// Whether a helper of this vendor is at work: its quota is being spent now.
    pub(super) fn helper_running(&self, vendor: Vendor) -> bool {
        self.helpers.values().any(|h| h.vendor == vendor)
    }

    /// A helper is never picked by hand nor approved for paid use, so it stops as soon as its vendor may bill beyond the
    /// subscription (R86), and the agent hears why.
    pub(super) fn stop_paid_helpers(&mut self, vendor: Vendor) {
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.vendor == vendor).map(|(call, _)| call.clone()).collect();
        for call in calls {
            self.end_helper(&call, false, Some("the other AI would be billed beyond its subscription"), None, true);
        }
    }

    /// A helper may outlive its lead's reply. Retire sessions whose saved launch authority was narrowed.
    pub(super) fn stop_helpers_for_settings(&mut self, permissions: Permissions, web: WebSearch) {
        let stale: Vec<String> = self
            .helpers
            .iter()
            .filter(|(_, h)| h.permissions.strictness() < permissions.strictness() || h.web.strictness() < web.strictness())
            .map(|(call, _)| call.clone())
            .collect();
        for call in stale {
            self.end_helper(&call, false, Some("the person reduced the helper's permissions or web access"), None, true);
        }
    }

    /// The engine is closing: every helper stops, and what it spent so far is noted as unknown.
    pub(super) fn close_helpers(&mut self) {
        let convs: HashSet<String> = self.helpers.values().map(|h| h.conv.clone()).collect();
        for conv in convs {
            self.stop_helpers(&conv, true);
        }
    }

    /// The agent's turn stopped or ended: its helpers stop with it. An interrupted helper still reports what it had.
    pub(super) fn stop_helpers(&mut self, conv: &str, close: bool) {
        let why = if self.closing {
            "the engine is shutting down"
        } else if close {
            "the calling session is closing or its turn ended"
        } else {
            "the calling turn ended during plan approval"
        };
        self.stop_helpers_because(conv, close, why);
    }

    /// Release controls whose calling turn cannot take another result; the actual worker still owns its final report.
    fn release_helper_calls(&mut self, conv: &str, why: &str, all: bool) {
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.conv == conv).map(|(call, _)| call.clone()).collect();
        for call in calls {
            let h = self.helpers.get_mut(&call).unwrap();
            if !all && h.lead_call.as_deref().is_none_or(|pending| pending == call) {
                continue;
            }
            if let Some(pending) = h.lead_call.take()
                && let Some(live) = self.live.get_mut(conv).filter(|l| l.sid == h.sid)
            {
                live.open_tools.retain(|id| id != &pending);
                if pending != call {
                    live.session.helper_reply(
                        &pending,
                        false,
                        format!("The caller stopped waiting: {why}. The helper's final report is retained separately."),
                    );
                }
            }
        }
    }

    pub(super) fn finish_helper_calls(&mut self, conv: &str) {
        self.release_helper_calls(conv, "the calling turn ended; the actual helper continues", true);
        let asking: Vec<String> =
            self.helpers.iter().filter(|(_, h)| h.conv == conv && h.question.is_some()).map(|(call, _)| call.clone()).collect();
        for call in asking {
            self.answer_team(&call, helper::NO_ANSWER, true);
        }
        let calls = self.working_helpers(conv);
        if !calls.is_empty() && self.convs.get(conv).is_some_and(|c| c.waiting_on.is_none()) {
            let since = (self.secs() * 1000.0) as u64;
            let c = self.convs.get_mut(conv).unwrap();
            c.waiting_on = Some(crate::model::WaitingOn {
                storage_blocked: false,
                ids: vec![],
                note: calls.join(", ").chars().take(200).collect(),
                since,
                until: since.saturating_add((300_000.0 * self.keep_pace).max(1.0) as u64),
                problem: c.stopped.then(|| "reason.wait.stopped".into()),
                pr: None,
            });
            self.save(conv);
            self.arm_wait(conv);
            self.list_changed();
        }
    }

    pub(super) fn stop_helpers_because(&mut self, conv: &str, close: bool, why: &'static str) {
        self.release_helper_calls(conv, why, close);
        self.reviews.retain(|_, run| run.conv != conv);
        self.cancel_helper_workers(conv);
        self.kit_checks.retain(|name, check| !name.starts_with("helper:") || check.owner != conv);
        // The checks it was waiting for stop too (`verifying.rs`).
        self.stop_verify(conv);
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.conv == conv).map(|(call, _)| call.clone()).collect();
        for call in calls {
            if close
                || self
                    .helpers
                    .get(&call)
                    .is_some_and(|h| h.question.is_some() || h.held.as_ref().is_some_and(|e| e.wait != HeldBy::Message))
            {
                if let Some(h) = self.helpers.get_mut(&call) {
                    h.stopping = Some(why);
                }
                self.end_helper(&call, false, Some(why), None, true);
                continue;
            }
            if let Some(h) = self.helpers.get_mut(&call).filter(|h| h.stopping.is_none()) {
                h.stopping = Some(why);
                h.session.interrupt();
                // A helper whose turn had not begun cannot be interrupted: it ends here if it has not reported by then.
                let hsid = h.hsid;
                self.expire_helper(
                    &call,
                    hsid,
                    STOP_GRACE,
                    "the calling conversation stopped or ended and the helper did not finish within ten seconds",
                );
            }
        }
    }
}

/// The person's words to a helper, framed as theirs, with the note naming the language the helper writes back in.
fn from_person(text: &str, screen: Option<&str>) -> String {
    super::reply_language::with_note(helper::from_person(text), text, screen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionCommand;

    const UNCONFIRMED_EXIT: u8 = 8;
    const BACKGROUND_COMMAND: u8 = 9;
    const BACKGROUND_NO_ACCOUNT: u8 = 10;
    const BACKGROUND_UNPRICED: u8 = 11;
    const BACKGROUND_PERMISSIONS: u8 = 12;
    const BACKGROUND_MESSAGE: u8 = 13;

    async fn helper_exit(vendor: Vendor, partial: bool, interrupt: bool, stop_reason: &'static str, lifecycle: u8, long_told: bool) {
        let delivered_directly = matches!(lifecycle, 0 | UNCONFIRMED_EXIT);
        let exit_reason = if lifecycle == UNCONFIRMED_EXIT {
            "writer exit is unconfirmed"
        } else {
            "the helper process ended before reporting its result"
        };
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let replies = Arc::new(Mutex::new(Vec::new()));
        let sent = replies.clone();
        let retained = Arc::new(Mutex::new(None));
        let held = retained.clone();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_caller = interrupted.clone();
        let caller = Session::from_fn(move |command| {
            if matches!(command, SessionCommand::Interrupt) {
                stop_caller.store(true, std::sync::atomic::Ordering::Release);
            }
            if let SessionCommand::HelperReply { id, ok, text, ack, .. } = command {
                sent.lock().unwrap().push((id, ok, text));
                // Lifecycle 3 is a CLI that never took the line: its acknowledgement is dropped.
                if lifecycle == 5 || lifecycle == 7 {
                    *held.lock().unwrap() = ack;
                } else if lifecycle != 3
                    && let Some(ack) = ack
                {
                    let _ = ack.send(crate::events::Delivery::Delivered);
                }
            }
        });
        let conv = super::super::answer::tests::seed(&engine, caller, &[("turn", &[])]);
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = closed.clone();
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sends = wakes.clone();
        let session = Session::from_fn(move |command| {
            if matches!(command, SessionCommand::Close) {
                stopped.store(true, std::sync::atomic::Ordering::Release);
            }
            if matches!(command, SessionCommand::Send(_)) {
                sends.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            engine.with(|core| {
                let sid = core.live[&conv].sid;
                core.live.get_mut(&conv).unwrap().vendor = helper::other(vendor);
                core.handle(
                    &conv,
                    sid,
                    EngineEvent::Tool {
                        id: "call".into(),
                        name: "Helper".into(),
                        input: serde_json::json!({"title":"Check", "brief":"Check the files"}),
                        parent: None,
                    },
                );
                core.turn(&conv).unwrap().cost_usd = Some(0.01);
                core.helpers.insert(
                    "call".into(),
                    Run {
                        conv: conv.clone(),
                        sid,
                        hsid: 42,
                        vendor,
                        model: "unknown-test-model".into(),
                        permissions: Permissions::FullAccess,
                        web: WebSearch::Live,
                        session,
                        started: core.secs(),
                        report: String::new(),
                        kept: 0,
                        new_block: false,
                        refused: vec![],
                        waiting: vec!["pending-message".into()],
                        told: if long_told { vec!["Keep the retries bounded.".into()] } else { vec![] },
                        held: None,
                        command_wake: 0,
                        command_cost: None,
                        prior_cost_incomplete: false,
                        stopping: None,
                        ready: true,
                        note_turn: None,
                        early: vec![],
                        lead_call: Some("call".into()),
                        question: None,
                        question_timed: false,
                        sent: 0,
                        reviewer: false,
                    },
                );
                if !partial {
                    core.handle(
                        &conv,
                        sid,
                        EngineEvent::ToolResult {
                            id: "call".into(),
                            ok: true,
                            declined: false,
                            exit_code: None,
                            text: "wrapper returned".into(),
                        },
                    );
                }
                if partial {
                    core.helper_event("call", 42, EngineEvent::Text("Keep these partial words.".into()));
                }
                if long_told {
                    core.live.get_mut(&conv).unwrap().cwd = dir.path().into();
                    core.helper_event("call", 42, EngineEvent::Text("Long report. ".repeat(250)));
                }
                if matches!(
                    lifecycle,
                    BACKGROUND_COMMAND | BACKGROUND_NO_ACCOUNT | BACKGROUND_UNPRICED | BACKGROUND_PERMISSIONS | BACKGROUND_MESSAGE
                ) {
                    core.helpers.get_mut("call").unwrap().waiting.clear();
                    core.ledger.of_mut(vendor).billing = crate::budget::Billing::Subscription;
                    let unpriced = lifecycle == BACKGROUND_UNPRICED;
                    let done = |cost| EngineEvent::Done {
                        ok: true,
                        reason: String::new(),
                        cost_total_usd: cost,
                        input_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cache_write_1h_tokens: 0,
                        incomplete: false,
                    };
                    let start = |id: &str| crate::background::Update::Start {
                        id: id.into(),
                        command: "run project checks".into(),
                        stoppable: true,
                        turn_ended: false,
                    };
                    let end = |id: &str| {
                        EngineEvent::Background(crate::background::Update::End { id: id.into(), state: crate::background::State::Done })
                    };
                    let parent = core.live[&conv].session.clone();
                    core.background.update(&conv, sid, helper::other(vendor), &parent, start("check"), now());
                    core.helper_event("call", 42, EngineEvent::Background(start("check")));
                    core.helper_event("call", 42, EngineEvent::Background(start("check-again")));
                    core.helper_event("call", 42, EngineEvent::Text("The check is still running.".into()));
                    core.helper_event("call", 42, done((!unpriced).then_some(0.02)));
                    assert!(core.helpers.contains_key("call"), "a completed reply is not a completed helper while its command runs");
                    assert!(!closed.load(std::sync::atomic::Ordering::Acquire));
                    assert!(replies.lock().unwrap().is_empty(), "the caller waits for the actual result");
                    assert_eq!(core.convs[&conv].spent_usd, if unpriced { 0.0 } else { 0.02 }, "known first turn cost precedes a new turn");
                    core.helper_event("call", 42, end("check"));
                    assert!(core.helpers["call"].held.as_ref().is_some_and(|e| e.wait == HeldBy::Command(None)));
                    core.helper_event("call", 42, end("check-again"));
                    let wake = core.helpers["call"].command_wake;
                    assert!(core.helpers["call"].held.as_ref().is_some_and(|e| e.wait == HeldBy::Command(Some(wake))));
                    if lifecycle == BACKGROUND_MESSAGE {
                        core.tell_helper(&conv, "call", "Keep the original test result.").unwrap();
                        if vendor == Vendor::Claude {
                            let h = &core.helpers["call"];
                            let tag = h.waiting[0].clone();
                            assert_eq!(h.early.len(), 1, "keep a Claude note until its own turn begins");
                            core.helper_event("call", 42, EngineEvent::OwnTurn);
                            assert!(core.helpers["call"].early.is_empty());
                            core.helper_event("call", 42, EngineEvent::Injected { tag: Some(tag) });
                        } else {
                            assert_eq!(
                                wakes.load(std::sync::atomic::Ordering::Relaxed),
                                1,
                                "an idle Codex thread needs a turn to read the note"
                            );
                            core.helper_event("call", 42, EngineEvent::TurnStarted);
                        }
                        assert!(core.tool_of(&conv, "call").unwrap().told.iter().any(|note| note.taken == Some(true)));
                    }
                    if interrupt || matches!(lifecycle, BACKGROUND_NO_ACCOUNT | BACKGROUND_PERMISSIONS) {
                        if interrupt {
                            core.stop_helpers_because(&conv, false, stop_reason);
                        } else if lifecycle == BACKGROUND_PERMISSIONS {
                            core.turn(&conv).unwrap().status = Some(Status::Done);
                            core.finish(&conv, Some("turn".into()));
                            assert!(core.helpers.contains_key("call"), "the helper was still waiting after its lead finished");
                            core.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
                        } else {
                            core.ledger.of_mut(vendor).billing = crate::budget::Billing::Unknown;
                        }
                        core.wake_helper_command("call", 42, wake);
                        assert!(core.helpers.is_empty());
                        assert!(closed.load(std::sync::atomic::Ordering::Acquire));
                        assert_eq!(wakes.load(std::sync::atomic::Ordering::Relaxed), 0);
                        let reports = replies.lock().unwrap();
                        if lifecycle == BACKGROUND_PERMISSIONS {
                            assert!(reports.is_empty(), "the previous lead turn has already ended");
                            let report = core.tool_of(&conv, "call").unwrap();
                            assert!(report.output.as_ref().is_some_and(|words| words.contains(stop_reason)));
                        } else {
                            assert_eq!(reports.len(), 1);
                            assert!(!reports[0].1 && reports[0].2.contains(stop_reason));
                        }
                        assert_eq!(core.convs[&conv].spent_usd, 0.02);
                        return;
                    }
                    if vendor == Vendor::Claude {
                        core.helper_event("call", 42, EngineEvent::OwnTurn);
                    }
                    core.wake_helper_command("call", 42, wake);
                    core.wake_helper_command("call", 42, wake);
                    assert_eq!(wakes.load(std::sync::atomic::Ordering::Relaxed), usize::from(vendor == Vendor::Codex));
                    core.helper_event("call", 42, EngineEvent::Text("The check passed.".into()));
                    core.helper_event("call", 42, done(Some(0.03)));
                    assert!(core.helpers.is_empty());
                    assert!(closed.load(std::sync::atomic::Ordering::Acquire));
                    let reports = replies.lock().unwrap();
                    assert_eq!(reports.len(), 1);
                    assert!(reports[0].1 && reports[0].2.contains("The check passed."));
                    let total = if unpriced || vendor == Vendor::Claude { 0.03 } else { 0.05 };
                    assert_eq!(core.convs[&conv].spent_usd, total, "the known cost of each turn counts only once");
                    assert_eq!(core.turn(&conv).unwrap().incomplete == Some(true), unpriced);
                    return;
                }
                if lifecycle == 1 {
                    core.live.get_mut(&conv).unwrap().sid += 1;
                } else if lifecycle == 2 || lifecycle == 6 {
                    core.turn(&conv).unwrap().status = Some(Status::Done);
                    core.finish(&conv, Some("turn".into()));
                } else if lifecycle == 4 {
                    core.turn(&conv).unwrap().status = Some(Status::Error);
                    core.finish(&conv, Some("turn".into()));
                    assert!(core.helpers.is_empty(), "a failed caller turn cancels its helper");
                    let saved = &core.convs[&conv];
                    let receipt = "helper-report-call-42";
                    let resumed = saved
                        .queued
                        .iter()
                        .find(|q| q.id == receipt)
                        .map(|q| q.text.clone())
                        .or_else(|| saved.messages.iter().find(|m| m.id == receipt).map(|m| m.text.clone()));
                    assert!(
                        resumed.is_some_and(|text| text.contains("the calling session is closing or its turn ended")),
                        "the cancelled helper must resume its caller with the reason"
                    );
                    return;
                }
                if lifecycle == 6 {
                    assert!(core.helpers.contains_key("call"), "the actual helper outlives the completed caller");
                    return;
                }
                core.helper_event("call", 41, EngineEvent::WriterExit { confirmed: true });
                assert!(core.helpers.contains_key("call"), "a replaced process cannot end the current helper");
                if interrupt {
                    core.stop_helpers_because(&conv, false, stop_reason);
                    core.helper_event(
                        "call",
                        42,
                        EngineEvent::Done {
                            ok: true,
                            reason: "success".into(),
                            cost_total_usd: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            cache_write_1h_tokens: 0,
                            incomplete: true,
                        },
                    );
                } else {
                    core.helper_event("call", 42, EngineEvent::WriterExit { confirmed: lifecycle != UNCONFIRMED_EXIT });
                }
                assert!(core.helpers.is_empty(), "process death must not wait for the helper timeout or a pending message");
                assert!(closed.load(std::sync::atomic::Ordering::Acquire), "the helper session is closed");
                let replies = replies.lock().unwrap();
                if lifecycle == 1 || lifecycle == 2 {
                    assert!(replies.is_empty(), "a stale or finished caller needs a durable continuation");
                    let saved = &core.convs[&conv];
                    let receipt = "helper-report-call-42";
                    assert!(saved.queued.iter().any(|q| q.id == receipt) || saved.messages.iter().any(|m| m.id == receipt));
                    return;
                }
                assert_eq!(replies.len(), 1, "the caller is answered immediately");
                let (call, ok, text) = &replies[0];
                assert_eq!(call, "call");
                assert!(!ok);
                if interrupt {
                    assert!(text.contains(stop_reason), "caller needs the observed stop cause: {text}");
                } else {
                    assert!(text.contains(exit_reason));
                    if lifecycle != UNCONFIRMED_EXIT {
                        assert!(text.contains("the stop cause is unknown"));
                    }
                }
                assert!(text.contains("action workspace"), "the caller gets recovery steps with the cause");
                assert_eq!(text.contains("Keep these partial words."), partial);
                if long_told {
                    assert!(text.contains("Full report: .faber/verify/helper-"), "{text}");
                    assert!(text.ends_with(&helper::told_note(&["Keep the retries bounded.".into()])), "{text}");
                }
                let reply = core.turn(&conv).unwrap();
                assert_eq!(reply.incomplete, Some(true), "unknown helper spend must not make the caller's known cost look exact");
                assert_eq!(reply.cost_usd, Some(0.01));
                let saved = reply.tools.as_ref().unwrap().iter().find(|t| t.id == "call").unwrap();
                assert_eq!(saved.ok, Some(false), "the engine must retain the outcome without a CLI echo");
                assert!(
                    saved.output.as_ref().is_some_and(|out| out.contains(if interrupt { stop_reason } else { exit_reason })),
                    "the observed reason must survive restart"
                );
                core.helper_event("call", 42, EngineEvent::WriterExit { confirmed: true });
                assert_eq!(replies.len(), 1, "duplicate exits cannot answer twice");
            });
        })
        .await
        .expect("a dead helper must release its caller within two seconds");
        if matches!(
            lifecycle,
            BACKGROUND_COMMAND | BACKGROUND_NO_ACCOUNT | BACKGROUND_UNPRICED | BACKGROUND_PERMISSIONS | BACKGROUND_MESSAGE
        ) {
            tokio::task::yield_now().await;
            let saved = crate::store::Store::new(dir.path()).get(&conv).unwrap();
            if lifecycle != BACKGROUND_PERMISSIONS {
                assert!(saved.queued.is_empty(), "a delivered helper report has no fallback in the queue");
            }
            let tool = saved.messages.iter().flat_map(|m| m.tools.iter().flatten()).find(|t| t.id == "call").unwrap();
            assert_eq!(tool.ok, Some(!interrupt && matches!(lifecycle, BACKGROUND_COMMAND | BACKGROUND_UNPRICED | BACKGROUND_MESSAGE)));
            return;
        }
        if lifecycle == 6 {
            let before = engine.open(&conv).unwrap().waiting_on.unwrap();
            engine.shutdown();
            let saved = crate::store::Store::new(dir.path()).get(&conv).unwrap();
            let tool = saved.messages.iter().flat_map(|m| m.tools.iter().flatten()).find(|t| t.id == "call").unwrap();
            assert_eq!(tool.ok, Some(false));
            assert!(tool.output.as_ref().unwrap().contains("the engine is shutting down"));
            let wait = saved.waiting_on.expect("shutdown must retain a finite continuation");
            assert!(wait.note.contains("call"));
            assert_eq!(wait.note, before.note, "the existing helper wait is retained; the saved tool above retains the shutdown reason");
            assert!(wait.note.chars().count() <= 200);
            assert_eq!(wait.until, before.until);
            engine.shutdown();
            let repeated = crate::store::Store::new(dir.path()).get(&conv).unwrap();
            assert_eq!(serde_json::to_value(&repeated.waiting_on).unwrap(), serde_json::to_value(Some(wait)).unwrap());
            assert!(repeated.queued.is_empty(), "repeated shutdown cannot dispatch or duplicate the return");
            drop(engine);
            let (engine, _) = super::super::answer::tests::engine(&dir);
            engine.with(|core| {
                let wait = core.convs[&conv].waiting_on.clone().expect("restart restores the continuation");
                assert!(wait.note.contains("call"));
                assert!(core.wait_timers.contains_key(&conv));
                core.return_from_wait(&conv, "restart deadline");
                let blocked = core.store.get(&conv).unwrap().waiting_on.unwrap();
                assert!(blocked.problem.is_some(), "startup without confirmed account must not dispatch");
                core.return_from_wait(&conv, "duplicate deadline");
                assert_eq!(core.store.get(&conv).unwrap().waiting_on.unwrap().note, wait.note);
                assert!(core.convs[&conv].queued.is_empty());
            });
            engine.shutdown();
            return;
        }
        let receipt = "helper-report-call-42";
        let resumed = |engine: &Engine| {
            engine
                .with(|c| c.convs[&conv].queued.iter().any(|q| q.id == receipt) || c.convs[&conv].messages.iter().any(|m| m.id == receipt))
        };
        if lifecycle == 5 || lifecycle == 7 {
            if lifecycle == 7 {
                engine.with(|core| {
                    let mut next = Message::new("next-turn".into(), Role::Assistant, "New work".into(), now());
                    next.status = Some(Status::Streaming);
                    core.convs.get_mut(&conv).unwrap().messages.push(next);
                    core.live.get_mut(&conv).unwrap().turn = Some("next-turn".into());
                });
            }
            tokio::task::yield_now().await;
            assert!(!resumed(&engine));
            tokio::time::advance(Duration::from_secs(36)).await;
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert!(resumed(&engine), "a retained ACK cannot strand fallback");
            assert_eq!(
                interrupted.load(std::sync::atomic::Ordering::Acquire),
                lifecycle == 5,
                "only the original unanswered native tool call is interrupted"
            );
            let ack = retained.lock().unwrap().take().unwrap();
            let _ = ack.send(crate::events::Delivery::Delivered);
            tokio::task::yield_now().await;
            engine.with(|c| {
                assert_eq!(
                    c.convs[&conv].queued.iter().filter(|q| q.id == receipt).count()
                        + c.convs[&conv].messages.iter().filter(|m| m.id == receipt).count(),
                    1
                )
            });
        } else if lifecycle == 3 {
            // The delivery receipt arrives on the runtime: give its task a few turns.
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert!(resumed(&engine), "an unconfirmed reply must resume the caller durably");
        } else if delivered_directly {
            tokio::task::yield_now().await;
            assert!(!resumed(&engine), "a delivered reply must not run the caller twice");
        }
        let restored = crate::store::Store::new(dir.path()).get(&conv).unwrap();
        if !delivered_directly {
            assert!(restored.queued.iter().any(|q| q.id == receipt) || restored.messages.iter().any(|m| m.id == receipt));
        }
        let tool = restored.messages.iter().flat_map(|m| m.tools.iter().flatten()).find(|t| t.id == "call").unwrap();
        assert_eq!(tool.ok, Some(false), "the result must survive reload without a CLI echo");
        assert!(tool.output.as_ref().is_some_and(|text| text.contains(if interrupt {
            stop_reason
        } else if lifecycle == 4 {
            "the calling session is closing or its turn ended"
        } else {
            exit_reason
        })));
        engine.shutdown();
    }

    #[tokio::test]
    async fn helper_shutdown_restores_finished_callers_pending_return() {
        helper_exit(Vendor::Codex, true, false, "unused", 6, false).await;
    }

    #[tokio::test]
    async fn helper_shutdown_preserves_existing_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        engine.session_wait(&conv, vec![], Some("An existing external check".into())).unwrap();
        engine.with(|core| {
            let before = serde_json::to_value(&core.convs[&conv].waiting_on).unwrap();
            core.closing = true;
            core.resume_helper_caller(&conv, 1, "call", 42, "The helper stopped: shutdown".into());
            assert_eq!(serde_json::to_value(&core.store.get(&conv).unwrap().waiting_on).unwrap(), before);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn helper_outlives_completed_wrapper_or_replaced_session_with_durable_receipt() {
        helper_exit(Vendor::Codex, true, false, "unused", 1, false).await;
        helper_exit(Vendor::Codex, true, false, "unused", 2, false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn retained_helper_ack_has_a_finite_deadline_and_late_delivery_is_once() {
        helper_exit(Vendor::Codex, true, false, "unused", 5, false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn late_helper_ack_cannot_interrupt_a_newer_caller_turn() {
        helper_exit(Vendor::Codex, true, false, "unused", 7, false).await;
    }

    #[tokio::test]
    async fn unconfirmed_helper_reply_resumes_the_caller_durably() {
        helper_exit(Vendor::Codex, true, false, "unused", 3, false).await;
    }

    #[tokio::test]
    async fn helper_cancelled_by_a_failed_caller_turn_resumes_the_caller_with_the_reason() {
        helper_exit(Vendor::Codex, true, false, "unused", 4, false).await;
    }

    #[tokio::test]
    async fn interrupted_helper_reports_observed_stop_even_after_success_with_partial_words() {
        for why in [
            "the calling conversation stopped or ended",
            "an explicit stop request interrupted the calling conversation",
            "the silence watchdog interrupted the calling conversation",
            "the engine interrupted the calling conversation",
        ] {
            helper_exit(Vendor::Codex, true, true, why, 0, false).await;
        }
    }

    #[tokio::test]
    async fn cli_lifetime_codex_helper_exit_without_words_releases_its_claude_caller() {
        helper_exit(Vendor::Codex, false, false, "unused", 0, false).await;
    }
    #[tokio::test]
    async fn unconfirmed_helper_writer_exit_releases_caller_with_the_uncertainty() {
        helper_exit(Vendor::Claude, true, false, "unused", UNCONFIRMED_EXIT, false).await;
    }
    #[tokio::test]
    async fn cli_lifetime_codex_helper_exit_keeps_partial_words_and_releases_its_claude_caller() {
        helper_exit(Vendor::Codex, true, false, "unused", 0, false).await;
    }
    #[tokio::test]
    async fn cli_lifetime_claude_helper_exit_without_words_releases_its_codex_caller() {
        helper_exit(Vendor::Claude, false, false, "unused", 0, false).await;
    }
    #[tokio::test]
    async fn cli_lifetime_claude_helper_exit_keeps_partial_words_and_releases_its_codex_caller() {
        helper_exit(Vendor::Claude, true, false, "unused", 0, false).await;
    }

    #[tokio::test]
    async fn helper_jobs_keep_person_note_after_shortening_report() {
        helper_exit(Vendor::Claude, false, false, "unused", 0, true).await;
    }

    #[tokio::test]
    async fn a_helper_reports_after_its_background_command_and_native_follow_up() {
        helper_exit(Vendor::Claude, true, false, "unused", BACKGROUND_COMMAND, false).await;
        helper_exit(Vendor::Codex, true, false, "unused", BACKGROUND_COMMAND, false).await;
    }

    #[tokio::test]
    async fn stopping_a_waiting_helper_cancels_its_pending_wake() {
        helper_exit(Vendor::Codex, true, true, "the person stopped the project", BACKGROUND_COMMAND, false).await;
    }

    #[tokio::test]
    async fn a_waiting_helper_rechecks_current_account_evidence_before_another_send() {
        helper_exit(Vendor::Codex, true, false, "account or quota evidence", BACKGROUND_NO_ACCOUNT, false).await;
    }

    #[tokio::test]
    async fn an_unknown_first_turn_cost_remains_incomplete_after_a_known_per_turn_cost() {
        helper_exit(Vendor::Codex, true, false, "unused", BACKGROUND_UNPRICED, false).await;
    }

    #[tokio::test]
    async fn a_waiting_helper_closes_when_its_launch_permissions_are_reduced() {
        helper_exit(Vendor::Claude, true, false, "the person reduced the helper's permissions", BACKGROUND_PERMISSIONS, false).await;
    }

    #[tokio::test]
    async fn a_message_to_a_waiting_codex_helper_starts_one_turn_and_is_acknowledged() {
        helper_exit(Vendor::Codex, true, false, "unused", BACKGROUND_MESSAGE, false).await;
    }

    #[tokio::test]
    async fn a_message_to_a_waiting_claude_helper_is_kept_until_its_native_turn() {
        helper_exit(Vendor::Claude, true, false, "unused", BACKGROUND_MESSAGE, false).await;
    }
}
use crate::model::{HelperTeam, QuestionState, TeamKind, TeamMessage, TeamState};

impl Core {
    fn new_team_name(&self, conv: &str, title: &str, number: usize) -> String {
        let language = self.store.settings().language.unwrap_or_else(|| "en".into());
        let names: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../../ui/locales/helper-names.json")).expect("checked helper names");
        let numbered = names.get(&language).unwrap_or(&names["en"]);
        let used = |name: &str| {
            self.convs.get(conv).is_some_and(|c| {
                c.messages.iter().flat_map(|m| m.tools.iter().flatten()).any(|t| t.team.as_ref().is_some_and(|team| team.name == name))
            })
        };
        let mut name = if language == "en" && !title.trim().is_empty() && !used(title) {
            title.to_string()
        } else {
            numbered.replace("{n}", &number.to_string())
        };
        let mut suffix = number;
        while used(&name) {
            suffix += 1;
            name = numbered.replace("{n}", &suffix.to_string());
        }
        name
    }

    fn team_of(&self, conv: &str, call: &str) -> Option<&HelperTeam> {
        self.convs.get(conv)?.messages.iter().rev().flat_map(|m| m.tools.iter().flatten()).find(|t| t.id == call)?.team.as_ref()
    }

    fn team_name(&self, conv: &str, call: &str) -> String {
        self.team_of(conv, call).map_or_else(|| call.to_string(), |t| t.name.clone())
    }

    fn change_team(&mut self, conv: &str, call: &str, change: impl FnOnce(&mut HelperTeam)) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(m) = c.messages.iter_mut().rev().find(|m| m.tools.iter().flatten().any(|t| t.id == call)) else { return };
        let Some(team) = m.tools.iter_mut().flatten().find(|t| t.id == call).and_then(|t| t.team.as_mut()) else { return };
        change(team);
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    fn team_record(&mut self, conv: &str, ends: (&str, &str), kind: TeamKind, text: &str, question: Option<String>) {
        let (from, to) = ends;
        let name = |id: &str| if matches!(id, "lead" | "engine") { id.to_string() } else { self.team_name(conv, id) };
        let message = TeamMessage {
            id: new_id(),
            from: from.into(),
            to: to.into(),
            from_name: name(from),
            to_name: name(to),
            kind,
            text: text.into(),
            at: now(),
            question,
            state: (kind == TeamKind::Ask).then_some(QuestionState::Waiting),
        };
        for call in [from, to].into_iter().filter(|c| !matches!(*c, "lead" | "engine")) {
            self.change_team(conv, call, |team| {
                match kind {
                    TeamKind::Ask => team.state = TeamState::WaitingLead,
                    TeamKind::Answer | TeamKind::NoAnswer => {
                        team.state = TeamState::Running;
                        if let Some(ask) = team.messages.iter_mut().find(|m| m.kind == TeamKind::Ask && m.question == message.question) {
                            ask.state = Some(if kind == TeamKind::NoAnswer { QuestionState::Lapsed } else { QuestionState::Answered });
                        }
                    }
                    TeamKind::Tell => {}
                }
                team.messages.push(message.clone());
            });
        }
    }

    pub(super) fn helper_call_released(&self, conv: &str, call: &str) -> bool {
        self.helpers.get(call).is_some_and(|h| h.conv == conv)
            || self.convs.get(conv).is_some_and(|c| {
                c.messages.iter().rev().flat_map(|m| m.tools.iter().flatten()).any(|t| {
                    t.id == call
                        && t.name == "Helper"
                        && t.team.is_some()
                        && (t.ok.is_some()
                            || self.worker_waiting(&format!("helper:{call}"))
                            || self.kit_checks.contains_key(&format!("helper:{call}")))
                })
            })
    }

    fn team_target(&self, conv: &str, name: &str) -> Option<String> {
        self.helpers.keys().find(|call| self.helpers[*call].conv == conv && self.team_name(conv, call) == name).cloned()
    }

    fn team_reply(&self, call: &str, id: &str, ok: bool, text: &str) {
        if let Some(h) = self.helpers.get(call) {
            h.session.helper_reply(id, ok, text.into());
        }
    }

    fn team_call(&mut self, call: &str, id: &str, input: &serde_json::Value) {
        let Some(h) = self.helpers.get(call) else { return };
        let conv = h.conv.clone();
        if h.reviewer {
            return self.team_reply(call, id, false, "Nobody can answer: decide yourself.");
        }
        if h.stopping.is_some() {
            return self.team_reply(call, id, false, "The helper was stopped. Decide yourself.");
        }
        if input["do"] == "who" {
            let mut roster = vec![serde_json::json!({"name":"lead","job":"lead","title":"lead","files":[]})];
            let mut calls: Vec<_> = self.helpers.keys().filter(|c| self.helpers[*c].conv == conv).collect();
            calls.sort_by_key(|c| self.team_name(&conv, c));
            for call in calls {
                if let Some(team) = self.team_of(&conv, call) {
                    roster.push(serde_json::json!({"name":team.name,"job":team.job,"title":team.title,"files":team.files}));
                }
            }
            return self.team_reply(call, id, true, &serde_json::Value::Array(roster).to_string());
        }
        let raw = input["text"].as_str().unwrap_or("");
        let text = raw.trim();
        if text.is_empty() || raw.chars().count() > helper::TEAM_LENGTH || h.sent >= helper::TEAM_LIMIT {
            return self.team_reply(call, id, false, "Team messages must be 1 to 1,000 characters, at most 6 per helper per reply. Decide yourself and state your assumption in your report.");
        }
        match input["do"].as_str() {
            Some("ask") if h.question.is_none() => {
                let h = self.helpers.get_mut(call).unwrap();
                h.sent += 1;
                h.question = Some(id.into());
                h.question_timed = false;
                self.team_record(&conv, (call, "lead"), TeamKind::Ask, text, Some(id.into()));
                let notified = self.helpers.values().any(|h| h.conv == conv && h.lead_call.is_some())
                    || self.live.get(&conv).is_some_and(|l| l.open_tools.iter().any(|id| self.queued_team_call(&conv, id, None)));
                self.release_team_calls(&conv, call);
                if !notified && let Some(live) = self.live.get(&conv) {
                    live.session.steer(UserTurn::text(self.team_question(&conv, call)), &format!("team:{id}"));
                }
                self.expire_team_question(call, self.helpers[call].hsid, id, Duration::from_secs(15 * 60));
                self.start_team_timers(&conv);
            }
            Some("tell") => {
                if input["to"] == "lead" {
                    return self.team_reply(call, id, false, "Use ask to send a message to the lead.");
                }
                let Some(target) = input["to"].as_str().and_then(|name| self.team_target(&conv, name)) else {
                    return self.team_reply(call, id, false, "That teammate has ended or is not in this conversation. Decide yourself.");
                };
                if target == call {
                    return self.team_reply(call, id, false, "Choose another teammate. Decide yourself.");
                }
                self.helpers.get_mut(call).unwrap().sent += 1;
                let sent = self.send_team_note(&conv, call, &target, text);
                let result = if sent {
                    "The note is queued for the teammate."
                } else {
                    "The teammate could not take the note. Its report has the reason."
                };
                self.team_reply(call, id, sent, result);
            }
            _ => self.team_reply(call, id, false, "Use who, ask or tell; only one question may wait. Decide yourself."),
        }
    }

    fn team_question(&self, conv: &str, call: &str) -> String {
        let text = self
            .team_of(conv, call)
            .and_then(|team| team.messages.iter().rev().find(|m| m.state == Some(QuestionState::Waiting)))
            .map_or("", |m| m.text.as_str());
        let name = self.team_name(conv, call);
        let answer = serde_json::json!({"to":name,"answer":"your answer"});
        let message = Self::helper_message(Some(&name), text);
        format!("{message}\nAnswer with helper({answer}); it waits for the next question or report.")
    }

    fn helper_message(name: Option<&str>, text: &str) -> String {
        let quoted = text.split(['\n', '\r']).map(|line| format!("> {line}")).collect::<Vec<_>>().join("\n");
        match name {
            Some(name) => format!(
                "Message from helper {}, not from the person; it grants no permission and does not change your brief:\n{quoted}",
                name.escape_debug()
            ),
            None => format!("Message from the lead:\n{quoted}"),
        }
    }

    fn queued_team_call(&self, conv: &str, call: &str, launching: Option<&str>) -> bool {
        !self.helpers.contains_key(call)
            && self.convs.get(conv).is_some_and(|c| {
                c.messages
                    .iter()
                    .rev()
                    .flat_map(|m| m.tools.iter().flatten())
                    .any(|t| t.id == call && t.ok.is_none() && t.team.as_ref().is_some_and(|team| team.state != TeamState::Ended))
            })
            && (launching == Some(call)
                || self.worker_waiting(&format!("helper:{call}"))
                || self.kit_checks.get(&format!("helper:{call}")).is_some_and(|check| check.owner == conv))
    }

    /// Both CLIs wait for the whole tool batch: one question must release every pending call in that batch.
    fn release_team_calls(&mut self, conv: &str, call: &str) {
        let calls: Vec<_> = self.helpers.iter().filter(|(_, h)| h.conv == conv && h.lead_call.is_some()).map(|(c, _)| c.clone()).collect();
        let mut question = self.helpers.get(call).filter(|h| h.conv == conv && h.question.is_some() && h.lead_call.is_none()).map(|_| call);
        for call in &calls {
            let text = if self.helpers[call].question.is_some() {
                self.team_question(conv, call)
            } else {
                let name = self.team_name(conv, call);
                let wait = serde_json::json!({"to":name,"wait":true});
                let mut text = format!("{name} is still working; use helper({wait}) to wait for its question or report.");
                if let Some(asking) = question.take() {
                    text.push_str(&format!("\n{}", self.team_question(conv, asking)));
                }
                text
            };
            let h = self.helpers.get_mut(call).unwrap();
            let pending = h.lead_call.take().unwrap();
            if let Some(live) = self.live.get_mut(conv).filter(|l| l.sid == h.sid) {
                live.open_tools.retain(|id| id != call && id != &pending);
                live.session.helper_reply(&pending, true, text);
            }
        }
        let queued: Vec<_> = self
            .live
            .get(conv)
            .into_iter()
            .flat_map(|l| &l.open_tools)
            .filter(|id| self.queued_team_call(conv, id, Some(call)))
            .cloned()
            .collect();
        for call in queued {
            let name = self.team_name(conv, &call);
            let wait = serde_json::json!({"to":name,"wait":true});
            let mut text = format!("{name} is still queued; use helper({wait}) to wait.");
            if let Some(asking) = question.take() {
                text.push_str(&format!("\n{}", self.team_question(conv, asking)));
            }
            if let Some(live) = self.live.get_mut(conv) {
                live.open_tools.retain(|id| id != &call);
                live.session.helper_reply(&call, true, text);
            }
        }
    }

    fn answer_team(&mut self, call: &str, text: &str, lapsed: bool) -> bool {
        let Some(h) = self.helpers.get_mut(call) else { return false };
        let Some(question) = h.question.take() else { return false };
        let (conv, session) = (h.conv.clone(), h.session.clone());
        self.team_record(
            &conv,
            (if lapsed { "engine" } else { "lead" }, call),
            if lapsed { TeamKind::NoAnswer } else { TeamKind::Answer },
            text,
            Some(question.clone()),
        );
        session.helper_reply(&question, true, text.into());
        true
    }

    fn send_team_note(&mut self, conv: &str, from: &str, to: &str, text: &str) -> bool {
        let name = (from != "lead").then(|| self.team_name(conv, from));
        let note = Self::helper_message(name.as_deref(), text);
        let id = format!("team:{}", new_id());
        if self.deliver_helper_note(to, id, note) {
            self.team_record(conv, (from, to), TeamKind::Tell, text, None);
            return true;
        }
        false
    }

    fn helper_control(&mut self, conv: &str, id: &str, input: &serde_json::Value) {
        let reply = |core: &Self, ok, text: String| {
            if let Some(live) = core.live.get(conv) {
                live.session.helper_reply(id, ok, text);
            }
        };
        let name = input["to"].as_str().unwrap_or("");
        let tell = input["tell"].as_str();
        let answer = input["answer"].as_str();
        if name.is_empty()
            || ["title", "brief", "job", "kind", "difficulty"].iter().any(|key| input.get(key).is_some())
            || ["answer", "tell", "wait"].iter().filter(|key| input.get(**key).is_some()).count() != 1
            || usize::from(tell.is_some()) + usize::from(answer.is_some()) + usize::from(input["wait"] == true) != 1
            || tell.or(answer).is_some_and(|text| text.trim().is_empty() || text.chars().count() > helper::TEAM_LENGTH)
        {
            return reply(
                self,
                false,
                "Use one of {to, answer}, {to, tell} or {to, wait:true}; messages must be 1 to 1,000 characters.".into(),
            );
        }
        let Some(call) = self.team_target(conv, name) else {
            let ended = self
                .convs
                .get(conv)
                .into_iter()
                .flat_map(|c| &c.messages)
                .rev()
                .flat_map(|m| m.tools.iter().flatten())
                .find(|t| t.team.as_ref().is_some_and(|team| team.name == name));
            if input.get("tell").is_some() || input.get("answer").is_some() {
                return reply(self, false, "That helper has ended or is not in this conversation.".into());
            }
            if ended.is_some_and(|t| t.ok.is_none()) {
                return reply(self, true, "Still queued: do other work, then wait again for its question or report.".into());
            }
            let text =
                ended.and_then(|t| t.output.clone()).unwrap_or_else(|| "That helper has ended or is not in this conversation.".into());
            return reply(self, ended.is_some_and(|t| t.ok == Some(true)), text);
        };
        if let Some(text) = tell {
            if self.helpers[&call].sent >= helper::TEAM_LIMIT {
                return reply(self, false, "At most 6 team messages per helper per reply. Decide yourself.".into());
            }
            self.helpers.get_mut(&call).unwrap().sent += 1;
            let sent = self.send_team_note(conv, "lead", &call, text);
            let result =
                if sent { "The note is queued for the helper." } else { "The helper could not take the note. Its report has the reason." };
            return reply(self, sent, result.into());
        }
        if self.helpers[&call].lead_call.is_some() {
            return reply(self, false, "A call is already waiting for this helper.".into());
        }
        if let Some(text) = answer
            && !self.answer_team(&call, text, false)
        {
            return reply(self, false, "This helper has no question waiting. Use wait:true.".into());
        }
        self.helpers.get_mut(&call).unwrap().lead_call = Some(id.into());
        if self.helpers[&call].question.is_some() {
            let text = self.team_question(conv, &call);
            self.helpers.get_mut(&call).unwrap().lead_call = None;
            reply(self, true, text);
        }
    }
}

#[cfg(test)]
#[path = "helpers/team_tests.rs"]
mod team_tests;

impl Core {
    /// Start the allowance only when the CLI's whole tool batch has returned, so a long command cannot consume it.
    pub(super) fn start_team_timers(&mut self, conv: &str) {
        if self.live.get(conv).is_none_or(|live| !live.open_tools.is_empty()) {
            return;
        }
        let pending: Vec<_> = self
            .helpers
            .iter_mut()
            .filter(|(_, h)| h.conv == conv && h.question.is_some() && !h.question_timed)
            .map(|(call, h)| {
                h.question_timed = true;
                (call.clone(), h.hsid, h.question.clone().unwrap())
            })
            .collect();
        for (call, hsid, id) in pending {
            self.expire_team_question(&call, hsid, &id, Duration::from_secs(5 * 60));
        }
    }

    fn expire_team_question(&self, call: &str, hsid: u64, id: &str, after: Duration) {
        let (me, call, id) = (self.me.clone(), call.to_string(), id.to_string());
        let deadline = tokio::time::Instant::now() + after;
        self.opts.runtime.spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.helpers.get(&call).is_some_and(|h| h.hsid == hsid && h.question.as_deref() == Some(&id)) {
                core.answer_team(&call, helper::NO_ANSWER, true);
            }
        });
    }
}
