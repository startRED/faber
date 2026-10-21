//! Faber's own browser pane (R161 step 7): one child webview of the one window, on the same WebView2 environment, so it
//! shares the browser and GPU processes (the spike's rules, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md).
//! The agent opens a page in it through the engine's browser tool; the person sees it when they open it from the reply,
//! placed over the running column. Away from the screen it is moved out of the window, never hidden, so its pictures
//! stay fresh, and it closes once nobody has used it for a while.
//!
//! Only http and https pages load in it, never Faber's own `*.localhost` origins; it opens no new windows, downloads
//! nothing and keeps no cookies or logins once it closes (a private profile), so one project's agent never reads a
//! session another conversation or the person left there. A page in it cannot call Faber: the command handler answers
//! only the main screen (`main.rs`), and the screen's capability names only that webview. One page opens at a time, and
//! never while the person is looking at the pane: it is theirs then, and the agent's call waits for them to go back
//! (`WAIT_PERSON`), then hears that they used it.
//!
//! The agent acts by the words a person sees (`act.js`, run inside the page); on a page that asks for a login, a captcha
//! or a payment it may read and scroll but never click or type.

pub(crate) mod native;

use base64::Engine as _;
use faber_engine::browser::{Browser, Look, MAX_CUT, MAX_TEXT, MAX_TITLE, MAX_URL, Page, Request, Turn};
use faber_engine::screens::Viewport;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewBuilder, WebviewUrl};

const LABEL: &str = "site";
/// Where the pane waits while nobody looks: outside the window, where it keeps drawing (the spike's rule 2).
const PARKED: LogicalPosition<f64> = LogicalPosition { x: -20_000.0, y: -20_000.0 };
/// The size the agent's pages load at while parked, a common laptop window.
const SIZE: LogicalSize<f64> = LogicalSize { width: 1280.0, height: 800.0 };
/// How long a page has to finish loading before its text is read anyway.
const LOAD: Duration = Duration::from_secs(30);
/// The longest one DevTools call may take; a page that holds it longer is closed with the pane.
const CALL: Duration = Duration::from_secs(10);
/// Unused this long and not on screen, the pane closes and gives its memory back (the spike's rule 4).
const IDLE: Duration = Duration::from_secs(5 * 60);
/// How long the agent's call waits for the person to hand the browser back.
const WAIT_PERSON: Duration = Duration::from_secs(30);
/// After a click, typing or going back, how long a new page has to start loading before the same page is read again.
const NAVIGATES: Duration = Duration::from_millis(1_500);
/// The agent's actions, run inside the page: the function `act.js` exports, which the screen's checks import as it is.
const ACT: &str = include_str!("act.js");

fn act_function() -> &'static str {
    ACT.split_once("export default ").map_or(ACT, |(_, f)| f.trim_end().trim_end_matches(';'))
}
/// The picture in the reply is this share of the page's size.
const PICTURE_SCALE: f64 = 0.375;
/// The longest side of a picture at a screen size of the agent's, in the picture's pixels, whatever the screen's density.
const LOOK_SIDE: f64 = 640.0;
/// How long a page has to lay itself out again at a new screen size before it is looked at.
const RESIZED: Duration = Duration::from_millis(400);

/// The share of a page at `size` its picture takes: the capture counts each of the screen's device pixels, so a dense phone
/// screen is scaled down further than a PC's, and every picture's longest side is `LOOK_SIDE`.
fn look_scale(size: &Viewport) -> f64 {
    (LOOK_SIDE / (f64::from(size.width.max(size.height)) * size.scale)).min(1.0)
}

/// The DevTools parameters that draw the page at `size`, as that screen would: its width, height and density, and a phone's
/// or tablet's layout rules and touch.
fn metrics(size: &Viewport) -> (Value, Value) {
    let mobile = size.mobile();
    (
        json!({ "width": size.width, "height": size.height, "deviceScaleFactor": size.scale, "mobile": mobile }),
        json!({ "enabled": mobile, "maxTouchPoints": if mobile { 5 } else { 1 } }),
    )
}

/// Whether the pane may load `url`: a web page, never one of Faber's own origins (nor, in a development build, the screen's
/// own dev server).
fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let dev_screen = cfg!(debug_assertions) && host == "localhost" && url.port() == Some(5173);
    matches!(url.scheme(), "http" | "https") && !host.is_empty() && !host.ends_with(".localhost") && !dev_screen
}

pub struct Pane {
    app: AppHandle,
    /// Never call a webview getter or await under this lock: `on_page_load` takes it on the window's thread.
    state: Mutex<State>,
    /// The pages that started and the last of them that finished, so an open waits for its own page and never for a late
    /// end of the one before.
    loads: tokio::sync::watch::Sender<(u64, u64)>,
    /// Held from a navigation to the last read of its page, so two calls never read each other's page.
    opening: tokio::sync::Mutex<()>,
    /// The person is looking at the pane: it is theirs until they go back.
    looking: tokio::sync::watch::Sender<bool>,
    /// Held while an action of the agent changes the page: the person takes the pane only when it is over.
    acting: tokio::sync::Mutex<()>,
    /// The random name `act.js` gives the ring it draws where the agent acted, so it removes its own ring and never a node of
    /// the page (each action runs in a world of its own and cannot keep the node itself).
    mark: String,
}

