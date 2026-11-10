//! Auto: routes a conversation's messages (see `route`), guessing while the user types so the answer can start the
//! moment it is sent. Once a message needed more than an easy model, the conversation stays with it.

use super::*;
use crate::auto_learned::{self, Signal};
use crate::budget::{Purpose, Shape, Snap, Why, family_of, gate};
use crate::route::{
    Asked, Factor, INSTRUCTIONS, PROFILES, Profile, ROUTER, Reading, Routed, asked, candidates, choose_paced, lifted, question, read_cost,
};
use crate::router::pinned;
use tokio::sync::oneshot;

/// How long a sent message may wait before the last choice or everyday default answers.
pub(super) const ROUTE_BUDGET: Duration = Duration::from_millis(2500);
/// Draft work includes CLI startup and inference (recorded medians exceed the send budget).
const ASK_LIMIT: Duration = Duration::from_secs(30);

/// How many times the draft of one message is routed while the person types it.
const DRAFT_ROUTES: u8 = 2;

type RouteAnswer = Option<(Option<Asked>, crate::act::Actor)>;

/// A route for a request, ready or on its way.
pub(super) struct Guess {
    request: String,
    /// What the small model said about the request (kind, level, confidence). The table runs when the message is sent, on the
    /// quota of that moment, so only this is kept.
    result: Option<RouteAnswer>,
    actor: crate::act::Actor,
    waiters: Vec<oneshot::Sender<RouteAnswer>>,
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
    fn route_profiles(&self, only: Option<Vendor>) -> Vec<Profile> {
        let vendors: Vec<_> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
        candidates(&vendors, &self.codex_models)
            .into_iter()
            .filter(|p| self.allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false).is_ok())
            .collect()
    }

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
        self.table_for(verdict, only, false, false, &Reading::default(), |_| true)
    }

    /// The same pick for the first message of a conversation, one rung up for a kind of task the person keeps redoing. Only a
    /// conversation's start learns: rule 3 lets nothing else change a model.
    pub(super) fn table_first(&self, verdict: &Asked, only: Option<Vendor>, reading: &Reading) -> Option<Routed> {
        self.table_for(verdict, only, false, true, reading, |_| true)
    }

    /// What the next answer of conversation `id` must read: the new message (`chars` of text, `images`, `file_bytes` of
    /// files) and the context its agent holds on the model it has been answering with (`before`).
    pub(super) fn reading(&self, id: &str, chars: usize, images: usize, file_bytes: usize, before: Option<&Pick>) -> Reading {
        let held = self.convs.get(id).and_then(|c| c.context).map_or(0, |c| c.used);
        let on = before.and_then(|p| p.model.as_deref().map(|m| pinned(p.vendor, m)));
        Reading::of(chars, images, file_bytes, held, on)
    }

    /// The pick of a message that may be the first of its conversation (`before` is the model so far, none for the first); a
    /// conversation under way only moves up.
    pub(super) fn table_at(&self, verdict: &Asked, only: Option<Vendor>, before: Option<&Pick>, reading: &Reading) -> Option<Routed> {
        let routed = if before.is_none() {
            self.table_first(verdict, only, reading)
        } else {
            self.table_for(verdict, only, false, false, reading, |_| true)
        };
        routed.filter(|r| before.is_none_or(|p| !r.weaker_than(p)))
    }

    pub(super) fn table_for(
        &self,
        verdict: &Asked,
        only: Option<Vendor>,
        mission: bool,
        learned: bool,
        reading: &Reading,
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
                read: read_cost(p, reading),
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
        if c.temporary {
            return None;
        }
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

    pub(super) fn sent_draft(&mut self, id: &str) {
        self.draft_routes.remove(id);
        // Already-started work can settle its price, but queued draft work must not start after this send.
        if self.next_draft.as_ref().is_some_and(|d| d.warm == id) {
            self.next_draft = None;
        }
    }

    /// A sent message uses a finished draft route; cold or pending classification never delays a send.
    pub(super) fn route_for(&mut self, id: &str, request: &str, attached: usize, only: Option<Vendor>) -> oneshot::Receiver<RouteAnswer> {
        self.sent_draft(id);
        let (tx, rx) = oneshot::channel();
        if self.route_profiles(only).len() <= 1 {
            let _ = tx.send(None);
            return rx;
        }
        // An injected classifier has no CLI startup. Native routing runs while typing, never on the send's clock.
        if self.opts.ask.is_none()
            && !self.guess.as_ref().is_some_and(|g| attached == 0 && g.only == only && covers(&g.request, request) && g.result.is_some())
        {
            let _ = tx.send(None);
            return rx;
        }
        match self.guess.as_mut().filter(|g| attached == 0 && g.only == only && covers(&g.request, request)) {
            Some(Guess { result: Some(routed), .. }) => drop(tx.send(routed.clone())),
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
    fn guessed(&mut self, request: &str, only: Option<Vendor>, verdict: Option<Asked>, attempted: bool) {
        let Some(g) = self.guess.as_mut().filter(|g| g.request == request && g.only == only && g.result.is_none()) else { return };
        let routed = attempted.then(|| (verdict, g.actor.clone()));
        g.result = Some(routed.clone());
        let (waiters, warm) = (std::mem::take(&mut g.waiters), g.warm.take());
        for waiter in waiters {
            let _ = waiter.send(routed.clone());
        }
        // A new conversation's CLI gets ready for the pick of this moment; the pick at dispatch may differ, and then the
        // process started here is replaced.
        if let (Some(r), Some(id)) = (verdict.and_then(|v| self.table_first(&v, only, &Reading::default())), warm)
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
        let cands = self.route_profiles(only);
        // Availability and quota can leave one model/effort profile, even with several subscriptions.
        let (answer, stop, actor) =
            if cands.len() > 1 { self.ask_router(question(&request, attached)) } else { (None, None, crate::act::Actor::Faber) };
        if let Some(old) = self.guess.replace(Guess { request: request.clone(), result: None, actor, waiters: vec![], warm, only, stop }) {
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
                core.guessed(&request, only, routed, cands.len() > 1);
            }
        });
    }

    /// Asks the fastest cheap connected model once, in a session that reads and runs nothing.
    fn ask_router(&mut self, prompt: String) -> (Option<oneshot::Receiver<Option<String>>>, Option<Stop>, crate::act::Actor) {
        if let Some(ask) = &self.opts.ask {
            let (tx, rx) = oneshot::channel();
            let answer = ask(prompt);
            tokio::spawn(async move { drop(tx.send(answer.await)) });
            return (Some(rx), None, crate::act::Actor::Faber);
        }
        let Some(Spare { session, answer, asked, vendor, model_name, .. }) = self.spare.take().or_else(|| self.open_router()) else {
            return (None, None, crate::act::Actor::Faber);
        };
        if self.allows(vendor, family_of(model_name), Purpose::Router, false).is_err() || self.begin_unbounded(vendor).is_err() {
            session.close();
            return (None, None, crate::act::Actor::Faber);
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
        (
            Some(answer),
            Some(Arc::new(Mutex::new(Some(session)))),
            crate::act::Actor::Ai { vendor, model: Some(model_name.into()), role: crate::act::ActorRole::Router },
        )
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
                    temporary: false,
                    title: None,
                    leave_plan: false,
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    leads: false,
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
                        temporary: false,
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        leads: false,
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
        fallback: (Pick, crate::act::Actor, bool),
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
        // The router AI is named only when its verdict made the pick; a verdict the table rejected leaves Faber's own fallback.
        let from_router = routed.is_some();
        let mut pick = routed.map(|r| r.pick()).unwrap_or(fallback.0);
        let route_reason = pick.reason.starts_with("reason.route.").then(|| pick.reason.clone());
        let only = self.convs.get(id).filter(|c| c.messages.len() > 2).map(|_| pick.vendor);
        if let [profile] = self.route_profiles(only).as_slice() {
            let current =
                PROFILES.iter().find(|p| p.vendor == pick.vendor && Some(p.model) == pick.model.as_deref() && p.effort == pick.effort);
            if only.is_none() || current.is_none_or(|p| (profile.iq, profile.agent) >= (p.iq, p.agent)) {
                pick = Pick {
                    vendor: profile.vendor,
                    model: Some(profile.model.into()),
                    effort: profile.effort.into(),
                    reason: "reason.autoSingleProfile".into(),
                    pace: None,
                };
            }
        } else if !from_router && !fallback.2 && self.route_profiles(only).len() > 1 {
            pick.reason = "reason.autoImmediate".into();
        }
        let shown_pick = pick.resolved(self.codex_recommended.as_deref(), &self.codex_models);
        // Immediate and single-profile explanations must not erase the route's difficulty for later messages.
        if let Some(reason) = route_reason {
            pick.reason = reason;
        }
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.vendor = Some(pick.vendor);
            m.model = pick.model.clone();
            m.reason = Some(pick.reason.clone());
            m.pace = pick.pace.clone();
            let mut act = crate::act::FaberAct::route(&shown_pick, if from_router { fallback.1 } else { crate::act::Actor::Faber });
            if fallback.2 && shown_pick.reason != "reason.autoSingleProfile" {
                act.why = Some("act.why.routerTimeout".into());
            }
            m.acts.push(act);
        }
        self.save(id);
        if let Err(error) = self.dispatch(id, reply_id, &pick, text, images, false) {
            self.fail_reply(id, reply_id, error);
            self.step_not_started(id);
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
        if self.opts.ask.is_some() || self.spare.is_some() || self.route_profiles(None).len() == 1 {
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

    fn auto_send(text: &str) -> SendRequest {
        SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: text.into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "auto".into(), model: None, effort: None },
        }
    }
    async fn wait_for(mut ready: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fake CLI did not reach the expected state");
    }
    async fn draft_finished(engine: &Engine, text: &str) {
        wait_for(|| engine.with(|c| c.guess.as_ref().is_some_and(|g| g.request == text && g.result.is_some()))).await;
    }
    async fn turn_finished(engine: &Engine, id: &str, turn: usize) -> Message {
        wait_for(|| engine.open(id).is_some_and(|c| c.messages.get(turn * 2 + 1).is_some_and(|m| m.status == Some(Status::Done)))).await;
        engine.open(id).unwrap().messages[turn * 2 + 1].clone()
    }
    // A native fake CLI: each draft has its own verdict; the second one cannot finish until the test releases it.
    async fn three_message_fallback(vendor: Vendor, path: &str) {
        use crate::events::EngineEvent;
        use crate::proc::{Proc, Spawner};
        use serde_json::json;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        async fn emit(output: &mut tokio::io::DuplexStream, msg: serde_json::Value) {
            output.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        }

        let dir = crate::workspace::TestRoot::new();
        let drafts = Arc::new(Mutex::new(Vec::new()));
        let release = Arc::new(tokio::sync::Notify::new());
        let (seen, held) = (drafts.clone(), release.clone());
        let queued = matches!(path, "queue" | "steer" | "other");
        let spawn: Spawner = Arc::new(move |_, args, _| {
            let (stdin, input) = tokio::io::duplex(65536);
            let (mut output, stdout) = tokio::io::duplex(65536);
            let mut input = tokio::io::BufReader::new(input);
            let model = args.windows(2).find(|p| p[0] == "--model").map(|p| p[1].clone()).unwrap_or_default();
            let claude_router = args.iter().any(|s| s.contains("You classify"));
            let (seen, held) = (seen.clone(), held.clone());
            tokio::spawn(async move {
                let mut threads = BTreeMap::new();
                let mut line = String::new();
                while input.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                    line.clear();
                    let method = msg["method"].as_str().unwrap_or_default();
                    if matches!(method, "thread/start" | "thread/resume") {
                        let id = msg["params"]["threadId"].as_str().map(str::to_string).unwrap_or_else(|| format!("thread-{}", msg["id"]));
                        threads.insert(id.clone(), msg["params"]["baseInstructions"].as_str().unwrap_or_default().contains("You classify"));
                        emit(&mut output, json!({"id":msg["id"],"result":{"model":msg["params"]["model"],"thread":{"id":id,"turns":[]}}}))
                            .await;
                        continue;
                    }
                    if method == "config/read" {
                        emit(&mut output, json!({"id":msg["id"],"result":{"config":{}}})).await;
                        continue;
                    }
                    if method == "thread/unsubscribe" {
                        emit(&mut output, json!({"id":msg["id"],"result":{"status":"unsubscribed"}})).await;
                        continue;
                    }
                    if method != "turn/start" && msg["type"] != "user" {
                        if !msg["id"].is_null() {
                            emit(&mut output, json!({"id":msg["id"],"result":{}})).await;
                        }
                        continue;
                    }
                    let router =
                        if vendor == Vendor::Claude { claude_router } else { threads[msg["params"]["threadId"].as_str().unwrap()] };
                    let text =
                        if vendor == Vendor::Claude { msg["message"]["content"].to_string() } else { msg["params"]["input"].to_string() };
                    let hard = text.contains("billing service");
                    if router {
                        seen.lock().unwrap().push(text.clone());
                        if text.contains("pong") {
                            held.notified().await;
                        }
                    }
                    let answer = if router {
                        json!({"kind":if hard {"code"} else {"chat"},"difficulty":if hard {"hard"} else {"easy"},"confidence":0.95})
                            .to_string()
                    } else {
                        "Answer".into()
                    };
                    if vendor == Vendor::Claude {
                        emit(&mut output, json!({"type":"system","subtype":"init","session_id":"test","model":model})).await;
                        emit(
                            &mut output,
                            json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":answer}}}),
                        )
                        .await;
                        if !router && queued {
                            continue;
                        }
                        emit(
                            &mut output,
                            json!({"type":"result","subtype":"success","is_error":false,"result":answer,"total_cost_usd":0.001,"usage":{}}),
                        )
                        .await;
                    } else {
                        let thread = &msg["params"]["threadId"];
                        emit(&mut output, json!({"id":msg["id"],"result":{"turn":{"id":"turn","status":"inProgress","items":[]}}})).await;
                        emit(&mut output, json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":"turn"}}})).await;
                        emit(&mut output, json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":"turn","itemId":"answer","delta":answer}})).await;
                        if !router && queued {
                            continue;
                        }
                        emit(&mut output, json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":"turn","status":"completed","items":[]}}})).await;
                    }
                }
            });
            Ok(Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None })
        });
        let engine = Engine::new(
            EngineOptions {
                data_dir: dir.path().into(),
                #[cfg(windows)]
                chat_dir: dir.work().into(),
                clis: BTreeMap::from([(vendor, Cli { command: "fake".into(), prefix: vec![] })]),
                spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        engine.note_account_events(
            vendor,
            vec![EngineEvent::Signals(crate::signals::Signals {
                billing: Some(crate::budget::Billing::Subscription),
                ..Default::default()
            })],
        );
        engine.set_codex_models(vec!["gpt-6-luna".into(), "gpt-6.1-sol".into()], Some("gpt-6.1-sol".into()));
        let warm = |id| WarmRequest {
            conversation_id: id,
            area: Area::Chat,
            project_id: None,
            choice: Choice { vendor: "auto".into(), model: None, effort: None },
        };
        engine.route_draft(warm(None), "In one sentence: why is the sky blue?".into());
        // A finished first draft warms the answering CLI, then Send consumes the cached verdict.
        draft_finished(&engine, "In one sentence: why is the sky blue?").await;
        let id = engine.send(auto_send("In one sentence: why is the sky blue?")).unwrap();
        engine.rename(&id, "Three messages");
        if queued {
            wait_for(|| engine.open(&id).unwrap().messages[1].text == "Answer").await;
            assert_eq!(engine.open(&id).unwrap().messages[1].reason.as_deref(), Some("reason.route.chat.easy"));
            let send = || engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto_send("A queued followup") }).unwrap();
            if path == "steer" {
                send();
            }
            let pending = "Reply with just the word: pong";
            engine.route_draft(warm(Some(id.clone())), pending.into());
            wait_for(|| drafts.lock().unwrap().len() == 2).await;
            let owner = if path == "other" { engine.with(|c| c.blank(Area::Chat, None)) } else { id.clone() };
            engine.route_draft(warm(Some(owner.clone())), "A queued draft".into());
            assert!(engine.with(|c| c.next_draft.as_ref().is_some_and(|d| d.warm == owner)));
            if path == "steer" {
                let item = engine.open(&id).unwrap().queued[0].id.clone();
                engine.send_queued(&id, &item).unwrap();
                assert!(engine.open(&id).unwrap().queued[0].sending);
            } else {
                send();
                assert_eq!(engine.open(&id).unwrap().queued.len(), 1);
            }
            assert_eq!(
                engine.with(|c| c.next_draft.is_some()),
                path == "other",
                "accepted sends discard only their conversation's queued draft"
            );
            release.notify_one();
            draft_finished(&engine, if path == "other" { "A queued draft" } else { pending }).await;
            assert_eq!(drafts.lock().unwrap().len(), if path == "other" { 3 } else { 2 }, "sending must not spend quota on a queued draft");
            engine.shutdown();
            return;
        }
        let first = turn_finished(&engine, &id, 0).await;
        assert_eq!(first.reason.as_deref(), Some("reason.route.chat.easy"));
        let second_text = "Reply with just the word: pong";
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
        let single_profile = |used| {
            if vendor == Vendor::Codex {
                engine.set_codex_models(
                    if used { vec!["gpt-6-luna".into()] } else { vec!["gpt-6-luna".into(), "gpt-6.1-sol".into()] },
                    Some("gpt-6-luna".into()),
                );
            } else {
                engine.note_account_events(
                    vendor,
                    ["seven_day_fable", "seven_day_opus", "seven_day_sonnet"]
                        .map(|window| EngineEvent::Quota {
                            window: window.into(),
                            used: if used { 1.0 } else { 0.0 },
                            resets_at: now + if used { 3600.0 } else { 7200.0 },
                        })
                        .to_vec(),
                );
            }
        };
        if path == "pending" {
            engine.route_draft(warm(Some(id.clone())), second_text.into());
            wait_for(|| drafts.lock().unwrap().len() == 2).await;
        } else if path == "single" {
            single_profile(true);
        }
        engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto_send(second_text) }).unwrap();
        let second = turn_finished(&engine, &id, 1).await;
        assert_eq!(second.model, first.model);
        assert_eq!(second.acts[0].why.as_deref(), Some(if path == "single" { "reason.autoSingleProfile" } else { "reason.autoImmediate" }));
        if path == "pending" {
            release.notify_one();
            draft_finished(&engine, second_text).await;
            assert_eq!(engine.open(&id).unwrap().messages[3].model, first.model, "a late classifier cannot change the sent turn");
        }
        if path == "single" {
            single_profile(false);
        }
        let hard = "Now refactor my billing service";
        engine.route_draft(warm(Some(id.clone())), hard.into());
        let expected = if path == "pending" { 3 } else { 2 };
        if engine.with(|c| c.guess.as_ref().is_some_and(|g| g.request == hard)) {
            draft_finished(&engine, hard).await;
        }
        engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto_send(hard) }).unwrap();
        let third = turn_finished(&engine, &id, 2).await;
        assert_eq!(third.vendor, Some(vendor));
        assert_eq!(third.reason.as_deref(), Some("reason.route.code.hard"));
        assert_ne!(third.model, first.model);
        assert_eq!(drafts.lock().unwrap().len(), expected);
        assert_eq!(engine.open(&id).unwrap().messages[3].reason.as_deref(), Some("reason.route.chat.easy"));
        let saved = engine.with(|c| c.store.get(&id).unwrap());
        assert_eq!(saved.messages[3].reason.as_deref(), Some("reason.route.chat.easy"));
        assert_eq!(saved.messages[3].acts[0].why, second.acts[0].why);
        engine.shutdown();
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_a_pending_draft() {
        three_message_fallback(Vendor::Claude, "pending").await;
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_a_pending_draft_codex() {
        three_message_fallback(Vendor::Codex, "pending").await;
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_a_cold_followup() {
        three_message_fallback(Vendor::Claude, "cold").await;
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_a_cold_followup_codex() {
        three_message_fallback(Vendor::Codex, "cold").await;
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_one_profile_recovers() {
        three_message_fallback(Vendor::Claude, "single").await;
    }

    #[tokio::test]
    async fn three_messages_preserve_routing_after_one_profile_recovers_codex() {
        three_message_fallback(Vendor::Codex, "single").await;
    }

    #[tokio::test]
    async fn queued_send_discards_pending_draft_claude() {
        three_message_fallback(Vendor::Claude, "queue").await;
    }

    #[tokio::test]
    async fn queued_send_discards_pending_draft_codex() {
        three_message_fallback(Vendor::Codex, "queue").await;
    }

    #[tokio::test]
    async fn queued_steer_discards_pending_draft_claude() {
        three_message_fallback(Vendor::Claude, "steer").await;
    }

    #[tokio::test]
    async fn queued_steer_discards_pending_draft_codex() {
        three_message_fallback(Vendor::Codex, "steer").await;
    }

    #[tokio::test]
    async fn queued_send_preserves_other_conversation_draft_claude() {
        three_message_fallback(Vendor::Claude, "other").await;
    }

    #[tokio::test]
    async fn queued_send_preserves_other_conversation_draft_codex() {
        three_message_fallback(Vendor::Codex, "other").await;
    }

    #[tokio::test]
    async fn router_send_discards_queued_draft_work_for_that_conversation() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|c| {
            c.next_draft = Some(Draft { request: "queued".into(), warm: "sent".into(), only: None });
            drop(c.route_for("sent", "queued", 0, None));
            assert!(c.next_draft.is_none(), "sending must not start another classifier later for already-sent text");
            c.next_draft = Some(Draft { request: "other".into(), warm: "other-conversation".into(), only: None });
            drop(c.route_for("sent", "queued", 0, None));
            assert!(c.next_draft.is_some(), "another conversation still owns its draft");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn native_cold_and_pending_routes_are_ready_without_waiting_for_the_classifier() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|c| {
            c.opts.clis.insert(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] });
            c.ledger.claude.billing = Billing::Subscription;
            assert!(c.opts.ask.is_none());
            assert!(c.route_profiles(None).len() > 1);
            let mut cold = c.route_for("sent", "question", 0, None);
            assert_eq!(cold.try_recv().unwrap(), None, "a cold send is ready before any task or timer runs");
            assert!(c.guess.is_none(), "sending must not start a classifier");
            c.guess = Some(Guess {
                request: "question".into(),
                result: None,
                actor: crate::act::Actor::Faber,
                waiters: vec![],
                warm: Some("sent".into()),
                only: None,
                stop: None,
            });
            let mut pending = c.route_for("sent", "question", 0, None);
            assert_eq!(pending.try_recv().unwrap(), None, "a pending classifier cannot hold the send");
            assert!(c.guess.as_ref().unwrap().waiters.is_empty(), "the send must not subscribe to the pending answer");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn one_usable_profile_filters_quota_before_routing_or_warming() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|c| {
            for vendor in [Vendor::Claude, Vendor::Codex] {
                c.opts.clis.insert(vendor, Cli { command: "fake".into(), prefix: vec![] });
                c.ledger.of_mut(vendor).billing = Billing::Subscription;
            }
            c.codex_models = vec!["gpt-6-luna".into()];
            c.opts.ask = Some(Arc::new(|_| panic!("the exhausted subscription is not a choice")));
        });
        engine.note_account_events(
            Vendor::Claude,
            vec![EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: unix_secs() + 3600.0 }],
        );
        let mut answer = engine.with(|c| {
            assert_eq!(c.route_profiles(None).len(), 1);
            assert!(c.route_profiles(Some(Vendor::Claude)).is_empty());
            c.route_for("sent", "test", 0, None)
        });
        assert_eq!(answer.try_recv().unwrap(), None, "no task, timeout or model request is needed");
        assert!(engine.ledger().codex.unmetered.is_empty());
        engine.shutdown();
    }

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
            c.try_save(&id).unwrap();
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
                    actor: crate::act::Actor::Faber,
                    waiters: vec![],
                    warm: Some("draft".into()),
                    only: None,
                    stop: None,
                });
                core.guessed("fix", None, Some(verdict), true);
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
            c.convs.get_mut(&id).unwrap().temporary = true;
            assert_eq!(c.kept_kind(&req), None, "temporary conversations teach nothing");
            c.convs.get_mut(&id).unwrap().temporary = false;
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
            c.convs.get_mut(&id).unwrap().temporary = true;
            for _ in 0..3 {
                again(c, "question-0", "question");
            }
            assert!(!lifted(c), "retrying a temporary conversation never teaches Auto");
            c.convs.get_mut(&id).unwrap().temporary = false;
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
