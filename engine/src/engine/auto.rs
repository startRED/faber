//! Auto: routes a conversation's messages (see `route`), guessing while the user types so the answer can start the
//! moment it is sent. Once a message needed more than an easy model, the conversation stays with it.

use super::*;
use crate::route::{INSTRUCTIONS, ROUTER, Routed, candidates, priced, question, read};
use tokio::sync::oneshot;

/// How long a sent message waits for its route before the everyday default answers. The router (GPT-6 Luna at
/// medium effort on the Codex process that is already running) measured 3 to 4.5 s on 10/10/2026.
pub(super) const ROUTE_BUDGET: Duration = Duration::from_secs(4);
/// A route for a draft may take a little longer: the user is still typing.
const ASK_LIMIT: Duration = Duration::from_secs(8);

/// A route for a request, ready or on its way.
pub(super) struct Guess {
    request: String,
    result: Option<Option<Routed>>,
    waiters: Vec<oneshot::Sender<Option<Routed>>>,
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

type Stop = Arc<Mutex<Option<Session>>>;

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
    pub(super) fn route_for(&mut self, request: &str, attached: usize, only: Option<Vendor>) -> oneshot::Receiver<Option<Routed>> {
        let (tx, rx) = oneshot::channel();
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
    fn guessed(&mut self, request: &str, only: Option<Vendor>, routed: Option<Routed>) {
        let Some(g) = self.guess.as_mut().filter(|g| g.request == request && g.only == only && g.result.is_none()) else { return };
        g.result = Some(routed.clone());
        let (waiters, warm) = (std::mem::take(&mut g.waiters), g.warm.take());
        for waiter in waiters {
            let _ = waiter.send(routed.clone());
        }
        if let (Some(r), Some(id)) = (&routed, warm)
            && self.convs.get(&id).is_some_and(|c| c.messages.is_empty())
        {
            let _ = self.launch(&id, &r.pick());
        }
        if let Some(d) = self.next_draft.take()
            && (d.request != request || d.only != only)
        {
            self.start_guess(d.request, 0, Some(d.warm), d.only);
        }
    }

    fn start_guess(&mut self, request: String, attached: usize, warm: Option<String>, only: Option<Vendor>) {
        let used = |vendor: Vendor| self.used.iter().filter(|((v, _), _)| *v == vendor).map(|(_, u)| *u).fold(0.0, f64::max);
        let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
        let cands = priced(&candidates(&vendors, &self.codex_models), &used);
        // One candidate needs no router: the everyday default answers.
        let (answer, stop) = if cands.len() > 1 { self.ask_router(question(&request, attached)) } else { (None, None) };
        if let Some(old) = self.guess.replace(Guess { request: request.clone(), result: None, waiters: vec![], warm, only, stop }) {
            stop_router(old.stop);
        }
        let me = self.me.clone();
        tokio::spawn(async move {
            let routed = match answer {
                Some(rx) => tokio::time::timeout(ASK_LIMIT, rx).await.ok().and_then(|r| r.ok()).flatten().and_then(|a| read(&a, &cands)),
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
        let (tx, rx) = oneshot::channel();
        if let Some(ask) = &self.opts.ask {
            let answer = ask(prompt);
            tokio::spawn(async move { drop(tx.send(answer.await)) });
            return (Some(rx), None);
        }
        let router = ROUTER.into_iter().find(|(vendor, model, _)| {
            self.opts.clis.contains_key(vendor) && (*vendor != Vendor::Codex || self.codex_models.iter().any(|m| m == model))
        });
        let Some(((vendor, model, effort), cli)) = router.and_then(|r| Some((r, self.opts.clis.get(&r.0)?.clone()))) else {
            return (None, None);
        };
        let text = Arc::new(Mutex::new(String::new()));
        let tx = Arc::new(Mutex::new(Some(tx)));
        let sink: EventSink = Arc::new(move |e| match e {
            EngineEvent::Text(t) => text.lock().unwrap_or_else(|e| e.into_inner()).push_str(&t),
            EngineEvent::Done { .. } | EngineEvent::Error { .. } => {
                if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(Some(std::mem::take(&mut *text.lock().unwrap_or_else(|e| e.into_inner()))));
                }
            }
            _ => {}
        });
        let (cwd, instructions) = (self.opts.data_dir.clone(), Some(INSTRUCTIONS.to_string()));
        let (model, effort) = (Some(model.to_string()), Some(effort.to_string()));
        let session = match vendor {
            Vendor::Codex => {
                self.codex_server(Area::Chat, false, &cli).open(CodexThread { cwd, model, effort, resume: None, instructions }, sink)
            }
            // ponytail: a cold Claude process takes about 9 s, past the budget unless a draft routes it early; a warm
            // router process comes when it proves worth its ~120 MB.
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch { area: Area::Chat, model, effort, resume: None, user_hooks: false, instructions },
                    spawn: spawner(&self.opts.spawn, &cli),
                },
                sink,
            ),
        };
        session.send(UserTurn::text(prompt));
        (Some(rx), Some(Arc::new(Mutex::new(Some(session)))))
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
        let pick = routed.map(|r| r.pick()).unwrap_or(fallback);
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.vendor = Some(pick.vendor);
            m.model = pick.model.clone();
            m.reason = Some(pick.reason.clone());
        }
        self.save(id);
        if let Err(error) = self.start(id, reply_id, &pick, text, images) {
            if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
                m.status = Some(Status::Error);
                m.error = Some(error);
            }
            self.save(id);
            self.push(id, Some(reply_id), true);
        }
    }

    /// Stop on an answer still waiting for its route: there is no CLI to interrupt yet, so it simply stops.
    pub(super) fn stop_waiting(&mut self, id: &str) {
        let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut()).filter(|m| m.status == Some(Status::Streaming)) else {
            return;
        };
        m.status = Some(Status::Stopped);
        let reply_id = m.id.clone();
        self.save(id);
        self.push(id, Some(&reply_id), true);
    }

    /// Gets the router's own process ready when the user starts typing in Auto.
    pub(super) fn warm_router(&mut self) {
        if let Some(cli) = self.opts.clis.get(&Vendor::Codex).cloned()
            && self.codex_models.iter().any(|m| m == ROUTER[0].1)
            && self.opts.ask.is_none()
        {
            self.codex_server(Area::Chat, false, &cli);
        }
    }
}

fn stop_router(stop: Option<Stop>) {
    if let Some(session) = stop.and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).take()) {
        session.close();
    }
}
