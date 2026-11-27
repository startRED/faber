//! Eyes (task 304): the engine side of the `look` tool, text only. The stored key (`seeOtherWindows`) and the person's
//! out-of-reach list are read when a call comes, again before every native step of the read
//! (`faber_engine_core::eyes::read`) and once more under the engine's lock right before the answer goes out, where saving
//! the settings also runs, so a change the person makes is never passed by. Switching the key off answers every waiting
//! card "no", forgets every window allowed and drops a call that began before it (an epoch taken when the call comes). A
//! window is bound to the process that drew it, the thread that made it and its class when it is admitted: a handle that
//! now names another window is refused. Windows of Faber itself, of a program on the list (or named after one), of a
//! program that cannot be named or runs elevated above Faber, cloaked windows and the shell's own are never listed or
//! read. In a mode that asks, the person answers a card before the AI sees the list (once a session) or reads a window it
//! has not read in this conversation; the card names the program, never the window's title, so nothing the person has not
//! allowed is written to the conversation. Full access does not ask. One read runs at a time, on a blocking thread away
//! from the engine's lock, and the accessibility request, which has no timeout of its own, runs on a thread it can leave
//! at the deadline; nothing there acts in a window (C006). Windows only: elsewhere the tool is never offered.
use super::*;
use crate::eyes::{self as look, Backend, Bound, Request};

const AWAY: &str = "The person is away and has not allowed this yet: go on without it.";
const WAITING: &str = "Another request is waiting for the person's answer: wait for it before asking for one more.";
const DECLINED: &str = "The person said no. Do not ask for it again in this turn.";

/// One call on its way: the call that waits for the answer, the session that made it, what it asks, the window it is
/// bound to, and the key's epoch when it came.
pub(super) struct Call {
    call: String,
    sid: u64,
    request: Request,
    bound: Option<Bound>,
    epoch: u64,
}

/// A card waiting for the person, on its turn.
pub(super) struct PendingLook {
    turn: String,
    asked: Call,
}

/// What a call may do, from the key and the mode.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Admit {
    Refuse(&'static str),
    Ask,
    Run,
}

/// The rule, in one place: the key first, then the mode. `allowed` is what the person already allowed (this window, or
/// the list in this session); `away` is a conversation working while the person is away, where nobody answers a card.
/// The out-of-reach list is applied to the window itself (`look::refusal`), before this.
pub(super) fn admit(key: bool, mode: Permissions, allowed: bool, away: bool) -> Admit {
    if !cfg!(windows) {
        return Admit::Refuse(look::NOT_HERE);
    }
    if !key {
        return Admit::Refuse(look::OFF);
    }
    match mode {
        Permissions::Isolated => Admit::Refuse(look::NOT_HERE),
        Permissions::FullAccess => Admit::Run,
        _ if allowed => Admit::Run,
        _ if away => Admit::Refuse(AWAY),
        _ => Admit::Ask,
    }
}

/// Whether a launch offers the tool: on Windows, while the key is on, outside Isolated (Faber's own internal sessions).
pub(super) fn offers(key: bool, mode: Permissions) -> bool {
    cfg!(windows) && key && mode != Permissions::Isolated
}

/// What a finished read answers, decided under the engine's lock: nothing once the key went off since the call came
/// (`epoch` against `now`), or while the key is off or anything in it is out of reach as stored at this moment.
pub(super) fn reply(read: Result<look::Answer, &'static str>, epoch: u64, now: u64, live: &look::Live) -> (bool, String) {
    if epoch != now {
        return (false, look::OFF.into());
    }
    match read.and_then(|answer| look::deliverable(&answer, live).map(|()| answer)) {
        Ok(answer) => (true, answer.text),
        Err(why) => (false, why.into()),
    }
}

/// The windows on this computer, through `faber_winproc::eyes`.
pub(super) fn desktop() -> Arc<dyn Backend> {
    Arc::new(Desktop)
}

/// One read at a time, for all conversations.
pub(super) fn one_at_a_time() -> Arc<tokio::sync::Semaphore> {
    Arc::new(tokio::sync::Semaphore::new(1))
}

impl Engine {
    /// Reads windows through `backend` instead of the real desktop (tests).
    #[doc(hidden)]
    pub fn set_eyes_backend(&self, backend: Arc<dyn Backend>) {
        self.with(|c| c.eyes = backend);
    }
}

