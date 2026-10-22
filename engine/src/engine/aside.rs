//! A quick question on the side (R250): a read-only copy of the conversation answers while its agent works, without stopping it
//! or waiting in its queue. Claude Code forks the conversation's own CLI session; Codex, and a Claude conversation with no
//! session yet, read Faber's record of it. Nothing goes back into the conversation, and the answers go when the turn after the
//! question ends.
use super::*;
use crate::helper::Helper;

/// Faber's record of the conversation stops at this many characters, its end kept.
const RECORD_LIMIT: usize = crate::handover::FULL_LIMIT;

/// What a side request is told when it reads Faber's record instead of the conversation's own session.
const RECORDED: &str = "You answer a quick side question from the person about a conversation with an AI agent, which keeps working \
meanwhile. Faber's record of that conversation comes first, then the question. Answer briefly and plainly from the record. You \
cannot change anything; if the answer needs an action, say so in one line: the person can send it to the main agent.";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Aside {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub vendor: Vendor,
    pub model: Option<String>,
    /// `running`, `done`, `stopped` or `failed`.
    pub state: &'static str,
    /// A translation key when it failed.
    pub error: Option<String>,
    #[serde(skip)]
    asked: f64,
    #[serde(skip)]
    new_block: bool,
    /// A turn that began after it was asked has ended: it goes as soon as it answers.
    #[serde(skip)]
    expired: bool,
}

/// Why a question cannot run on the vendor's quota now, as a key.
fn refusal(why: Why) -> String {
    match why {
        Why::Billing => "error.billedPerUse",
        _ => "error.quotaOut",
    }
    .into()
}

pub(super) struct Asking {
    conv: String,
    vendor: Vendor,
    session: Session,
    model: String,
    started: f64,
}

impl Core {
    pub(super) fn asides_of(&self, conv: &str) -> Vec<Aside> {
        self.asides.get(conv).cloned().unwrap_or_default()
    }