#[derive(Default)]
struct State {
    webview: Option<Webview>,
    shown: bool,
    used: Option<Instant>,
    /// An idle close is already waiting; one at a time, however often the pane is used.
    idle_watch: bool,
    /// The person looked at the pane since the agent's last call.
    handed_back: bool,
    /// The conversation whose page the pane holds: another conversation's agent never acts on it or reads it.
    owner: Option<String>,
    /// Which of the screen's boxes has the pane.
    boxes: Boxes,
    /// Counts the times the person took the pane, so a call that ran meanwhile knows, even if they already went back.
    taken: u64,
    /// The whole address the last answer to the owner reported (never the cut one the agent gets), which an action only runs on
    /// (the page may have moved since). None until the owner has read or opened a page: nothing is clicked or typed before.
    reported: Option<String>,
    /// The sites the person went to while the pane was theirs, since the agent's last call (`went_to`).
    visited: Vec<String>,
    /// The sites the pane's pages were on since it was made, the latest `MAX_ORIGINS`: what `fresh_for` clears.
    origins: Vec<String>,
    /// Pages Faber loaded (the agent's, or a card's) that have not finished: that many next finishes are not sites the
    /// person went to. A load that never finishes leaves one more skipped: the list can fall short, never say more.
    skip_finish: u32,
    /// The screen size the agent chose for its pages (`Request::Size`), none for the pane's own; kept until it closes.
    size: Option<Viewport>,
    /// The page is drawn at a size of the agent's, not the pane's: the person taking the pane gets the pane's own back.
    emulated: bool,
}

/// The screen's boxes the pane is placed over, named by numbers that only grow while the screen lives. A box that let the
/// pane go places it no more, and a box older than the one that has it cannot move or take it away. Each load of the screen
/// has an id of its own, chosen at random: a request of any other load (a screen that went away, its late calls) is ignored.
#[derive(Default)]
struct Boxes {
    load: Option<u32>,
    /// Loads that were the current one before: a late start of one of them never takes the pane back.
    retired: Vec<u32>,
    parked: u64,
    placed: u64,
}

impl Boxes {
    /// Whether box `id` of screen load `load` may place the pane.
    fn place(&mut self, load: u32, id: u64) -> bool {
        let fresh = self.load == Some(load) && id > self.parked && id >= self.placed;
        if fresh {
            self.placed = id;
        }
        fresh
    }

    /// Whether box `id` letting go takes the pane off the screen. Box 0 is a screen just loaded: it makes `load` the one that
    /// counts and forgets every box.
    fn park(&mut self, load: u32, id: u64) -> bool {
        if id == 0 {
            if self.retired.contains(&load) {
                return false;
            }
            let mut retired = std::mem::take(&mut self.retired);
            if let Some(old) = self.load.filter(|old| *old != load) {
                retired.push(old);
                if retired.len() > 16 {
                    retired.remove(0);
                }
            }
            *self = Boxes { load: Some(load), retired, ..Boxes::default() };
            return true;
        }
        if self.load != Some(load) {
            return false;
        }
        self.parked = self.parked.max(id);
        id >= self.placed
    }
}

/// Closes the pane when dropped while armed: an action of the agent cut off before its end.
struct Unfinished<'a> {
    pane: &'a Pane,
    armed: bool,
}

impl Drop for Unfinished<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.pane.close();
        }
    }
}

/// Runs `then` once no action of the agent is changing the page, and keeps further ones out while it runs.
async fn when_idle<T>(acting: &tokio::sync::Mutex<()>, then: impl FnOnce() -> T) -> T {
    let _idle = acting.lock().await;
    then()
}

/// The address as the pane loads it, or why it never does.
fn checked(url: &str) -> Result<Url, String> {
    let parsed: Url = url.parse().map_err(|_| "the address is not valid".to_string())?;
    if !allowed(&parsed) {
        return Err("only web pages open in Faber's browser".into());
    }
    Ok(parsed)
}

/// The full address the owner last read, or the sentence that sends the agent to read first.
fn bound(reported: Option<&str>) -> Result<&str, String> {
    reported.ok_or_else(|| "read the page first; click and type only run on a page you have read".to_string())
}