impl Core {
    pub(super) fn offers_eyes(&self, mode: Permissions) -> bool {
        offers(self.store.settings().see_other_windows(), mode)
    }

    fn eyes_live(&self) -> look::Live {
        let settings = self.store.settings();
        look::Live { key: settings.see_other_windows(), list: settings.programs_out_of_reach() }
    }

    pub(super) fn eyes_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(live) = self.live.get(conv) else { return };
        let (sid, mode) = (live.sid, live.permissions);
        // Taken before anything else: a key switched off and on while the window is looked up still stops this call.
        let epoch = self.eyes_epoch;
        if let Admit::Refuse(why) = admit(self.store.settings().see_other_windows(), mode, true, false) {
            return self.answer_toolbox(conv, sid, &call, false, why.into());
        }
        let request = match look::request(&input) {
            Err(why) => return self.answer_toolbox(conv, sid, &call, false, why.into()),
            Ok(request) => request,
        };
        if self.eyes_asks.contains_key(conv) {
            return self.answer_toolbox(conv, sid, &call, false, WAITING.into());
        }
        let Some(window) = request.window() else {
            let allowed = self.eyes_listing.get(conv) == Some(&sid);
            return self.eyes_decide(conv, Call { call, sid, request, bound: None, epoch }, String::new(), allowed);
        };
        let (me, conv, backend) = (self.me.clone(), conv.to_string(), self.eyes.clone());
        let found = self.opts.runtime.spawn_blocking(move || backend.window(window).map(|w| (w, backend.me())));
        self.opts.runtime.spawn(async move {
            let found = found.await.ok().flatten();
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            let Some((window, me)) = found else { return core.answer_toolbox(&conv, sid, &call, false, look::GONE.into()) };
            // The list is read again here: the person may have changed it while the window was looked up.
            if let Some(why) = look::refusal(&window, &core.eyes_live().list, &me) {
                return core.answer_toolbox(&conv, sid, &call, false, why.into());
            }
            let bound = Bound::of(&window);
            let allowed = core.eyes_allowed.get(&conv).is_some_and(|all| all.iter().any(|(h, b)| *h == window.handle && b.holds(&window)));
            let program = bound.as_ref().and_then(|b| b.owner.program()).unwrap_or_default();
            core.eyes_decide(&conv, Call { call, sid, request, bound, epoch }, program, allowed);
        });
    }

    /// Under the lock, with the window's program: the epoch, the key and the mode are read once more.
    fn eyes_decide(&mut self, conv: &str, asked: Call, program: String, allowed: bool) {
        let Some(live) = self.live.get(conv).filter(|l| l.sid == asked.sid) else { return };
        if asked.epoch != self.eyes_epoch {
            return self.answer_toolbox(conv, asked.sid, &asked.call, false, look::OFF.into());
        }
        let settings = self.store.settings();
        let away = self.convs.get(conv).is_some_and(|c| c.keep_going.is_some());
        match admit(settings.see_other_windows(), live.permissions, allowed, away) {
            Admit::Refuse(why) => self.answer_toolbox(conv, asked.sid, &asked.call, false, why.into()),
            Admit::Run => self.eyes_read(conv, asked),
            Admit::Ask if self.eyes_asks.contains_key(conv) => self.answer_toolbox(conv, asked.sid, &asked.call, false, WAITING.into()),
            Admit::Ask => {
                let Some(turn) = live.turn.clone() else {
                    return self.answer_toolbox(conv, asked.sid, &asked.call, false, "The turn has ended.".into());
                };
                // The program only: a window's title can say what the person has not allowed yet, and the card is saved.
                let (tool, input) = match asked.request {
                    Request::Windows => (look::CARD_LIST, serde_json::json!({})),
                    Request::Tree { .. } => (look::CARD, serde_json::json!({ "program": program })),
                };
                let card = Permission { request_id: asked.call.clone(), tool: tool.into(), input, epoch: settings.grants_epoch.clone() };
                if let Some(message) = self.turn(conv) {
                    message.permissions.get_or_insert_default().push(card);
                }
                self.eyes_asks.insert(conv.into(), PendingLook { turn: turn.clone(), asked });
                self.save(conv);
                self.weigh(conv);
                self.push(conv, Some(&turn), true);
            }
        }
    }

    /// Takes a waiting card off its turn and answers its call `why`.
    fn eyes_drop_card(&mut self, conv: &str, waiting: &PendingLook, why: &str) {
        let asked = &waiting.asked;
        if let Some(message) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == waiting.turn))
            && let Some(cards) = message.permissions.as_mut()
        {
            cards.retain(|p| !(p.request_id == asked.call && look::is_card(&p.tool)));
        }
        self.answer_toolbox(conv, asked.sid, &asked.call, false, why.into());
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(&waiting.turn), true);
    }

    /// The key went off: every waiting card is answered "no", every window allowed and every list seen is forgotten, and a
    /// call or read that came before is never answered (the epoch it came under is gone).
    pub(super) fn eyes_off(&mut self) {
        self.eyes_epoch += 1;
        self.eyes_allowed.clear();
        self.eyes_listing.clear();
        for (conv, waiting) in std::mem::take(&mut self.eyes_asks) {
            self.eyes_drop_card(&conv, &waiting, look::OFF);
        }
    }

    /// The person answered a window or list card. None when the request is not one. With the key off it does nothing but
    /// spend the card. An Allow lets this conversation read that window, as it is bound, until Faber restarts (or see the
    /// list for the rest of this session); "always" means no more than that.
    pub(super) fn answer_eyes(&mut self, conv: &str, turn: &str, call: &str, allow: bool) -> Option<Result<(), String>> {
        let message = self.convs.get_mut(conv)?.messages.iter_mut().find(|m| m.id == turn)?;
        let cards = message.permissions.as_mut()?;
        let at = cards.iter().position(|p| p.request_id == call && look::is_card(&p.tool))?;
        cards.remove(at);
        let waiting = self.eyes_asks.remove(conv).filter(|w| w.asked.call == call && w.turn == turn);
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(turn), true);
        let Some(asked) =
            waiting.map(|w| w.asked).filter(|a| self.live.get(conv).is_some_and(|l| l.sid == a.sid && l.turn.as_deref() == Some(turn)))
        else {
            return Some(Err(super::answer::STALE.into()));
        };
        if asked.epoch != self.eyes_epoch || !self.store.settings().see_other_windows() {
            self.answer_toolbox(conv, asked.sid, call, false, look::OFF.into());
            return Some(Err(super::answer::STALE.into()));
        }
        if !allow {
            self.answer_toolbox(conv, asked.sid, call, false, DECLINED.into());
            return Some(Ok(()));
        }
        match (asked.request.window(), &asked.bound) {
            (None, _) => {
                self.eyes_listing.insert(conv.to_string(), asked.sid);
            }
            (Some(window), Some(bound)) => self.eyes_allowed.entry(conv.to_string()).or_default().push((window, bound.clone())),
            (Some(_), None) => {}
        }
        self.eyes_read(conv, asked);
        Some(Ok(()))
    }

    /// Reads on a blocking thread, one read at a time, and answers. The key and the list are read from the store at every
    /// step there, and once more here under the lock right before the answer goes out ([`reply`]).
    fn eyes_read(&mut self, conv: &str, asked: Call) {
        let Call { call, sid, request, bound, epoch } = asked;
        let Ok(permit) = self.eyes_busy.clone().try_acquire_owned() else {
            return self.answer_toolbox(conv, sid, &call, false, look::BUSY.into());
        };
        let (store, backend) = (self.store.clone(), self.eyes.clone());
        let (me, conv) = (self.me.clone(), conv.to_string());
        let read = self.opts.runtime.spawn_blocking(move || {
            let _permit = permit;
            let live = || {
                let settings = store.settings();
                look::Live { key: settings.see_other_windows(), list: settings.programs_out_of_reach() }
            };
            look::read(backend.as_ref(), &request, bound.as_ref(), &live)
        });
        self.opts.runtime.spawn(async move {
            let read = read.await.unwrap_or(Err(look::UNREAD));
            let Some(core) = me.upgrade() else { return };
            let core = core.lock().unwrap_or_else(|e| e.into_inner());
            let (ok, text) = reply(read, epoch, core.eyes_epoch, &core.eyes_live());
            core.answer_toolbox(&conv, sid, &call, ok, text);
        });
    }
}