    /// Asks the side question. The model is the conversation's own unless the person picked another of the same AI.
    pub(super) fn ask_aside(&mut self, conv: &str, question: &str, model: Option<String>) -> Result<String, String> {
        let question = question.trim();
        if question.is_empty() {
            return Err("error.aside.empty".into());
        }
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant && m.vendor.is_some());
        let live = self.live.get(conv);
        let vendor = live.map(|l| l.vendor).or(last.and_then(|m| m.vendor)).ok_or("error.aside.none")?;
        if self.asking.values().any(|a| a.conv == conv) {
            return Err("error.aside.busy".into());
        }
        // A model of the other AI is never run here: the tab offers only the conversation's own.
        let codex_model = |m: &String| self.codex_models.contains(m);
        let model = model
            .filter(|m| codex_model(m) == (vendor == Vendor::Codex))
            .or_else(|| live.filter(|l| l.vendor == vendor).and_then(|l| l.running.clone().or_else(|| l.model.clone())))
            .or_else(|| last.filter(|m| m.vendor == Some(vendor)).and_then(|m| m.model.clone()))
            // A conversation that never named one still asks on a model Faber chose, never on the CLI's own (rule 2).
            .or_else(|| Some(crate::router::default_model(vendor, self.codex_recommended.as_deref(), &self.codex_models)))
            // The picker's alias ("opus") goes to the CLI as the full id.
            .map(|m| crate::router::pinned(vendor, &m).to_string());
        let cli = self.opts.clis.get(&vendor).cloned().ok_or("error.aside.none")?;
        self.allows(vendor, model.as_deref().and_then(family_of), Purpose::UserTurn { by_hand: true }, false).map_err(refusal)?;
        let (area, cwd) = (c.area, self.cwd_of(c)?);
        let resume = (vendor == Vendor::Claude).then(|| c.resumable(vendor).cloned()).flatten();
        let text = match resume {
            Some(_) => question.to_string(),
            None => format!("{}\n\nQuestion: {question}", record(&c.messages)),
        };
        self.begin_unbounded(vendor)?;
        let id = new_id();
        let (me, key) = (self.me.clone(), id.clone());
        let sink: EventSink = Arc::new(move |event| {
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).aside_event(&key, event);
            }
        });
        let effort = Some("low".to_string());
        let session = match vendor {
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch {
                        area,
                        model: model.clone(),
                        effort,
                        // The fork reads the session and writes nothing: the conversation's own stays as it was.
                        instructions: resume.is_none().then(|| RECORDED.to_string()),
                        resume,
                        user_hooks: false,
                        // Every read is checked by the driver with the working-folder policy (see `claude_args`).
                        permissions: Permissions::Isolated,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        flags: self.claude_flags_for(&cli),
                        tuning: Default::default(),
                        helper: Helper::Aside,
                        browse: false,
                        conversations: false,
                        progress: false,
                        verify: false,
                        tools: Default::default(),
                    },
                    spawn: spawner(&self.opts.spawn, &cli),
                    sandbox: None,
                },
                sink,
            ),
            // The server Faber's own questions already use (`open_aux`), not one more beside the conversation's.
            Vendor::Codex => self.codex_server(Area::Chat, false, Permissions::Isolated, &cli).open(
                CodexThread {
                    leave_plan: false,
                    cwd,
                    model: model.clone(),
                    effort,
                    resume: None,
                    instructions: Some(RECORDED.to_string()),
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                    helper: Helper::Aside,
                    browse: false,
                    conversations: false,
                    progress: false,
                    verify: false,
                },
                sink,
            ),
        };
        session.send(UserTurn::text(text));
        let asked = self.secs();
        let shown = model.clone().unwrap_or_else(|| vendor.name().to_string());
        self.asking.insert(id.clone(), Asking { conv: conv.to_string(), vendor, session, model: shown, started: asked });
        let aside = Aside {
            id: id.clone(),
            question: question.to_string(),
            answer: String::new(),
            vendor,
            model,
            state: "running",
            error: None,
            asked,
            new_block: false,
            expired: false,
        };
        self.asides.entry(conv.to_string()).or_default().push(aside);
        self.emit_asides(conv);
        Ok(id)
    }

    pub(super) fn stop_aside(&mut self, id: &str) {
        self.end_aside(id, "stopped", None, None);
    }

    fn aside_event(&mut self, id: &str, event: EngineEvent) {
        let Some(conv) = self.asking.get(id).map(|a| a.conv.clone()) else { return };
        match event {
            EngineEvent::Break { .. } => {
                if let Some(a) = self.aside_mut(&conv, id) {
                    a.new_block = true;
                }
            }
            EngineEvent::Text(delta) => {
                let Some(a) = self.aside_mut(&conv, id) else { return };
                // The words before a search are its preamble: the answer is the last block.
                if std::mem::take(&mut a.new_block) && !delta.is_empty() {
                    a.answer.clear();
                }
                a.answer.push_str(&delta);
                self.emit_asides(&conv);
            }
            e @ EngineEvent::Done { .. } => {
                let ok = matches!(e, EngineEvent::Done { ok: true, .. });
                let error = (!ok).then(|| "error.aside.failed".to_string());
                self.end_aside(id, if ok { "done" } else { "failed" }, error, Some(e));
            }
            EngineEvent::Started { model, .. } => {
                if let Some(a) = self.asking.get_mut(id) {
                    a.model = model;
                }
            }
            // A turn Codex refused to start never ends on its own.
            EngineEvent::Error { message, fatal } if fatal || message.starts_with("Codex turn/start failed") => {
                self.end_aside(id, "failed", Some("error.aside.failed".into()), None)
            }
            e @ (EngineEvent::Quota { .. } | EngineEvent::Signals(_)) => {
                let Some(vendor) = self.asking.get(id).map(|a| a.vendor) else { return };
                self.note_account_event(vendor, e);
                self.stop_paid_asides(vendor, false);
            }
            _ => {}
        }
    }

    fn end_aside(&mut self, id: &str, state: &'static str, error: Option<String>, done: Option<EngineEvent>) {
        let Some(asking) = self.asking.remove(id) else { return };
        let vendor = self.aside_mut(&asking.conv, id).map(|a| a.vendor);
        let mut expired = false;
        if let Some(a) = self.aside_mut(&asking.conv, id) {
            a.state = state;
            a.error = error;
            expired = a.expired;
        }
        if expired && let Some(list) = self.asides.get_mut(&asking.conv) {
            list.retain(|a| a.id != id);
        }
        if let Some(vendor) = vendor {
            let (cost, incomplete) = done.as_ref().map_or((None, true), |e| usage::fresh_price(e, &asking.model));
            self.note_span(vendor, asking.started, &asking.model, cost, incomplete);
        }
        // One process per question: it is gone once it answered.
        asking.session.close();
        self.emit_asides(&asking.conv);
    }

    /// A turn of the conversation ended: the questions asked before it began are over.
    pub(super) fn drop_asides(&mut self, conv: &str, turn_started: f64) {
        let Some(list) = self.asides.get_mut(conv) else { return };
        let before = list.len();
        for a in list.iter_mut().filter(|a| a.asked < turn_started) {
            a.expired = true;
        }
        list.retain(|a| a.state == "running" || !a.expired);
        if list.len() != before {
            self.emit_asides(conv);
        }
    }

    /// Ends the questions still asking: those of one conversation, or all of them when Faber closes.
    pub(super) fn close_asides(&mut self, conv: Option<&str>) {
        let ids: Vec<String> = self.asking.iter().filter(|(_, a)| conv.is_none_or(|c| a.conv == c)).map(|(id, _)| id.clone()).collect();
        for id in ids {
            self.end_aside(&id, "stopped", None, None);
        }
    }

    /// A reading of the vendor's account, from anywhere: a question its quota may no longer pay for stops, and says why. `paid`:
    /// the reading shows paid use already (credits falling, overage), which no quick question was ever approved for.
    pub(super) fn stop_paid_asides(&mut self, vendor: Vendor, paid: bool) {
        let asking: Vec<(String, String)> =
            self.asking.iter().filter(|(_, a)| a.vendor == vendor).map(|(id, a)| (id.clone(), a.model.clone())).collect();
        for (id, model) in asking {
            match self.allows(vendor, family_of(&model), Purpose::UserTurn { by_hand: true }, false) {
                Err(why) => self.end_aside(&id, "failed", Some(refusal(why)), None),
                Ok(()) if paid => self.end_aside(&id, "failed", Some("error.quotaOut".into()), None),
                Ok(()) => {}
            }
        }
    }

    pub(super) fn aside_running(&self, vendor: Vendor) -> bool {
        self.asides.values().flatten().any(|a| a.vendor == vendor && a.state == "running")
    }

    fn aside_mut(&mut self, conv: &str, id: &str) -> Option<&mut Aside> {
        self.asides.get_mut(conv)?.iter_mut().find(|a| a.id == id)
    }

    fn emit_asides(&self, conv: &str) {
        (self.emit)(AppEvent::Asides { conversation_id: conv.to_string(), asides: self.asides_of(conv) });
    }
}

/// Faber's record of the conversation, the reply under way included, its end kept when it is long.
pub fn record(messages: &[Message]) -> String {
    let all = crate::handover::transcript(messages);
    let count = all.chars().count();
    let kept: String = all.chars().skip(count.saturating_sub(RECORD_LIMIT)).collect();
    let cut = if count > RECORD_LIMIT { "[...]\n\n" } else { "" };
    format!("Faber's record of the conversation so far:\n\n{cut}{kept}")
}
