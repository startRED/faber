//! Auto: routes a conversation's messages (see `route`), guessing while the user types so the answer can start the
//! moment it is sent. Once a message needed more than an easy model, the conversation stays with it.

use super::*;
use crate::budget::{Purpose, Shape, Snap, Why, family_of, gate};
use crate::route::{Asked, Factor, INSTRUCTIONS, Profile, ROUTER, Routed, asked, candidates, choose_paced, question};
use tokio::sync::oneshot;

/// How long a sent message may wait for its route before the everyday default answers: a timeout, not a target (Erick,
/// 10/10/2026), so a very long request still gets routed. The router (GPT-6 Luna at medium effort on the Codex process
/// that is already running) measured p50 2.8 s and p90 3.9 s that day; about 2.2 s of it is the service's first word,
/// whatever the prompt size or service tier.
pub(super) const ROUTE_BUDGET: Duration = Duration::from_secs(30);
/// The router itself gives up at the same point, drafts included.
const ASK_LIMIT: Duration = ROUTE_BUDGET;

/// A route for a request, ready or on its way.
pub(super) struct Guess {
    request: String,
    /// What the small model said about the request (kind, level, confidence). The table runs when the message is sent, on the
    /// quota of that moment, so only this is kept.
    result: Option<Option<Asked>>,
    waiters: Vec<oneshot::Sender<Option<Asked>>>,
    /// The new conversation whose answering CLI starts as soon as the route is known.
    warm: Option<String>,
    /// A conversation under way is routed inside its own subscription, where the CLI keeps the history.
    only: Option<Vendor>,
    /// Stops the router's session when nobody waits for its answer any more.
    stop: Option<Stop>,
}

/// A newer draft to route once the one in flight is done.
pub(super) struct Draft {
    request: String,
    warm: String,
    only: Option<Vendor>,
}

pub(super) type Stop = Arc<Mutex<Option<Session>>>;

/// A router session started ahead of its question.
pub(super) struct Spare {
    pub(super) session: Session,
    pub(super) answer: oneshot::Receiver<Option<String>>,
    id: u64,
    pub(super) vendor: Vendor,
    pub(super) asked: Arc<Mutex<Option<f64>>>,
    pub(super) model_name: &'static str,
}

/// A draft's route still holds for the sent text when the text only grew a little past it.
fn covers(guessed: &str, sent: &str) -> bool {
    let sent = sent.trim();
    guessed == sent || (!guessed.is_empty() && sent.starts_with(guessed) && guessed.chars().count() * 10 >= sent.chars().count() * 7)
}

impl Core {
    /// Whether Auto routes this conversation's next message: its first one (`Some(None)`), or the next ones while only
    /// an easy model was needed, such as after a greeting (`Some(Some(the model so far))`). A conversation whose first
    /// route came too late stays with the default, so a slow router does not hold up every message. ponytail: only the
    /// step up from easy; a conversation that grows from medium to hard stays put until routing follows changes of kind.
    pub(super) fn next_route(&self, id: &str) -> Option<Option<Pick>> {
        let c = self.convs.get(id)?;
        if c.messages.is_empty() {
            return Some(None);
        }
        let before = same_as_before(c, self.live.get(id)).filter(|p| self.opts.clis.contains_key(&p.vendor))?;
        (before.reason.starts_with("reason.route.") && before.reason.ends_with(".easy")).then_some(Some(before))
    }

    /// The one door before a request reaches a CLI (3.4): may this vendor and family be asked for this purpose now.
    pub(super) fn allows(&self, vendor: Vendor, family: Option<&str>, purpose: Purpose, approved: bool) -> Result<(), Why> {
        self.quota_allows(vendor, family, purpose, approved)
    }

    fn quota_allows(&self, vendor: Vendor, family: Option<&str>, purpose: Purpose, approved: bool) -> Result<(), Why> {
        let now = self.secs();
        let mut snap = self.snap(vendor, now);
        if vendor == Vendor::Claude && self.opts.claude_env_billed {
            snap.billing = Billing::UsageBilled;
        }
        gate(&snap, family, purpose, approved, now)
    }

    /// The table's pick for the router's verdict, on the quota as it is now. Profiles the gate would refuse are not in the
    /// pool; with every multiplier and weight at 1 the pick is exactly the table without quota (`choose`).
    pub(super) fn table(&self, verdict: &Asked, only: Option<Vendor>) -> Option<Routed> {
        self.table_for(verdict, only, false, |_| true)
    }