/// The real desktop. Off Windows every call finds nothing, and the tool is never offered there.
struct Desktop;

#[cfg(windows)]
mod native {
    use super::look::{self, Listed, Me, Node, Owner, Walked};
    use faber_winproc::eyes as win;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    /// Set while an accessibility request still runs, one at most (`look::within`).
    static ASKING: AtomicBool = AtomicBool::new(false);

    pub(super) fn owner(pid: u32) -> Owner {
        Owner { pid, started: win::started(pid).unwrap_or(0), path: win::path(pid), elevated: win::elevated(pid) }
    }

    pub(super) fn listed(w: win::Window) -> Listed {
        let owner = win::drawn_by(w.handle).map(owner).filter(|o| o.started != 0);
        Listed {
            handle: w.handle,
            title: w.title,
            class: w.class,
            thread: w.thread,
            width: w.width,
            height: w.height,
            minimized: w.minimized,
            cloaked: w.cloaked,
            owner,
        }
    }

    pub(super) fn me() -> Me {
        let own = std::process::id();
        // The shell that shows Faber's windows (Electron) says its process; the engine in Tauri is that process itself.
        let shell = std::env::var("FABER_SHELL_PID").ok().and_then(|p| p.trim().parse::<u32>().ok());
        let pairs: Vec<(u32, u32)> = faber_winproc::processes().unwrap_or_default().iter().map(|p| (p.pid, p.parent)).collect();
        let roots: Vec<u32> = [Some(own), shell].into_iter().flatten().collect();
        let folders = [
            std::env::var("FABER_INSTALL_DIR").ok(),
            std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.display().to_string())),
        ]
        .into_iter()
        .flatten()
        .filter(|f| !f.trim().is_empty())
        .collect();
        Me { pids: look::family(&pairs, &roots), folders, elevated: win::elevated(own).unwrap_or(false) }
    }

    /// The request runs on a thread of its own: a view that stops answering mid-request holds that thread, not the read.
    pub(super) fn request_accessibility(handle: u64, deadline: Instant) {
        let _ = look::within(&ASKING, deadline, move || win::request_accessibility(handle, deadline));
    }

    pub(super) fn tree(handle: u64, depth: usize, deadline: Instant, refused: &dyn Fn(&Owner) -> bool) -> Option<(Vec<Node>, Walked)> {
        let automation = win::Automation::open(handle).ok()?;
        let mut owners = std::collections::HashMap::<u32, bool>::new();
        let mut kept = |pid: u32| *owners.entry(pid).or_insert_with(|| pid == 0 || refused(&owner(pid)));
        let root = automation.root();
        let root_kept = kept(automation.process(&root));
        let (mut nodes, late) = (vec![], std::cell::Cell::new(false));
        let in_time = || {
            let on_time = Instant::now() < deadline;
            late.set(late.get() || !on_time);
            on_time
        };
        let kept = std::cell::RefCell::new(kept);
        let mut walked = look::walk(
            (root, root_kept),
            look::Caps { depth, nodes: look::NODE_LIMIT },
            // An element of a program out of reach is never entered.
            |(e, out)| match *out || !in_time() {
                true => Ok(None),
                false => automation.first_child(e).map(|c| {
                    c.map(|c| {
                        let p = automation.process(&c);
                        (c, kept.borrow_mut()(p))
                    })
                }),
            },
            |(e, _)| match in_time() {
                false => Ok(None),
                true => automation.next_sibling(e).map(|c| {
                    c.map(|c| {
                        let p = automation.process(&c);
                        (c, kept.borrow_mut()(p))
                    })
                }),
            },
            |(e, out), depth| {
                if *out {
                    nodes.push(Node { depth, kind: "(out of reach)".into(), enabled: true, ..Default::default() });
                    return;
                }
                let d = automation.describe(e, deadline);
                nodes.push(Node { depth, kind: d.kind, name: d.name, id: d.id, enabled: d.enabled, value: d.value, rect: d.rect });
            },
        )
        .ok()?;
        walked.capped |= late.get();
        Some((nodes, walked))
    }
}

