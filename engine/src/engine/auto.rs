//! Auto: routes a conversation's messages (see `route`), guessing while the user types so the answer can start the
//! moment it is sent. Once a message needed more than an easy model, the conversation stays with it.

use super::*;
use crate::auto_learned::{self, Signal};
use crate::budget::{Purpose, Shape, Snap, Why, family_of, gate};
use crate::route::{Asked, Factor, INSTRUCTIONS, PROFILES, Profile, ROUTER, Routed, asked, candidates, choose_paced, lifted, question};
use crate::router::pinned;
use tokio::sync::oneshot;

/// How long a sent message may wait for its route before the everyday default answers: a timeout, not a target (Erick,
/// 10/10/2026), so a very long request still gets routed. The router (GPT-6 Luna at medium effort on the Codex process
/// that is already running) measured p50 2.8 s and p90 3.9 s that day; about 2.2 s of it is the service's first word,
/// whatever the prompt size or service tier.
pub(super) const ROUTE_BUDGET: Duration = Duration::from_secs(30);
/// The router itself gives up at the same point, drafts included.
const ASK_LIMIT: Duration = ROUTE_BUDGET;

/// How many times the draft of one message is routed while the person types it. The route of the sent message is never
/// counted: it is asked for when the message leaves, or its draft's answer is reused.
const DRAFT_ROUTES: u8 = 2;

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
    pub(super) id: u64,
    pub(super) vendor: Vendor,
    pub(super) asked: Arc<Mutex<Option<f64>>>,
    pub(super) model_name: &'static str,
}