    pub(super) fn table_for(
        &self,
        verdict: &Asked,
        only: Option<Vendor>,
        mission: bool,
        eligible: impl Fn(&Profile) -> bool,
    ) -> Option<Routed> {
        let now = self.secs();
        let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
        let usable: Vec<Profile> = candidates(&vendors, &self.codex_models)
            .into_iter()
            .filter(|p| {
                let allowed = if mission {
                    self.quota_allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false)
                } else {
                    self.allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false)
                };
                allowed.is_ok() && eligible(p)
            })
            .collect();
        let known: BTreeMap<Vendor, (Snap, Shape)> =
            vendors.iter().map(|v| (*v, (self.snap(*v, now), self.ledger.of(*v).shape()))).collect();
        let factor = |p: &Profile| {
            let (snap, shape) = &known[&p.vendor];
            Factor {
                m: snap.profile_m(family_of(p.model), shape, self.utc_offset, now),
                w: self.ledger.of(p.vendor).weights.w.get(p.model).copied().unwrap_or(1.0),
                step_ok: snap.step_ok(now),
            }
        };
        choose_paced(verdict.kind, verdict.difficulty, verdict.check, verdict.confidence, &usable, &factor)
    }

    /// While the user types in Auto.
    pub(super) fn route_draft(&mut self, req: WarmRequest, text: String) {
        let text = text.trim().to_string();
        if req.choice.vendor != "auto" || text.is_empty() {
            return;
        }
        let id = self.draft_for(&req);
        let Some(route) = self.next_route(&id) else { return };
        let only = route.map(|p| p.vendor);
        match &self.guess {
            Some(g) if g.request == text && g.only == only => {}
            Some(g) if g.result.is_none() => self.next_draft = Some(Draft { request: text, warm: id, only }),
            _ => self.start_guess(text, 0, Some(id), only),
        }
    }

    /// The route for a sent message: the draft's while it still holds, or a new one.
    pub(super) fn route_for(&mut self, request: &str, attached: usize, only: Option<Vendor>) -> oneshot::Receiver<Option<Asked>> {
        let (tx, rx) = oneshot::channel();
        match self.guess.as_mut().filter(|g| attached == 0 && g.only == only && covers(&g.request, request)) {
            Some(Guess { result: Some(routed), .. }) => drop(tx.send(*routed)),
            Some(g) => g.waiters.push(tx),
            None => {
                self.start_guess(request.trim().to_string(), attached, None, only);
                if let Some(g) = &mut self.guess {
                    g.waiters.push(tx);
                }
            }
        }
        rx
    }

    /// The router answered (or gave up): the waiting message starts, and a routed draft gets its CLI ready.
    fn guessed(&mut self, request: &str, only: Option<Vendor>, verdict: Option<Asked>) {
        let Some(g) = self.guess.as_mut().filter(|g| g.request == request && g.only == only && g.result.is_none()) else { return };
        g.result = Some(verdict);
        let (waiters, warm) = (std::mem::take(&mut g.waiters), g.warm.take());
        for waiter in waiters {
            let _ = waiter.send(verdict);
        }
        // A new conversation's CLI gets ready for the pick of this moment; the pick at dispatch may differ, and then the
        // process started here is replaced.
        if let (Some(r), Some(id)) = (verdict.and_then(|v| self.table(&v, only)), warm)
            && self.convs.get(&id).is_some_and(|c| c.messages.is_empty())
        {
            self.warm_launch(&id, &r.pick());
        }
        if let Some(d) = self.next_draft.take()
            && (d.request != request || d.only != only)
        {
            self.start_guess(d.request, 0, Some(d.warm), d.only);
        }
    }

    fn start_guess(&mut self, request: String, attached: usize, warm: Option<String>, only: Option<Vendor>) {
        let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
        let cands = candidates(&vendors, &self.codex_models);
        // One candidate needs no router: the everyday default answers.
        let (answer, stop) = if cands.len() > 1 { self.ask_router(question(&request, attached)) } else { (None, None) };
        if let Some(old) = self.guess.replace(Guess { request: request.clone(), result: None, waiters: vec![], warm, only, stop }) {
            stop_router(old.stop);
        }
        let me = self.me.clone();
        tokio::spawn(async move {
            let routed = match answer {
                Some(rx) => tokio::time::timeout(ASK_LIMIT, rx).await.ok().and_then(|r| r.ok()).flatten().and_then(|a| asked(&a)),
                None => None,
            };
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(g) = core.guess.as_mut().filter(|g| g.request == request && g.only == only) {
                    stop_router(g.stop.take());
                }
                core.guessed(&request, only, routed);
            }
        });
    }

    /// Asks the fastest cheap connected model once, in a session that reads and runs nothing.
    fn ask_router(&mut self, prompt: String) -> (Option<oneshot::Receiver<Option<String>>>, Option<Stop>) {
        if let Some(ask) = &self.opts.ask {
            let (tx, rx) = oneshot::channel();
            let answer = ask(prompt);
            tokio::spawn(async move { drop(tx.send(answer.await)) });
            return (Some(rx), None);
        }
        let Some(Spare { session, answer, asked, vendor, model_name, .. }) = self.spare.take().or_else(|| self.open_router()) else {
            return (None, None);
        };
        if self.allows(vendor, family_of(model_name), Purpose::Router, false).is_err() || self.begin_unbounded(vendor).is_err() {
            session.close();
            return (None, None);
        }
        *asked.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.secs());
        session.send(UserTurn::text(prompt));
        let me = self.me.clone();
        tokio::spawn(async move {
            tokio::time::sleep(ASK_LIMIT).await;
            if let (Some(at), Some(core)) = (asked.lock().unwrap_or_else(|e| e.into_inner()).take(), me.upgrade()) {
                core.lock().unwrap_or_else(|e| e.into_inner()).note_span(vendor, at, model_name, None, true);
            }
        });
        (Some(answer), Some(Arc::new(Mutex::new(Some(session)))))
    }

    /// A router session waiting for its question: a thread on the running Codex process, or a Claude process of its own.
    fn open_router(&mut self) -> Option<Spare> {
        let router = ROUTER.into_iter().find(|(vendor, model, _)| {
            self.opts.clis.contains_key(vendor)
                && (*vendor != Vendor::Codex || self.codex_models.iter().any(|m| m == model))
                && self.allows(*vendor, family_of(model), Purpose::Router, false).is_ok()
        });
        let (vendor, model, effort) = router?;
        self.open_aux(vendor, model, effort, INSTRUCTIONS, Purpose::Router)
    }

    pub(super) fn open_aux(
        &mut self,
        vendor: Vendor,
        model: &'static str,
        effort: &str,
        instructions: &str,
        purpose: Purpose,
    ) -> Option<Spare> {
        self.allows(vendor, family_of(model), purpose, false).ok()?;
        let cli = self.opts.clis.get(&vendor)?.clone();
        let (tx, answer) = oneshot::channel();
        let text = Arc::new(Mutex::new(String::new()));
        let tx = Arc::new(Mutex::new(Some(tx)));
        let me = self.me.clone();
        let asked = Arc::new(Mutex::new(None));
        let ran = asked.clone();
        let stop = Stop::default();
        let closer = stop.clone();
        let sink: EventSink = Arc::new(move |e| match e {
            EngineEvent::Text(t) => text.lock().unwrap_or_else(|e| e.into_inner()).push_str(&t),
            e @ (EngineEvent::Done { .. } | EngineEvent::Error { .. }) => {
                if let (Some(start), Some(core)) = (ran.lock().unwrap_or_else(|e| e.into_inner()).take(), me.upgrade()) {
                    let (cost, incomplete) = usage::fresh_price(&e, model);
                    core.lock().unwrap_or_else(|e| e.into_inner()).note_span(vendor, start, model, cost, incomplete);
                }
                let ok = matches!(e, EngineEvent::Done { ok: true, .. });
                if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(ok.then(|| std::mem::take(&mut *text.lock().unwrap_or_else(|e| e.into_inner()))));
                }
            }
            // The router's process sees account-wide news too (Codex hands it to one thread): the ledger must not miss it.
            e @ (EngineEvent::Quota { .. } | EngineEvent::Signals(_)) => {
                if let Some(core) = me.upgrade() {
                    let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                    c.note_account_event(vendor, e);
                    if c.allows(vendor, family_of(model), purpose, false).is_err() {
                        stop_router(Some(closer.clone()));
                    }
                }
            }
            _ => {}
        });
        let model_name = model;
        let (cwd, instructions) = (self.opts.data_dir.clone(), Some(instructions.to_string()));
        let (model, effort) = (Some(model.to_string()), Some(effort.to_string()));
        let session = match vendor {
            Vendor::Codex => self.codex_server(Area::Chat, false, Permissions::Isolated, &cli).open(
                CodexThread {
                    leave_plan: false,
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    progress: false,
                    verify: false,
                    cwd,
                    model,
                    effort,
                    resume: None,
                    instructions,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                },
                sink,
            ),
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        progress: false,
                        verify: false,
                        area: Area::Chat,
                        model,
                        effort,
                        resume: None,
                        user_hooks: false,
                        permissions: Permissions::Isolated,
                        instructions,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        flags: self.claude_flags_for(&cli),
                        tuning: Default::default(),
                        tools: Default::default(),
                    },
                    spawn: spawner(&self.opts.spawn, &cli),
                    sandbox: None,
                },
                sink,
            ),
        };
        *stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
        self.next_sid += 1;
        Some(Spare { session, answer, id: self.next_sid, vendor, asked, model_name })
    }

    /// Starts a routed first message once its route is known, or the everyday default when the route is late.
    pub(super) fn after_route(
        &mut self,
        id: &str,
        reply_id: &str,
        routed: Option<Routed>,
        fallback: Pick,
        text: String,
        images: Vec<Image>,
    ) {
        let waiting = |c: &Core| {
            c.convs.get(id).and_then(|c| c.messages.iter().find(|m| m.id == reply_id)).is_some_and(|m| m.status == Some(Status::Streaming))
        };
        if !waiting(self) {
            return;
        }
        // Nobody else waits for a route that came too late: its session stops now.
        if routed.is_none()
            && let Some(g) =
                self.guess.as_mut().filter(|g| g.result.is_none() && g.warm.is_none() && g.waiters.iter().all(|w| w.is_closed()))
        {
            stop_router(g.stop.take());
        }
        if let (Some(r), Some(t)) = (&routed, self.transfers.get_mut(id)) {
            t.check = r.check;
        }
        let pick = routed.map(|r| r.pick()).unwrap_or(fallback);
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.vendor = Some(pick.vendor);
            m.model = pick.model.clone();
            m.reason = Some(pick.reason.clone());
            m.pace = pick.pace.clone();
        }
        self.save(id);
        if let Err(error) = self.dispatch(id, reply_id, &pick, text, images, false) {
            self.fail_reply(id, reply_id, error);
        }
    }

    /// Stop on an answer still waiting for its route: there is no CLI to interrupt yet, so it simply stops.
    pub(super) fn stop_waiting(&mut self, id: &str) {
        let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut()).filter(|m| m.status == Some(Status::Streaming)) else {
            return;
        };
        m.status = Some(Status::Stopped);
        m.wait_until = None;
        m.permissions = None;
        self.transfers.remove(id);
        let reply_id = m.id.clone();
        self.save(id);
        self.push(id, Some(&reply_id), true);
    }

    /// Gets the router ready when the user starts typing in Auto. Codex answers on its running process; a cold Claude
    /// process takes about 9 s, so one starts now and waits for the question, closing when it goes unused.
    pub(super) fn warm_router(&mut self) {
        if self.opts.ask.is_some() || self.spare.is_some() {
            return;
        }
        let codex = self.opts.clis.get(&Vendor::Codex).cloned().filter(|_| self.codex_models.iter().any(|m| m == ROUTER[0].1));
        if let Some(cli) = codex {
            self.codex_server(Area::Chat, false, Permissions::Isolated, &cli);
            return;
        }
        let Some(spare) = self.open_router().filter(|s| s.vendor == Vendor::Claude) else { return };
        let (me, id, idle) = (self.me.clone(), spare.id, self.opts.idle);
        self.spare = Some(spare);
        tokio::spawn(async move {
            tokio::time::sleep(idle).await;
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if core.spare.as_ref().is_some_and(|s| s.id == id) {
                    core.close_spare();
                }
            }
        });
    }

    pub(super) fn close_spare(&mut self) {
        if let Some(spare) = self.spare.take() {
            spare.session.close();
        }
    }
}