#[cfg(windows)]
impl Backend for Desktop {
    fn me(&self) -> look::Me {
        native::me()
    }
    fn windows(&self) -> Option<Vec<look::Listed>> {
        Some(faber_winproc::eyes::windows().ok()?.into_iter().map(native::listed).collect())
    }
    fn window(&self, handle: u64) -> Option<look::Listed> {
        faber_winproc::eyes::window(handle).map(native::listed)
    }
    fn hung(&self, handle: u64) -> bool {
        faber_winproc::eyes::hung(handle)
    }
    fn request_accessibility(&self, handle: u64, deadline: std::time::Instant) {
        native::request_accessibility(handle, deadline);
    }
    fn tree(
        &self,
        handle: u64,
        depth: usize,
        deadline: std::time::Instant,
        refused: &dyn Fn(&look::Owner) -> bool,
    ) -> Option<(Vec<look::Node>, look::Walked)> {
        native::tree(handle, depth, deadline, refused)
    }
}

#[cfg(not(windows))]
impl Backend for Desktop {
    fn me(&self) -> look::Me {
        look::Me::default()
    }
    fn windows(&self) -> Option<Vec<look::Listed>> {
        None
    }
    fn window(&self, _: u64) -> Option<look::Listed> {
        None
    }
    fn hung(&self, _: u64) -> bool {
        true
    }
    fn request_accessibility(&self, _: u64, _: std::time::Instant) {}
    fn tree(&self, _: u64, _: usize, _: std::time::Instant, _: &dyn Fn(&look::Owner) -> bool) -> Option<(Vec<look::Node>, look::Walked)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_and_the_mode_decide_in_that_order() {
        let on = cfg!(windows);
        // The key off refuses in every mode, Full access included, and the tool is not offered.
        for mode in [Permissions::Ask, Permissions::Auto, Permissions::FullAccess, Permissions::Plan] {
            assert_eq!(admit(false, mode, true, false), Admit::Refuse(if on { look::OFF } else { look::NOT_HERE }));
            assert!(!offers(false, mode));
        }
        if !on {
            assert_eq!(admit(true, Permissions::FullAccess, false, false), Admit::Refuse(look::NOT_HERE), "Windows only");
            assert!(!offers(true, Permissions::FullAccess));
            return;
        }
        assert!(offers(true, Permissions::Ask) && offers(true, Permissions::FullAccess));
        assert!(!offers(true, Permissions::Isolated));
        assert_eq!(admit(true, Permissions::FullAccess, false, true), Admit::Run, "Full access does not ask");
        assert_eq!(admit(true, Permissions::Ask, false, false), Admit::Ask, "a new window, or the list, asks");
        assert_eq!(admit(true, Permissions::Auto, false, false), Admit::Ask);
        assert_eq!(admit(true, Permissions::AcceptEdits, false, false), Admit::Ask);
        assert_eq!(admit(true, Permissions::Plan, false, false), Admit::Ask);
        assert_eq!(admit(true, Permissions::Ask, true, false), Admit::Run, "what the person allowed is not asked again");
        assert_eq!(admit(true, Permissions::Ask, false, true), Admit::Refuse(AWAY), "nobody answers while the person is away");
        assert_eq!(admit(true, Permissions::Isolated, true, false), Admit::Refuse(look::NOT_HERE));
    }

    #[test]
    fn a_finished_read_is_answered_only_under_the_key_epoch_and_list_of_the_moment_it_goes_out() {
        let read = || Ok(look::Answer { text: "edit".into(), programs: vec!["notepad.exe".into(), "vault.exe".into()] });
        let live = |list: &[&str]| look::Live { key: true, list: list.iter().map(|p| p.to_string()).collect() };
        assert_eq!(reply(read(), 3, 3, &live(&[])), (true, "edit".into()));
        // The person kept a program out of reach after the read's own last check, before the answer went out.
        assert_eq!(reply(read(), 3, 3, &live(&["vault"])), (false, look::KEPT.into()));
        // The key went off and on again since the call came.
        assert_eq!(reply(read(), 3, 4, &live(&[])), (false, look::OFF.into()));
        assert_eq!(reply(read(), 3, 3, &look::Live { key: false, list: vec![] }), (false, look::OFF.into()));
        assert_eq!(reply(Err(look::GONE), 3, 3, &live(&[])), (false, look::GONE.into()));
    }
}