struct AuxEvents {
    sink: EventSink,
    answer: oneshot::Receiver<Option<String>>,
    asked: Arc<Mutex<Option<f64>>>,
    stop: Stop,
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
        self.table_for(verdict, only, false, false, |_| true)
    }

    /// The same pick for the first message of a conversation, one rung up for a kind of task the person keeps redoing. Only a
    /// conversation's start learns: rule 3 lets nothing else change a model.
    pub(super) fn table_first(&self, verdict: &Asked, only: Option<Vendor>) -> Option<Routed> {
        self.table_for(verdict, only, false, true, |_| true)
    }

    /// The pick of a message that may be the first of its conversation (`before` is the model so far, none for the first); a
    /// conversation under way only moves up.
    pub(super) fn table_at(&self, verdict: &Asked, only: Option<Vendor>, before: Option<&Pick>) -> Option<Routed> {
        let routed = if before.is_none() { self.table_first(verdict, only) } else { self.table(verdict, only) };
        routed.filter(|r| before.is_none_or(|p| !r.weaker_than(p)))
    }

    pub(super) fn table_for(
        &self,
        verdict: &Asked,
        only: Option<Vendor>,
        mission: bool,
        learned: bool,
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
        let routed = choose_paced(verdict.kind, verdict.difficulty, verdict.check, verdict.confidence, &usable, &factor)?;
        Some(if learned && self.learned.lifted(routed.kind, now as u64) { lifted(routed, &usable, &factor) } else { routed })
    }

    /// What a redone or kept answer says about the pick that made it. Rare events, written at once; a failed write only
    /// loses a nudge.
    fn learn(&mut self, kind: &str, signal: Signal) {
        self.learned.note(kind, signal, self.secs() as u64);
        let _ = self.store.save_auto_learned(&self.learned);
    }

    /// An answer that was just sent again: the same request asked once more says the pick was not good enough, and asked on the
    /// step up Faber offered it says so twice. An edited request is a new request and says nothing; nor does a redo on a model
    /// the person picked by hand that is weaker than the one that answered. `answer` is the reply to the question being redone,
    /// and `again` says that question is the first of its conversation and was asked word for word: only a first answer is
    /// Auto's own pick, the later ones keep its model.
    pub(super) fn learn_from_redo(&mut self, answer: &Message, again: bool, choice: &Choice) {
        let Some(kind) = answer.reason.as_deref().and_then(auto_learned::kind_of).map(str::to_string) else { return };
        if !again || answer.status != Some(Status::Done) || weaker_choice(answer, choice) {
            return;
        }
        let step_up = answer.nudge.as_ref().is_some_and(|n| {
            choice.vendor == n.vendor.name()
                && choice.model.as_deref().map(|m| pinned(n.vendor, m)) == Some(n.model.as_str())
                && choice.effort.map(|e| e.name()) == Some(n.effort.as_str())
        });
        self.learn(&kind, if step_up { Signal::StepUp } else { Signal::Redo });
    }

    /// A message sent after a conversation's first answer finished says that answer was kept; once per conversation, because
    /// only the first answer is Auto's own pick.
    pub(super) fn kept_kind(&mut self, req: &SendRequest) -> Option<String> {
        let c = self.load(req.conversation_id.as_deref()?)?;
        let last = c.messages.last().filter(|m| c.messages.len() == 2 && m.role == Role::Assistant && m.status == Some(Status::Done))?;
        last.reason.as_deref().and_then(auto_learned::kind_of).map(str::to_string)
    }

    pub(super) fn learn_kept(&mut self, kind: &str) {
        self.learn(kind, Signal::Kept);
    }

    /// Forgets everything Auto learned, in memory and on disk. Memory is cleared even when the file cannot be written, and the
    /// fault comes back so the screen can say Forget did not stick.
    pub(super) fn forget_learned(&mut self) -> Result<(), crate::store::Fault> {
        self.learned.forget();
        self.store.save_auto_learned(&self.learned)
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
        // A new route for the draft, other than the pending one this replaces, spends one of the message's routes.
        let replaces = self.next_draft.as_ref().is_some_and(|d| d.warm == id);
        let spent = self.draft_routes.get(&id).copied().unwrap_or(0);
        match &self.guess {
            Some(g) if g.request == text && g.only == only => {}
            _ if spent >= DRAFT_ROUTES && !replaces => {}
            Some(g) if g.result.is_none() => {
                self.next_draft = Some(Draft { request: text, warm: id.clone(), only });
                if !replaces {
                    self.draft_routes.insert(id, spent + 1);
                }
            }
            _ => {
                self.draft_routes.insert(id.clone(), spent + 1);
                self.start_guess(text, 0, Some(id), only);
            }
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
        if let (Some(r), Some(id)) = (verdict.and_then(|v| self.table_first(&v, only)), warm)
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
        self.open_aux(vendor, (model, effort), INSTRUCTIONS, Purpose::Router, None)
    }

    fn aux_events(&self, vendor: Vendor, model: &'static str, purpose: Purpose, bounded: Option<(usize, String)>) -> AuxEvents {
        let (tx, answer) = oneshot::channel();
        let text = Arc::new(Mutex::new(Some(String::new())));
        let tx = Arc::new(Mutex::new(Some(tx)));
        let me = self.me.clone();
        let asked = Arc::new(Mutex::new(None));
        let ran = asked.clone();
        let stop = Stop::default();
        let closer = stop.clone();
        let done = std::sync::atomic::AtomicBool::new(false);
        let sink: EventSink = Arc::new(move |e| match e {
            EngineEvent::Text(t) => {
                let mut output = text.lock().unwrap_or_else(|e| e.into_inner());
                let mut overflow = false;
                if let Some(value) = output.as_mut() {
                    if bounded.as_ref().is_some_and(|(limit, _)| value.len().saturating_add(t.len()) > *limit) {
                        *output = None;
                        overflow = true;
                    } else {
                        value.push_str(&t);
                    }
                }
                drop(output);
                if overflow {
                    if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(None);
                    }
                    stop_router(Some(closer.clone()));
                }
            }
            e @ (EngineEvent::Done { .. } | EngineEvent::Error { .. }) => {
                let final_cost = matches!(e, EngineEvent::Done { .. });
                if final_cost && done.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let start = ran.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(core) = me.upgrade() {
                    let (cost, incomplete) = usage::fresh_price(&e, model);
                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(start) = start {
                        core.note_span(vendor, start, model, cost, incomplete);
                    } else if final_cost && bounded.is_some() && cost.is_some() {
                        // Cancellation already settled the in-flight count. A late price must not settle another request.
                        let (now, offset) = (core.secs(), core.utc_offset);
                        core.ledger.of_mut(vendor).spend(cost, incomplete, now, offset);
                        core.save_ledger(true);
                        core.emit_usage();
                    }
                    if let Some((_, conv)) = &bounded
                        && cost.is_some()
                    {
                        core.load(conv);
                        core.add_spend(conv, cost);
                        core.save(conv);
                    }
                }
                let ok = matches!(e, EngineEvent::Done { ok: true, .. });
                if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(if ok { text.lock().unwrap_or_else(|e| e.into_inner()).take() } else { None });
                }
            }
            // The router's process sees account-wide news too (Codex hands it to one thread): the ledger must not miss it.
            e @ (EngineEvent::Quota { .. } | EngineEvent::Signals(_)) => {
                if let Some(core) = me.upgrade() {
                    let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                    c.note_account_event(vendor, e);
                    if c.allows(vendor, family_of(model), purpose, false).is_err() {
                        if bounded.is_some()
                            && let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take()
                        {
                            let _ = tx.send(None);
                        }
                        stop_router(Some(closer.clone()));
                    }
                }
            }
            _ => {}
        });
        AuxEvents { sink, answer, asked, stop }
    }

    pub(super) fn open_aux(
        &mut self,
        vendor: Vendor,
        profile: (&'static str, &str),
        instructions: &str,
        purpose: Purpose,
        bounded: Option<(usize, String)>,
    ) -> Option<Spare> {
        let (model, effort) = profile;
        self.allows(vendor, family_of(model), purpose, false).ok()?;
        let cli = self.opts.clis.get(&vendor)?.clone();
        let AuxEvents { sink, answer, asked, stop } = self.aux_events(vendor, model, purpose, bounded);
        let model_name = model;
        let (cwd, instructions) = (self.opts.data_dir.clone(), Some(instructions.to_string()));
        let (model, effort) = (Some(model.to_string()), Some(effort.to_string()));
        let session = match vendor {
            Vendor::Gemini => return None,
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
        self.worker_memory.registry.register(session.owned_tree());
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

    /// Stops a wait with no active CLI. Only the person's Stop cancels explicit recovery; closing preserves it.
    pub(super) fn stop_waiting(&mut self, id: &str, stop_recovery: bool) {
        let recovery = stop_recovery && self.transfers.get(id).is_some_and(|t| self.pending_turn(id, &t.reply).is_ok());
        let Some(m) =
            self.convs.get_mut(id).and_then(|c| c.messages.last_mut()).filter(|m| m.status == Some(Status::Streaming) || recovery)
        else {
            return;
        };
        m.status = Some(Status::Stopped);
        m.wait_until = None;
        m.error = None;
        m.recovery = None;
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

/// True when the person asked for a model by hand that the table knows to be weaker than the one that answered.
fn weaker_choice(answer: &Message, choice: &Choice) -> bool {
    let Some(vendor) = Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor) else { return false };
    let rank = |vendor, model: &str, effort: &str| {
        PROFILES.iter().find(|p| p.vendor == vendor && p.model == model && p.effort == effort).map(|p| (p.iq, p.agent))
    };
    let (Some(then), Some(model), Some(now)) = (answer.vendor, choice.model.as_deref(), answer.model.as_deref()) else { return false };
    let asked = rank(vendor, pinned(vendor, model), choice.effort.unwrap_or_default().name());
    let had = rank(then, now, answer.effort.as_deref().unwrap_or("medium"));
    matches!((asked, had), (Some(a), Some(h)) if a < h)
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
    async fn creative_aux_events_bound_output_and_settle_cost_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let AuxEvents { sink, answer, asked, stop } = engine.with(|c| {
            c.begin_unbounded(Vendor::Claude).unwrap();
            c.aux_events(Vendor::Claude, "claude-sonnet-5-5", Purpose::UserTurn { by_hand: false }, Some((4, id.clone())))
        });
        *asked.lock().unwrap() = Some(engine.with(|c| c.secs()));
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let got = closed.clone();
        *stop.lock().unwrap() = Some(Session::from_fn(move |command| {
            if matches!(command, crate::events::SessionCommand::Close) {
                got.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
        sink(EngineEvent::Text("éé".into())); // Exactly four UTF-8 bytes.
        sink(EngineEvent::Text("x".into()));
        sink(EngineEvent::Text("ignored after overflow".into()));
        assert_eq!(answer.await.unwrap(), None);
        assert_eq!(closed.load(std::sync::atomic::Ordering::SeqCst), 1);
        let done = EngineEvent::Done {
            ok: true,
            reason: "done".into(),
            cost_total_usd: Some(0.25),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        };
        sink(done.clone());
        sink(done);
        engine.with(|c| {
            assert_eq!(c.convs[&id].spent_usd, 0.25);
            assert_eq!(crate::project::budget::Budget::load(&c.ledger).unwrap().unbounded[&Vendor::Claude], 0);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn creative_aux_quota_revocation_closes_and_refuses_the_answer() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let AuxEvents { sink, answer, stop, .. } = engine.with(|c| {
            c.aux_events(Vendor::Claude, "claude-sonnet-5-5", Purpose::UserTurn { by_hand: false }, Some((4096, "synthetic".into())))
        });
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let got = closed.clone();
        *stop.lock().unwrap() = Some(Session::from_fn(move |command| {
            if matches!(command, crate::events::SessionCommand::Close) {
                got.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
        sink(EngineEvent::Signals(crate::signals::Signals { billing: Some(crate::budget::Billing::UsageBilled), ..Default::default() }));
        assert_eq!(answer.await.unwrap(), None);
        assert_eq!(closed.load(std::sync::atomic::Ordering::SeqCst), 1);
        engine.shutdown();
    }

    #[tokio::test]
    async fn creative_late_cost_after_cancel_is_counted_once_without_settling_another_request() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let AuxEvents { sink, asked, .. } = engine.with(|c| {
            c.begin_unbounded(Vendor::Claude).unwrap();
            c.aux_events(Vendor::Claude, "claude-sonnet-5-5", Purpose::UserTurn { by_hand: false }, Some((4096, id.clone())))
        });
        *asked.lock().unwrap() = Some(engine.with(|c| c.secs()));
        engine.with(|c| {
            let start = asked.lock().unwrap().take().unwrap();
            c.note_span(Vendor::Claude, start, "claude-sonnet-5-5", None, true);
            c.begin_unbounded(Vendor::Claude).unwrap(); // A different request is still active.
            c.save(&id);
            c.convs.remove(&id); // The stopped conversation can leave the in-memory cache before its final price arrives.
        });
        sink(EngineEvent::Error { message: "closed".into(), fatal: true });
        let done = EngineEvent::Done {
            ok: false,
            reason: "cancelled".into(),
            cost_total_usd: Some(0.25),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        };
        sink(done.clone());
        sink(done);
        engine.with(|c| {
            assert_eq!(c.convs[&id].spent_usd, 0.25);
            assert_eq!(crate::project::budget::Budget::load(&c.ledger).unwrap().unbounded[&Vendor::Claude], 1);
            assert_eq!(c.ledger.of(Vendor::Claude).own_spend.values().sum::<f64>(), 0.25);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn rewind_a_guessed_project_route_never_starts_before_its_message() {
        let data = crate::workspace::TestRoot::new();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
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
        )
        .unwrap();
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

    // Rule 3: the live process keeps what it was asked for, the account default included; the name the CLI reports never counts.
    #[tokio::test]
    async fn auto_keeps_the_model_the_live_process_was_asked_for_even_when_that_is_the_default() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("t1", &[])]);
        engine.with(|c| {
            let m = c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap();
            (m.model, m.effort, m.status) = (Some("the-name-the-cli-reported".into()), Some("medium".into()), Some(Status::Done));
            let before = same_as_before(&c.convs[&id], c.live.get(&id)).unwrap();
            assert_eq!(before.model, None, "asked for the default: the next message asks for the default too");
            c.live.get_mut(&id).unwrap().model = Some("asked".into());
            assert_eq!(same_as_before(&c.convs[&id], c.live.get(&id)).unwrap().model.as_deref(), Some("asked"));
            // No process (after a restart): the last reply's model is all there is.
            assert_eq!(same_as_before(&c.convs[&id], None).unwrap().model.as_deref(), Some("the-name-the-cli-reported"));
        });
        engine.shutdown();
    }

    fn answer(reason: &str, status: Status) -> Message {
        let mut m = Message::new("answer".into(), Role::Assistant, "x".into(), 1);
        m.vendor = Some(Vendor::Claude);
        m.model = Some("claude-sonnet-5-5".into());
        m.effort = Some("medium".into());
        m.reason = Some(reason.into());
        m.status = Some(status);
        m
    }

    fn choice(vendor: &str, model: Option<&str>) -> Choice {
        Choice { vendor: vendor.into(), model: model.map(String::from), effort: None }
    }

    /// Three times over, as the person would.
    fn retried(core: &mut Core, answer: &Message, same: bool, choice: &Choice) {
        for _ in 0..crate::auto_learned::UP_AT {
            core.learn_from_redo(answer, same, choice);
        }
    }

    fn lifted(core: &Core) -> bool {
        core.learned.lifted("code", core.secs() as u64)
    }

    #[tokio::test]
    async fn only_a_retry_of_a_finished_auto_answer_says_the_pick_was_not_good_enough() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|c| {
            let auto = choice("auto", None);
            let done = answer("reason.route.code.medium", Status::Done);
            // An edited request, or a question that is not the first of its conversation, is not Auto's pick asked again.
            retried(c, &done, false, &auto);
            // An answer that was stopped, still running, or that failed says nothing about the pick.
            for status in [Status::Stopped, Status::Streaming] {
                retried(c, &answer("reason.route.code.medium", status), true, &auto);
            }
            // An answer nobody routed: a model picked by hand, the default, or a key Auto never wrote.
            for reason in ["reason.chosen", "reason.autoDefault", "reason.route.poetry.easy"] {
                retried(c, &answer(reason, Status::Done), true, &auto);
            }
            // A hand-picked model weaker than the one that answered is exploring; the same or a stronger one is not.
            retried(c, &done, true, &choice("claude", Some("haiku")));
            assert!(!lifted(c), "none of these taught anything");
            retried(c, &done, true, &choice("claude", Some("opus")));
            assert!(lifted(c), "a retry on a stronger model counts");
        });
        engine.with(|c| {
            c.forget_learned().unwrap();
            retried(c, &answer("reason.route.code.medium", Status::Done), true, &choice("auto", None));
            assert!(lifted(c), "a retry on Auto counts");
            c.forget_learned().unwrap();
            retried(c, &answer("reason.route.code.medium", Status::Done), true, &choice("claude", Some("sonnet")));
            assert!(lifted(c), "and so does the same model again");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn taking_the_offered_step_up_counts_twice() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|c| {
            let mut done = answer("reason.route.code.medium", Status::Done);
            done.nudge = Some(crate::nudge::Nudge {
                reason: "reason.nudge.failed".into(),
                vendor: Vendor::Claude,
                model: "claude-sonnet-5-5".into(),
                effort: "high".into(),
            });
            let step = Choice { vendor: "claude".into(), model: Some("sonnet".into()), effort: Some(crate::router::Effort::High) };
            c.learn_from_redo(&done, true, &step);
            assert!(!lifted(c), "one step up is two points, not three");
            c.learn_from_redo(&done, true, &choice("auto", None));
            assert!(lifted(c), "two points and a plain retry make three");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_message_after_a_finished_auto_answer_keeps_it() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("t1", &[])]);
        engine.with(|c| {
            let req = SendRequest {
                conversation_id: Some(id.clone()),
                area: Area::Chat,
                project_id: None,
                text: "next".into(),
                images: vec![],
                files: vec![],
                choice: choice("auto", None),
            };
            let set = |c: &mut Core, reason: &str, status: Status| {
                let m = c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap();
                (m.reason, m.status) = (Some(reason.into()), Some(status));
            };
            set(c, "reason.route.code.hard", Status::Streaming);
            assert_eq!(c.kept_kind(&req), None, "an answer still arriving is not kept yet");
            set(c, "reason.route.code.hard", Status::Stopped);
            assert_eq!(c.kept_kind(&req), None, "a stopped answer says nothing");
            set(c, "reason.chosen", Status::Done);
            assert_eq!(c.kept_kind(&req), None, "a model picked by hand is not Auto's pick");
            set(c, "reason.route.code.hard", Status::Done);
            assert_eq!(c.kept_kind(&req).as_deref(), Some("code"));
            assert_eq!(c.kept_kind(&SendRequest { conversation_id: None, ..req.clone() }), None, "a new conversation keeps nothing");
            // Later answers keep the first one's model: they are not Auto's pick, so they are not marked again.
            let mut later = Message::new("later".into(), Role::Assistant, "x".into(), 3);
            (later.reason, later.status) = (Some("reason.route.code.hard".into()), Some(Status::Done));
            c.convs.get_mut(&id).unwrap().messages.extend([Message::new("q2".into(), Role::User, "more".into(), 3), later]);
            assert_eq!(c.kept_kind(&req), None, "a second answer says nothing about Auto's pick");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn only_the_first_question_of_a_conversation_asked_again_teaches() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("t1", &[])]);
        engine.with(|c| {
            let conversation = c.convs.get_mut(&id).unwrap();
            let mut second = Message::new("answer-2".into(), Role::Assistant, "x".into(), 4);
            conversation.messages.extend([Message::new("question-1".into(), Role::User, "more".into(), 3)]);
            second.vendor = Some(Vendor::Claude);
            conversation.messages.push(second);
            for m in conversation.messages.iter_mut().filter(|m| m.role == Role::Assistant) {
                (m.status, m.reason, m.vendor) = (Some(Status::Done), Some("reason.route.code.medium".into()), Some(Vendor::Claude));
            }
            // A refused retry teaches nothing, so the CLI must exist for the resend to start.
            c.opts.clis.insert(Vendor::Claude, crate::cli::Cli { command: "fake".into(), prefix: vec![] });
            let again = |c: &mut Core, message: &str, text: &str| {
                let request = BranchRequest {
                    conversation_id: id.clone(),
                    message_id: message.into(),
                    text: text.into(),
                    choice: choice("auto", None),
                };
                // The fake session answers nothing: only whether the resend started matters.
                let _ = c.branch(request, None, false);
            };
            for _ in 0..3 {
                again(c, "question-1", "more");
            }
            assert!(!lifted(c), "a later answer keeps the first one's model: it is not Auto's pick");
            // A retry the engine refuses (no AI to start) teaches nothing, however often it is clicked.
            c.opts.clis.clear();
            for _ in 0..3 {
                again(c, "question-0", "question");
            }
            assert!(!lifted(c), "a refused retry is not counted");
            c.opts.clis.insert(Vendor::Claude, crate::cli::Cli { command: "fake".into(), prefix: vec![] });
            for _ in 0..3 {
                again(c, "question-0", "question");
            }
            assert!(lifted(c), "the first question asked again word for word is");
        });
        engine.shutdown();
    }

    // A draft's route count starts again when Send is pressed, also when the message waits in the queue, and goes with a removed
    // conversation.
    #[tokio::test]
    async fn the_draft_count_starts_again_when_a_message_is_sent_queued_or_not_and_goes_with_the_conversation() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("t1", &[])]);
        engine.with(|c| {
            c.opts.clis.insert(Vendor::Claude, crate::cli::Cli { command: "fake".into(), prefix: vec![] });
            c.draft_routes.insert(id.clone(), 2)
        });
        let choice = crate::router::Choice { vendor: "claude".into(), model: None, effort: None };
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Chat,
            project_id: None,
            text: "next".into(),
            images: vec![],
            files: vec![],
            choice,
        };
        engine.send(req).unwrap();
        assert_eq!(engine.open(&id).unwrap().queued.len(), 1, "the reply is still running, so the message waits");
        assert!(engine.with(|c| c.draft_routes.is_empty()), "pressing Send was enough");
        engine.with(|c| c.draft_routes.insert(id.clone(), 1));
        engine.remove(&id);
        assert!(engine.with(|c| c.draft_routes.is_empty()), "nothing is kept for a removed conversation");
        engine.shutdown();
    }
}
