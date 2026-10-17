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

mod native;

use base64::Engine as _;
use faber_engine::browser::{Browser, MAX_TEXT, MAX_TITLE, MAX_URL, Page, Request, Turn};
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

/// Whether the pane may load `url`: a web page, never one of Faber's own origins (nor, in a development build, the screen's
/// own dev server).
fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let dev_screen = cfg!(debug_assertions) && host == "localhost" && url.port() == Some(5173);
    matches!(url.scheme(), "http" | "https") && !host.is_empty() && !host.ends_with(".localhost") && !dev_screen
}

pub struct Pane {
    app: AppHandle,
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
        })
    }

    fn webview(&self) -> Option<Webview> {
        crate::lock(&self.state).webview.clone()
    }

    /// Loads `url` in the pane, making it first if it is closed. Answers the count of started pages to wait past.
    fn load(self: &Arc<Self>, url: &str) -> Result<u64, String> {
        let parsed: Url = url.parse().map_err(|_| "the address is not valid".to_string())?;
        if !allowed(&parsed) {
            return Err("only web pages open in Faber's browser".into());
        }
        let before = self.loads.borrow().0;
        self.touch();
        if let Some(webview) = self.webview() {
            if webview.navigate(parsed.clone()).is_ok() {
                return Ok(before);
            }
            // A pane left from a window that closed and came back: it is made again.
            self.close();
        }
        let window = self.app.get_window("main").ok_or("Faber's window is closed")?;
        let (loads, app) = (self.loads.clone(), self.app.clone());
        let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(parsed))
            .incognito(true)
            .on_navigation(allowed)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_page_load(move |_, payload| match payload.event() {
                PageLoadEvent::Started => loads.send_modify(|(started, _)| *started += 1),
                PageLoadEvent::Finished => {
                    loads.send_modify(|(started, finished)| *finished = *started);
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
        let _turn = self.opening.lock().await;
        let shown = self.load(url).is_ok();
        if shown {
            let mut s = crate::lock(&self.state);
            s.owner = Some(conversation.to_string());
            s.reported = None;
        }
        shown
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
    async fn run_act(&self, request: Value, taken: u64) -> Result<(bool, Option<Turn>), String> {
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

    async fn act_page(self: Arc<Self>, conversation: String, request: Request) -> Result<Page, String> {
        let _turn = self.take().await?;
        let (person, taken, reported) = {
            let mut s = crate::lock(&self.state);
            (std::mem::take(&mut s.handed_back), s.taken, s.reported.clone())
        };
        let mut loads = self.loads.subscribe();
        let before = self.loads.borrow().0;
        let action = match &request {
            Request::Open(url) => {
                self.load(url)?;
                let mut s = crate::lock(&self.state);
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
        };
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
        } else if !matches!(request, Request::Read) && refused.is_none() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let (page, href) = self.read(refused, person, taken).await?;
        crate::lock(&self.state).reported = Some(href);
        Ok(page)
    }

    /// The page as it is: its address, title and text, and a picture of what is in view, none on a page that is the
    /// person's turn, where it could hold what they typed. `refused` is the turn an action just refused there. Answers, besides
    /// the page, its whole address, which stays in the shell.
    async fn read(&self, refused: Option<Turn>, person: bool, taken: u64) -> Result<(Page, String), String> {
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
        let at = |key: &str| value[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0).min(1e7);
        let mut turn = refused.or_else(|| Turn::from_words(&text("turn")));
        let mut picture = None;
        if turn.is_none() {
            self.person_took_it(taken)?;
            picture = self
                .call(
                    "Page.captureScreenshot",
                    json!({
                        "format": "jpeg",
                        "quality": 60,
                        "clip": { "x": at("x"), "y": at("y"), "width": SIZE.width, "height": SIZE.height, "scale": PICTURE_SCALE }
                    }),
                )
                .await
                .ok()
                .and_then(|shot| shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()));
            // A login or a payment that came up while the picture was taken is in it, and so is anything the page changed
            // meanwhile (it cannot be told from here what it was): the picture is dropped. A page that went to another document
            // took the world with it.
            let settle = format!("({})({{ kind: 'settle' }})", act_function());
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
        }
        self.person_took_it(taken)?;
        let page = Page { url: text("url"), title: text("title"), text: text("text"), picture, turn, person };
        Ok((page, text("href")))
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
