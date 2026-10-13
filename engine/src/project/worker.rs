//! Bounded typed worker state. Session drivers remain behind the T2 gate.
use super::{Brief, Extra, Role, SCHEMA, kit, valid_id, wire};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPin {
    pub vendor: Vendor,
    pub model: String,
    pub effort: String,
    pub cli_version: String,
}
pub(crate) fn full_model_id(vendor: Vendor, model: &str) -> bool {
    match vendor {
        Vendor::Claude => ["claude-haiku-", "claude-sonnet-", "claude-opus-"].iter().any(|p| {
            model.strip_prefix(p).is_some_and(|v| {
                !v.is_empty() && v.bytes().any(|b| b.is_ascii_digit()) && v.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'.')
            })
        }),
        Vendor::Codex => model.strip_prefix("gpt-").is_some_and(|v| {
            let version = ["-luna", "-sol", "-astra"].iter().find_map(|s| v.strip_suffix(s)).unwrap_or(v);
            !version.is_empty() && version.bytes().any(|b| b.is_ascii_digit()) && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        }),
    }
}
impl ModelPin {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !full_model_id(self.vendor, &self.model)
            || !matches!(self.effort.as_str(), "low" | "medium" | "high" | "xhigh" | "max")
            || (self.model.starts_with("claude-sonnet-") && self.effort == "max")
            || self.cli_version.is_empty()
            || self.cli_version.len() > 128
            || !self.cli_version.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || !self.cli_version.bytes().any(|b| b.is_ascii_digit())
        {
            return Err("error.project.invocation");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Accepted(wire::Envelope),
    /// Test patches are artifacts; they can never enter the source action path.
    Proposal(wire::Envelope),
    Bounce(&'static str),
    Continue,
    Blocked(&'static str),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
    pub schema: u32,
    pub brief: Brief,
    pub pin: ModelPin,
    /// Per-attempt allowance shrinks on retry; lifetime counters never reset.
    pub turn_limit: u32,
    pub spent_turns: u32,
    pub answered: Vec<ModelPin>,
    pub fresh: bool,
    lifetime_turns: u32,
    attempt_turns: u32,
    bounces: u8,
    stalls: u8,
    escalations: u8,
    failed: bool,
    proofs: BTreeMap<String, u8>,
    hunks: BTreeMap<String, u8>,
    blocked: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

fn inside(path: &str, scope: &str) -> bool {
    scope.is_empty() || path == scope || path.starts_with(&format!("{scope}/"))
}
fn scoped(raw: &str) -> Result<String, &'static str> {
    let path = super::lease::normalized(raw).map_err(|e| e.key())?;
    if path.split('/').any(|p| matches!(p.to_ascii_lowercase().as_str(), ".claude" | ".codex" | ".gemini")) {
        return Err("error.wire.path");
    }
    Ok(path)
}
fn validate(b: &Brief) -> Result<(), &'static str> {
    if b.schema != SCHEMA
        || !valid_id(&b.task)
        || b.attempt == 0
        || !super::git::valid_oid(&b.base)
        || !valid_id(&b.contract.id)
        || b.contract.digest.len() != 64
        || !b.contract.digest.bytes().all(|c| c.is_ascii_hexdigit())
        || b.grant_micros.is_none_or(|n| n == 0)
        || b.stop.trim().is_empty()
    {
        return Err("error.wire.stale");
    }
    let writes = b.role == Role::Writer;
    if writes && (b.writes.is_empty() || b.lease.is_none_or(|n| n == 0)) || !writes && (!b.writes.is_empty() || b.lease.is_some()) {
        return Err("error.wire.role");
    }
    for list in [&b.scope, &b.writes, &b.sources, &b.proofs] {
        if list.len() > wire::MAX_PATHS {
            return Err("error.wire.tooBig");
        }
    }
    for path in b.scope.iter().chain(&b.writes).chain(&b.sources) {
        scoped(path)?;
    }
    for path in &b.writes {
        let path = scoped(path)?;
        if !b.scope.iter().any(|s| scoped(s).is_ok_and(|s| inside(&path, &s))) {
            return Err("error.wire.path");
        }
    }
    for proof in &b.proofs {
        wire::check_id(proof).map_err(|e| e.key())?;
    }
    for list in [&b.requires, &b.checks, &b.quotes] {
        if list.len() > wire::MAX_PATHS {
            return Err("error.wire.tooBig");
        }
        let mut seen = std::collections::BTreeSet::new();
        for v in list {
            wire::check_id(&v.id).map_err(|e| e.key())?;
            if v.text.len() > wire::MAX_TEXT || !seen.insert(&v.id) {
                return Err("error.wire.malformed");
            }
        }
    }
    if let Some(pin) = &b.kit {
        pin.validate().map_err(|e| e.key())?;
    }
    Ok(())
}

/// Text travels byte-for-byte. Only ids and bounded engine fields surround it; private sessions never enter a brief.
pub fn render(brief: &Brief) -> Result<String, &'static str> {
    validate(brief)?;
    let mut text = crate::persona::worker_contract(brief.role);
    let header = serde_json::json!({"schema":wire::SCHEMA,"task":brief.task,"attempt":brief.attempt,"base":brief.base,
        "contract":brief.contract,"lease":brief.lease,"goal":brief.goal,"scope":brief.scope,"writes":brief.writes,
        "sources":brief.sources,"proofs":brief.proofs,"grantMicros":brief.grant_micros,"stop":brief.stop,"kit":brief.kit});
    text.push('\n');
    text.push_str(&header.to_string());
    for (kind, list) in [("requires", &brief.requires), ("checks", &brief.checks), ("quotes", &brief.quotes)] {
        for v in list {
            text.push_str(&format!("\n{kind} {} ({} UTF-8 bytes):\n", v.id, v.text.len()));
            text.push_str(&v.text);
        }
    }
    if text.len() > wire::MAX_ENVELOPE {
        return Err("error.wire.tooBig");
    }
    Ok(text)
}
impl Worker {
    pub fn new(brief: Brief, pin: ModelPin, turns: u32) -> Result<Self, &'static str> {
        pin.validate()?;
        render(&brief)?;
        if turns == 0 || turns > 64 {
            return Err("reason.turnLimit");
        }
        Ok(Self {
            schema: SCHEMA,
            brief,
            pin,
            turn_limit: turns,
            spent_turns: 0,
            answered: vec![],
            fresh: true,
            lifetime_turns: turns,
            attempt_turns: 0,
            bounces: 0,
            stalls: 0,
            escalations: 0,
            failed: false,
            proofs: BTreeMap::new(),
            hunks: BTreeMap::new(),
            blocked: None,
            extra: Extra::new(),
        })
    }
    pub fn kit_prompt(&self, receipt: &kit::KitReceipt, bodies: &[kit::InlineBody]) -> Result<String, &'static str> {
        if self.brief.kit.as_ref() != Some(&receipt.pin())
            || receipt.task != self.brief.task
            || receipt.attempt != self.brief.attempt
            || receipt.kit.role != self.brief.role
            || receipt.kit.vendor != self.pin.vendor
            || receipt.kit.model != self.pin.model
            || receipt.kit.effort != self.pin.effort
            || self.brief.proofs.iter().any(|id| !receipt.kit.proofs.contains(id))
        {
            return Err("reason.kit.stale");
        }
        let contract = crate::persona::worker_contract(self.brief.role);
        let mut prompt = kit::compose_role_prompt(&contract, &receipt.kit, bodies).map_err(|e| e.key())?;
        let brief = render(&self.brief)?;
        prompt.push_str(brief.strip_prefix(&contract).ok_or("error.wire.stale")?);
        if prompt.len() > wire::MAX_ENVELOPE {
            return Err("error.wire.tooBig");
        }
        Ok(prompt)
    }
    pub fn digest(&self) -> String {
        kit::digest(serde_json::to_string(&(&self.brief, &self.pin)).expect("plain data").as_bytes())
    }
    /// The caller has already rechecked T2/T3. A requested pin is never substituted for a missing CLI report.
    pub fn begin_turn(&mut self, reported: &ModelPin) -> Result<(), &'static str> {
        self.pin.validate()?;
        reported.validate()?;
        if &self.pin != reported {
            return Err("reason.worker.identity");
        }
        if self.blocked.is_some() {
            return Err("reason.worker.blocked");
        }
        if self.spent_turns >= self.lifetime_turns || self.attempt_turns >= self.turn_limit {
            return Err("reason.turnLimit");
        }
        self.spent_turns += 1;
        self.attempt_turns += 1;
        self.answered.push(reported.clone());
        self.fresh = false;
        Ok(())
    }
    pub fn reply(&mut self, text: &str) -> Reply {
        if self.blocked.is_some() {
            return Reply::Blocked("reason.worker.blocked");
        }
        let expected = wire::Expect {
            task: self.brief.task.clone(),
            attempt: self.brief.attempt,
            base: self.brief.base.clone(),
            contract: self.brief.contract.digest.clone(),
            lease: self.brief.lease.unwrap_or(0),
        };
        let parsed = wire::Envelope::parse(text, &expected, self.brief.role).map_err(|e| e.key());
        let parsed = parsed.and_then(|e| {
            for (ids, list) in [(&e.requires, &self.brief.requires), (&e.checks, &self.brief.checks), (&e.quotes, &self.brief.quotes)] {
                if ids.iter().any(|id| !list.iter().any(|v| &v.id == id)) {
                    return Err("error.wire.stale");
                }
            }
            if matches!(e.action, wire::Action::Patch(_)) && !matches!(self.brief.role, Role::Writer | Role::TestProposal) {
                return Err("error.wire.role");
            }
            if matches!(e.action,wire::Action::Complete(ref c) if e.ping == Some(wire::Ping::DoneReady) && c.receipt.is_none()) {
                return Err("reason.receipt.missing");
            }
            match &e.action {
                wire::Action::Read(r) => {
                    for p in &r.paths {
                        let path = scoped(p)?;
                        if !self.brief.scope.iter().any(|s| scoped(s).is_ok_and(|s| inside(&path, &s))) {
                            return Err("error.wire.path");
                        }
                    }
                    // Callers must be supplied by a separately scoped engine index, never guessed as paths.
                    if !r.callers.is_empty() {
                        return Err("reason.worker.callers");
                    }
                }
                wire::Action::Proof(p) if p.ids.iter().any(|id| !self.brief.proofs.contains(id)) => return Err("error.wire.stale"),
                _ => {}
            }
            Ok(e)
        });
        let envelope = match parsed {
            Ok(e) => e,
            Err(key) => {
                self.bounces += 1;
                if self.bounces >= 2 {
                    self.blocked = Some(key.into());
                    return Reply::Blocked(key);
                }
                return Reply::Bounce(key);
            }
        };
        if envelope.ping == Some(wire::Ping::Blocked) {
            self.blocked = Some("reason.worker.blocked".into());
            return Reply::Blocked("reason.worker.blocked");
        }
        if matches!(envelope.action, wire::Action::Complete(_)) && envelope.ping != Some(wire::Ping::DoneReady) {
            self.stalls += 1;
            if self.stalls == 1 {
                return Reply::Continue;
            }
            self.blocked = Some("reason.stall".into());
            return Reply::Blocked("reason.stall");
        }
        if self.brief.role == Role::TestProposal && matches!(envelope.action, wire::Action::Patch(_)) {
            return Reply::Proposal(envelope);
        }
        Reply::Accepted(envelope)
    }
    fn count(map: &mut BTreeMap<String, u8>, key: String) -> bool {
        let n = map.entry(key).or_default();
        *n = n.saturating_add(1);
        *n >= 3
    }
    pub fn failed_proof(&mut self, id: &str, result: &str) -> Result<(), &'static str> {
        self.failed = true;
        if Self::count(&mut self.proofs, kit::digest(format!("{id}\0{result}").as_bytes())) {
            self.blocked = Some("reason.loop".into());
            return Err("reason.loop");
        }
        Ok(())
    }
    pub fn patched(&mut self, hunk: &str) -> Result<(), &'static str> {
        if Self::count(&mut self.hunks, kit::digest(hunk.as_bytes())) {
            self.blocked = Some("reason.loop".into());
            return Err("reason.loop");
        }
        Ok(())
    }
    pub fn proof_progress(&mut self) {
        self.proofs.clear();
        self.hunks.clear();
        self.failed = false;
    }
    pub fn escalate(&mut self, next: ModelPin) -> Result<(), &'static str> {
        next.validate()?;
        let efforts = ["low", "medium", "high", "xhigh", "max"];
        let rank = |s: &str| efforts.iter().position(|e| *e == s);
        let effort_raise = next.model == self.pin.model && rank(&next.effort) == rank(&self.pin.effort).map(|n| n + 1);
        let model_rank = |m: &str| {
            if m.contains("haiku") || m.ends_with("-luna") {
                0
            } else if m.contains("sonnet") || m.ends_with("-sol") {
                1
            } else {
                2
            }
        };
        let model_raise =
            next.model != self.pin.model && next.effort == self.pin.effort && model_rank(&next.model) > model_rank(&self.pin.model);
        if !self.failed
            || self.blocked.is_some()
            || self.escalations >= 2
            || self.turn_limit <= 1
            || next.vendor != self.pin.vendor
            || (self.escalations == 0 && !effort_raise)
            || (self.escalations == 1 && !model_raise)
        {
            return Err("reason.worker.escalation");
        }
        self.pin = next;
        self.escalations += 1;
        self.brief.attempt += 1;
        self.turn_limit /= 2;
        self.attempt_turns = 0;
        self.fresh = true;
        self.failed = false;
        // Only the typed brief survives. Driver session ids and transcripts stay in the private driver store.
        Ok(())
    }
}