impl Pane {
    pub fn new(app: AppHandle) -> Arc<Pane> {
        Arc::new(Pane {
            app,
            state: Mutex::default(),
            loads: tokio::sync::watch::channel((0, 0)).0,
            opening: tokio::sync::Mutex::new(()),
            looking: tokio::sync::watch::channel(false).0,
            acting: tokio::sync::Mutex::new(()),
            mark: format!("{:016x}", std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), 0u8)),
        })
    }

    fn webview(&self) -> Option<Webview> {
        crate::lock(&self.state).webview.clone()
    }

    /// Loads `url` in the pane, making it first if it is closed. Answers the count of started pages to wait past.
    fn load(self: &Arc<Self>, url: &str) -> Result<u64, String> {
        let parsed = checked(url)?;
        let before = self.loads.borrow().0;
        // A page Faber loads is the agent's or a card's, never one the person went to.
        crate::lock(&self.state).skip_finish += 1;
        self.touch();
        if let Some(webview) = self.webview() {
            if webview.navigate(parsed.clone()).is_ok() {
                return Ok(before);
            }
            // A pane left from a window that closed and came back: it is made again.
            self.close();
        }
        let window = self.app.get_window("main").ok_or("Faber's window is closed")?;
        let (loads, app, pane) = (self.loads.clone(), self.app.clone(), Arc::downgrade(self));
        let seen = pane.clone();
        let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(parsed))
            .incognito(true)
            .on_navigation(allowed)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_page_load(move |_, payload| match payload.event() {
                PageLoadEvent::Started => {
                    loads.send_modify(|(started, _)| *started += 1);
                    if let Some(pane) = seen.upgrade() {
                        stored_by(&mut crate::lock(&pane.state).origins, payload.url());
                    }
                }
                PageLoadEvent::Finished => {
                    loads.send_modify(|(started, finished)| *finished = *started);
                    // A page that finished while the person had the pane is one they went to: the agent hears its site next time.
                    if let Some(pane) = pane.upgrade() {
                        let mut s = crate::lock(&pane.state);
                        stored_by(&mut s.origins, payload.url());
                        if s.skip_finish > 0 {
                            s.skip_finish -= 1;
                        } else if s.shown {
                            went_to(&mut s.visited, payload.url());
                        }
                    }
                    // The screen names the page it shows by where it is now, never by the card it was opened from.
                    let _ = app.emit_to("main", "faber:site", payload.url().as_str());
                }
            });
        let webview = window.add_child(builder, PARKED, SIZE).map_err(|e| e.to_string())?;
        crate::lock(&self.state).webview = Some(webview);
        Ok(before)
    }

    fn touch(self: &Arc<Self>) {
        let mut s = crate::lock(&self.state);
        s.used = Some(Instant::now());
        if std::mem::replace(&mut s.idle_watch, true) {
            return;
        }
        let pane = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                // Decided and taken under one lock, so a page that opens meanwhile is never closed under it.
                let wait = {
                    let mut s = crate::lock(&pane.state);
                    let left = s.used.map_or(Duration::ZERO, |u| IDLE.saturating_sub(u.elapsed()));
                    if left.is_zero() && !s.shown {
                        s.idle_watch = false;
                        s.visited.clear();
                        s.origins.clear();
                        Err(s.webview.take())
                    } else {
                        Ok(left.max(Duration::from_secs(1)))
                    }
                };
                match wait {
                    Ok(wait) => tokio::time::sleep(wait).await,
                    Err(webview) => {
                        if let Some(webview) = webview {
                            let _ = webview.close();
                        }
                        break;
                    }
                }
            }
        });
    }

    pub fn close(&self) {
        let webview = {
            let mut s = crate::lock(&self.state);
            s.shown = false;
            s.reported = None;
            s.visited.clear();
            s.origins.clear();
            (s.size, s.emulated) = (None, false);
            s.webview.take()
        };
        self.looking.send_replace(false);
        if let Some(webview) = webview {
            let _ = webview.close();
        }
    }

    /// Shows the pane over the screen's own rectangle for it, in the page's logical pixels. `box_id` names the screen's box,
    /// newer ones larger; a box that already let the pane go places it no more.
    /// The person taking the pane waits for an action of the agent in flight (bounded by `CALL`), so the page never changes
    /// under their hands and the call finds, when it goes on, that they took it.
    pub async fn place(self: &Arc<Self>, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> bool {
        if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) || width < 1.0 || height < 1.0 {
            return false;
        }
        let placed = when_idle(&self.acting, || {
            // The check and the move are one step under the lock, so a park that comes between cannot be undone by this box.
            let mut s = crate::lock(&self.state);
            let Some(webview) = s.webview.clone() else { return false };
            if !s.boxes.place(load, box_id) {
                return false;
            }
            s.shown = true;
            s.handed_back = true;
            s.taken += 1;
            self.looking.send_replace(true);
            webview.set_size(LogicalSize { width, height }).is_ok() && webview.set_position(LogicalPosition { x, y }).is_ok()
        })
        .await;
        // The person sees the page at the pane's own size, never at a screen size the agent chose; its next call draws it so
        // again. Under `acting`, so a check at several sizes never changes the size under their hands.
        if placed && crate::lock(&self.state).emulated {
            let _acting = self.acting.lock().await;
            if self.call("Emulation.clearDeviceMetricsOverride", json!({})).await.is_ok() {
                let _ = self.call("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })).await;
                crate::lock(&self.state).emulated = false;
            }
        }
        self.touch();
        placed
    }

    /// Takes the pane off the screen, back at the size the agent's pages load at, for the screen's box `box_id` and every
    /// older one of screen load `load`, unless a newer box has it. Box 0 says the screen was just loaded and no box is left.
    pub fn park(self: &Arc<Self>, load: u32, box_id: u64) {
        {
            let mut s = crate::lock(&self.state);
            if !s.boxes.park(load, box_id) {
                return;
            }
            s.shown = false;
            self.looking.send_replace(false);
            let Some(webview) = &s.webview else { return };
            let _ = webview.set_position(PARKED);
            let _ = webview.set_size(SIZE);
        }
        self.touch();
    }

    /// Loads a page the person asked for from a conversation's reply. The page is that conversation's from then on.
    pub async fn show_page(self: &Arc<Self>, conversation: &str, url: &str) -> bool {
        // The page the agent left is usually the one the card names: it shows at once, even while a call of the agent's runs,
        // which then finds the person looking and stops before it acts.
        let owner = crate::lock(&self.state).owner.clone();
        let same = self.webview().and_then(|w| w.url().ok()).is_some_and(|now| now.as_str() == url);
        if same && owner.as_deref() == Some(conversation) {
            return true;
        }
        // An address the pane never loads leaves another conversation's page as it was.
        if checked(url).is_err() {
            return false;
        }
        let _turn = self.opening.lock().await;
        self.fresh_for(conversation).await;
        let shown = self.load(url).is_ok();
        if shown {
            let mut s = crate::lock(&self.state);
            if s.owner.as_deref() != Some(conversation) {
                s.visited.clear();
            }
            s.owner = Some(conversation.to_string());
            s.reported = None;
        }
        shown
    }

    /// A page that passes to another conversation gets a pane of its own: one conversation's history (and anything the
    /// person signed in to there) is never a Back away from another's agent. The private pane forgets its data once its last
    /// view is gone, which may come only after the next one is made, so the cookies and what every site it was on stored go
    /// first.
    async fn fresh_for(&self, conversation: &str) {
        let origins = {
            let s = crate::lock(&self.state);
            if !s.owner.as_deref().is_some_and(|owner| owner != conversation) {
                return;
            }
            s.origins.clone()
        };
        for origin in origins {
            let _ = self.call("Storage.clearDataForOrigin", json!({ "origin": origin, "storageTypes": "all" })).await;
        }
        let _ = self.call("Network.clearBrowserCookies", json!({})).await;
        self.close();
    }

    /// The person goes back, forward or loads the page again from the pane's head, only on the page of the conversation
    /// they look at. The history holds only pages the pane already let in, one conversation's own since `fresh_for`; a step
    /// that changes the document passes `allowed` again (one inside the same document cannot change the site), and the
    /// steps are the person's own going somewhere, so the agent hears of them like any other page they went to.
    pub async fn step(self: &Arc<Self>, conversation: &str, step: &str) -> bool {
        let _turn = self.opening.lock().await;
        {
            // Checked once the turn is ours: another conversation may have opened its page while this step waited, and a
            // pane out of sight (a step's picture stands in for it) is not the person's to move.
            let s = crate::lock(&self.state);
            if s.owner.as_deref() != Some(conversation) || !s.shown {
                return false;
            }
        }
        self.touch();
        match step {
            "reload" => self.call("Page.reload", json!({})).await.is_ok(),
            "back" | "forward" => {
                let Ok(history) = self.call("Page.getNavigationHistory", json!({})).await else {
                    return false;
                };
                let at = history["currentIndex"].as_i64().unwrap_or(-1) + if step == "back" { -1 } else { 1 };
                let entry = usize::try_from(at).ok().and_then(|at| history["entries"].get(at));
                let Some(id) = entry.and_then(|e| e["id"].as_i64()) else {
                    return false;
                };
                self.call("Page.navigateToHistoryEntry", json!({ "entryId": id })).await.is_ok()
            }
            _ => false,
        }
    }

    /// One DevTools call, with a deadline: a page that holds it longer closes with the pane, which opens fresh next time.
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let webview = self.webview().ok_or("the browser closed")?;
        let answer = tokio::time::timeout(CALL, native::devtools(&webview, method, params.to_string())).await;
        let Ok(answer) = answer else {
            self.close();
            return Err("the page stopped answering".into());
        };
        let json = answer.map_err(|_| "the browser closed".to_string())??;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    /// A world of Faber's own inside the page: it shares the page's document but none of its scripts, so a page cannot change
    /// what Faber's code calls or what it answers.
    async fn world(&self) -> Result<i64, String> {
        let tree = self.call("Page.getFrameTree", json!({})).await?;
        let frame = tree["frameTree"]["frame"]["id"].as_str().ok_or("the page has no frame")?.to_string();
        let world = self.call("Page.createIsolatedWorld", json!({ "frameId": frame, "worldName": "faber" })).await?;
        world["executionContextId"].as_i64().ok_or_else(|| "the page has no world for Faber".to_string())
    }

    /// Evaluates `expression` in the world `context`.
    ///
    /// `taken` is how many times the person had taken the pane when the agent's call began: if that moved, the call stops just
    /// before the page runs anything. An evaluation that `changes` the page holds `acting` from that look to its end, so the
    /// person's taking waits for it.
    async fn evaluate_in(&self, context: i64, expression: &str, taken: u64, changes: bool) -> Result<Value, String> {
        let _acting = if changes { Some(self.acting.lock().await) } else { None };
        self.person_took_it(taken)?;
        // A call the engine gave up on drops this mid-evaluation: the pane closes before `acting` lets the person in, so an
        // action still running in the page never meets the person's hands.
        let mut unfinished = Unfinished { pane: self, armed: changes };
        let done = self.call("Runtime.evaluate", json!({ "expression": expression, "returnByValue": true, "contextId": context })).await;
        unfinished.armed = false;
        Ok(done?["result"]["value"].clone())
    }

    async fn evaluate(&self, expression: &str, taken: u64, changes: bool) -> Result<Value, String> {
        let context = self.world().await?;
        self.evaluate_in(context, expression, taken, changes).await
    }

    /// Takes the browser for the agent: waits until the person is not looking at it, at most `WAIT_PERSON`, without holding
    /// it meanwhile, so the person's own clicks on other cards never wait for the agent.
    async fn take(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let deadline = tokio::time::Instant::now() + WAIT_PERSON;
        let mut looking = self.looking.subscribe();
        loop {
            if tokio::time::timeout_at(deadline, looking.wait_for(|on| !*on)).await.is_err() {
                return Err("the person is using Faber's browser; ask them, or try again once they go back".into());
            }
            let turn = self.opening.lock().await;
            if !*self.looking.borrow() {
                return Ok(turn);
            }
        }
    }

    /// The person took the browser while the agent's call ran, and may have gone back already: the call stops there, with no
    /// picture of what they did.
    fn person_took_it(&self, taken: u64) -> Result<(), String> {
        if *self.looking.borrow() || crate::lock(&self.state).taken != taken {
            return Err("the person took over Faber's browser; ask them, or try again once they go back".into());
        }
        Ok(())
    }

    /// Runs `act.js` on the page and answers what it said: whether it did it, and what is the person's turn there. Its
    /// reasons are codes; the words are Faber's, and the page's own words in them are quoted.
    async fn run_act(&self, mut request: Value, taken: u64) -> Result<(bool, Option<Turn>), String> {
        request["mark"] = json!(self.mark);
        let changes = matches!(request["kind"].as_str(), Some("click" | "type" | "back"));
        let value = self.evaluate(&format!("({})({request})", act_function()), taken, changes).await?;
        if !value.is_object() {
            return Err("the page did not let Faber act on it".into());
        }
        let turn = value["turn"].as_str().and_then(Turn::from_words);
        if value["ok"].as_bool() == Some(true) || turn.is_some() {
            return Ok((value["ok"].as_bool() == Some(true), turn));
        }
        let quoted = |list: &Value| -> String {
            let words = list.as_array().into_iter().flatten().filter_map(Value::as_str).take(5);
            words
                .map(|w| format!("\"{}\"", w.chars().filter(|c| !c.is_control()).take(60).collect::<String>()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        Err(match value["why"].as_str() {
            Some("click") => "nothing on view to click reads so".into(),
            Some("field") => "no field on view reads so".into(),
            Some("nofield") => "the page has no field on view to type in".into(),
            Some("submit") => "say which field to type in when you also press Enter".into(),
            Some("moved") => format!("the page changed to {} since you read it; read it again first", quoted(&value["seen"])),
            Some("many") => format!("several different things read so ({}); say the whole words of one", quoted(&value["seen"])),
            _ => "the page did not let Faber act on it".into(),
        })
    }

    /// Draws the page at `size` (none: the pane's own), unless the person took the pane since the call began: under `acting`,
    /// so their taking it waits for the change and then gets the pane's own size back (`place`).
    async fn emulate(&self, size: Option<Viewport>, taken: u64) -> Result<(), String> {
        let _acting = self.acting.lock().await;
        self.person_took_it(taken)?;
        let Some(size) = size else {
            if crate::lock(&self.state).emulated {
                self.call("Emulation.clearDeviceMetricsOverride", json!({})).await?;
                self.call("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })).await?;
                crate::lock(&self.state).emulated = false;
            }
            return Ok(());
        };
        let (screen, touch) = metrics(&size);
        crate::lock(&self.state).emulated = true;
        self.call("Emulation.setDeviceMetricsOverride", screen).await?;
        self.call("Emulation.setTouchEmulationEnabled", touch).await?;
        Ok(())
    }

    /// The page at each of `sizes`, a picture and whether it fits each, then back at the size it had. A page that is the
    /// person's turn stops the check there, with no picture.
    async fn looks(&self, sizes: &[Viewport], taken: u64) -> Result<Vec<Look>, String> {
        let looked = self.each_size(sizes, taken).await;
        // Back at the size it had, whether or not every size was looked at; one the person took gets the pane's own (`place`).
        let size = crate::lock(&self.state).size;
        let back = self.emulate(size, taken).await;
        let looks = looked?;
        back?;
        tokio::time::sleep(RESIZED).await;
        Ok(looks)
    }

    async fn each_size(&self, sizes: &[Viewport], taken: u64) -> Result<Vec<Look>, String> {
        let mut looks = Vec::new();
        for size in sizes {
            self.emulate(Some(*size), taken).await?;
            tokio::time::sleep(RESIZED).await;
            let expression = format!(
                "(() => {{ const act = {}; const turn = act({{ kind: 'read', watch: true }}).turn; const fits = act({{ kind: 'fits' }}); \
                 return {{ turn: fits.turn || turn, wider: fits.wider, cut: fits.cut, x: scrollX, y: scrollY }}; }})()",
                act_function()
            );
            let context = self.world().await?;
            let value = self.evaluate_in(context, &expression, taken, false).await?;
            let turn = value["turn"].as_str().and_then(Turn::from_words);
            let wider = value["wider"].as_u64().unwrap_or(0).min(100_000) as u32;
            let cut: Vec<String> =
                value["cut"].as_array().into_iter().flatten().filter_map(Value::as_str).take(MAX_CUT).map(str::to_string).collect();
            let (picture, turn) = match turn {
                Some(turn) => (None, Some(turn)),
                None => self.picture(context, &value, Some(size), taken).await?,
            };
            looks.push(Look { size: *size, picture, wider, cut, turn });
            if turn.is_some() {
                break;
            }
        }
        Ok(looks)
    }

    async fn act_page(self: Arc<Self>, conversation: String, request: Request) -> Result<Page, String> {
        let _turn = self.take().await?;
        let (mine, taken, reported) = {
            let s = crate::lock(&self.state);
            // What the person did on another conversation's page is never this agent's to hear.
            (s.owner.as_deref() == Some(conversation.as_str()), s.taken, s.reported.clone())
        };
        let mut loads = self.loads.subscribe();
        let before = self.loads.borrow().0;
        let action = match &request {
            Request::Open(url) => {
                checked(url)?;
                // Under `acting`, so the person cannot take the pane between this look and the load: a page they took is never
                // closed or moved under their hands.
                let _acting = self.acting.lock().await;
                self.person_took_it(taken)?;
                self.fresh_for(&conversation).await;
                self.load(url)?;
                let mut s = crate::lock(&self.state);
                if !mine {
                    s.visited.clear();
                }
                s.owner = Some(conversation);
                s.reported = None;
                None
            }
            _ if self.webview().is_none() => return Err("no page is open; open one with its address first".into()),
            _ if crate::lock(&self.state).owner.as_deref() != Some(conversation.as_str()) => {
                return Err("the page open in Faber's browser is not this conversation's; open yours again with its address".into());
            }
            Request::Read => None,
            Request::Click(target) => Some(json!({ "kind": "click", "target": target, "at": bound(reported.as_deref())? })),
            Request::Type { target, text, submit } => {
                Some(json!({ "kind": "type", "target": target, "text": text, "submit": submit, "at": bound(reported.as_deref())? }))
            }
            Request::Scroll { down } => Some(json!({ "kind": "scroll", "down": down })),
            Request::Back => Some(json!({ "kind": "back" })),
            Request::Size(size) => {
                crate::lock(&self.state).size = *size;
                None
            }
            Request::Sizes(_) => None,
        };
        // The size the agent chose holds for every call, after the person had the pane and on a page just opened; a check at
        // several sizes that was cut off (an error, the engine's deadline) left another drawn, which goes here.
        let (size, emulated) = {
            let s = crate::lock(&self.state);
            (s.size, s.emulated)
        };
        if size.is_some() || emulated {
            self.touch();
            self.emulate(size, taken).await?;
        }
        let mut navigated = matches!(request, Request::Open(_));
        // The turn the action itself refused: the page is the same and stays the person's, whatever the next look says.
        let mut refused = None;
        if let Some(action) = action {
            self.touch();
            self.person_took_it(taken)?;
            let (done, turn) = self.run_act(action, taken).await?;
            refused = turn.filter(|_| !done);
            // An action that starts a new page waits for it; one that does not reads the same page again.
            if refused.is_none() {
                navigated = tokio::time::timeout(NAVIGATES, loads.wait_for(|(started, _)| *started > before)).await.is_ok();
            }
        }
        if navigated {
            // A page that never says it finished is read as it is once the time is up.
            let _ = tokio::time::timeout(LOAD, loads.wait_for(|(_, finished)| *finished > before)).await;
            // Pages that draw themselves after the load get a moment.
            tokio::time::sleep(Duration::from_millis(600)).await;
        } else if matches!(request, Request::Size(_)) {
            tokio::time::sleep(RESIZED).await;
        } else if !matches!(request, Request::Read | Request::Sizes(_)) && refused.is_none() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let looks = match &request {
            Request::Sizes(sizes) => {
                self.touch();
                self.looks(sizes, taken).await?
            }
            _ => Vec::new(),
        };
        let (mut page, href) = self.read(refused, taken).await?;
        page.looks = looks;
        let mut s = crate::lock(&self.state);
        // The person took the pane after the last look: this answer is old, and their use is the next one's to tell.
        if s.taken != taken {
            return Err("the person took over Faber's browser; ask them, or try again once they go back".into());
        }
        // Taken only by an answer that reaches the agent: a call that failed leaves them for the next.
        let used = std::mem::take(&mut s.handed_back);
        let visited = std::mem::take(&mut s.visited);
        if mine {
            (page.person, page.visited) = (used, visited);
        }
        s.reported = Some(href);
        Ok(page)
    }

    /// The page as it is: its address, title and text, and a picture of what is in view, none on a page that is the
    /// person's turn, where it could hold what they typed. `refused` is the turn an action just refused there. Answers, besides
    /// the page, its whole address, which stays in the shell.
    async fn read(&self, refused: Option<Turn>, taken: u64) -> Result<(Page, String), String> {
        // Cut inside the page, so a huge page never crosses into Faber whole; one more character tells the engine it was cut.
        // Its turn is looked at in the same step as its text, and what changes in the page from then on is counted.
        let expression = format!(
            "(() => {{ const turn = ({})({{ kind: 'read', watch: true }}).turn; const t = document.body ? document.body.innerText : ''; \
             return {{ turn, href: String(location.href), url: String(location.href).slice(0, {MAX_URL}), title: String(document.title).slice(0, {MAX_TITLE}), \
             text: String(t).slice(0, {}), x: scrollX, y: scrollY }}; }})()",
            act_function(),
            MAX_TEXT + 1
        );
        let context = self.world().await?;
        let value = self.evaluate_in(context, &expression, taken, false).await?;
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let mut turn = refused.or_else(|| Turn::from_words(&text("turn")));
        let size = crate::lock(&self.state).size;
        let mut picture = None;
        if turn.is_none() {
            (picture, turn) = self.picture(context, &value, size.as_ref(), taken).await?;
        }
        self.person_took_it(taken)?;
        let page = Page { url: text("url"), title: text("title"), text: text("text"), picture, turn, size, ..Page::default() };
        Ok((page, text("href")))
    }

    /// A picture of what is in view at `size` (none: the pane's own), from where `seen` says the page is scrolled to, after a
    /// look that started watching the page (`kind: 'read', watch: true`). A login or a payment that came up while it was
    /// taken is in it, and so is anything the page changed meanwhile (it cannot be told from here what it was): the picture
    /// is dropped, and the turn the page shows then is answered. A page that went to another document took the world with it.
    async fn picture(
        &self,
        context: i64,
        seen: &Value,
        size: Option<&Viewport>,
        taken: u64,
    ) -> Result<(Option<Vec<u8>>, Option<Turn>), String> {
        let at = |key: &str| seen[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0).min(1e7);
        let (width, height, scale) = match size {
            Some(size) => (f64::from(size.width), f64::from(size.height), look_scale(size)),
            None => (SIZE.width, SIZE.height, PICTURE_SCALE),
        };
        self.person_took_it(taken)?;
        let mut picture = self
            .call(
                "Page.captureScreenshot",
                json!({
                    "format": "jpeg",
                    "quality": 60,
                    "clip": { "x": at("x"), "y": at("y"), "width": width, "height": height, "scale": scale }
                }),
            )
            .await
            .ok()
            .and_then(|shot| shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()));
        let settle = format!("({})({{ kind: 'settle' }})", act_function());
        let mut turn = None;
        match self.evaluate_in(context, &settle, taken, false).await {
            Ok(after) => {
                turn = after["turn"].as_str().and_then(Turn::from_words);
                if turn.is_some() || after["changed"].as_bool() != Some(false) {
                    picture = None;
                }
            }
            Err(_) if self.person_took_it(taken).is_ok() => picture = None,
            Err(e) => return Err(e),
        }
        Ok((picture, turn))
    }
}

/// The most sites the agent hears the person went to: the latest of them.
const MAX_VISITED: usize = 5;
/// The sites a pane remembers to clear: more than a person or an agent visits in five minutes of one conversation.
const MAX_ORIGINS: usize = 64;

/// Keeps the site of a page the pane was on, once, the latest last.
fn stored_by(origins: &mut Vec<String>, url: &Url) {
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }
    let origin = url.origin().ascii_serialization();
    origins.retain(|o| *o != origin);
    origins.push(origin);
    if origins.len() > MAX_ORIGINS {
        origins.remove(0);
    }
}

/// Notes a site the person went to, for the agent and so for its vendor: only the site's own name (`docs.github.com` is
/// `github.com`, since a subdomain can be a token or a tenant), only for web pages with a plain name, never a local address
/// or an intranet name, once for pages in a row on the same site, the latest few kept. A page that finishes after the person
/// left the pane is not counted: the list can fall short, never say more.
fn went_to(visited: &mut Vec<String>, url: &Url) {
    let Some(host) = url.domain() else { return };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let plain = host.len() <= 253 && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    let labels: Vec<&str> = host.split('.').collect();
    if !matches!(url.scheme(), "http" | "https") || !plain || labels.len() < 2 || labels.iter().any(|l| l.is_empty() || l.len() > 63) {
        return;
    }
    let tld = labels[labels.len() - 1];
    // Names that only mean something inside a network (RFC 6762, 6761, and the usual intranet ones) are never told.
    if matches!(
        tld,
        "local" | "localhost" | "internal" | "intranet" | "corp" | "home" | "lan" | "localdomain" | "test" | "invalid" | "arpa" | "private"
    ) {
        return;
    }
    // The last two labels, always: under a country's second level that is only `co.uk`, which says little, but any rule that
    // keeps a third label lets a subdomain through somewhere (`token.org.de`).
    let site = labels[labels.len() - 2..].join(".");
    if visited.last() != Some(&site) {
        visited.push(site);
    }
    if visited.len() > MAX_VISITED {
        visited.remove(0);
    }
}

/// The engine's view of the pane.
pub struct PaneBrowser(pub Arc<Pane>);

/// A task that ends when the engine stops waiting for it, so a page the engine gave up on stops being read.
struct Abort(tauri::async_runtime::JoinHandle<Result<Page, String>>);

impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Browser for PaneBrowser {
    fn act(&self, conversation: String, request: Request) -> Pin<Box<dyn Future<Output = Result<Page, String>> + Send>> {
        let pane = self.0.clone();
        // Making the webview waits for the window's thread, so it runs off the engine's.
        Box::pin(async move {
            let mut task = Abort(tauri::async_runtime::spawn(pane.act_page(conversation, request)));
            (&mut task.0).await.map_err(|e| e.to_string())?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_hears_the_sites_the_person_went_to_by_name_once_each_in_a_row_and_only_the_latest() {
        let mut visited = Vec::new();
        for url in [
            "https://www.GitHub.com/a?code=secret",
            "https://docs.github.com./b",
            "https://docs.rs/",
            "about:blank",
            "http://localhost:3000/",
            "http://192.168.0.1/",
            "http://[::1]/",
            "http://intranet/",
            "https://a1b2c3d4e5.ngrok.io/",
            "https://shop.example.co.uk/",
            "https://ignore.previous!instructions.com/",
            "https://secret-token.box.com/",
            "https://alice.corp/",
            "https://printer.local/",
            "https://secret-token.org.de/",
        ] {
            went_to(&mut visited, &url.parse().unwrap());
        }
        assert_eq!(
            visited,
            ["docs.rs", "ngrok.io", "co.uk", "box.com", "org.de"],
            "site names only, never an address, a token or a local name; the latest five"
        );
        for n in 0..9 {
            went_to(&mut visited, &format!("https://s{n}.org/").parse().unwrap());
        }
        assert_eq!(visited.len(), MAX_VISITED);
        assert_eq!(visited.last().map(String::as_str), Some("s8.org"));
    }

    #[test]
    fn every_site_a_pane_was_on_is_kept_for_clearing_once_the_latest_last() {
        let mut origins = Vec::new();
        for url in ["https://bank.example/login", "http://localhost:3000/", "https://bank.example/home", "about:blank", "https://docs.rs/x"]
        {
            stored_by(&mut origins, &url.parse().unwrap());
        }
        assert_eq!(
            origins,
            ["http://localhost:3000", "https://bank.example", "https://docs.rs"],
            "each site once, no page that stores nothing"
        );
        for n in 0..MAX_ORIGINS {
            stored_by(&mut origins, &format!("https://s{n}.org/").parse().unwrap());
        }
        assert_eq!(origins.len(), MAX_ORIGINS);
        assert_eq!(origins.last().map(String::as_str), Some(format!("https://s{}.org", MAX_ORIGINS - 1).as_str()));
    }

    #[test]
    fn boxes_that_let_go_place_no_more_and_an_older_one_cannot_take_the_pane_from_a_newer() {
        let mut b = Boxes::default();
        assert!(b.park(7, 0), "a screen just loaded makes its load the one that counts");
        assert!(b.place(7, 1) && b.place(7, 2), "boxes that only grow place the pane");
        assert!(!b.place(7, 1), "a box older than the one that has the pane cannot move it");
        assert!(b.place(7, 2), "the box that has it can place it again");
        assert!(!b.park(7, 1), "box 1 letting go while box 2 has the pane leaves it there");
        assert!(!b.place(7, 1), "and box 1 places no more");
        assert!(b.park(7, 2) && !b.place(7, 2), "the box that has it takes it off the screen, once");
        assert!(b.place(7, 3));
        assert!(b.park(7, 0) && b.place(7, 1), "a screen just loaded starts its boxes again, whatever the clock said");
    }

    #[test]
    fn a_screen_that_went_away_cannot_place_or_park_the_pane_of_the_one_that_loaded_after() {
        let mut b = Boxes::default();
        assert!(!b.place(1, 1), "before any screen said it loaded, nothing places the pane");
        assert!(b.park(1, 0) && b.place(1, 5));
        assert!(b.park(2, 0), "the next load takes over");
        assert!(!b.place(1, 9) && !b.park(1, 9), "and the old one's late calls are ignored");
        assert!(b.place(2, 1) && !b.park(1, 1), "even a box number that would fit");
        assert!(b.park(2, 1));
        assert!(!b.park(1, 0) && b.place(2, 2), "a late start of the old screen does not take the pane back");
    }

    #[test]
    fn click_and_type_need_the_whole_address_of_a_page_the_agent_read() {
        assert!(bound(None).unwrap_err().starts_with("read the page first"));
        let long = format!("https://example.org/{}", "a".repeat(5_000));
        assert_eq!(bound(Some(&long)).unwrap(), long, "the address is kept whole, not cut like the one the agent gets");
    }

    #[test]
    fn the_person_taking_the_pane_waits_for_an_action_in_flight() {
        tauri::async_runtime::block_on(async {
            let acting = tokio::sync::Mutex::new(());
            let busy = acting.lock().await;
            let mut taking = std::pin::pin!(when_idle(&acting, || "placed"));
            assert!(
                tokio::time::timeout(Duration::from_millis(50), &mut taking).await.is_err(),
                "it waits while the page is being changed"
            );
            drop(busy);
            assert_eq!(taking.await, "placed", "and goes on once the action is over");
        });
    }

    #[test]
    fn a_page_at_a_screen_size_is_drawn_as_that_screen_and_pictured_at_one_size() {
        for size in faber_engine::screens::SIZES {
            let side = f64::from(size.width.max(size.height)) * size.scale * look_scale(&size);
            assert!((side - LOOK_SIDE).abs() < 0.01, "{} at {}x is pictured {side} px long", size.name(), size.scale);
            let (screen, touch) = metrics(&size);
            assert_eq!(
                (screen["width"].as_u64(), screen["deviceScaleFactor"].as_f64(), screen["mobile"].as_bool()),
                (Some(u64::from(size.width)), Some(size.scale), Some(size.mobile()))
            );
            assert_eq!(touch["enabled"].as_bool(), Some(size.mobile()), "touch on phones and tablets only");
        }
    }

    #[test]
    fn the_actions_are_one_function_the_page_can_call() {
        let f = act_function();
        assert!(f.starts_with("(request) => {") && f.ends_with('}'), "{f}");
    }

    #[test]
    fn only_web_pages_load_and_never_faber_s_own_origins() {
        let ok = |u: &str| allowed(&u.parse().unwrap());
        assert!(ok("https://docs.rs/tokio") && ok("http://localhost:3000/") && ok("http://127.0.0.1:8080/"));
        for bad in [
            "http://tauri.localhost/",
            "https://ipc.localhost/",
            "http://x.localhost./",
            "file:///C:/",
            "tauri://localhost/",
            "data:text/html,x",
        ] {
            assert!(!ok(bad), "{bad} must not load");
        }
        assert_eq!(ok("http://localhost:5173/"), !cfg!(debug_assertions), "the dev screen is Faber's own in a dev build");
    }
}