pub(super) fn stop_router(stop: Option<Stop>) {
    if let Some(session) = stop.and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).take()) {
        session.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rewind_a_guessed_project_route_never_starts_before_its_message() {
        let data = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                clis: BTreeMap::from([(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] })]),
                spawn: Arc::new(|_, _, _| panic!("project startup must wait for capture")),
                windows: false,
                claude_env_billed: false,
                max_warm: 3,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        );
        let project = engine.add_project(&data.path().to_string_lossy()).unwrap();
        engine.note_account_events(
            Vendor::Claude,
            vec![EngineEvent::Signals(crate::signals::Signals { billing: Some(Billing::Subscription), ..Default::default() })],
        );
        engine.with(|core| {
            let verdict = asked(r#"{"kind":"code","difficulty":"easy","confidence":0.95}"#).unwrap();
            assert!(core.table(&verdict, None).is_some(), "the route must have a launchable pick");
            for area in [Area::Project, Area::Chat] {
                let conversation = serde_json::from_value(serde_json::json!({
                    "id":"draft", "title":"Draft", "area": area, "projectId": project.id, "workingFolder": data.path(),
                    "createdAt":1, "updatedAt":1, "messages":[]
                }))
                .unwrap();
                core.convs.insert("draft".into(), conversation);
                core.guess = Some(Guess {
                    request: "fix".into(),
                    result: None,
                    waiters: vec![],
                    warm: Some("draft".into()),
                    only: None,
                    stop: None,
                });
                core.guessed("fix", None, Some(verdict));
                assert!(core.live.is_empty(), "a guessed draft route waits for its baseline in either area");
            }
        });
        engine.shutdown();
    }
}
